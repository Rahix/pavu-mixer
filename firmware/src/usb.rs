use crate::level;
use crate::status_leds;
use core::cell::RefCell;
use embedded_hal::digital::v2::OutputPin;
use rtt_target::rprintln;

#[derive(Debug)]
pub enum Error {
    Usb(usb_device::UsbError),
    Serdes(postcard::Error),
    WouldBlock,
}

impl From<usb_device::UsbError> for Error {
    fn from(e: usb_device::UsbError) -> Self {
        match e {
            usb_device::UsbError::WouldBlock => Self::WouldBlock,
            e => Self::Usb(e),
        }
    }
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Self::Serdes(e)
    }
}

/// Custom USB class for Pavu-Mixer.
///
/// This class provides one interface which looks like this:
/// ```text
/// Interface Descriptor:
///   bLength                 9
///   bDescriptorType         4
///   bInterfaceNumber        0
///   bAlternateSetting       0
///   bNumEndpoints           2
///   bInterfaceClass       255 Vendor Specific Class
///   bInterfaceSubClass    195
///   bInterfaceProtocol    195
///   iInterface              0
///   Endpoint Descriptor:
///     bLength                 7
///     bDescriptorType         5
///     bEndpointAddress     0x01  EP 1 OUT
///     bmAttributes            3
///       Transfer Type            Interrupt
///       Synch Type               None
///       Usage Type               Data
///     wMaxPacketSize     0x0040  1x 64 bytes
///     bInterval              10
///   Endpoint Descriptor:
///     bLength                 7
///     bDescriptorType         5
///     bEndpointAddress     0x81  EP 1 IN
///     bmAttributes            3
///       Transfer Type            Interrupt
///       Synch Type               None
///       Usage Type               Data
///     wMaxPacketSize     0x0040  1x 64 bytes
///     bInterval             100
/// ```
pub struct PavuMixerClass<'a, B: usb_device::bus::UsbBus> {
    interface: usb_device::bus::InterfaceNumber,
    read_ep: usb_device::endpoint::EndpointOut<'a, B>,
    write_ep: usb_device::endpoint::EndpointIn<'a, B>,
}

impl<'a, B: usb_device::bus::UsbBus> PavuMixerClass<'a, B> {
    pub fn new(alloc: &'a usb_device::bus::UsbBusAllocator<B>) -> Self {
        Self {
            interface: alloc.interface(),
            read_ep: alloc.interrupt(64, 10),   // 10ms
            write_ep: alloc.interrupt(64, 100), // 100ms
        }
    }

    /// Attempt receiving a message from the USB host.
    ///
    /// If no message could be received, `Error::WouldBlock` is returned.
    pub fn recv_host_message(&mut self) -> Result<common::HostMessage, Error> {
        let mut buf = [0x00; 64];
        let bytes_read = self.read_ep.read(&mut buf)?;
        let msg = postcard::from_bytes(&buf[0..bytes_read])?;
        Ok(msg)
    }

    /// Send a message to the USB host.
    ///
    /// If a messages is still in-flight, this returns `Error::WouldBlock`.
    pub fn send_device_message(&mut self, msg: common::DeviceMessage) -> Result<(), Error> {
        let mut buf = [0x00; 64];
        let bytes = postcard::to_slice(&msg, &mut buf)?;
        self.write_ep.write(bytes)?;
        Ok(())
    }
}

impl<'a, B: usb_device::bus::UsbBus> usb_device::class::UsbClass<B> for PavuMixerClass<'a, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut usb_device::descriptor::DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(self.interface, 0xff, 0xc3, 0xc3)?;
        writer.endpoint(&self.read_ep)?;
        writer.endpoint(&self.write_ep)?;
        Ok(())
    }
}

