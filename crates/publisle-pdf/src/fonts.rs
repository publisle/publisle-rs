use crate::model::{Diagnostic, FontSpec, Result};
use rustybuzz::{Face, UnicodeBuffer};
use std::{collections::BTreeMap, fs};

pub struct Font {
    pub bytes: Vec<u8>,
    pub index: u32,
    pub role: String,
    pub glyphs: BTreeMap<Unit, u16>,
}
// One Unicode shaping cluster is one PDF character, including all its marks.
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
        let supports = |font: &Font| {
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
            value.chars().all(|c| {
                c.is_control()
                    || c == '\u{200d}'
                    || c == '\u{200c}'
                    || face.glyph_index(c).is_some()
            })
        };
        self.fonts
            .iter()
            .position(|f| f.role == role && supports(f))
            .or_else(|| self.fonts.iter().position(supports))
            .ok_or_else(|| {
                Diagnostic::new(
                    "missing-glyph",
                    format!("No pinned font supports {value:?}."),
                )
            })
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
            let font_index = self.select(slice, role)?;
            let font = &mut self.fonts[font_index];
            let face = Face::from_slice(&font.bytes, font.index).unwrap();
            let scale = size / face.units_per_em() as f32;
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(slice);
            buffer.guess_segment_properties();
            buffer.set_direction(if direction {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            let shaped = rustybuzz::shape(&face, &[], buffer);
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
                            format!("Font shaping failed for {slice:?}."),
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
                let unit = Unit {
                    components,
                    advance,
                    unicode: slice[start..end].to_string(),
                };
                let next = u16::try_from(font.glyphs.len() + 1).map_err(|_| {
                    Diagnostic::new(
                        "glyph-limit",
                        "A font exceeded 65534 distinct glyph mappings.",
                    )
                })?;
                let cid = *font.glyphs.entry(unit).or_insert(next);
                logical.push((
                    run.start + start,
                    Glyph {
                        font: font_index,
                        cid,
                        x: origin,
                        y: 0.,
                        advance: advance as f32 * scale,
                    },
                ));
                x += advance as f32 * scale;
            }
            // Physical positions retain visual order; stream characters retain source order.
            glyphs.extend(logical);
        }
        glyphs.sort_by_key(|(offset, _)| *offset);
        Ok(Shaped {
            glyphs: glyphs.into_iter().map(|(_, glyph)| glyph).collect(),
            width: x,
        })
    }
}
