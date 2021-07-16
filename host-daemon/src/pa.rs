use anyhow::Context;
use pulse::callbacks::ListResult;
use pulse::context;
use pulse::mainloop::standard as mainloop;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;

/// Sample Spec for monitoring streams
const SAMPLE_SPEC: pulse::sample::Spec = pulse::sample::Spec {
    format: pulse::sample::Format::FLOAT32NE,
    channels: 1,
    rate: 25,
};

pub struct SinkInputInfo {
    index: u32,
    name: Option<String>,
    application: Option<String>,
    connected_to_sink: u32,
    properties: pulse::proplist::Proplist,
}

impl SinkInputInfo {
    fn from_pa(info: &context::introspect::SinkInputInfo) -> Self {
        Self {
            index: info.index,
            name: info.name.as_ref().map(|c| c.to_owned().into_owned()),
            application: info
                .proplist
                .get_str(pulse::proplist::properties::APPLICATION_NAME),
            connected_to_sink: info.sink,
            properties: info.proplist.clone(),
        }
    }
}

impl std::fmt::Debug for SinkInputInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut propmap = std::collections::BTreeMap::new();
        for key in self.properties.iter() {
            let value = self
                .properties
                .get_str(&key)
                .expect("missing property for iterated key");
            propmap.insert(key, value);
        }
        f.debug_struct("SinkInputInfo")
            .field("index", &self.index)
            .field("name", &self.name)
            .field("application", &self.application)
            .field("connected_to_sink", &self.connected_to_sink)
            .field("properties", &propmap)
            .finish()
    }
}

#[derive(Debug)]
pub enum Event {
    /// After querying the default sink, PulseAudio came back with this stream.
    NewDefaultSink(Stream),
    /// A new sink-input showed up and we need to check whether it matches any of our channels - if
    /// yes, it should be attached.
    SinkInputAdded(SinkInputInfo),
    /// A sink-input was removed and we should probably drop it from a potentially connected
    /// channel as well.
    SinkInputRemoved(u32),
    /// New signal peak information is available for this stream (sink / sink-input).
    NewPeakData(u32),
    // /// An error occurred asynchronously and we need to abort.
    // FatalError(anyhow::Error),
}

#[derive(Debug)]
enum InternalEvent {
    /// Sinks were added or removed (or default was changed) and we might need to reconnect the
    /// main channel.  This will trigger us to query the default sink next, leading to an external
    /// `DefaultSink` event.
    SinkUpdateNeeded,
    /// PulseAudio reported back the name of the default sink - we can now go and query its
    /// information.
    DefaultSinkName(String),
    /// We got information about the default sink - enough to create a stream for it.
    SinkData,
    /// A new sink-input was detected - we should query its information and tell the application
    /// about it.
    SinkInputPending(u32),
}

/// Interface for interacting with Pulseaudio.
///
/// This interface will provide information about connect{ed,ing} streams, stream peak samples, and
/// allow the mixer to control stream volumes.
///
/// It is built upon the Pulseaudio single-threaded "simple" mainloop.
pub struct PulseInterface {
    mainloop: mainloop::Mainloop,
    pub context: context::Context,
    introspector: context::introspect::Introspector,
    external_rx: Option<mpsc::Receiver<Event>>,
    external_tx: mpsc::Sender<Event>,
    internal_rx: mpsc::Receiver<InternalEvent>,
    internal_tx: mpsc::Sender<InternalEvent>,

    /// Name of the current default sink (used to check if it changed).
    current_default_sink: Option<String>,
}

impl PulseInterface {
    pub fn init() -> anyhow::Result<Self> {
        let mut proplist = pulse::proplist::Proplist::new().context("failed creating proplist")?;
        proplist
            .set_str(
                pulse::proplist::properties::APPLICATION_NAME,
                "Pavu-Mixer Daemon",
            )
            .ok()
            .context("failed setting proplist string")?;

        let mut mainloop = mainloop::Mainloop::new().context("failed creating mainloop")?;
        let mut context =
            pulse::context::Context::new_with_proplist(&mainloop, "PavuMixerContext", &proplist)
                .context("failed creating context")?;
        context
            .connect(None, pulse::context::FlagSet::NOFLAGS, None)
            .context("failed connecting context")?;

        'wait_for_ctx: loop {
            Self::iterate_mainloop(&mut mainloop, true)?;
            match context.get_state() {
                pulse::context::State::Ready => {
                    break 'wait_for_ctx;
                }
                pulse::context::State::Terminated | pulse::context::State::Failed => {
                    anyhow::bail!("terminated or failed context");
                }
                _ => (),
            }
        }

        let introspector = context.introspect();

        let (external_tx, external_rx) = mpsc::channel();
        let (internal_tx, internal_rx) = mpsc::channel();

        context.set_subscribe_callback({
            let external_tx = external_tx.clone();
            let internal_tx = internal_tx.clone();
            Some(Box::new(move |facility, op, index| {
                use pulse::context::subscribe::Facility;
                use pulse::context::subscribe::Operation;

                // it would be very odd if PulseAudio gave us invalid enum values
                let op = op.expect("invalid subscribe callback params");
                let facility = facility.expect("invalid subscribe callback params");

                match (facility, op) {
                    (Facility::Sink, Operation::New) => internal_tx
                        .send(InternalEvent::SinkUpdateNeeded)
                        .expect("event channel error"),
                    (Facility::Sink, Operation::Removed) => internal_tx
                        .send(InternalEvent::SinkUpdateNeeded)
                        .expect("event channel error"),
                    (Facility::Sink, Operation::Changed) => (), // ignore
                    (Facility::Server, _) => internal_tx
                        .send(InternalEvent::SinkUpdateNeeded) // default sink might have changed
                        .expect("event channel error"),
                    (Facility::SinkInput, Operation::New) => internal_tx
                        .send(InternalEvent::SinkInputPending(index))
                        .expect("event channel error"),
                    (Facility::SinkInput, Operation::Removed) => external_tx
                        .send(Event::SinkInputRemoved(index))
                        .expect("event channel error"),
                    (Facility::SinkInput, Operation::Changed) => (), // ignore
                    _ => unreachable!("unexpected facility: {:?}", facility),
                };
            }))
        });

