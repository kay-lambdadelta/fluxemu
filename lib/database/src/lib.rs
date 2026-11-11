use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use fluxemu_program::{
    Derivative, DumpStatus, FileDigest, FileMatch, GroupId, Manifest, MatchStrength, ProgramId,
    ProgramInfo, RomFile, RomForm, RomHash, RomHashes, RomId, SystemId, Variant,
    versioned_table_name,
};
use redb::{
    Database, MultimapTableDefinition, MultimapTableHandle, ReadOnlyDatabase, ReadableDatabase,
    ReadableMultimapTable, ReadableTable, TableDefinition, TableHandle, WriteTransaction,
};
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
    RedbCompaction(#[from] redb::CompactionError),
    #[error("{0}")]
    Redb(#[from] redb::Error),
    #[error("Unknown program {0:?}")]
    UnknownProgram(ProgramId),
}

/// Program id -> Program info mapping
pub const PROGRAM_TABLE: TableDefinition<ProgramId, ProgramInfo> =
    TableDefinition::new(versioned_table_name!("program"));
/// Program id -> Known file sets of that program
pub const VARIANT_TABLE: MultimapTableDefinition<ProgramId, Variant> =
    MultimapTableDefinition::new(versioned_table_name!("variant"));
/// Rom id -> Programs with a variant containing that file
pub const ROM_ID_TABLE: MultimapTableDefinition<RomId, ProgramId> =
    MultimapTableDefinition::new(versioned_table_name!("rom_id"));
/// DAT hash -> Programs with a variant containing that file, with or without its header
pub const HASH_TABLE: MultimapTableDefinition<RomHash, ProgramId> =
    MultimapTableDefinition::new(versioned_table_name!("hash"));
/// Clone group -> Program ids in that group, groups of one are not stored
pub const CLONE_TABLE: MultimapTableDefinition<GroupId, ProgramId> =
    MultimapTableDefinition::new(versioned_table_name!("clone"));
/// Program id -> Clone group it belongs to
pub const CLONE_OF_TABLE: TableDefinition<ProgramId, GroupId> =
    TableDefinition::new(versioned_table_name!("clone_of"));
/// Program id -> Programs made from it, such as patches and translations
pub const DERIVATIVE_TABLE: MultimapTableDefinition<ProgramId, Derivative> =
    MultimapTableDefinition::new(versioned_table_name!("derivative"));

macro_rules! open_tables {
    ($transaction:expr) => {
        open_tables!(
            $transaction;
            open_table PROGRAM_TABLE,
            open_multimap_table VARIANT_TABLE,
            open_multimap_table ROM_ID_TABLE,
            open_multimap_table HASH_TABLE,
            open_multimap_table CLONE_TABLE,
            open_table CLONE_OF_TABLE,
            open_multimap_table DERIVATIVE_TABLE,
        )
    };
    ($transaction:expr; $($open:ident $table:ident),* $(,)?) => {{
        let transaction = &$transaction;
        $(transaction.$open($table)?;)*

        [$($table.name()),*]
    }};
}

#[derive(Debug, Clone)]
pub struct FileIdentification {
    pub program_id: ProgramId,
    pub path: String,
    pub file: RomFile,
    pub file_match: FileMatch,
}

impl FileIdentification {
    fn rank(&self) -> (MatchStrength, RomForm, DumpStatus) {
        (
            self.file_match.strength,
            self.file_match.form,
            self.file.status,
        )
    }
}

#[derive(Debug, Clone)]
pub struct ProgramMatch {
    pub manifest: Manifest,
    pub strength: MatchStrength,
}

impl ProgramMatch {
    fn rank(&self) -> (MatchStrength, usize, DumpStatus) {
        (
            self.strength,
            self.manifest.variant.files().len(),
            self.manifest.variant.dump_status(),
        )
    }
}

pub struct ProgramDatabase<D: ReadableDatabase = Database> {
    database: D,
}

impl ProgramDatabase<ReadOnlyDatabase> {
    pub fn read_only(database: ReadOnlyDatabase) -> Result<Arc<Self>, Error> {
        let current_tables = open_tables!(database.begin_read()?);
        warn_about_old_tables(&database, &current_tables)?;

        Ok(Arc::new(Self { database }))
    }
}

impl ProgramDatabase {
    pub fn new(database: Database) -> Result<Arc<Self>, Error> {
        let mut database_transaction = database.begin_write()?;
        database_transaction.set_quick_repair(true);
        let current_tables = open_tables!(database_transaction);
        database_transaction.commit()?;

        warn_about_old_tables(&database, &current_tables)?;

        Ok(Arc::new(Self { database }))
    }
}

impl<D: ReadableDatabase> ProgramDatabase<D> {
    /// Attempts to identify a program from the digests of its files
    pub fn identify_program(&self, digests: &[FileDigest]) -> Result<Vec<ProgramMatch>, Error> {
        let read_transaction = self.database.begin_read()?;

        let rom_id_table = read_transaction.open_multimap_table(ROM_ID_TABLE)?;
        let hash_table = read_transaction.open_multimap_table(HASH_TABLE)?;
        let program_table = read_transaction.open_table(PROGRAM_TABLE)?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        let mut possible_programs = Vec::new();

        for program_id in candidates(&rom_id_table, &hash_table, digests)? {
            possible_programs.extend(variant_matches(
                &program_table,
                &variant_table,
                &program_id,
                digests,
            )?);
        }

        possible_programs.sort_by_key(|program_match| Reverse(program_match.rank()));

        Ok(possible_programs)
    }

    pub fn satisfied_programs(&self, digests: &[FileDigest]) -> Result<Vec<ProgramMatch>, Error> {
        let read_transaction = self.database.begin_read()?;

        let rom_id_table = read_transaction.open_multimap_table(ROM_ID_TABLE)?;
        let hash_table = read_transaction.open_multimap_table(HASH_TABLE)?;
        let program_table = read_transaction.open_table(PROGRAM_TABLE)?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        let mut candidates: BTreeMap<_, Vec<_>> = BTreeMap::new();

        for digest in digests {
            for program_id in digest_candidates(&rom_id_table, &hash_table, digest)? {
                candidates
                    .entry(program_id)
                    .or_default()
                    .push(digest.clone());
            }
        }

        let mut satisfied_programs = Vec::new();

        for (program_id, program_digests) in candidates {
            if let Some(program_match) = best_variant(
                &program_table,
                &variant_table,
                &program_id,
                &program_digests,
            )? {
                satisfied_programs.push(program_match);
            }
        }

        Ok(satisfied_programs)
    }

    pub fn satisfied_program(
        &self,
        program_id: &ProgramId,
        digests: &[FileDigest],
    ) -> Result<Option<ProgramMatch>, Error> {
        let read_transaction = self.database.begin_read()?;
        let program_table = read_transaction.open_table(PROGRAM_TABLE)?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        best_variant(&program_table, &variant_table, program_id, digests)
    }

    pub fn identify_file(&self, digest: &FileDigest) -> Result<Vec<FileIdentification>, Error> {
        let read_transaction = self.database.begin_read()?;

        let rom_id_table = read_transaction.open_multimap_table(ROM_ID_TABLE)?;
        let hash_table = read_transaction.open_multimap_table(HASH_TABLE)?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        let mut identifications: BTreeMap<_, FileIdentification> = BTreeMap::new();

        for program_id in candidates(&rom_id_table, &hash_table, std::slice::from_ref(digest))? {
            for variant in variant_table.get(&program_id)? {
                for (path, file) in variant?.value().files() {
                    let Some(file_match) = file.compare(digest) else {
                        continue;
                    };

                    let identification = FileIdentification {
                        program_id: program_id.clone(),
                        path: path.clone(),
                        file: file.clone(),
                        file_match,
                    };

                    identifications
                        .entry((program_id.clone(), path.clone()))
                        .and_modify(|existing| {
                            if identification.rank() > existing.rank() {
                                *existing = identification.clone();
                            }
                        })
                        .or_insert(identification);
                }
            }
        }

        let mut identifications: Vec<_> = identifications.into_values().collect();
        identifications.sort_by_key(|identification| Reverse(identification.rank()));

        Ok(identifications)
    }

    pub fn program(&self, program_id: &ProgramId) -> Result<Option<ProgramInfo>, Error> {
        let read_transaction = self.database.begin_read()?;
        let program_table = read_transaction.open_table(PROGRAM_TABLE)?;

        Ok(program_table.get(program_id)?.map(|info| info.value()))
    }

    pub fn variants(&self, program_id: &ProgramId) -> Result<Vec<Variant>, Error> {
        let read_transaction = self.database.begin_read()?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        variant_table
            .get(program_id)?
            .map(|variant| Ok(variant?.value()))
            .collect()
    }

    pub fn clones(&self, program_id: &ProgramId) -> Result<Vec<ProgramId>, Error> {
        let read_transaction = self.database.begin_read()?;
        let clone_of_table = read_transaction.open_table(CLONE_OF_TABLE)?;
        let clone_table = read_transaction.open_multimap_table(CLONE_TABLE)?;

        let Some(group_id) = clone_of_table.get(program_id)? else {
            return Ok(vec![program_id.clone()]);
        };

        clone_table
            .get(group_id.value())?
            .map(|member| Ok(member?.value()))
            .collect()
    }

    pub fn derivatives(&self, program_id: &ProgramId) -> Result<Vec<Derivative>, Error> {
        let read_transaction = self.database.begin_read()?;
        let derivative_table = read_transaction.open_multimap_table(DERIVATIVE_TABLE)?;

        derivative_table
            .get(program_id)?
            .map(|derivative| Ok(derivative?.value()))
            .collect()
    }

    pub fn programs(
        &self,
        system: Option<SystemId>,
    ) -> Result<Vec<(ProgramId, ProgramInfo)>, Error> {
        let read_transaction = self.database.begin_read()?;
        let program_table = read_transaction.open_table(PROGRAM_TABLE)?;

        program_table
            .iter()?
            .filter_map(|entry| {
                entry
                    .map(|(program_id, info)| {
                        let program_id = program_id.value();

                        system
                            .is_none_or(|system| program_id.system == system)
                            .then(|| (program_id, info.value()))
                    })
                    .map_err(Error::from)
                    .transpose()
            })
            .collect()
    }

    pub fn rom_digests(&self) -> Result<Vec<FileDigest>, Error> {
        let read_transaction = self.database.begin_read()?;
        let rom_id_table = read_transaction.open_multimap_table(ROM_ID_TABLE)?;
        let variant_table = read_transaction.open_multimap_table(VARIANT_TABLE)?;

        let mut digests = Vec::new();

        for entry in rom_id_table.iter()? {
            let (rom_id, program_ids) = entry?;
            let rom_id = rom_id.value();

            'programs: for program_id in program_ids {
                for variant in variant_table.get(program_id?.value())? {
                    if let Some(file) = variant?
                        .value()
                        .files()
                        .values()
                        .find(|file| file.rom_id == Some(rom_id))
                    {
                        digests.push(FileDigest {
                            id: rom_id,
                            size: file.size,
                            hashes: file.hashes,
                        });

                        break 'programs;
                    }
                }
            }
        }

        Ok(digests)
    }
}

