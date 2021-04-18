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

    let mut fader_ch1_adc = gpioa.pa0.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut fader_ch2_adc = gpioa.pa1.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut fader_ch3_adc = gpioa.pa2.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut fader_ch4_adc = gpioa.pa3.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut fader_main_adc = gpiof.pf4.into_analog(&mut gpiof.moder, &mut gpiof.pupdr);

    let mut previous_fader_values = [-1000.0f32; 5];

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

    let pins = (
        gpiob.pb6.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SCL
        gpiob.pb7.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SDA
    );

    let pca_int = gpioa
        .pa10
        .into_floating_input(&mut gpioa.moder, &mut gpioa.pupdr);

    let mut i2c = hal::i2c::I2c::new(dp.I2C1, pins, 100.khz(), clocks, &mut rcc.apb1);

    rprintln!("I2C bus initialized.");

    i2c.write(0x41, &[0x03, 0x00]).unwrap();
    i2c.write(0x41, &[0x01, 0x0F]).unwrap();

    rprintln!("PCA9536 initialized.");

    // Configure IO0
    //  0: DESYNC_MAIN      = OUTPUT
    //  1: MUTE_MAIN_BTN    = INPUT
    //  2: MUTE_MAIN_LED1   = OUTPUT
    //  3: MUTE_MAIN_LED2   = OUTPUT
    //  4: MUTE1_BTN        = INPUT
    //  5: MUTE1_LED1       = OUTPUT
    //  6: MUTE1_LED2       = OUTPUT
    //  7: MUTE2_BTN        = INPUT
    i2c.write(0x20, &[0x06, 0b10010010]).unwrap();

    // Configure IO1
    //  0: MUTE2_LED1       = OUTPUT
    //  1: MUTE2_LED2       = OUTPUT
    //  2: MUTE3_BTN        = INPUT
    //  3: MUTE3_LED1       = OUTPUT
    //  4: MUTE3_LED2       = OUTPUT
    //  5: MUTE4_BTN        = INPUT
    //  6: MUTE4_LED1       = OUTPUT
    //  7: MUTE4_LED2       = OUTPUT
    i2c.write(0x20, &[0x07, 0b00100100]).unwrap();

    // Read inputs once to clear interrupt
    let mut buf = [0x00];
    i2c.write_read(0x20, &[0x00], &mut buf).unwrap();
    i2c.write_read(0x20, &[0x01], &mut buf).unwrap();

    // Set all outputs appropriately
    i2c.write(0x20, &[0x02, 0b00000001]).unwrap();
    i2c.write(0x20, &[0x03, 0b00000000]).unwrap();

    if pca_int.is_low().unwrap() {
        rprintln!("PCA interrupt is asserted when it should not be!");
    }

    rprintln!("PCA9555 initialized.");

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
    rprintln!("Ready.");
    rprintln!("");

    let mut message_buf = [0x00u8; 64];
    let mut queued_message: Option<&[u8]> = None;
    loop {
        if !usb_dev.poll(&mut [&mut usb_class]) {
            continue;
        }

        let mut buf = [0x00; 64];
        match usb_class.read(&mut buf) {
            Ok(buf) => {
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
                        common::HostMessage::UpdateChannelState(ch, state) => {
                            let mut o_state = [0x00, 0x00];
                            i2c.write_read(0x20, &[0x02], &mut o_state[0..1]).unwrap();
                            i2c.write_read(0x20, &[0x03], &mut o_state[1..2]).unwrap();

                            match ch {
                                common::Channel::Ch1 => {
                                    o_state[0] &= 0b10011111;
                                    if state == Some(true) {
                                        o_state[0] |= 0b00100000;
                                    } else if state == Some(false) {
                                        o_state[0] |= 0b01000000;
                                    }
                                }
                                common::Channel::Ch2 => {
                                    o_state[1] &= 0b11111100;
                                    if state == Some(true) {
                                        o_state[1] |= 0b00000001;
                                    } else if state == Some(false) {
                                        o_state[1] |= 0b00000010;
                                    }
                                }
                                common::Channel::Ch3 => {
                                    o_state[1] &= 0b11100111;
                                    if state == Some(true) {
                                        o_state[1] |= 0b00001000;
                                    } else if state == Some(false) {
                                        o_state[1] |= 0b00010000;
                                    }
                                }
                                common::Channel::Ch4 => {
                                    o_state[1] &= 0b00111111;
                                    if state == Some(true) {
                                        o_state[1] |= 0b01000000;
                                    } else if state == Some(false) {
                                        o_state[1] |= 0b10000000;
                                    }
                                }
                                common::Channel::Main => {
                                    o_state[0] &= 0b11110011;
                                    if state == Some(true) {
                                        o_state[0] |= 0b00000100;
                                    } else if state == Some(false) {
                                        o_state[0] |= 0b00001000;
                                    } else {
                                        rprintln!("Main channel disabled?");
                                    }
                                }
                            }

                            i2c.write(0x20, &[0x02, o_state[0]]).unwrap();
                            i2c.write(0x20, &[0x03, o_state[1]]).unwrap();
                        }
                    }
                } else {
                    rprintln!("Failed decoding: {:?}", buf);
                }
            }
            Err(usb_device::UsbError::WouldBlock) => (),
            Err(e) => rprintln!("USB read error: {:?}", e),
        }

        if let Some(msg) = queued_message {
            match usb_class.write(msg) {
                Ok(_) => queued_message = None,
                Err(usb_device::UsbError::WouldBlock) => continue,
                Err(e) => rprintln!("USB write error: {:?}", e),
            }
        }

        if pca_int.is_low().unwrap() {
            let mut i_state = [0x00, 0x00];
            i2c.write_read(0x20, &[0x00], &mut i_state[0..1]).unwrap();
            i2c.write_read(0x20, &[0x01], &mut i_state[1..2]).unwrap();

            if let Some(btn) = match (i_state[0] & 0b10010010, i_state[1] & 0b00100100) {
                (0b00010010, 0b00100100) => Some(common::Channel::Ch2),
                (0b10000010, 0b00100100) => Some(common::Channel::Ch1),
                (0b10010000, 0b00100100) => Some(common::Channel::Main),
                (0b10010010, 0b00000100) => Some(common::Channel::Ch4),
                (0b10010010, 0b00100000) => Some(common::Channel::Ch3),
                (0b10010010, 0b00100100) => None,
                _ => {
                rprintln!(
                    "Got invalid button state: {:08b} {:08b}",
                    i_state[0] & 0b10010010,
                    i_state[1] & 0b00100100
                );
                None
                },
            } {
                let msg = common::DeviceMessage::ToggleChannelMute(btn);
                rprintln!("{:?}", msg);
                let bytes = postcard::to_slice(&msg, &mut message_buf).unwrap();
                queued_message = Some(bytes);
                continue;
            }
        }

        let raw_values: [(common::Channel, u16); 5] = [
            (
                common::Channel::Ch1,
                adc1.read(&mut fader_ch1_adc).expect("Error reading ADC."),
            ),
            (
                common::Channel::Ch2,
                adc1.read(&mut fader_ch2_adc).expect("Error reading ADC."),
            ),
            (
                common::Channel::Ch3,
                adc1.read(&mut fader_ch3_adc).expect("Error reading ADC."),
            ),
            (
                common::Channel::Ch4,
                adc1.read(&mut fader_ch4_adc).expect("Error reading ADC."),
            ),
            (
                common::Channel::Main,
                adc1.read(&mut fader_main_adc).expect("Error reading ADC."),
            ),
        ];

        for ((ch, raw), previous) in raw_values.iter().zip(previous_fader_values.iter_mut()) {
            let fader = ((*raw as f32).clamp(8.0, 3608.0) - 8.0) / 3600.0;
            if (*previous - fader).abs() > 0.01 {
                let msg = common::DeviceMessage::UpdateVolume(*ch, fader);
                let bytes = postcard::to_slice(&msg, &mut message_buf).unwrap();
                *previous = fader;

                queued_message = Some(bytes);
            }
        }
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
