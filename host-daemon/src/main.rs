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
                    if let Some(peak) = ch.get_recent_peak(pa)? {
                        let msg = common::HostMessage::UpdatePeak(ch_id, peak);
                        pavu_mixer.send(msg).with_context(|| {
                            format!("failed updating channel peak for {:?}", ch_id)
                        })?;
                    }
                }
                e @ pa::Event::UpdateSinks | e @ pa::Event::UpdateSinkInputs => {
                    if e == pa::Event::UpdateSinks {
                        if !ch_main.try_connect(pa)? {
                            log::warn!("no main channel!");
                        }
                    }

                    for (id, ch) in [
                        (common::Channel::Ch1, &mut ch1),
                        (common::Channel::Ch2, &mut ch2),
                        (common::Channel::Ch3, &mut ch3),
                        (common::Channel::Ch4, &mut ch4),
                    ]
                    .iter_mut()
                    {
                        let msg = if ch.try_connect(pa)? {
                            common::HostMessage::UpdateChannelState(*id, Some(!ch.is_muted()))
                        } else {
                            common::HostMessage::UpdateChannelState(*id, None)
                        };
                        pavu_mixer
                            .send(msg)
                            .with_context(|| format!("failed de/activating channel {:?}", id))?;
                    }
                }
            }
        }

        if let Some(msg) = pavu_mixer.try_recv()? {
            match msg {
                common::DeviceMessage::UpdateVolume(ch_id, vol) => {
                    let ch = match ch_id {
                        common::Channel::Main => &mut ch_main,
                        common::Channel::Ch1 => &mut ch1,
                        common::Channel::Ch2 => &mut ch2,
                        common::Channel::Ch3 => &mut ch3,
                        common::Channel::Ch4 => &mut ch4,
                    };

                    let vol = vol.clamp(0.0, 1.0);
                    log::debug!("{:?} Volume: {:3.0}", ch_id, vol * 100.0);
                    ch.set_volume(pa, vol)?;
                }
                common::DeviceMessage::ToggleChannelMute(ch_id) => {
                    log::info!("Muting/unmuting {:?}", ch_id);
                    let ch = match ch_id {
                        common::Channel::Main => &mut ch_main,
                        common::Channel::Ch1 => &mut ch1,
                        common::Channel::Ch2 => &mut ch2,
                        common::Channel::Ch3 => &mut ch3,
                        common::Channel::Ch4 => &mut ch4,
                    };

                    // TODO: Check current state
                    let muted = !ch.is_muted();
                    let new_state = ch.mute(pa, muted)?;

                    pavu_mixer
                        .send(common::HostMessage::UpdateChannelState(
                            ch_id,
                            Some(!new_state),
                        ))
                        .with_context(|| format!("failed de/activating channel {:?}", ch_id))?;
                }
            }
        }
    }
}
