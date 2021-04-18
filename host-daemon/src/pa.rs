use anyhow::Context;
use pulse::callbacks::ListResult;
use pulse::context;
use pulse::mainloop::standard as mainloop;
use std::cell;
use std::collections;
use std::rc::Rc;
use std::sync::mpsc;

/// Sample Spec for monitoring streams
const SAMPLE_SPEC: pulse::sample::Spec = pulse::sample::Spec {
    format: pulse::sample::Format::FLOAT32NE,
    channels: 1,
    rate: 25,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Sinks were added or removed (or default was changed) and we might need to reconnect the
    /// main channel.
    UpdateSinks,
    /// Sink inputs were added or removed and we need to recheck the 4 channels.
    UpdateSinkInputs,
    /// New Peak data is available on one of the channels.
    NewPeaks(common::Channel),
}

pub struct PulseInterface {
    mainloop: mainloop::Mainloop,
    pub context: context::Context,
    introspector: context::introspect::Introspector,
    event_rx: Option<mpsc::Receiver<Event>>,
    event_tx: mpsc::Sender<Event>,
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

        // Wait for context
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

        let (event_tx, event_rx) = mpsc::channel();

        // register subscription stuff
        context.set_subscribe_callback({
            let event_tx = event_tx.clone();
            Some(Box::new(move |facility, op, _idx| {
                use pulse::context::subscribe::Facility;
                use pulse::context::subscribe::Operation;

                let op = op.expect("invalid callback params");
                let facility = facility.expect("invalid callback params");

                match (facility, op) {
                    (Facility::Sink, Operation::New) => event_tx.send(Event::UpdateSinks),
                    (Facility::Sink, Operation::Removed) => event_tx.send(Event::UpdateSinks),
                    (Facility::Sink, Operation::Changed) => Ok(()), // ignore
                    (Facility::Server, _) => event_tx.send(Event::UpdateSinks),
                    (Facility::SinkInput, Operation::New) => event_tx.send(Event::UpdateSinkInputs),
                    (Facility::SinkInput, Operation::Removed) => {
                        event_tx.send(Event::UpdateSinkInputs)
                    }
                    (Facility::SinkInput, Operation::Changed) => Ok(()), // ignore
                    _ => unreachable!("unexpected facility: {:?}", facility),
                }
                .expect("channel failure");
            }))
        });

        {
            use pulse::context::subscribe::InterestMaskSet;
            context.subscribe(
                InterestMaskSet::SINK | InterestMaskSet::SINK_INPUT | InterestMaskSet::SERVER,
                |_| (),
            );
        }

        // send events for initial discovery
        event_tx.send(Event::UpdateSinks).expect("channel failure");
        event_tx
            .send(Event::UpdateSinkInputs)
            .expect("channel failure");

