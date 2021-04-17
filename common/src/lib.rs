#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Channel {
    Ch1,
    Ch2,
    Ch3,
    Ch4,
    Main,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum HostMessage {
    UpdateVolume(Channel, f32),
}
