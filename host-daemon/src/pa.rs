use anyhow::Context;
use pulse::context;
use pulse::mainloop::standard as mainloop;

pub fn init() -> anyhow::Result<(mainloop::Mainloop, context::Context)> {
    let mut proplist = pulse::proplist::Proplist::new().context("failed creating proplist")?;
    proplist
        .set_str(
            pulse::proplist::properties::APPLICATION_NAME,
            "Pavu-Mixer Daemon",
        )
        .ok()
        .context("failed setting proplist string")?;

    let mut mainloop = mainloop::Mainloop::new().context("failed creating mainloop")?;

    let mut context =
        pulse::context::Context::new_with_proplist(&mainloop, "PavuMixerContext", &proplist)
            .context("failed creating context")?;
    context
        .connect(None, pulse::context::FlagSet::NOFLAGS, None)
        .context("failed connecting context")?;

    // Wait for context
    'wait_for_ctx: loop {
        iterate(&mut mainloop, true)?;
        match context.get_state() {
            pulse::context::State::Ready => {
                break 'wait_for_ctx;
            }
            pulse::context::State::Terminated | pulse::context::State::Failed => {
                anyhow::bail!("terminated or failed context");
            }
            _ => (),
        }
    }

    Ok((mainloop, context))
}

pub fn iterate(mainloop: &mut mainloop::Mainloop, block: bool) -> anyhow::Result<()> {
    match mainloop.iterate(block) {
        mainloop::IterateResult::Success(_) => Ok(()),
        mainloop::IterateResult::Quit(_) => unreachable!("no code should quit the mainloop!"),
        mainloop::IterateResult::Err(e) => Err(e).context("failed mainloop iteration"),
    }
}
