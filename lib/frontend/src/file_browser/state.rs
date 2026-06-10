use std::{ffi::OsString, path::PathBuf, thread::JoinHandle, time::SystemTime};

use indexmap::IndexMap;
use strum::{AsRefStr, EnumIter};

#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub readable: bool,
    pub modified: SystemTime,
    pub is_hidden: bool,
    pub is_directory: bool,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, EnumIter, AsRefStr)]
pub enum SortingMethod {
    Name,
    Modified,
}

#[derive(Debug, Clone)]
pub enum PathBarState {
    Normal(PathBuf),
    Editing(String),
}

#[derive(Debug)]
pub struct FileBrowserState {
    pub pathbar_state: PathBarState,
    pub current_directory: PathBuf,
    pub current_directory_contents: IndexMap<OsString, DirectoryEntry>,
    pub sorting_method: SortingMethod,
    pub reverse_sorting: bool,
    pub show_hidden: bool,
    pub directory_to_navigate_to: Option<PathBuf>,

    pub refresh_directory_results:
        Option<JoinHandle<Result<IndexMap<OsString, DirectoryEntry>, std::io::Error>>>,

    #[cfg(feature = "external-file-dialog")]
    pub native_file_picker_dialog_job: Option<JoinHandle<Option<rfd::FileHandle>>>,
}

impl FileBrowserState {
    pub fn new(home_directory: PathBuf) -> Self {
        Self {
            pathbar_state: PathBarState::Normal(home_directory.clone()),
            current_directory: home_directory.clone(),
            sorting_method: SortingMethod::Name,
            reverse_sorting: false,
            show_hidden: false,
            current_directory_contents: IndexMap::default(),
            refresh_directory_results: None,
            directory_to_navigate_to: Some(home_directory),
            #[cfg(feature = "external-file-dialog")]
            native_file_picker_dialog_job: None,
        }
    }
}
