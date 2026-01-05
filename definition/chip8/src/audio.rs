use std::io::{Read, Write};

use fluxemu_audio::SquareWave;
use fluxemu_runtime::{
    component::{Component, ComponentConfig, ComponentVersion, SampleSource},
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    path::FluxEmuPath,
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use nalgebra::SVector;
use ringbuffer::{AllocRingBuffer, RingBuffer};

/// Imaginary chip8 hardware sample rate
const INTERNAL_SAMPLE_RATE: f32 = 8000.0;

#[derive(Debug)]
pub struct Chip8Audio {
    // The CPU will set this according to what the program wants
    timer: u8,
    buffer: AllocRingBuffer<SVector<f32, 1>>,
    wave_generator: SquareWave<f32, 1>,
    processor_frequency: Frequency,
    timer_accumulator: Period,
    audio_accumulator: f32,
}

impl Chip8Audio {
    pub fn set(&mut self, value: u8) {
        self.timer = value;
    }
}

impl Component for Chip8Audio {
    fn snapshot_version(&self) -> Option<ComponentVersion> {
        Some(0)
    }

    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        mut reader: Box<dyn Read>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(version, 0);
        let timer = std::array::from_mut(&mut self.timer);

        reader.read_exact(timer)?;

        Ok(())
    }

    fn store_snapshot(&self, mut writer: Box<dyn Write>) -> Result<(), Box<dyn std::error::Error>> {
        let timer = std::array::from_ref(&self.timer);

        writer.write_all(timer)?;

        Ok(())
    }

    fn get_audio_channel(&mut self, _audio_output_path: &FluxEmuPath) -> SampleSource<'_> {
        SampleSource {
            source: &mut self.buffer,
            sample_rate: INTERNAL_SAMPLE_RATE,
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let timer_period = Period::from_num(60).recip();
        let samples_per_tick = INTERNAL_SAMPLE_RATE / self.processor_frequency.to_num::<f32>();

        for _ in context.allocate(self.processor_frequency.recip(), None) {
            self.audio_accumulator += samples_per_tick;

            while self.audio_accumulator >= 1.0 {
                if self.timer > 0 {
                    let sample = self.wave_generator.next().unwrap();
                    let _ = self.buffer.enqueue(sample);
                };

                self.audio_accumulator -= 1.0;
            }

            self.timer_accumulator += self.processor_frequency.recip();
            while self.timer_accumulator >= timer_period {
                self.timer = self.timer.saturating_sub(1);
                self.timer_accumulator -= timer_period;
            }
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        delta >= self.processor_frequency.recip()
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
        component_builder: ComponentBuilder<'_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        component_builder
            .set_scheduler_participation(SchedulerParticipation::OnDemand)
            .insert_audio_channel("mono");

        Ok(Chip8Audio {
            timer: 0,
            buffer: AllocRingBuffer::new((INTERNAL_SAMPLE_RATE * 10.0) as _),
            wave_generator: SquareWave::new(440.0, INTERNAL_SAMPLE_RATE, 0.5),
            processor_frequency: self.processor_frequency,
            timer_accumulator: Period::ZERO,
            audio_accumulator: 0.0,
        })
    }
}
