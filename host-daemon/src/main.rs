use anyhow::Context;

mod channel;
mod config;
mod connection;
mod icon;
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

    gtk::init()?;

    let mut main = channel::Channel::new(None);
    let mut channels = [
        channel::Channel::new(Some(config.channel_1.property_matches.clone())),
        channel::Channel::new(Some(config.channel_2.property_matches.clone())),
        channel::Channel::new(Some(config.channel_3.property_matches.clone())),
        channel::Channel::new(Some(config.channel_4.property_matches.clone())),
    ];

    let mut pa = pa::PulseInterface::init().context("failed initializing pulseaudio client")?;

    let events = pa.take_event_receiver().expect("events channel missing");

    // When we start up, there might still be some messages waiting for us - drop them because we
    // will request up-to-date ones in the next step.
    while let Some(message) = pavu_mixer.try_recv().context("failed reading from mixer")? {
        log::debug!("Dropping stale message from device: {:?}", message);
    }

    // Force an update during daemon startup so we'll have up-to-date values for all channels.
    pavu_mixer.send(common::HostMessage::ForceUpdate)?;

    loop {
        // Handle all pending events from PulseAudio.
        for event in events.try_iter() {
            match event {
                pa::Event::NewDefaultSink(stream) => {
                    main.detach_all();
                    let (stream, index, state) = main.attach_stream(&mut pa, stream);
                    stream.set_connected_channel(common::Channel::Main, index);
                    pavu_mixer.send(common::HostMessage::UpdateChannelState(
                        common::Channel::Main,
                        state,
                    ))?;
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
                            pa.request_sink_input_stream(info, common::Channel::from_index(index));
                            break;
                        }
                    }
                }
                pa::Event::NewSinkInput(ch, stream) => {
                    let channel = &mut channels[ch.to_index()];
                    let (stream, index, state) = channel.attach_stream(&mut pa, stream);
                    stream.set_connected_channel(ch, index);
                    pavu_mixer.send(common::HostMessage::UpdateChannelState(ch, state))?;
                    if let Some(icon_name) = stream.get_icon_name(&config.icon_mappings) {
                        log::debug!("Icon {:?} for Channel {:?}", icon_name, ch);
                        if let Some(icon_data) = icon::get_icon_data(&icon_name) {
                            pavu_mixer.send(common::HostMessage::SetIcon(ch))?;
                            pavu_mixer.send_bulk(&icon_data)?;
                        }
                    }
                }
                pa::Event::SinkInputRemoved(index) => {
                    for (ch, channel) in channels.iter_mut().enumerate() {
                        let new_state = channel.try_drop_stream(index);
                        pavu_mixer.send(common::HostMessage::UpdateChannelState(
                            common::Channel::from_index(ch),
                            new_state,
                        ))?;
                    }
                }
            }
        }

        // Handle pending messages from the mixer device.
        if let Some(message) = pavu_mixer.try_recv().context("failed reading from mixer")? {
            match message {
                common::DeviceMessage::UpdateVolume(ch, volume) => match ch {
                    common::Channel::Main => main.update_volume(&mut pa, volume),
                    ch => channels[ch.to_index()].update_volume(&mut pa, volume),
                },
                common::DeviceMessage::ToggleChannelMute(ch) => {
                    let new_state = match ch {
                        common::Channel::Main => main.toggle_mute(&mut pa),
                        ch => channels[ch.to_index()].toggle_mute(&mut pa),
                    };
                    pavu_mixer.send(common::HostMessage::UpdateChannelState(ch, new_state))?;
                }
            }
        }

        pa.iterate(true)?;
    }
}
