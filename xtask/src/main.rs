use std::{
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;

const TOOLCHAIN_FILE: &str = "rust-toolchain.toml";
const CARGO_CONFIG_FILE: &str = ".cargo/config.toml";
const ZED_SETTINGS_FILE: &str = ".zed/settings.json";

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Unknown shell {name}, available shells: {}", available.join(", "))]
    UnknownShell {
        name: String,
        available: Vec<String>,
    },
    #[error("{} is not a symlink, refusing to replace it", .0.display())]
    NotASymlink(PathBuf),
    #[error("Could not create symlink {}: {source}", link.display())]
    Symlink { link: PathBuf, source: io::Error },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Parser, Debug)]
enum Cli {
    Switch { shell: String },
}

fn main() -> ExitCode {
    match run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Error> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    match Cli::parse() {
        Cli::Switch { shell } => switch(workspace_root, &shell),
    }
}

fn switch(workspace_root: &Path, shell: &str) -> Result<(), Error> {
    let available = available_shells(workspace_root)?;

    if !available.iter().any(|name| name == shell) {
        return Err(Error::UnknownShell {
            name: shell.to_string(),
            available,
        });
    }

    let shell_directory = Path::new("shell").join(shell);

    let toolchain_link = workspace_root.join(TOOLCHAIN_FILE);
    remove_symlink(&toolchain_link)?;
    create_symlink(&shell_directory.join(TOOLCHAIN_FILE), &toolchain_link)?;

    switch_optional_link(workspace_root, &shell_directory, CARGO_CONFIG_FILE)?;
    switch_optional_link(workspace_root, &shell_directory, ZED_SETTINGS_FILE)?;

    println!("Switched to {}", shell);

    Ok(())
}

fn switch_optional_link(
    workspace_root: &Path,
    shell_directory: &Path,
    file: &str,
) -> Result<(), Error> {
    let link = workspace_root.join(file);
    remove_symlink(&link)?;

    if workspace_root.join(shell_directory).join(file).exists() {
        std::fs::create_dir_all(link.parent().unwrap())?;

        create_symlink(&Path::new("..").join(shell_directory).join(file), &link)?;
    }

    Ok(())
}

fn available_shells(workspace_root: &Path) -> Result<Vec<String>, Error> {
    let mut shells = Vec::default();

    for entry in std::fs::read_dir(workspace_root.join("shell"))? {
        let path = entry?.path();

        if path.join(TOOLCHAIN_FILE).is_file()
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            shells.push(name.to_string());
        }
    }

    shells.sort();

    Ok(shells)
}

fn remove_symlink(link: &Path) -> Result<(), Error> {
    match std::fs::symlink_metadata(link) {
        Ok(metadata) if metadata.is_symlink() => Ok(std::fs::remove_file(link)?),
        Ok(_) => Err(Error::NotASymlink(link.to_path_buf())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn create_symlink(target: &Path, link: &Path) -> Result<(), Error> {
    cfg_select! {
        unix => std::os::unix::fs::symlink(target, link),
        windows => std::os::windows::fs::symlink_file(target, link),
    }
    .map_err(|source| Error::Symlink {
        link: link.to_path_buf(),
        source,
    })
}
