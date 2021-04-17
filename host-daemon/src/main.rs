use anyhow::Context;

mod config;
mod connection;
mod pa;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter(
            Some("pavu_mixer_host"),
            if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            },
        )
        .init();

    let config: config::Config = confy::load("pavu-mixer")?;
    let mut pavu_mixer = connection::PavuMixer::connect(&config.connection)?;

    let mut pa = pa::PulseInterface::init().context("failed pulseaudio init")?;

    dbg!(pa.find_sink_input_by_props(config.channel_1.property_matches)?);
    dbg!(pa.find_sink_input_by_props(config.channel_2.property_matches)?);
    dbg!(pa.find_sink_input_by_props(config.channel_3.property_matches)?);
    dbg!(pa.find_sink_input_by_props(config.channel_4.property_matches)?);

    let channel_volumes = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut introspector = pa.context.introspect();

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
        pa.iterate(true)?;
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

    let (mut stream, read_length) = pa.create_monitor_stream(Some(2), None)?;

    let mut last_main_volume = u32::MAX;
    loop {
        pa.iterate(true)?;

        let length = read_length.get();
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
            read_length.set(0);
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
