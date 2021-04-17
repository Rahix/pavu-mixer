use anyhow::Context;
use pulse::context;
use pulse::mainloop::standard as mainloop;

pub struct PulseInterface {
    mainloop: mainloop::Mainloop,
    pub context: context::Context,
}

impl PulseInterface {
    pub fn init() -> anyhow::Result<Self> {
        let mut proplist = pulse::proplist::Proplist::new().context("failed creating proplist")?;
        proplist
            .set_str(
                pulse::proplist::properties::APPLICATION_NAME,
                "Pavu-Mixer Daemon",
            )
            .ok()
            .context("failed setting proplist string")?;

        let mainloop = mainloop::Mainloop::new().context("failed creating mainloop")?;

        let context =
            pulse::context::Context::new_with_proplist(&mainloop, "PavuMixerContext", &proplist)
                .context("failed creating context")?;

        let mut this = PulseInterface { mainloop, context };

        this.context
            .connect(None, pulse::context::FlagSet::NOFLAGS, None)
            .context("failed connecting context")?;

        // Wait for context
        'wait_for_ctx: loop {
            this.iterate(true)?;
            match this.context.get_state() {
                pulse::context::State::Ready => {
                    break 'wait_for_ctx;
                }
                pulse::context::State::Terminated | pulse::context::State::Failed => {
                    anyhow::bail!("terminated or failed context");
                }
                _ => (),
            }
        }

        Ok(this)
    }

    pub fn iterate(&mut self, block: bool) -> anyhow::Result<()> {
        match self.mainloop.iterate(block) {
            mainloop::IterateResult::Success(_) => Ok(()),
            mainloop::IterateResult::Quit(_) => unreachable!("no code should quit the mainloop!"),
            mainloop::IterateResult::Err(e) => Err(e).context("failed mainloop iteration"),
        }
    }
}
