use std::{collections::HashMap, marker::PhantomData, ops::RangeInclusive};

use fluxemu_graphics::api::software::texture::Texture;
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    component::config::{ComponentConfig, LateContext},
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::{AddressSpaceId, MemoryMapCommand, Permissions},
    path::ComponentPath,
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

    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        let backend = <P::GraphicsApi as SupportedGraphicsApiTia>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        component.backend = Some(backend);
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (mut component_builder, _) = component_builder
            .scheduler_participation(Some(SchedulerParticipation::OnAccess))
            .framebuffer("framebuffer");

        let my_path = component_builder.path().clone();

        component_builder = component_builder.map_memory(
            self.cpu_address_space,
            MemoryMapCommand::with_component(
                my_path.clone(),
                ReadRegisters::iter().map(|address| {
                    (
                        RangeInclusive::from_single(address as usize),
                        Permissions::READ,
                    )
                }),
            ),
        );
        component_builder = component_builder.map_memory(
            self.cpu_address_space,
            MemoryMapCommand::with_component(
                my_path.clone(),
                WriteRegisters::iter().map(|address| {
                    (
                        RangeInclusive::from_single(address as usize),
                        Permissions::WRITE,
                    )
                }),
            ),
        );

        let staging_buffer = Texture::from_value(
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
                in_vsync: false,
                input_control: [InputControl::default(); 6],
                electron_beam: Point2::default(),
                missiles: Default::default(),
                ball: Default::default(),
                players: Default::default(),
                playfield: Default::default(),
                high_playfield_ball_priority: false,
                background_color: Default::default(),
                staging_buffer,
                hmove_pending: false,
            },
            path: component_builder.path().clone(),
        })
    }
}
