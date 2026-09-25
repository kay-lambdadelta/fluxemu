use std::{fmt::Display, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand, ValueEnum};
use fluxemu_program::{ProgramManager, RomId, SystemId};

#[derive(Clone, Parser)]
pub enum Cli {
    #[command(subcommand)]
    Database(DatabaseCommand),
    #[command(subcommand)]
    Rom(RomCommand),
    #[command(subcommand)]
    Manifest(ManifestCommand),
    #[command(subcommand)]
    Logiqx(LogiqxCommand),
}

#[derive(Clone, Subcommand)]
pub enum DatabaseCommand {
    /// Copy the contents of external databases into the internal database
    Import {
        #[clap(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,
    },
}

#[derive(Clone, Subcommand)]
pub enum RomCommand {
    /// Scan for ROMs and import them into the internal store
    Import {
        /// Symlink ROMs into the store instead of copying them
        #[clap(long)]
        symlink: bool,
        /// Scans paths recursively and through archive files
        #[clap(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,
    },
    /// Export ROMs into the organizational format more familiar for external use
    Export {
        /// Symlink ROMs out of the store instead of copying them
        #[clap(long)]
        symlink: bool,
        /// Set the format that they will be organized in
        #[clap(long, default_value_t = ExportStyle::default())]
        style: ExportStyle,
        /// Base directory to export into
        destination: PathBuf,
    },
    /// Verify ROMs within stores
    Verify,
    /// Patch ROM within store, producing a new ROM
    Patch {
        /// Source ROM to apply patch to
        source: RomSource,
        /// Patch file on disk
        ///
        /// Supported formats are: UPS
        patch: PathBuf,
        /// Add the patched ROM to the database, with an entry based on the source ROM
        #[clap(long)]
        add_to_database: bool,
    },
}

#[derive(Clone, Subcommand)]
pub enum ManifestCommand {
    /// Import RON based manifest files
    Import {
        #[clap(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,
    },
    /// Export RON based manifest files
    Export {
        /// Directory to output the program identifiers
        output_directory: PathBuf,
    },
}

#[derive(Clone, Subcommand)]
pub enum LogiqxCommand {
    /// Import logiqx format datasheet
    Import {
        #[clap(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,
    },
    /// Download Redump datasheets and import them
    DownloadRedump {
        #[clap(long, num_args = 1..)]
        system_filter: Vec<SystemId>,
    },
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum ExportStyle {
    #[default]
    NoIntro,
    Native,
    EmulationStation,
}

impl Display for ExportStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ExportStyle::NoIntro => "no-intro",
                ExportStyle::Native => "native",
                ExportStyle::EmulationStation => "emulationstation",
            }
        )
    }
}

#[derive(Clone, Debug)]
pub enum RomSource {
    Path(PathBuf),
    Id(RomId),
}

impl RomSource {
    pub fn to_id(&self, program_manager: &ProgramManager) -> Result<RomId, fluxemu_program::Error> {
        match self {
            RomSource::Path(path) => program_manager.register_external_rom(path),
            RomSource::Id(id) => Ok(*id),
        }
    }
}

impl FromStr for RomSource {
    type Err = String;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let path = PathBuf::from(string);
        if path.exists() {
            return Ok(RomSource::Path(path));
        }

        string.parse::<RomId>().map(RomSource::Id).map_err(|err| {
            format!("\"{string}\" is not an existing path, and not a valid ROM ID: {err}")
        })
    }
}
