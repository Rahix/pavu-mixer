use core::cell::RefCell;
use stm32f3xx_hal::{self as hal, pac, prelude::*};

use micromath::F32Ext;

pub async fn faders_task(
    mut adc1: hal::adc::Adc<pac::ADC1>,
    mut fader_main_adc: impl embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
    mut fader_ch1_adc: impl embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
    mut fader_ch2_adc: impl embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
    mut fader_ch3_adc: impl embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
    mut fader_ch4_adc: impl embedded_hal::adc::Channel<pac::ADC1, ID = u8>,
    pending_volume_updates: &RefCell<heapless::LinearMap<common::Channel, f32, 5>>,
) {
    let enqueue_if_changed = |ch, val: u16, prev: &mut f32| {
        let scaled_value = ((val as f32).clamp(8.0, 3308.0) - 8.0) / 3300.0;
        if (*prev - scaled_value).abs() > 0.01 {
            *prev = scaled_value;
            &pending_volume_updates
                .borrow_mut()
                .insert(ch, scaled_value)
                .unwrap();
        }
    };

    let mut previous_values: [f32; 5] = [-1.0; 5];
    loop {
        let main_value = adc1.read(&mut fader_main_adc).expect("Error reading ADC.");
        enqueue_if_changed(common::Channel::Main, main_value, &mut previous_values[0]);
        cassette::yield_now().await;

        let ch1_value = adc1.read(&mut fader_ch1_adc).expect("Error reading ADC.");
        enqueue_if_changed(common::Channel::Ch1, ch1_value, &mut previous_values[1]);
        cassette::yield_now().await;

        let ch2_value = adc1.read(&mut fader_ch2_adc).expect("Error reading ADC.");
        enqueue_if_changed(common::Channel::Ch2, ch2_value, &mut previous_values[2]);
        cassette::yield_now().await;

        let ch3_value = adc1.read(&mut fader_ch3_adc).expect("Error reading ADC.");
        enqueue_if_changed(common::Channel::Ch3, ch3_value, &mut previous_values[3]);
        cassette::yield_now().await;

        let ch4_value = adc1.read(&mut fader_ch4_adc).expect("Error reading ADC.");
        enqueue_if_changed(common::Channel::Ch4, ch4_value, &mut previous_values[4]);
        cassette::yield_now().await;
    }
}
