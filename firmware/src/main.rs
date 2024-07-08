#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use stm32f3xx_hal::{self as hal, pac, prelude::*};

use core::cell::{Cell, RefCell};

mod display;
mod faders;
mod level;
mod mute;
mod status_leds;
mod usb;

trait ResultWarn {
    fn err_warn(self, msg: &str);
}

impl<T, E> ResultWarn for Result<T, E> {
    fn err_warn(self, msg: &str) {
        match self {
            Ok(_) => (),
            Err(_) => {
                rprintln!("Error: {}", msg);
            }
        }
    }
}

fn get_device_serial(buf: &mut [u8; 16]) -> &str {
    use numtoa::NumToA;

    // SAFETY: Read-only device identifiers
    let coords = unsafe { core::ptr::read_volatile(0x1FFF_F7AC as *const u32) };
    let lotwafer = unsafe { core::ptr::read_volatile(0x1FFF_F7B0 as *const u32) };

    let serial = lotwafer.wrapping_add(coords);
    serial.numtoa_str(16, buf)
}

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();

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

    let mut delay = stm32f3xx_hal::delay::Delay::new(cp.SYST, clocks);

    rprintln!("Hello World!");

    let mut buf = [0; 16];
    let serial = get_device_serial(&mut buf);
    rprintln!("Device Serial: {}", serial);
    rprintln!("");

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
     */

    let backlight = gpiob
        .pb0
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);

    let dc = gpioa
        .pa8
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    let rst = gpioa
        .pa9
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    let cs = gpioa
        .pa4
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);

    let sck = gpioa
        .pa5
        .into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);
    let miso = gpioa
        .pa6
        .into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);
    let mosi = gpioa
        .pa7
        .into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);

    let spi = stm32f3xx_hal::spi::Spi::new(
        dp.SPI1,
        (sck, miso, mosi),
        hal::spi::config::Config::default()
            .frequency(16.MHz())
            .mode(waveshare_display::SPI_MODE),
        clocks,
        &mut rcc.apb2,
    );

    let mut display = waveshare_display::WaveshareDisplay::new(spi, cs, dc, rst);
    for _ in 0..6 {
        if let Err(e) = display.initialize(&mut delay) {
            rprintln!("Failed to initialize the display: {:?}", e);
        } else {
            break;
        }
    }

    let gui = crate::display::Gui::new(display, backlight);

    rprintln!("Display initialized.");

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

    /*
     * ADC initialization (faders)
     * ===========================
     */

    let adc_common = hal::adc::CommonAdc::new(dp.ADC1_2, &clocks, &mut rcc.ahb);
    let adc1 = hal::adc::Adc::new(
        dp.ADC1,
        hal::adc::config::Config::default(),
        &clocks,
        &adc_common,
    );

    let fader_ch1_adc = gpioa.pa0.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let fader_ch2_adc = gpioa.pa1.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let fader_ch3_adc = gpioa.pa2.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let fader_ch4_adc = gpioa.pa3.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let fader_main_adc = gpiof.pf4.into_analog(&mut gpiof.moder, &mut gpiof.pupdr);

    rprintln!("ADC initialized.");

    /*
     * PWM Channel initialization for channel level indicators
     * =======================================================
     */

    // deprecated function `stm32f3xx_hal::pwm::tim1`: needs refactoring and might violate safety
    // rules conflicting with the timer API
    #[allow(deprecated)]
    let tim1_channels = hal::pwm::tim1(dp.TIM1, 1280, 100.Hz(), &clocks);

    let pe9 = gpioe
        .pe9
        .into_af_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe11 = gpioe
        .pe11
        .into_af_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe13 = gpioe
        .pe13
        .into_af_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);
    let pe14 = gpioe
        .pe14
        .into_af_push_pull(&mut gpioe.moder, &mut gpioe.otyper, &mut gpioe.afrh);

    let ch1_level = level::PwmLevel::new(tim1_channels.0.output_to_pe9(pe9));
    let ch2_level = level::PwmLevel::new(tim1_channels.1.output_to_pe11(pe11));
    let ch3_level = level::PwmLevel::new(tim1_channels.2.output_to_pe13(pe13));
    let ch4_level = level::PwmLevel::new(tim1_channels.3.output_to_pe14(pe14));

    rprintln!("PWM initialized.");

    /*
     * I2C bus initialization
     * ======================
     */

    let mut scl =
        gpiob
            .pb6
            .into_af_open_drain(&mut gpiob.moder, &mut gpiob.otyper, &mut gpiob.afrl);
    let mut sda =
        gpiob
            .pb7
            .into_af_open_drain(&mut gpiob.moder, &mut gpiob.otyper, &mut gpiob.afrl);
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

    /*
     * I2C port-expanders for desync LEDs and mute-buttons
     * ===================================================
     */

    let mut pca9536 = port_expander::Pca9536::new(i2c.acquire_i2c());
    let pca9536_pins = pca9536.split();
    let mut pca9555 = port_expander::Pca9555::new(i2c.acquire_i2c(), false, false, false);
    let pca9555_pins = pca9555.split();

    let mut status_leds_main = status_leds::ChannelStatusLeds {
        sync_led: pca9555_pins.io0_0.into_output().unwrap(),
        button_led1: pca9555_pins.io0_2.into_output().unwrap(),
        button_led2: pca9555_pins.io0_3.into_output().unwrap(),
    };
    let mut status_leds_ch1 = status_leds::ChannelStatusLeds {
        sync_led: pca9536_pins.io0.into_output().unwrap(),
        button_led1: pca9555_pins.io0_5.into_output().unwrap(),
        button_led2: pca9555_pins.io0_6.into_output().unwrap(),
    };
    let mut status_leds_ch2 = status_leds::ChannelStatusLeds {
        sync_led: pca9536_pins.io1.into_output().unwrap(),
        button_led1: pca9555_pins.io1_0.into_output().unwrap(),
        button_led2: pca9555_pins.io1_1.into_output().unwrap(),
    };
    let mut status_leds_ch3 = status_leds::ChannelStatusLeds {
        sync_led: pca9536_pins.io2.into_output().unwrap(),
        button_led1: pca9555_pins.io1_3.into_output().unwrap(),
        button_led2: pca9555_pins.io1_4.into_output().unwrap(),
    };
    let mut status_leds_ch4 = status_leds::ChannelStatusLeds {
        sync_led: pca9536_pins.io3.into_output().unwrap(),
        button_led1: pca9555_pins.io1_6.into_output().unwrap(),
        button_led2: pca9555_pins.io1_7.into_output().unwrap(),
    };

    status_leds_main
        .set_sync(false)
        .err_warn("Failed setting LEDs");
    status_leds_ch1
        .set_sync(false)
        .err_warn("Failed setting LEDs");
    status_leds_ch2
        .set_sync(false)
        .err_warn("Failed setting LEDs");
    status_leds_ch3
        .set_sync(false)
        .err_warn("Failed setting LEDs");
    status_leds_ch4
        .set_sync(false)
        .err_warn("Failed setting LEDs");

    let mute_main = pca9555_pins.io0_1;
    let mute_ch1 = pca9555_pins.io0_4;
    let mute_ch2 = pca9555_pins.io0_7;
    let mute_ch3 = pca9555_pins.io1_2;
    let mute_ch4 = pca9555_pins.io1_5;

    // Read inputs once to clear interrupt
    port_expander::read_multiple([&mute_main, &mute_ch1, &mute_ch2, &mute_ch3, &mute_ch4])
        .err_warn("Failed reading buttons");

    // Set all outputs appropriately
    status_leds_main
        .set_button_led(status_leds::Led::Green)
        .err_warn("Failed setting LEDs");
    status_leds_ch1
        .set_button_led(status_leds::Led::Off)
        .err_warn("Failed setting LEDs");
    status_leds_ch2
        .set_button_led(status_leds::Led::Off)
        .err_warn("Failed setting LEDs");
    status_leds_ch3
        .set_button_led(status_leds::Led::Off)
        .err_warn("Failed setting LEDs");
    status_leds_ch4
        .set_button_led(status_leds::Led::Off)
        .err_warn("Failed setting LEDs");

    if pca_int.is_low().unwrap() {
        rprintln!("PCA interrupt is asserted when it should not be!");
    }

    rprintln!("PCA9536 & PCA9555 initialized.");

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
        pin_dm: gpioa.pa11.into_af_push_pull(
            &mut gpioa.moder,
            &mut gpioa.otyper,
            &mut gpioa.afrh,
        ),
        pin_dp: usb_dp.into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrh),
    };
    let usb_bus = hal::usb::UsbBus::new(usb);

    let usb_class = RefCell::new(usb::PavuMixerClass::new(&usb_bus));

    let mut usb_dev = usb_device::prelude::UsbDeviceBuilder::new(
        &usb_bus,
        // random VID:PID.....
        usb_device::prelude::UsbVidPid(0xde5f, 0x3d20),
    )
    // General Information
    .manufacturer("Rahix")
    .product("Pavu Mixer")
    .device_release(0x0010)
    .serial_number(serial)
    // Power
    .self_powered(false)
    .max_power(400)
    // Device Class
    .device_class(0xff)
    .build();

    rprintln!("USB device initialized.");

    let pending_volume_updates =
        RefCell::new(heapless::LinearMap::<common::Channel, f32, 5>::new());
    let pending_presses = RefCell::new(heapless::LinearMap::<common::Channel, (), 5>::new());
    let pending_forced_update = Cell::new(false);

    rprintln!("Ready.");
    rprintln!("");

    // Update main level to full to indicate that we are ready.
    main_level.update_level(1.0);

    let usb_recv_task = usb::usb_recv_task(
        &mut usb_dev,
        &usb_class,
        main_level,
        status_leds_main,
        ch1_level,
        status_leds_ch1,
        ch2_level,
        status_leds_ch2,
        ch3_level,
        status_leds_ch3,
        ch4_level,
        status_leds_ch4,
        gui,
        &pending_forced_update,
    );
    futures_util::pin_mut!(usb_recv_task);

    let usb_send_task = usb::usb_send_task(&usb_class, &pending_volume_updates, &pending_presses);
    futures_util::pin_mut!(usb_send_task);

    let mute_buttons_task = mute::mute_buttons_task(
        pca_int,
        mute_main,
        mute_ch1,
        mute_ch2,
        mute_ch3,
        mute_ch4,
        &pending_presses,
    );
    futures_util::pin_mut!(mute_buttons_task);

    let faders_task = faders::faders_task(
        adc1,
        fader_main_adc,
        fader_ch1_adc,
        fader_ch2_adc,
        fader_ch3_adc,
        fader_ch4_adc,
        &pending_volume_updates,
        &pending_forced_update,
    );
    futures_util::pin_mut!(faders_task);

    let all_tasks = async {
        // join!() all tasks to poll them one after the other indefinitely.
        futures_util::join!(usb_recv_task, usb_send_task, mute_buttons_task, faders_task);
    };
    futures_util::pin_mut!(all_tasks);

    let c = cassette::Cassette::new(all_tasks);
    c.block_on();
    unreachable!();
}

#[cortex_m_rt::exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
