use embedded_hal::digital::v2::OutputPin;

struct ActiveIconStream {
    ch: common::Channel,
    cursor: usize,
}

pub struct Gui<SPI, CS, DC, RST, BL> {
    display: waveshare_display::WaveshareDisplay<SPI, CS, DC, RST>,
    backlight: BL,
    icon_buf: &'static mut [u8],
    active_icon_stream: Option<ActiveIconStream>,
}

impl<SPI, CS, DC, RST, BL> Gui<SPI, CS, DC, RST, BL>
where
    SPI: embedded_hal::blocking::spi::Write<u8>,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BL: OutputPin,
{
    pub fn new(
        display: waveshare_display::WaveshareDisplay<SPI, CS, DC, RST>,
        backlight: BL,
    ) -> Self {
        let icon_buf = cortex_m::singleton!(
            :[u8; common::ICON_SIZE * common::ICON_SIZE * 2]
                = [0x00; common::ICON_SIZE * common::ICON_SIZE * 2]
        )
        .unwrap();
        Self {
            display,
            backlight,
            icon_buf,
            active_icon_stream: None,
        }
    }

    pub fn suspend(&mut self) {
        let _ = self.backlight.set_low();
    }

    pub fn resume(&mut self) {
        let _ = self.display.clear_screen();
        let _ = self.backlight.set_high();
    }

    fn icon_coords(ch: common::Channel) -> (u16, u16, u16, u16) {
        let (x, y) = match ch {
            common::Channel::Ch1 => (10, 10),
            common::Channel::Ch2 => (10, 130),
            common::Channel::Ch3 => (130, 10),
            common::Channel::Ch4 => (130, 130),
            _ => unreachable!(),
        };
        (
            x,
            y,
            x + common::ICON_SIZE as u16 - 1,
            y + common::ICON_SIZE as u16 - 1,
        )
    }

    pub fn clear_icon(&mut self, ch: common::Channel) {
        const CLEARROWS: u16 = 4;
        let (x, y, _, _) = Self::icon_coords(ch);
        let clearbuf = [0x00; common::ICON_SIZE * 2 * CLEARROWS as usize];
        for off in 0..(common::ICON_SIZE as u16 / CLEARROWS) {
            let _ = self.display.write_fb_partial(
                x,
                y + off * CLEARROWS,
                x + common::ICON_SIZE as u16 - 1,
                y + (off + 1) * CLEARROWS - 1,
                &clearbuf[..],
            );
        }
    }

    pub fn start_icon_stream(&mut self, ch: common::Channel) {
        self.active_icon_stream = Some(ActiveIconStream { ch, cursor: 0 });
    }

    /// Call the closure to retrieve new icon data in case a stream is currently active.
    pub fn try_push_icon_data_if_active<F>(&mut self, f: F)
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if let Some(info) = &mut self.active_icon_stream {
            let new_bytes = f(&mut self.icon_buf[info.cursor..]);
            info.cursor += new_bytes;

            // Full icon received - write it to the display.
            if info.cursor >= self.icon_buf.len() {
                let (x1, y1, x2, y2) = Self::icon_coords(info.ch);
                let _ = self
                    .display
                    .write_fb_partial(x1, y1, x2, y2, &self.icon_buf);
                self.active_icon_stream = None;
            }
        }
    }
}
