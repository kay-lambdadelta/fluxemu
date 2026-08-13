// This crate will not work on a non-unix system
#![cfg(unix)]

use std::{ffi::OsString, path::PathBuf};

use clap::{Args, Parser, Subcommand};

mod build_nuttallite;

const DEFAULT_NUTTX_VERSION: &str = "13.0.0";

#[derive(Clone)]
pub enum NuttxLocation {
    Path { root: PathBuf },
    Download { version: String },
}

#[derive(Args, Clone)]
#[group(multiple = false, required = true)]
struct NuttxLocationArgs {
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long, num_args = 0..=1, default_missing_value = DEFAULT_NUTTX_VERSION)]
    download: Option<String>,
}

impl From<NuttxLocationArgs> for NuttxLocation {
    fn from(args: NuttxLocationArgs) -> Self {
        match (args.path, args.download) {
            (Some(root), _) => NuttxLocation::Path { root },
            (None, Some(version)) => NuttxLocation::Download { version },
            _ => unreachable!("clap group enforces exactly one"),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    BuildNuttallite {
        #[arg(long)]
        board: String,
        #[arg(long, default_value = "nsh")]
        board_config: String,
        #[command(flatten)]
        nuttx_location: NuttxLocationArgs,
        #[arg(long)]
        clean: bool,
        #[arg(last = true)]
        make_args: Vec<OsString>,
    },
}

fn main() {
    tracing_subscriber::fmt().init();

    match Cli::parse().command {
        Commands::BuildNuttallite {
            board,
            board_config,
            nuttx_location,
            clean,
            make_args,
        } => {
            build_nuttallite::build(board, board_config, nuttx_location.into(), clean, make_args);
        }
    }
}
