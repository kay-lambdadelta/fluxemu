use std::{error::Error, fmt::Debug};

use fluxemu_platform::Platform;

use crate::{component::Component, graphics::Submitter, machine::builder::ComponentBuilder};

/// Factory config to construct a component
pub trait ComponentConfig<P: Platform>: Debug + Sized + Sync + Send {
    /// The component that this config will create
    type Component: Component;

    /// Make a new component from the config
    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn Error>>;

    /// Do setup for subsystems that cannot be initialized during [`Self::build_component`]
    #[allow(unused)]
    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {}
}

/// Late initialized data the runtime will produce for you
pub struct LateContext<P: Platform> {
    /// Submits graphics jobs to the thread that owns the graphics api
    pub graphics_submitter: Submitter<P::GraphicsApi>,
}
