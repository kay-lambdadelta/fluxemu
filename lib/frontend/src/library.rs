use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZero,
    sync::Arc,
    thread::available_parallelism,
};

use fluxemu_database::ProgramDatabase;
use fluxemu_program::{
    FileDigest, ListingProblem, Manifest, MatchStrength, ProgramId, ProgramManager, RomId,
    RomIdListing, RomSource,
};
use futures::{StreamExt, stream};
use tokio::{runtime::Handle, sync::mpsc::UnboundedSender, task::spawn_blocking};

use crate::spawn::spawn;

#[derive(Debug, Clone)]
pub struct LibraryEntry {
    pub manifest: Manifest,
    pub identification: Identification,
    pub sources: BTreeSet<RomSource>,
}

pub(crate) enum LibraryUpdate {
    Entry(Box<LibraryEntry>),
    ListingProblem {
        rom_store_name: String,
        problem: ListingProblem,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identification {
    Database(MatchStrength),
    FileName,
    Unknown(RomId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LibraryKey {
    title: String,
    program_id: ProgramId,
}

impl LibraryKey {
    pub(crate) fn new(entry: &LibraryEntry) -> Self {
        Self {
            title: entry.manifest.info.title().text.to_lowercase(),
            program_id: entry.manifest.id.clone(),
        }
    }
}

#[derive(Default)]
struct ScanState {
    digests: Vec<FileDigest>,
    program_digests: BTreeMap<ProgramId, Vec<FileDigest>>,
    used_roms: BTreeSet<RomId>,
    rom_sources: BTreeMap<RomId, BTreeSet<RomSource>>,
}

impl ScanState {
    fn sources(&self, rom_ids: impl IntoIterator<Item = RomId>) -> BTreeSet<RomSource> {
        rom_ids
            .into_iter()
            .filter_map(|rom_id| self.rom_sources.get(&rom_id))
            .flatten()
            .cloned()
            .collect()
    }
}

pub(crate) async fn scan_library(
    runtime: Handle,
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    sender: UnboundedSender<LibraryUpdate>,
) {
    let limit = available_parallelism().map_or(1, NonZero::get);
    let listing = program_manager.rom_ids().await;

    for unlisted_rom_store in &listing.unlisted_rom_stores {
        let _ = sender.send(LibraryUpdate::ListingProblem {
            rom_store_name: unlisted_rom_store.name().to_string(),
            problem: ListingProblem::CannotList,
        });
    }

    for (rom_store_name, error) in &listing.failed_rom_stores {
        let _ = sender.send(LibraryUpdate::ListingProblem {
            rom_store_name: rom_store_name.clone(),
            problem: ListingProblem::Failed(error.clone()),
        });
    }

    let rom_sources = if listing.unlisted_rom_stores.is_empty() {
        listing.rom_sources
    } else {
        probe_unlisted_rom_stores(&program_manager, &program_database, listing, limit).await
    };
    let rom_ids: BTreeSet<_> = rom_sources.keys().copied().collect();

    let mut state = ScanState {
        rom_sources,
        ..Default::default()
    };
    let mut digests = stream::iter(rom_ids.iter().copied())
        .map({
            let program_manager = program_manager.clone();

            move |rom_id| {
                let program_manager = program_manager.clone();

                spawn(&runtime, async move {
                    (rom_id, program_manager.digest(rom_id).await)
                })
            }
        })
        .buffer_unordered(limit);

    while let Some(result) = digests.next().await {
        if sender.is_closed() {
            return;
        }

        add_digest(&program_database, &sender, &mut state, result.unwrap()).await;
    }

    drop(digests);

    spawn_blocking({
        let program_manager = program_manager.clone();

        move || program_manager.collect_digest_cache_garbage(&rom_ids)
    })
    .await
    .unwrap();

    for digest in &state.digests {
        if sender.is_closed() {
            return;
        }

        if state.used_roms.contains(&digest.id) {
            continue;
        }

        match program_manager.auto_generate_specification(digest.id).await {
            Ok(Some(manifest)) => {
                let identification = if manifest.id.main_name == digest.id.to_string() {
                    Identification::Unknown(digest.id)
                } else {
                    Identification::FileName
                };

                let _ = sender.send(LibraryUpdate::Entry(Box::new(LibraryEntry {
                    manifest,
                    identification,
                    sources: state.sources([digest.id]),
                })));
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("Could not describe ROM {}: {}", digest.id, err);
            }
        }
    }
}

async fn probe_unlisted_rom_stores(
    program_manager: &ProgramManager,
    program_database: &Arc<ProgramDatabase>,
    listing: RomIdListing,
    limit: usize,
) -> BTreeMap<RomId, BTreeSet<RomSource>> {
    let RomIdListing {
        mut rom_sources,
        unlisted_rom_stores,
        ..
    } = listing;

    let known_digests: BTreeMap<_, _> = match spawn_blocking({
        let program_database = program_database.clone();

        move || program_database.rom_digests()
    })
    .await
    .unwrap()
    {
        Ok(known_digests) => known_digests
            .into_iter()
            .map(|digest| (digest.id, digest))
            .collect(),
        Err(err) => {
            tracing::warn!(
                "Could not read the ROMs known to the program database: {}",
                err
            );

            return rom_sources;
        }
    };

    let mut found = Vec::new();
    let mut failures = 0usize;
    let mut first_error = None;

    let probes: Vec<_> = known_digests
        .keys()
        .flat_map(|rom_id| {
            unlisted_rom_stores
                .iter()
                .map(move |rom_store| (*rom_id, rom_store.clone()))
        })
        .collect();

    let mut probes = stream::iter(probes)
        .map(|(rom_id, rom_store)| async move {
            let result = rom_store.contains(rom_id).await;

            (rom_id, rom_store, result)
        })
        .buffer_unordered(limit);

    while let Some((rom_id, rom_store, result)) = probes.next().await {
        match result {
            Ok(true) => found.push((rom_id, rom_store.name().to_string())),
            Ok(false) => {}
            Err(err) => {
                failures += 1;
                first_error.get_or_insert(err);
            }
        }
    }

    drop(probes);

    if let Some(err) = first_error {
        tracing::warn!(
            "Could not look up {} ROMs in unlisted ROM stores, first error: {}",
            failures,
            err
        );
    }

    for (rom_id, rom_store_name) in found {
        rom_sources
            .entry(rom_id)
            .or_insert_with(|| {
                program_manager.remember_digest_in_memory(known_digests[&rom_id].clone());

                BTreeSet::new()
            })
            .insert(RomSource::Store(rom_store_name));
    }

    rom_sources
}

async fn add_digest(
    program_database: &Arc<ProgramDatabase>,
    sender: &UnboundedSender<LibraryUpdate>,
    state: &mut ScanState,
    (rom_id, result): (RomId, Result<Option<FileDigest>, fluxemu_program::Error>),
) {
    let digest = match result {
        Ok(Some(digest)) => digest,
        Ok(None) => {
            tracing::warn!("ROM {} disappeared while scanning the library", rom_id);

            return;
        }
        Err(err) => {
            tracing::error!("Could not read ROM {}: {}", rom_id, err);

            return;
        }
    };

    state.digests.push(digest.clone());

    let candidates: BTreeSet<_> = match spawn_blocking({
        let program_database = program_database.clone();
        let digest = digest.clone();

        move || program_database.identify_file(&digest)
    })
    .await
    .unwrap()
    {
        Ok(identifications) => identifications
            .into_iter()
            .map(|identification| identification.program_id)
            .collect(),
        Err(err) => {
            tracing::warn!("Could not identify ROM {}: {}", rom_id, err);

            return;
        }
    };

    for program_id in candidates {
        let program_digests = state.program_digests.entry(program_id.clone()).or_default();
        program_digests.push(digest.clone());
        let program_digests = program_digests.clone();

        let satisfied = spawn_blocking({
            let program_database = program_database.clone();

            move || program_database.satisfied_program(&program_id, &program_digests)
        })
        .await
        .unwrap();

        match satisfied {
            Ok(Some(program_match)) => {
                state.used_roms.extend(
                    program_match
                        .manifest
                        .files
                        .values()
                        .map(|file_match| file_match.rom_id),
                );

                let sources = state.sources(
                    program_match
                        .manifest
                        .files
                        .values()
                        .map(|file_match| file_match.rom_id),
                );

                let _ = sender.send(LibraryUpdate::Entry(Box::new(LibraryEntry {
                    manifest: program_match.manifest,
                    identification: Identification::Database(program_match.strength),
                    sources,
                })));
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("Could not resolve a library program: {}", err);
            }
        }
    }
}