fn fill_missing_hashes(hashes: &mut RomHashes, digest_hashes: &RomHashes) {
    hashes.crc32 = hashes.crc32.or(digest_hashes.crc32);
    hashes.md5 = hashes.md5.or(digest_hashes.md5);
    hashes.sha1 = hashes.sha1.or(digest_hashes.sha1);
    hashes.sha256 = hashes.sha256.or(digest_hashes.sha256);
}

fn warn_about_old_tables(
    database: &impl ReadableDatabase,
    current_tables: &[&str],
) -> Result<(), Error> {
    let read_transaction = database.begin_read()?;
    let old_tables: Vec<_> = read_transaction
        .list_tables()?
        .map(|table| table.name().to_string())
        .chain(
            read_transaction
                .list_multimap_tables()?
                .map(|table| table.name().to_string()),
        )
        .filter(|name| !current_tables.contains(&name.as_str()))
        .collect();

    if !old_tables.is_empty() {
        tracing::warn!(
            "Program database holds tables from another version, they are ignored: {}",
            old_tables.join(", ")
        );
    }

    Ok(())
}

fn best_variant(
    program_table: &impl ReadableTable<ProgramId, ProgramInfo>,
    variant_table: &impl ReadableMultimapTable<ProgramId, Variant>,
    program_id: &ProgramId,
    digests: &[FileDigest],
) -> Result<Option<ProgramMatch>, Error> {
    Ok(
        variant_matches(program_table, variant_table, program_id, digests)?
            .into_iter()
            .min_by_key(|program_match| Reverse(program_match.rank())),
    )
}

