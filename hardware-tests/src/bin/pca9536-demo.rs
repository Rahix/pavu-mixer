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

    let mut i2c = hal::i2c::I2c::new(dp.I2C1, pins, 100.khz(), clocks, &mut rcc.apb1);

    rprintln!("Initialization completed.");

    // All pins outputs
    i2c.write(0x41, &[0x03, 0x00]).unwrap();
    i2c.write(0x41, &[0x01, 0x0F]).unwrap();

    rprintln!("Initialized PCA.");

    let mut count = 0;
    loop {
        let pinvals = 1 << (count % 4);

        rprintln!("Setting {:04b}.", pinvals);
        i2c.write(0x41, &[0x01, !pinvals]).unwrap();

        delay.delay_ms(200u16);
        count += 1;
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
