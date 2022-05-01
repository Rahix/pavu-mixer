use std::collections;
use std::rc::Rc;

pub type PropertyMatches = Rc<Vec<collections::BTreeMap<String, String>>>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub connection: Connection,

    pub channel_1: Channel,
    pub channel_2: Channel,
    pub channel_3: Channel,
    pub channel_4: Channel,

    pub icon_mappings: Vec<IconMapping>,

    pub sink_peak_multiplier: Vec<SinkPeakMultiplier>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Connection {
    /// Use `sudo chmod` to make the device accessible instead of proper udev
    pub sudo_hack: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Channel {
    pub property_matches: PropertyMatches,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IconMapping {
    pub icon: String,
    pub property_matches: collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SinkPeakMultiplier {
    pub sink_name: String,
    pub multiplier: f32,
}

impl Default for Config {
    fn default() -> Self {
        toml::de::from_str(include_str!("default-config.toml")).unwrap()
    }
}
