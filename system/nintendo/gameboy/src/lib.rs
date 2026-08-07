use fluxemu_program::{NintendoSystem, SystemId};
use fluxemu_runtime::{
    Platform,
    machine::builder::{MachineBuilder, SealedMachineBuilder},
};
use fluxemu_system::System;

#[derive(Debug, Default)]
pub struct Gameboy;

impl<P: Platform> System<P> for Gameboy {
    type Quirks = ();
    const ID: SystemId = SystemId::Nintendo(NintendoSystem::GameBoy);

    fn build(
        &self,
        quirks: Self::Quirks,
        machine_builder: MachineBuilder<P>,
    ) -> SealedMachineBuilder<P> {
        todo!()
    }
}
