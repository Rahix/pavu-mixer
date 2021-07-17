use std::collections;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub connection: Connection,

    #[serde(default)]
    pub icon_mappings: collections::BTreeMap<String, String>,

    pub channel_1: Channel,
    pub channel_2: Channel,
    pub channel_3: Channel,
    pub channel_4: Channel,
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
    pub property_matches: Rc<collections::BTreeMap<String, String>>,
}

impl Default for Config {
    fn default() -> Self {
        toml::de::from_str(include_str!("default-config.toml")).unwrap()
    }
}