fn variant_matches(
    program_table: &impl ReadableTable<ProgramId, ProgramInfo>,
    variant_table: &impl ReadableMultimapTable<ProgramId, Variant>,
    program_id: &ProgramId,
    digests: &[FileDigest],
) -> Result<Vec<ProgramMatch>, Error> {
    let Some(info) = program_table.get(program_id)? else {
        return Ok(Vec::new());
    };
    let info = info.value();

    let mut matches = Vec::new();

    for variant in variant_table.get(program_id)? {
        let variant = variant?.value();

        if let Some(variant_match) = variant.resolve(digests) {
            matches.push(ProgramMatch {
                manifest: Manifest {
                    id: program_id.clone(),
                    info: info.clone(),
                    variant,
                    files: variant_match.files,
                },
                strength: variant_match.strength,
            });
        }
    }

    Ok(matches)
}

fn candidates(
    rom_id_table: &impl ReadableMultimapTable<RomId, ProgramId>,
    hash_table: &impl ReadableMultimapTable<RomHash, ProgramId>,
    digests: &[FileDigest],
) -> Result<BTreeSet<ProgramId>, Error> {
    let mut candidates = BTreeSet::new();

    for digest in digests {
        candidates.extend(digest_candidates(rom_id_table, hash_table, digest)?);
    }

    Ok(candidates)
}

