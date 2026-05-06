use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ComponentPath, scheduler::Period};

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub timestamp: Period,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    components: BTreeMap<ComponentPath, ComponentMetadata>,
}
