use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub action: Option<CliAction>,
}

#[derive(Clone, Subcommand)]
pub enum CliAction {
    Run {
        #[clap(required=true, num_args=1..)]
        roms: Vec<PathBuf>,
    },
}