        Ok(PulseInterface {
            mainloop,
            context,
            introspector,
            event_rx: Some(event_rx),
            event_tx,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<Event>> {
        self.event_rx.take()
    }

    fn iterate_mainloop(mainloop: &mut mainloop::Mainloop, block: bool) -> anyhow::Result<()> {
        match mainloop.iterate(block) {
            mainloop::IterateResult::Success(_) => Ok(()),
            mainloop::IterateResult::Quit(_) => unreachable!("no code should quit the mainloop!"),
            mainloop::IterateResult::Err(e) => Err(e).context("failed mainloop iteration"),
        }
    }

    pub fn iterate(&mut self, block: bool) -> anyhow::Result<()> {
        Self::iterate_mainloop(&mut self.mainloop, block)
    }

    fn find_sink_input_by_props(
        &mut self,
        props: Rc<collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<Option<SinkInputInfo>> {
        let sink_input_info = Rc::new(cell::RefCell::new(None));
        let done = Rc::new(cell::Cell::new(Ok(false)));

        self.introspector.get_sink_input_info_list({
            let sink_input_info = sink_input_info.clone();
            let done = done.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    if sink_input_info.borrow().is_some() {
                        // already got one, ignore
                        return;
                    }

                    for (name, value) in props.iter() {
                        if info.proplist.get_str(name).as_ref() != Some(value) {
                            // this is not the sink-input we're looking for
                            return;
                        }
                    }

                    // all props matched if we're here!
                    sink_input_info.replace(Some(SinkInputInfo {
                        name: info.name.as_ref().map(|c| c.to_owned().into_owned()),
                        application: info
                            .proplist
                            .get_str(pulse::proplist::properties::APPLICATION_NAME),
                        index: info.index,
                        sink: info.sink,
                    }));
                }
                ListResult::Error => done.set(Err(())),
                ListResult::End => done.set(Ok(true)),
            }
        });

        loop {
            let _ = self.iterate(true)?;
            match done.get() {
                Ok(true) => break,
                Ok(false) => (),
                Err(_) => anyhow::bail!("failed querying sink-inputs"),
            }
        }

        Ok(sink_input_info.take())
    }

    pub fn find_default_sink(&mut self) -> anyhow::Result<Option<String>> {
        let default_sink = Rc::new(cell::RefCell::new(None));
        let done = Rc::new(cell::Cell::new(false));

        self.introspector.get_server_info({
            let default_sink = default_sink.clone();
            let done = done.clone();
            move |info| {
                default_sink.replace(
                    info.default_sink_name
                        .as_ref()
                        .map(|s| s.clone().into_owned()),
                );
                done.set(true);
            }
        });

        loop {
            self.iterate(true)?;
            if done.get() {
                break;
            }
        }

        Ok(default_sink.take())
    }

    pub fn get_monitor_for_sink(&mut self, sink: &str) -> anyhow::Result<(u32, u32)> {
        let sink_monitor = Rc::new(cell::RefCell::new(None));
        let done = Rc::new(cell::Cell::new(Ok(false)));

        self.introspector.get_sink_info_by_name(sink, {
            let sink_monitor = sink_monitor.clone();
            let done = done.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    sink_monitor.replace(Some((info.monitor_source, info.index)));
                }
                ListResult::End => done.set(Ok(true)),
                ListResult::Error => done.set(Err(())),
            }
        });

        loop {
            self.iterate(true)?;
            if done
                .get()
                .map_err(|_| anyhow::anyhow!("get_sink_info_by_name() list error"))?
            {
                break;
            }
        }

