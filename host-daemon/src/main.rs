use anyhow::Context;
use std::sync::atomic;

mod config;
mod connection;
mod pa;

/// Sample Spec for monitoring streams
const SAMPLE_SPEC: pulse::sample::Spec = pulse::sample::Spec {
    format: pulse::sample::Format::FLOAT32NE,
    channels: 1,
    rate: 25,
};

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config: config::Config = confy::load("pavu-mixer")?;
    let mut pavu_mixer = connection::PavuMixer::connect(&config.connection)?;

    let (mut mainloop, mut context) = pa::init().context("failed pulseaudio init")?;

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
        pa::iterate(&mut mainloop, true)?;
        match op.get_state() {
            pulse::operation::State::Done => {
                break 'wait_for_info;
            }
            pulse::operation::State::Cancelled => {
                panic!("info request was cancelled");
            }
            _ => (),
        }
    }

    // get the volumes out of the refcell
    let mut channel_volumes = channel_volumes
        .borrow_mut()
        .take()
        .expect("callback done but no channel_volumes set");

    let mut stream = pulse::stream::Stream::new(&mut context, "Peak Detect", &SAMPLE_SPEC, None)
        .context("failed creating monitoring stream")?;

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
        .context("failed connecting monitoring stream")?;

    // Wait for stream
    'wait_for_stream: loop {
        pa::iterate(&mut mainloop, true)?;
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
        pa::iterate(&mut mainloop, true)?;

        let length = read_length.load(atomic::Ordering::SeqCst);
        if length > 0 {
            'peekloop: loop {
                match stream
                    .peek()
                    .context("failed reading from monitoring stream")?
                {
                    pulse::stream::PeekResult::Empty => break 'peekloop,
                    pulse::stream::PeekResult::Hole(_) => {
                        stream.discard().context("failed dropping fragments")?
                    }
                    pulse::stream::PeekResult::Data(d) => {
                        use std::convert::TryInto;
                        let buf: [u8; 4] = d[(d.len() - std::mem::size_of::<f32>())..]
                            .try_into()
                            .expect("impossible");
                        let v = f32::from_ne_bytes(buf);

                        stream.discard().context("failed dropping fragments")?;

                        pavu_mixer
                            .send(common::HostMessage::UpdatePeak(common::Channel::Main, v))
                            .context("failed updating main channel peak")?;
                    }
                }
            }
            read_length.store(0, atomic::Ordering::SeqCst);
        }

        // read main volume from usb
        if let Some(msg) = pavu_mixer.try_recv()? {
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