        // subscribe to interesting events:
        // - SINK: if the available sinks (= output devices) change
        // - SINK_INPUT: if the playing audio sources change
        // - SERVER: if the selected default sink (output device) changes
        {
            use pulse::context::subscribe::InterestMaskSet;
            context.subscribe(
                InterestMaskSet::SINK | InterestMaskSet::SINK_INPUT | InterestMaskSet::SERVER,
                |_| (),
            );
        }

        // Queue initial events to get the mixer going.  This means triggering an update of the
        // default sink...
        internal_tx
            .send(InternalEvent::SinkUpdateNeeded)
            .expect("event channel error");

        // ...and "adding" all currently existing sink-inputs.
        let done = Rc::new(Cell::new(Ok(false)));
        introspector.get_sink_input_info_list({
            let external_tx = external_tx.clone();
            let done = done.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    if let Err(e) =
                        external_tx.send(Event::SinkInputAdded(SinkInputInfo::from_pa(info)))
                    {
                        done.set(Err(e.into()));
                    }
                }
                ListResult::Error => done.set(Err(anyhow::anyhow!("pulseaudio list error"))),
                ListResult::End => done.set(Ok(true)),
            }
        });

        let mut this = Self {
            mainloop,
            context,
            introspector,
            external_rx: Some(external_rx),
            external_tx,
            internal_rx,
            internal_tx,

            current_default_sink: None,
        };

        'add_all_sink_inputs: loop {
            this.iterate(true)?;
            // returns `true` once we hit the end of the sink-input list
            if done.replace(Ok(false))? {
                break 'add_all_sink_inputs;
            }
        }

        Ok(this)
    }

    fn iterate_mainloop(mainloop: &mut mainloop::Mainloop, block: bool) -> anyhow::Result<()> {
        match mainloop.iterate(block) {
            mainloop::IterateResult::Success(_) => Ok(()),
            mainloop::IterateResult::Quit(_) => unreachable!("no code should quit the mainloop!"),
            mainloop::IterateResult::Err(e) => Err(e).context("failed mainloop iteration"),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<Event>> {
        self.external_rx.take()
    }

    pub fn iterate(&mut self, block: bool) -> anyhow::Result<()> {
        Self::iterate_mainloop(&mut self.mainloop, block)?;
        while let Ok(event) = self.internal_rx.try_recv() {
            match event {
                InternalEvent::SinkUpdateNeeded => self.query_default_sink(),
                InternalEvent::DefaultSinkName(name) => if self.current_default_sink.as_ref() != Some(&name) {
                    // the name differs from the previous one - we need to issue an update
                    self.query_sink_data(&name);
                }
                InternalEvent::SinkInputPending(index) => self.query_added_sink_input(index),
            }
        }
        Ok(())
    }

    /// Query the default sink.
    ///
    /// Triggers [`InternalEvent::DefaultSinkName`] on completion.
    fn query_default_sink(&mut self) {
        self.introspector.get_server_info({
            let internal_tx = self.internal_tx.clone();
            move |info| {
                if let Some(default_sink) = &info.default_sink_name {
                    internal_tx.send(InternalEvent::DefaultSinkName(default_sink.clone().into_owned())).expect("event channel error");
                } else {
                    log::warn!("PulseAudio does not have a default sink - the main channel is not operational.");
                }
            }
        });
    }

    /// Query sink information for a sink.
    ///
    /// Triggers [`InternalEvent::SinkData`] on completion.
    fn query_sink_data(&mut self, sink_name: &str) {
        self.introspector.get_sink_info_by_name(sink_name, {
            let internal_tx = self.internal_tx.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    dbg!(
                        info.index,
                        info.monitor_source,
                        info.volume.avg().print(),
                        info.mute,
                    );
                }
                ListResult::End => (),
                ListResult::Error => log::warn!("error while querying sink data - ignoring this sink"),
            }
        });
    }

    /// Query a newly added sink-input.
    ///
    /// Triggers [`Event::SinkInputAdded`] on completion.
    fn query_added_sink_input(&mut self, index: u32) {
        self.introspector.get_sink_input_info(index, {
            let external_tx = self.external_tx.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    external_tx
                        .send(Event::SinkInputAdded(SinkInputInfo::from_pa(info)))
                        .expect("event channel error");
                }
                ListResult::Error => {
                    log::warn!("Error while querying sink-input {} - ignoring.", index)
                }
                ListResult::End => (),
            }
        });
    }
}

impl Drop for PulseInterface {
    fn drop(&mut self) {
        // SAFETY: Not doing this causes a segfault /o\
        self.context.disconnect();
    }
}

#[derive(Debug)]
pub enum StreamInfo {
    Sink { index: u32, name: String },
    SinkInput(SinkInputInfo),
}

pub struct Stream {
    stream: pulse::stream::Stream,
    info: StreamInfo,
    volume: Option<pulse::volume::ChannelVolumes>,
    muted: bool,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("info", &self.info)
            .field("volume", &self.volume.as_ref().map(|v| v.avg().print()))
            .field("muted", &self.muted)
            .finish()
    }
}

impl Stream {
    pub fn new(pa: &mut PulseInterface) -> anyhow::Result<()> {
        todo!()
    }
}
