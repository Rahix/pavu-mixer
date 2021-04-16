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

    rprintln!("Hello world!");

    loop {
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
