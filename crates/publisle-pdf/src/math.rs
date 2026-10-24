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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Display,
    Text,
    Script,
}

pub fn layout(
    node: &Math,
    fonts: &mut Fonts,
    size: f32,
    color: [f32; 3],
    style: Style,
) -> Result<BoxLayout> {
    match node {
        Math::Text(text) | Math::Operator(text, _) | Math::PlainText(text) => {
            let size = if style == Style::Display
                && matches!(node, Math::Operator(op, _) if ["∑", "∏", "∐", "⋃", "⋂", "lim", "max", "min", "sup", "inf"].contains(&op.as_str()))
            {
                size * 1.4
            } else {
                size
            };
            let italic = matches!(node, Math::Text(_))
                && text.chars().count() == 1
                && text
                    .chars()
                    .any(|c| c.is_ascii_alphabetic() || ('α'..='ω').contains(&c));
            let shaped =
                fonts.shape(text, size, if italic { "italic" } else { "regular" }, false)?;
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
            use publisle_math::AtomClass::*;
            let mut previous = None;
            for (index, node) in nodes.iter().enumerate() {
                let mut class = node.atom_class();
                if class == Bin
                    && (previous.is_none()
                        || matches!(previous, Some(Bin | Op | Rel | Open | Punct))
                        || nodes
                            .get(index + 1)
                            .is_none_or(|next| matches!(next.atom_class(), Rel | Close | Punct)))
                {
                    class = Ord;
                }
                let spacing = match (previous, class) {
                    (Some(Open), _) | (_, Close) | (_, Punct) | (None, _) => 0.,
                    (Some(Rel), _) | (_, Rel) => 5. / 18.,
                    (Some(Bin), _) | (_, Bin) => 4. / 18.,
                    (Some(Op | Punct), _) | (_, Op) => 3. / 18.,
                    _ => 0.,
                };
                if style != Style::Script {
                    out.width += size * spacing;
                }
                previous = Some(class);
                let b = layout(node, fonts, size, color, style)?;
                let x = out.width;
                out.width += b.width;
                out.ascent = out.ascent.max(b.ascent);
                out.descent = out.descent.max(b.descent);
                out.append(b, x, 0.);
            }
            Ok(out)
        }
        Math::Color(c, node) => layout(node, fonts, size, *c, style),
        Math::Fraction(a, b) => {
            let a = layout(
                a,
                fonts,
                size * if style == Style::Display { 0.8 } else { 0.7 },
                color,
                Style::Script,
            )?;
            let b = layout(
                b,
                fonts,
                size * if style == Style::Display { 0.8 } else { 0.7 },
                color,
                Style::Script,
            )?;
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
            let limits = style == Style::Display
                && matches!(base.as_ref(), Math::Operator(op, _) if ["∑", "∏", "∐", "⋃", "⋂", "lim", "max", "min", "sup", "inf"].contains(&op.as_str()));
            let mut out = layout(
                base,
                fonts,
                if limits { size * 1.4 } else { size },
                color,
                Style::Text,
            )?;
            if limits {
                let lower = sub
                    .as_ref()
                    .map(|n| layout(n, fonts, size * 0.65, color, Style::Script))
                    .transpose()?;
                let upper = sup
                    .as_ref()
                    .map(|n| layout(n, fonts, size * 0.65, color, Style::Script))
                    .transpose()?;
                let width = lower
                    .iter()
                    .chain(upper.iter())
                    .fold(out.width, |w, b| w.max(b.width));
                let original = out;
                out = BoxLayout {
                    width,
                    ascent: original.ascent,
                    descent: original.descent,
                    ..Default::default()
                };
                let x = (width - original.width) / 2.;
                out.append(original, x, 0.);
                if let Some(b) = lower {
                    let x = (width - b.width) / 2.;
                    let y = -out.descent - b.ascent - size * 0.1;
                    out.descent = -y + b.descent;
                    out.append(b, x, y);
                }
                if let Some(b) = upper {
                    let x = (width - b.width) / 2.;
                    let y = out.ascent + b.descent + size * 0.1;
                    out.ascent = y + b.ascent;
                    out.append(b, x, y);
                }
                return Ok(out);
            }
            if matches!(base.as_ref(), Math::Accent("underbrace", _))
                && sup.is_none()
                && let Some(sub) = sub
            {
                let b = layout(sub, fonts, size * 0.65, color, Style::Script)?;
                let x = (out.width - b.width) / 2.;
                let y = -out.descent - b.ascent - size * 0.1;
                out.descent = -y + b.descent;
                out.append(b, x, y);
                return Ok(out);
            }
            let base_width = out.width;
            let mut script_width: f32 = 0.;
            if let Some(sub) = sub {
                let b = layout(sub, fonts, size * 0.65, color, Style::Script)?;
                let y = -size * 0.3 - b.ascent * 0.25;
                script_width = script_width.max(b.width);
                out.descent = out.descent.max(-y + b.descent);
                out.append(b, base_width, y);
            }
            if let Some(sup) = sup {
                let b = layout(sup, fonts, size * 0.65, color, Style::Script)?;
                let y = size * 0.58;
                script_width = script_width.max(b.width);
                out.ascent = out.ascent.max(y + b.ascent);
                out.append(b, base_width, y);
            }
            out.width += script_width;
            Ok(out)
        }
        Math::IndexedRoot(index, node) => {
            let root = layout(&Math::Root(node.clone()), fonts, size, color, style)?;
            let index = layout(index, fonts, size * 0.55, color, style)?;
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
            let b = layout(node, fonts, size, color, style)?;
            let root = layout(&Math::operator("√".into()), fonts, size * 1.2, color, style)?;
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
            let mut out = layout(node, fonts, size, color, style)?;
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
            if matches!(*kind, "hat" | "tilde" | "dot") {
                let glyph = match *kind {
                    "hat" => "^",
                    "tilde" => "~",
                    _ => "·",
                };
                let accent = layout(
                    &Math::PlainText(glyph.into()),
                    fonts,
                    size * 0.7,
                    color,
                    Style::Text,
                )?;
                let x = (out.width - accent.width) / 2.;
                // Text accents sit above the ink rather than a full em above
                // the nominal math box. The glyph already has a raised bearing.
                let bearing = match *kind {
                    "hat" => 0.7,
                    "tilde" => 0.65,
                    _ => 0.45,
                };
                let y = out.ascent - size * bearing;
                out.ascent = out.ascent.max(y + accent.ascent);
                out.append(accent, x, y);
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
                let arrow = layout(&Math::operator("›".into()), fonts, size * 0.5, color, style)?;
                let x = out.width - arrow.width;
                out.append(arrow, x, y - size * 0.2);
            }
            out.ascent += size * 0.2;
            Ok(out)
        }
        Math::Over(base, label) => {
            let base = layout(base, fonts, size, color, style)?;
            let label = layout(label, fonts, size * 0.65, color, Style::Script)?;
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
            let b = layout(node, fonts, size, color, style)?;
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
        Math::Unsupported(source) => layout(
            &Math::PlainText(source.clone()),
            fonts,
            size,
            [0.85, 0.05, 0.05],
            style,
        ),
        Math::Under(base, label) => {
            let mut out = layout(base, fonts, size, color, style)?;
            let label = layout(label, fonts, size * 0.65, color, Style::Script)?;
            let width = out.width.max(label.width);
            let old = out;
            out = BoxLayout {
                width,
                ascent: old.ascent,
                descent: old.descent,
                ..Default::default()
            };
            let x = (width - old.width) / 2.;
            out.append(old, x, 0.);
            let x = (width - label.width) / 2.;
            let y = -out.descent - label.ascent - size * 0.15;
            out.descent = -y + label.descent;
            out.append(label, x, y);
            Ok(out)
        }
        Math::Fenced { open, close, body } => {
            let body = layout(body, fonts, size, color, style)?;
            let delimiter_size = ((body.ascent + body.descent) / 1.07).max(size);
            let left = layout(
                &Math::operator(open.clone()),
                fonts,
                delimiter_size,
                color,
                Style::Text,
            )?;
            let right = layout(
                &Math::operator(close.clone()),
                fonts,
                delimiter_size,
                color,
                Style::Text,
            )?;
            let mut out = BoxLayout {
                width: left.width + body.width + right.width,
                ascent: body.ascent,
                descent: body.descent,
                ..Default::default()
            };
            let y = (body.ascent - body.descent - left.ascent + left.descent) / 2.;
            out.ascent = out.ascent.max(y + left.ascent);
            out.descent = out.descent.max(-y + left.descent);
            let bx = left.width;
            let rx = bx + body.width;
            out.append(left, 0., y);
            out.append(body, bx, 0.);
            out.append(right, rx, y);
            Ok(out)
        }
        Math::Grid(rows) => {
            let mut cells = Vec::new();
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0f32; cols];
            for row in rows {
                let mut cell_row = Vec::new();
                for (i, cell) in row.iter().enumerate() {
                    let b = layout(cell, fonts, size, color, style)?;
                    widths[i] = widths[i].max(b.width);
                    cell_row.push(b);
                }
                cells.push(cell_row);
            }
            let heights: Vec<f32> = cells
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.ascent).fold(size * 0.85, f32::max)
                        + row.iter().map(|b| b.descent).fold(size * 0.22, f32::max)
                        + size * 0.3
                })
                .collect();
            let height: f32 = heights.iter().sum();
            let mut out = BoxLayout {
                width: widths.iter().sum::<f32>() + size * cols.saturating_sub(1) as f32,
                ascent: height * 0.5,
                descent: height * 0.5,
                ..Default::default()
            };
            let mut top = height * 0.5;
            for (r, row) in cells.into_iter().enumerate() {
                let ascent = row.iter().map(|b| b.ascent).fold(size * 0.85, f32::max);
                let y = top - size * 0.15 - ascent;
                let mut x = 0.;
                for (c, b) in row.into_iter().enumerate() {
                    out.append(b, x, y);
                    x += widths[c] + size;
                }
                top -= heights[r];
            }
            Ok(out)
        }
        _ => Err(Diagnostic::new(
            "unsupported-math-node",
            "Unsupported math syntax tree variant.",
        )),
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use crate::model::FontSpec;
    fn fonts() -> Fonts {
        let directory = std::path::PathBuf::from(
            std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned test fonts required"),
        );
        Fonts::load(&[
            FontSpec {
                path: directory.join("DejaVuSerif.ttf"),
                role: "regular".into(),
                index: 0,
            },
            FontSpec {
                path: directory.join("DejaVuSans.ttf"),
                role: "fallback".into(),
                index: 0,
            },
            FontSpec {
                path: directory.join("DejaVuSerif-Italic.ttf"),
                role: "italic".into(),
                index: 0,
            },
        ])
        .unwrap()
    }
    fn draw(source: &str, style: Style) -> BoxLayout {
        layout(&parse(source).unwrap(), &mut fonts(), 12., [0.; 3], style).unwrap()
    }
    #[test]
    fn variables_are_italic_but_functions_and_digits_remain_upright() {
        assert_eq!(draw("x", Style::Text).runs[0].shaped.glyphs[0].font, 2);
        assert_eq!(draw("1", Style::Text).runs[0].shaped.glyphs[0].font, 0);
        assert_eq!(draw(r"\sin", Style::Text).runs[0].shaped.glyphs[0].font, 0);
    }
    #[test]
    fn relations_and_binary_operators_have_spacing_but_unary_signs_do_not() {
        let tight = draw("xy", Style::Text).width + draw("=", Style::Text).width;
        assert!((draw("x=y", Style::Text).width - tight - 12. * 10. / 18.).abs() < 0.01);
        let tight = draw("xy", Style::Text).width + draw("+", Style::Text).width;
        assert!((draw("x+y", Style::Text).width - tight - 12. * 8. / 18.).abs() < 0.01);
        assert!(
            (draw("-x", Style::Text).width
                - draw("-", Style::Text).width
                - draw("x", Style::Text).width)
                .abs()
                < 0.01
        );
    }
    #[test]
    fn display_limits_stack_and_fences_follow_fraction_height() {
        let display = draw(r"\sum_{i=0}^n x_i", Style::Display);
        let text = draw(r"\sum_{i=0}^n x_i", Style::Text);
        assert!(display.ascent + display.descent > text.ascent + text.descent);
        assert!(display.runs[0].size > text.runs[0].size);
        let fenced = draw(r"\left(\frac{x}{y}\right)", Style::Display);
        assert!(fenced.runs[0].size > 12.);
        assert!(
            draw(r"\frac{x}{y}", Style::Display).ascent > draw(r"\frac{x}{y}", Style::Text).ascent
        );
    }
    #[test]
    fn unsupported_commands_are_red_and_keep_the_source_text() {
        let rendered = draw(r"\unknownpublisle{x}", Style::Text);
        assert_eq!(rendered.runs[0].color, [0.85, 0.05, 0.05]);
        assert!(rendered.runs.len() >= 2);
    }
}
