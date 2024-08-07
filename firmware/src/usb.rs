use crate::display;
use crate::level;
use crate::status_leds;
use crate::ResultWarn;
use core::cell::{Cell, RefCell};
use embedded_hal::digital::v2::OutputPin;
use rtt_target::rprintln;

#[allow(dead_code)]
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
    bulk_ep: usb_device::endpoint::EndpointOut<'a, B>,
}

impl<'a, B: usb_device::bus::UsbBus> PavuMixerClass<'a, B> {
    pub fn new(alloc: &'a usb_device::bus::UsbBusAllocator<B>) -> Self {
        Self {
            interface: alloc.interface(),
            read_ep: alloc.interrupt(64, 10),   // 10ms
            write_ep: alloc.interrupt(64, 100), // 100ms
            bulk_ep: alloc.bulk(64),
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

    /// Attempt receiving bulk data from the USB host.
    ///
    /// If no message could be received, `Error::WouldBlock` is returned.
    pub fn recv_bulk(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        Ok(self.bulk_ep.read(buf)?)
    }

    /// Send a message to the USB host.
    ///
    /// If a messages is still in-flight, this returns `Error::WouldBlock`.
    #[allow(dead_code)]
    pub fn send_device_message(&mut self, msg: common::DeviceMessage) -> Result<(), Error> {
        let mut buf = [0x00; 64];
        let bytes = postcard::to_slice(&msg, &mut buf)?;
        self.write_ep.write(bytes)?;
        Ok(())
    }

    pub async fn send_device_message_async(
        this: &RefCell<Self>,
        msg: common::DeviceMessage,
    ) -> Result<(), Error> {
        let mut buf = [0x00; 64];
        let bytes = postcard::to_slice(&msg, &mut buf)?;

        futures_util::future::poll_fn(|_| {
            let this = this.borrow_mut();
            match this.write_ep.write(bytes) {
                Ok(_) => core::task::Poll::Ready(Ok(())),
                Err(usb_device::UsbError::WouldBlock) => core::task::Poll::Pending,
                Err(e) => core::task::Poll::Ready(Err(Error::Usb(e))),
            }
        })
        .await
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
        writer.endpoint(&self.bulk_ep)?;
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
    mut gui: display::Gui<
        impl embedded_hal::blocking::spi::Write<u8>,
        impl OutputPin,
        impl OutputPin,
        impl OutputPin,
        impl OutputPin,
    >,
    pending_forced_update: &Cell<bool>,
) where
    B: usb_device::bus::UsbBus,
    E: core::fmt::Debug,
{
    let mut suspend = true;
    loop {
        let new_suspend = match usb_dev.state() {
            usb_device::device::UsbDeviceState::Suspend => true,
            usb_device::device::UsbDeviceState::Configured => false,
            _ => suspend,
        };

        if suspend != new_suspend {
            // If we're going into suspend, turn off all UI
            if new_suspend {
                gui.suspend();
                let _ = ch1_level.update_level(0.0);
                let _ = ch2_level.update_level(0.0);
                let _ = ch3_level.update_level(0.0);
                let _ = ch4_level.update_level(0.0);
                let _ = main_level.update_level(0.0);
                let _ = ch1_leds.set_button_led_state(common::ChannelState::Inactive);
                let _ = ch2_leds.set_button_led_state(common::ChannelState::Inactive);
                let _ = ch3_leds.set_button_led_state(common::ChannelState::Inactive);
                let _ = ch4_leds.set_button_led_state(common::ChannelState::Inactive);
                let _ = main_leds.set_button_led_state(common::ChannelState::Inactive);
                let _ = ch1_leds.set_sync(false);
                let _ = ch2_leds.set_sync(false);
                let _ = ch3_leds.set_sync(false);
                let _ = ch4_leds.set_sync(false);
                let _ = main_leds.set_sync(false);
            } else {
                gui.resume();
            }
        }

        suspend = new_suspend;

        if {
            let mut usb_class = usb_class.borrow_mut();
            !usb_dev.poll(&mut [&mut *usb_class])
        } {
            cassette::yield_now().await;
            continue;
        }

        // If we're currently expecting icon data, try polling for it.
        gui.try_push_icon_data_if_active(|buf| match usb_class.borrow_mut().recv_bulk(buf) {
            Err(Error::WouldBlock) => 0,
            Ok(len) => len,
            Err(e) => {
                rprintln!("USB read error: {:?}", e);
                0
            }
        });

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
                        main_leds
                            .set_button_led_state(state)
                            .err_warn("Failed setting LEDs");
                    }
                    common::Channel::Ch1 => {
                        ch1_leds
                            .set_button_led_state(state)
                            .err_warn("Failed setting LEDs");
                        if !state.is_active() {
                            ch1_level.update_level(0.0);
                            gui.clear_icon(ch);
                        }
                    }
                    common::Channel::Ch2 => {
                        ch2_leds
                            .set_button_led_state(state)
                            .err_warn("Failed setting LEDs");
                        if !state.is_active() {
                            ch2_level.update_level(0.0);
                            gui.clear_icon(ch);
                        }
                    }
                    common::Channel::Ch3 => {
                        ch3_leds
                            .set_button_led_state(state)
                            .err_warn("Failed setting LEDs");
                        if !state.is_active() {
                            ch3_level.update_level(0.0);
                            gui.clear_icon(ch);
                        }
                    }
                    common::Channel::Ch4 => {
                        ch4_leds
                            .set_button_led_state(state)
                            .err_warn("Failed setting LEDs");
                        if !state.is_active() {
                            ch4_level.update_level(0.0);
                            gui.clear_icon(ch);
                        }
                    }
                },
                common::HostMessage::SetIcon(ch) => {
                    gui.start_icon_stream(ch);
                }
                common::HostMessage::ForceUpdate => {
                    rprintln!("Forcing an update.");
                    pending_forced_update.set(true);
                }
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
            let maybe_pressed = pending_presses.borrow().get(ch).cloned();
            if let Some(()) = maybe_pressed {
                let msg = common::DeviceMessage::ToggleChannelMute(*ch);
                if let Err(e) = PavuMixerClass::send_device_message_async(usb_class, msg).await {
                    rprintln!("USB write error: {:?}", e);
                } else {
                    pending_presses.borrow_mut().remove(ch);
                }
            }

            // The loop for the volume update works differently to ensure we will always send the
            // most recent value possible.
            loop {
                let maybe_volume = pending_volume_updates.borrow().get(ch).cloned();
                if let Some(volume) = maybe_volume {
                    let msg = common::DeviceMessage::UpdateVolume(*ch, volume);
                    let result = usb_class.borrow_mut().send_device_message(msg);
                    match result {
                        Err(Error::WouldBlock) => {
                            cassette::yield_now().await;
                            continue;
                        }
                        Ok(()) => {
                            pending_volume_updates.borrow_mut().remove(ch);
                        }
                        Err(e) => {
                            rprintln!("USB write error: {:?}", e);
                        }
                    }
                }
                break;
            }
        }

        // yield after all channels were updated (or weren't) because otherwise we'd busy loop here...
        cassette::yield_now().await;
    }
}
