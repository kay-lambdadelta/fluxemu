use std::{
    ffi::OsString,
    fs::{File, read_dir},
    path::{Path, PathBuf},
    thread::JoinHandle,
    time::SystemTime,
};

use egui::{Align, Button, ComboBox, Frame, Layout, ScrollArea, Stroke, TextEdit, TextWrapMode};
use indexmap::IndexMap;
use palette::{
    WithAlpha,
    named::{GREEN, RED},
};
use strum::{AsRefStr, EnumIter};

use crate::{Frontend, FrontendPlatform, MachineInitializationStep, to_egui_color};

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
    pathbar_state: PathBarState,
    current_directory: PathBuf,
    current_directory_contents: IndexMap<OsString, DirectoryEntry>,
    sorting_method: SortingMethod,
    reverse_sorting: bool,
    show_hidden: bool,
    directory_to_navigate_to: Option<PathBuf>,
    refresh_directory_results:
        Option<JoinHandle<Result<IndexMap<OsString, DirectoryEntry>, std::io::Error>>>,
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
        }
    }
}

impl<P: FrontendPlatform> Frontend<P> {
    pub(super) fn handle_file_browser(&mut self, ui: &mut egui::Ui) {
        let FileBrowserState {
            pathbar_state,
            sorting_method,
            reverse_sorting,
            show_hidden,
            current_directory,
            current_directory_contents,
            refresh_directory_results,
            directory_to_navigate_to,
        } = &mut self.file_browser;

        ui.horizontal_top(|ui| {
            #[cfg(any(target_family = "unix", target_os = "windows", target_arch = "wasm32"))]
            if ui
                .button(egui_phosphor::regular::UPLOAD_SIMPLE)
                .on_hover_text("Open native file picker")
                .clicked()
                && self.native_file_picker_dialog_job.is_none()
            {
                let handle = std::thread::spawn(|| {
                    use pollster::FutureExt;

                    rfd::AsyncFileDialog::new().pick_file().block_on()
                });

                self.native_file_picker_dialog_job = Some(handle);
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

                    ui.add_space(2.0);

                    if ui
                        .button(egui_phosphor::regular::PENCIL)
                        .on_hover_text("Manually edit path bar")
                        .clicked()
                    {
                        *pathbar_state = PathBarState::Editing(path.to_string_lossy().into_owned());
                    }
                }
                PathBarState::Editing(pathbar_contents) => {
                    let pathbuf = PathBuf::from(pathbar_contents.trim());

                    let is_real_dir = pathbuf.is_dir() && pathbuf.read_dir().is_ok();

                    // Check if the path the user entered is real and we can read it
                    let edit_box_frame_color =
                        if is_real_dir { GREEN } else { RED }.with_alpha(u8::MAX / 2);

                    Frame::NONE
                        .stroke(Stroke::new(4.0, to_egui_color(edit_box_frame_color)))
                        .corner_radius(2.0)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            let mut edit = TextEdit::singleline(pathbar_contents);
                            edit = edit.desired_width(ui.available_width());

                            // Note that [TextEdit] loses focus when you press enter
                            if ui.add(edit).lost_focus() && is_real_dir {
                                *directory_to_navigate_to = Some(pathbuf);
                            }
                        });
                }
            }
        });

        ui.separator();

        ui.horizontal_top(|ui| {
            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh file browser file listings")
                .clicked()
                && let PathBarState::Normal(path) = &pathbar_state
            {
                *directory_to_navigate_to = Some(path.clone());
            }

            let old_settings = (*sorting_method, *reverse_sorting, *show_hidden);

            if ui
                .button(if *reverse_sorting {
                    egui_phosphor::regular::ARROW_UP
                } else {
                    egui_phosphor::regular::ARROW_DOWN
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

            ui.toggle_value(show_hidden, egui_phosphor::regular::EYE_CLOSED)
                .on_hover_text("Toggle hidden file visiblity");

            if old_settings != (*sorting_method, *reverse_sorting, *show_hidden) {
                *directory_to_navigate_to = Some(current_directory.clone());
            }

            if let Some(job) = refresh_directory_results.as_mut()
                && job.is_finished()
            {
                let job = refresh_directory_results.take().unwrap();

                if let Ok(new_contents) = job.join().unwrap() {
                    *current_directory_contents = new_contents;
                }
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
                            format!("{} {}", egui_phosphor::regular::FOLDER_OPEN, name_str)
                        } else if !*readable {
                            format!("{} {}", name_str, egui_phosphor::regular::LOCK)
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
                                let program_manager = self.program_manager.clone();

                                self.machine_initialization_step =
                                    Some(MachineInitializationStep::CalculatingRomIds {
                                        job: std::thread::spawn(move || {
                                            let rom_id = program_manager.register_external(path)?;

                                            Ok(vec![rom_id])
                                        }),
                                    });
                            }
                        }
                    }
                },
            );
        });

        if let Some(directory_to_navigate_to) = directory_to_navigate_to.take() {
            current_directory_contents.clear();
            *pathbar_state = PathBarState::Normal(directory_to_navigate_to.clone());
            *current_directory = directory_to_navigate_to.clone();

            let sorting_method = *sorting_method;
            let reverse_sorting = *reverse_sorting;

            let handle = std::thread::spawn(move || {
                refresh_current_dir_task(directory_to_navigate_to, sorting_method, reverse_sorting)
            });

            *refresh_directory_results = Some(handle);
        }
    }
}

/// Populates cwd_tracker with the contents of directory
fn refresh_current_dir_task(
    directory: PathBuf,
    sorting_method: SortingMethod,
    reverse: bool,
) -> Result<IndexMap<OsString, DirectoryEntry>, std::io::Error> {
    let mut contents = IndexMap::default();
    let directory_reader = read_dir(&directory)?;

    for entry in directory_reader {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();

        let readable = path_is_readable(&path);
        let is_hidden = path_is_hidden(&path);

        let modified = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or_else(|_| SystemTime::now());

        contents.insert(
            name,
            DirectoryEntry {
                readable,
                modified,
                is_hidden,
                is_directory: path.is_dir(),
            },
        );
    }

    match sorting_method {
        SortingMethod::Name => contents.sort_by_key(|name, _| name.clone()),
        SortingMethod::Modified => contents.sort_by_key(|_, entry| entry.modified),
    };

    if reverse {
        contents.reverse();
    }

    Ok(contents)
}

fn path_is_readable(path: &Path) -> bool {
    if path.is_file() {
        File::open(path).is_ok()
    } else if path.is_dir() {
        path.read_dir().is_ok()
    } else {
        true
    }
}

fn path_is_hidden(path: &std::path::Path) -> bool {
    #[cfg(target_family = "unix")]
    {
        path.file_name()
            .map(|name| name.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

        path.metadata()
            .map(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
            .unwrap_or(false)
    }

    #[cfg(not(any(target_family = "unix", target_os = "windows")))]
    {
        false
    }
}
