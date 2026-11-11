use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    iter,
    ops::RangeInclusive,
    sync::Arc,
    time::Duration,
};

use byte_unit::{Byte, UnitType};
use egui::{
    Align, Align2, Atom, AtomExt, Button, CentralPanel, Color32, ComboBox, ContainerAtom,
    CornerRadius, Direction, FontId, Frame, Galley, Grid, Id, Image, Key, Layout, Modal, Panel,
    Rect, Response, RichText, ScrollArea, Sense, StrokeKind, TextEdit, TextFormat, TextStyle,
    TextWrapMode, TextureHandle, Ui, Widget, pos2, text::LayoutJob, vec2,
};
use egui_virtual_list::VirtualList;
use fluxemu_frontend::{FrontendCore, Identification, LibraryEntry, Platform};
use fluxemu_locale::Iso639Alpha3;
use fluxemu_program::{
    ListingProblem, Manifest, MatchStrength, ProgramFlag, ProgramId, Region, RomSource,
};
use nalgebra::Vector2;
use rust_i18n::t;

use crate::icon;

const PREFERRED_CARD_WIDTH_EM: f32 = 10.0;
const CARDS_PER_ROW: RangeInclusive<usize> = 1..=10;
const MINIMUM_SEARCH_WIDTH_EM: f32 = 8.0;
const COVER_ASPECT_RATIO: Vector2<f32> = Vector2::new(3.0, 4.0);
const PLACEHOLDER_ICON_SIZE: f32 = 48.0;
const TITLE_ROWS: usize = 2;
const SHORT_ROM_ID_LENGTH: usize = 12;
const SCAN_REPAINT_INTERVAL: Duration = Duration::from_millis(100);
const DETAILS_COVER_WIDTH: f32 = 240.0;
const DETAILS_WIDTH_FRACTION: f32 = 0.8;
const BADGE_FONT_SCALE: f32 = 0.8;

#[derive(Default)]
pub struct LibraryState {
    search: String,
    language: Option<Iso639Alpha3>,
    flag: Option<ProgramFlag>,
    store: Option<RomSource>,
    system: Option<String>,
    details: Option<ProgramId>,
    covers: BTreeMap<ProgramId, TextureHandle>,
    program_list: VirtualList,
    program_list_filter: ProgramListFilter,
    page: LibraryPage,
}

#[derive(Default, PartialEq, Eq)]
enum LibraryPage {
    #[default]
    Systems,
    Programs,
}

#[derive(Default, PartialEq)]
struct ProgramListFilter {
    system: Option<String>,
    search: String,
    language: Option<Iso639Alpha3>,
    flag: Option<ProgramFlag>,
    store: Option<RomSource>,
    length: usize,
}

pub struct Library<'a, P: Platform> {
    pub state: &'a mut LibraryState,
    pub core: &'a mut FrontendCore<P>,
}

