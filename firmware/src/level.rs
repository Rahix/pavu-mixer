use micromath::F32Ext;

/// Level indicator built from a shift-register chain
pub struct ShiftRegLevel<D, DCK, SCK> {
    pub data_pin: D,
    pub data_clock: DCK,
    pub storage_clock: SCK,
}

impl<D, DCK, SCK> ShiftRegLevel<D, DCK, SCK>
where
    D: embedded_hal::digital::v2::OutputPin,
    DCK: embedded_hal::digital::v2::OutputPin,
    SCK: embedded_hal::digital::v2::OutputPin,
{
    #[allow(unused_must_use)]
    pub fn update_level(&mut self, level: f32) {
        let value = (level * 20.5) as u32;

        for i in 0..20 {
            if (19 - i) < value {
                self.data_pin.set_low();
            } else {
                self.data_pin.set_high();
            }

            self.data_clock.set_high();
            self.data_clock.set_low();
        }

        self.storage_clock.set_high();
        self.storage_clock.set_low();
    }
}

/// Level indicator built from a PWM pin
pub struct PwmLevel<T> {
    pwm_pin: T,
}

impl<T> PwmLevel<T>
where
    T: embedded_hal::PwmPin<Duty = u16>,
{
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
