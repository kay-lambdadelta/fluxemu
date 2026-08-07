use std::{collections::HashMap, fmt::Debug};

use fluxemu_program::SystemId;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, SealedMachineBuilder},
    platform::Platform,
};
use fluxemu_system::System;

type MachineConstructor<P> =
    Box<dyn Fn(ron::Value, MachineBuilder<P>) -> SealedMachineBuilder<P> + Send + Sync>;

/// Factory storage for frontend machine generation automation
pub struct FactoryManager<P: Platform>(HashMap<SystemId, MachineConstructor<P>>);

impl<P: Platform> Debug for FactoryManager<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineFactories").finish()
    }
}

impl<P: Platform> FactoryManager<P> {
    /// Register a factory
    pub fn insert_factory<S: System<P> + Default>(&mut self) {
        self.0.insert(
            S::ID,
            Box::new(|quirks, machine_builder| {
                let factory = S::default();
                let quirks = quirks.into_rust().unwrap();

                factory.build(quirks, machine_builder)
            }),
        );
    }

    /// Construct a machine based upon the factories
    pub fn construct_machine(
        &self,
        quirks: ron::Value,
        machine_builder: MachineBuilder<P>,
    ) -> Option<SealedMachineBuilder<P>> {
        let system = machine_builder.system_id()?;

        Some(self.0.get(&system)?(quirks, machine_builder))
    }
}

impl<P: Platform> Default for FactoryManager<P> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}
