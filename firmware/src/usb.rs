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
