use std::{collections::HashMap, marker::PhantomData};

use fluxemu_runtime::{
    component::config::{ComponentConfig, LateContext},
    graphics::software::Texture,
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::AddressSpaceId,
    path::ComponentPath,
    persistence::PersistanceFormatVersion,
    platform::Platform,
};
use nalgebra::Point2;
use palette::named::BLACK;
use strum::IntoEnumIterator;

use super::{Tia, region::Region};
use crate::tia::{
    InputControl, State, VISIBLE_SCANLINE_LENGTH,
    backend::{SupportedGraphicsApiTia, TiaDisplayBackend},
    memory::{ReadRegisters, WriteRegisters},
};

#[derive(Debug, Clone)]
pub(crate) struct TiaConfig<R: Region> {
    pub cpu: ComponentPath,
    pub cpu_address_space: AddressSpaceId,
    pub _phantom: PhantomData<R>,
}

impl<R: Region, P: Platform<GraphicsApi: SupportedGraphicsApiTia>> ComponentConfig<P>
    for TiaConfig<R>
{
    type Component = Tia<R, P::GraphicsApi>;
    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        let backend = <P::GraphicsApi as SupportedGraphicsApiTia>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        component.backend = Some(backend);
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (mut component_builder, _) = component_builder
            .scheduler_participation(Some(SchedulerParticipation::OnAccess))
            .framebuffer("framebuffer");

        for register in ReadRegisters::iter() {
            component_builder = component_builder.memory_map_component_read(
                self.cpu_address_space,
                register as usize..=register as usize,
            );
        }

        for register in WriteRegisters::iter() {
            component_builder = component_builder.memory_map_component_write(
                self.cpu_address_space,
                register as usize..=register as usize,
            );
        }

        let staging_buffer = Texture::new(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::TOTAL_SCANLINES as usize,
            BLACK.into(),
        );

        Ok(Tia {
            backend: None,
            cpu_path: self.cpu,
            state: State {
                collision_matrix: HashMap::default(),
                vblank_active: false,
                cycles_waiting_for_vsync: None,
                input_control: [InputControl::default(); 6],
                electron_beam: Point2::default(),
                missiles: Default::default(),
                ball: Default::default(),
                players: Default::default(),
                playfield: Default::default(),
                high_playfield_ball_priority: false,
                background_color: Default::default(),
                staging_buffer,
            },
            path: component_builder.path().clone(),
        })
    }
}
