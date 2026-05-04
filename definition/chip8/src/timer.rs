use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    persistence::PersistanceFormatVersion,
    platform::Platform,
    scheduler::{Period, SynchronizationContext},
};

#[derive(Debug)]
pub struct Chip8Timer {
    // The CPU will set this according to what the program wants
    timer: u8,
}

impl Chip8Timer {
    pub fn set(&mut self, value: u8) {
        self.timer = value;
    }

    pub fn get(&self) -> u8 {
        self.timer
    }
}

impl Component for Chip8Timer {
    type Event = ();

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        for _ in context.allocate_continuous(Period::ONE / 60) {
            self.timer = self.timer.saturating_sub(1);
        }
    }

    fn needs_work(&self, _timestamp: &Period, delta: &Period) -> bool {
        *delta >= Period::ONE / 60
    }
}

#[derive(Debug, Default)]
pub struct Chip8TimerConfig;

impl<P: Platform> ComponentConfig<P> for Chip8TimerConfig {
    type Component = Chip8Timer;
    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        component_builder.scheduler_participation(Some(SchedulerParticipation::OnAccess));

        Ok(Chip8Timer { timer: 0 })
    }
}
