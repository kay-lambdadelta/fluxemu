use std::{collections::HashMap, fmt::Debug};

use fluxemu_program::MachineId;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, MachineFactory},
    platform::Platform,
};

type MachineConstructor<P> =
    Box<dyn for<'b> Fn(MachineBuilder<'b, P>) -> MachineBuilder<'b, P> + Send + Sync>;

/// Factory storage for frontend machine generation automation
pub struct MachineFactories<P: Platform>(HashMap<MachineId, MachineConstructor<P>>);

impl<P: Platform> Debug for MachineFactories<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineFactories").finish()
    }
}

impl<P: Platform> MachineFactories<P> {
    /// Register a factory
    pub fn insert_factory<M: MachineFactory<P> + Default>(&mut self, system: MachineId) {
        self.0.insert(
            system,
            Box::new(|machine_builder| {
                let factory = M::default();

                factory.construct(machine_builder)
            }),
        );
    }

    /// Spit out a machine based upon the factories
    pub fn construct_machine<'a>(
        &self,
        machine_builder: MachineBuilder<'a, P>,
    ) -> Option<MachineBuilder<'a, P>> {
        let system = machine_builder.machine_id()?;

        Some(self.0.get(&system)?(machine_builder))
    }
}

impl<P: Platform> Default for MachineFactories<P> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}
