use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::{Debug, Display},
    sync::Arc,
};

use fluxemu_graphics::{
    api::{Framebuffer, GraphicsApi, software::Software},
    texture::{AsViewTexture, RefTexture},
};
use palette::Srgba;
use rustc_hash::FxBuildHasher;

use crate::{
    ResourcePath,
    graphics::{Gpu, Handle, JobQueue, Resources, SimpleDisplayBackend, Submitter},
};

pub type DisplayBackend<G> = Box<dyn SimpleDisplayBackend<GraphicsApi = G> + Send + Sync>;

type SoftwareFramebuffer = Handle<<Software as GraphicsApi>::Framebuffer>;

pub struct DisplayBackendSelector<G: GraphicsApi> {
    submitter: Option<Submitter<G>>,
    selected: Option<DisplayBackend<G>>,
}

impl<G: GraphicsApi> DisplayBackendSelector<G> {
    pub fn new(submitter: Submitter<G>) -> Self {
        Self {
            submitter: Some(submitter),
            selected: None,
        }
    }

    pub fn with<A: GraphicsApi>(
        mut self,
        construct: impl FnOnce(Submitter<A>) -> DisplayBackend<A>,
    ) -> Self {
        if self.selected.is_none() && TypeId::of::<A>() == TypeId::of::<G>() {
            let submitter = self.submitter.take().unwrap();
            let backend = construct(cast(submitter));

            self.selected = Some(cast(backend));
        }

        self
    }

    pub fn or_software(
        self,
        component: impl Display,
        construct: impl FnOnce(Submitter<Software>) -> DisplayBackend<Software>,
    ) -> DisplayBackend<G> {
        if TypeId::of::<G>() == TypeId::of::<Software>() {
            return self.with::<Software>(construct).selected.unwrap();
        }

        if let Some(selected) = self.selected {
            return selected;
        }

        tracing::warn!(
            "Component {} has no {} backend, falling back to software rendering",
            component,
            G::NAME
        );

        Box::new(SoftwareFallback::new(self.submitter.unwrap(), construct))
    }
}

fn cast<FROM: 'static, TO: 'static>(value: FROM) -> TO {
    *(Box::new(value) as Box<dyn Any>)
        .downcast()
        .expect("Types were checked to be identical")
}

#[derive(Debug)]
pub struct SoftwareFallback<G: GraphicsApi> {
    submitter: Submitter<G>,
    software: DisplayBackend<Software>,
    software_jobs: Arc<JobQueue<Software>>,
    software_resources: Handle<Resources>,
    framebuffers: HashMap<Handle<G::Framebuffer>, SoftwareFramebuffer, FxBuildHasher>,
}

impl<G: GraphicsApi> SoftwareFallback<G> {
    pub fn new(
        submitter: Submitter<G>,
        construct: impl FnOnce(Submitter<Software>) -> DisplayBackend<Software>,
    ) -> Self {
        let software_jobs = Arc::new(JobQueue::default());
        let software = construct(software_jobs.submitter());
        let software_resources = submitter.reserve();

        Self {
            submitter,
            software,
            software_jobs,
            software_resources,
            framebuffers: HashMap::default(),
        }
    }

    fn synchronize(
        &self,
        framebuffer: Handle<G::Framebuffer>,
        software_framebuffer: SoftwareFramebuffer,
    ) -> impl for<'a> FnOnce(&mut Gpu<'a, G>) + Send + 'static {
        let software_jobs = self.software_jobs.clone();
        let software_resources = self.software_resources;

        move |gpu| {
            let mut resources = gpu.resources.remove(software_resources).unwrap_or_default();

            software_jobs.run(&mut Gpu {
                context: &mut (),
                resources: &mut resources,
            });

            if let Some(frame) = resources.get(software_framebuffer) {
                let size = frame.size();
                let outdated = gpu.resources.get(framebuffer).is_none_or(|existing| {
                    existing.width() != size.x || existing.height() != size.y
                });

                if outdated {
                    let created = G::create_software_framebuffer(gpu.context, size);
                    gpu.resources.insert(framebuffer, created);
                }

                let target = gpu.resources.get_mut(framebuffer).unwrap();
                G::upload_software_frame(gpu.context, target, frame.as_view());
            }

            gpu.resources.insert(software_resources, resources);
        }
    }
}

impl<G: GraphicsApi> SimpleDisplayBackend for SoftwareFallback<G> {
    type GraphicsApi = G;

    fn produce_initial_framebuffer(&mut self, path: &ResourcePath) -> Handle<G::Framebuffer> {
        let software_framebuffer = self.software.produce_initial_framebuffer(path);
        let framebuffer = self.submitter.reserve();

        self.framebuffers.insert(framebuffer, software_framebuffer);
        self.submitter
            .submit(self.synchronize(framebuffer, software_framebuffer));

        framebuffer
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: Handle<G::Framebuffer>,
    ) {
        let software_framebuffer = self.framebuffers[&framebuffer];

        self.software
            .commit_staging_buffer(staging_buffer, software_framebuffer);
        self.submitter.submit_replacing(
            framebuffer,
            self.synchronize(framebuffer, software_framebuffer),
        );
    }
}
