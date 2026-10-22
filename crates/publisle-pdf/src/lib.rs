mod assets;
mod composite;
mod fonts;
mod layout;
mod math;
pub mod model;
mod pdf;
mod profiles;
mod svg;

pub fn compile(input: &model::Input) -> model::Result<(Vec<u8>, model::Report)> {
    let started = std::time::Instant::now();
    let layout = layout::build(input)?;
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
            diagnostics: vec![model::Diagnostic::new(
                "native-engine-experimental",
                "Native layout is experimental; publisher fidelity and PDF/UA conformance require independent validation.",
            )],
            timings,
        },
    ))
}
