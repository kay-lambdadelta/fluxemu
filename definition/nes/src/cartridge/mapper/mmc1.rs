use fluxemu_runtime::{
    component::{Component, ComponentConfig},
    machine::builder::ComponentBuilder,
    platform::Platform,
};

use crate::cartridge::CartConfig;

#[derive(Debug)]
pub struct Mmc1;

impl Component for Mmc1 {
    fn load_snapshot(
        &mut self,
        _version: fluxemu_runtime::component::ComponentVersion,
        _reader: &mut dyn std::io::Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn store_snapshot(
        &self,
        _writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}

#[derive(Debug)]
pub struct Mmc1Config {
    pub config: CartConfig,
}

impl<P: Platform> ComponentConfig<P> for Mmc1Config {
    type Component = Mmc1;

    fn build_component(
        self,
        _component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        todo!()
    }
}
