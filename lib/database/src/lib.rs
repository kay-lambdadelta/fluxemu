use std::sync::Arc;

use fluxemu_program::{Manifest, ProgramId, ProgramInfo, RomId};
use redb::{Database, MultimapTableDefinition, ReadableDatabase};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    RedbTransaction(#[from] redb::TransactionError),
    #[error("{0}")]
    RedbTable(#[from] redb::TableError),
    #[error("{0}")]
    RedbStorage(#[from] redb::StorageError),
    #[error("{0}")]
    RedbCommit(#[from] redb::CommitError),
    #[error("{0}")]
    Redb(#[from] redb::Error),
}

/// Program id -> Program info mapping
pub const PROGRAM_INFORMATION_TABLE: MultimapTableDefinition<ProgramId, ProgramInfo> =
    MultimapTableDefinition::new("program_information");
/// Hash -> Program id reverse mapping
pub const HASH_ALIAS_TABLE: MultimapTableDefinition<RomId, ProgramId> =
    MultimapTableDefinition::new("hash_alias");

pub struct ProgramDatabase {
    database: Database,
}

impl ProgramDatabase {
    pub fn new(database: Database) -> Result<Arc<Self>, Error> {
        let mut database_transaction = database.begin_write()?;
        database_transaction.set_quick_repair(true);
        database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;
        database_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;
        database_transaction.commit()?;

        Ok(Arc::new(Self { database }))
    }

    /// Attempts to identify a program from its program ids
    pub fn identify_program(
        &self,
        roms: impl IntoIterator<Item = RomId> + Clone,
    ) -> Result<Vec<Manifest>, Error> {
        let read_transaction = self.database.begin_read()?;

        let hash_alias_table = read_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;
        let program_info_table = read_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

        let mut possible_programs = Vec::default();

        for rom_id in roms.clone() {
            for access_guard in hash_alias_table.get(rom_id)? {
                let program_id = access_guard?.value();

                for access_guard in program_info_table.get(&program_id)? {
                    let program_info = access_guard?.value();

                    let found_all = roms
                        .clone()
                        .into_iter()
                        .all(|id| program_info.filesystem().contains_key(&id));

                    if found_all {
                        possible_programs.push(Manifest {
                            id: program_id.clone(),
                            info: program_info,
                        });
                    }
                }
            }
        }

        Ok(possible_programs)
    }

    pub fn database(&self) -> &Database {
        &self.database
    }
}
