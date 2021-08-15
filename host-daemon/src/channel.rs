use std::collections;
use std::rc::Rc;

#[derive(Debug)]
struct StreamData {
    stream: crate::pa::Stream,
    last_peak: f32,
}

/// Representation of one of the "physical" mixer channels.
///
/// This explicitly includes the `main` "channel".
#[derive(Debug)]
pub struct Channel {
    /// Current fader position as last reported by the mixer.
    current_volume: f32,
    /// Attached Pulseaudio streams - their volume is controlled by this channel.
    attached_streams: slab::Slab<StreamData>,
    /// Property matches for this channel (from the configuration).
    property_matches: Option<Rc<Vec<collections::BTreeMap<String, String>>>>,
    /// Whether this channel is currently muted.
    mute: bool,
    /// The current volume for this channel, as last reported by the mixer.
    volume: Option<f32>,
}

impl Channel {
    pub fn new(property_matches: Option<Rc<Vec<collections::BTreeMap<String, String>>>>) -> Self {
        Self {
            current_volume: 0.0,
            attached_streams: slab::Slab::new(),
            property_matches,
            mute: false,
            volume: None,
        }
    }

    pub fn match_sink_input(&self, info: &crate::pa::SinkInputInfo) -> bool {
        if let Some(property_matches) = &self.property_matches {
            'sets_loop: for matches_set in property_matches.iter() {
                for (name, value) in matches_set.iter() {
                    if info.properties.get_str(name).as_ref() != Some(value) {
                        continue 'sets_loop;
                    }
                }
                // only reached after all properties in some match set have been matched
                return true;
            }
        }
        return false;
    }

    /// Detach all currently connected streams.
    pub fn detach_all(&mut self) {
        // TODO: Graceful cleanup, for now let's rely on the streams Drop impl
        self.attached_streams = slab::Slab::new();
    }

    /// Attach a new stream to this channel.
    ///
    /// Returns a mutable reference and the index where it was inserted
    pub fn attach_stream(
        &mut self,
        pa: &mut crate::pa::PulseInterface,
        mut stream: crate::pa::Stream,
    ) -> (&mut crate::pa::Stream, usize, common::ChannelState) {
        // if this is the first stream, we need to update our local mute information.
        // This will be the initial source of truth for the channel until the device updates it.
        if self.attached_streams.is_empty() {
            self.mute = stream.is_mute();
        }

        // bring the stream in line with our knowledge
        if stream.is_mute() != self.mute {
            stream.set_mute(pa, self.mute);
        }

        if let Some(volume) = self.volume {
            stream.set_volume(pa, volume);
        }

        let index = self.attached_streams.insert(StreamData {
            stream,
            last_peak: 0.0,
        });
        let state = self.state();
        (&mut self.attached_streams[index].stream, index, state)
    }

    pub fn try_drop_stream(&mut self, sink_input: u32) -> common::ChannelState {
        self.attached_streams
            .retain(|_, stream_data| !stream_data.stream.is_for_sink_input(sink_input));
        self.state()
    }

    pub fn update_peak(&mut self, index: usize) -> anyhow::Result<f32> {
        match self.attached_streams[index].stream.get_recent_peak() {
            Ok(Some(peak)) => self.attached_streams[index].last_peak = peak,
            Err(_) => self.attached_streams[index].last_peak = 0.0,
            _ => (),
        }
        Ok(self
            .attached_streams
            .iter()
            .map(|(_, s)| s.last_peak)
            .max_by(|a, b| a.partial_cmp(b).expect("wrong peak information"))
            .expect("no streams found"))
    }

    pub fn update_volume(&mut self, pa: &mut crate::pa::PulseInterface, volume: f32) {
        self.volume = Some(volume);
        for (_, stream_data) in self.attached_streams.iter_mut() {
            stream_data.stream.set_volume(pa, volume);
        }
    }

    pub fn toggle_mute(&mut self, pa: &mut crate::pa::PulseInterface) -> common::ChannelState {
        self.mute = !self.mute;
        for (_, stream_data) in self.attached_streams.iter_mut() {
            stream_data.stream.set_mute(pa, self.mute);
        }
        self.state()
    }

    pub fn state(&self) -> common::ChannelState {
        if self.attached_streams.is_empty() {
            common::ChannelState::Inactive
        } else if self.mute {
            common::ChannelState::Muted
        } else {
            common::ChannelState::Running
        }
    }
}
