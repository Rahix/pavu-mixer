#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use micromath::F32Ext as _;
use stm32f3xx_hal::{self as hal, pac, prelude::*};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let mut dp = pac::Peripherals::take().unwrap();
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

    let mut adc1 = hal::adc::Adc::adc1(
        dp.ADC1, // The ADC we are going to control
        // The following is only needed to make sure the clock signal for the ADC is set up
        // correctly.
        &mut dp.ADC1_2,
        &mut rcc.ahb,
        hal::adc::CkMode::default(),
        clocks,
    );

    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);
    let mut gpiof = dp.GPIOF.split(&mut rcc.ahb);

    // for the main fader
    let mut adc1_in1_pin = gpioa.pa0.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut adc1_in2_pin = gpioa.pa1.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut adc1_in3_pin = gpioa.pa2.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut adc1_in4_pin = gpioa.pa3.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);
    let mut adc1_in5_pin = gpiof.pf4.into_analog(&mut gpiof.moder, &mut gpiof.pupdr);

    // for the output
    let mut data = gpiob
        .pb15
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    let mut dclk = gpiob
        .pb13
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    let mut sclk = gpiob
        .pb12
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);

    loop {
        let raw_value: u16 = adc1.read(&mut adc1_in5_pin).expect("Error reading adc1.");

        let mut main = raw_value;
        if main > 4080 {
            main = 4080;
        }
        if main < 8 {
            main = 8;
        }
        main -= 8;
        main = (main as f32 / ((4080 - 8) as f32) * 20.5) as u16;

        rprintln!("{:4}: {:2}", raw_value, main);

        for i in 0..20 {
            if i < (20 - main) {
                data.set_high().unwrap();
            } else {
                data.set_low().unwrap();
            }

            dclk.set_high().unwrap();
            dclk.set_low().unwrap();
        }

        sclk.set_high().unwrap();
        sclk.set_low().unwrap();
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
