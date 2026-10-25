#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
mod assets;
mod composite;
mod fonts;
mod layout;
mod math;
/// Resolved article input, assets, fonts, and structured compilation diagnostics.
pub mod model;
mod pdf;
mod profiles;
#[cfg(feature = "svg")]
mod svg;

/// Compile resolved native input into PDF bytes and a structured report.
pub fn compile(input: &model::Input) -> model::Result<(Vec<u8>, model::Report)> {
    let mut diagnostics = Vec::new();
    fn collect(
        value: &str,
        index: usize,
        diagnostics: &mut Vec<model::Diagnostic>,
    ) -> model::Result<()> {
        let (_, warnings) = publisle_math::parse_with_diagnostics(value)
            .map_err(|e| model::Diagnostic::new(&e.code, e.message).at(index))?;
        diagnostics.extend(
            warnings
                .into_iter()
                .map(|e| model::Diagnostic::new(&e.code, e.message).at(index)),
        );
        Ok(())
    }
    fn inlines(
        values: &[model::Inline],
        index: usize,
        diagnostics: &mut Vec<model::Diagnostic>,
    ) -> model::Result<()> {
        for value in values {
            if let model::Inline::Math { value } = value {
                collect(value, index, diagnostics)?;
            }
        }
        Ok(())
    }
    for (index, block) in input.article.blocks.iter().enumerate() {
        use model::Block;
        match block {
            Block::Math { value, .. } => collect(value, index, &mut diagnostics)?,
            Block::Heading {
                inlines: values, ..
            }
            | Block::Paragraph {
                inlines: values, ..
            } => inlines(values, index, &mut diagnostics)?,
            Block::List { items, .. }
            | Block::Quote { paragraphs: items }
            | Block::Footnote {
                paragraphs: items, ..
            } => {
                for values in items {
                    inlines(values, index, &mut diagnostics)?;
                }
            }
            Block::Figure { caption, .. } => inlines(caption, index, &mut diagnostics)?,
            Block::Table { rows, caption, .. } => {
                inlines(caption, index, &mut diagnostics)?;
                for row in rows {
                    for cell in row {
                        inlines(cell, index, &mut diagnostics)?;
                    }
                }
            }
            _ => {}
        }
    }
    diagnostics.push(model::Diagnostic::new("native-engine-experimental", "Native layout is experimental; publisher fidelity and PDF/UA conformance require independent validation."));
    let started = std::time::Instant::now();
    let (layout, mut layout_warnings) = layout::build(input)?;
    diagnostics.append(&mut layout_warnings);
    let layout_ms = started.elapsed().as_millis();
    let bytes = pdf::write(&layout, input.pdf_standard.as_deref())?;
    let mut timings = std::collections::BTreeMap::new();
    timings.insert("layoutMs", layout_ms);
    timings.insert("pdfMs", started.elapsed().as_millis() - layout_ms);
    timings.insert("compilerMs", started.elapsed().as_millis());
    Ok((
        bytes,
        model::Report {
            ok: true,
            compiler_version: env!("CARGO_PKG_VERSION"),
            pages: layout.pages.len(),
            diagnostics,
            timings,
        },
    ))
}

/// Validate a pinned font manifest without producing a PDF.
pub fn check_fonts(fonts: &[model::FontSpec]) -> model::Result<()> {
    fonts::Fonts::load(fonts).map(|_| ())
}
