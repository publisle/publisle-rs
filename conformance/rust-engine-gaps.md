# Rust (native) PDF engine vs LuaLaTeX: gap report

This was tested on Fri 23 Oct 2026 between 20:41 and 21:30 UTC+3.
- **Live PDF service:** api.publisle.xyz, PDF service deployed at 12:38 today, native compiler `publisle-pdf 0.3.0`.
- **Repo:** main at cc5f666.
- **Method:** every export used a unique title, so nothing was served from cache.

This was read-only work; nothing was posted.

Where things are:
- **Fixtures:** `rust-gaps/fixtures/` holds Publisle Markdown (`.md`), the converted document (`.json`), and the 3 Playground sample bodies.
- **PDFs:** `rust-gaps/pdf/`.
- **Side-by-side images:** `rust-gaps/png/`.
- **Local reproduction:** `rust-gaps/local.mjs` runs the same native binary and fonts as the hosted service (`@publisle/pdf-native-linux-x64@0.3.0`), and returns the detailed diagnostics that the service hides.
- **Math command scan:** `rust-gaps/math-scan.tsv`.

## Verdict
**Not ready to be the default.**

What's already good:
- Rust is 10–15× faster.
- Figures are better than LaTeX's: true vectors with selectable text.
- It handles mixed scripts, CJK and Unicode code blocks, all of which make LuaLaTeX fail.

What blocks it, in two groups:
1. **Rust aborts the whole export on ordinary content:**
   - very common math commands, including the Greek letters \rho, \xi and \Sigma;
   - wide tables;
   - a tab character;
   - an emoji.

   The service then returns a generic 422 with no PDF.
2. **Display math looks clearly worse than LaTeX:**
   - limits are drawn as scripts instead of above and below;
   - delimiters, matrices and cases don't stretch;
   - variables aren't italic;
   - there's no spacing around operators.

## Timings (first time, uncached, unique title; seconds)
| Fixture | Rust | LaTeX | Notes |
|---|---:|---:|---|
| sample-default | 3.00 | 39.3 | |
| sample-research | 4.08 | 49.8 | |
| sample-fourier | 4.41 | 54.0 | |
| f1-math | 3.11 | 50.1 | |
| f1b-math-extended | **422** (2.56) | 44.4 | Rust: unsupported \hat |
| f2-figures | 3.92 | 42.2 | |
| f3-tables (wide) | **422** (3.14) | 44.6 | Rust: layout-overflow |
| f3b-tables-narrow | 3.09 | 40.4 | |
| f4-code (tab and Unicode) | **422** (2.59) | **422** (44.2) | Rust: tab glyph; LaTeX: Unicode in code |
| f4b-code-notab (Unicode) | 2.89 | **422** (43.5) | LaTeX fails |
| f4c-code-ascii | 3.39 | 39.5 | |
| f5-citations (numeric) | 3.10 | 40.9 | |
| f5-citations (elsevier-author-date) | 3.11 | 8.50 (pdflatex) | |
| f6-structure | 2.89 | 42.8 | |
| f7-unicode-long (emoji and CJK) | **422** (2.87) | **422 resource-limit** (36.9) | |
| f7b (CJK, no emoji) | 4.21 | **422 resource-limit** (30.9) | LaTeX exceeds memory |
| f7c (Arabic inline, symbols) | 3.42 | **422** (44.4) | LaTeX fails |
| f7d-long-plain (5 pages) | 3.32 | 41.7 | |
| f8-arabic (article-arabic) | 3.04 | 43.9 | |

- **Rust:** 2.9–4.4 s on every fixture.
- **LaTeX:** 39–54 s (pdflatex 8.5 s).
- **Cached** (measured earlier today, in v12): about 0.5–0.75 s for both engines.

## Prioritised gaps
Severity levels: **B** = blocks making Rust the default, **M** = major, **m** = minor. "Shared" means the bug is in the common projection and affects LaTeX too.

