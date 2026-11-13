use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use fluxemu_graphics::texture::{CopyMode, CowTexture, OwnedTexture, RefTexture};

const MAX_POOLED_BUFFERS: usize = 4;

pub struct StagingPool<T> {
    buffers: Arc<Mutex<Vec<OwnedTexture<T>>>>,
}

impl<T> Clone for StagingPool<T> {
    fn clone(&self) -> Self {
        Self {
            buffers: self.buffers.clone(),
        }
    }
}

impl<T> Default for StagingPool<T> {
    fn default() -> Self {
        Self {
            buffers: Arc::default(),
        }
    }
}

impl<T> Debug for StagingPool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagingPool")
            .field("pooled", &self.buffers.lock().unwrap().len())
            .finish()
    }
}

impl<T: Clone> StagingPool<T> {
    pub fn copy(&self, source: RefTexture<'_, T>) -> OwnedTexture<T> {
        let reused = {
            let mut buffers = self.buffers.lock().unwrap();

            buffers
                .iter()
                .position(|buffer| buffer.size() == source.size())
                .map(|index| buffers.swap_remove(index))
        };

        match reused {
            Some(mut buffer) => {
                buffer.copy_from(source, CopyMode::Nearest);

                buffer
            }
            None => CowTexture::from(source).into_owned(),
        }
    }

    pub fn recycle(&self, buffer: OwnedTexture<T>) {
        let mut buffers = self.buffers.lock().unwrap();

        if buffers.len() < MAX_POOLED_BUFFERS {
            buffers.push(buffer);
        }
    }
}
