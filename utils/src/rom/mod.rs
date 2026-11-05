mod export;
mod import;
pub mod patch;

use std::{io, path::Path};

pub use export::export;
pub use import::import;
pub use patch::patch;

async fn symlink_file(source: &Path, destination: &Path) -> io::Result<()> {
    cfg_select! {
        target_family = "unix" => tokio::fs::symlink(source, destination).await,
        target_os = "windows" => tokio::fs::symlink_file(source, destination).await,
        _ => panic!("Unsupported operating system for symlinking"),
    }
}
