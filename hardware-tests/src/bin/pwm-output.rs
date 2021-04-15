#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use micromath::F32Ext as _;
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

    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);

    let pa7 = gpioa.pa7.into_af2(&mut gpioa.moder, &mut gpioa.afrl);

    let tim3_channels = hal::pwm::tim3(dp.TIM3, 1280, 100.hz(), &clocks);
    let mut tim3_ch2 = tim3_channels.1.output_to_pa7(pa7);
    tim3_ch2.enable();

    let mut frame = 0u64;
    loop {
        let wavy = (frame as f32 / 6.0).sin() * 0.5 + 0.5;
        let brightness_corrected = wavy.powf(2.8);
        let value = (brightness_corrected * (tim3_ch2.get_max_duty() as f32 + 0.5)) as u16;

        rprintln!("{:6} - {:2}", frame, value);

        tim3_ch2.set_duty(value);

        delay.delay_ms(10u16);
        frame += 1;
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
