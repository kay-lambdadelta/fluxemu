use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::event_loop::DisplayBackend;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub action: Option<CliAction>,
    #[clap(short, long, default_value_t=DisplayBackend::default())]
    pub display_backend: DisplayBackend,
}

#[derive(Clone, Subcommand)]
pub enum CliAction {
    /// Pass in ROMs to launch with
    Run {
        #[clap(required=true, num_args=1..)]
        roms: Vec<PathBuf>,
    },
}
