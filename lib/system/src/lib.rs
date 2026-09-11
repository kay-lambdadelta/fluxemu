#![no_std]

extern crate alloc;

use core::error::Error;

use alloc::boxed::Box;
use fluxemu_program::SystemId;
use fluxemu_runtime::{
    Platform,
    machine::builder::{MachineBuilder, SealedMachineBuilder},
};
use serde::{Serialize, de::DeserializeOwned};

pub trait System<P: Platform> {
    type Quirks: Serialize + DeserializeOwned;
    const ID: SystemId;

    fn build(
        &self,
        quirks: Self::Quirks,
        machine_builder: MachineBuilder<P>,
    ) -> Result<SealedMachineBuilder<P>, Box<dyn Error>>;
}