impl<P: Platform> Widget for Library<'_, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let scanning = self.core.is_library_scanning();
        let back_requested = self.state.details.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && !ui.ctx().any_popup_open()
            && ui.input(|input| input.key_pressed(Key::Escape));

        let header = ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !scanning,
                    Button::new(RichText::new(icon::REFRESH_CW).size(20.0)),
                )
                .on_hover_text(t!("library.refresh"))
                .clicked()
            {
                self.core.refresh_library();
            }

            let mut languages: Vec<_> = self
                .core
                .library()
                .flat_map(|entry| entry.manifest.info.languages())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            languages.sort_by_key(Iso639Alpha3::english_name);

            ComboBox::from_id_salt("library_language")
                .selected_text(language_filter_label(self.state.language))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.state.language,
                        None,
                        language_filter_label(None),
                    );

                    for language in languages {
                        ui.selectable_value(
                            &mut self.state.language,
                            Some(language),
                            language.english_name(),
                        );
                    }
                });

            let flags: BTreeSet<_> = self
                .core
                .library()
                .flat_map(|entry| entry.manifest.info.flags())
                .copied()
                .collect();

            ComboBox::from_id_salt("library_flag")
                .selected_text(flag_filter_label(self.state.flag))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.state.flag, None, flag_filter_label(None));

                    for flag in flags {
                        ui.selectable_value(
                            &mut self.state.flag,
                            Some(flag),
                            flag_filter_label(Some(flag)),
                        );
                    }
                });

            let mut stores: Vec<_> = self
                .core
                .environment()
                .rom_stores
                .keys()
                .cloned()
                .map(RomSource::Store)
                .collect();

            let other_sources: BTreeSet<_> = self
                .core
                .library()
                .flat_map(|entry| &entry.sources)
                .filter(|source| !matches!(source, RomSource::Store(_)))
                .cloned()
                .collect();
            stores.extend(other_sources);

            if let Some(store) = &self.state.store
                && !stores.contains(store)
            {
                self.state.store = None;
            }

            let listing_problems = self.core.listing_problems();

            ComboBox::from_id_salt("library_store")
                .selected_text(store_filter_label(
                    ui,
                    self.state.store.as_ref(),
                    listing_problems,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.state.store,
                        None,
                        store_filter_label(ui, None, listing_problems),
                    );

                    for store in stores {
                        let label = store_filter_label(ui, Some(&store), listing_problems);
                        ui.selectable_value(&mut self.state.store, Some(store), label);
                    }
                });

            let minimum_search_width =
                MINIMUM_SEARCH_WIDTH_EM * TextStyle::Body.resolve(ui.style()).size;

            if ui.available_size_before_wrap().x < minimum_search_width {
                ui.end_row();
            }

            ui.label(icon::SEARCH);
            ui.add(
                TextEdit::singleline(&mut self.state.search)
                    .hint_text(t!("library.search"))
                    .desired_width(ui.available_size_before_wrap().x),
            );
        });

        if scanning {
            ui.ctx().request_repaint_after(SCAN_REPAINT_INTERVAL);
        }

        for (name, problem) in self.core.listing_problems() {
            if let Some(RomSource::Store(store)) = &self.state.store
                && store != name
            {
                continue;
            }

            show_listing_problem(ui, name, problem);
        }

        let search = self.state.search.to_lowercase();
        let mut systems: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let mut count = 0;

        for entry in self.core.library() {
            if !search.is_empty() && !matches_search(entry, &search) {
                continue;
            }

            if let Some(language) = self.state.language
                && !entry.manifest.info.languages().contains(&language)
            {
                continue;
            }

            if let Some(flag) = self.state.flag
                && !entry.manifest.info.flags().contains(&flag)
            {
                continue;
            }

            if let Some(store) = &self.state.store
                && !entry.sources.contains(store)
            {
                continue;
            }

            systems
                .entry(entry.manifest.id.system.to_nointro_string())
                .or_default()
                .push(entry);
            count += 1;
        }

        if let Some(system) = &self.state.system
            && !systems.contains_key(system.as_str())
        {
            self.state.system = None;
        }

        let shown: BTreeMap<_, _> = systems
            .iter()
            .filter(|(system_name, _)| {
                self.state
                    .system
                    .as_deref()
                    .is_none_or(|system| system == **system_name)
            })
            .map(|(system_name, entries)| (*system_name, entries.as_slice()))
            .collect();
        let shown_count: usize = shown.values().map(|entries| entries.len()).sum();

        ui.horizontal(|ui| {
            ui.weak(format!("{} {}", shown_count, t!("library.programs")));

            if scanning {
                ui.spinner();
                ui.weak(t!("library.scanning"));
            }
        });

        if count == 0 && !scanning {
            ui.centered_and_justified(|ui| {
                ui.weak(t!("library.empty"));
            });

            return header.response;
        }

        let mut launch = None;

        let all_systems: BTreeSet<_> = self
            .core
            .library()
            .map(|entry| entry.manifest.id.system.to_nointro_string())
            .collect();
        let panel_width = systems_panel_width(ui, &all_systems);
        let central_margin = Frame::central_panel(ui.style()).total_margin().sum().x;
        let scroll_bar_width = ui.spacing().scroll.allocated_width();
        let two_panes = ui.available_width() - panel_width - central_margin - scroll_bar_width
            >= preferred_card_width(ui);

        if two_panes {
            Panel::left("library_systems")
                .resizable(false)
                .exact_size(panel_width)
                .show(ui, |ui| show_systems(ui, &mut self.state.system, &systems));

            CentralPanel::default_margins().show(ui, |ui| {
                show_programs(ui, self.state, &shown, &mut launch);
            });
        } else {
            match self.state.page {
                LibraryPage::Systems => {
                    if show_systems(ui, &mut self.state.system, &systems) {
                        self.state.page = LibraryPage::Programs;
                    }
                }
                LibraryPage::Programs => {
                    let back_button =
                        ui.button((icon::ARROW_LEFT, t!("library.systems").into_owned()));

                    if back_requested || back_button.clicked() {
                        self.state.page = LibraryPage::Systems;
                    }

                    show_programs(ui, self.state, &shown, &mut launch);
                }
            }
        }

        if let Some(details) = self.state.details.clone() {
            match self
                .core
                .library()
                .find(|entry| entry.manifest.id == details)
            {
                Some(entry) => {
                    let cover = self.state.covers.get(&details);

                    if show_details(ui, entry, cover, &mut launch) {
                        self.state.details = None;
                    }
                }
                None => self.state.details = None,
            }
        }

        if launch.is_some() {
            self.state.details = None;
        }

        if let Some(manifest) = launch {
            self.core.launch_program(manifest);
        }

        header.response
    }
}

