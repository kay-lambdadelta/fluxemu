use std::collections::{BTreeMap, BTreeSet};

use fluxemu_locale::Iso639Alpha3;
use redb::{Key, TypeName, Value};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use versions::Versioning;

use crate::RomId;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Information about a program, for the database
pub enum ProgramInfo {
    /// Version 0
    #[serde(rename = "0")]
    V0 {
        /// Identifiable names of the program
        ///
        /// Preferably these will be the names associated with the below languages, in their original script
        names: BTreeSet<String>,
        /// Paths are unixlike
        filesystem: BTreeMap<RomId, BTreeSet<String>>,
        /// The language this program is associated with
        ///
        /// Note that this is the languages a coherent title supports
        ///
        /// If alternate files are required a different database entry is required
        languages: BTreeSet<Iso639Alpha3>,
        /// The version or revision of the program
        #[serde_as(as = "Option<DisplayFromStr>")]
        version: Option<Versioning>,
    },
}

impl ProgramInfo {
    /// Returns the name of the program
    pub fn names(&self) -> &BTreeSet<String> {
        match self {
            ProgramInfo::V0 { names, .. } => names,
        }
    }

    /// Returns the path of the program
    pub fn filesystem(&self) -> &BTreeMap<RomId, BTreeSet<String>> {
        match self {
            ProgramInfo::V0 { filesystem, .. } => filesystem,
        }
    }

    /// Converts this to the latest version
    pub fn mitigate(mut self) -> Self {
        match &mut self {
            ProgramInfo::V0 { filesystem, .. } => {
                filesystem.retain(|_, paths| !paths.is_empty());

                self
            }
        }
    }
}

impl Value for ProgramInfo {
    type SelfType<'a> = ProgramInfo;

    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        rmp_serde::from_slice(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        rmp_serde::to_vec_named(value).unwrap()
    }

    fn type_name() -> redb::TypeName {
        TypeName::new("program_info")
    }
}

impl Key for ProgramInfo {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        let data1 = Self::from_bytes(data1);
        let data2 = Self::from_bytes(data2);

        data1.cmp(&data2)
    }
}
