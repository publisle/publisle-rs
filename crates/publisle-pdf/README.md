# Native PDF compiler (experimental)

`publisle-pdf` compiles Publisle's resolved article model into tagged PDF 2.0.
It embeds subset fonts, shapes text with Rustybuzz, lays out Unicode text and
vectors, and associates MathML with equations. It does not fetch the network
and it does not need a TeX installation. LaTeX source export stays in
`@publisle/research`.

The public API is `compile` plus the `model` module (`Input`, `Report`, and
`Diagnostic`). Everything else in this crate is private. The versioned input
is the JSON a trusted host produces from a Publisle document. The compiler
writes one JSON report, creates the output file exclusively, and returns an
error on failure. Font files and local figures are supplied by that host.
They are not downloaded and they are not bundled in the crate.

```json
[
  { "path": "/absolute/path/NotoSerif-Regular.ttf", "role": "regular" },
  { "path": "/absolute/path/NotoSerif-Bold.ttf", "role": "bold" },
  { "path": "/absolute/path/NotoSerif-Italic.ttf", "role": "italic" },
  { "path": "/absolute/path/DejaVuSansMono.ttf", "role": "mono" },
  { "path": "/absolute/path/NotoNaskhArabic-Regular.ttf", "role": "fallback" },
  { "path": "/absolute/path/NotoSansMath-Regular.ttf", "role": "math" }
]
```

The command-line wrapper is `publisle-rs` from `publisle-cli`. Markdown
conversion stays in npm `@publisle/cli`.

Math is the bounded Publisle contract, not a TeX interpreter. Unknown commands,
malformed groups, oversized expressions, and unsupported glyphs return
diagnostics. Indexed roots are supported. Arbitrary macros and packages are not.
TrueType clusters keep a Unicode mapping when one character needs several
glyphs. The Arabic article profile is compiled here. Complex CFF1 clusters that
cannot preserve that mapping fail with a font diagnostic; use a TrueType font
for those scripts.

PDF/UA metadata records the requested standard. Compliance still needs
independent validation of alt text, reading order, and equations.
