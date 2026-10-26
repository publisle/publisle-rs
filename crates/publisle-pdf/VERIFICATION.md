# Native PDF rollout evidence — 22 October 2026

## Local heading outline verification — 26 October 2026

Issue #8 was reproduced against main at `24c745e`: MuPDF reported no outline
for a six-page article with three headings. With the heading outline change,
`mutool show OUTPUT.pdf outline` reports `1 Introduction` and its child
`1.0.1 Nested detail` on page 1, and `2 Conclusion` on page 6. Both PDFs use
the same resolved input and DejaVu 2.37 fonts. Poppler reads the candidate as
a six-page tagged PDF 2.0.

Regression tests independently parse the generated PDF with lopdf and verify
numbered titles, parent/child and sibling links, skipped heading levels, real
page destinations after pagination, and omission of outlines for documents
without headings. The workspace suite (43 tests including doctests), Clippy
with warnings denied, formatting, and all 46 pinned conformance files pass.
This local check does not change hosted rollout or PDF/UA readiness gates.

## Current replacement candidate

Release readiness requires standards compliance, complete feature support and
readable, complete rendering. Exact legacy typography, geometry, line breaks,
pagination and raster similarity are no longer requirements. The frozen legacy
comparison is an optional diagnostic and does not run in the required CI job.
Its previous nine failed comparisons do not block publication.

Local verification now includes all 721 JavaScript tests, 14 Rust tests,
TypeScript checking, Clippy, and the optimized worker container build using two
compiler workers. Hosted tests check Poppler and PDF.js extraction independently;
interactive Acrobat selection still needs separate evidence. Every profile is
also tested against strict PDF/UA-1 and PDF/UA-2 validation. Passing validation
does not close the rendering quality, reader, distribution or performance gates in
[READINESS.json](READINESS.json).

Local comparison artifacts are retained in
`/tmp/publisle-native-local/parity/`, including both PDFs, page and figure renders,
differences and per-fixture reports. These are diagnostic artifacts, not release
evidence for a later compiler image. The existing production image and its
measurements below have not been replaced by this candidate.

## Historical staged rollout measurement

The generic article profile is ready for the staged hosted rollout. Journal
profiles remain experimental pending complete visual fidelity review. Arabic
remains on LuaLaTeX because geometry-based copying/extraction differs for some
ligatures and diacritics even though the logical tagged text is correct.

Candidate worker image:
`sha256:1d1a21572ec68e5a3e8c08301312b38e003af07b64e9c9be60bfca99f6c378dc`.
The image contains the native executable, pinned fonts, Node and the bounded image
preparation wrapper; no TeX installation. CLI use is explicit with `--engine rust`.

Actual VPS measurements ran the API preparation path, image fetching and fresh
sandbox compilation inside the existing aggregate **0.5 CPU / 512 MiB** slice,
with swap disabled. PDF caching was disabled. Each fixture had five discarded
warmups followed by 30 uncached observations:

| Fixture                    |  Median |     p95 |
| -------------------------- | ------: | ------: |
| Fourier playground article | 3.018 s | 3.708 s |
| Research manuscript        | 2.878 s | 3.589 s |

These are service execution times on the measured VPS, including public image
fetches. Browser transfer time and queueing depend on the client and workload.
The under-five-second target applies to these validated fixtures, not arbitrary
long documents or concurrent clients.

Validation:

- All seven candidate profiles compiled the research fixture and passed the
  checksum-pinned veraPDF 1.28.2 PDF/UA-2 validation profile.
- The actual VPS Fourier PDF also passed PDF/UA-2 validation. Its title,
  abstract, section numbering, equations, figure/fallback content, references,
  and footnotes were reviewed; no exact repeated source paragraph was found.
- TrueType shaping clusters retain logical Unicode text in the structure tree.
  Arabic viewer copying remains a known rollout blocker for that profile.
- Native browser downloads preserve immediate source, block, and JSON edits.
  Unsupported math and cancellation clean up their job directories.
- Rust tests include mixed-direction placement and punctuation staying with
  inline mathematics. Independent fixture tests check searchable text, tags,
  and bibliography retention.

Reproduce compiler and container validation using [README.md](README.md) and the
[native service instructions](../../deploy/vps/README.md#native-rust-candidate-and-rollout-gate).
A different compiler/font image requires a new verification record. Publisher
approval and universal reader compatibility are not inferred from validator
results.


## Local code continuation verification — 26 October 2026

Issue #12's blank lines, framing and language projection shipped in Rust #33
and TypeScript #128. A follow-up reproduction found that every piece on a
wrapped line gained 12 pt of width in addition to the line's 12 pt indent.
With a 50-token monospace fixture, Poppler measured normal spaces at 4.816 pt
and continuation spaces at 16.816 pt; the rightmost token reached x=529.26 pt,
outside the code frame. Applying the indent once at the line origin retains
4.816 pt spaces on both lines and keeps the rightmost token at x=445.26 pt.

The new wrapping regression fails on unchanged main and passes with the fix.
It checks that all 50 tokens survive, continuations have one hanging indent,
shaped word spacing is unchanged, and every token remains inside its frame.
All 42 workspace tests including doctests, Clippy with warnings denied,
formatting and 46 pinned conformance files pass. Independent Poppler extraction
and a rendered page confirm the blank line, separate frames, indentation and
complete source text. This local evidence does not change hosted or PDF/UA
readiness gates.
