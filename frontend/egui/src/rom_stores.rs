use std::collections::{BTreeSet, HashMap};

use egui::{Button, ComboBox, TextEdit, Ui, vec2};
use fluxemu_frontend::{FrontendCore, Platform};
use fluxemu_program::RomStoreSpecification;
use indexmap::IndexMap;
use rust_i18n::t;

#[derive(Debug, Clone)]
struct RomStoreDraft {
    name: String,
    scheme: String,
    options: Vec<(String, String)>,
}

impl RomStoreDraft {
    fn to_specification(&self) -> RomStoreSpecification {
        RomStoreSpecification {
            scheme: self.scheme.clone(),
            options: self
                .options
                .iter()
                .filter(|(key, _)| !key.is_empty())
                .cloned()
                .collect(),
        }
    }
}

impl RomStoreDraft {
    fn new(name: String, specification: &RomStoreSpecification) -> Self {
        let mut options: Vec<_> = specification
            .options
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        options.sort();

        Self {
            name,
            scheme: specification.scheme.clone(),
            options,
        }
    }
}

pub struct RomStoreEditor {
    drafts: Vec<RomStoreDraft>,
    available_schemes: BTreeSet<String>,
    error: Option<String>,
}

impl RomStoreEditor {
    pub fn new(rom_stores: &IndexMap<String, RomStoreSpecification>) -> Self {
        Self {
            drafts: rom_stores
                .iter()
                .map(|(name, specification)| RomStoreDraft::new(name.clone(), specification))
                .collect(),
            available_schemes: RomStoreSpecification::available_schemes(),
            error: None,
        }
    }

    fn specifications(&self) -> IndexMap<String, RomStoreSpecification> {
        self.drafts
            .iter()
            .map(|draft| (draft.name.clone(), draft.to_specification()))
            .collect()
    }

    fn name_validity(&self) -> Vec<bool> {
        let mut name_counts = HashMap::<&str, usize>::default();

        for draft in &self.drafts {
            *name_counts.entry(draft.name.as_str()).or_default() += 1;
        }

        self.drafts
            .iter()
            .map(|draft| !draft.name.trim().is_empty() && name_counts[draft.name.as_str()] == 1)
            .collect()
    }

    fn unused_name(&self) -> String {
        (1..)
            .map(|number| format!("{} {}", t!("rom_stores.store"), number))
            .find(|name| self.drafts.iter().all(|draft| &draft.name != name))
            .unwrap()
    }

    pub fn show<P: Platform>(&mut self, ui: &mut Ui, core: &mut FrontendCore<P>) {
        let name_validity = self.name_validity();
        let mut removed_store = None;

        for (store_index, draft) in self.drafts.iter_mut().enumerate() {
            ui.push_id(store_index, |ui| {
                if show_store(
                    ui,
                    draft,
                    name_validity[store_index],
                    &self.available_schemes,
                ) {
                    removed_store = Some(store_index);
                }
            });
            ui.separator();
        }

        if let Some(store_index) = removed_store {
            self.drafts.remove(store_index);
        }

        let names_are_valid = name_validity.iter().all(|is_valid| *is_valid);
        let specifications = self.specifications();
        let is_modified = !specifications
            .iter()
            .eq(core.environment().rom_stores.iter());

        ui.horizontal_top(|ui| {
            if ui.button(t!("rom_stores.add_store")).clicked() {
                self.drafts.push(RomStoreDraft::new(
                    self.unused_name(),
                    &RomStoreSpecification::filesystem(""),
                ));
            }

            if ui
                .add_enabled(
                    is_modified && names_are_valid,
                    Button::new(t!("rom_stores.apply")),
                )
                .clicked()
            {
                self.error = core
                    .set_rom_stores(specifications)
                    .err()
                    .map(|err| err.to_string());
            }

            if ui
                .add_enabled(is_modified, Button::new(t!("rom_stores.revert")))
                .clicked()
            {
                *self = Self::new(&core.environment().rom_stores);
            }
        });

        if !names_are_valid {
            ui.colored_label(ui.visuals().error_fg_color, t!("rom_stores.invalid_names"));
        }

        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
}

/// Returns true if the store should be removed
fn show_store(
    ui: &mut Ui,
    draft: &mut RomStoreDraft,
    name_is_valid: bool,
    available_schemes: &BTreeSet<String>,
) -> bool {
    let mut remove_store = false;

    ui.horizontal(|ui| {
        let mut name_edit = TextEdit::singleline(&mut draft.name)
            .hint_text(t!("rom_stores.name"))
            .desired_width(160.0);

        if !name_is_valid {
            name_edit = name_edit.text_color(ui.visuals().error_fg_color);
        }

        ui.add(name_edit);
        ui.label(t!("rom_stores.scheme"));

        ComboBox::from_id_salt("scheme")
            .selected_text(draft.scheme.as_str())
            .show_ui(ui, |ui| {
                for scheme in available_schemes {
                    ui.selectable_value(&mut draft.scheme, scheme.clone(), scheme.as_str());
                }
            });

        ui.hyperlink_to(
            t!("rom_stores.documentation"),
            draft.to_specification().documentation_url(),
        );

        remove_store = ui.button(t!("rom_stores.remove_store")).clicked();
    });

    ui.add_space(ui.spacing().item_spacing.y);

    let row_height = ui.spacing().interact_size.y;
    let mut removed_option = None;

    for (option_index, (key, value)) in draft.options.iter_mut().enumerate() {
        ui.push_id(option_index, |ui| {
            ui.horizontal(|ui| {
                let key_width = (ui.available_width() * 0.3).clamp(80.0, 200.0);
                ui.add_sized(
                    [key_width, row_height],
                    TextEdit::singleline(key).hint_text(t!("rom_stores.option_key")),
                );

                let value_width = ui.available_width() - row_height - ui.spacing().item_spacing.x;
                ui.add_sized(
                    [value_width, row_height],
                    TextEdit::singleline(value).hint_text(t!("rom_stores.option_value")),
                );

                if ui
                    .add(Button::new("x").min_size(vec2(row_height, row_height)))
                    .on_hover_text(t!("rom_stores.remove_option"))
                    .clicked()
                {
                    removed_option = Some(option_index);
                }
            });
        });
    }

    if let Some(option_index) = removed_option {
        draft.options.remove(option_index);
    }

    ui.horizontal(|ui| {
        if ui.small_button(t!("rom_stores.add_option")).clicked() {
            draft.options.push(Default::default());
        }
    });

    remove_store
}