pub async fn usb_recv_task<'a, B, E>(
    usb_dev: &mut usb_device::device::UsbDevice<'a, B>,
    usb_class: &RefCell<PavuMixerClass<'a, B>>,
    mut main_level: level::ShiftRegLevel<impl OutputPin, impl OutputPin, impl OutputPin>,
    mut main_leds: status_leds::ChannelStatusLeds<
        impl OutputPin,
        impl OutputPin<Error = E>,
        impl OutputPin<Error = E>,
    >,
    mut ch1_level: level::PwmLevel<impl embedded_hal::PwmPin<Duty = u16>>,
    mut ch1_leds: status_leds::ChannelStatusLeds<
        impl OutputPin,
        impl OutputPin<Error = E>,
        impl OutputPin<Error = E>,
    >,
    mut ch2_level: level::PwmLevel<impl embedded_hal::PwmPin<Duty = u16>>,
    mut ch2_leds: status_leds::ChannelStatusLeds<
        impl OutputPin,
        impl OutputPin<Error = E>,
        impl OutputPin<Error = E>,
    >,
    mut ch3_level: level::PwmLevel<impl embedded_hal::PwmPin<Duty = u16>>,
    mut ch3_leds: status_leds::ChannelStatusLeds<
        impl OutputPin,
        impl OutputPin<Error = E>,
        impl OutputPin<Error = E>,
    >,
    mut ch4_level: level::PwmLevel<impl embedded_hal::PwmPin<Duty = u16>>,
    mut ch4_leds: status_leds::ChannelStatusLeds<
        impl OutputPin,
        impl OutputPin<Error = E>,
        impl OutputPin<Error = E>,
    >,
) where
    B: usb_device::bus::UsbBus,
    E: core::fmt::Debug,
{
    loop {
        if {
            let mut usb_class = usb_class.borrow_mut();
            !usb_dev.poll(&mut [&mut *usb_class])
        } {
            cassette::yield_now().await;
            continue;
        }

        match {
            let mut usb_class = usb_class.borrow_mut();
            usb_class.recv_host_message()
        } {
            Err(Error::WouldBlock) => (),
            Err(e) => rprintln!("USB read error: {:?}", e),
            Ok(msg) => match msg {
                common::HostMessage::UpdatePeak(common::Channel::Main, v) => {
                    main_level.update_level(v);
                }
                common::HostMessage::UpdatePeak(ch, v) => match ch {
                    common::Channel::Ch1 => ch1_level.update_level(v),
                    common::Channel::Ch2 => ch2_level.update_level(v),
                    common::Channel::Ch3 => ch3_level.update_level(v),
                    common::Channel::Ch4 => ch4_level.update_level(v),
                    _ => unreachable!(),
                },
                common::HostMessage::UpdateChannelState(ch, state) => match ch {
                    common::Channel::Main => {
                        main_leds.set_button_led_state(state).unwrap();
                    }
                    common::Channel::Ch1 => {
                        ch1_leds.set_button_led_state(state).unwrap();
                        if !state.is_active() {
                            ch1_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch2 => {
                        ch2_leds.set_button_led_state(state).unwrap();
                        if !state.is_active() {
                            ch2_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch3 => {
                        ch3_leds.set_button_led_state(state).unwrap();
                        if !state.is_active() {
                            ch3_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch4 => {
                        ch4_leds.set_button_led_state(state).unwrap();
                        if !state.is_active() {
                            ch4_level.update_level(0.0);
                        }
                    }
                },
            },
        }
    }
}

pub async fn usb_send_task<'a, B>(
    usb_class: &RefCell<PavuMixerClass<'a, B>>,
    pending_volume_updates: &RefCell<heapless::LinearMap<common::Channel, f32, 5>>,
    pending_presses: &RefCell<heapless::LinearMap<common::Channel, (), 5>>,
) where
    B: usb_device::bus::UsbBus,
{
    loop {
        for ch in &[
            common::Channel::Main,
            common::Channel::Ch1,
            common::Channel::Ch2,
            common::Channel::Ch3,
            common::Channel::Ch4,
        ] {
            'try_send_mute_loop: loop {
                let mut pending_presses = pending_presses.borrow_mut();
                if let Some(()) = pending_presses.get(ch) {
                    let mut usb_class = usb_class.borrow_mut();
                    match usb_class
                        .send_device_message(common::DeviceMessage::ToggleChannelMute(*ch))
                    {
                        Ok(()) => {
                            pending_presses.remove(ch);
                            break 'try_send_mute_loop;
                        }
                        Err(Error::WouldBlock) => (),
                        Err(e) => rprintln!("USB write error: {:?}", e),
                    }
                } else {
                    break 'try_send_mute_loop;
                }

                drop(pending_presses);
                cassette::yield_now().await;
            }

            'try_send_volume_loop: loop {
                let mut pending_volume_updates = pending_volume_updates.borrow_mut();
                if let Some(volume) = pending_volume_updates.get(ch) {
                    let mut usb_class = usb_class.borrow_mut();
                    match usb_class
                        .send_device_message(common::DeviceMessage::UpdateVolume(*ch, *volume))
                    {
                        Ok(()) => {
                            pending_volume_updates.remove(ch);
                            break 'try_send_volume_loop;
                        }
                        Err(Error::WouldBlock) => (),
                        Err(e) => rprintln!("USB write error: {:?}", e),
                    }
                } else {
                    break 'try_send_volume_loop;
                }

                drop(pending_volume_updates);
                cassette::yield_now().await;
            }
        }

        // yield after all channels were updated (or weren't) because otherwise we'd busy loop here...
        cassette::yield_now().await;
    }
}
