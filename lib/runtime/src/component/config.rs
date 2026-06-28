use std::{error::Error, fmt::Debug};

use fluxemu_graphics::api::GraphicsApi;

use crate::{Platform, component::Component, machine::builder::ComponentBuilder};

/// Factory config to construct a component
#[allow(unused)]
pub trait ComponentConfig<P: Platform>: Debug + Sized + Sync + Send {
    /// The component that this config will create
    type Component: Component;

    /// Make a new component from the config
    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn Error>>;

    /// Do setup for subsystems that cannot be initialized during [`Self::build_component`]
    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        Default::default()
    }
}

/// Late initialized data the runtime will produce for you
pub struct LateContext<P: Platform> {
    /// Graphics initialization data matching the specifications the component gave, or a superset of them
    pub graphics_initialization_data: <P::GraphicsApi as GraphicsApi>::InitializationData,
}
