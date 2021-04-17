use anyhow::Context;

mod config;
mod connection;
mod pa;

fn main() {
    let mut pa = pa::PulseInterface::init().context("failed pulseaudio init").unwrap();

    match inner(&mut pa) {
        Ok(_) => eprintln!("Success!"),
        Err(e) => eprintln!("{}", e),
    }
}

fn inner(pa: &mut pa::PulseInterface) -> anyhow::Result<()> {
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

    let channel_main = pa.attach_main_channel()?;

    let channel_1 = pa.find_sink_input_by_props(config.channel_1.property_matches)?;
    let channel_2 = pa.find_sink_input_by_props(config.channel_2.property_matches)?;
    let channel_3 = pa.find_sink_input_by_props(config.channel_3.property_matches)?;
    let channel_4 = pa.find_sink_input_by_props(config.channel_4.property_matches)?;

    let mut channels = [
        (common::Channel::Main, Some(channel_main)),
        (common::Channel::Ch1, channel_1),
        (common::Channel::Ch2, channel_2),
        (common::Channel::Ch3, channel_3),
        (common::Channel::Ch4, channel_4),
    ];

    let mut last_main_volume = u32::MAX;
    loop {
        pa.iterate(true)?;

        for (id, channel) in channels.iter_mut() {
            if let Some(channel) = channel {
                if let Some(peak) = channel.get_recent_peak()? {
                    let msg = common::HostMessage::UpdatePeak(*id, peak);
                    pavu_mixer
                        .send(dbg!(msg))
                        .with_context(|| format!("failed updating channel peak for {:?}", id))?;
                }
            }
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