fn split_system_name(system_name: &str) -> (&str, &str) {
    system_name.split_once(" - ").unwrap_or(("", system_name))
}

fn show_systems(
    ui: &mut Ui,
    selected: &mut Option<String>,
    systems: &BTreeMap<&str, Vec<&LibraryEntry>>,
) -> bool {
    let mut picked = false;

    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            picked |= ui
                .selectable_value(selected, None, t!("library.all_systems"))
                .clicked();

            let mut current_vendor = None;

            for system_name in systems.keys() {
                let (vendor, name) = split_system_name(system_name);

                if current_vendor != Some(vendor) {
                    ui.add_space(ui.spacing().item_spacing.y);
                    ui.weak(vendor);
                    current_vendor = Some(vendor);
                }

                let is_selected = selected.as_deref() == Some(*system_name);
                let button =
                    Button::selectable(is_selected, name).wrap_mode(TextWrapMode::Truncate);

                if ui.add(button).on_hover_text(*system_name).clicked() {
                    *selected = Some(system_name.to_string());
                    picked = true;
                }
            }
        });
    });

    picked
}

fn systems_panel_width(ui: &Ui, systems: &BTreeSet<&str>) -> f32 {
    let style = ui.style();
    let button_font = TextStyle::Button.resolve(style);
    let body_font = TextStyle::Body.resolve(style);
    let text_width = |text: &str, font: &FontId| {
        ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
        })
    };

    let all_systems = t!("library.all_systems");
    let button_width = iter::once(all_systems.as_ref())
        .chain(
            systems
                .iter()
                .map(|system_name| split_system_name(system_name).1),
        )
        .map(|name| text_width(name, &button_font))
        .fold(0.0, f32::max)
        + style.spacing.button_padding.x * 2.0;
    let vendor_width = systems
        .iter()
        .map(|system_name| text_width(split_system_name(system_name).0, &body_font))
        .fold(0.0, f32::max);

    button_width.max(vendor_width)
        + Frame::side_top_panel(style).total_margin().sum().x
        + style.spacing.scroll.allocated_width()
}

fn preferred_card_width(ui: &Ui) -> f32 {
    PREFERRED_CARD_WIDTH_EM * TextStyle::Body.resolve(ui.style()).size
}

