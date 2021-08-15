Driver for the "1.3inch LCD Module" from Waveshare
--------------------------------------------------
Driver for Waveshare's "[1.3inch LCD Module](https://www.waveshare.com/wiki/1.3inch_LCD_Module)" (240x240, RGB565):

![1.3inch LCD Module](https://www.waveshare.com/w/thumb.php?f=1.3inch-LCD-Module-1.jpg&width=300)

The module uses an ST7789 display controller. The driver code is adjusted for
the specific LCD panel.  I documented the initialization procedure in
[Initialization.md](Initialization.md).

The driver is intentionally barebones and only contains what is needed for
_Pavu Mixer_.  It might make sense to add proper interoperability with
[`embedded-graphics`](https://crates.io/crates/embedded-graphics) at some
point... I'll gladly accept contributions :)

## License
This display driver is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
