use pulse::mainloop::standard as mainloop;
use std::sync::atomic;

mod config;
mod connection;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config: config::Config = confy::load("pavu-mixer")?;
    let mut pavu_mixer = connection::PavuMixer::connect(&config.connection)?;

    let ss = pulse::sample::Spec {
        format: pulse::sample::Format::FLOAT32NE,
        channels: 1,
        rate: 25,
    };

    let mut proplist = pulse::proplist::Proplist::new().unwrap();
    proplist
        .set_str(
            pulse::proplist::properties::APPLICATION_NAME,
            "Pavu-Mixer Daemon",
        )
        .unwrap();

    let mut mainloop = mainloop::Mainloop::new().unwrap();

    let mut context =
        pulse::context::Context::new_with_proplist(&mainloop, "PavuMixerContext", &proplist)
            .unwrap();
    context
        .connect(None, pulse::context::FlagSet::NOFLAGS, None)
        .unwrap();

    // Wait for context
    'wait_for_ctx: loop {
        match mainloop.iterate(false) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }
        match context.get_state() {
            pulse::context::State::Ready => {
                break 'wait_for_ctx;
            }
            pulse::context::State::Terminated | pulse::context::State::Failed => {
                panic!("Broken context");
            }
            _ => (),
        }
    }

    let channel_volumes = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut introspector = context.introspect();

    let op = introspector.get_sink_info_by_index(1, {
        let channel_volumes = channel_volumes.clone();
        move |res| match res {
            pulse::callbacks::ListResult::Item(i) => {
                *channel_volumes.borrow_mut() = Some(i.volume.clone());
            }
            _ => (),
        }
    });

    // Wait for channel info
    'wait_for_info: loop {
        match mainloop.iterate(true) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }
        match op.get_state() {
            pulse::operation::State::Done => {
                break 'wait_for_info;
            }
            pulse::operation::State::Cancelled => {
                panic!("Broken info cb");
            }
            _ => (),
        }
    }

    // get the volumes out of the refcell
    let mut channel_volumes = channel_volumes.borrow_mut().take().unwrap();

    let mut stream = pulse::stream::Stream::new(&mut context, "Peak Detect", &ss, None).unwrap();

    // Select which sink-input to monitor
    // stream.set_monitor_stream(0).unwrap();

    // from pavucontrol:src/mainwindow.cc:666
    let stream_flags = pulse::stream::FlagSet::DONT_MOVE
        | pulse::stream::FlagSet::PEAK_DETECT
        | pulse::stream::FlagSet::ADJUST_LATENCY;

    let stream_attrs = pulse::def::BufferAttr {
        fragsize: std::mem::size_of::<f32>() as u32,
        maxlength: u32::MAX,
        ..Default::default()
    };

    let read_length = std::sync::Arc::new(atomic::AtomicUsize::new(0));

    stream.set_read_callback({
        let read_length = read_length.clone();
        Some(Box::new(move |length| {
            read_length.store(length, atomic::Ordering::SeqCst);
        }))
    });

    stream
        .connect_record(Some("2"), Some(&stream_attrs), stream_flags)
        .unwrap();

    // Wait for stream
    'wait_for_stream: loop {
        match mainloop.iterate(false) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }
        match stream.get_state() {
            pulse::stream::State::Ready => {
                break 'wait_for_stream;
            }
            pulse::stream::State::Terminated | pulse::stream::State::Failed => {
                panic!("Broken stream");
            }
            _ => (),
        }
    }

    let mut last_main_volume = u32::MAX;
    loop {
        match mainloop.iterate(true) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }

        let length = read_length.load(atomic::Ordering::SeqCst);
        if length > 0 {
            'peekloop: loop {
                match stream.peek().unwrap() {
                    pulse::stream::PeekResult::Empty => break 'peekloop,
                    pulse::stream::PeekResult::Hole(_) => stream.discard().unwrap(),
                    pulse::stream::PeekResult::Data(d) => {
                        use std::convert::TryInto;
                        let buf: [u8; 4] = d[(d.len() - std::mem::size_of::<f32>())..]
                            .try_into()
                            .unwrap();
                        let v = f32::from_ne_bytes(buf);

                        stream.discard().unwrap();

                        pavu_mixer
                            .send(common::HostMessage::UpdatePeak(common::Channel::Main, v))
                            .unwrap();
                    }
                }
            }
            read_length.store(0, atomic::Ordering::SeqCst);
        }

        // read main volume from usb
        if let Some(msg) = pavu_mixer.try_recv().unwrap() {
            match msg {
                common::DeviceMessage::UpdateVolume(common::Channel::Main, vol) => {
                    let vol = vol.clamp(0.0, 1.0);
                    let volume: u32 = (vol * 100.5) as u32;
                    if volume != last_main_volume {
                        println!("New Main Volume: {}", volume);

                        let pa_volume = pulse::volume::Volume(
                            (pulse::volume::Volume::NORMAL.0 as f32 * vol) as u32,
                        );
                        channel_volumes.set(channel_volumes.len(), pa_volume);

                        introspector.set_sink_volume_by_index(1, &channel_volumes, None);

                        last_main_volume = volume;
                    }
                }
                m => println!("Ignored message: {:?}", m),
            }
        }
    }
}
