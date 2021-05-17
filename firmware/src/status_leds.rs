#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Green,
    Red,
    Off,
}

pub struct ChannelStatusLeds<S, L1, L2> {
    pub sync_led: S,
    pub button_led1: L1,
    pub button_led2: L2,
}

impl<S, L1, L2, E> ChannelStatusLeds<S, L1, L2>
where
    S: embedded_hal::digital::v2::OutputPin,
    L1: embedded_hal::digital::v2::OutputPin<Error = E>,
    L2: embedded_hal::digital::v2::OutputPin<Error = E>,
{
    // TODO: Do we even need the sync_leds anymore?
    #[allow(dead_code)]
    pub fn set_sync(&mut self, state: bool) -> Result<(), S::Error> {
        if state {
            self.sync_led.set_low()
        } else {
            self.sync_led.set_high()
        }
    }

    pub fn set_button_led_state(&mut self, state: common::ChannelState) -> Result<(), E> {
        let led = match state {
            common::ChannelState::Inactive => Led::Off,
            common::ChannelState::Running => Led::Green,
            common::ChannelState::Muted => Led::Red,
        };
        self.set_button_led(led)
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
}
