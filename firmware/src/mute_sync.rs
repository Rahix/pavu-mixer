#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Green,
    Red,
    Off,
}

impl Led {
    pub fn from_state(state: Option<bool>) -> Led {
        match state {
            Some(true) => Led::Green,
            Some(false) => Led::Red,
            None => Led::Off,
        }
    }
}

pub struct ChannelMuteSync<S, L1, L2, B> {
    pub sync_led: S,
    pub button_led1: L1,
    pub button_led2: L2,
    pub button: B,
}

impl<S, L1, L2, B, E> ChannelMuteSync<S, L1, L2, B>
where
    S: embedded_hal::digital::v2::OutputPin,
    L1: embedded_hal::digital::v2::OutputPin<Error = E>,
    L2: embedded_hal::digital::v2::OutputPin<Error = E>,
    B: embedded_hal::digital::v2::InputPin,
{
    pub fn set_sync(&mut self, state: bool) -> Result<(), S::Error> {
        if state {
            self.sync_led.set_low()
        } else {
            self.sync_led.set_high()
        }
    }

    pub fn set_button_led(&mut self, state: Led) -> Result<(), E> {
        match state {
            Led::Green => {
                self.button_led1.set_high()?;
                self.button_led2.set_low()?;
            }
            Led::Red => {
                self.button_led1.set_low()?;
                self.button_led2.set_high()?;
            }
            Led::Off => {
                self.button_led1.set_high()?;
                self.button_led2.set_high()?;
            }
        }
        Ok(())
    }

    pub fn read_button_state(&self) -> Result<bool, B::Error> {
        self.button.is_low()
    }
}
