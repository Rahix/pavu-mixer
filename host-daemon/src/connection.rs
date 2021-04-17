use crate::config;
use anyhow::Context;

pub struct PavuMixer {
    dev_info: DeviceInfo,
    dev_handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

struct DeviceInfo {
    device: rusb::Device<rusb::GlobalContext>,
    interface: u8,
    interface_setting: u8,
    ep: Endpoints,
}

struct Endpoints {
    read_address: u8,
    write_address: u8,
}

impl PavuMixer {
    pub fn connect(config: &config::Connection) -> anyhow::Result<Self> {
        let dev_info = DeviceInfo::search_device()?;

        if config.sudo_hack {
            let dev_path = std::path::PathBuf::from(format!(
                "/dev/bus/usb/{:03}/{:03}",
                dev_info.device.bus_number(),
                dev_info.device.address()
            ));
            log::warn!("sudo hack! chmodding {:?} ...", dev_path);
            let retcode = std::process::Command::new("sudo")
                .arg("chmod")
                .arg("a+rw")
                .arg(&dev_path)
                .status()?;
            if !retcode.success() {
                anyhow::bail!("sudo hack failed");
            }
        }

        let mut dev_handle = dev_info
            .device
            .open()
            .context("failed opening USB device")?;

        dev_handle
            .claim_interface(dev_info.interface)
            .context("failed claiming USB interface")?;
        dev_handle
            .set_alternate_setting(dev_info.interface, dev_info.interface_setting)
            .context("failed setting up USB interface")?;

        Ok(Self {
            dev_info,
            dev_handle,
        })
    }

    pub fn send(&mut self, msg: common::HostMessage) -> anyhow::Result<()> {
        log::trace!("sending: {:?}", msg);

        // for now we know that the ep can only take 64 bytes
        let mut buf = [0x00; 64];
        let msg_bytes = postcard::to_slice(&msg, &mut buf).context("failed encoding message")?;

        self.dev_handle
            .write_interrupt(
                self.dev_info.ep.write_address,
                &msg_bytes,
                std::time::Duration::from_secs(5),
            )
            .context("failed sending USB message")?;

        Ok(())
    }

    pub fn try_recv(&mut self) -> anyhow::Result<Option<common::DeviceMessage>> {
        let mut buf = [0x00; 64];
        match self.dev_handle.read_interrupt(
            self.dev_info.ep.read_address,
            &mut buf,
            std::time::Duration::from_secs(0),
        ) {
            Ok(len) => {
                let msg_bytes = &buf[0..len];
                let msg = postcard::from_bytes(msg_bytes).context("failed decoding message")?;
                log::trace!("received: {:?}", msg);
                Ok(Some(msg))
            }
            Err(rusb::Error::Timeout) => Ok(None),
            Err(e) => Err(e).context("failed receiving USB message"),
        }
    }
}

impl DeviceInfo {
    fn search_device() -> anyhow::Result<Self> {
        for device in rusb::devices()?.iter() {
            if let Ok(config_desc) = device.active_config_descriptor() {
                for interface in config_desc.interfaces() {
                    for interface_desc in interface.descriptors() {
                        if Self::match_interface(&interface_desc) {
                            return Ok(Self {
                                device,
                                interface: interface.number(),
                                interface_setting: interface_desc.setting_number(),
                                ep: Endpoints::from_descriptor(&interface_desc)?,
                            });
                        }
                    }
                }
            }
        }
        anyhow::bail!("no USB device found");
    }

    /// Match an interface descriptor against our searched interface
    fn match_interface(desc: &rusb::InterfaceDescriptor) -> bool {
        match (
            desc.class_code(),
            desc.sub_class_code(),
            desc.protocol_code(),
        ) {
            (0xff, 0xc3, 0xc3) => true,
            _ => false,
        }
    }
}

impl Endpoints {
    /// Get the endpoints for communication.
    fn from_descriptor(interface_desc: &rusb::InterfaceDescriptor) -> anyhow::Result<Self> {
        let mut found_read_ep = None;
        let mut found_write_ep = None;

        for endpoint_desc in interface_desc.endpoint_descriptors() {
            match endpoint_desc.direction() {
                rusb::Direction::In => found_read_ep = Some(endpoint_desc.address()),
                rusb::Direction::Out => found_write_ep = Some(endpoint_desc.address()),
            }
        }

        Ok(Self {
            read_address: found_read_ep.context("missing read endpoint")?,
            write_address: found_write_ep.context("missing write endpoint")?,
        })
    }
}
