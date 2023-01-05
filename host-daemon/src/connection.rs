use crate::config;
use anyhow::Context;
use std::sync;
use std::sync::atomic;
use std::time;

/// Error to mark that the USB device disconnected.
///
/// This "error" is handled specially to allow the application to gracefully shutdown in such a
/// case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceDisconnectedError;

impl std::fmt::Display for DeviceDisconnectedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mixer USB device disconnected.")
    }
}

impl std::error::Error for DeviceDisconnectedError {}

fn interpret_usb_error(e: rusb::Error) -> anyhow::Error {
    match e {
        rusb::Error::NoDevice => anyhow::Error::new(DeviceDisconnectedError),
        // We assume an I/O error also means the device vanished from the bus...
        rusb::Error::Io => anyhow::Error::new(DeviceDisconnectedError),
        e => anyhow::Error::new(e).context("error in USB communication"),
    }
}

pub struct PavuMixer {
    dev_info: sync::Arc<DeviceInfo>,
    dev_handle: sync::Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    incoming: sync::mpsc::Receiver<anyhow::Result<common::DeviceMessage>>,
    teardown_flag: sync::Arc<atomic::AtomicBool>,
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
    bulk_address: u8,
}

impl PavuMixer {
    pub fn connect(config: &config::Connection) -> anyhow::Result<Self> {
        let strategy = backoff::ExponentialBackoff {
            initial_interval: time::Duration::from_millis(500),
            multiplier: 1.25,
            max_elapsed_time: Some(time::Duration::from_secs(60)),
            ..Default::default()
        };
        let dev_info = match backoff::retry(strategy, || {
            DeviceInfo::search_device().map_err(|e| {
                log::warn!("No USB device found, retrying...");
                backoff::Error::transient(e)
            })
        }) {
            Ok(d) => d,
            Err(_) => anyhow::bail!("Could not find USB device in 60 seconds, giving up"),
        };

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

        let dev_info = sync::Arc::new(dev_info);
        let dev_handle = sync::Arc::new(dev_handle);
        let (tx, rx) = sync::mpsc::channel();
        let teardown_flag = sync::Arc::new(atomic::AtomicBool::new(false));

        // we spawn a thread for receiving data because `rusb` only exposes blocking APIs and we do
        // not want to block pulseaudio with usb transfers.
        std::thread::spawn({
            let dev_info = sync::Arc::clone(&dev_info);
            let dev_handle = sync::Arc::clone(&dev_handle);
            let teardown_flag = teardown_flag.clone();
            move || receiver_task(dev_handle, dev_info, tx, teardown_flag)
        });

        Ok(Self {
            dev_info,
            dev_handle,
            incoming: rx,
            teardown_flag,
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
            .map_err(interpret_usb_error)?;

        Ok(())
    }

    pub fn try_recv(&mut self) -> anyhow::Result<Option<common::DeviceMessage>> {
        match self.incoming.try_recv() {
            Ok(val) => Some(val).transpose(),
            Err(sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e).context("failed receiving from channel"),
        }
    }

    pub fn send_bulk(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        log::trace!("sending bulk: {} bytes", buf.len());

        self.dev_handle
            .write_bulk(
                self.dev_info.ep.bulk_address,
                buf,
                std::time::Duration::from_secs(5),
            )
            .map_err(interpret_usb_error)?;

        Ok(())
    }
}

impl Drop for PavuMixer {
    fn drop(&mut self) {
        self.teardown_flag.store(true, atomic::Ordering::Relaxed);
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
        let mut found_bulk_ep = None;

        for endpoint_desc in interface_desc.endpoint_descriptors() {
            match (endpoint_desc.transfer_type(), endpoint_desc.direction()) {
                (rusb::TransferType::Interrupt, rusb::Direction::In) => {
                    found_read_ep = Some(endpoint_desc.address())
                }
                (rusb::TransferType::Interrupt, rusb::Direction::Out) => {
                    found_write_ep = Some(endpoint_desc.address())
                }
                (rusb::TransferType::Bulk, rusb::Direction::Out) => {
                    found_bulk_ep = Some(endpoint_desc.address())
                }
                _ => anyhow::bail!("found unexpected endpoint: {:?}", endpoint_desc),
            }
        }

        Ok(Self {
            read_address: found_read_ep.context("missing read endpoint")?,
            write_address: found_write_ep.context("missing write endpoint")?,
            bulk_address: found_bulk_ep.context("missing bulk endpoint")?,
        })
    }
}

fn try_recv(
    dev_handle: &sync::Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    dev_info: &sync::Arc<DeviceInfo>,
) -> anyhow::Result<Option<common::DeviceMessage>> {
    let mut buf = [0x00; 64];
    match dev_handle.read_interrupt(
        dev_info.ep.read_address,
        &mut buf,
        std::time::Duration::from_millis(50),
    ) {
        Ok(len) => {
            let msg_bytes = &buf[0..len];
            let msg = postcard::from_bytes(msg_bytes).context("failed decoding message")?;
            log::trace!("received: {:?}", msg);
            Ok(Some(msg))
        }
        Err(rusb::Error::Timeout) => Ok(None),
        Err(e) => Err(e).map_err(interpret_usb_error),
    }
}

fn receiver_task(
    dev_handle: sync::Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    dev_info: sync::Arc<DeviceInfo>,
    tx: sync::mpsc::Sender<anyhow::Result<common::DeviceMessage>>,
    teardown_flag: sync::Arc<atomic::AtomicBool>,
) {
    loop {
        if let Some(msg) = try_recv(&dev_handle, &dev_info).transpose() {
            tx.send(msg).expect("mpsc sender failed");
        }

        if teardown_flag.load(atomic::Ordering::Relaxed) {
            log::debug!("Receiver task exiting.");
            return;
        }
    }
}
