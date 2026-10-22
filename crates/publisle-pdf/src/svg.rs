//! SVGs use exactly the same pinned font set as document text, never host fonts.
use crate::{
    fonts::Fonts,
    model::{Diagnostic, Result},
};
use resvg::usvg::{self, fontdb};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex},
};

fn invalid(message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new("invalid-image", message.to_string())
}

pub(crate) fn font_database(fonts: &Fonts) -> Result<fontdb::Database> {
    let mut database = fontdb::Database::new();
    let mut serif = None;
    let mut sans = None;
    let mut mono = None;
    for font in &fonts.fonts {
        let ids = database.load_font_source(fontdb::Source::Binary(Arc::new(font.bytes.clone())));
        for id in ids {
            let mut face = database
                .face(id)
                .ok_or_else(|| invalid("Invalid SVG font face."))?
                .clone();
            // A TTC may contain many language faces. Only expose the pinned index.
            if face.index != font.index {
                database.remove_face(id);
                continue;
            }
            let family = face
                .families
                .first()
                .ok_or_else(|| invalid("Font has no family name."))?
                .0
                .clone();
            if font.role == "regular" {
                serif.get_or_insert(family.clone());
            }
            if font.role == "mono" {
                mono.get_or_insert(family.clone());
                for alias in [
                    "ui-monospace",
                    "SFMono-Regular",
                    "Menlo",
                    "Consolas",
                    "Courier New",
                ] {
                    face.families
                        .push((alias.into(), fontdb::Language::English_UnitedStates));
                }
            }
            if font.role == "sans" || family == "DejaVu Sans" {
                sans.get_or_insert(family.clone());
                for alias in ["system-ui", "ui-sans-serif", "Arial", "Helvetica"] {
                    face.families
                        .push((alias.into(), fontdb::Language::English_UnitedStates));
                }
            }
            database.remove_face(id);
            database.push_face_info(face);
        }
    }
    let default = serif
        .or_else(|| database.faces().next().map(|f| f.families[0].0.clone()))
        .ok_or_else(|| invalid("No pinned SVG fonts are available."))?;
    database.set_serif_family(default.clone());
    database.set_sans_serif_family(sans.unwrap_or_else(|| default.clone()));
    database.set_monospace_family(mono.unwrap_or(default));
    Ok(database)
}

pub(crate) fn tree(path: &Path, fonts: &Fonts) -> Result<usvg::Tree> {
    let bytes = std::fs::read(path).map_err(invalid)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("SVG exceeds 4 MiB."));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(invalid)?
        .to_ascii_lowercase();
    if [
        "<!doctype",
        "<!entity",
        "<script",
        "<foreignobject",
        "<image",
        "@import",
    ]
    .iter()
    .any(|token| text.contains(token))
    {
        return Err(invalid(
            "SVG contains unsupported active or external content.",
        ));
    }
    let failures = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let mut options = usvg::Options::default();
    // Both resolver entry points must be disabled: the default data: resolver can
    // recursively process embedded SVGs, and the string resolver may read files.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    let database = font_database(fonts)?;
    options.font_family = database.family_name(&fontdb::Family::Serif).into();
    options.fontdb = Arc::new(database);
    let missing = Arc::clone(&failures);
    let selector = usvg::FontResolver::default_font_selector();
    options.font_resolver.select_font = Box::new(move |font, db| {
        let selected = selector(font, db);
        if selected.is_none() {
            missing.lock().unwrap().insert(format!(
                "No pinned face for {}.",
                format!("{:?}", font.families())
                    .chars()
                    .take(512)
                    .collect::<String>()
            ));
        }
        selected
    });
    let missing = Arc::clone(&failures);
    let fallback = usvg::FontResolver::default_fallback_selector();
    options.font_resolver.select_fallback = Box::new(move |character, used, db| {
        let selected = fallback(character, used, db);
        if selected.is_none() && !character.is_control() {
            missing.lock().unwrap().insert(format!(
                "No pinned font can shape U+{:04X} ({character:?}).",
                u32::from(character)
            ));
        }
        selected
    });
    let tree = usvg::Tree::from_data(&bytes, &options).map_err(invalid)?;
    let failures = failures.lock().unwrap();
    if !failures.is_empty() {
        return Err(Diagnostic::new(
            "missing-glyph",
            format!(
                "SVG {}: {}",
                path.display(),
                failures
                    .iter()
                    .take(16)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ));
    }
    let size = tree.size().to_int_size();
    if size.width() > 8000
        || size.height() > 8000
        || u64::from(size.width()) * u64::from(size.height()) > 8_000_000
    {
        return Err(invalid("SVG exceeds the pixel limit."));
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FontSpec;
    fn fonts() -> Fonts {
        let directory = std::env::var_os("PUBLISLE_TEST_FONT_DIR")
            .expect("Set PUBLISLE_TEST_FONT_DIR to the pinned DejaVu test font directory");
        let directory = Path::new(&directory);
        Fonts::load(&[
            FontSpec {
                path: directory.join("DejaVuSerif.ttf"),
                role: "regular".into(),
                index: 0,
            },
            FontSpec {
                path: directory.join("DejaVuSansMono.ttf"),
                role: "mono".into(),
                index: 0,
            },
            FontSpec {
                path: directory.join("DejaVuSans.ttf"),
                role: "sans".into(),
                index: 0,
            },
        ])
        .unwrap()
    }
    #[test]
    fn browser_aliases_and_generic_families_resolve_without_system_fonts() {
        let db = font_database(&fonts()).unwrap();
        for family in [
            fontdb::Family::Monospace,
            fontdb::Family::Name("ui-monospace"),
            fontdb::Family::Name("SFMono-Regular"),
            fontdb::Family::Name("Menlo"),
        ] {
            let id = db
                .query(&fontdb::Query {
                    families: &[family],
                    ..Default::default()
                })
                .unwrap();
            assert!(
                db.face(id)
                    .unwrap()
                    .families
                    .iter()
                    .any(|(name, _)| name == "DejaVu Sans Mono")
            );
        }
        assert!(
            db.query(&fontdb::Query {
                families: &[fontdb::Family::SansSerif],
                ..Default::default()
            })
            .is_some()
        );
        assert!(
            db.query(&fontdb::Query {
                families: &[fontdb::Family::Serif],
                ..Default::default()
            })
            .is_some()
        );
    }
    #[test]
    fn real_fourier_figures_retain_text() {
        let fonts = fonts();
        for name in [
            "fft-spectrum.svg",
            "fft-spectrum-dark.svg",
            "gibbs-overshoot.svg",
            "gibbs-overshoot-dark.svg",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../sites/www/public")
                .join(name);
            let result = tree(&path, &fonts).unwrap();
            assert!(!result.root().children().is_empty(), "{name}");
            assert!(!result.fontdb().is_empty(), "{name}");
        }
    }
    #[test]
    fn unresolved_svg_glyph_is_an_error_not_a_replacement_box() {
        let path =
            std::env::temp_dir().join(format!("publisle-missing-glyph-{}.svg", std::process::id()));
        std::fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"30\"><text y=\"20\" font-family=\"ui-monospace\">&#x10ffff;</text></svg>").unwrap();
        let result = tree(&path, &fonts());
        std::fs::remove_file(path).unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code, "missing-glyph");
        assert!(error.message.contains("U+10FFFF"));
    }
}