| # | Sev | Gap | Repro fixture | Rust does | LaTeX does | Evidence | Likely code location | Suggested fix |
|---|---|---|---|---|---|---|---|---|
| 1 | **B** | Unsupported math aborts the export | f1b-math-extended; `math-scan.tsv` | **101 of 112** common commands are rejected. The first one aborts the whole PDF with a 422. The list includes Greek \rho \xi \eta \nu \zeta \kappa \chi \psi \Sigma \Pi \Phi \Psi \Lambda \Theta, accents \hat \tilde \dot \widehat, \forall \exists \subset \cup \cap \mapsto \propto \mid \circ \oplus \binom, \sinh \arg \gcd \Pr \bmod, \limits \displaystyle, \overset \underset \stackrel, vmatrix/array/gather/split, \tag \label \boxed \not \Re \Im \circ, and even \$ | Renders everything | png/gap-math-unsupported-commands.png | `native/pdf/src/math.rs` `atom()`: symbol table L297–345, command match L346–518 (`_ => Unsupported command`) | Generate a full symbol table (unicode-math mapping covering Greek, relations, binary operators, arrows, big operators, functions). Add generic accents via combining marks. **Never abort the document:** on an unknown command, emit the TeX source in mono, red, inside the formula, plus a warning. Also add an automatic LaTeX fallback in the service when the native engine fails |
| 2 | **B** | Display-math layout quality | f1-math | ∑/lim/max limits drawn as right-side scripts. Big operators aren't enlarged. \left( \right), pmatrix, bmatrix and cases delimiters are single-line glyphs, and matrix B's second row sits outside its bracket. Variables are upright. No spacing around = ≤ + | TeX-quality layout | **png/gap-math-display.png**, png/f1-math-p1.png, p2 | `math.rs layout()` L631–860: every atom is shaped with the "regular" text font (L633) and fixed ascent/descent (0.85/0.22). `Row` has no spacing between atoms. `Scripts` (L692) always places scripts on the right. `Grid` (L823) gives cells a fixed height with no delimiter. `left`/`right` (L469) just reads the next atom | Use an OpenType MATH font (STIX Two Math, Latin Modern Math or Noto Sans Math) with its MATH table for stretchy delimiters and big operators, via ttf-parser/rustybuzz. Add math styles (display, text, script) and TeX atom classes (ord, op, bin, rel, open, close, punct) with standard spacing. Place limits under and over in display style, italicise single-letter variables (math-italic codepoints), and size \left/\right to their content |
| 3 | **B** | A wide table aborts the export | f3-tables | 422 `layout-overflow`: "An unbreakable text or math span exceeds the column width" | Renders it (narrow but complete) | png/gap-wide-table.png, gap-wide-table-latex-only.png | `native/pdf/src/layout.rs` Table block L1084–1245: `cell_width = self.width()/cols` gives equal columns. `wrap()` L1295 errors on any word wider than its column | Measure column widths from content (min and max word widths). Allow emergency breaks or hyphenation inside a word. Shrink the table font to 7–7.5 pt, and only then warn and let it overflow into the margin. Never fail the document |
| 4 | **B** | One unsupported character aborts the export | f4-code (tab), f7-unicode-long (🙂) | 422 `missing-glyph`: 'Font shaping failed for "\t"' and 'No pinned font covers "🙂"' | Tab: fine. Emoji: LuaLaTeX failed for a different reason (CJK memory) | png/gap-tab-emoji-abort.png | `layout.rs` Code block L921 (no tab expansion). `native/pdf/src/fonts.rs` `select()` L77–110 (fails if no font covers the run) | Expand tabs to 4 spaces. Fall back to .notdef or U+FFFD with a warning. Add a monochrome emoji or symbol fallback (Noto Emoji or Noto Sans Symbols 2) to `deploy/vps/native-fonts.json` and the npm package fonts |
| 5 | **B** (for author-date profiles) / M | Author-date bibliography drops fields | f5-citations (elsevier-author-date) | Entries are only "Author (Year). Title. Container." No volume, issue, pages, DOI, URL or publisher | Full entries, with doi and URL | png/gap-authordate-bibliography.png (right side is pdflatex) | `packages/research/src/csl.ts` `AUTHOR_DATE_STYLE` L44–66. Only the native path uses it; LaTeX uses biblatex. `NUMERIC_STYLE` L19–41 also has no publisher, edition or URL (Knuth's book loses Addison-Wesley) | Add volume(issue):page, publisher, edition, DOI and URL to both built-in styles. Better: ship real CSL files (APA or Elsevier Harvard, IEEE) |
| 6 | M | The service hides native diagnostics | every Rust 422 | "PDF compilation failed. Check figures, math, citations, and document size." | Gives a specific message where it can (e.g. "Unresolved citations: …") | compile.out / timings.tsv | `packages/pdf-service/src/worker.ts` L479–483 replaces every failure with a generic string. `deploy/vps/server.mjs` L675 does the same | Pass the native report's structured diagnostics (`code`, `message`, `blockIndex`; bounded, with no host paths) to the client, and map `blockIndex` to the block ID (`blockIds` from `createNativePdfPackage`) so the Playground can highlight the failing block |
| 7 | M | Footnote markers aren't superscript | sample-research, f5, f8 | "novelty.1" at full size on the baseline. In Arabic it reads as a digit ("١٢٣1.") | Superscript ¹, linked to the note | **png/gap-footnote-marker.png**, gap-arabic-body.png | `layout.rs` `pieces()` L542–600: Text with `reference.kind=="footnote"` gets the resolved label but the normal role and size | Render as a superscript (about 0.65× size, raised about 0.35 em), add a link annotation to the note, and keep the marker LTR inside RTL text |
| 8 | M | No PDF outline (bookmarks) | all | 0 outline entries | 1–6 entries (headings) | props.jsonl | `native/pdf/src/pdf.rs`: no `/Outlines` is written | Build the outline from the H1–H6 structure tags, and set `/PageMode /UseOutlines` |
| 9 | M | Inline math is display-sized | f1-math, sample-research | Inline \frac and \sum are drawn at full display size, which pushes lines apart. Upright letters | Text-style fractions and limits | **png/gap-inline-math.png** | `math.rs` Fraction L668 always uses 0.8× size, and there's no text-style flag in `layout()` | Pass a style (display or text) into `layout()`. In text style use small fractions and right-side limits, and cap the ascender and descender added to the line |
| 10 | M | No justification or hyphenation; widows | f7d-long-plain | Ragged right; **0** hyphenated line ends. Single-line widows at the top of a page (p2 starts "and the careful handling…") | Justified with **64** hyphenations, and widow/orphan control | png/gap-justification-hyphenation.png | `layout.rs` `wrap()` L1295 is greedy. `visual_positions()` L1343 only aligns left or right. `ensure()` L419 has no widow logic | Add the `hyphenation` crate with en patterns (Arabic and CJK skipped), justify by stretching inter-word space on all but the last line, optionally use Knuth-Plass, and keep at least 2 lines at a page break |
| 11 | m | Unlabeled display equations aren't numbered | sample-fourier | Only labelled equations get a number, so the Fourier sample shows (1) only | Numbers every display: (1), (2), (3) | png/sample-fourier-p2.png | `layout.rs` Math block L927–978 (`if label.is_some()`) | Pick one policy and use it in both engines (LaTeX's numbering of unlabelled displays is arguably the bug) |
| 12 | m | Code blocks | f4c-code-ascii, f4b | Blank lines are dropped. Wrapped continuation lines lose their indent. No background or frame, and consecutive blocks run together. The language is dropped, so there's no highlighting | Keeps blank lines but **clips** long lines; no highlighting either | **png/gap-code-blocks.png**, f4b-code-rust-only.png | `layout.rs` Code L921 (one paragraph per `line`; empty lines collapse). `model.rs` `Block::Code {value}` has no language. `packages/research/src/article.ts` L320 | Keep empty lines, use a hanging indent on wrapped lines, add a light background, rule and block spacing, and carry `language` through for optional highlighting |
| 13 | m | Math fonts ignored silently | scan | \mathcal, \mathbf, \mathit and \mathsf render as plain text (no script or bold); \operatorname is italic-less text | Correct styles | math-scan.tsv | `math.rs` L397–431 maps these to `Math::Text` without a style (only \mathbb is mapped) | Map them to the Mathematical Alphanumeric Symbols codepoints (𝓛, 𝐯, …), which a MATH font covers |
| 14 | m | Bold italic renders as italic only | f6-structure | "***bold italic***" comes out italic | Same | f6-structure-p1.png | `deploy/vps/native-fonts.json`: NotoSerif-BoldItalic has role "fallback". `model.rs` `Inline::Text.mark` holds one mark | Add a `bolditalic` role and support combined marks |
| 15 | m | File size 1.8–4.5× larger | f7d-long-plain (203 vs 45 KB), sample-research (165 vs 93) | About 22 KB of content stream per text page; the structure tree isn't in object streams | Compact | props.jsonl | `native/pdf/src/pdf.rs` (per-glyph positioning, no object streams) | Emit TJ arrays per run instead of glyph-by-glyph, compress every stream, and use object streams and an xref stream |
| 16 | m | Front matter and captions | samples | No date line. Short captions are left-aligned rather than centred. Inline math in captions is upright and unspaced. Equation numbers are 9 pt. *(Better than LaTeX: keywords and affiliations are printed.)* | Date, centred captions | sample-*-p1.png, f2-figures-p2.png | `layout.rs` `build()` front matter L114–330, Figure L980, equation number L962 | Cosmetic; align with the template |
| 17 | m, shared | List start, nested lists, table alignment | f6-structure, f3b | `3.` lists restart at 1. Nested lists are flattened and run together ("outer oneinner ainner b"). Column alignment is ignored | **Same** in LaTeX | f6-structure-p1.png, f3b-tables-p1.png | `packages/research/src/article.ts` L243–250 ("nested-list-flattened", which joins without spaces), L300–310 (list `start` dropped). Table `align` isn't projected. `model.rs` `List`/`Table` have no fields for these | Carry `start`, nesting depth and `align` through the projection, and render them in both engines |
| 18 | m, shared | Keywords missing from PDF metadata | all | `/Keywords` is empty | Empty | props.jsonl | `pdf.rs` XMP and Info (L225–240) | Write the subjects to `/Keywords` and to dc:subject |

