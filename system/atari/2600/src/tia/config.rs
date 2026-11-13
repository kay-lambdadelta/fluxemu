use std::{collections::BTreeMap, marker::PhantomData, ops::RangeInclusive};

use fluxemu_graphics::texture::Texture;
use fluxemu_math::{range::ContiguousRange, rectangle::Rectangle};
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    component::config::{ComponentConfig, LateContext},
    graphics::DisplayGeometry,
    machine::builder::ComponentBuilder,
    memory::{AddressSpaceId, MemoryMapCommand, Permissions},
    path::ComponentPath,
    scheduler::task::{FrequencyBased, Mode},
};
use nalgebra::Point2;
use palette::named::BLACK;
use strum::IntoEnumIterator;

use super::{Tia, region::Region};
use crate::tia::{
    InputControl, State, VISIBLE_SCANLINE_LENGTH,
    memory::{ReadRegisters, WriteRegisters},
};

#[derive(Debug, Clone)]
pub(crate) struct TiaConfig<R: Region> {
    pub cpu: ComponentPath,
    pub cpu_address_space: AddressSpaceId,
    pub joystick: ComponentPath,
    pub _phantom: PhantomData<R>,
}

impl<R: Region, P: Platform> ComponentConfig<P> for TiaConfig<R> {
    type Component = Tia<R, P::GraphicsApi>;

    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        let backend = super::backend::display_backend::<R, P::GraphicsApi>(
            data.graphics_submitter.clone(),
            component.framebuffer_path.parent().unwrap(),
        );
        component.backend = Some(backend);
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (mut component_builder, framebuffer_path) = component_builder
            .task(
                "synchronization",
                Mode::OnDemand,
                FrequencyBased::new(R::frequency(), Self::Component::task),
            )
            .framebuffer(
                "framebuffer",
                DisplayGeometry {
                    pixel_aspect_ratio: R::pixel_aspect_ratio(),
                    display_rectangle: Some(Rectangle::from_min_and_max(
                        Point2::new(0, R::DISPLAYED_SCANLINES.start as usize),
                        Point2::new(
                            VISIBLE_SCANLINE_LENGTH as usize,
                            R::DISPLAYED_SCANLINES.end as usize,
                        ),
                    )),
                },
                |component, path| {
                    component
                        .backend
                        .as_mut()
                        .unwrap()
                        .produce_initial_framebuffer(path)
                },
            );

        let my_path = component_builder.path().clone();

        component_builder = component_builder.map_memory(
            self.cpu_address_space,
            MemoryMapCommand::with_component(
                my_path.clone(),
                (0x00..=0x30).step_by(0x10).flat_map(|mirror| {
                    ReadRegisters::iter().map(move |address| {
                        (
                            RangeInclusive::from_single(mirror + address as usize),
                            Permissions::READ,
                        )
                    })
                }),
            ),
        );

        component_builder.map_memory(
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
            _region: PhantomData,
            cpu_path: self.cpu,
            joystick_path: self.joystick,
            framebuffer_path,
            state: State {
                collision_matrix: BTreeMap::default(),
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
                hmove_pending: false,
            },
            staging_buffer,
        })
    }
}
