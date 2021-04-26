#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use micromath::F32Ext;
use stm32f3xx_hal::{self as hal, pac, prelude::*};

mod level;
mod mute_sync;
mod usb;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let mut dp = pac::Peripherals::take().unwrap();
    let _cp = cortex_m::Peripherals::take().unwrap();

    /*
     * Clocks
     * ======
     */

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8u32.MHz())
        .sysclk(48u32.MHz())
        .pclk1(24u32.MHz())
        .freeze(&mut flash.acr);

    assert!(clocks.usbclk_valid());

    rprintln!("Hello World!");

    /*
     *
     * GPIO blocks
     * ===========
     */

    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);
    let mut gpioe = dp.GPIOE.split(&mut rcc.ahb);
    let mut gpiof = dp.GPIOF.split(&mut rcc.ahb);

    /*
     * Display
     * =======
     * TODO: The actual display code...
     */

    let mut backlight_gpio = gpiob
        .pb0
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    // Turn off backlight for now
    backlight_gpio.set_low().unwrap();

    /*
     * Main level indicator shift register
     * ===================================
     */

    let mut main_level = level::ShiftRegLevel {
        data_pin: gpiob
            .pb15
            .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper),
        data_clock: gpiob
            .pb13
            .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper),
        storage_clock: gpiob
            .pb12
            .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper),
    };

    rprintln!("ShiftRegs initialized.");
    main_level.update_level(1.0 / 5.0);

    /*
     * ADC initialization (faders)
     * ===========================
     */

    let mut adc1 = hal::adc::Adc::adc1(
        dp.ADC1,
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
    main_level.update_level(2.0 / 5.0);

    /*
     * PWM Channel initialization for channel level indicators
     * =======================================================
     */

    let tim1_channels = hal::pwm::tim1(dp.TIM1, 1280, 100.Hz(), &clocks);

    let pe9 = gpioe
        .pe9
        .into_af2_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe11 = gpioe
        .pe11
        .into_af2_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe13 = gpioe
        .pe13
        .into_af2_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe14 = gpioe
        .pe14
        .into_af2_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let mut ch1_level = level::PwmLevel::new(tim1_channels.0.output_to_pe9(pe9));
    let mut ch2_level = level::PwmLevel::new(tim1_channels.1.output_to_pe11(pe11));
    let mut ch3_level = level::PwmLevel::new(tim1_channels.2.output_to_pe13(pe13));
    let mut ch4_level = level::PwmLevel::new(tim1_channels.3.output_to_pe14(pe14));

    rprintln!("PWM initialized.");
    main_level.update_level(3.0 / 5.0);

    /*
     * I2C bus initialization
     * ======================
     */

    let mut scl =
        gpiob
            .pb6
            .into_af4_open_drain(&mut gpiob.moder, &mut gpiob.otyper, &mut gpiob.afrl);
    let mut sda =
        gpiob
            .pb7
            .into_af4_open_drain(&mut gpiob.moder, &mut gpiob.otyper, &mut gpiob.afrl);
    scl.internal_pull_up(&mut gpiob.pupdr, true);
    sda.internal_pull_up(&mut gpiob.pupdr, true);

    let pca_int = gpioa
        .pa10
        .into_floating_input(&mut gpioa.moder, &mut gpioa.pupdr);

    let i2c = shared_bus::BusManagerSimple::new(hal::i2c::I2c::new(
        dp.I2C1,
        (scl, sda),
        100_000.Hz(),
        clocks,
        &mut rcc.apb1,
    ));

    rprintln!("I2C bus initialized.");

    let mut pca9536 = port_expander::Pca9536::new(i2c.acquire_i2c());
    let pca9536_pins = pca9536.split();
    let mut pca9555 = port_expander::Pca9555::new(i2c.acquire_i2c(), false, false, false);
    let pca9555_pins = pca9555.split();

    let mut mute_sync_main = mute_sync::ChannelMuteSync {
        sync_led: pca9555_pins.io0_0.into_output().unwrap(),
        button_led1: pca9555_pins.io0_2.into_output().unwrap(),
        button_led2: pca9555_pins.io0_3.into_output().unwrap(),
        button: pca9555_pins.io0_1,
    };
    let mut mute_sync_ch1 = mute_sync::ChannelMuteSync {
        sync_led: pca9536_pins.io0.into_output().unwrap(),
        button_led1: pca9555_pins.io0_5.into_output().unwrap(),
        button_led2: pca9555_pins.io0_6.into_output().unwrap(),
        button: pca9555_pins.io0_4,
    };
    let mut mute_sync_ch2 = mute_sync::ChannelMuteSync {
        sync_led: pca9536_pins.io1.into_output().unwrap(),
        button_led1: pca9555_pins.io1_0.into_output().unwrap(),
        button_led2: pca9555_pins.io1_1.into_output().unwrap(),
        button: pca9555_pins.io0_7,
    };
    let mut mute_sync_ch3 = mute_sync::ChannelMuteSync {
        sync_led: pca9536_pins.io2.into_output().unwrap(),
        button_led1: pca9555_pins.io1_3.into_output().unwrap(),
        button_led2: pca9555_pins.io1_4.into_output().unwrap(),
        button: pca9555_pins.io1_2,
    };
    let mut mute_sync_ch4 = mute_sync::ChannelMuteSync {
        sync_led: pca9536_pins.io3.into_output().unwrap(),
        button_led1: pca9555_pins.io1_6.into_output().unwrap(),
        button_led2: pca9555_pins.io1_7.into_output().unwrap(),
        button: pca9555_pins.io1_5,
    };

    // Read inputs once to clear interrupt
    mute_sync_main.read_button_state().unwrap();
    mute_sync_ch1.read_button_state().unwrap();
    mute_sync_ch2.read_button_state().unwrap();
    mute_sync_ch3.read_button_state().unwrap();
    mute_sync_ch4.read_button_state().unwrap();

    // Set all outputs appropriately
    mute_sync_main
        .set_button_led(mute_sync::Led::Green)
        .unwrap();
    mute_sync_ch1.set_button_led(mute_sync::Led::Off).unwrap();
    mute_sync_ch2.set_button_led(mute_sync::Led::Off).unwrap();
    mute_sync_ch3.set_button_led(mute_sync::Led::Off).unwrap();
    mute_sync_ch4.set_button_led(mute_sync::Led::Off).unwrap();

    if pca_int.is_low().unwrap() {
        rprintln!("PCA interrupt is asserted when it should not be!");
    }

    rprintln!("PCA9536 & PCA9555 initialized.");
    main_level.update_level(4.0 / 5.0);

    /*
     * USB FS
     * ======
     */

    /*
     * F3 Discovery board has a pull-up resistor on the D+ line.
     * Pull the D+ pin down to send a RESET condition to the USB bus.
     * This forced reset is needed only for development, without it host
     * will not reset your device when you upload new firmware.
     */
    let mut usb_dp = gpioa
        .pa12
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    usb_dp.set_low().ok();
    cortex_m::asm::delay(clocks.sysclk().0 / 100);

    let usb = hal::usb::Peripheral {
        usb: dp.USB,
        pin_dm: gpioa.pa11.into_af14_push_pull(
            &mut gpioa.moder,
            &mut gpioa.otyper,
            &mut gpioa.afrh,
        ),
        pin_dp: usb_dp.into_af14_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrh),
    };
    let usb_bus = hal::usb::UsbBus::new(usb);

    let mut usb_class = usb::PavuMixerClass::new(&usb_bus);

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
    main_level.update_level(5.0 / 5.0);

    rprintln!("Ready.");
    rprintln!("");

    let mut queued_message: Option<common::DeviceMessage> = None;
    loop {
        if !usb_dev.poll(&mut [&mut usb_class]) {
            continue;
        }

        match usb_class.recv_host_message() {
            Err(usb::Error::WouldBlock) => (),
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
                    common::Channel::Ch1 => {
                        mute_sync_ch1
                            .set_button_led(mute_sync::Led::from_state(state))
                            .unwrap();
                        if state.is_none() {
                            ch1_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch2 => {
                        mute_sync_ch2
                            .set_button_led(mute_sync::Led::from_state(state))
                            .unwrap();
                        if state.is_none() {
                            ch2_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch3 => {
                        mute_sync_ch3
                            .set_button_led(mute_sync::Led::from_state(state))
                            .unwrap();
                        if state.is_none() {
                            ch3_level.update_level(0.0);
                        }
                    }
                    common::Channel::Ch4 => {
                        mute_sync_ch4
                            .set_button_led(mute_sync::Led::from_state(state))
                            .unwrap();
                        if state.is_none() {
                            ch4_level.update_level(0.0);
                        }
                    }
                    common::Channel::Main => {
                        mute_sync_main
                            .set_button_led(mute_sync::Led::from_state(state))
                            .unwrap();
                    }
                },
            },
        }

        if let Some(msg) = queued_message {
            match usb_class.send_device_message(msg) {
                Ok(()) => queued_message = None,
                Err(usb::Error::WouldBlock) => continue,
                Err(e) => rprintln!("USB write error: {:?}", e),
            }
        }

        if pca_int.is_low().unwrap() {
            if let Some(btn) = if mute_sync_main.read_button_state().unwrap() {
                Some(common::Channel::Main)
            } else if mute_sync_ch1.read_button_state().unwrap() {
                Some(common::Channel::Ch1)
            } else if mute_sync_ch2.read_button_state().unwrap() {
                Some(common::Channel::Ch2)
            } else if mute_sync_ch3.read_button_state().unwrap() {
                Some(common::Channel::Ch3)
            } else if mute_sync_ch4.read_button_state().unwrap() {
                Some(common::Channel::Ch4)
            } else {
                None
            } {
                queued_message = Some(common::DeviceMessage::ToggleChannelMute(btn));
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
            let fader = ((*raw as f32).clamp(8.0, 3308.0) - 8.0) / 3300.0;
            if (*previous - fader).abs() > 0.01 {
                *previous = fader;
                queued_message = Some(common::DeviceMessage::UpdateVolume(*ch, fader));
            }
        }
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
