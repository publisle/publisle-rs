# Publisle Rust engines

The native PDF engine and bounded math parser for [Publisle](https://github.com/publisle/publisle). The engine was extracted from the TypeScript repository with all 13 native commits preserved. `git log -- crates/publisle-pdf` shows the original engine history; `.git/filter-repo/commit-map` records rewritten commits locally.

- `crates/publisle`: optional PDF and math facade for library users.
- `crates/publisle-cli`: `publisle-rs` command line tools; this name coexists with the npm `publisle` executable.
- `crates/publisle-pdf`: article layout, font shaping, vector graphics and tagged PDFs.
- `crates/publisle-math`: bounded TeX math syntax and MathML, with no PDF or font dependencies.
- `npm/`: manifests for five binary-and-font packages; package versions derive from the PDF crate at build time.
- `conformance/`: immutable upstream JSON Schemas, fixtures, figure regressions and the engine gap report.

## Development

Rust 1.90.0 is pinned in `rust-toolchain.toml`; Node 24 is required for packaging and conformance checks.

```sh
npm ci --ignore-scripts
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
PUBLISLE_TEST_FONT_DIR=/usr/share/fonts/truetype/dejavu cargo test --workspace --locked
node tools/check-conformance.mjs
node --test tools/tests/*.test.mjs
cargo build --release --locked -p publisle-pdf
```

Install `fonts-dejavu-core` and `fonts-dejavu-extra` for the font regression tests. The five-platform CI builds against Debian bookworm glibc on Linux and the pinned Rust toolchain on macOS and Windows. Each package is smoke-tested with an empty PATH before its exact tarball is uploaded. Fonts and their licenses use the same reviewed Docker font stage as the hosted worker.

## Releases

`release.yml` runs full CI, uses release-plz to prepare crate version PRs and publish crates through crates.io OIDC, then publishes the five validated npm tarballs through npm OIDC. The default GitHub token needs permission to create release PRs. A release PR made with that token needs a human close/reopen or an explicit workflow dispatch before merging, because GitHub suppresses downstream workflow events created by that token.

Crates.io trusted publishers for `publisle`, `publisle-cli`, `publisle-pdf`, `publisle-math`, and the reserved `publisle-research` name must point to `publisle/publisle-rs`, workflow `release.yml`, with no environment. All five names are already bootstrapped; configure their publishers before merging a release PR. `publisle-research` remains a reservation and is not part of this workspace. No long-lived crates.io token is stored here. Native npm packages must trust the same repository and workflow, with no environment restriction:

```sh
node tools/npm-trusted-publishers.mjs
node tools/npm-trusted-publishers.mjs --apply
node tools/npm-trusted-publishers.mjs --audit
```

Use npm >=12.2 and a browser-authenticated account with package administration rights. `NPM_CLI_PATH` and `NPM_CONFIG_USERCONFIG` can select a separate CLI and login.

The npm publisher waits for the `publisle-pdf-vVERSION` crate release tag at the exact source commit and the corresponding crates.io version. It preflights all five immutable versions before uploading anything; recovery accepts an existing version only when its tarball SHA-512 matches. Use **Re-run failed jobs** on the original run to finish a partial release from its retained tarballs and upload record. Never rerun validation or rebuild after an upload has been accepted. The crate release retains the tarballs and `native-release.json` as evidence. Never bump versions to hide a partial publication.

## Migration boundary

Deployment, service integration and VPS secrets stay in `publisle/publisle`.
Research and the VPS worker pin the five external native packages to the same
PDF crate version. When a native release ships, update Research optional
dependencies, the workspace release-age exclusions, the VPS runtime manifest
and lockfile, and the root pnpm lock. Verify Linux, macOS and Windows consumer
installs before releasing the Research patch. Dependabot groups native updates.

## Native CLI and library APIs

`cargo install publisle-cli` installs `publisle-rs`. It consumes resolved input
JSON, not Markdown; use npm `@publisle/cli` for Markdown conversion.

```sh
publisle-rs pdf INPUT.json NEW-OUTPUT.pdf --fonts /absolute/path/fonts.json
publisle-rs math '\hat{x}+\binom{n}{k}' --mathml
publisle-rs fonts check --fonts /absolute/path/fonts.json
```

`PUBLISLE_NATIVE_FONTS` can replace `--fonts`. See the [CLI manifest format](crates/publisle-cli/README.md)
and the matching npm native package's `fonts/` directory. Font files are not
bundled into crates.io packages. The facade defaults to the `pdf` feature;
`default-features = false, features = ["math"]` selects the font-independent parser.
Unsupported math commands stay visible in red with structured warnings. Syntax
limits, malformed input and TeX execution attempts still fail with diagnostics.

The engine remains experimental. Extraction does not resolve every gap in [the measured gap report](conformance/rust-engine-gaps.md) or change the existing readiness gates.
