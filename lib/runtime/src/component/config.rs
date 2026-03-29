use std::{borrow::Cow, collections::HashMap, error::Error, fmt::Debug};

use crate::{
    Platform, component::Component, graphics::GraphicsApi, machine::builder::ComponentBuilder,
};

#[allow(unused)]
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
    ///
    /// Return any framebuffers that were created during initialization
    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        Default::default()
    }
}

/// Data that the runtime will provide at the end of the initialization sequence
pub struct LateContext<P: Platform> {
    pub graphics_initialization_data: <P::GraphicsApi as GraphicsApi>::InitializationData,
}

pub struct LateInitializedData<P: Platform> {
    pub framebuffers: HashMap<Cow<'static, str>, <P::GraphicsApi as GraphicsApi>::Framebuffer>,
}

impl<P: Platform> Default for LateInitializedData<P> {
    fn default() -> Self {
        Self {
            framebuffers: HashMap::default(),
        }
    }
}
