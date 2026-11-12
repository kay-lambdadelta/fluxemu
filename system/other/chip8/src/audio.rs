use fluxemu_audio::{SampleFormat, SquareWave};
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{Component, SampleSource, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    persistence::PersistentComponentState,
    scheduler::{
        Frequency, Period, QuantaAllocator,
        task::{FrequencyBased, Mode},
    },
};
use nalgebra::SVector;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::{Deserialize, Serialize};

/// Imaginary chip8 hardware sample rate
const INTERNAL_SAMPLE_RATE: f32 = 1760.0;

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    // The CPU will set this according to what the program wants
    timer: u8,
    wave_generator: SquareWave<f32, 1>,
    timer_accumulator: Period,
    audio_accumulator: f32,
}

#[derive(Debug)]
pub struct Chip8Audio {
    state: State,
    buffer: AllocRingBuffer<SVector<f32, 1>>,
    processor_frequency: Frequency,
}

impl PersistentComponentState<Chip8Audio> for State {
    fn capture(component: &Chip8Audio) -> impl Serialize {
        &component.state
    }

    fn apply(self, component: &mut Chip8Audio) {
        component.state = self;
    }
}

impl Chip8Audio {
    pub fn set(&mut self, value: u8) {
        self.state.timer = value;
    }

    #[inline]
    fn task(&mut self, _runtime_handle: &RuntimeHandle, quanta_allocator: QuantaAllocator<'_, '_>) {
        let timer_period = Period::from_num(60).recip();
        let samples_per_tick = INTERNAL_SAMPLE_RATE / self.processor_frequency.to_num::<f32>();

        for _ in quanta_allocator {
            self.state.audio_accumulator += samples_per_tick;

            while self.state.audio_accumulator >= 1.0 {
                let generated_sample = self.state.wave_generator.next().unwrap();
                let sample = if self.state.timer > 0 {
                    generated_sample
                } else {
                    SVector::from_element(f32::equilibrium())
                };

                let _ = self.buffer.enqueue(sample);

                self.state.audio_accumulator -= 1.0;
            }

            self.state.timer_accumulator += self.processor_frequency.recip();

            while self.state.timer_accumulator >= timer_period {
                self.state.timer = self.state.timer.saturating_sub(1);
                self.state.timer_accumulator -= timer_period;
            }
        }
    }
}

impl Component for Chip8Audio {
    type Event = ();
    type Save = ();
    type Snapshot = State;

    fn get_audio_channel(&mut self, _name: &str) -> SampleSource<'_> {
        SampleSource {
            audio_ring: &mut self.buffer,
            sample_rate: INTERNAL_SAMPLE_RATE,
        }
    }
}

#[derive(Debug)]
pub struct Chip8AudioConfig {
    pub processor_frequency: Frequency,
}

impl<P: Platform> ComponentConfig<P> for Chip8AudioConfig {
    type Component = Chip8Audio;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        component_builder
            .task(
                "synchronization",
                Mode::OnDemand,
                FrequencyBased::new(self.processor_frequency, Self::Component::task),
            )
            .audio_channel("mono");

        Ok(Chip8Audio {
            state: State {
                timer: 0,
                wave_generator: SquareWave::new(440.0, INTERNAL_SAMPLE_RATE, 0.5),
                timer_accumulator: Period::ZERO,
                audio_accumulator: 0.0,
            },
            buffer: AllocRingBuffer::new(INTERNAL_SAMPLE_RATE as _),
            processor_frequency: self.processor_frequency,
        })
    }
}
