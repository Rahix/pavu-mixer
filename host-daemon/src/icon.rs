use gtk::prelude::IconThemeExt;

pub fn get_icon_data(name: &str) -> Option<Vec<u8>> {
    let icon_theme = gtk::IconTheme::default()?;
    let icon = icon_theme
        .load_icon(
            name,
            common::ICON_SIZE as i32,
            gtk::IconLookupFlags::FORCE_SIZE,
        )
        .ok()??;

    if icon.bits_per_sample() != 8 {
        log::warn!("Icon pixbuf does not use 8-bits-per-sample.");
        return None;
    }
    if icon.colorspace() != gdk_pixbuf::Colorspace::Rgb {
        log::warn!("Icon pixbuf does not use Rgb colorspace.");
        return None;
    }

    let icon_buffer = icon.read_pixel_bytes()?;

    let mut target_buffer = vec![];

    match icon.n_channels() {
        4 => {
            for i in 0..(icon_buffer.len() / 4) {
                let (r, g, b) = (
                    icon_buffer[i * 4] as u16,
                    icon_buffer[i * 4 + 1] as u16,
                    icon_buffer[i * 4 + 2] as u16,
                );
                let alpha = icon_buffer[i * 4 + 3] as u16;
                let (r, g, b) = (r * alpha / 255, g * alpha / 255, b * alpha / 255);
                let rgb565: u16 = ((r & 0b11111000) << 8) | ((g & 0b11111100) << 3) | (b >> 3);
                target_buffer.extend_from_slice(&rgb565.to_be_bytes());

                // print!("\x1B[48;2;{};{};{}m  ", r, g, b);
                // if i % common::ICON_SIZE == (common::ICON_SIZE - 1) {
                //     println!("\x1B[0m");
                // }
            }
        }
        i => todo!("image has {} channels and this is not yet supported", i),
    }

    assert!(target_buffer.len() == common::ICON_SIZE * common::ICON_SIZE * 2);
    Some(target_buffer)
}
