use anyhow::Context;

mod channel;
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

    let config: config::Config =
        confy::load("pavu-mixer").context("failed loading configuration")?;

    let mut pavu_mixer =
        connection::PavuMixer::connect(&config.connection).context("failed connecting to mixer")?;

    let mut main = channel::Channel::new(None);
    let mut channels = [
        channel::Channel::new(Some(config.channel_1.property_matches.clone())),
        channel::Channel::new(Some(config.channel_2.property_matches.clone())),
        channel::Channel::new(Some(config.channel_3.property_matches.clone())),
        channel::Channel::new(Some(config.channel_4.property_matches.clone())),
    ];

    let mut pa = pa::PulseInterface::init().context("failed initializing pulseaudio client")?;

    let events = pa.take_event_receiver().expect("events channel missing");

    loop {
        // Handle all pending events from PulseAudio.
        for event in events.try_iter() {
            match event {
                pa::Event::NewDefaultSink(stream) => {
                    main.detach_all();
                    let (stream, index) = main.attach_stream(stream);
                    stream.set_connected_channel(common::Channel::Main, index);
                }
                pa::Event::NewPeakData(ch, index) => {
                    let peak = match ch {
                        common::Channel::Main => main.update_peak(index)?,
                        ch => channels[ch.to_index()].update_peak(index)?,
                    };
                    pavu_mixer.send(common::HostMessage::UpdatePeak(ch, peak))?;
                }
                pa::Event::SinkInputAdded(info) => {
                    // check whether this sink-input should be connected to one of our channels -
                    // if yes, request a stream for it.
                    for (index, channel) in channels.iter().enumerate() {
                        if channel.match_sink_input(&info) {
                            log::debug!(
                                "matched for {:?}: {:#?}",
                                common::Channel::from_index(index),
                                info
                            );
                            pa.request_sink_input_stream(info, common::Channel::from_index(index));
                            break;
                        }
                    }
                }
                pa::Event::NewSinkInput(ch, stream) => {
                    let channel = &mut channels[ch.to_index()];
                    let (stream, index) = channel.attach_stream(stream);
                    stream.set_connected_channel(ch, index);
                }
                e => log::warn!("Unhandled PulseAudio Event: {:#?}", e),
            }
        }

        // Handle pending messages from the mixer device.
        if let Some(message) = pavu_mixer.try_recv().context("failed reading from mixer")? {
            match message {
                m => log::warn!("Unhandled device message: {:?}", m),
            }
        }

        pa.iterate(true)?;
    }
}

#[cfg(f)]
fn inner(pa: &mut pa::PulseInterface) -> anyhow::Result<()> {
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
                            common::HostMessage::UpdateChannelState(
                                *id,
                                if ch.is_muted() {
                                    common::ChannelState::Muted
                                } else {
                                    common::ChannelState::Running
                                },
                            )
                        } else {
                            common::HostMessage::UpdateChannelState(
                                *id,
                                common::ChannelState::Inactive,
                            )
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
                            if new_state {
                                common::ChannelState::Muted
                            } else {
                                common::ChannelState::Running
                            },
                        ))
                        .with_context(|| format!("failed de/activating channel {:?}", ch_id))?;
                }
            }
        }
    }
}
