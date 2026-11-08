use std::{
    ffi::{OsStr, OsString},
    fs::Metadata,
    path::PathBuf,
    time::SystemTime,
};

use egui::{
    Align, Button, ComboBox, Context, Frame, Layout, Response, ScrollArea, Sense, Spinner,
    TextEdit, TextWrapMode, Widget,
};
use fluxemu_frontend::{FrontendCore, Platform};
use indexmap::IndexMap;
use palette::{
    WithAlpha,
    named::{BLACK, GREEN, RED},
};
use rust_i18n::t;
use strum::{AsRefStr, EnumIter};
use tokio::{
    fs::{DirEntry, File, read_dir},
    sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel},
};

use crate::{icon, utils::to_egui_color};

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

pub type DirectoryScanMessage = Result<(OsString, DirectoryEntry), std::io::Error>;

#[derive(Debug)]
pub struct FileBrowserState {
    pub pathbar_state: PathBarState,
    pub current_directory: PathBuf,
    pub current_directory_contents: IndexMap<OsString, DirectoryEntry>,
    pub sorting_method: SortingMethod,
    pub reverse_sorting: bool,
    pub show_hidden: bool,
    pub directory_to_navigate_to: Option<PathBuf>,

    pub directory_scan: Option<UnboundedReceiver<DirectoryScanMessage>>,

    #[cfg(feature = "external-file-dialog")]
    pub native_file_picker_dialog_job:
        Option<tokio::task::JoinHandle<Option<Vec<rfd::FileHandle>>>>,
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
            directory_scan: None,
            directory_to_navigate_to: Some(home_directory),
            #[cfg(feature = "external-file-dialog")]
            native_file_picker_dialog_job: None,
        }
    }
}

pub struct FileBrowser<'a, P: Platform> {
    pub state: &'a mut FileBrowserState,
    pub core: &'a mut FrontendCore<P>,
}

