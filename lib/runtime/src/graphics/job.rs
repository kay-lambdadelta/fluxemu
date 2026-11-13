use std::{
    any::Any,
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use fluxemu_graphics::api::GraphicsApi;
use rustc_hash::FxBuildHasher;

pub struct Handle<T> {
    id: usize,
    _type: PhantomData<fn() -> T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Handle").field(&self.id).finish()
    }
}

#[derive(Debug, Default)]
pub struct Resources {
    entries: HashMap<usize, Box<dyn Any>, FxBuildHasher>,
}

impl Resources {
    pub fn insert<T: 'static>(&mut self, handle: Handle<T>, resource: T) -> Option<T> {
        let previous = self.entries.insert(handle.id, Box::new(resource))?;

        previous.downcast().ok().map(|previous| *previous)
    }

    pub fn get<T: 'static>(&self, handle: Handle<T>) -> Option<&T> {
        self.entries.get(&handle.id)?.downcast_ref()
    }

    pub fn get_mut<T: 'static>(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.entries.get_mut(&handle.id)?.downcast_mut()
    }

    pub fn remove<T: 'static>(&mut self, handle: Handle<T>) -> Option<T> {
        let resource = self.entries.remove(&handle.id)?;

        resource.downcast().ok().map(|resource| *resource)
    }
}

pub struct Gpu<'a, G: GraphicsApi> {
    pub context: &'a mut G::Context,
    pub resources: &'a mut Resources,
}

type Job<G> = Box<dyn for<'a> FnOnce(&mut Gpu<'a, G>) + Send>;

struct PendingJob<G: GraphicsApi> {
    key: Option<usize>,
    job: Job<G>,
}

pub struct JobQueue<G: GraphicsApi> {
    next_handle: AtomicUsize,
    pending: Mutex<Vec<PendingJob<G>>>,
}

impl<G: GraphicsApi> Default for JobQueue<G> {
    fn default() -> Self {
        Self {
            next_handle: AtomicUsize::new(0),
            pending: Mutex::default(),
        }
    }
}

impl<G: GraphicsApi> Debug for JobQueue<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobQueue")
            .field("pending", &self.pending.lock().unwrap().len())
            .finish()
    }
}

impl<G: GraphicsApi> JobQueue<G> {
    pub fn submitter(self: &Arc<Self>) -> Submitter<G> {
        Submitter {
            queue: self.clone(),
        }
    }

    pub fn run(&self, gpu: &mut Gpu<'_, G>) {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());

        for PendingJob { job, .. } in pending {
            job(gpu);
        }
    }

    fn push(&self, key: Option<usize>, job: Job<G>) {
        let mut pending = self.pending.lock().unwrap();

        if let Some(key) = key
            && let Some(existing) = pending
                .iter_mut()
                .find(|pending_job| pending_job.key == Some(key))
        {
            existing.job = job;
        } else {
            pending.push(PendingJob { key, job });
        }
    }
}

pub struct Submitter<G: GraphicsApi> {
    queue: Arc<JobQueue<G>>,
}

impl<G: GraphicsApi> Clone for Submitter<G> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
        }
    }
}

impl<G: GraphicsApi> Debug for Submitter<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Submitter").finish_non_exhaustive()
    }
}

impl<G: GraphicsApi> Submitter<G> {
    pub fn headless() -> Self {
        Arc::new(JobQueue::default()).submitter()
    }

    pub fn reserve<T: 'static>(&self) -> Handle<T> {
        Handle {
            id: self.queue.next_handle.fetch_add(1, Ordering::Relaxed),
            _type: PhantomData,
        }
    }

    pub fn submit(&self, job: impl for<'a> FnOnce(&mut Gpu<'a, G>) + Send + 'static) {
        self.queue.push(None, Box::new(job));
    }

    pub fn submit_replacing<T>(
        &self,
        key: Handle<T>,
        job: impl for<'a> FnOnce(&mut Gpu<'a, G>) + Send + 'static,
    ) {
        self.queue.push(Some(key.id), Box::new(job));
    }
}
