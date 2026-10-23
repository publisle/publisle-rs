//! PDF font shaping and box layout for the independent math syntax tree.
use crate::{
    fonts::{Fonts, Shaped},
    model::{Diagnostic, Result},
};
use publisle_math::Math;
pub use publisle_math::xml;
pub fn parse(value: &str) -> Result<Math> {
    publisle_math::parse(value).map_err(|error| Diagnostic::new(&error.code, error.message))
}

#[derive(Clone, Debug)]
pub struct Run {
    pub shaped: Shaped,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct Rule {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub thickness: f32,
}
#[derive(Clone, Debug, Default)]
pub struct BoxLayout {
    pub runs: Vec<Run>,
    pub rules: Vec<Rule>,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub backgrounds: Vec<Background>,
}
#[derive(Clone, Debug)]
pub struct Background {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
    pub border: Option<[f32; 3]>,
}
impl BoxLayout {
    pub fn scale(&mut self, factor: f32) {
        self.width *= factor;
        self.ascent *= factor;
        self.descent *= factor;
        for run in &mut self.runs {
            run.x *= factor;
            run.y *= factor;
            run.size *= factor;
            run.shaped.width *= factor;
            for glyph in &mut run.shaped.glyphs {
                glyph.x *= factor;
                glyph.y *= factor;
                glyph.advance *= factor;
            }
        }
        for rule in &mut self.rules {
            rule.x *= factor;
            rule.y *= factor;
            rule.width *= factor;
            rule.thickness *= factor;
        }
        for background in &mut self.backgrounds {
            background.x *= factor;
            background.y *= factor;
            background.width *= factor;
            background.height *= factor;
        }
    }
    pub fn append(&mut self, mut other: Self, x: f32, y: f32) {
        for run in &mut other.runs {
            run.x += x;
            run.y += y;
        }
        for rule in &mut other.rules {
            rule.x += x;
            rule.y += y;
        }
        for background in &mut other.backgrounds {
            background.x += x;
            background.y += y;
        }
        self.backgrounds.extend(other.backgrounds);
        self.runs.extend(other.runs);
        self.rules.extend(other.rules);
    }
}
pub fn layout(node: &Math, fonts: &mut Fonts, size: f32, color: [f32; 3]) -> Result<BoxLayout> {
    match node {
        Math::Text(text) | Math::Operator(text) | Math::PlainText(text) => {
            let shaped = fonts.shape(text, size, "regular", false)?;
            let width = shaped.width;
            Ok(BoxLayout {
                runs: vec![Run {
                    shaped,
                    x: 0.,
                    y: 0.,
                    size,
                    color,
                }],
                rules: vec![],
                width,
                ascent: size * 0.85,
                descent: size * 0.22,
                backgrounds: vec![],
            })
        }
        Math::Space(w) => Ok(BoxLayout {
            width: w * size,
            ..Default::default()
        }),
        Math::Row(nodes) => {
            let mut out = BoxLayout::default();
            for node in nodes {
                let b = layout(node, fonts, size, color)?;
                let x = out.width;
                out.width += b.width;
                out.ascent = out.ascent.max(b.ascent);
                out.descent = out.descent.max(b.descent);
                out.append(b, x, 0.);
            }
            Ok(out)
        }
        Math::Color(c, node) => layout(node, fonts, size, *c),
        Math::Fraction(a, b) => {
            let a = layout(a, fonts, size * 0.8, color)?;
            let b = layout(b, fonts, size * 0.8, color)?;
            let width = a.width.max(b.width) + size * 0.35;
            let ay = size * 0.3 + a.descent + size * 0.12;
            let by = -size * 0.12 - b.ascent;
            let mut out = BoxLayout {
                width,
                ascent: ay + a.ascent,
                descent: -by + b.descent,
                ..Default::default()
            };
            let ax = (width - a.width) / 2.;
            let bx = (width - b.width) / 2.;
            out.append(a, ax, ay);
            out.append(b, bx, by);
            out.rules.push(Rule {
                x: 0.,
                y: size * 0.18,
                width,
                thickness: (size * 0.045).max(0.35),
            });
            Ok(out)
        }
        Math::Scripts { base, sub, sup } => {
            let mut out = layout(base, fonts, size, color)?;
            if matches!(base.as_ref(), Math::Accent("underbrace", _))
                && sup.is_none()
                && let Some(sub) = sub
            {
                let b = layout(sub, fonts, size * 0.65, color)?;
                let x = (out.width - b.width) / 2.;
                let y = -out.descent - b.ascent - size * 0.1;
                out.descent = -y + b.descent;
                out.append(b, x, y);
                return Ok(out);
            }
            let base_width = out.width;
            let mut script_width: f32 = 0.;
            if let Some(sub) = sub {
                let b = layout(sub, fonts, size * 0.65, color)?;
                let y = -size * 0.3 - b.ascent * 0.25;
                script_width = script_width.max(b.width);
                out.descent = out.descent.max(-y + b.descent);
                out.append(b, base_width, y);
            }
            if let Some(sup) = sup {
                let b = layout(sup, fonts, size * 0.65, color)?;
                let y = size * 0.58;
                script_width = script_width.max(b.width);
                out.ascent = out.ascent.max(y + b.ascent);
                out.append(b, base_width, y);
            }
            out.width += script_width;
            Ok(out)
        }
        Math::IndexedRoot(index, node) => {
            let root = layout(&Math::Root(node.clone()), fonts, size, color)?;
            let index = layout(index, fonts, size * 0.55, color)?;
            let shift = index.width.max(size * 0.2);
            let y = size * 0.6;
            let mut out = BoxLayout {
                width: root.width + shift,
                ascent: root.ascent.max(y + index.ascent),
                descent: root.descent,
                ..Default::default()
            };
            out.append(index, 0., y);
            out.append(root, shift, 0.);
            Ok(out)
        }
        Math::Root(node) => {
            let b = layout(node, fonts, size, color)?;
            let root = layout(&Math::Operator("√".into()), fonts, size * 1.2, color)?;
            let rw = root.width;
            let mut out = BoxLayout {
                width: rw + b.width,
                ascent: b.ascent + size * 0.12,
                descent: b.descent,
                ..Default::default()
            };
            out.append(root, 0., 0.);
            out.rules.push(Rule {
                x: rw,
                y: b.ascent + size * 0.08,
                width: b.width,
                thickness: size * 0.04,
            });
            out.append(b, rw, 0.);
            Ok(out)
        }
        Math::Accent(kind, node) => {
            let mut out = layout(node, fonts, size, color)?;
            if *kind == "underbrace" {
                out.rules.push(Rule {
                    x: 0.,
                    y: -out.descent - size * 0.12,
                    width: out.width,
                    thickness: size * 0.04,
                });
                out.descent += size * 0.3;
                return Ok(out);
            }
            let y = out.ascent + size * 0.08;
            out.rules.push(Rule {
                x: 0.,
                y,
                width: out.width,
                thickness: size * 0.04,
            });
            if *kind == "arrow" {
                let arrow = layout(&Math::Operator("›".into()), fonts, size * 0.5, color)?;
                let x = out.width - arrow.width;
                out.append(arrow, x, y - size * 0.2);
            }
            out.ascent += size * 0.2;
            Ok(out)
        }
        Math::Over(base, label) => {
            let base = layout(base, fonts, size, color)?;
            let label = layout(label, fonts, size * 0.65, color)?;
            let width = base.width.max(label.width);
            let y = base.ascent + label.descent + size * 0.15;
            let mut out = BoxLayout {
                width,
                ascent: y + label.ascent,
                descent: base.descent,
                ..Default::default()
            };
            let x = (width - base.width) / 2.;
            out.append(base, x, 0.);
            let x = (width - label.width) / 2.;
            out.append(label, x, y);
            Ok(out)
        }
        Math::Background(background, border, node) => {
            let b = layout(node, fonts, size, color)?;
            let padding = size * 0.2;
            let mut out = BoxLayout {
                width: b.width + 2. * padding,
                ascent: b.ascent + padding,
                descent: b.descent + padding,
                ..Default::default()
            };
            out.backgrounds.push(Background {
                x: 0.,
                y: -out.descent,
                width: out.width,
                height: out.ascent + out.descent,
                color: *background,
                border: *border,
            });
            out.append(b, padding, 0.);
            Ok(out)
        }
        Math::Grid(rows) => {
            let mut cells = Vec::new();
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0f32; cols];
            for row in rows {
                let mut cell_row = Vec::new();
                for (i, cell) in row.iter().enumerate() {
                    let b = layout(cell, fonts, size, color)?;
                    widths[i] = widths[i].max(b.width);
                    cell_row.push(b);
                }
                cells.push(cell_row);
            }
            let row_height = cells
                .iter()
                .flatten()
                .map(|b| b.ascent + b.descent)
                .fold(size * 1.4, f32::max)
                + size * 0.3;
            let height = row_height * cells.len() as f32;
            let mut out = BoxLayout {
                width: widths.iter().sum::<f32>() + size * (cols.saturating_sub(1)) as f32,
                ascent: height * 0.5,
                descent: height * 0.5,
                ..Default::default()
            };
            for (r, row) in cells.into_iter().enumerate() {
                let mut x = 0.;
                for (c, b) in row.into_iter().enumerate() {
                    let y = height * 0.5 - row_height * (r as f32 + 0.8);
                    out.append(b, x, y);
                    x += widths[c] + size;
                }
            }
            Ok(out)
        }
    }
}