fn digest_candidates(
    rom_id_table: &impl ReadableMultimapTable<RomId, ProgramId>,
    hash_table: &impl ReadableMultimapTable<RomHash, ProgramId>,
    digest: &FileDigest,
) -> Result<BTreeSet<ProgramId>, Error> {
    let mut candidates = BTreeSet::new();

    for program_id in rom_id_table.get(digest.id)? {
        candidates.insert(program_id?.value());
    }

    for hash in digest.hashes.iter() {
        for program_id in hash_table.get(&hash)? {
            candidates.insert(program_id?.value());
        }
    }

    Ok(candidates)
}

impl ProgramDatabase {
    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn write(&self) -> Result<ProgramDatabaseWriter, Error> {
        let mut transaction = self.database.begin_write()?;
        transaction.set_quick_repair(true);

        Ok(ProgramDatabaseWriter { transaction })
    }

    pub fn compact(&mut self) -> Result<bool, Error> {
        Ok(self.database.compact()?)
    }
}

pub struct ProgramDatabaseWriter {
    transaction: WriteTransaction,
}

impl ProgramDatabaseWriter {
    /// This must come before [`Self::insert_variant`], [`Self::set_clone_group`], and [`Self::insert_derivative`]
    /// otherwise you will get an error
    pub fn insert_program(&self, program_id: &ProgramId, info: &ProgramInfo) -> Result<(), Error> {
        self.transaction
            .open_table(PROGRAM_TABLE)?
            .insert(program_id, info)?;

        Ok(())
    }

    pub fn remove_program(&self, program_id: &ProgramId) -> Result<(), Error> {
        self.transaction
            .open_table(PROGRAM_TABLE)?
            .remove(program_id)?;

        self.clear_variants(program_id)?;
        self.remove_from_clone_group(program_id)?;
        self.transaction
            .open_multimap_table(DERIVATIVE_TABLE)?
            .remove_all(program_id)?;

        Ok(())
    }

