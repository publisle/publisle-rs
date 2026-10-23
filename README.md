# Publisle Rust engines

The native PDF engine and bounded math parser for [Publisle](https://github.com/publisle/publisle). The engine was extracted from the TypeScript repository with all 13 native commits preserved. `git log -- crates/publisle-pdf` shows the original engine history; `.git/filter-repo/commit-map` records rewritten commits locally.

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

Install `fonts-dejavu-core` for the font regression tests. The five-platform CI builds against Debian bookworm glibc on Linux and the pinned Rust toolchain on macOS and Windows. Each package is smoke-tested with an empty PATH before its exact tarball is uploaded. Fonts and their licenses use the same reviewed Docker font stage as the hosted worker.

## Releases

`release.yml` runs full CI, uses release-plz to prepare crate version PRs and publish crates through crates.io OIDC, then publishes the five validated npm tarballs through npm OIDC. The default GitHub token needs permission to create release PRs. A release PR made with that token needs a human close/reopen or an explicit workflow dispatch before merging, because GitHub suppresses downstream workflow events created by that token.

Crates.io trusted publishers for both crates must point to `publisle/publisle-rs`, workflow `release.yml`. New crates require one manual bootstrap publication before trusted publishing can be configured. No long-lived crates.io token is stored here. Native npm packages must trust the same repository and workflow, with no environment restriction:

```sh
node tools/npm-trusted-publishers.mjs
node tools/npm-trusted-publishers.mjs --apply
node tools/npm-trusted-publishers.mjs --audit
```

Use npm >=12.2 and a browser-authenticated account with package administration rights. `NPM_CLI_PATH` and `NPM_CONFIG_USERCONFIG` can select a separate CLI and login.

The npm publisher waits for the `publisle-pdf-vVERSION` crate release tag at the exact source commit and the corresponding crates.io version. It preflights all five immutable versions before uploading anything; recovery accepts an existing version only when its tarball SHA-512 matches. Use **Re-run failed jobs** on the original run to finish a partial release from its retained tarballs and upload record. Never rerun validation or rebuild after an upload has been accepted. The crate release retains the tarballs and `native-release.json` as evidence. Never bump versions to hide a partial publication.

## Migration boundary

Deployment, service integration and VPS secrets stay in `publisle/publisle`. That repository retains `native/` until this repository has independently published all five packages. Only then should Research pin the external versions, the VPS worker install the pinned package, and a cleanup PR remove the old engine and native release jobs. Verify Linux, macOS and Windows consumer installs before releasing the Research patch. Dependabot in the TypeScript repository will keep those exact pins updated together.

The engine remains experimental. Extraction does not resolve every gap in [the measured gap report](conformance/rust-engine-gaps.md) or change the existing readiness gates.
