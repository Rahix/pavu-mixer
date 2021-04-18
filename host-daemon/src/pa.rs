use anyhow::Context;
use pulse::callbacks::ListResult;
use pulse::context;
use pulse::mainloop::standard as mainloop;
use std::borrow::Cow;
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
    /// Sinks were added or removed and we need to recheck the main channel.
    NewSinks,
    /// Sink inputs were added or removed and we need to recheck the 4 channels.
    NewSinkInputs,
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

                match op.expect("invalid callback params") {
                    Operation::New => (),
                    Operation::Removed => (),
                    // ignore "changed" notifications
                    Operation::Changed => return,
                }

                match facility.expect("invalid callback params") {
                    Facility::Sink => event_tx.send(Event::NewSinks),
                    Facility::SinkInput => event_tx.send(Event::NewSinkInputs),
                    f => unreachable!("got wrong facility: {:?}", f),
                }
                .expect("channel failure");
            }))
        });

        {
            use pulse::context::subscribe::InterestMaskSet;
            context.subscribe(InterestMaskSet::SINK | InterestMaskSet::SINK_INPUT, |_| ());
        }

        // send events for initial discovery
        event_tx.send(Event::NewSinks).expect("channel failure");
        event_tx
            .send(Event::NewSinkInputs)
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

    pub fn find_sink_input_by_props<'a>(
        &'a mut self,
        props: collections::BTreeMap<String, String>,
    ) -> anyhow::Result<Option<Channel>> {
        pub struct SinkInputInfo {
            pub name: Option<String>,
            pub application: Option<String>,
            pub index: u32,
            pub sink: u32,
        }

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

        let sink_input_info = if let Some(s) = sink_input_info.take() {
            s
        } else {
            log::debug!("No sink-input found.");
            return Ok(None);
        };

        log::debug!(
            "Found sink-input: \"{}\" from \"{}\"",
            sink_input_info.name.as_deref().unwrap_or("<no name>"),
            sink_input_info
                .application
                .as_deref()
                .unwrap_or("<unknown app>")
        );

        let sink_monitor_source = Rc::new(cell::RefCell::new(None));
        let done = Rc::new(cell::Cell::new(Ok(false)));

        self.introspector
            .get_sink_info_by_index(sink_input_info.sink, {
                let sink_monitor_source = sink_monitor_source.clone();
                let done = done.clone();
                move |result| match result {
                    ListResult::Item(info) => {
                        sink_monitor_source.replace(Some(info.monitor_source));
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
                Err(_) => anyhow::bail!("failed querying sink monitor-source"),
            }
        }

        todo!()

        // Ok(Some(Channel::new(
        //     self,
        //     Some(sink_monitor_source.take().expect("impossible")),
        //     Some(sink_input_info.index),
        // )?))
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

    pub fn get_monitor_for_sink(&mut self, sink: &str) -> anyhow::Result<u32> {
        let sink_monitor = Rc::new(cell::RefCell::new(None));
        let done = Rc::new(cell::Cell::new(Ok(false)));

        self.introspector.get_sink_info_by_name(sink, {
            let sink_monitor = sink_monitor.clone();
            let done = done.clone();
            move |result| match result {
                ListResult::Item(info) => {
                    sink_monitor.replace(Some(info.monitor_source));
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

pub struct Channel {
    stream: pulse::stream::Stream,
    ch: common::Channel,
    prop_matches: Option<collections::BTreeMap<String, String>>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Channel {{ {:?}, ... }}", self.ch)
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

        // will be written to by the callback
        let read_length = Rc::new(cell::Cell::new(0));

        stream.set_read_callback({
            let event_tx = pa.event_tx.clone();
            Some(Box::new(move |_length| {
                event_tx.send(Event::NewPeaks(ch)).expect("channel failure");
            }))
        });

        Ok(Self {
            stream,
            ch,
            prop_matches,
        })
    }

    pub fn is_for_sink(&self) -> bool {
        self.prop_matches.is_none()
    }

    pub fn is_connected(&self) -> bool {
        use pulse::stream::State;

        match self.stream.get_state() {
            State::Ready => true,
            _ => false,
        }
    }

    /// Attempt to connect to a sink monitor or sink input
    pub fn try_connect(&mut self, pa: &mut PulseInterface) -> anyhow::Result<()> {
        let (monitor_source, sink_input): (u32, Option<u32>) = if self.is_for_sink() {
            let sink_name = if let Some(s) = pa.find_default_sink()? {
                s
            } else {
                // no default sink found, not connecting then...
                return Ok(());
            };
            let monitor_source = pa.get_monitor_for_sink(&sink_name)?;
            (monitor_source, None)
        } else {
            todo!("sink input");
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

    pub fn foo(
        pa: &mut PulseInterface,
        sink_monitor: Option<u32>,
        sink_input: Option<u32>,
    ) -> anyhow::Result<Self> {
        let mut stream =
            pulse::stream::Stream::new(&mut pa.context, "Peak Detect", &SAMPLE_SPEC, None)
                .context("failed creating monitoring stream")?;

        if let Some(sink_input) = sink_input {
            stream.set_monitor_stream(sink_input)?;
        }

        // will be written to by the callback
        let read_length = Rc::new(cell::Cell::new(0));

        stream.set_read_callback({
            let read_length = read_length.clone();
            Some(Box::new(move |length| {
                read_length.set(length);
            }))
        });

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

        stream
            .connect_record(
                sink_monitor.map(|s| s.to_string()).as_deref(),
                Some(&attrs),
                flags,
            )
            .context("failed connecting monitoring stream")?;

        // TODO: is it really necessary to block until the stream is ready?
        loop {
            pa.iterate(true)?;
            match stream.get_state() {
                pulse::stream::State::Ready => break,
                pulse::stream::State::Terminated => anyhow::bail!("terminated stream"),
                pulse::stream::State::Failed => anyhow::bail!("failed stream"),
                _ => (),
            }
        }

        todo!();

        // Ok(Self {
        //     stream,
        //     read_length,
        // })
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