impl<P: Platform> Widget for FileBrowser<'_, P> {
    fn ui(mut self, ui: &mut egui::Ui) -> Response {
        let FileBrowserState {
            pathbar_state,
            sorting_method,
            reverse_sorting,
            show_hidden,
            current_directory,
            current_directory_contents,
            directory_scan,
            directory_to_navigate_to,
            #[cfg(feature = "external-file-dialog")]
            native_file_picker_dialog_job,
        } = &mut self.state;

        ui.horizontal_top(|ui| {
            #[cfg(feature = "external-file-dialog")]
            {
                let clicked = ui
                    .button(icon::FOLDER_OPEN)
                    .on_hover_text(t!("browser.open_native_file_dialog"))
                    .clicked();

                match native_file_picker_dialog_job {
                    Some(job) if job.is_finished() => {
                        let job = native_file_picker_dialog_job.take().unwrap();

                        if let Some(file_handles) = self.core.runtime().block_on(job).unwrap()
                            && !file_handles.is_empty()
                        {
                            self.core.load_roms(
                                file_handles
                                    .iter()
                                    .map(|handle| handle.path().to_path_buf())
                                    .collect(),
                            );
                        }
                    }
                    None if clicked => {
                        let context = ui.ctx().clone();

                        let job = self.core.runtime().spawn(async move {
                            let file_handles = rfd::AsyncFileDialog::new().pick_files().await;

                            // Wake the UI so it picks up the result without waiting for input
                            context.request_repaint();

                            file_handles
                        });

                        *native_file_picker_dialog_job = Some(job);
                    }
                    _ => {}
                }
            }

            match pathbar_state {
                PathBarState::Normal(path) => {
                    // Iter over the path segments
                    for (index, path_segment) in path.iter().enumerate() {
                        if index > 1 {
                            ui.label(std::path::MAIN_SEPARATOR_STR);
                        }

                        if ui.button(path_segment.to_string_lossy()).clicked() {
                            *directory_to_navigate_to =
                                Some(PathBuf::from_iter(path.iter().take(index + 1)));
                        }
                    }

                    if ui
                        .button(icon::PENCIL)
                        .on_hover_text("Manually edit path bar")
                        .clicked()
                    {
                        *pathbar_state = PathBarState::Editing(path.to_string_lossy().into_owned());
                    }
                }
                PathBarState::Editing(pathbar_contents) => {
                    let pathbuf = PathBuf::from(pathbar_contents.trim());

                    let is_accessible_dir = pathbuf.is_dir() && pathbuf.read_dir().is_ok();

                    // Check if the path the user entered is real and we can read it
                    let edit_box_frame_color =
                        if is_accessible_dir { GREEN } else { RED }.with_alpha(u8::MAX / 2);

                    Frame::NONE
                        .fill(to_egui_color(edit_box_frame_color))
                        .show(ui, |ui| {
                            // Make sure the background is visible through the text edit
                            ui.visuals_mut().extreme_bg_color = to_egui_color(BLACK.with_alpha(0));

                            let mut edit = TextEdit::singleline(pathbar_contents);
                            edit = edit.desired_width(ui.available_width());

                            // Note that [TextEdit] loses focus when you press enter
                            if ui.add(edit).lost_focus() {
                                if is_accessible_dir {
                                    *directory_to_navigate_to = Some(pathbuf);
                                } else {
                                    tracing::error!(
                                        "{}",
                                        t!("browser.cannot_navigate_to_directory")
                                    );
                                }
                            }
                        });
                }
            }
        });

        ui.separator();

        ui.horizontal_top(|ui| {
            if ui
                .button(icon::REFRESH_CW)
                .on_hover_text("Refresh file browser file listings")
                .clicked()
                && let PathBarState::Normal(path) = &pathbar_state
            {
                *directory_to_navigate_to = Some(path.clone());
            }

            let old_sorting = (*sorting_method, *reverse_sorting);

            if ui
                .button(if *reverse_sorting {
                    icon::ARROW_UP
                } else {
                    icon::ARROW_DOWN
                })
                .on_hover_text("Toggle sort order")
                .clicked()
            {
                *reverse_sorting = !*reverse_sorting;
            }

            ComboBox::from_id_salt("Sorting Method")
                .selected_text(sorting_method.as_ref())
                .show_ui(ui, |ui| {
                    ui.selectable_value(sorting_method, SortingMethod::Name, "Name");
                    ui.selectable_value(sorting_method, SortingMethod::Modified, "Date");
                })
                .response
                .on_hover_text("Swap the file browser sorting method");

            ui.toggle_value(show_hidden, icon::EYE)
                .on_hover_text("Toggle hidden file visiblity");

            let mut needs_sorting = old_sorting != (*sorting_method, *reverse_sorting);

            if let Some(receiver) = directory_scan {
                loop {
                    match receiver.try_recv() {
                        Ok(Ok((name, entry))) => {
                            current_directory_contents.insert(name, entry);
                            needs_sorting = true;
                        }
                        Ok(Err(err)) => {
                            tracing::error!("Could not refresh directory: {}", err);
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            *directory_scan = None;
                            break;
                        }
                    }
                }
            }

            if needs_sorting {
                sort_directory_contents(
                    current_directory_contents,
                    *sorting_method,
                    *reverse_sorting,
                );
            }

            if directory_scan.is_some() {
                ui.add(Spinner::new())
                    .on_hover_text(t!("browser.loading_directory"));
            }
        });

        ScrollArea::vertical().show(ui, |ui| {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    for (
                        name,
                        DirectoryEntry {
                            readable,
                            is_hidden,
                            is_directory,
                            ..
                        },
                    ) in current_directory_contents.iter()
                    {
                        let name_str = name.to_string_lossy();
                        if *is_hidden && !*show_hidden {
                            continue;
                        }

                        let label = if *is_directory {
                            format!("{} {}", icon::FOLDER, name_str)
                        } else if !*readable {
                            format!("{} {}", name_str, icon::LOCK)
                        } else {
                            name_str.to_string()
                        };

                        let button = Button::new(label).wrap_mode(TextWrapMode::Truncate);

                        if ui
                            .add_enabled(*readable, button)
                            .on_disabled_hover_text(
                                "You have no read permissions for this filesystem entry",
                            )
                            .clicked()
                        {
                            let path = current_directory.join(name);
                            if *is_directory {
                                *directory_to_navigate_to = Some(path.clone());
                            } else {
                                self.core.load_roms(vec![path]);
                            }
                        }
                    }

                    if directory_scan.is_some() && current_directory_contents.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add(Spinner::new());
                            ui.label(t!("browser.loading_directory"));
                        });
                    }
                },
            );
        });

        if let Some(directory_to_navigate_to) = directory_to_navigate_to.take() {
            current_directory_contents.clear();

            *pathbar_state = PathBarState::Normal(directory_to_navigate_to.clone());
            *current_directory = directory_to_navigate_to.clone();

            // Dropping the old receiver makes any scan still running stop at its next send
            let (sender, receiver) = unbounded_channel();
            let context = ui.ctx().clone();

            self.core.runtime().spawn(scan_directory_task(
                directory_to_navigate_to,
                sender,
                context,
            ));

            *directory_scan = Some(receiver);
        }

        ui.allocate_rect(ui.min_rect(), Sense::empty())
    }
}