### Tested and found at parity, or Rust is better
- **Figures:** SVG figures stay vector with selectable text in Rust; LaTeX rasterises them (8 images with smasks). Raster PNGs, captions, numbering and cross-references are fine, and print uses the light variant. Evidence: png/f2-figures-p1.png, f2-figures-p2.png.
- **Citations:** numeric citations, numbering, repeats and grouped citations ([2, 3]), DOIs as clickable URIs (18 link annotations against LaTeX's 4), and long URLs breaking at slashes.
- **Headings:** numbering to depth 4 (1.1.1.1). LaTeX mis-numbers level 4 as 1.1.2.
- **Line breaks:** hard line breaks are kept (LaTeX drops them).
- **Mixed scripts:** Latin accents, Greek, Cyrillic, inline Arabic, Chinese, Japanese and Korean all render in Rust (png/f7b-unicode-rust-p1.png); LuaLaTeX hits the memory limit.
- **Arabic article:** right-to-left layout, Naskh shaping, a correct right-to-left table column order (LaTeX's order is left-to-right), and math kept left-to-right (png/f8-arabic-p1.png).
- **PDF properties:** both produce tagged PDF 2.0 with PDF/UA-2 identification, /Lang, all fonts embedded and subset, and Title and Author set. Neither declares PDF/A.
- **Text extraction:** Rust 649 words against LaTeX 665. The difference is LaTeX's date line and its hyphen splits; nothing is lost.

### LaTeX-side bugs found along the way (outside Rust's scope)
- An empty "References" heading is printed in documents with no bibliography (default sample, f1, f2, f3).
- Figures and tables float to after "References".
- Unicode in code blocks (f4b) and inline Arabic or symbols (f7c) make LuaLaTeX fail.
- CJK hits a resource-limit failure.
- Long code lines are clipped.
- Arabic tables come out with their columns in left-to-right order.

## Engine scope as documented in the repo
From `native/pdf/README.md` and `READINESS.json` (gates):
- `svg-fonts` and `python-free-pdf` are at "implementation".
- These are still **blocked**: `vector-figures`, `text-and-inline`, `layout-quality`, `tables`, `math`, `arabic`, `pdf-ua`, `native-distribution`, `sandbox`, `hosted-performance`.
- The README describes math as "a bounded parser … not a TeX interpreter. Unknown commands … return diagnostics", with arbitrary macros and packages unsupported.
- It also says Arabic "remains on the legacy hosted engine". However, live `/pdf/capabilities` lists `rust` for `article-arabic`.

## "Ready to default?" checklist
- [ ] **No whole-document aborts.** Unknown math, a missing glyph, a tab or an over-wide table degrades locally with a warning, and f1b, f3, f4 and f7 all return 200.
- [ ] **Math corpus:** at least 95% of `math-scan.tsv` renders, including all Greek letters, standard relations and operators, accents, \binom, \mathcal and \mathbf, \limits, vmatrix and array.
- [ ] **Display math parity:** limits above and below, stretchy \left/\right and matrix and cases delimiters, math italic, and TeX spacing. A visual review of f1-math against LaTeX shows no obviously worse equation.
- [ ] **Inline math** uses text style, and line spacing stays within +20% of plain lines.
- [ ] **Tables:** content-based column widths. A 9-column table renders without failing or clipping.
- [ ] **Footnote markers** are superscript and linked.
- [ ] **Bibliography:** author-date and numeric entries include volume, issue, pages, publisher, DOI and URL, matching LaTeX's fields for f5.
- [ ] **PDF outline** comes from the headings.
- [ ] **Justification and hyphenation** for Latin scripts, plus widow and orphan control.
- [ ] **Service** returns the native diagnostic and block ID on failure. Optional: auto-fallback to LaTeX.
- [ ] **READINESS.json gates** `math`, `tables`, `text-and-inline` and `layout-quality` are no longer "blocked".
- [ ] **Speed** is kept: p95 under 5 s on the fixtures. Today it's 2.9–4.4 s, so this already passes.
- [ ] **Re-run** this fixture set: all 3 samples and f1–f8 return 200 for Rust, and a side-by-side review shows no blocker or major gap.

Nice to have: an emoji/symbol fallback font, code-block styling and highlighting, smaller files, an outline, and shared fixes for list start, nesting and table alignment.

## Unsupported-command list from the local scan (101 of 112 rejected)
`\hat{x} \tilde{x} \dot{x} \ddot{x} \widehat{AB} \widetilde{AB} \overbrace{a+b} \underline{x} \mathfrak{g} \boldsymbol{\alpha} \rho \xi \eta \zeta \kappa \nu \chi \psi \iota \Sigma \Pi \Phi \Psi \Lambda \Theta \vartheta \ell \hbar \forall \exists \emptyset \subset \subseteq \supset \cup \cap \setminus \mapsto \implies \iff \ll \gg \propto \mid \circ \oplus \otimes \wedge \vee \neg \cdotp \ast \star \dagger \prime \oint \iiint \coprod \bigcup \bigoplus \binom{n}{k} \tbinom{n}{k} \sinh x \arg z \gcd(a,b) \Pr(A) a \bmod b a \pmod{n} \displaystyle\sum_{i=1}^n \sum\limits_{i=1}^n \int\limits_0^1 \stackrel{?}{=} \overset{def}{=} \underset{x}{\arg\min} \begin{vmatrix} a & b \\ c & d \end{vmatrix} \begin{array}{cc} a & b \end{array} \begin{gather} a \\ b \end{gather} \begin{split} a &= b \end{split} \Bigg( x \Bigg) \tag{5} \label{eq:x} \phantom{x} \hspace{1em} \color{red} x \boxed{x} \cancel{x} \not= \lesssim \rightleftharpoons \uparrow \Longrightarrow \hookrightarrow \aleph \Re \Im \angle \triangle \degree 90^\circ \checkmark \$ `
