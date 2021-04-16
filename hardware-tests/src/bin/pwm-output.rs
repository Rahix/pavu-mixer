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

    let mut gpioe = dp.GPIOE.split(&mut rcc.ahb);

    let pe9 = gpioe.pe9.into_af2(&mut gpioe.moder, &mut gpioe.afrh);
    let pe14 = gpioe.pe14.into_af2(&mut gpioe.moder, &mut gpioe.afrh);

    let tim1_channels = hal::pwm::tim1(dp.TIM1, 1280, 100.hz(), &clocks);
    let mut ch1 = tim1_channels.0.output_to_pe9(pe9);
    let mut ch2 = tim1_channels.3.output_to_pe14(pe14);
    ch1.enable();
    ch2.enable();

    let mut frame = 0u64;
    loop {
        let wavy = (frame as f32 / 6.0).sin() * 0.5 + 0.5;
        let brightness_corrected = wavy.powf(2.8);
        let value = (brightness_corrected * (ch1.get_max_duty() as f32 + 0.5)) as u16;

        rprintln!("{:6} - {:2}", frame, value);

        ch1.set_duty(value);

        let wavy = 1.0 - ((frame as f32 / 6.0).sin() * 0.5 + 0.5);
        let brightness_corrected = wavy.powf(2.8);
        let value = (brightness_corrected * (ch2.get_max_duty() as f32 + 0.5)) as u16;

        ch2.set_duty(value);

        delay.delay_ms(10u16);
        frame += 1;
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
