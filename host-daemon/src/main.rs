use anyhow::Context;

mod config;
mod connection;
mod pa;

fn main() {
    let mut pa = pa::PulseInterface::init()
        .context("failed pulseaudio init")
        .unwrap();

    match inner(&mut pa) {
        Ok(_) => eprintln!("Success!"),
        Err(e) => eprintln!("{:?}", e),
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

    // let channel_volumes = std::rc::Rc::new(std::cell::RefCell::new(None));
    // let mut introspector = pa.context.introspect();

    // let op = introspector.get_sink_info_by_index(1, {
    //     let channel_volumes = channel_volumes.clone();
    //     move |res| match res {
    //         pulse::callbacks::ListResult::Item(i) => {
    //             *channel_volumes.borrow_mut() = Some(i.volume.clone());
    //         }
    //         _ => (),
    //     }
    // });

    // // Wait for channel info
    // 'wait_for_info: loop {
    //     let _ = pa.iterate(true)?;
    //     match op.get_state() {
    //         pulse::operation::State::Done => {
    //             break 'wait_for_info;
    //         }
    //         pulse::operation::State::Cancelled => {
    //             panic!("info request was cancelled");
    //         }
    //         _ => (),
    //     }
    // }

    // // get the volumes out of the refcell
    // let mut channel_volumes = channel_volumes
    //     .borrow_mut()
    //     .take()
    //     .expect("callback done but no channel_volumes set");

    let mut ch_main = pa::Channel::new_for_sink(pa, common::Channel::Main)?;
    let mut ch1 = pa::Channel::new_for_sink_input(
        pa,
        common::Channel::Ch1,
        Some(config.channel_1.property_matches),
    )?;
    let mut ch2 = pa::Channel::new_for_sink_input(
        pa,
        common::Channel::Ch2,
        Some(config.channel_2.property_matches),
    )?;
    let mut ch3 = pa::Channel::new_for_sink_input(
        pa,
        common::Channel::Ch3,
        Some(config.channel_3.property_matches),
    )?;
    let mut ch4 = pa::Channel::new_for_sink_input(
        pa,
        common::Channel::Ch4,
        Some(config.channel_4.property_matches),
    )?;

    let event_rx = pa.take_event_receiver().expect("event_rx missing");

    // let mut last_main_volume = u32::MAX;
    loop {
        pa.iterate(true)?;
        for event in event_rx.try_iter() {
            match event {
                pa::Event::NewPeaks(ch_id) => {
                    let ch = match ch_id {
                        common::Channel::Main => &mut ch_main,
                        common::Channel::Ch1 => &mut ch1,
                        common::Channel::Ch2 => &mut ch2,
                        common::Channel::Ch3 => &mut ch3,
                        common::Channel::Ch4 => &mut ch4,
                    };
                    if let Some(peak) = ch.get_recent_peak()? {
                        let msg = common::HostMessage::UpdatePeak(ch_id, peak);
                        pavu_mixer.send(msg).with_context(|| {
                            format!("failed updating channel peak for {:?}", ch_id)
                        })?;
                    }
                }
                pa::Event::UpdateSinks => ch_main.try_connect(pa)?,
                pa::Event::UpdateSinkInputs => {
                    ch1.try_connect(pa)?;
                    ch2.try_connect(pa)?;
                    ch3.try_connect(pa)?;
                    ch4.try_connect(pa)?;
                }
            }
        }

        // for (id, channel) in channels.iter_mut() {
        //     if let Some(channel) = channel {
        //         if let Some(peak) = channel.get_recent_peak()? {
        //             let msg = common::HostMessage::UpdatePeak(*id, peak);
        //             pavu_mixer
        //                 .send(msg)
        //                 .with_context(|| format!("failed updating channel peak for {:?}", id))?;
        //         }
        //     }
        // }

        // read main volume from usb
        // if let Some(msg) = pavu_mixer.try_recv()? {
        //     match msg {
        //         common::DeviceMessage::UpdateVolume(common::Channel::Main, vol) => {
        //             let vol = vol.clamp(0.0, 1.0);
        //             let volume: u32 = (vol * 100.5) as u32;
        //             if volume != last_main_volume {
        //                 println!("New Main Volume: {}", volume);

        //                 let pa_volume = pulse::volume::Volume(
        //                     (pulse::volume::Volume::NORMAL.0 as f32 * vol) as u32,
        //                 );
        //                 channel_volumes.set(channel_volumes.len(), pa_volume);

        //                 introspector.set_sink_volume_by_index(1, &channel_volumes, None);

        //                 last_main_volume = volume;
        //             }
        //         }
        //         m => println!("Ignored message: {:?}", m),
        //     }
        // }
    }
}