    pub fn clear_variants(&self, program_id: &ProgramId) -> Result<(), Error> {
        let variants: Vec<_> = self
            .transaction
            .open_multimap_table(VARIANT_TABLE)?
            .remove_all(program_id)?
            .map(|variant| Ok::<_, Error>(variant?.value()))
            .collect::<Result<_, _>>()?;

        for variant in &variants {
            self.unindex_variant(program_id, variant)?;
        }

        Ok(())
    }

    pub fn insert_variant(&self, program_id: &ProgramId, variant: &Variant) -> Result<(), Error> {
        self.ensure_program(program_id)?;

        self.transaction
            .open_multimap_table(VARIANT_TABLE)?
            .insert(program_id, variant)?;

        self.index_variant(program_id, variant)
    }

    pub fn remove_variant(&self, program_id: &ProgramId, variant: &Variant) -> Result<bool, Error> {
        let removed = self
            .transaction
            .open_multimap_table(VARIANT_TABLE)?
            .remove(program_id, variant)?;

        if removed {
            self.unindex_variant(program_id, variant)?;
            self.reindex_program(program_id)?;
        }

        Ok(removed)
    }

    pub fn replace_variant(
        &self,
        program_id: &ProgramId,
        old: &Variant,
        new: &Variant,
    ) -> Result<bool, Error> {
        let removed = self.remove_variant(program_id, old)?;
        self.insert_variant(program_id, new)?;

        Ok(removed)
    }

    pub fn set_clone_group(
        &self,
        members: impl IntoIterator<Item = ProgramId>,
    ) -> Result<(), Error> {
        let members: BTreeSet<_> = members.into_iter().collect();

        for member in &members {
            self.ensure_program(member)?;
            self.remove_from_clone_group(member)?;
        }

        self.insert_clone_group(&members)
    }

    pub fn remove_from_clone_group(&self, program_id: &ProgramId) -> Result<(), Error> {
        let mut clone_of_table = self.transaction.open_table(CLONE_OF_TABLE)?;

        let Some(group_id) = clone_of_table
            .remove(program_id)?
            .map(|group| group.value())
        else {
            return Ok(());
        };

        let remaining: BTreeSet<_> = self
            .transaction
            .open_multimap_table(CLONE_TABLE)?
            .remove_all(&group_id)?
            .map(|member| Ok::<_, Error>(member?.value()))
            .filter(|member| member.as_ref().map_or(true, |member| member != program_id))
            .collect::<Result<_, _>>()?;

        for member in &remaining {
            clone_of_table.remove(member)?;
        }

        drop(clone_of_table);

        self.insert_clone_group(&remaining)
    }

    pub fn insert_derivative(
        &self,
        program_id: &ProgramId,
        derivative: &Derivative,
    ) -> Result<(), Error> {
        self.ensure_program(program_id)?;
        self.ensure_program(&derivative.program)?;

        self.transaction
            .open_multimap_table(DERIVATIVE_TABLE)?
            .insert(program_id, derivative)?;

        Ok(())
    }

    pub fn merge_from<D: ReadableDatabase>(&self, other: &ProgramDatabase<D>) -> Result<(), Error> {
        let read_transaction = other.database.begin_read()?;

        for entry in read_transaction.open_table(PROGRAM_TABLE)?.iter()? {
            let (program_id, info) = entry?;
            self.insert_program(&program_id.value(), &info.value())?;
        }

        for entry in read_transaction
            .open_multimap_table(VARIANT_TABLE)?
            .iter()?
        {
            let (program_id, variants) = entry?;
            let program_id = program_id.value();

            for variant in variants {
                self.insert_variant(&program_id, &variant?.value())?;
            }
        }

        for entry in read_transaction.open_multimap_table(CLONE_TABLE)?.iter()? {
            let (_, members) = entry?;
            let members: Vec<_> = members
                .map(|member| Ok::<_, Error>(member?.value()))
                .collect::<Result<_, _>>()?;

            self.set_clone_group(members)?;
        }

        for entry in read_transaction
            .open_multimap_table(DERIVATIVE_TABLE)?
            .iter()?
        {
            let (program_id, derivatives) = entry?;
            let program_id = program_id.value();

            for derivative in derivatives {
                self.insert_derivative(&program_id, &derivative?.value())?;
            }
        }

        Ok(())
    }

