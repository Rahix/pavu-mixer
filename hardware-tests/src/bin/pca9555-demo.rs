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

    let pca_int = gpiob
        .pb0
        .into_floating_input(&mut gpiob.moder, &mut gpiob.pupdr);

    let mut i2c = hal::i2c::I2c::new(dp.I2C1, pins, 100.khz(), clocks, &mut rcc.apb1);

    rprintln!("Initialization completed.");

    // Configure IO0
    //  0: DESYNC_MAIN      = OUTPUT
    //  1: MUTE_MAIN_BTN    = INPUT
    //  2: MUTE_MAIN_LED1   = OUTPUT
    //  3: MUTE_MAIN_LED2   = OUTPUT
    //  4: MUTE1_BTN        = INPUT
    //  5: MUTE1_LED1       = OUTPUT
    //  6: MUTE1_LED2       = OUTPUT
    //  7: MUTE2_BTN        = INPUT
    i2c.write(0x20, &[0x06, 0b10010010]).unwrap();

    // Configure IO1
    //  0: MUTE2_LED1       = OUTPUT
    //  1: MUTE2_LED2       = OUTPUT
    //  2: MUTE3_BTN        = INPUT
    //  3: MUTE3_LED1       = OUTPUT
    //  4: MUTE3_LED2       = OUTPUT
    //  5: MUTE4_BTN        = INPUT
    //  6: MUTE4_LED1       = OUTPUT
    //  7: MUTE4_LED2       = OUTPUT
    i2c.write(0x20, &[0x07, 0b00100100]).unwrap();

    rprintln!("Initialized PCA.");

    // Read inputs once to clear interrupt
    let mut buf = [0x00];
    i2c.write_read(0x20, &[0x00], &mut buf).unwrap();
    rprintln!("Reading 0: {:08b}", buf[0]);
    i2c.write_read(0x20, &[0x01], &mut buf).unwrap();
    rprintln!("Reading 1: {:08b}", buf[0]);

    // Make all button LEDs green
    i2c.write(0x20, &[0x02, 0b00100101]).unwrap();
    i2c.write(0x20, &[0x03, 0b01001001]).unwrap();

    rprintln!("Ready for the action!");

    loop {
        rprintln!("Waiting for interrupt...");
        while pca_int.is_high().unwrap() {}

        // find out which input caused the interrupt

        let mut buf = [0x00, 0x00];
        i2c.write_read(0x20, &[0x00], &mut buf[0..1]).unwrap();
        i2c.write_read(0x20, &[0x01], &mut buf[1..2]).unwrap();
        let btn = match (buf[0] & 0b10010010, buf[1] & 0b00100100) {
            (0b00010010, 0b00100100) => Some(2),
            (0b10000010, 0b00100100) => Some(1),
            (0b10010000, 0b00100100) => Some(0),
            (0b10010010, 0b00000100) => Some(4),
            (0b10010010, 0b00100000) => Some(3),
            _ => {
                rprintln!(
                    "Got invalid button state: {:08b} {:08b}",
                    buf[0] & 0b10010010,
                    buf[1] & 0b00100100
                );
                None
            }
        };
        if let Some(btn) = btn {
            rprintln!("Got button: {}", btn);

            let mut buf = [0x00, 0x00];
            i2c.write_read(0x20, &[0x02], &mut buf[0..1]).unwrap();
            i2c.write_read(0x20, &[0x03], &mut buf[1..2]).unwrap();

            match btn {
                0 => {
                    buf[0] &= 0b11110011;
                    buf[0] |= 0b00001000;
                }
                1 => {
                    buf[0] &= 0b10011111;
                    buf[0] |= 0b01000000;
                }
                2 => {
                    buf[1] &= 0b11111100;
                    buf[1] |= 0b00000010;
                }
                3 => {
                    buf[1] &= 0b11100111;
                    buf[1] |= 0b00010000;
                }
                4 => {
                    buf[1] &= 0b00111111;
                    buf[1] |= 0b10000000;
                }
                _ => unreachable!(),
            }

            i2c.write(0x20, &[0x02, buf[0]]).unwrap();
            i2c.write(0x20, &[0x03, buf[1]]).unwrap();

            for _ in 0..10 {
                buf[0] &= 0b11111110;
                i2c.write(0x20, &[0x02, buf[0]]).unwrap();
                delay.delay_ms(50u16);
                buf[0] |= 0b00000001;
                i2c.write(0x20, &[0x02, buf[0]]).unwrap();
                delay.delay_ms(50u16);
            }
        }

        // Make all button LEDs green again
        i2c.write(0x20, &[0x02, 0b00100101]).unwrap();
        i2c.write(0x20, &[0x03, 0b01001001]).unwrap();
    }
}

#[cortex_m_rt::exception]
fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("Hard Fault: {:#?}", ef);
}
