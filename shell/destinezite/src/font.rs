use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};
use fontique::{
    Collection, CollectionOptions, FontStyle, FontWeight, FontWidth, GenericFamily, SourceCache,
};

fn load_face(
    collection: &mut Collection,
    source_cache: &mut SourceCache,
    font_name: impl AsRef<str>,
    generic: GenericFamily,
) -> Option<(String, Vec<u8>)> {
    let font_name = font_name.as_ref();
    let font_info = collection.family_by_name(font_name).and_then(|family| {
        family
            .match_font(
                FontWidth::NORMAL,
                FontStyle::Normal,
                FontWeight::NORMAL,
                true,
            )
            .cloned()
    });

    if let Some(font_info) = font_info
        && let Some(blob) = font_info.load(Some(source_cache))
    {
        tracing::info!("Loading font: {}", font_name);

        return Some((font_name.to_string(), blob.data().to_vec()));
    }

    tracing::warn!("Could not find desired font: {}", font_name);

    let ids = collection.generic_families(generic).collect::<Vec<_>>();
    let family_id = ids.into_iter().find(|family_id| {
        let Some(family_name) = collection.family_name(*family_id) else {
            return false;
        };

        // FIXME: Find a more robust way to filter color emojis
        family_name != "Noto Color Emoji"
    })?;

    let family_name = collection.family_name(family_id)?.to_string();
    let family = collection.family(family_id)?;

    let font_info = family.match_font(
        FontWidth::NORMAL,
        FontStyle::Normal,
        FontWeight::NORMAL,
        true,
    )?;
    let blob = font_info.load(Some(source_cache))?;

    tracing::warn!("Loading backup font: {}", family_name);

    Some((family_name, blob.data().to_vec()))
}

pub fn load_fonts() -> FontDefinitions {
    let mut collection = Collection::new(CollectionOptions::default());
    let mut source_cache = SourceCache::default();

    let proportional = load_face(
        &mut collection,
        &mut source_cache,
        "Noto Sans",
        GenericFamily::SystemUi,
    );

    let monospace = load_face(
        &mut collection,
        &mut source_cache,
        "Noto Sans Mono",
        GenericFamily::Monospace,
    );

    let emoji = load_face(
        &mut collection,
        &mut source_cache,
        "Noto Emoji",
        GenericFamily::Emoji,
    );

    let mut definitions = FontDefinitions::default();

    if let Some((name, bytes)) = proportional {
        definitions
            .font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));

        definitions
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(name);
    }

    if let Some((name, bytes)) = monospace {
        definitions
            .font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));

        definitions
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push(name);
    }

    if let Some((name, bytes)) = emoji {
        definitions
            .font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));

        definitions
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(name.clone());

        definitions
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push(name);
    }

    definitions
}
