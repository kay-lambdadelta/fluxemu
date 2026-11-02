use std::{error::Error, fs::File, io::BufReader};

use fluxemu_database::ProgramDatabase;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::logiqx::add_to_database;

pub fn import(
    program_database: &ProgramDatabase,
    paths: impl IntoParallelIterator<Item = Result<walkdir::DirEntry, walkdir::Error>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    paths.into_par_iter().try_for_each(|entry| {
        match entry {
            Ok(entry) => {
                let file = File::open(entry.into_path())?;

                add_to_database(program_database, BufReader::new(file))?;
            }
            Err(err) => {
                tracing::error!("Failed to open file: {}", err);
            }
        }

        Ok(())
    })
}
