#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rprintln;

use stm32f3xx_hal::prelude::*;

const IMAGE: &[u8] = &*include_bytes!("demo-image.raw");

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_target::rtt_init_print!();

    let dp = stm32f3xx_hal::stm32::Peripherals::take().unwrap();
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
    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);

    let dc = gpioa
        .pa8
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    let rst = gpioa
        .pa9
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    let cs = gpioa
        .pa4
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);

    let sck = gpioa.pa5.into_af5(&mut gpioa.moder, &mut gpioa.afrl);
    let miso = gpioa.pa6.into_af5(&mut gpioa.moder, &mut gpioa.afrl);
    let mosi = gpioa.pa7.into_af5(&mut gpioa.moder, &mut gpioa.afrl);

    let mut backlight = gpiob
        .pb0
        .into_push_pull_output(&mut gpiob.moder, &mut gpiob.otyper);
    backlight.set_high().unwrap();

    let spi = stm32f3xx_hal::spi::Spi::spi1(
        dp.SPI1,
        (sck, miso, mosi),
        waveshare_display::SPI_MODE,
        16u32.mhz(),
        clocks,
        &mut rcc.apb2,
    );

    rprintln!("Hello World!");

    let mut display = waveshare_display::WaveshareDisplay::new(spi, cs, dc, rst);
    display.initialize(&mut delay).unwrap();

    rprintln!("Done initializing.");

    let clearbuf = [0x00; 240 * 2];
    for row in 0..240 {
        display.write_fb_partial(0, row, 239, row, &clearbuf).unwrap();
    }

    rprintln!("Cleared!");

    for row in 0..240 {
        let mut rowbuf = [0u8; 240 * 2];
        for col in 0..240 {
            rowbuf[col * 2] = row as u8 >> 3 << 3;
            rowbuf[col * 2 + 1] = col as u8 >> 3;
        }
        display.write_fb_partial(0, row, 239, row, &rowbuf).unwrap();
    }

    display.write_fb_partial(0, 0, 0, 0, &[0xFF, 0xFF]).unwrap();

    display.write_fb(IMAGE).unwrap();

    rprintln!("Drawn!");

    loop { }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
