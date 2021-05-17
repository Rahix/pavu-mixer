use core::cell::RefCell;

pub async fn mute_buttons_task<'a, E, M, I2C, EBUS>(
    pca_int: impl embedded_hal::digital::v2::InputPin<Error = E>,
    mute_main: port_expander::Pin<'a, port_expander::mode::Input, M>,
    mute_ch1: port_expander::Pin<'a, port_expander::mode::Input, M>,
    mute_ch2: port_expander::Pin<'a, port_expander::mode::Input, M>,
    mute_ch3: port_expander::Pin<'a, port_expander::mode::Input, M>,
    mute_ch4: port_expander::Pin<'a, port_expander::mode::Input, M>,
    pending_presses: &RefCell<heapless::LinearMap<common::Channel, (), 5>>,
) where
    E: core::fmt::Debug,
    M: shared_bus::BusMutex<Bus = port_expander::dev::pca9555::Driver<I2C>>,
    I2C: port_expander::I2cBus<BusError = EBUS>,
    EBUS: core::fmt::Debug,
{
    loop {
        if pca_int.is_high().unwrap() {
            // nothing happened...
            cassette::yield_now().await;
            continue;
        }

        let buttons =
            port_expander::read_multiple([&mute_main, &mute_ch1, &mute_ch2, &mute_ch3, &mute_ch4])
                .unwrap();

        let mut pending_presses = pending_presses.borrow_mut();
        if !buttons[0] {
            pending_presses.insert(common::Channel::Main, ()).unwrap();
        }
        if !buttons[1] {
            pending_presses.insert(common::Channel::Ch1, ()).unwrap();
        }
        if !buttons[2] {
            pending_presses.insert(common::Channel::Ch2, ()).unwrap();
        }
        if !buttons[3] {
            pending_presses.insert(common::Channel::Ch3, ()).unwrap();
        }
        if !buttons[4] {
            pending_presses.insert(common::Channel::Ch4, ()).unwrap();
        }
        drop(pending_presses);

        cassette::yield_now().await;
    }
}
