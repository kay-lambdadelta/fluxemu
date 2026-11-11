use std::{error::Error, num::NonZero, sync::Arc, thread::available_parallelism};

use fluxemu_database::ProgramDatabase;
use fluxemu_program::{FileDigest, ProgramManager, RomSource};
use futures::{StreamExt, stream};
use tokio::task::spawn_blocking;

pub async fn reindex(
    program_database: Arc<ProgramDatabase>,
    program_manager: Arc<ProgramManager>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let listing = program_manager.rom_ids().await;
    let rom_ids: Vec<_> = listing
        .rom_sources
        .into_iter()
        .filter(|(_, sources)| {
            sources
                .iter()
                .any(|source| matches!(source, RomSource::Store(_)))
        })
        .map(|(rom_id, _)| rom_id)
        .collect();

    tracing::info!("Reindexing {} ROMs", rom_ids.len());

    let limit = available_parallelism().map_or(1, NonZero::get);
    let mut digests = Vec::with_capacity(rom_ids.len());
    let mut results = stream::iter(rom_ids)
        .map(|rom_id| {
            let program_manager = program_manager.clone();

            tokio::spawn(async move { (rom_id, program_manager.digest(rom_id).await) })
        })
        .buffer_unordered(limit);

    while let Some(result) = results.next().await {
        match result? {
            (_, Ok(Some(digest))) => digests.push(digest),
            (rom_id, Ok(None)) => {
                tracing::warn!("ROM {} disappeared while reindexing", rom_id);
            }
            (rom_id, Err(err)) => tracing::warn!("Could not read ROM {}: {}", rom_id, err),
        }
    }

    record_rom_ids(program_database, digests).await
}

pub(crate) async fn record_rom_ids(
    program_database: Arc<ProgramDatabase>,
    digests: Vec<FileDigest>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rom_count = digests.len();
    let assigned = spawn_blocking(move || {
        let writer = program_database.write()?;
        let mut assigned = 0;

        for digest in &digests {
            assigned += writer.assign_rom_id(digest)?;
        }

        writer.commit()?;

        Ok::<_, fluxemu_database::Error>(assigned)
    })
    .await??;

    tracing::info!(
        "Recorded ROM IDs for {} database files from {} ROMs",
        assigned,
        rom_count
    );

    Ok(())
}
