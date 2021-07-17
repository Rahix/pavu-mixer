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
                common::DeviceMessage::UpdateVolume(ch, volume) => match ch {
                    common::Channel::Main => main.update_volume(&mut pa, volume),
                    ch => channels[ch.to_index()].update_volume(&mut pa, volume),
                },
                m => log::warn!("Unhandled device message: {:?}", m),
            }
        }

        pa.iterate(true)?;
    }
}
