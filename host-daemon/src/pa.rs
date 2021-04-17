use anyhow::Context;
use pulse::callbacks::ListResult;
use pulse::context;
use pulse::mainloop::standard as mainloop;
use std::cell;
use std::collections;
use std::rc::Rc;

/// Sample Spec for monitoring streams
const SAMPLE_SPEC: pulse::sample::Spec = pulse::sample::Spec {
    format: pulse::sample::Format::FLOAT32NE,
    channels: 1,
    rate: 25,
};

pub struct PulseInterface {
    mainloop: mainloop::Mainloop,
    pub context: context::Context,
    introspector: context::introspect::Introspector,
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

        Ok(PulseInterface {
            mainloop,
            context,
            introspector,
        })
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
            self.iterate(true)?;
            match done.get() {
                Ok(true) => break,
                Ok(false) => (),
                Err(_) => anyhow::bail!("failed querying sink-inputs"),
            }
        }

        if let Some(sink_input_info) = sink_input_info.take() {
            log::debug!(
                "Found sink-input: \"{}\" from \"{}\"",
                sink_input_info.name.as_deref().unwrap_or("<no name>"),
                sink_input_info
                    .application
                    .as_deref()
                    .unwrap_or("<unknown app>")
            );
            Ok(Some(Channel::new(
                self,
                Some(sink_input_info.sink),
                Some(sink_input_info.index),
            )?))
        } else {
            log::debug!("No sink-input found.");
            Ok(None)
        }
    }

    pub fn attach_main_channel(&mut self) -> anyhow::Result<Channel> {
        Channel::new(self, None, None)
    }
}

pub struct Channel {
    stream: pulse::stream::Stream,
    read_length: Rc<cell::Cell<usize>>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Channel {{ ... }}")
    }
}

impl Channel {
    pub fn new(
        pa: &mut PulseInterface,
        mut sink: Option<u32>,
        sink_input: Option<u32>,
    ) -> anyhow::Result<Self> {
        let mut stream =
            pulse::stream::Stream::new(&mut pa.context, "Peak Detect", &SAMPLE_SPEC, None)
                .context("failed creating monitoring stream")?;

        if let Some(sink_input) = sink_input {
            // TODO: This is a hack and needs to be done properly
            sink = Some(2);
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
            .connect_record(sink.map(|s| s.to_string()).as_deref(), Some(&attrs), flags)
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

        Ok(Self {
            stream,
            read_length,
        })
    }

    pub fn get_recent_peak(&mut self) -> anyhow::Result<Option<f32>> {
        if self.read_length.get() <= 0 {
            return Ok(None);
        }

        let mut recent_peak: f32 = 0.0;
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
                    recent_peak = recent_peak.max(f32::from_ne_bytes(buf));
                    self.stream.discard().context("failed dropping fragments")?;
                }
            }
        }
        self.read_length.set(0);

        Ok(Some(recent_peak))
    }
}
