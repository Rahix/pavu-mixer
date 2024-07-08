#![no_std]

use embedded_hal::blocking::delay as hal_delay;
use embedded_hal::blocking::spi as hal_spi;
use embedded_hal::digital::v2 as hal_digital;

/// SPI mode to be used for this display.
pub const SPI_MODE: embedded_hal::spi::Mode = embedded_hal::spi::MODE_3;

#[derive(Debug, Clone, Copy)]
pub enum DisplayError {
    /// Error on the SPI bus
    BusError,
    /// Error while setting CS pin
    CsError,
    /// Error while setting DC pin
    DcError,
    /// Error while setting RST pin
    RstError,
}

/// Driver for the ST7789 controller and Waveshare display
pub struct WaveshareDisplay<SPI, CS, DC, RST> {
    bus: SPI,
    cs_pin: CS,
    dc_pin: DC,
    rst_pin: RST,
}

impl<SPI, CS, DC, RST> WaveshareDisplay<SPI, CS, DC, RST>
where
    SPI: hal_spi::Write<u8>,
    CS: hal_digital::OutputPin,
    DC: hal_digital::OutputPin,
    RST: hal_digital::OutputPin,
{
    pub fn new(bus: SPI, cs_pin: CS, dc_pin: DC, rst_pin: RST) -> Self {
        Self {
            bus,
            cs_pin,
            dc_pin,
            rst_pin,
        }
    }

    pub fn hard_reset(
        &mut self,
        delay: &mut impl hal_delay::DelayMs<u16>,
    ) -> Result<(), DisplayError> {
        delay.delay_ms(10);
        self.rst_pin.set_low().map_err(|_| DisplayError::RstError)?;
        delay.delay_ms(10);
        self.rst_pin
            .set_high()
            .map_err(|_| DisplayError::RstError)?;
        delay.delay_ms(10);
        Ok(())
    }

    pub fn initialize(
        &mut self,
        delay: &mut impl hal_delay::DelayMs<u16>,
    ) -> Result<(), DisplayError> {
        self.hard_reset(delay)?;

        self.cs_pin.set_low().map_err(|_| DisplayError::CsError)?;

        // Configure "MADCTL", which defines the pixel and color access order.
        self.send_command(DisplayCommand::MADCTL, &[0x00])?;
        // Waveshare Driver: 0x00 = (none)

        // Configure 16-bit pixel format.
        self.send_command(DisplayCommand::COLMOD, &[0x55])?;
        // Waveshare Driver: 0x05

        // Configure porch settings.
        self.send_command(DisplayCommand::PORCTRL, &[0x0C, 0x0C, 0x00, 0x33, 0x33])?;

        // Configigure VGH = 13.26 V; VGL = -10.43 V
        self.send_command(DisplayCommand::GCTRL, &[0x35])?;

        // Configure VCOM = 0.725 V
        self.send_command(DisplayCommand::VCOMS, &[0x19])?;

        // Configure "LCM", this interacts with MADCTL somehow...
        self.send_command(DisplayCommand::LCMCTRL, &[lcmctrl::XBGR | lcmctrl::XMY])?;
        // Waveshare Driver: 0x2C = lcmctrl::XBGR | lcmctrl::XMX | lcmctrl::XMV

        // VAP = 4.45; VAN = -4.45; VDV = 0V
        self.send_command(DisplayCommand::VDVVRHEN, &[0x01])?;
        self.send_command(DisplayCommand::VRHS, &[0x12])?;
        self.send_command(DisplayCommand::VDVS, &[0x20])?;

        // 60 Hz Framerate
        self.send_command(DisplayCommand::FRCTRL2, &[0x0F])?;

        // Power Control: AVDD = 6.8 V, AVCL = -4.8 V, VDS = 2.3 V
        self.send_command(DisplayCommand::PWCTRL1, &[0xA4, 0xA1])?;

        // Write positive and negative gamma voltage control values.
        self.send_command(
            DisplayCommand::PVGAMCTRL,
            &[
                0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54, 0x4C, 0x18, 0x0D, 0x0B, 0x1F, 0x23,
            ],
        )?;
        self.send_command(
            DisplayCommand::NVGAMCTRL,
            &[
                0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44, 0x51, 0x2F, 0x1F, 0x1F, 0x20, 0x23,
            ],
        )?;

        // Invert Display.
        self.send_command(DisplayCommand::INVON, &[])?;

        // Exit Sleep.
        self.send_command(DisplayCommand::SLPOUT, &[])?;

        // Turn the display on.
        self.send_command(DisplayCommand::DISPON, &[])?;
        Ok(())
    }

    pub fn write_fb_partial(
        &mut self,
        xstart: u16,
        ystart: u16,
        xend: u16,
        yend: u16,
        fb: &[u8],
    ) -> Result<(), DisplayError> {
        let mut param_buffer = [0u8; 4];

        let colstart = xstart;
        let colend = xend;
        param_buffer[0..2].copy_from_slice(&colstart.to_be_bytes());
        param_buffer[2..4].copy_from_slice(&colend.to_be_bytes());
        self.send_command(DisplayCommand::CASET, &param_buffer)?;

        // TODO: Where does this offset come from?
        let rowstart = ystart + 80;
        let rowend = yend + 80;
        param_buffer[0..2].copy_from_slice(&rowstart.to_be_bytes());
        param_buffer[2..4].copy_from_slice(&rowend.to_be_bytes());
        self.send_command(DisplayCommand::RASET, &param_buffer)?;

        self.send_command(DisplayCommand::RAMWR, fb)
    }

    pub fn write_fb(&mut self, fb: &[u8]) -> Result<(), DisplayError> {
        self.write_fb_partial(0, 0, 239, 239, fb)
    }

    pub(crate) fn send_command(
        &mut self,
        command: DisplayCommand,
        args: &[u8],
    ) -> Result<(), DisplayError> {
        self.dc_pin.set_low().map_err(|_| DisplayError::DcError)?;
        self.bus
            .write(&[command as u8])
            .map_err(|_| DisplayError::BusError)?;
        self.dc_pin.set_high().map_err(|_| DisplayError::DcError)?;
        self.bus.write(args).map_err(|_| DisplayError::BusError)?;
        Ok(())
    }

    pub fn clear_screen(&mut self) -> Result<(), DisplayError> {
        const ROWSATONCE: usize = 4;
        let clearbuf = [0x00; 240 * 2 * ROWSATONCE];
        for row in (0..(240 / ROWSATONCE)).map(|r| r * ROWSATONCE) {
            self.write_fb_partial(0, row as u16, 239, (row + ROWSATONCE - 1) as u16, &clearbuf)?;
        }
        Ok(())
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayCommand {
    /// Sleep Out
    SLPOUT = 0x11,
    /// Display Inversion On
    INVON = 0x21,
    /// Display On
    DISPON = 0x29,
    /// Column Address Set
    CASET = 0x2A,
    /// Row Address Set
    RASET = 0x2B,
    /// Memory Write
    RAMWR = 0x2C,
    /// Memory Data Access Control
    MADCTL = 0x36,
    /// Interface Pixel Format
    COLMOD = 0x3A,
    /// Porch Setting
    PORCTRL = 0xB2,
    /// Gate Control
    GCTRL = 0xB7,
    /// VCOM Setting
    VCOMS = 0xBB,
    /// LCM Control
    LCMCTRL = 0xC0,
    /// VDV and VRH Command Enable
    VDVVRHEN = 0xC2,
    /// VRH Set
    VRHS = 0xC3,
    /// VDV Set
    VDVS = 0xC4,
    /// Frame Rate Control in Normal Mode
    FRCTRL2 = 0xC6,
    /// Power Control 1
    PWCTRL1 = 0xD0,
    /// Positive Voltage Gamma Control
    PVGAMCTRL = 0xE0,
    /// Negative Voltage Gamma Control
    NVGAMCTRL = 0xE1,
}

/// Bit-Flags for the MADCTL (Memory Data Access Control) parameter
#[allow(dead_code)]
pub(crate) mod madctl {
    pub(crate) const MY: u8 = 0x80;
    pub(crate) const MX: u8 = 0x40;
    pub(crate) const MV: u8 = 0x20;
    pub(crate) const ML: u8 = 0x10;
    pub(crate) const RGB: u8 = 0x08;
    pub(crate) const MH: u8 = 0x04;
}

/// Bit-Flags for the LCMCTRL (LCM Control) parameter
#[allow(dead_code)]
pub(crate) mod lcmctrl {
    pub(crate) const XMY: u8 = 0x40;
    pub(crate) const XBGR: u8 = 0x20;
    pub(crate) const XINV: u8 = 0x10;
    pub(crate) const XMX: u8 = 0x08;
    pub(crate) const XMH: u8 = 0x04;
    pub(crate) const XMV: u8 = 0x02;
    pub(crate) const XGS: u8 = 0x01;
}
