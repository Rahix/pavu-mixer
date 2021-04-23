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

    pub fn recv_host_message(&mut self) -> Result<common::HostMessage, Error> {
        let mut buf = [0x00; 64];
        let bytes_read = self.read_ep.read(&mut buf)?;
        let msg = postcard::from_bytes(&buf[0..bytes_read])?;
        Ok(msg)
    }

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
