# Contributing

Rust 1.90.0 is pinned in `rust-toolchain.toml`. Node 24 is required for packaging
and conformance checks.

```sh
npm ci --ignore-scripts
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
PUBLISLE_TEST_FONT_DIR=/usr/share/fonts/truetype/dejavu cargo test --workspace --locked
node tools/check-conformance.mjs
node --test tools/tests/*.test.mjs
cargo build --release --locked -p publisle-pdf
```

Install `fonts-dejavu-core` and `fonts-dejavu-extra` for the font regression
tests. The five-platform CI builds against Debian bookworm glibc on Linux and
the pinned Rust toolchain on macOS and Windows. Each package is smoke-tested
with an empty PATH before its exact tarball is uploaded. Fonts and their
licenses use the same reviewed Docker font stage as the hosted worker.

## Releases

`release.yml` runs full CI, uses release-plz to prepare crate version pull
requests and publish crates through crates.io OIDC, then publishes the five
validated npm tarballs through npm OIDC. The default GitHub token needs
permission to create release pull requests. A release pull request made with
that token needs a human close/reopen or an explicit workflow dispatch before
merging, because GitHub suppresses downstream workflow events created by that
token.

Crates.io trusted publishers for `publisle`, `publisle-cli`, `publisle-pdf`,
`publisle-math`, and the reserved `publisle-research` name must point to
`publisle/publisle-rs`, workflow `release.yml`, with no environment. All five
names are already bootstrapped; configure their publishers before merging a
release pull request. `publisle-research` remains a reservation and is not part
of this workspace. Release pull request analysis uses Rust 1.93.0 for the
pinned cargo-semver-checks tool; engine builds, tests, and crate publication
remain on Rust 1.90.0. No long-lived crates.io token is stored here. Native npm
packages must trust the same repository and workflow, with no environment
restriction:

```sh
node tools/npm-trusted-publishers.mjs
node tools/npm-trusted-publishers.mjs --apply
node tools/npm-trusted-publishers.mjs --audit
```

Use npm >=12.2 and a browser-authenticated account with package administration
rights. `NPM_CLI_PATH` and `NPM_CONFIG_USERCONFIG` can select a separate CLI
and login.

The npm publisher waits for the `publisle-pdf-vVERSION` crate release tag at
the exact source commit and the corresponding crates.io version. It preflights
all five immutable versions before uploading anything; recovery accepts an
existing version only when its tarball SHA-512 matches. Use **Re-run failed
jobs** on the original run to finish a partial release from its retained
tarballs and upload record. Never rerun validation or rebuild after an upload
has been accepted. The crate release retains the tarballs and
`native-release.json` as evidence. Never bump versions to hide a partial
publication.

Squash-merge a release-plz version pull request so the crate tag points at
main HEAD.

## Migration boundary

Deployment, service integration, and VPS secrets stay in `publisle/publisle`.
Research and the VPS worker pin the five external native packages to the same
PDF crate version. When a native release ships, update Research optional
dependencies, the workspace release-age exclusions, the VPS runtime manifest
and lockfile, and the root pnpm lock. Verify Linux, macOS, and Windows consumer
installs before releasing the Research patch. Dependabot groups native updates.