enum ProgramListItem<'a> {
    Heading { system_name: &'a str, count: usize },
    Program(&'a LibraryEntry),
}

fn show_programs(
    ui: &mut Ui,
    state: &mut LibraryState,
    systems: &BTreeMap<&str, &[&LibraryEntry]>,
    launch: &mut Option<Manifest>,
) {
    let items: Vec<_> = systems
        .iter()
        .flat_map(|(system_name, entries)| {
            iter::once(ProgramListItem::Heading {
                system_name,
                count: entries.len(),
            })
            .chain(entries.iter().map(|entry| ProgramListItem::Program(entry)))
        })
        .collect();

    let filter = ProgramListFilter {
        system: state.system.clone(),
        search: state.search.clone(),
        language: state.language,
        flag: state.flag,
        store: state.store.clone(),
        length: items.len(),
    };

    if state.program_list_filter != filter {
        state.program_list.reset();
        state.program_list_filter = filter;
    }

    ScrollArea::vertical()
        .id_salt(("library_programs", &state.system))
        .auto_shrink(false)
        .show(ui, |ui| {
            let card_spacing = ui.spacing().item_spacing.x;
            ui.spacing_mut().item_spacing.y = card_spacing;

            let columns =
                card_columns(ui.available_width(), card_spacing, preferred_card_width(ui));
            let card_width = card_width(ui.available_width(), card_spacing, columns);

            state
                .program_list
                .ui_custom_layout(ui, items.len(), |ui, index| match items[index] {
                    ProgramListItem::Heading { system_name, count } => {
                        ui.vertical_centered(|ui| ui.label(system_heading(ui, system_name, count)));
                        ui.separator();

                        1
                    }
                    ProgramListItem::Program(_) => {
                        let row =
                            items[index..]
                                .iter()
                                .take(columns)
                                .map_while(|item| match item {
                                    ProgramListItem::Program(entry) => Some(*entry),
                                    ProgramListItem::Heading { .. } => None,
                                });

                        ui.horizontal(|ui| {
                            let mut length = 0;

                            for entry in row {
                                let id = &entry.manifest.id;
                                let card = ui
                                    .push_id(id, |ui| {
                                        show_card(ui, entry, state.covers.get(id), card_width)
                                    })
                                    .inner;

                                if card.clicked() {
                                    *launch = Some(entry.manifest.clone());
                                }

                                if card.secondary_clicked()
                                    || (card.has_focus()
                                        && ui.input(|input| input.key_pressed(Key::Menu)))
                                {
                                    state.details = Some(id.clone());
                                }

                                length += 1;
                            }

                            length
                        })
                        .inner
                    }
                });
        });
}

fn card_columns(available_width: f32, card_spacing: f32, preferred_card_width: f32) -> usize {
    CARDS_PER_ROW
        .filter_map(|columns| {
            let width = card_width(available_width, card_spacing, columns);
            let distance = (width / preferred_card_width).ln().abs();

            (width > 0.0).then_some((columns, distance))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map_or(*CARDS_PER_ROW.start(), |(columns, _)| columns)
}

fn card_width(available_width: f32, card_spacing: f32, columns: usize) -> f32 {
    (available_width - card_spacing * (columns - 1) as f32) / columns as f32
}

fn matches_search(entry: &LibraryEntry, search: &str) -> bool {
    let manifest = &entry.manifest;

    [
        manifest.info.title().text.as_str(),
        manifest.id.main_name.as_str(),
        manifest.id.system.to_nointro_string(),
    ]
    .iter()
    .any(|text| text.to_lowercase().contains(search))
}

fn system_heading(ui: &Ui, system_name: &str, count: usize) -> LayoutJob {
    let mut heading = LayoutJob::default();

    heading.append(
        system_name,
        0.0,
        TextFormat {
            font_id: TextStyle::Heading.resolve(ui.style()),
            color: ui.visuals().strong_text_color(),
            valign: Align::BOTTOM,
            ..Default::default()
        },
    );
    heading.append(
        &format!("{} {}", count, t!("library.programs")),
        ui.spacing().item_spacing.x,
        TextFormat {
            font_id: TextStyle::Body.resolve(ui.style()),
            color: ui.visuals().weak_text_color(),
            valign: Align::BOTTOM,
            ..Default::default()
        },
    );

    heading
}

fn card_title(entry: &LibraryEntry) -> String {
    match entry.identification {
        Identification::Database(_) | Identification::FileName => {
            entry.manifest.info.title().text.clone()
        }
        Identification::Unknown(rom_id) => {
            let mut short_rom_id = rom_id.to_string();
            short_rom_id.truncate(SHORT_ROM_ID_LENGTH);

            format!("{} {}", t!("library.unknown_rom"), short_rom_id)
        }
    }
}

fn show_card(
    ui: &mut Ui,
    entry: &LibraryEntry,
    cover: Option<&TextureHandle>,
    card_width: f32,
) -> Response {
    let cover_width = (card_width - ui.spacing().button_padding.x * 2.0).max(0.0);
    let cover_size = vec2(
        cover_width,
        cover_width * COVER_ASPECT_RATIO.y / COVER_ASPECT_RATIO.x,
    );
    let weak_match = entry.identification == Identification::Database(MatchStrength::Weak);
    let icon_inset = ui.spacing().icon_spacing;

    let cover = Atom::paint(cover_size, move |ui, arguments| {
        paint_cover(ui, arguments.rect, cover);

        // Add a warning symbol to games we are not sure about
        if weak_match {
            ui.painter().text(
                arguments.rect.right_top() + vec2(-icon_inset, icon_inset),
                Align2::RIGHT_TOP,
                icon::TRIANGLE_ALERT,
                FontId::proportional(20.0),
                ui.visuals().warn_fg_color,
            );
        }
    });

    let title = card_text(
        ui,
        card_title(entry),
        Color32::PLACEHOLDER,
        TITLE_ROWS,
        cover_width,
    );
    let tags = card_badges(
        ui,
        entry
            .manifest
            .info
            .flags()
            .iter()
            .map(|flag| enum_label("flag", *flag))
            .collect(),
        cover_width,
    );
    let languages = card_badges(
        ui,
        entry
            .manifest
            .info
            .languages()
            .iter()
            .map(language_code)
            .collect(),
        cover_width,
    );

    let contents =
        ContainerAtom::new((cover, title, tags, languages)).direction(Direction::TopDown);

    ui.scope(|ui| {
        let spacing = ui.spacing_mut();

        // Library items come out as visually incorrect with the default button padding
        spacing.button_padding.y = spacing.button_padding.x;

        ui.add(Button::new(Atom::container(contents)))
    })
    .inner
}

fn card_badges(ui: &Ui, labels: Vec<String>, width: f32) -> Atom<'static> {
    let body = TextStyle::Body.resolve(ui.style());
    let font = FontId::new(body.size * BADGE_FONT_SCALE, body.family);
    let padding = ui.spacing().button_padding;
    let badge_gap = ui.spacing().icon_spacing;
    let badge_width = move |galley: &Galley| galley.size().x + padding.x * 2.0;
    let layout = |text: String| {
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(text, font.clone(), Color32::PLACEHOLDER))
    };
    let gap = |shown: &[Arc<Galley>]| if shown.is_empty() { 0.0 } else { badge_gap };

    let total = labels.len();
    let mut shown = Vec::new();
    let mut used_width = 0.0;

    for label in labels {
        let galley = layout(label);
        let needed_width = badge_width(&galley) + gap(&shown);

        if used_width + needed_width > width {
            break;
        }

        used_width += needed_width;
        shown.push(galley);
    }

    while shown.len() < total {
        let overflow = layout(format!("+{}", total - shown.len()));
        let needed_width = badge_width(&overflow) + gap(&shown);

        if used_width + needed_width <= width || shown.is_empty() {
            shown.push(overflow);
            break;
        }

        let removed = shown.pop().unwrap();
        used_width -= badge_width(&removed) + gap(&shown);
    }

    let height = ui.fonts_mut(|fonts| fonts.row_height(&font)) + padding.y * 2.0;

    Atom::paint(vec2(width, height), move |ui, arguments| {
        let visuals = ui.visuals();
        let mut left = arguments.rect.left();

        for galley in &shown {
            let rect = Rect::from_min_size(
                pos2(left, arguments.rect.top()),
                vec2(badge_width(galley), height),
            );

            ui.painter().rect(
                rect,
                CornerRadius::same(3),
                visuals.extreme_bg_color,
                visuals.widgets.noninteractive.bg_stroke,
                StrokeKind::Inside,
            );
            ui.painter().galley(
                pos2(
                    rect.left() + padding.x,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley.clone(),
                visuals.text_color(),
            );

            left = rect.right() + badge_gap;
        }
    })
}

fn card_text(ui: &Ui, text: String, color: Color32, rows: usize, width: f32) -> Atom<'static> {
    let mut job = LayoutJob::simple(text, TextStyle::Body.resolve(ui.style()), color, width);
    job.wrap.max_rows = rows;

    Atom::from(ui.fonts_mut(|fonts| fonts.layout_job(job)))
        .atom_size(vec2(
            width,
            ui.text_style_height(&TextStyle::Body) * rows as f32,
        ))
        .atom_align(Align2::LEFT_TOP)
}

fn show_details(
    ui: &Ui,
    entry: &LibraryEntry,
    cover: Option<&TextureHandle>,
    launch: &mut Option<Manifest>,
) -> bool {
    let manifest = &entry.manifest;
    let info = &manifest.info;
    let mut close = false;

    let modal = Modal::new(Id::unique("library_details")).show(ui.ctx(), |ui| {
        ui.set_max_width(ui.ctx().content_rect().width() * DETAILS_WIDTH_FRACTION);

        ui.horizontal_top(|ui| {
            let cover_size = vec2(
                DETAILS_COVER_WIDTH,
                DETAILS_COVER_WIDTH * COVER_ASPECT_RATIO.y / COVER_ASPECT_RATIO.x,
            );
            let (cover_rect, _) = ui.allocate_exact_size(cover_size, Sense::hover());
            paint_cover(ui, cover_rect, cover);

            ui.vertical(|ui| {
                ui.heading(card_title(entry));
                ui.weak(&manifest.id.main_name);
                ui.weak(manifest.id.system.to_nointro_string());
                ui.add_space(ui.spacing().item_spacing.y);

                Grid::new("library_details_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let mut field = |label: Cow<'_, str>, value: String| {
                            if !value.is_empty() {
                                ui.weak(label);
                                ui.label(value);
                                ui.end_row();
                            }
                        };

                        field(
                            t!("library.details.identification"),
                            identification_label(entry),
                        );
                        field(
                            t!("library.details.regions"),
                            join(info.regions().iter().map(region_label)),
                        );
                        field(
                            t!("library.details.languages"),
                            join(
                                info.languages()
                                    .iter()
                                    .map(|language| language.english_name().to_string()),
                            ),
                        );
                        field(
                            t!("library.details.revision"),
                            info.revision().unwrap_or_default().to_string(),
                        );
                        field(t!("library.details.kind"), enum_label("kind", info.kind()));
                        field(
                            t!("library.details.flags"),
                            join(info.flags().iter().map(|flag| enum_label("flag", *flag))),
                        );
                        field(
                            t!("library.details.serials"),
                            join(info.serials().iter().cloned()),
                        );
                    });

                ui.add_space(ui.spacing().item_spacing.y);
                ui.weak(t!("library.details.files"));

                Grid::new("library_details_files")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (path, file) in manifest.variant.files() {
                            ui.label(path);
                            ui.weak(format!(
                                "{:#.1}",
                                Byte::from_u64(file.size).get_appropriate_unit(UnitType::Binary)
                            ));
                            ui.end_row();
                        }
                    });
            });
        });

        ui.add_space(ui.spacing().item_spacing.y);

        ui.horizontal(|ui| {
            let launch_button = ui.button(t!("library.details.launch"));

            if ui.memory(|memory| memory.focused().is_none()) {
                launch_button.request_focus();
            }

            if launch_button.clicked() {
                *launch = Some(manifest.clone());
            }

            if ui.button(t!("library.details.close")).clicked() {
                close = true;
            }
        });
    });

    close || modal.should_close()
}

