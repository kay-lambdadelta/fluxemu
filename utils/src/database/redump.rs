use std::{
    error::Error,
    io::{BufReader, Seek},
};

use fluxemu_program::{NintendoSystem, ProgramManager, SegaSystem, SonySystem, SystemId};
use strum::{Display, EnumIter};
use tempfile::tempfile;
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

pub fn download_and_import_redump_system(
    program_manager: &ProgramManager,
    system: RedumpSystem,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing::info!("Downloading redump dat for system {}", system);
    let url = format!("{}/{}", BASE_URL, system.to_string().to_lowercase());

    let mut temp_file = tempfile()?;

    let response = ureq::get(&url).call()?;
    let response_body = response.into_body();
    let mut response_reader = response_body.into_reader();

    // Download to temp file
    std::io::copy(&mut response_reader, &mut temp_file)?;
    temp_file.rewind()?;

    let mut archive = ZipArchive::new(temp_file)?;

    for index in 0..archive.len() {
        let file = BufReader::new(archive.by_index(index)?);

        crate::logiqx::add_to_database(program_manager, file)?;
    }

    Ok(())
}
