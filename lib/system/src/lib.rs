#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::error::Error;

use fluxemu_platform::{MaybeSend, Platform};
use fluxemu_program::SystemId;
use fluxemu_runtime::machine::builder::{MachineBuilder, SealedMachineBuilder};
use serde::{Serialize, de::DeserializeOwned};

#[cfg(feature = "testing")]
pub mod testing;

pub trait System<P: Platform>: Send + Sync {
    type Quirks: Serialize + DeserializeOwned;
    const ID: SystemId;

    fn build(
        &self,
        quirks: Self::Quirks,
        machine_builder: MachineBuilder<P>,
    ) -> impl Future<Output = Result<SealedMachineBuilder<P>, Box<dyn Error>>> + MaybeSend;
}
