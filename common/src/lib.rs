#![no_std]

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Channel {
    Ch1,
    Ch2,
    Ch3,
    Ch4,
    Main,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum HostMessage {
    UpdatePeak(Channel, f32),
    ActivateChannel(Channel, bool),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum DeviceMessage {
    UpdateVolume(Channel, f32),
}