fn paint_cover(ui: &Ui, cover_rect: Rect, cover: Option<&TextureHandle>) {
    match cover {
        Some(cover) => {
            Image::new(cover)
                .maintain_aspect_ratio(true)
                .fit_to_exact_size(cover_rect.size())
                .paint_at(ui, cover_rect);
        }
        None => {
            let painter = ui.painter();

            painter.rect_filled(
                cover_rect,
                CornerRadius::same(4),
                ui.visuals().extreme_bg_color,
            );
            painter.text(
                cover_rect.center(),
                Align2::CENTER_CENTER,
                icon::GAMEPAD_2,
                FontId::proportional(PLACEHOLDER_ICON_SIZE),
                ui.visuals().weak_text_color(),
            );
        }
    }
}

fn identification_label(entry: &LibraryEntry) -> String {
    match entry.identification {
        Identification::Database(MatchStrength::Canonical) => t!("library.identification.exact"),
        Identification::Database(MatchStrength::Strong(_)) => t!("library.identification.strong"),
        Identification::Database(MatchStrength::Weak) => t!("library.identification.weak"),
        Identification::FileName => t!("library.not_in_database"),
        Identification::Unknown(_) => t!("library.identification.unknown"),
    }
    .into_owned()
}

fn region_label(region: &Region) -> String {
    match region {
        Region::Country(country) => country.to_string().to_uppercase(),
        Region::World => t!("library.region.world").into_owned(),
        Region::Europe => t!("library.region.europe").into_owned(),
        Region::Asia => t!("library.region.asia").into_owned(),
        Region::LatinAmerica => t!("library.region.latin_america").into_owned(),
        Region::Scandinavia => t!("library.region.scandinavia").into_owned(),
    }
}

