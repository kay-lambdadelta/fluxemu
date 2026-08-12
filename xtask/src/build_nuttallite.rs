use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use flate2::read::GzDecoder;
use tempfile::tempdir;

use crate::{NuttxLocation, nuttx_apps_url, nuttx_url};

pub fn build(
    board: String,
    board_config: String,
    location: NuttxLocation,
    clean: bool,
    make_args: Vec<OsString>,
) {
    let root = match location {
        NuttxLocation::Path { root } => root,
        NuttxLocation::Download { version } => fetch_workspace(&version),
    };

    let nuttx_directory = root.join("nuttx");
    let nuttx_apps_directory = root.join("apps");

    register_app(&nuttx_apps_directory);
    write_boot_script(&nuttx_apps_directory);

    if clean {
        sh(&nuttx_directory, "make", ["distclean"]);
    }

    sh(
        &nuttx_directory,
        "./tools/configure.sh",
        [format!("{board}:{board_config}").as_str()],
    );

    {
        let config_path = nuttx_directory.join(".config");
        let mut config_file = OpenOptions::new()
            .append(true)
            .open(&config_path)
            .expect("failed to open .config for appending");

        let fragment_path = PathBuf::from("shell/nuttallite/nuttx/config/base");
        tracing::info!("Merging config fragment: {}", fragment_path.display());

        let fragment =
            fs::read_to_string(&fragment_path).expect("failed to read base config fragment");

        config_file.write_all(fragment.as_bytes()).unwrap();

        let fragment_path = PathBuf::from("shell/nuttallite/nuttx/config/device").join(&board);
        if let Ok(fragment) = fs::read_to_string(&fragment_path) {
            tracing::info!(
                "Merging board specific config fragment: {}",
                fragment_path.display()
            );

            config_file.write_all(fragment.as_bytes()).unwrap();
        } else {
            tracing::warn!(
                "Missing board specific config fragment: {}",
                fragment_path.display()
            );
        }
    }

    sh(&nuttx_directory, "make", ["olddefconfig"]);
    sh(&nuttx_directory, "make", ["context"]);
    sh(&nuttx_directory, "make", make_args);

    tracing::info!(
        "Build finished. Board-specific output files are somewhere under {} (usually starting \
         with nuttx), check the NuttX docs for your board",
        nuttx_directory.display()
    );

    for file in nuttx_directory
        .read_dir()
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry.path().is_file() && entry.file_name().to_string_lossy().starts_with("nuttx")
        })
    {
        tracing::info!("Potential file: {}", file.path().display());
    }
}

fn fetch_workspace(version: &str) -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .expect("Could not find cache directory")
        .join("fluxemu")
        .join("nuttx-builds")
        .join(version);

    if cache_dir.join("nuttx").exists() {
        tracing::info!(
            "Reusing already-fetched NuttX workspace at {}",
            cache_dir.display()
        );

        return cache_dir;
    }

    fs::create_dir_all(&cache_dir).unwrap();

    for url in [nuttx_url(version), nuttx_apps_url(version)] {
        tracing::info!("Fetching {}", url);

        let response = ureq::get(url.as_str()).call().unwrap();
        let body = response.into_body();

        let gz_archive = GzDecoder::new(body.into_reader());
        let mut tar_archive = tar::Archive::new(gz_archive);

        tar_archive
            .unpack(&cache_dir)
            .expect("Could not unpack tar archive");
    }

    cache_dir
}

fn copy_cargo_workspace(destination: &Path) {
    fs::create_dir_all(destination).unwrap();

    run(Command::new("rsync")
        .arg("-a")
        .arg("--delete")
        .arg("--filter=:- .gitignore")
        .arg("--exclude=.git")
        .arg("./")
        .arg(destination));
}

fn register_app(nuttx_apps_directory: &Path) {
    let nuttx_external_directory = nuttx_apps_directory.join("external");
    fs::create_dir_all(&nuttx_external_directory).unwrap();

    let makefile = nuttx_external_directory.join("Makefile");
    if !makefile.exists() {
        fs::write(
            &makefile,
            "MENUDESC = \"External Apps\"
            include $(APPDIR)/Directory.mk",
        )
        .unwrap();
    }

    let make_defs = nuttx_external_directory.join("Make.defs");
    if !make_defs.exists() {
        fs::write(
            &make_defs,
            "include $(wildcard $(APPDIR)/external/*/Make.defs)",
        )
        .unwrap();
    }

    let workspace_copy = nuttx_external_directory.join("fluxemu");
    copy_cargo_workspace(&workspace_copy);

    let nuttallite_workspace_copy = workspace_copy.join("shell").join("nuttallite");

    let build_glue = nuttallite_workspace_copy
        .join("nuttx")
        .join("config")
        .join("build");
    for entry in fs::read_dir(&build_glue)
        .expect("Failed to enumerate NuttX build glue files")
        .flatten()
    {
        tracing::debug!(
            "Linking NuttX build glue file {} into shim directory",
            entry.path().display()
        );

        std::os::unix::fs::symlink(entry.path(), workspace_copy.join(entry.file_name()))
            .unwrap_or_else(|err| {
                panic!(
                    "failed to symlink {}: {}",
                    entry.file_name().to_string_lossy(),
                    err
                );
            });
    }

    // Ensure the nightly version nuttallite uses is respected by the NuttX build system
    let _ = fs::remove_file(workspace_copy.join("rust-toolchain.toml"));
    fs::copy(
        nuttallite_workspace_copy.join("rust-toolchain.toml"),
        workspace_copy.join("rust-toolchain.toml"),
    )
    .unwrap();
}

fn sh(
    working_directory: &Path,
    command: &str,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
) {
    run(Command::new(command)
        .current_dir(working_directory)
        // Make sure that this env var isn't overwriting our rust-toolchain.toml file
        .env_remove("RUSTUP_TOOLCHAIN")
        .args(arguments));
}

fn run(command: &mut Command) {
    tracing::info!("Running {:?}", command);

    let status = command.status().expect("failed to spawn");
    assert!(status.success(), "command failed: {:?}", command);
}

fn write_boot_script(nuttx_apps_directory: &Path) {
    let staging = tempdir().unwrap();
    let init_directory = staging.path().join("etc").join("init.d");

    fs::create_dir_all(&init_directory).unwrap();
    fs::write(init_directory.join("rcS"), "fluxemu_shell_nuttallite").unwrap();

    sh(
        staging.path(),
        "genromfs",
        ["-f", "romfs_img", "-d", "etc", "-V", "NSHBOOT"],
    );

    let xxd_output = Command::new("xxd")
        .current_dir(staging.path())
        .args(["-i", "romfs_img"])
        .output()
        .unwrap();
    assert!(xxd_output.status.success(), "xxd failed");

    let header_path = nuttx_apps_directory.join("nshlib").join("nsh_romfsimg.h");
    fs::write(&header_path, xxd_output.stdout).unwrap();
}
