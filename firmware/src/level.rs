use micromath::F32Ext;
use stm32f3xx_hal::{gpio, prelude::*};

pub struct ShiftRegLevel {
    pub data_pin: gpio::gpiob::PB15<gpio::Output<gpio::PushPull>>,
    pub data_clock: gpio::gpiob::PB13<gpio::Output<gpio::PushPull>>,
    pub storage_clock: gpio::gpiob::PB12<gpio::Output<gpio::PushPull>>,
}

impl ShiftRegLevel {
    pub fn update_level(&mut self, level: f32) {
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

pub struct PwmLevel<T> {
    pwm_pin: T,
}

impl<T: embedded_hal::PwmPin<Duty = u16>> PwmLevel<T> {
    pub fn new(pwm_pin: T) -> Self {
        Self { pwm_pin }
    }

    pub fn update_level(&mut self, level: f32) {
        if level > 0.01 {
            self.pwm_pin.enable();
            self.pwm_pin
                .set_duty((self.pwm_pin.get_max_duty() as f32 * (1.0 - level.powf(2.8))) as u16);
        } else {
            self.pwm_pin.disable();
        }
    }
}
