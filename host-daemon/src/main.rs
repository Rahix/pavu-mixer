use pulse::mainloop::standard as mainloop;
use std::sync::atomic;

fn main() {
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
        .connect_record(Some("0"), Some(&stream_attrs), stream_flags)
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

    loop {
        match mainloop.iterate(false) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }

        let length = read_length.load(atomic::Ordering::SeqCst);
        if length > 0 {
            match stream.peek().unwrap() {
                pulse::stream::PeekResult::Empty => continue,
                pulse::stream::PeekResult::Hole(_) => stream.discard().unwrap(),
                pulse::stream::PeekResult::Data(d) => {
                    use std::convert::TryInto;
                    let buf: [u8; 4] = d[(d.len() - std::mem::size_of::<f32>())..]
                        .try_into()
                        .unwrap();
                    let v = f32::from_ne_bytes(buf);
                    stream.discard().unwrap();

                    for _ in 0..((v * 100.5) as usize) {
                        print!("#");
                    }
                    println!("");
                }
            }
        }
    }
}
