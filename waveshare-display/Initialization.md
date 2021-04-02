Initialization Sequence
=======================
This is a documentation of the initialization sequence found in the official
example code.

#### `0x36` - `MADCTL`: Memory Data Access Control
Values: `0x00`

Bits:
- `7`: `MY` - Page Address Order
  - 0: Top to Bottom (**selected**)
  - 1: Bottom to Top
- `6`: `MX` - Column Address Order
  - 0: Left to Right (**selected**)
  - 1: Right to Left
- `5`: `MV` - Page/Column Order
  - 0: Normal (**selected**)
  - 1: Reverse Mode
- `4`: `ML` - Line Address Order
  - 0: LCD Refresh Top to Bottom (selected)
  - 1: LCD Refresh Bottom to Top
- `3`: `RGB` - BRGB/BGR Order
  - 0: RGB
  - 1: BGR
- `2`: `MH` - Display Data Latch Order
  - 0: LCD Refresh Left to Right
  - 1: LCD Refresh Right to Left

#### `0x3A` - `COLMOD`: Interface Pixel Format
Values: `0x05`

Bits:
- `6:4`: RGB interface color format
  - `101`: 65K of RGB interface
  - `110`: 262K of RGB interface
  - `0`: Undocumented value which is used here?
- `2:0`: Control interface color format
  - `011`: 12bit/pixel
  - `101`: 16bit/pixel (**selected**)
  - `110`: 18bit/pixel
  - `111`: 16M truncated

The "RGB interface color format" isn't set here; the datasheet suggests using
`0x55` for our purposes... Need to check what is going on here.

#### `0xB2` - `PORCTRL`: Porch Setting
Values: `0x0C 0x0C 0x00 0x33 0x33`

Probably empirically gained values for the porch settings, let's keep them.
"Separate porch control" is disabled.

#### `0xB7` - `GCTRL`: Gate Control
Values: `0x35`

Bits:
- `6:4`: VGH Setting (Voltage)
  - 0: 12.2 V
  - 1: 12.54 V
  - 2: 12.89 V
  - 3: 13.26 V
  - 4: 13.65 V
  - 5: 14.06 V
  - 6: 14.5 V
  - 7: 14.97 V
- `2:0`: VGL Setting (Voltage)
  - 0: -7.16 V
  - 1: -7.67 V
  - 2: -8.23 V
  - 3: -8.87 V
  - 4: -9.6 V
  - 5: -10.43 V
  - 6: -11.38 V
  - 7: -12.5 V

#### `0xBB` - `VCOMS`: VCOM Setting
Values: `0x19`

`0x19` -> 0.725 V

#### `0xC0` - `LCMCTRL`: LCM Control
Values: `0x2C`

Bits:
- `6`: XMY: XOR MY setting in command 36h
- `5`: XBGR: XOR RGB setting in command 36h (**selected**)
- `4`: XREV: XOR inverse setting in command 21h
- `3`: XMH: this bit can reverse source output order and only support for RGB
  interface without RAM mode (**selected**)
- `2`: XMV: XOR MV setting in command 36h (**selected**)
- `1`: XMX: XOR MX setting in command 36h
- `0`: XGS: XOR GS setting in command E4h

#### `0xC2` - `VDVVRHEN`: VDV and VRH Command Enable
Values: `0x01`

- `0`: VDV and VRH register value comes from NVM.
- `1`: VDV and VRH register value comes from command write (**selected**).

#### `0xC3` - `VRHS`: VRH Set
Values: `0x12`

`0x12` -> VAP: 4.45+( vcom+vcom offset+vdv)
`0x12` -> VAN: -4.45+( vcom+vcom offset-vdv)

#### `0xC4` - `VDVS`: VDV Set
Values: `0x20`

`0x20` -> VDV: 0 V

#### `0xC6` - `FRCTRL2`: Frame Rate Control in Normal Mode
Values: `0x0F`

Bits:
- `4:0`: RTNA
  - `0x0F` -> 60 Hz
- `7:5`: NLA - Inversion selection in normal mode
  - `0x0`: dot inversion (**selected**).
  - `0x7`: column inversion.

#### `0xD0` - `PWCTRL1`: Power Control 1
Values: `0xA4 0xA1`

`0xA4` is a fixed parameter?

Second Parameter Bits:
- `7:6`: AVDD, `2` -> 6.8 V
- `5:4`: AVCL, `2` -> -4.8 V
- `1:0`: VDS, `1` -> 2.3 V

#### `0xE0` - `PVGAMCTRL`: Positive Voltage Gamma Control
Values: `0xD0 0x04 0x0D 0x11 0x13 0x2B 0x3F 0x54 0x4C 0x18 0x0D 0x0B 0x1F 0x23`

Gamma Curve, probably calibrated for this device...

#### `0xE1` - `NVGAMCTRL`: Negative Voltage Gamma Control
Values: `0xD0 0x04 0x0C 0x11 0x13 0x2C 0x3F 0x44 0x51 0x2F 0x1F 0x1F 0x20 0x23`

Gamma Curve, probably calibrated for this device...

#### `0x21` - `INVON`: Display Inversion On
Self-explaining...

#### `0x11` - `SLPOUT`: Sleep Out
Self-explaining...

Interestingly, other drivers have added a delay after sending `SLPOUT` but here
this is not the case.  Maybe the timing allows for this here?

#### `0x29` - `DISPON`: Display On
Self-explaining...
