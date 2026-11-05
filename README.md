<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/publisle/publisle/main/docs/assets/brand/lockup-dark.svg" />
    <img src="https://raw.githubusercontent.com/publisle/publisle/main/docs/assets/brand/lockup-light.svg" alt="Publisle" width="420" />
  </picture>
  <p><strong>The native PDF and math engines behind Publisle.</strong></p>
  <p>
    <a href="https://github.com/publisle/publisle-rs/actions/workflows/ci.yml"><img src="https://github.com/publisle/publisle-rs/actions/workflows/ci.yml/badge.svg" alt="CI status" /></a>
    <img src="https://img.shields.io/badge/status-experimental-9B3214" alt="experimental" />
    <a href="https://crates.io/crates/publisle-pdf"><img src="https://img.shields.io/crates/v/publisle-pdf" alt="publisle-pdf on crates.io" /></a>
    <a href="https://docs.rs/publisle-pdf"><img src="https://img.shields.io/docsrs/publisle-pdf" alt="publisle-pdf docs" /></a>
  </p>
</div>

Publisle Rust compiles a resolved article into tagged PDF and parses a bounded
TeX math fragment into MathML. It does not fetch the network and it does not
run TeX. Markdown conversion stays in the npm [`@publisle/cli`](https://github.com/publisle/publisle).

## What's inside

- `crates/publisle-pdf`: article layout, font shaping, vector graphics, and tagged PDF.
- `crates/publisle-math`: bounded TeX math syntax and MathML, with no PDF or font dependencies.
- `crates/publisle`: a small facade over the PDF compiler and the math parser.
- `crates/publisle-cli`: the `publisle-rs` command. The name sits beside the npm `publisle` executable.
- `npm/`: five platform packages that ship the compiler binary and the pinned fonts.
- `conformance/`: schemas, fixtures, and the measured gap report.

## Quickstart

Requires Rust 1.90.0. `cargo install publisle-cli` installs `publisle-rs`. The
commands take resolved JSON or a math fragment, not Markdown.

```sh
publisle-rs pdf INPUT.json NEW-OUTPUT.pdf --fonts /absolute/path/fonts.json
publisle-rs math '\hat{x}+\binom{n}{k}' --mathml
publisle-rs fonts check --fonts /absolute/path/fonts.json
```

`PUBLISLE_NATIVE_FONTS` can replace `--fonts`. Font files are not part of the
crates.io packages; start from the matching `@publisle/pdf-native-*` package.
The facade defaults to the `pdf` feature. `default-features = false, features = ["math"]`
selects the font-independent parser. Unsupported math commands stay visible in
red, with structured warnings.

Library and manifest details live in the
[CLI README](crates/publisle-cli/README.md) and on
[docs.rs](https://docs.rs/publisle-pdf).

## Status

The engine remains experimental. Shipping a release does not close every gap in
[the measured gap report](conformance/rust-engine-gaps.md). PDF/UA metadata
records the requested standard. Compliance still needs independent validation.

Production Publisle pins `publisle-pdf` 0.6.5.

## Contributing

See [Contributing](CONTRIBUTING.md) for local checks, releases, and how a native
version is pinned in Publisle.

## License

[MIT](LICENSE) — Copyright (c) 2026 Publisle.
