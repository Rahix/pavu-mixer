#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use micromath::F32Ext;
use stm32f3xx_hal::{self as hal, pac, prelude::*};

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

    pub fn read<'b>(&mut self, buf: &'b mut [u8]) -> usb_device::Result<&'b mut [u8]> {
        let bytes_read = self.read_ep.read(buf)?;
        Ok(&mut buf[0..bytes_read])
    }

    pub fn write(&mut self, buf: &[u8]) -> usb_device::Result<usize> {
        self.write_ep.write(buf)
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

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let mut dp = pac::Peripherals::take().unwrap();
    let _cp = cortex_m::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8u32.mhz())
        .sysclk(48u32.mhz())
        .pclk1(24u32.mhz())
        .freeze(&mut flash.acr);

    assert!(clocks.usbclk_valid());

    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);
    let mut gpioe = dp.GPIOE.split(&mut rcc.ahb);
    let mut gpiof = dp.GPIOF.split(&mut rcc.ahb);

    // -----------------------------------------------

    let mut adc1 = hal::adc::Adc::adc1(
        dp.ADC1, // The ADC we are going to control
        // The following is only needed to make sure the clock signal for the ADC is set up
        // correctly.
        &mut dp.ADC1_2,
        &mut rcc.ahb,
        hal::adc::CkMode::default(),
        clocks,
    );

    rprintln!("ADC initialized.");

    let tim1_channels = hal::pwm::tim1(dp.TIM1, 1280, 100.hz(), &clocks);

    let pe9 = gpioe.pe9.into_af2(&mut gpioe.moder, &mut gpioe.afrh);
    let pe11 = gpioe.pe11.into_af2(&mut gpioe.moder, &mut gpioe.afrh);
    let pe13 = gpioe.pe13.into_af2(&mut gpioe.moder, &mut gpioe.afrh);
    let pe14 = gpioe.pe14.into_af2(&mut gpioe.moder, &mut gpioe.afrh);
    let mut ch1_pwm = tim1_channels.0.output_to_pe9(pe9);
    let mut ch2_pwm = tim1_channels.1.output_to_pe11(pe11);
    let mut ch3_pwm = tim1_channels.2.output_to_pe13(pe13);
    let mut ch4_pwm = tim1_channels.3.output_to_pe14(pe14);

    rprintln!("PWM initialized.");

    // F3 Discovery board has a pull-up resistor on the D+ line.
    // Pull the D+ pin down to send a RESET condition to the USB bus.
    // This forced reset is needed only for development, without it host
    // will not reset your device when you upload new firmware.
    let mut usb_dp = gpioa
        .pa12
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    usb_dp.set_low().ok();
    cortex_m::asm::delay(clocks.sysclk().0 / 100);

    let usb = hal::usb::Peripheral {
        usb: dp.USB,
        pin_dm: gpioa.pa11.into_af14(&mut gpioa.moder, &mut gpioa.afrh),
        pin_dp: usb_dp.into_af14(&mut gpioa.moder, &mut gpioa.afrh),
    };
    let usb_bus = hal::usb::UsbBus::new(usb);

    let mut usb_class = PavuMixerClass::new(&usb_bus);

    let mut usb_dev = usb_device::prelude::UsbDeviceBuilder::new(
        &usb_bus,
        usb_device::prelude::UsbVidPid(0x1209, 0x0001),
    )
    .manufacturer("Rahix")
    .product("Pavu Mixer")
    .serial_number("DEADBEEF")
    .device_class(0x00)
    .build();

    rprintln!("USB device initialized.");

    let mut data = gpiob
        .pb15
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    let mut dclk = gpiob
        .pb13
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    let mut sclk = gpiob
        .pb12
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);

    rprintln!("GPIOs initialized.");

    let mut adc1_in5_pin = gpiof.pf4.into_analog(&mut gpiof.moder, &mut gpiof.pupdr);

    loop {
        if !usb_dev.poll(&mut [&mut usb_class]) {
            continue;
        }

        let mut buf = [0x00; 64];
        match usb_class.read(&mut buf) {
            Ok(buf) if buf.len() > 0 => {
                if let Ok(msg) = postcard::from_bytes::<common::HostMessage>(buf) {
                    match msg {
                        common::HostMessage::UpdatePeak(common::Channel::Main, v) => {
                            let value = (v * 20.5) as u32;

                            for i in 0..20 {
                                if (19 - i) <= value {
                                    data.set_low().unwrap();
                                } else {
                                    data.set_high().unwrap();
                                }

                                dclk.set_high().unwrap();
                                dclk.set_low().unwrap();
                            }

                            sclk.set_high().unwrap();
                            sclk.set_low().unwrap();
                        }
                        common::HostMessage::UpdatePeak(ch, v) => {
                            let ch_pwm: &mut dyn embedded_hal::PwmPin<Duty = u16> = match ch {
                                common::Channel::Ch1 => &mut ch1_pwm,
                                common::Channel::Ch2 => &mut ch2_pwm,
                                common::Channel::Ch3 => &mut ch3_pwm,
                                common::Channel::Ch4 => &mut ch4_pwm,
                                _ => unreachable!(),
                            };

                            if v > 0.01 {
                                ch_pwm.enable();
                                ch_pwm.set_duty(
                                    (ch_pwm.get_max_duty() as f32 * (1.0 - v.powf(2.8))) as u16,
                                );
                            } else {
                                ch_pwm.disable();
                            }
                        }
                    }
                } else {
                    rprintln!("Failed decoding: {:?}", buf);
                }
            }
            Err(usb_device::UsbError::WouldBlock) => (),
            Err(e) => {
                rprintln!("Error: {:?}", e);
            }
            _ => (),
        }

        // check main fader and send back value
        let raw_value: u16 = adc1.read(&mut adc1_in5_pin).expect("Error reading adc1.");
        let mut main = raw_value;
        if main > 4080 {
            main = 4080;
        }
        if main < 8 {
            main = 8;
        }
        main -= 8;
        let main = main as f32 / ((4080 - 8) as f32);

        let msg = common::DeviceMessage::UpdateVolume(common::Channel::Main, main);
        let mut buf = [0x00; 64];
        let bytes = postcard::to_slice(&msg, &mut buf).unwrap();

        match usb_class.write(bytes) {
            Ok(_) => (),
            Err(usb_device::UsbError::WouldBlock) => (),
            Err(e) => panic!("write error: {:?}", e),
        }
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
