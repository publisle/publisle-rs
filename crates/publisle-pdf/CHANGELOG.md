# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