    pub fn assign_rom_id(&self, digest: &FileDigest) -> Result<usize, Error> {
        let candidates = {
            let rom_id_table = self.transaction.open_multimap_table(ROM_ID_TABLE)?;
            let hash_table = self.transaction.open_multimap_table(HASH_TABLE)?;

            digest_candidates(&rom_id_table, &hash_table, digest)?
        };

        let mut assigned = 0;

        for program_id in candidates {
            let variants: Vec<_> = self
                .transaction
                .open_multimap_table(VARIANT_TABLE)?
                .get(&program_id)?
                .map(|variant| Ok::<_, Error>(variant?.value()))
                .collect::<Result<_, _>>()?;

            for variant in variants {
                let mut updated = variant.clone();
                let mut changed = false;

                for file in updated.files_mut().values_mut() {
                    if file.rom_id.is_some() {
                        continue;
                    }

                    let Some(file_match) = file.compare(digest) else {
                        continue;
                    };

                    if file_match.form != RomForm::Canonical
                        || file_match.strength == MatchStrength::Weak
                    {
                        continue;
                    }

                    file.rom_id = Some(digest.id);
                    fill_missing_hashes(&mut file.hashes, &digest.hashes);
                    changed = true;
                    assigned += 1;
                }

                if changed {
                    self.replace_variant(&program_id, &variant, &updated)?;
                }
            }
        }

        Ok(assigned)
    }

    pub fn commit(self) -> Result<(), Error> {
        self.transaction.commit()?;

        Ok(())
    }

    fn ensure_program(&self, program_id: &ProgramId) -> Result<(), Error> {
        if self
            .transaction
            .open_table(PROGRAM_TABLE)?
            .get(program_id)?
            .is_none()
        {
            return Err(Error::UnknownProgram(program_id.clone()));
        }

        Ok(())
    }

    fn insert_clone_group(&self, members: &BTreeSet<ProgramId>) -> Result<(), Error> {
        if members.len() < 2 {
            return Ok(());
        }

        let group_id = GroupId::from_members(members).unwrap();
        let mut clone_table = self.transaction.open_multimap_table(CLONE_TABLE)?;
        let mut clone_of_table = self.transaction.open_table(CLONE_OF_TABLE)?;

        for member in members {
            clone_table.insert(&group_id, member)?;
            clone_of_table.insert(member, &group_id)?;
        }

        Ok(())
    }

    fn index_variant(&self, program_id: &ProgramId, variant: &Variant) -> Result<(), Error> {
        let mut rom_id_table = self.transaction.open_multimap_table(ROM_ID_TABLE)?;
        let mut hash_table = self.transaction.open_multimap_table(HASH_TABLE)?;

        for rom_id in variant.rom_ids() {
            rom_id_table.insert(rom_id, program_id)?;
        }

        for hash in variant.search_hashes() {
            hash_table.insert(&hash, program_id)?;
        }

        Ok(())
    }

    fn unindex_variant(&self, program_id: &ProgramId, variant: &Variant) -> Result<(), Error> {
        let mut rom_id_table = self.transaction.open_multimap_table(ROM_ID_TABLE)?;
        let mut hash_table = self.transaction.open_multimap_table(HASH_TABLE)?;

        for rom_id in variant.rom_ids() {
            rom_id_table.remove(rom_id, program_id)?;
        }

        for hash in variant.search_hashes() {
            hash_table.remove(&hash, program_id)?;
        }

        Ok(())
    }

    fn reindex_program(&self, program_id: &ProgramId) -> Result<(), Error> {
        let variants: Vec<Variant> = self
            .transaction
            .open_multimap_table(VARIANT_TABLE)?
            .get(program_id)?
            .map(|variant| Ok::<_, Error>(variant?.value()))
            .collect::<Result<_, _>>()?;

        for variant in &variants {
            self.index_variant(program_id, variant)?;
        }

        Ok(())
    }
}
