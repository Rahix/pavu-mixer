#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use stm32f3xx_hal::{self as hal, pac, prelude::*};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8u32.mhz())
        .sysclk(48u32.mhz())
        .pclk1(24u32.mhz())
        .freeze(&mut flash.acr);

    let mut delay = stm32f3xx_hal::delay::Delay::new(cp.SYST, clocks);

    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);
    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);

    let pins = (
        gpiob.pb6.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SCL
        gpiob.pb7.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SDA
    );

    let pca_int = gpioa
        .pa10
        .into_floating_input(&mut gpioa.moder, &mut gpioa.pupdr);

    let i2c = hal::i2c::I2c::new(dp.I2C1, pins, 100.khz(), clocks, &mut rcc.apb1);

    let mut pca = port_expander::Pca9555::new(i2c, false, false, false);
    let pins = pca.split();

    rprintln!("Initialization completed.");

    let mut desync_main = pins.io0_0.into_output().unwrap();
    let mut mute_main_btn = pins.io0_1;
    let mut mute_main_led1 = pins.io0_2.into_output().unwrap();
    let mut mute_main_led2 = pins.io0_3.into_output().unwrap();
    let mut mute1_btn = pins.io0_4;
    let mut mute1_led1 = pins.io0_5.into_output().unwrap();
    let mut mute1_led2 = pins.io0_6.into_output().unwrap();
    let mut mute2_btn = pins.io0_7;
    let mut mute2_led1 = pins.io1_0.into_output().unwrap();
    let mut mute2_led2 = pins.io1_1.into_output().unwrap();
    let mut mute3_btn = pins.io1_2;
    let mut mute3_led1 = pins.io1_3.into_output().unwrap();
    let mut mute3_led2 = pins.io1_4.into_output().unwrap();
    let mut mute4_btn = pins.io1_5;
    let mut mute4_led1 = pins.io1_6.into_output().unwrap();
    let mut mute4_led2 = pins.io1_7.into_output().unwrap();

    rprintln!("Initialized PCA9555.");

    // Read inputs once to clear interrupt
    for btn in [
        &mut mute_main_btn,
        &mut mute1_btn,
        &mut mute2_btn,
        &mut mute3_btn,
        &mut mute4_btn,
    ]
    .iter_mut()
    {
        btn.is_high().unwrap();
    }

    // Make all button LEDs green
    for (led1, led2) in [
        (&mut mute_main_led1, &mut mute_main_led2),
        (&mut mute1_led1, &mut mute1_led2),
        (&mut mute2_led1, &mut mute2_led2),
        (&mut mute3_led1, &mut mute3_led2),
        (&mut mute4_led1, &mut mute4_led2),
    ]
    .iter_mut()
    {
        led1.set_high().unwrap();
        led2.set_low().unwrap();
    }

    rprintln!("Ready for the action!");

    loop {
        rprintln!("Waiting for interrupt...");
        while pca_int.is_high().unwrap() {}

        // find out which input caused the interrupt

        let mut event = None;
        for (i, btn) in [
            &mut mute_main_btn,
            &mut mute1_btn,
            &mut mute2_btn,
            &mut mute3_btn,
            &mut mute4_btn,
        ]
        .iter_mut()
        .enumerate()
        {
            if btn.is_low().unwrap() {
                rprintln!("Button {} was pressed!", i);
                event = Some(i);
                break;
            }
        }
        if event.is_none() {
            rprintln!("Interrupt, but no button pressed?");
        }

        if let Some(btn) = event {
            let (ref mut led1, ref mut led2) = [
                (&mut mute_main_led1, &mut mute_main_led2),
                (&mut mute1_led1, &mut mute1_led2),
                (&mut mute2_led1, &mut mute2_led2),
                (&mut mute3_led1, &mut mute3_led2),
                (&mut mute4_led1, &mut mute4_led2),
            ][btn];

            led2.set_high().unwrap();
            led1.set_low().unwrap();

            for _ in 0..10 {
                desync_main.set_low().unwrap();
                delay.delay_ms(50u16);
                desync_main.set_high().unwrap();
                delay.delay_ms(50u16);
            }

            led1.set_high().unwrap();
            led2.set_low().unwrap();
        }
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