fn enum_label(category: &str, value: impl Into<&'static str>) -> String {
    let key = format!("library.{category}.{}", value.into());

    t!(&key).into_owned()
}

fn language_code(language: &Iso639Alpha3) -> String {
    match language.to_alpha2() {
        Some(language) => language.to_string(),
        None => language.to_string(),
    }
    .to_uppercase()
}

fn language_filter_label(language: Option<Iso639Alpha3>) -> String {
    match language {
        Some(language) => language.english_name().to_string(),
        None => t!("library.all_languages").into_owned(),
    }
}

fn flag_filter_label(flag: Option<ProgramFlag>) -> String {
    match flag {
        Some(flag) => enum_label("flag", flag),
        None => t!("library.all_flags").into_owned(),
    }
}

fn store_filter_label(
    ui: &Ui,
    store: Option<&RomSource>,
    listing_problems: &BTreeMap<String, ListingProblem>,
) -> RichText {
    match store {
        None => RichText::new(t!("library.all_stores")),
        Some(RomSource::Store(name)) if listing_problems.contains_key(name) => {
            RichText::new(format!("{} {}", icon::TRIANGLE_ALERT, name))
                .color(ui.visuals().warn_fg_color)
        }
        Some(RomSource::Store(name)) => RichText::new(name),
        Some(RomSource::External) => RichText::new(t!("library.source.external")),
        Some(RomSource::Embedded) => RichText::new(t!("library.source.embedded")),
    }
}

fn show_listing_problem(ui: &mut Ui, name: &str, problem: &ListingProblem) {
    let color = ui.visuals().warn_fg_color;

    let message = match problem {
        ListingProblem::CannotList => t!("library.listing_problem.cannot_list", name = name),
        ListingProblem::Failed(_) => t!("library.listing_problem.failed", name = name),
    };

    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, icon::TRIANGLE_ALERT);
        let label = ui.colored_label(color, message);

        if let ListingProblem::Failed(error) = problem {
            label.on_hover_text(error);
        }
    });
}

fn join(values: impl Iterator<Item = String>) -> String {
    values.collect::<Vec<_>>().join(", ")
}
