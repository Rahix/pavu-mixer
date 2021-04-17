use pulse::mainloop::standard as mainloop;
use std::sync::atomic;

use rusb::UsbContext;

fn main() {
    let mut found_device = None;
    let mut found_iface = None;
    let mut found_setting = None;
    let mut found_read_endpoint = None;
    let mut found_write_endpoint = None;
    'loop_devices: for device in rusb::DeviceList::new().unwrap().iter() {
        // find a device with the appropriate vendor class
        if let Ok(config) = device.active_config_descriptor() {
            for interface in config.interfaces() {
                for desc in interface.descriptors() {
                    match (
                        desc.class_code(),
                        desc.sub_class_code(),
                        desc.protocol_code(),
                    ) {
                        (0xff, 0xc3, 0xc3) => {
                            found_device = Some(device);
                            found_iface = Some(interface.number());
                            found_setting = Some(desc.setting_number());

                            // get read and write endpoints
                            for endpoint_desc in desc.endpoint_descriptors() {
                                match endpoint_desc.direction() {
                                    rusb::Direction::In => {
                                        found_read_endpoint = Some(endpoint_desc.address())
                                    }
                                    rusb::Direction::Out => {
                                        found_write_endpoint = Some(endpoint_desc.address())
                                    }
                                }
                            }

                            break 'loop_devices;
                        }
                        _ => (),
                    }
                }
            }
        }
    }
    let usb_device = dbg!(found_device.unwrap());
    let usb_iface = found_iface.unwrap();
    let usb_setting = found_setting.unwrap();
    let usb_read_endpoint = found_read_endpoint.unwrap();
    let usb_write_endpoint = found_write_endpoint.unwrap();
    let mut usb_handle = usb_device.open().unwrap();

    // configure endpoints
    usb_handle.claim_interface(usb_iface).unwrap();
    usb_handle
        .set_alternate_setting(usb_iface, usb_setting)
        .unwrap();

    let ss = pulse::sample::Spec {
        format: pulse::sample::Format::FLOAT32NE,
        channels: 1,
        rate: 25,
    };

    let mut proplist = pulse::proplist::Proplist::new().unwrap();
    proplist
        .set_str(
            pulse::proplist::properties::APPLICATION_NAME,
            "Pavu-Mixer Daemon",
        )
        .unwrap();

    let mut mainloop = mainloop::Mainloop::new().unwrap();

    let mut context =
        pulse::context::Context::new_with_proplist(&mainloop, "PavuMixerContext", &proplist)
            .unwrap();
    context
        .connect(None, pulse::context::FlagSet::NOFLAGS, None)
        .unwrap();

    // Wait for context
    'wait_for_ctx: loop {
        match mainloop.iterate(false) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }
        match context.get_state() {
            pulse::context::State::Ready => {
                break 'wait_for_ctx;
            }
            pulse::context::State::Terminated | pulse::context::State::Failed => {
                panic!("Broken context");
            }
            _ => (),
        }
    }

    let mut stream = pulse::stream::Stream::new(&mut context, "Peak Detect", &ss, None).unwrap();

    // Select which sink-input to monitor
    // stream.set_monitor_stream(0).unwrap();

    // from pavucontrol:src/mainwindow.cc:666
    let stream_flags = pulse::stream::FlagSet::DONT_MOVE
        | pulse::stream::FlagSet::PEAK_DETECT
        | pulse::stream::FlagSet::ADJUST_LATENCY;

    let stream_attrs = pulse::def::BufferAttr {
        fragsize: std::mem::size_of::<f32>() as u32,
        maxlength: u32::MAX,
        ..Default::default()
    };

    let read_length = std::sync::Arc::new(atomic::AtomicUsize::new(0));

    stream.set_read_callback({
        let read_length = read_length.clone();
        Some(Box::new(move |length| {
            read_length.store(length, atomic::Ordering::SeqCst);
        }))
    });

    stream
        .connect_record(Some("2"), Some(&stream_attrs), stream_flags)
        .unwrap();

    // Wait for stream
    'wait_for_stream: loop {
        match mainloop.iterate(false) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }
        match stream.get_state() {
            pulse::stream::State::Ready => {
                break 'wait_for_stream;
            }
            pulse::stream::State::Terminated | pulse::stream::State::Failed => {
                panic!("Broken stream");
            }
            _ => (),
        }
    }

    loop {
        match mainloop.iterate(true) {
            mainloop::IterateResult::Quit(_) | mainloop::IterateResult::Err(_) => {
                panic!("Mainloop iteration error");
            }
            mainloop::IterateResult::Success(_) => (),
        }

        let length = read_length.load(atomic::Ordering::SeqCst);
        if length > 0 {
            match stream.peek().unwrap() {
                pulse::stream::PeekResult::Empty => continue,
                pulse::stream::PeekResult::Hole(_) => stream.discard().unwrap(),
                pulse::stream::PeekResult::Data(d) => {
                    use std::convert::TryInto;
                    let buf: [u8; 4] = d[(d.len() - std::mem::size_of::<f32>())..]
                        .try_into()
                        .unwrap();
                    let v = f32::from_ne_bytes(buf);
                    stream.discard().unwrap();

                    let msg = common::HostMessage::UpdateVolume(common::Channel::Main, v);
                    let bytes = postcard::to_allocvec(&msg).unwrap();
                    usb_handle
                        .write_interrupt(
                            usb_write_endpoint,
                            &bytes,
                            std::time::Duration::from_secs(100),
                        )
                        .unwrap();
                }
            }
        }
    }
}
