use std::{
    error::Error,
    io::{BufReader, Seek},
    sync::Arc,
};

use clap::Subcommand;
use fluxemu_program::{MachineId, NintendoSystem, ProgramManager, SegaSystem, SonySystem};
use strum::{Display, EnumIter};
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

impl TryFrom<MachineId> for RedumpSystem {
    type Error = ();

    fn try_from(value: MachineId) -> Result<Self, Self::Error> {
        match value {
            MachineId::Nintendo(NintendoSystem::GameCube) => Ok(Self::Gc),
            MachineId::Nintendo(NintendoSystem::Wii) => Ok(Self::Wii),
            MachineId::Sony(SonySystem::Playstation) => Ok(Self::Psx),
            MachineId::Sony(SonySystem::Playstation2) => Ok(Self::Ps2),
            MachineId::Sony(SonySystem::Playstation3) => Ok(Self::Ps3),
            MachineId::Sony(SonySystem::PlaystationPortable) => Ok(Self::Psp),
            MachineId::Sega(SegaSystem::SegaCD) => Ok(Self::Mcd),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Subcommand)]
pub enum RedumpAction {}

pub fn download_and_import_redump_system(
    system: RedumpSystem,
    program_manager: Arc<ProgramManager>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing::info!("Downloading redump dat for system {}", system);

    let url = format!("{}/{}", BASE_URL, system.to_string().to_lowercase());

    let mut temp_file = tempfile::tempfile()?;

    let response = ureq::get(&url).call()?;
    let response_body = response.into_body();
    let mut response_reader = response_body.into_reader();

    // Download to temp file
    std::io::copy(&mut response_reader, &mut temp_file)?;
    temp_file.seek(std::io::SeekFrom::Start(0))?;

    // Go into blocking mode for a zip operation
    let program_manager = program_manager.clone();
    let mut archive = ZipArchive::new(temp_file)?;

    for index in 0..archive.len() {
        let file = BufReader::new(archive.by_index(index)?);

        crate::logiqx::import(file, &program_manager)?;
    }

    Ok(())
}
