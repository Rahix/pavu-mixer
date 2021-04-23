use stm32f3xx_hal::{gpio, prelude::*};

pub struct MainLevelShiftReg {
    pub data_pin: gpio::gpiob::PB15<gpio::Output<gpio::PushPull>>,
    pub data_clock: gpio::gpiob::PB13<gpio::Output<gpio::PushPull>>,
    pub storage_clock: gpio::gpiob::PB12<gpio::Output<gpio::PushPull>>,
}

impl MainLevelShiftReg {
    /// Write out a main channel level to the indicator
    pub fn write_level(&mut self, level: f32) {
        let value = (level * 20.5) as u32;

        for i in 0..20 {
            if (19 - i) <= value {
                self.data_pin.set_low().unwrap();
            } else {
                self.data_pin.set_high().unwrap();
            }

            self.data_clock.set_high().unwrap();
            self.data_clock.set_low().unwrap();
        }

        self.storage_clock.set_high().unwrap();
        self.storage_clock.set_low().unwrap();
    }
}
