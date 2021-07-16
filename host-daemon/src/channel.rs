use std::collections;
use std::rc::Rc;

/// Representation of one of the "physical" mixer channels.
///
/// This explicitly includes the `main` "channel".
#[derive(Debug)]
pub struct Channel {
    /// Label describing this channel.  Should eventually be displayed on the LCD.
    label: String,
    /// Current fader position as last reported by the mixer.
    current_volume: f32,
    /// Attached Pulseaudio streams - their volume is controlled by this channel.
    attached_streams: Vec<()>,
    /// Property matches for this channel (from the configuration).
    property_matches: Option<Rc<collections::BTreeMap<String, String>>>,
}

impl Channel {
    pub fn new(property_matches: Option<Rc<collections::BTreeMap<String, String>>>) -> Self {
        Self {
            label: "<inactive>".to_string(),
            current_volume: 0.0,
            attached_streams: vec![],
            property_matches,
        }
    }
}
