use std::{
    error::Error,
    fs::{File, create_dir_all},
    path::Path,
};

use fluxemu_program::{Manifest, PROGRAM_INFORMATION_TABLE, ProgramManager};
use rayon::iter::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use redb::{ReadableDatabase, ReadableMultimapTable};
use ron::ser::PrettyConfig;

pub fn import(
    program_manager: &ProgramManager,
    paths: impl IntoParallelIterator<Item = Result<walkdir::DirEntry, walkdir::Error>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    paths.into_par_iter().try_for_each(|entry| {
        match entry {
            Ok(entry) => {
                let database_transaction = program_manager.database().begin_write()?;
                let mut database_table =
                    database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

                let file = File::open(entry.path())?;

                match ron::Options::default().from_reader::<_, Manifest>(file) {
                    Ok(program_specification) => {
                        database_table
                            .insert(program_specification.id, program_specification.info)?;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to open file {:?}: {}", entry.path(), err);
                    }
                }
            }
            Err(err) => {
                tracing::error!("Failed to open file: {}", err);
            }
        }

        Ok(())
    })
}

pub fn export(
    program_manager: &ProgramManager,
    output_directory: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = create_dir_all(output_directory);

    let database_transaction = program_manager.database().begin_read()?;
    let database_table = database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

    database_table.iter()?.par_bridge().try_for_each(|item| {
        let (program_id, program_infos) = item?;

        for program_info in program_infos {
            let program_info = program_info?;
            let program_id = program_id.value();

            let specification = Manifest {
                id: program_id.clone(),
                info: program_info.value(),
            };

            let mut output_path = output_directory.join(program_id.to_string());
            output_path.set_extension("ron");

            let output_file = File::create(output_path)?;

            ron::Options::default().to_io_writer_pretty(
                output_file,
                &specification,
                PrettyConfig::default(),
            )?;
        }

        Ok(())
    })
}
