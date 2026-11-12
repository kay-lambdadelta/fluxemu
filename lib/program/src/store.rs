use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use opendal::OperatorRegistry;
#[cfg(feature = "filesystem")]
use opendal::services::{FILE_SCHEME, FS_SCHEME};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomStoreSpecification {
    pub scheme: String,
    #[serde(default)]
    pub options: HashMap<String, String>,
}

impl RomStoreSpecification {
    #[cfg(feature = "filesystem")]
    pub fn filesystem(root: impl Into<PathBuf>) -> Self {
        Self {
            scheme: FS_SCHEME.to_string(),
            options: HashMap::from_iter([(
                "root".to_string(),
                root.into().to_string_lossy().into_owned(),
            )]),
        }
    }

    pub fn available_schemes() -> BTreeSet<String> {
        OperatorRegistry::get().schemes().into_iter().collect()
    }

    pub fn documentation_url(&self) -> String {
        let scheme = cfg_select! {
            feature = "filesystem" => {
                if self.scheme == FILE_SCHEME {
                    FS_SCHEME
                } else {
                    &self.scheme
                }
            }
            _ => &self.scheme,
        };

        format!("https://opendal.apache.org/services/{}", scheme)
    }

    pub fn filesystem_root(&self) -> Option<PathBuf> {
        cfg_select! {
            feature = "filesystem" => {
                if self.scheme != FS_SCHEME && self.scheme != FILE_SCHEME {
                    return None;
                }

                self.options.get("root").map(PathBuf::from)
            }
            _ => None,
        }
    }
}
