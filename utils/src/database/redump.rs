use std::{
    error::Error,
    io::{BufReader, Seek},
    sync::Arc,
};

use fluxemu_database::ProgramDatabase;
use fluxemu_program::{NintendoSystem, SegaSystem, SonySystem, SystemId};
use strum::{Display, EnumIter};
use tempfile::tempfile;
use tokio::{fs::File, io::AsyncWriteExt, task::spawn_blocking};
use zip::ZipArchive;

const BASE_URL: &str = "http://redump.org/datfile";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, Display)]
pub enum RedumpSystem {
    Gc,
    Wii,
    Psx,
    Ps2,
    Ps3,
    Psp,
    Mcd,
}

impl TryFrom<SystemId> for RedumpSystem {
    type Error = ();

    fn try_from(value: SystemId) -> Result<Self, Self::Error> {
        match value {
            SystemId::Nintendo(NintendoSystem::GameCube) => Ok(Self::Gc),
            SystemId::Nintendo(NintendoSystem::Wii) => Ok(Self::Wii),
            SystemId::Sony(SonySystem::Playstation) => Ok(Self::Psx),
            SystemId::Sony(SonySystem::Playstation2) => Ok(Self::Ps2),
            SystemId::Sony(SonySystem::Playstation3) => Ok(Self::Ps3),
            SystemId::Sony(SonySystem::PlaystationPortable) => Ok(Self::Psp),
            SystemId::Sega(SegaSystem::SegaCD) => Ok(Self::Mcd),
            _ => Err(()),
        }
    }
}

pub async fn download_and_import_redump_system(
    client: &reqwest::Client,
    program_database: Arc<ProgramDatabase>,
    system: RedumpSystem,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing::info!("Downloading redump dat for system {}", system);
    let url = format!("{}/{}", BASE_URL, system.to_string().to_lowercase());

    let mut temp_file = File::from_std(spawn_blocking(tempfile).await??);

    let mut response = client.get(&url).send().await?.error_for_status()?;

    // Download to temp file
    while let Some(chunk) = response.chunk().await? {
        temp_file.write_all(&chunk).await?;
    }
    temp_file.flush().await?;

    let mut temp_file = temp_file.into_std().await;

    spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        temp_file.rewind()?;

        let mut archive = ZipArchive::new(temp_file)?;

        for index in 0..archive.len() {
            let file = BufReader::new(archive.by_index(index)?);

            crate::logiqx::add_to_database(&program_database, file)?;
        }

        Ok(())
    })
    .await?
}
