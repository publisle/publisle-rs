# Native PDF rollout evidence — 22 October 2026

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
