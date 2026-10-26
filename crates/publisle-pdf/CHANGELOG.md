# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.1](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.6.0...publisle-pdf-v0.6.1) - 2026-10-26

### Fixed

- *(pdf)* write hierarchical heading bookmarks with final page destinations ([#35](https://github.com/publisle/publisle-rs/pull/35))

## [0.6.0](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.5.1...publisle-pdf-v0.6.0) - 2026-10-26

### Added

- render nested lists, authored starts, alignment, framed code and front-matter dates (gaps 16, 17, 12) ([#33](https://github.com/publisle/publisle-rs/pull/33))

## [0.5.1](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.5.0...publisle-pdf-v0.5.1) - 2026-10-25

### Fixed

- close the bounded-failure and typography gaps (3, 4, 7, 14, 18) ([#31](https://github.com/publisle/publisle-rs/pull/31))

## [0.5.0](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.4.0...publisle-pdf-v0.5.0) - 2026-10-24

### Added

- [**breaking**] gate asset support behind build features ([#28](https://github.com/publisle/publisle-rs/pull/28))

## [0.4.0](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.3.1...publisle-pdf-v0.4.0) - 2026-10-24

### Added

- [**breaking**] improve native math and add facade and CLI crates ([#25](https://github.com/publisle/publisle-rs/pull/25))

### Other

- *(deps)* bump lopdf from 0.38.0 to 0.46.0 ([#22](https://github.com/publisle/publisle-rs/pull/22))
- *(deps)* bump pdf-writer from 0.14.0 to 0.15.0 ([#19](https://github.com/publisle/publisle-rs/pull/19))

## [0.3.1](https://github.com/publisle/publisle-rs/compare/publisle-pdf-v0.0.1...publisle-pdf-v0.3.1) - 2026-10-23

### Added

- establish standalone Rust workspace, conformance and native releases
- *(pdf)* stage native platform packages and frozen-reference fidelity checks
- *(pdf)* preserve native SVG vectors and add indexed roots and UA-1 output
- *(native)* experimental Rust PDF compiler crate

### Fixed

- *(pdf)* separate RTL cluster characters without changing shaped outlines
- *(pdf)* include font style assets and correct indexed-root regression
- *(pdf)* bound SVG missing-family diagnostic text
- *(pdf)* preserve logical text during viewer copying
- *(pdf)* resolve pinned SVG fonts and replace Python PDF tooling

### Other

- retain extraction provenance and avoid duplicate main matrix builds
- Version packages ([#90](https://github.com/publisle/publisle-rs/pull/90))
- Validate releases before publication and automate gated PDF deployment ([#89](https://github.com/publisle/publisle-rs/pull/89))
- Remove exact legacy layout comparison from native PDF release gates
- Preserve Arabic marks in PDF.js and record local parity results
- Fix Arabic mark extraction and validate native compiler reports
