use crate::model::{Diagnostic, FontSpec, Result};
use rustybuzz::{Face, UnicodeBuffer};
use std::{collections::BTreeMap, fs};

pub struct Font {
    pub bytes: Vec<u8>,
    pub index: u32,
    pub role: String,
    pub glyphs: BTreeMap<Unit, u16>,
}
// PDF character outlines and Unicode mappings. RTL cluster characters retain
// individual positions; Latin ligatures can map to a Unicode sequence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unit {
    pub components: Vec<(u16, i32, i32)>,
    pub advance: i32,
    pub unicode: String,
}
#[derive(Clone, Debug)]
pub struct Glyph {
    pub font: usize,
    pub cid: u16,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}
#[derive(Clone, Debug)]
pub struct Shaped {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
}

pub struct Fonts {
    pub fonts: Vec<Font>,
}
impl Fonts {
    pub fn load(specs: &[FontSpec]) -> Result<Self> {
        if specs.is_empty() || specs.len() > 16 {
            return Err(Diagnostic::new(
                "invalid-fonts",
                "Supply one to sixteen pinned font files.",
            ));
        }
        let mut fonts = Vec::new();
        let mut total = 0;
        for spec in specs {
            let meta = fs::metadata(&spec.path)
                .map_err(|e| Diagnostic::new("font-unavailable", e.to_string()))?;
            total += meta.len();
            if total > 96 * 1024 * 1024 {
                return Err(Diagnostic::new("font-limit", "Pinned fonts exceed 96 MiB."));
            }
            let bytes = fs::read(&spec.path)
                .map_err(|e| Diagnostic::new("font-unavailable", e.to_string()))?;
            let face = Face::from_slice(&bytes, spec.index).ok_or_else(|| {
                Diagnostic::new("invalid-font", "The font face cannot be parsed.")
            })?;
            if face.tables().glyf.is_none()
                && face.tables().cff.is_none()
                && face.tables().cff2.is_none()
            {
                return Err(Diagnostic::new(
                    "unsupported-font",
                    "The native compiler requires OpenType glyph outlines.",
                ));
            }
            fonts.push(Font {
                bytes,
                index: spec.index,
                role: spec.role.clone(),
                glyphs: BTreeMap::new(),
            });
        }
        Ok(Self { fonts })
    }

