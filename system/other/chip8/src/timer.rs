use fluxemu_runtime::{
    RuntimeHandle,
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    platform::Platform,
    scheduler::{
        Frequency, QuantaAllocator,
        task::{FrequencyBased, Mode},
    },
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

    #[inline]
    fn task(&mut self, _runtime_handle: &RuntimeHandle, quanta_allocator: QuantaAllocator<'_, '_>) {
        for _ in quanta_allocator {
            self.timer = self.timer.saturating_sub(1);
        }
    }
}

impl Component for Chip8Timer {
    type Event = ();
}

#[derive(Debug, Default)]
pub struct Chip8TimerConfig;

impl<P: Platform> ComponentConfig<P> for Chip8TimerConfig {
    type Component = Chip8Timer;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        component_builder.task(
            "synchronization",
            Mode::OnDemand,
            FrequencyBased::new(Frequency::from_num(60), Self::Component::task),
        );

        Ok(Chip8Timer { timer: 0 })
    }
}
