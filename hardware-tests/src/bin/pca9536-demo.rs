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

    let pins = (
        gpiob.pb6.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SCL
        gpiob.pb7.into_af4(&mut gpiob.moder, &mut gpiob.afrl), // SDA
    );

    let i2c = hal::i2c::I2c::new(dp.I2C1, pins, 100.khz(), clocks, &mut rcc.apb1);

    let mut pca = port_expander::Pca9536::new(i2c);
    let pins = pca.split();
    let mut pins = [
        pins.io0.into_output().unwrap(),
        pins.io1.into_output().unwrap(),
        pins.io2.into_output().unwrap(),
        pins.io3.into_output().unwrap(),
    ];

    rprintln!("Initialized PCA9536.");
    rprintln!("Initialization completed.");

    loop {
        for pin in pins.iter_mut() {
            pin.set_low().unwrap();
            delay.delay_ms(200u16);
            pin.set_high().unwrap();
        }
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
