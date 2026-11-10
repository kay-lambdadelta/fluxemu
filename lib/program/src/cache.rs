use std::{collections::BTreeSet, path::Path};

use jiff::{Span, Timestamp, civil::Date, tz::TimeZone};
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{FileDigest, RomId};

/// Rom id -> Digest of a stored ROM
const DIGEST_TABLE: TableDefinition<RomId, CachedDigest> = TableDefinition::new("digest");

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct CachedDigest {
    digest: FileDigest,
    last_seen: Date,
}

pub struct DigestCache {
    database: Database,
}

impl DigestCache {
    // FIXME: Random amount of time I came up with
    //
    // Needs real world testing
    pub const GRACE_PERIOD_DAYS: u32 = 14;

    pub fn open(path: impl AsRef<Path>) -> Result<Self, redb::Error> {
        let database = Database::create(path)?;

        let transaction = database.begin_write()?;
        transaction.open_table(DIGEST_TABLE)?;
        transaction.commit()?;

        Ok(Self { database })
    }

    pub fn try_open(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();

        let result = match Self::open(path) {
            Err(err) if holds_unusable_data(&err) => {
                tracing::warn!(
                    "Recreating the digest cache at {}, ROMs will be rehashed: {}",
                    path.display(),
                    err
                );

                std::fs::remove_file(path)
                    .map_err(redb::Error::from)
                    .and_then(|_| Self::open(path))
            }
            result => result,
        };

        match result {
            Ok(digest_cache) => Some(digest_cache),
            Err(err) => {
                tracing::error!(
                    "Not using the digest cache at {}, ROMs will be rehashed: {}",
                    path.display(),
                    err
                );

                None
            }
        }
    }

    pub fn get(&self, rom_id: RomId) -> Result<Option<FileDigest>, redb::Error> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(DIGEST_TABLE)?;

        Ok(table
            .get(rom_id)?
            .map(|cached_digest| cached_digest.value().digest))
    }

    pub fn insert(&self, digest: &FileDigest) -> Result<(), redb::Error> {
        let mut transaction = self.database.begin_write()?;

        // It's a cache, the integrity of it isn't very important
        transaction.set_durability(Durability::None)?;

        transaction.open_table(DIGEST_TABLE)?.insert(
            digest.id,
            CachedDigest {
                digest: digest.clone(),
                last_seen: today(),
            },
        )?;
        transaction.commit()?;

        Ok(())
    }

    /// FIXME: Garbage collection is the only cleaning method I can think of, but is unappealing
    pub fn collect_garbage(&self, reachable: &BTreeSet<RomId>) -> Result<(), redb::Error> {
        let today = today();
        let expiry = today.saturating_sub(Span::new().days(Self::GRACE_PERIOD_DAYS));
        let transaction = self.database.begin_write()?;

        {
            let mut table = transaction.open_table(DIGEST_TABLE)?;

            for rom_id in reachable {
                let Some(mut cached_digest) = table.get(*rom_id)?.map(|entry| entry.value()) else {
                    continue;
                };

                if cached_digest.last_seen < today {
                    cached_digest.last_seen = today;
                    table.insert(*rom_id, cached_digest)?;
                }
            }

            table.retain(|_, cached_digest| cached_digest.last_seen >= expiry)?;
        }

        transaction.commit()?;

        Ok(())
    }
}

fn holds_unusable_data(err: &redb::Error) -> bool {
    matches!(
        err,
        redb::Error::Corrupted(_)
            | redb::Error::UpgradeRequired(_)
            | redb::Error::TableTypeMismatch { .. }
            | redb::Error::TypeDefinitionChanged { .. }
            | redb::Error::TableIsMultimap(_)
    )
}

fn today() -> Date {
    Timestamp::now().to_zoned(TimeZone::UTC).date()
}