    fn select(&self, value: &str, role: &str) -> Result<usize> {
        self.fonts
            .iter()
            .position(|f| f.role == role && font_covers(f, value))
            .or_else(|| {
                self.fonts
                    .iter()
                    .position(|f| f.role == "fallback" && font_covers(f, value))
            })
            .or_else(|| self.fonts.iter().position(|f| font_covers(f, value)))
            .ok_or_else(|| {
                Diagnostic::new(
                    "missing-glyph",
                    format!(
                        "No pinned font covers this run: {:?} ({}).",
                        value.chars().take(80).collect::<String>(),
                        value
                            .chars()
                            .take(8)
                            .map(|c| format!("U+{:04X}", u32::from(c)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            })
    }

    /// Whether any pinned font for `role` (then fallback, then any face) maps
    /// every character. Line breaks and joiners are treated as covered so they
    /// do not split a run; other controls, including tab, are not.
    pub(crate) fn covers(&self, value: &str, role: &str) -> bool {
        self.fonts
            .iter()
            .any(|font| font.role == role && font_covers(font, value))
            || self
                .fonts
                .iter()
                .any(|font| font.role == "fallback" && font_covers(font, value))
            || self.fonts.iter().any(|font| font_covers(font, value))
    }

    /// Shape one glyph id. Variant glyphs from an OpenType MATH table are not
    /// cmap-mapped, so they cannot go through [`Fonts::shape`].
    pub fn shape_glyph(
        &mut self,
        glyph_id: u16,
        size: f32,
        role: &str,
        unicode: &str,
    ) -> Result<Shaped> {
        let font_index = self
            .fonts
            .iter()
            .position(|font| font.role == role)
            .ok_or_else(|| {
                Diagnostic::new("missing-glyph", format!("No pinned font has role {role}."))
            })?;
        let font = &mut self.fonts[font_index];
        let face = Face::from_slice(&font.bytes, font.index).unwrap();
        let advance = face
            .glyph_hor_advance(ttf_parser::GlyphId(glyph_id))
            .unwrap_or(0);
        let scale = size / face.units_per_em() as f32;
        let next = u16::try_from(font.glyphs.len() + 1).map_err(|_| {
            Diagnostic::new(
                "glyph-limit",
                "A font exceeded 65534 distinct glyph mappings.",
            )
        })?;
        let cid = *font
            .glyphs
            .entry(Unit {
                components: vec![(glyph_id, 0, 0)],
                advance: i32::from(advance),
                unicode: unicode.to_string(),
            })
            .or_insert(next);
        let width = f32::from(advance) * scale;
        Ok(Shaped {
            glyphs: vec![Glyph {
                font: font_index,
                cid,
                x: 0.,
                y: 0.,
                advance: width,
            }],
            width,
        })
    }

    /// Maximal substrings of a bidi slice that [`Fonts::select`] assigns to one font.
    /// A slice one font already covers is not split, so Arabic joining stays intact.
    fn font_runs<'a>(&self, slice: &'a str, role: &str) -> Result<Vec<(usize, &'a str, usize)>> {
        if slice.is_empty() {
            return Ok(Vec::new());
        }
        if let Ok(font) = self.select(slice, role) {
            return Ok(vec![(0, slice, font)]);
        }
        let mut runs = Vec::new();
        let mut pending_start = 0usize;
        let mut current: Option<usize> = None;
        for (offset, ch) in slice.char_indices() {
            if matches!(ch, '\n' | '\r' | '\u{200c}' | '\u{200d}') {
                continue;
            }
            let mut buf = [0u8; 4];
            let font = self.select(ch.encode_utf8(&mut buf), role)?;
            match current {
                Some(previous) if previous == font => {}
                Some(previous) => {
                    runs.push((pending_start, offset, previous));
                    pending_start = offset;
                    current = Some(font);
                }
                None => {
                    pending_start = if runs.is_empty() { 0 } else { offset };
                    current = Some(font);
                }
            }
        }
        let Some(font) = current else {
            return self.select(slice, role).map(|font| vec![(0, slice, font)]);
        };
        runs.push((pending_start, slice.len(), font));
        Ok(runs
            .into_iter()
            .filter(|(start, end, _)| start < end)
            .map(|(start, end, font)| (start, &slice[start..end], font))
            .collect())
    }

    pub fn shape(&mut self, value: &str, size: f32, role: &str, rtl: bool) -> Result<Shaped> {
        if value.is_empty() {
            return Ok(Shaped {
                glyphs: Vec::new(),
                width: 0.0,
            });
        }
        // Shape complete script runs so Arabic joining and ligatures survive.
        let bidi = unicode_bidi::BidiInfo::new(
            value,
            Some(if rtl {
                unicode_bidi::Level::rtl()
            } else {
                unicode_bidi::Level::ltr()
            }),
        );
        let paragraph = bidi
            .paragraphs
            .first()
            .ok_or_else(|| Diagnostic::new("invalid-text", "Text has no bidi paragraph."))?;
        let (_, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        for run in runs {
            let slice = &value[run.clone()];
            let direction = bidi.levels[run.start].is_rtl();
            // Split only where font coverage changes, keeping joining scripts together.
            let parts = self.font_runs(slice, role)?;
            for (part_start, part, font_index) in parts {
                let (logical, advance) =
                    self.shape_font_slice(part, font_index, size, direction, x)?;
                for (offset, glyph) in logical {
                    glyphs.push((run.start + part_start + offset, glyph));
                }
                x += advance;
            }
        }
        glyphs.sort_by_key(|(offset, _)| *offset);
        Ok(Shaped {
            glyphs: glyphs.into_iter().map(|(_, glyph)| glyph).collect(),
            width: x,
        })
    }

    fn shape_font_slice(
        &mut self,
        slice: &str,
        font_index: usize,
        size: f32,
        direction: bool,
        x_origin: f32,
    ) -> Result<(Vec<(usize, Glyph)>, f32)> {
        let font = &mut self.fonts[font_index];
        let face = Face::from_slice(&font.bytes, font.index).unwrap();
        let scale = size / face.units_per_em() as f32;
        let true_type = face.tables().glyf.is_some();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(slice);
        buffer.guess_segment_properties();
        buffer.set_direction(if direction {
            rustybuzz::Direction::RightToLeft
        } else {
            rustybuzz::Direction::LeftToRight
        });
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let mut x = x_origin;
        let mut boundaries: Vec<usize> = shaped
            .glyph_infos()
            .iter()
            .map(|i| i.cluster as usize)
            .collect();
        boundaries.push(slice.len());
        boundaries.sort_unstable();
        boundaries.dedup();
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        let mut logical = Vec::new();
        let mut i = 0;
        while i < infos.len() {
            let start = infos[i].cluster as usize;
            let end = boundaries
                .iter()
                .copied()
                .find(|b| *b > start)
                .unwrap_or(slice.len());
            let origin = x;
            let mut advance = 0;
            let mut components = Vec::new();
            while i < infos.len() && infos[i].cluster as usize == start {
                let info = &infos[i];
                let pos = &positions[i];
                if info.glyph_id == 0 {
                    return Err(Diagnostic::new(
                        "missing-glyph",
                        format!(
                            "Font shaping failed for {:?}.",
                            slice.chars().take(80).collect::<String>()
                        ),
                    ));
                }
                components.push((info.glyph_id as u16, advance + pos.x_offset, pos.y_offset));
                advance += pos.x_advance;
                i += 1;
            }
            if components.len() > 64 {
                return Err(Diagnostic::new(
                    "glyph-limit",
                    "A shaping cluster exceeds 64 glyphs.",
                ));
            }
            let characters: Vec<_> = slice[start..end].char_indices().collect();
            let is_mark = |c| {
                unicode_bidi::bidi_class(c) == unicode_bidi::BidiClass::NSM
                    || c == '\u{200c}'
                    || c == '\u{200d}'
            };
            let bases = characters.iter().filter(|(_, c)| !is_mark(*c)).count();
            let mut emit = |unit: Unit, offset: usize, position: i32| -> Result<()> {
                let unit_advance = unit.advance;
                let next = u16::try_from(font.glyphs.len() + 1).map_err(|_| {
                    Diagnostic::new(
                        "glyph-limit",
                        "A font exceeded 65534 distinct glyph mappings.",
                    )
                })?;
                let cid = *font.glyphs.entry(unit).or_insert(next);
                logical.push((
                    start + offset,
                    Glyph {
                        font: font_index,
                        cid,
                        x: origin + position as f32 * scale,
                        y: 0.,
                        advance: unit_advance as f32 * scale,
                    },
                ));
                Ok(())
            };
            if direction && true_type && characters.len() > 1 && bases > 0 {
                // Give each Unicode scalar a real character position. Mapping
                // an Arabic ligature/mark cluster to one PDF character lets
                // geometry-based readers reverse its *internal* characters.
                // Only the first CID paints the original shaped outline;
                // the remaining CIDs are empty glyphs with real Unicode maps.
                let mut base = 0;
                let mut previous_left = 0;
                for (number, (offset, character)) in characters.into_iter().enumerate() {
                    let (position, width) = if is_mark(character) {
                        (previous_left - 1, 0)
                    } else {
                        let left = advance * (bases - base - 1) as i32 / bases as i32;
                        let right = advance * (bases - base) as i32 / bases as i32;
                        base += 1;
                        previous_left = left;
                        (left, right - left)
                    };
                    let components = if number == 0 {
                        components
                            .iter()
                            .map(|(gid, dx, dy)| (*gid, dx - position, *dy))
                            .collect()
                    } else {
                        vec![]
                    };
                    emit(
                        Unit {
                            components,
                            advance: width,
                            unicode: character.to_string(),
                        },
                        offset,
                        position,
                    )?;
                }
            } else {
                emit(
                    Unit {
                        components,
                        advance,
                        unicode: slice[start..end].to_string(),
                    },
                    0,
                    0,
                )?;
            }
            x += advance as f32 * scale;
        }
        Ok((logical, x - x_origin))
    }
}

fn font_covers(font: &Font, value: &str) -> bool {
    let face = Face::from_slice(&font.bytes, font.index).unwrap();
    // Coverage alone is insufficient: math fonts can expose isolated
    // Arabic glyphs without Arabic joining/substitution tables.
    if value
        .chars()
        .any(|c| ('\u{0600}'..='\u{06ff}').contains(&c))
        && !face.tables().gsub.is_some_and(|table| {
            table
                .scripts
                .find(ttf_parser::Tag::from_bytes(b"arab"))
                .is_some()
        })
    {
        return false;
    }
    // Tabs and other controls are not implicitly covered. rustybuzz maps a
    // tab to glyph 0, which used to fail the whole token after select() had
    // accepted it via `char::is_control`.
    value.chars().all(|c| {
        matches!(c, '\n' | '\r' | '\u{200c}' | '\u{200d}') || face.glyph_index(c).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rtl_ligature_and_mark_characters_have_individual_unicode_positions() {
        let directory =
            std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned test fonts are required");
        let mut fonts = Fonts::load(&[FontSpec {
            path: std::path::Path::new(&directory).join("DejaVuSans.ttf"),
            role: "regular".into(),
            index: 0,
        }])
        .unwrap();
        let input = "العَرَبِيَّة لا للاختبار";
        let shaped = fonts.shape(input, 12., "regular", true).unwrap();
        let reconstructed: String = shaped
            .glyphs
            .iter()
            .map(|glyph| {
                let unit = fonts.fonts[glyph.font]
                    .glyphs
                    .iter()
                    .find(|(_, cid)| **cid == glyph.cid)
                    .unwrap()
                    .0;
                assert_eq!(unit.unicode.chars().count(), 1);
                unit.unicode.clone()
            })
            .collect();
        assert_eq!(reconstructed, input);
        assert!(
            (shaped.glyphs.iter().map(|glyph| glyph.advance).sum::<f32>() - shaped.width).abs()
                < 0.01
        );
        assert!(
            fonts.fonts[0]
                .glyphs
                .keys()
                .any(|unit| unit.components.is_empty())
        );
    }

    fn sha256_file(path: &std::path::Path) -> String {
        let output = std::process::Command::new("sha256sum")
            .arg(path)
            .output()
            .expect("sha256sum");
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    }

    fn pinned_font(file_name: &str, url: &str, sha256: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(file_name);
        if path.exists() && sha256_file(&path) == sha256 {
            return path;
        }
        let status = std::process::Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(&path)
            .arg(url)
            .status()
            .expect("curl");
        assert!(status.success(), "font download failed: {url}");
        assert_eq!(sha256_file(&path), sha256);
        path
    }

    #[test]
    fn latin_and_emoji_shape_from_separate_faces() {
        let directory =
            std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned test fonts are required");
        let emoji = pinned_font(
            "NotoEmoji-Regular.ttf",
            "https://github.com/google/fonts/raw/b979dba422e445492b0eb9951ac52ee0b4d648c3/ofl/notoemoji/NotoEmoji%5Bwght%5D.ttf",
            "de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551",
        );
        let mut fonts = Fonts::load(&[
            FontSpec {
                path: std::path::Path::new(&directory).join("DejaVuSerif.ttf"),
                role: "regular".into(),
                index: 0,
            },
            FontSpec {
                path: emoji,
                role: "fallback".into(),
                index: 0,
            },
        ])
        .unwrap();
        let input = "Hi \u{1F600}";
        let shaped = fonts.shape(input, 12., "regular", false).unwrap();
        let reconstructed: String = shaped
            .glyphs
            .iter()
            .map(|glyph| {
                fonts.fonts[glyph.font]
                    .glyphs
                    .iter()
                    .find(|(_, cid)| **cid == glyph.cid)
                    .unwrap()
                    .0
                    .unicode
                    .clone()
            })
            .collect();
        assert_eq!(reconstructed, input);
        assert!(reconstructed.contains('\u{1F600}'));
        assert!(!reconstructed.contains('\u{FFFD}'));
        assert!(shaped.glyphs.iter().any(|glyph| glyph.font == 1));
        assert!(shaped.glyphs.iter().any(|glyph| glyph.font == 0));
        let uncovered = fonts.shape("Hi \u{1F600}\u{0378}", 12., "regular", false);
        assert_eq!(uncovered.unwrap_err().code, "missing-glyph");
    }
}
