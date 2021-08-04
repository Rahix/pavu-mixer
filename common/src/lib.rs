#![no_std]

pub const ICON_SIZE: usize = 100;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Channel {
    Ch1,
    Ch2,
    Ch3,
    Ch4,
    Main,
}

impl Channel {
    #[inline]
    pub fn to_index(self) -> usize {
        match self {
            Channel::Ch1 => 0,
            Channel::Ch2 => 1,
            Channel::Ch3 => 2,
            Channel::Ch4 => 3,
            Channel::Main => panic!("called to_index() for Channel::Main"),
        }
    }

    #[inline]
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Channel::Ch1,
            1 => Channel::Ch2,
            2 => Channel::Ch3,
            3 => Channel::Ch4,
            i => panic!("invalid channel index {}", i),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum ChannelState {
    Inactive,
    Running,
    Muted,
}

impl ChannelState {
    #[inline]
    pub fn is_active(self) -> bool {
        self != ChannelState::Inactive
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum HostMessage {
    UpdatePeak(Channel, f32),
    UpdateChannelState(Channel, ChannelState),
    SetIcon(Channel),
    ForceUpdate,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum DeviceMessage {
    UpdateVolume(Channel, f32),
    ToggleChannelMute(Channel),
}
