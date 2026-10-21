# Native PDF compiler (experimental)

`publisle-pdf` is a bounded, network-free Rust compiler for Publisle's resolved
article model. It embeds subset fonts, shapes text with Rustybuzz, lays out
Unicode text and vectors, and writes tagged PDF 2.0 with MathML associations.
No TeX installation is needed at runtime. LaTeX source/submission export keeps
using the existing research source generators.

```sh
cargo build --release --locked --manifest-path native/pdf/Cargo.toml
cargo test --locked --manifest-path native/pdf/Cargo.toml
cargo clippy --locked --manifest-path native/pdf/Cargo.toml --all-targets -- -D warnings
native/pdf/target/release/publisle-pdf INPUT.json NEW-OUTPUT.pdf
```

The versioned input is defined in `src/model.rs` and lowered by
`createNativePdfPackage` in `@publisle/research`. The CLI writes one bounded JSON
report to stdout, creates output exclusively, and returns nonzero on failure.
Input files, fonts and local figure paths must be supplied by the trusted host.
The hosted runtime has no network, writable root, or TeX execution support.

For the Research CLI, explicitly configure `PUBLISLE_NATIVE_COMPILER` with the
absolute binary path and `PUBLISLE_NATIVE_FONTS` with a JSON font manifest:

```json
[
  { "path": "/absolute/path/NotoSerif-Regular.ttf", "role": "regular" },
  { "path": "/absolute/path/NotoSerif-Bold.ttf", "role": "bold" },
  { "path": "/absolute/path/NotoSerif-Italic.ttf", "role": "italic" },
  { "path": "/absolute/path/DejaVuSansMono.ttf", "role": "mono" },
  { "path": "/absolute/path/NotoNaskhArabic-Regular.ttf", "role": "fallback" }
]
```

Then use `publisle export manuscript.md --to pdf --engine rust --output paper.pdf`.
The Node API also exposes `compileNativePdf(document, options)` and the
`exportPdf(document, {engine: "rust", sourceDirectory, ...options})` overload.
Local figures must remain inside `sourceDirectory`; HTTP sources are rejected
by this API. Hosted images are fetched and validated separately by the service.

Seven profiles have publisher-class measurements for page geometry, margins,
font sizes and column counts. They remain candidate ports: matching geometry
alone does not establish publisher approval or complete typographic fidelity.
Native CLI use is explicit. Hosted profiles are advertised only after immutable
image, resource-budget, content, visual-layout, PDF/UA-2 and speed verification.

Math is a bounded parser for the supported Publisle math contract, not a TeX
interpreter. Unknown commands, malformed groups, oversized expressions and
unsupported glyphs return diagnostics rather than silently losing content.
Indexed roots and arbitrary macros/packages are currently unsupported.
TrueType clusters preserve Unicode text even when one character needs several
shaped glyphs. Arabic geometry-based extraction still differs between readers for some ligatures
and diacritics, despite correct logical tags. Arabic remains on the legacy hosted
engine pending copy/paste parity. Complex CFF1 clusters that cannot preserve this mapping fail with
an actionable font diagnostic; use a TrueType font for those scripts.

PDF/UA metadata records the requested standard. Compliance requires independent
validation and review of authored alt text, reading order and equations. Run the
native service integration test with `PDF_TEST_NATIVE_WORKER_IMAGE` and
`PDF_TEST_VERAPDF` to validate each candidate's actual output.