async fn scan_directory_task(
    directory: PathBuf,
    sender: UnboundedSender<DirectoryScanMessage>,
    context: Context,
) {
    let mut directory_reader = match read_dir(&directory).await {
        Ok(directory_reader) => directory_reader,
        Err(err) => {
            let _ = sender.send(Err(err));
            context.request_repaint();
            return;
        }
    };

    loop {
        let message = match directory_reader.next_entry().await {
            Ok(Some(entry)) => Ok(read_directory_entry(entry).await),
            Ok(None) => break,
            Err(err) => Err(err),
        };

        if sender.send(message).is_err() {
            return;
        }

        context.request_repaint();
    }

    // Wake the UI so it sees the disconnect and hides the loading indicator
    drop(sender);
    context.request_repaint();
}

async fn read_directory_entry(entry: DirEntry) -> (OsString, DirectoryEntry) {
    let path = entry.path();
    let metadata = tokio::fs::metadata(&path).await.ok();

    let is_directory = metadata.as_ref().is_some_and(Metadata::is_dir);
    let is_file = metadata.as_ref().is_some_and(Metadata::is_file);

    let modified = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .unwrap_or_else(SystemTime::now);

    let readable = if is_file {
        File::open(&path).await.is_ok()
    } else if is_directory {
        read_dir(&path).await.is_ok()
    } else {
        true
    };

    let name = entry.file_name();
    let is_hidden = path_is_hidden(&name, metadata.as_ref());

    (
        name,
        DirectoryEntry {
            readable,
            modified,
            is_hidden,
            is_directory,
        },
    )
}

fn sort_directory_contents(
    contents: &mut IndexMap<OsString, DirectoryEntry>,
    sorting_method: SortingMethod,
    reverse: bool,
) {
    match sorting_method {
        SortingMethod::Name => contents.sort_unstable_keys(),
        SortingMethod::Modified => contents.sort_unstable_by(|a_name, a, b_name, b| {
            a.modified.cmp(&b.modified).then_with(|| a_name.cmp(b_name))
        }),
    };

    if reverse {
        contents.reverse();
    }
}

fn path_is_hidden(name: &OsStr, metadata: Option<&Metadata>) -> bool {
    // Each platform only looks at one of these
    let _ = (name, metadata);

    #[cfg(target_family = "unix")]
    {
        name.to_string_lossy().starts_with('.')
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

        metadata.is_some_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
    }

    #[cfg(not(any(target_family = "unix", target_os = "windows")))]
    {
        false
    }
}