        Ok(sink_monitor.take().expect("no sink monitor source was set"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkInputInfo {
    pub name: Option<String>,
    pub application: Option<String>,
    pub index: u32,
    pub sink: u32,
}

pub struct Channel {
    stream: pulse::stream::Stream,
    ch: common::Channel,
    prop_matches: Option<Rc<collections::BTreeMap<String, String>>>,
    sink: Option<u32>,
    sink_input: Option<SinkInputInfo>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("ch", &self.ch)
            .field("sink", &self.sink)
            .field("sink_input", &self.sink_input)
            .finish()
    }
}

impl Channel {
    /// Create a new channel which is not yet connected
    pub fn new_for_sink(pa: &mut PulseInterface, ch: common::Channel) -> anyhow::Result<Self> {
        Self::new_for_sink_input(pa, ch, None)
    }

    /// Create a new channel which is not yet connected
    pub fn new_for_sink_input(
        pa: &mut PulseInterface,
        ch: common::Channel,
        prop_matches: Option<collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<Self> {
        let mut stream =
            pulse::stream::Stream::new(&mut pa.context, "Peak Detect", &SAMPLE_SPEC, None)
                .context("failed creating monitoring stream")?;

        stream.set_read_callback({
            let event_tx = pa.event_tx.clone();
            Some(Box::new(move |_length| {
                event_tx.send(Event::NewPeaks(ch)).expect("channel failure");
            }))
        });

        Ok(Self {
            stream,
            ch,
            prop_matches: prop_matches.map(|p| Rc::new(p)),
            sink: None,
            sink_input: None,
        })
    }

    pub fn is_for_sink(&self) -> bool {
        self.prop_matches.is_none()
    }

    /// Attempt to connect to a sink monitor or sink input
    pub fn try_connect(&mut self, pa: &mut PulseInterface) -> anyhow::Result<()> {
        if self.stream.get_state() != pulse::stream::State::Unconnected {
            let mut new_stream =
                pulse::stream::Stream::new(&mut pa.context, "Peak Detect", &SAMPLE_SPEC, None)
                    .context("failed creating monitoring stream")?;

            new_stream.set_read_callback({
                let event_tx = pa.event_tx.clone();
                let ch = self.ch;
                Some(Box::new(move |_length| {
                    event_tx.send(Event::NewPeaks(ch)).expect("channel failure");
                }))
            });

            let mut old_stream = std::mem::replace(&mut self.stream, new_stream);

            // ignore disconnection errors
            let _ = old_stream.disconnect();
        }

        let (monitor_source, sink_input) = if self.is_for_sink() {
            let sink_name = if let Some(s) = pa.find_default_sink()? {
                s
            } else {
                // no default sink found, not connecting then...
                return Ok(());
            };
            let (monitor_source, sink_index) = pa.get_monitor_for_sink(&sink_name)?;
            self.sink = Some(sink_index);
            (monitor_source, None)
        } else {
            let sink_input_info = match pa.find_sink_input_by_props(
                self.prop_matches
                    .clone()
                    .expect("no prop matches for sink input"),
            )? {
                Some(s) => {
                    log::info!(
                        "{:?}: \"{}\" from \"{}\"",
                        self.ch,
                        s.name.as_deref().unwrap_or("<no name>"),
                        s.application.as_deref().unwrap_or("<unknown app>")
                    );
                    s
                }
                // no sink input found for this channel, not connecting then...
                None => {
                    log::info!("{:?}: No sink-input found.", self.ch);
                    return Ok(());
                }
            };

            let (monitor_source, _) = pa.get_monitor_for_sink(&sink_input_info.sink.to_string())?;
            let sink_input_index = sink_input_info.index;
            self.sink_input = Some(sink_input_info);
            (monitor_source, Some(sink_input_index))
        };

        if let Some(sink_input) = sink_input {
            self.stream.set_monitor_stream(sink_input)?;
        }

        // TODO: do DONT_INHIBIT_AUTO_SUSPEND and DONT_MOVE properly
        let mut flags =
            pulse::stream::FlagSet::PEAK_DETECT | pulse::stream::FlagSet::ADJUST_LATENCY;

        if sink_input.is_some() {
            flags |= pulse::stream::FlagSet::DONT_MOVE;
        }

        let attrs = pulse::def::BufferAttr {
            fragsize: std::mem::size_of::<f32>() as u32,
            maxlength: u32::MAX,
            ..Default::default()
        };

        self.stream
            .connect_record(Some(&monitor_source.to_string()), Some(&attrs), flags)
            .context("failed connecting monitoring stream")?;

        // TODO: is it really necessary to block until the stream is ready?
        loop {
            pa.iterate(true)?;
            match self.stream.get_state() {
                pulse::stream::State::Ready => break,
                pulse::stream::State::Terminated => anyhow::bail!("terminated stream"),
                pulse::stream::State::Failed => anyhow::bail!("failed stream"),
                _ => (),
            }
        }

        Ok(())
    }

    pub fn get_recent_peak(&mut self) -> anyhow::Result<Option<f32>> {
        let mut recent_peak: Option<f32> = None;
        'peek_loop: loop {
            match self
                .stream
                .peek()
                .context("failed reading from monitoring stream")?
            {
                pulse::stream::PeekResult::Empty => break 'peek_loop,
                pulse::stream::PeekResult::Hole(_) => {
                    self.stream.discard().context("failed dropping fragments")?;
                }
                pulse::stream::PeekResult::Data(buf) => {
                    use std::convert::TryInto;
                    let buf: [u8; 4] = buf.try_into().context("got fragment of wrong length")?;
                    let rp = recent_peak.get_or_insert(0.0);
                    *rp = rp.max(f32::from_ne_bytes(buf));
                    self.stream.discard().context("failed dropping fragments")?;
                }
            }
        }
        Ok(recent_peak)
    }
}
