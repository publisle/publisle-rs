use crate::{
    fonts::{Fonts, Shaped},
    math::{self, BoxLayout},
    model::*,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Layout {
    pub pages: Vec<Page>,
    pub tags: Vec<Tag>,
    pub fonts: Fonts,
    pub language: String,
    pub title: String,
    pub authors: String,
    pub keywords: Vec<String>,
}
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub draws: Vec<Draw>,
}
pub struct Tag {
    pub outline: Option<(usize, String)>,
    pub role: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub alt: Option<String>,
    pub mathml: Option<String>,
    pub header: bool,
    pub label: Option<String>,
    pub actual_text: Option<String>,
}
pub struct Draw {
    pub tag: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub kind: DrawKind,
}
pub enum DrawKind {
    Text {
        shaped: Shaped,
        size: f32,
        text: String,
        link: Option<String>,
        target: Option<String>,
    },
    Math(BoxLayout),
    #[cfg(feature = "pdf-import")]
    Pdf {
        asset: Box<crate::assets::PdfAsset>,
        draw_width: f32,
        draw_height: f32,
    },
    Rule {
        width: f32,
        thickness: f32,
    },
    /// Filled background rectangle (code block frames).
    Rect {
        width: f32,
        height: f32,
    },
    Image {
        pixels: Vec<u8>,
        alpha: Option<Vec<u8>>,
        width: u32,
        height: u32,
        draw_width: f32,
        draw_height: f32,
    },
}
struct Piece {
    kind: PieceKind,
    width: f32,
    /// Point size the glyphs were shaped and must be drawn at.
    size: f32,
    ascent: f32,
    descent: f32,
    /// Baseline offset for superscript runs such as footnote markers.
    raise: f32,
    break_after: bool,
}
enum PieceKind {
    Text {
        shaped: Shaped,
        text: String,
        link: Option<String>,
        target: Option<String>,
        strike: bool,
    },
    Math(BoxLayout, String, String),
    Break,
}
struct Builder<'a> {
    input: &'a Input,
    output: Layout,
    cursor: f32,
    column: usize,
    page_width: f32,
    page_height: f32,
    text_width: f32,
    left: [f32; 2],
    top_margin: f32,
    bottom_margin: f32,
    body_size: f32,
    leading: f32,
    centered: bool,
    columns: usize,
    gutter: f32,
    references: HashMap<String, String>,
    citation_cursor: usize,
    bibliography_written: bool,
    heading_counts: [u32; 6],
    min_heading: u8,
    figures: usize,
    tables: usize,
    equations: usize,
    block_index: usize,
    column_top: f32,
    active_page: usize,
    note_reservations: BTreeMap<(usize, usize), f32>,
    note_placements: Vec<(usize, usize, usize, f32)>,
    scheduled_notes: BTreeSet<usize>,
    rendering_notes: bool,
    warnings: Vec<Diagnostic>,
    warned_runs: BTreeSet<String>,
}
pub fn build(input: &Input) -> Result<(Layout, Vec<Diagnostic>)> {
    if input.schema_version != 1 {
        return Err(Diagnostic::new(
            "input-version",
            "Unsupported native compiler input version.",
        ));
    }
    if input.article.title.trim().is_empty() || input.article.authors.is_empty() {
        return Err(Diagnostic::new(
            "missing-metadata",
            "PDF requires a title and at least one author.",
        ));
    }
    if input.article.blocks.len() > 10000 {
        return Err(Diagnostic::new(
            "document-limit",
            "At most 10000 projected blocks are supported.",
        ));
    }
    let profile = crate::profiles::profile(&input.template)?;
    let [page_width, page_height] = profile.paper;
    let columns = profile.columns;
    let gutter = profile.gutter;
    let fonts = Fonts::load(&input.fonts)?;
    let min_heading = input
        .article
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Heading {
                level,
                r#abstract: false,
                ..
            } = b
            {
                Some(*level)
            } else {
                None
            }
        })
        .min()
        .unwrap_or(1);
    let mut builder = Builder {
        input,
        output: Layout {
            pages: vec![],
            tags: vec![],
            fonts,
            language: input.article.language.clone(),
            title: input.article.title.clone(),
            authors: input.article.authors.join(", "),
            keywords: input.article.subjects.clone(),
        },
        cursor: page_height - profile.top,
        column: 0,
        page_width,
        page_height,
        text_width: profile.text_width,
        left: profile.left,
        top_margin: profile.top,
        bottom_margin: profile.bottom,
        body_size: profile.body_size,
        leading: profile.leading,
        centered: true,
        columns,
        gutter,
        references: HashMap::new(),
        citation_cursor: 0,
        bibliography_written: false,
        heading_counts: [0; 6],
        min_heading,
        figures: 0,
        tables: 0,
        equations: 0,
        block_index: 0,
        column_top: page_height - profile.top,
        active_page: 0,
        note_reservations: BTreeMap::new(),
        note_placements: vec![],
        scheduled_notes: BTreeSet::new(),
        rendering_notes: false,
        warnings: Vec::new(),
        warned_runs: BTreeSet::new(),
    };
    builder.index_references();
    builder.new_page()?;
    // Front matter occupies full width before publisher columns begin.
    let body_columns = builder.columns;
    builder.columns = 1;
    let title_tag = builder.tag("Title", None, None, None, false);
    builder.paragraph(
        &[plain(&input.article.title)],
        profile.title_size,
        "bold",
        title_tag,
        0.,
    )?;
    let authors_tag = builder.tag("P", None, None, None, false);
    builder.paragraph(
        &[plain(&input.article.authors.join(", "))],
        11.,
        "regular",
        authors_tag,
        0.,
    )?;
    if let Some(date) = &input.article.date {
        let tag = builder.tag("P", None, None, None, false);
        builder.paragraph(
            &[plain(&format_long_date(date, &input.article.language))],
            10.,
            "regular",
            tag,
            0.,
        )?;
    }
    for author in &input.article.author_details {
        if let Some(affiliation) = &author.affiliation {
            let tag = builder.tag("P", None, None, None, false);
            builder.paragraph(
                &[plain(&format!(
                    "{}{}{}",
                    if input.article.authors.len() > 1 {
                        format!("{} — ", author.name)
                    } else {
                        String::new()
                    },
                    affiliation,
                    if input.template == "acm-journal" {
                        input
                            .template_data
                            .country
                            .as_ref()
                            .map(|country| format!(", {country}"))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                ))],
                9.,
                "regular",
                tag,
                0.,
            )?;
        }
    }
    builder.cursor -= 8.;
    builder.centered = false;
    if input.template == "ieee-journal" {
        builder.column_top = builder.cursor;
        builder.columns = body_columns;
    }
    let mut body_started = false;
    for (index, block) in input.article.blocks.iter().enumerate() {
        builder.block_index = index;
        let abstract_content = matches!(
            block,
            Block::Heading {
                r#abstract: true,
                ..
            } | Block::Paragraph {
                r#abstract: true,
                ..
            }
        );
        if !abstract_content && !body_started && !matches!(block, Block::Footnote { .. }) {
            if !input.article.subjects.is_empty() {
                let tag = builder.tag("P", None, None, None, false);
                builder.paragraph(
                    &[plain(&format!(
                        "{}: {}",
                        if builder.rtl() {
                            "الكلمات المفتاحية"
                        } else {
                            "Keywords"
                        },
                        input.article.subjects.join(", ")
                    ))],
                    9.,
                    "regular",
                    tag,
                    0.,
                )?;
            }
            if builder.columns != body_columns {
                builder.column_top = builder.cursor;
            }
            builder.columns = body_columns;
            body_started = true;
        }
        if !matches!(block, Block::Footnote { .. }) {
            builder.block(block).map_err(|error| error.at(index))?;
        }
    }
    if !builder.bibliography_written && !input.bibliography.is_empty() {
        builder.bibliography()?;
    }
    for (index, block) in input.article.blocks.iter().enumerate() {
        if matches!(block, Block::Footnote { .. }) && !builder.scheduled_notes.contains(&index) {
            builder.block(block).map_err(|error| error.at(index))?;
        }
    }
    builder.flush_notes()?;
    if builder.citation_cursor != input.citations.len() {
        return Err(Diagnostic::new(
            "citation-count",
            "Resolved citations do not match the projected article.",
        ));
    }
    for page_index in 0..builder.output.pages.len() {
        let value = (page_index + 1).to_string();
        let shaped = builder.output.fonts.shape(&value, 9., "regular", false)?;
        let x = builder.left[page_index % 2] + builder.text_width / 2. - shaped.width / 2.;
        builder.output.pages[page_index].draws.push(Draw {
            tag: None,
            x,
            y: (builder.bottom_margin - 30.).max(16.),
            kind: DrawKind::Text {
                shaped,
                size: 9.,
                text: value,
                link: None,
                target: None,
            },
        });
    }
    let warnings = std::mem::take(&mut builder.warnings);
    Ok((builder.output, warnings))
}
fn plain(value: &str) -> Inline {
    Inline::Text {
        value: value.into(),
        mark: None,
        direction: None,
        reference: None,
    }
}
impl Builder<'_> {
    fn tag(
        &mut self,
        role: &str,
        parent: Option<usize>,
        alt: Option<String>,
        mathml: Option<String>,
        header: bool,
    ) -> usize {
        let id = self.output.tags.len();
        self.output.tags.push(Tag {
            outline: None,
            role: role.into(),
            parent,
            children: vec![],
            alt,
            mathml,
            header,
            label: None,
            actual_text: None,
        });
        if let Some(p) = parent {
            self.output.tags[p].children.push(id);
        }
        id
    }
    fn text_tag(&mut self, parent: usize, text: &str, link: bool) -> usize {
        let complex = text.chars().any(|c| {
            ('\u{0300}'..='\u{036f}').contains(&c) || ('\u{0590}'..='\u{1cff}').contains(&c)
        });
        let tag = if link {
            self.tag("Link", Some(parent), None, None, false)
        } else if complex {
            self.tag("Span", Some(parent), None, None, false)
        } else {
            parent
        };
        if complex {
            self.output.tags[tag].actual_text = Some(text.into());
        }
        tag
    }
    fn width(&self) -> f32 {
        (self.text_width - self.gutter * (self.columns - 1) as f32) / self.columns as f32
    }
    /// Size and embed a PDF or SVG figure as a Form XObject.
    #[cfg(feature = "pdf-import")]
    fn vector_figure(
        path: &std::path::Path,
        fonts: &Fonts,
        text_width: f32,
        is_svg: bool,
    ) -> Result<(DrawKind, f32, f32)> {
        let pdf = if is_svg {
            #[cfg(feature = "svg")]
            {
                crate::assets::PdfAsset::from_svg(path, fonts)?
            }
            #[cfg(not(feature = "svg"))]
            {
                let _ = fonts;
                return Err(Diagnostic::new(
                    "asset-support",
                    "SVG figures are disabled in this build; rebuild with the `svg` feature.",
                ));
            }
        } else {
            crate::assets::PdfAsset::load(path)?
        };
        let width = pdf.bbox[2] - pdf.bbox[0];
        let height = pdf.bbox[3] - pdf.bbox[1];
        let scale = (text_width / width).min(240. / height).min(1.);
        Ok((
            DrawKind::Pdf {
                asset: Box::new(pdf),
                draw_width: width * scale,
                draw_height: height * scale,
            },
            width * scale,
            height * scale,
        ))
    }
    /// Vector figures require the `pdf-import` feature (implied by `svg`).
    #[cfg(not(feature = "pdf-import"))]
    fn vector_figure(
        path: &std::path::Path,
        fonts: &Fonts,
        text_width: f32,
        is_svg: bool,
    ) -> Result<(DrawKind, f32, f32)> {
        let _ = (path, fonts, text_width, is_svg);
        Err(Diagnostic::new(
            "asset-support",
            "PDF and SVG figures are disabled in this build; rebuild with the `pdf-import` feature.",
        ))
    }
    /// Size and embed a raster PNG or JPEG figure.
    #[cfg(feature = "raster")]
    fn raster_figure(
        path: &std::path::Path,
        fonts: &Fonts,
        text_width: f32,
    ) -> Result<(DrawKind, f32, f32)> {
        let img = crate::assets::raster(path, fonts)?;
        let w = img.width();
        let h = img.height();
        let scale = (text_width / w as f32).min(240. / h as f32).min(1.);
        let dw = w as f32 * scale;
        let dh = h as f32 * scale;
        let rgba = img.to_rgba8();
        let alpha = if rgba.pixels().any(|p| p[3] != 255) {
            Some(rgba.pixels().map(|p| p[3]).collect())
        } else {
            None
        };
        Ok((
            DrawKind::Image {
                pixels: img.to_rgb8().into_raw(),
                alpha,
                width: w,
                height: h,
                draw_width: dw,
                draw_height: dh,
            },
            dw,
            dh,
        ))
    }
    /// Raster figures require the `raster` feature.
    #[cfg(not(feature = "raster"))]
    fn raster_figure(
        path: &std::path::Path,
        fonts: &Fonts,
        text_width: f32,
    ) -> Result<(DrawKind, f32, f32)> {
        let _ = (path, fonts, text_width);
        Err(Diagnostic::new(
            "asset-support",
            "Raster figures are disabled in this build; rebuild with the `raster` feature.",
        ))
    }
    fn x(&self) -> f32 {
        self.left[self.active_page % 2] + (self.width() + self.gutter) * self.column as f32
    }
    fn rtl(&self) -> bool {
        match self.input.article.direction.as_deref() {
            Some("rtl") => true,
            Some("ltr") => false,
            _ => {
                self.input.template == "article-arabic"
                    || ["ar", "fa", "he", "ur"].contains(
                        &self
                            .input
                            .article
                            .language
                            .split('-')
                            .next()
                            .unwrap_or("en"),
                    )
            }
        }
    }
    fn new_page(&mut self) -> Result<()> {
        if self.output.pages.len() >= 300 {
            return Err(Diagnostic::new(
                "page-limit",
                "The document exceeds 300 pages.",
            ));
        }
        self.output.pages.push(Page {
            width: self.page_width,
            height: self.page_height,
            draws: vec![],
        });
        self.cursor = self.page_height - self.top_margin;
        self.active_page = self.output.pages.len() - 1;
        self.column_top = self.cursor;
        self.column = 0;
        Ok(())
    }
    fn ensure(&mut self, height: f32) -> Result<()> {
        if height > self.page_height - self.top_margin - self.bottom_margin {
            return Err(Diagnostic::new(
                "layout-overflow",
                "A block is taller than the printable page.",
            ));
        }
        let bottom = self.bottom_margin
            + if self.rendering_notes {
                0.
            } else {
                self.note_reservations
                    .get(&(self.active_page, self.column))
                    .copied()
                    .unwrap_or(0.)
            };
        if self.cursor - height < bottom {
            if self.column + 1 < self.columns {
                self.column += 1;
                self.cursor = self.column_top;
            } else {
                self.new_page()?;
            }
            if self.cursor - height < self.bottom_margin {
                self.new_page()?;
            }
        }
        Ok(())
    }
    fn draw(&mut self, tag: Option<usize>, x: f32, y: f32, kind: DrawKind) {
        self.output.pages[self.active_page]
            .draws
            .push(Draw { tag, x, y, kind });
    }
    fn index_references(&mut self) {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut headings = [0u32; 6];
        for block in &self.input.article.blocks {
            if let Block::Heading {
                level,
                label,
                r#abstract: false,
                ..
            } = block
            {
                let depth = level.saturating_sub(self.min_heading).min(5) as usize;
                headings[depth] += 1;
                for count in headings.iter_mut().skip(depth + 1) {
                    *count = 0;
                }
                if let Some(label) = label {
                    self.references.insert(
                        label.clone(),
                        heading_number(&self.input.template, &headings, depth),
                    );
                }
                continue;
            }
            if let Block::Math {
                value,
                display: true,
                label,
            } = block
                && let Ok(tree) = math::parse(value)
                && let Some(tag) = tree.equation_tag()
            {
                if let Some(label) = label {
                    self.references.insert(label.clone(), tag.to_string());
                }
                continue;
            }
            let (kind, label) = match block {
                Block::Figure { label, .. } => ("figure", label),
                Block::Table { label, .. } => ("table", label),
                Block::Math {
                    label,
                    display: true,
                    ..
                } => ("math", label),
                _ => continue,
            };
            let ordinal = counts.entry(kind).or_default();
            *ordinal += 1;
            if let Some(label) = label {
                self.references.insert(label.clone(), ordinal.to_string());
            }
        }
        let notes: HashMap<_, _> = self
            .input
            .article
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Footnote { id, paragraphs } = b {
                    Some((id.as_str(), paragraphs.as_slice()))
                } else {
                    None
                }
            })
            .collect();
        let mut order = vec![];
        let mut seen = BTreeSet::new();
        for block in &self.input.article.blocks {
            match block {
                Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
                    note_order(inlines, &notes, &mut seen, &mut order)
                }
                Block::List { items, .. } => {
                    note_list_items(items, &notes, &mut seen, &mut order, 0);
                }
                Block::Quote { paragraphs } => {
                    for p in paragraphs {
                        note_order(p, &notes, &mut seen, &mut order);
                    }
                }
                Block::Figure { caption, .. } => note_order(caption, &notes, &mut seen, &mut order),
                Block::Table { rows, caption, .. } => {
                    note_order(caption, &notes, &mut seen, &mut order);
                    for row in rows {
                        for cell in row {
                            note_order(cell, &notes, &mut seen, &mut order);
                        }
                    }
                }
                _ => {}
            }
        }
        for block in &self.input.article.blocks {
            if let Block::Footnote { id, .. } = block
                && seen.insert(id.clone())
            {
                order.push(id.clone());
            }
        }
        for (i, id) in order.into_iter().enumerate() {
            self.references.insert(id, (i + 1).to_string());
        }
    }
    fn pieces(&mut self, inlines: &[Inline], size: f32, role: &str) -> Result<Vec<Piece>> {
        let mut pieces = Vec::new();
        for inline in inlines {
            let (value, mark, direction, link, target, superscript) = match inline {
                Inline::Text {
                    value,
                    mark,
                    direction,
                    reference,
                } => {
                    let (value, target, superscript) = if let Some(r) = reference {
                        let text = self.references.get(&r.target).ok_or_else(|| {
                            Diagnostic::new(
                                "unresolved-reference",
                                format!("Unknown {} reference {}.", r.kind, r.target),
                            )
                        })?;
                        (text.clone(), Some(r.target.clone()), r.kind == "footnote")
                    } else {
                        (value.clone(), None, false)
                    };
                    (
                        value,
                        mark.as_deref(),
                        direction.as_deref(),
                        None,
                        target,
                        superscript,
                    )
                }
                Inline::Link { value, url } => {
                    if !(url.starts_with("https://")
                        || url.starts_with("http://")
                        || url.starts_with("mailto:")
                        || url.starts_with('#'))
                    {
                        return Err(Diagnostic::new(
                            "unsafe-link",
                            "PDF links require http, https, mailto, or a local fragment.",
                        ));
                    }
                    if let Some(target) = url.strip_prefix('#') {
                        if !self.references.contains_key(target) {
                            return Err(Diagnostic::new(
                                "unresolved-reference",
                                format!("Unknown link target {target}."),
                            ));
                        }
                        (value.clone(), None, None, None, Some(target.into()), false)
                    } else {
                        (value.clone(), None, None, Some(url.clone()), None, false)
                    }
                }
                Inline::Citation { cluster, text } => {
                    let value = text
                        .as_ref()
                        .or_else(|| self.input.citations.get(self.citation_cursor))
                        .ok_or_else(|| {
                            Diagnostic::new("citation-count", "Missing resolved citation text.")
                        })?
                        .clone();
                    self.citation_cursor += 1;
                    let target = cluster
                        .get("items")
                        .and_then(|items| items.get(0))
                        .and_then(|item| item.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .map(|id| format!("bib:{id}"));
                    (value, None, None, None, target, false)
                }
                Inline::Break => {
                    pieces.push(Piece {
                        kind: PieceKind::Break,
                        width: 0.,
                        size,
                        ascent: size,
                        descent: 0.,
                        raise: 0.,
                        break_after: true,
                    });
                    continue;
                }
                Inline::Math { value } => {
                    let tree = math::parse(value)?;
                    let b = math::layout(
                        &tree,
                        &mut self.output.fonts,
                        size,
                        [0.; 3],
                        math::Style::Text,
                    )?;
                    let xml = format!(
                        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{}</math>",
                        tree.mathml()
                    );
                    pieces.push(Piece {
                        width: b.width,
                        size,
                        ascent: b.ascent,
                        descent: b.descent,
                        raise: 0.,
                        kind: PieceKind::Math(b, xml, value.clone()),
                        break_after: false,
                    });
                    continue;
                }
            };
            let font_role = match mark {
                Some("strong") => "bold",
                Some("emphasis") => "italic",
                Some("bold-italic") => "bold-italic",
                Some("code") => "mono",
                _ => role,
            };
            let rtl = direction == Some("rtl") || (direction != Some("ltr") && self.rtl());
            // Unicode line-break boundaries keep CJK breakable and preserve all text.
            let mut start = 0;
            for (end, opportunity) in unicode_linebreak::linebreaks(&value) {
                let word = &value[start..end];
                start = end;
                if word.is_empty() {
                    continue;
                }
                // Footnote markers render as linked superscripts, matching the
                // LaTeX reference output.
                let superscript = superscript && word.chars().all(|c| c.is_ascii_digit());
                let word_size = if superscript { size * 0.72 } else { size };
                let shaped = self.shape_checked(word, word_size, font_role, rtl)?;
                let raise = if superscript { size * 0.42 } else { 0. };
                pieces.push(Piece {
                    width: shaped.width,
                    size: word_size,
                    ascent: word_size * 0.9 + raise,
                    descent: word_size * 0.25,
                    raise,
                    kind: PieceKind::Text {
                        shaped,
                        text: word.into(),
                        link: link.clone(),
                        target: target.clone(),
                        strike: mark == Some("strike"),
                    },
                    break_after: opportunity == unicode_linebreak::BreakOpportunity::Allowed
                        || word.chars().last().is_some_and(char::is_whitespace),
                });
            }
        }
        Ok(pieces)
    }
    /// Indentation width for a list nesting depth.
    fn indent_width(depth: usize) -> f32 {
        10. + depth as f32 * 14.
    }
    /// Width reserved for the list marker (hanging indent).
    fn marker_width(ordered: bool) -> f32 {
        if ordered { 16. } else { 12. }
    }
    /// Render a list and its nested sub-lists with hanging markers.
    fn render_list(
        &mut self,
        ordered: bool,
        start: Option<u32>,
        items: &[ListItem],
        depth: usize,
        parent: usize,
    ) -> Result<()> {
        let list = self.tag("L", Some(parent), None, None, false);
        let first = Self::indent_width(depth);
        let rest = first + Self::marker_width(ordered);
        let mut ordinal = start.unwrap_or(1);
        for item in items {
            let li = self.tag("LI", Some(list), None, None, false);
            let body = self.tag("LBody", Some(li), None, None, false);
            let (content, children): (&[Inline], Option<&Vec<ListNode>>) = match item {
                ListItem::Inlines(inlines) => (inlines, None),
                ListItem::Structured { content, children } => (content, Some(children)),
            };
            let marker = if ordered {
                format!("{ordinal}. ")
            } else {
                "\u{2022} ".to_string()
            };
            ordinal = ordinal.saturating_add(1);
            let mut line = vec![plain(&marker)];
            line.extend(content.iter().cloned());
            self.paragraph_hanging(&line, self.body_size, "regular", body, first, rest)?;
            if let Some(children) = children {
                for child in children {
                    self.render_list(child.ordered, child.start, &child.items, depth + 1, body)?;
                }
            }
        }
        Ok(())
    }
    fn paragraph(
        &mut self,
        inlines: &[Inline],
        size: f32,
        role: &str,
        tag: usize,
        indent: f32,
    ) -> Result<()> {
        self.paragraph_hanging(inlines, size, role, tag, indent, indent)
    }
    /// Lay out with a distinct first-line indent so list and code items keep
    /// their marker or padding on wrapped continuation lines.
    fn paragraph_hanging(
        &mut self,
        inlines: &[Inline],
        size: f32,
        role: &str,
        tag: usize,
        first_indent: f32,
        rest_indent: f32,
    ) -> Result<()> {
        let width = self.width() - first_indent.max(rest_indent);
        let lines = self.lines(inlines, size, role, width)?;
        for (line_index, line) in lines.into_iter().enumerate() {
            let indent = if line_index == 0 {
                first_indent
            } else {
                rest_indent
            };
            let ascent = line.iter().map(|p| p.ascent).fold(size * 0.9, f32::max);
            let descent = line.iter().map(|p| p.descent).fold(size * 0.25, f32::max);
            let height = (ascent + descent).max(size * self.leading);
            let notes = if self.rendering_notes {
                vec![]
            } else {
                self.line_notes(&line)?
            };
            self.ensure(height + notes.iter().map(|(_, h)| h).sum::<f32>())?;
            for (index, note_height) in notes {
                self.scheduled_notes.insert(index);
                *self
                    .note_reservations
                    .entry((self.active_page, self.column))
                    .or_default() += note_height;
                self.note_placements
                    .push((index, self.active_page, self.column, note_height));
            }
            self.cursor -= ascent;
            let used = line.iter().map(|p| p.width).sum::<f32>();
            let available = width - indent;
            let positions = visual_positions(
                &line,
                self.rtl(),
                self.x()
                    + indent
                    + if self.centered {
                        (available - used) / 2.
                    } else {
                        0.
                    },
                if self.centered { used } else { available },
            );
            for (piece, x) in line.into_iter().zip(positions) {
                match piece.kind {
                    PieceKind::Text {
                        shaped,
                        text,
                        link,
                        target,
                        strike,
                        ..
                    } => {
                        if strike {
                            self.draw(
                                None,
                                x,
                                self.cursor + size * 0.3 + piece.raise,
                                DrawKind::Rule {
                                    width: piece.width,
                                    thickness: 0.5,
                                },
                            );
                        }
                        let text_tag =
                            self.text_tag(tag, &text, link.is_some() || target.is_some());
                        self.draw(
                            Some(text_tag),
                            x,
                            self.cursor + piece.raise,
                            DrawKind::Text {
                                shaped,
                                size: piece.size,
                                text,
                                link,
                                target,
                            },
                        );
                    }
                    PieceKind::Math(b, xml, alternative) => {
                        let formula =
                            self.tag("Formula", Some(tag), Some(alternative), Some(xml), false);
                        self.draw(Some(formula), x, self.cursor, DrawKind::Math(b));
                    }
                    PieceKind::Break => {}
                }
            }
            self.cursor -= height - ascent;
        }
        self.cursor -= size * 0.45;
        Ok(())
    }
    /// Shape a run, degrading unsupported characters to a visible replacement
    /// glyph with a warning instead of aborting the whole export.
    fn shape_checked(&mut self, word: &str, size: f32, role: &str, rtl: bool) -> Result<Shaped> {
        match self.output.fonts.shape(word, size, role, rtl) {
            Ok(shaped) => Ok(shaped),
            Err(error) => {
                if error.code == "missing-glyph" {
                    let fingerprint =
                        format!("{role}\0{}", word.chars().take(40).collect::<String>());
                    if self.warned_runs.insert(fingerprint) {
                        self.warnings.push(Diagnostic::new(
                            "missing-glyph",
                            format!(
                                "No pinned font covers {:?}; rendered as a replacement glyph.",
                                word.chars().take(40).collect::<String>()
                            ),
                        ));
                    }
                    if let Ok(shaped) = self.output.fonts.shape("\u{fffd}", size, role, rtl) {
                        return Ok(shaped);
                    }
                    if let Ok(shaped) = self.output.fonts.shape("\u{fffd}", size, "fallback", rtl) {
                        return Ok(shaped);
                    }
                }
                Err(error)
            }
        }
    }
    fn lines(
        &mut self,
        inlines: &[Inline],
        size: f32,
        role: &str,
        width: f32,
    ) -> Result<Vec<Vec<Piece>>> {
        wrap(self.pieces(inlines, size, role)?, width)
    }
    fn line_notes(&mut self, line: &[Piece]) -> Result<Vec<(usize, f32)>> {
        let mut indices = BTreeSet::new();
        for piece in line {
            if let PieceKind::Text {
                target: Some(target),
                ..
            } = &piece.kind
                && let Some(index) = self
                    .input
                    .article
                    .blocks
                    .iter()
                    .position(|b| matches!(b,Block::Footnote{id,..} if id==target))
                && !self.scheduled_notes.contains(&index)
            {
                indices.insert(index);
            }
        }
        let saved = self.citation_cursor;
        let mut out = vec![];
        for index in indices {
            let block = self.input.article.blocks[index].clone();
            if let Block::Footnote { id, paragraphs } = block {
                let mut height = 8.;
                for (i, paragraph) in paragraphs.iter().enumerate() {
                    let mut content = vec![];
                    if i == 0 {
                        content.push(plain(&format!(
                            "{}. ",
                            self.references.get(&id).cloned().unwrap_or_default()
                        )));
                    }
                    content.extend(paragraph.iter().cloned());
                    for line in self.lines(&content, 9., "regular", self.width())? {
                        let a = line.iter().map(|p| p.ascent).fold(8.1, f32::max);
                        let d = line.iter().map(|p| p.descent).fold(2.25, f32::max);
                        height += (a + d).max(9. * self.leading);
                    }
                    height += 4.05;
                }
                out.push((index, height));
            }
        }
        self.citation_cursor = saved;
        Ok(out)
    }
    fn flush_notes(&mut self) -> Result<()> {
        self.rendering_notes = true;
        let mut used: BTreeMap<(usize, usize), f32> = BTreeMap::new();
        for (index, page, column, height) in self.note_placements.clone() {
            self.active_page = page;
            self.column = column;
            let offset = used.entry((page, column)).or_default();
            self.cursor =
                self.bottom_margin + self.note_reservations[&(page, column)] - *offset - 4.;
            if *offset == 0. {
                self.draw(
                    None,
                    self.x(),
                    self.cursor + 2.,
                    DrawKind::Rule {
                        width: self.width().min(90.),
                        thickness: 0.4,
                    },
                );
            }
            let block = self.input.article.blocks[index].clone();
            self.block(&block).map_err(|error| error.at(index))?;
            if self.active_page != page {
                return Err(Diagnostic::new(
                    "footnote-overflow",
                    "Footnote layout exceeded its reserved space.",
                ));
            }
            *offset += height;
        }
        self.rendering_notes = false;
        Ok(())
    }
    fn block(&mut self, block: &Block) -> Result<()> {
        match block {
            Block::Heading {
                level,
                inlines,
                r#abstract,
                label,
            } => {
                let _ = label;
                self.ensure(48.)?;
                let depth = level.saturating_sub(self.min_heading).min(5) as usize;
                let mut title = inlines.clone();
                if !r#abstract {
                    self.heading_counts[depth] += 1;
                    for c in self.heading_counts.iter_mut().skip(depth + 1) {
                        *c = 0;
                    }
                    let prefix = heading_number(&self.input.template, &self.heading_counts, depth);
                    title.insert(0, plain(&format!("{prefix} ")));
                }
                let tag = self.tag(
                    &format!("H{}", if *r#abstract { 1 } else { depth + 1 }),
                    None,
                    None,
                    None,
                    false,
                );
                self.output.tags[tag].label = label.clone();
                let mut citation = self.citation_cursor;
                self.output.tags[tag].outline = Some((
                    if *r#abstract { 0 } else { depth },
                    title
                        .iter()
                        .map(|inline| match inline {
                            Inline::Text { value, .. }
                            | Inline::Math { value }
                            | Inline::Link { value, .. } => value.as_str(),
                            Inline::Citation { text, .. } => {
                                let value =
                                    text.as_ref().or_else(|| self.input.citations.get(citation));
                                citation += 1;
                                value.map(String::as_str).unwrap_or("")
                            }
                            Inline::Break => " ",
                        })
                        .collect::<String>(),
                ));
                self.cursor -= 6.;
                let ieee = self.input.template == "ieee-journal" && !r#abstract;
                self.centered =
                    (ieee && depth == 0) || (*r#abstract && self.input.template != "ieee-journal");
                self.paragraph(
                    &title,
                    if ieee || *r#abstract {
                        self.body_size
                    } else if depth == 0 {
                        self.body_size + 3.
                    } else {
                        self.body_size + 1.
                    },
                    if ieee && depth > 0 { "italic" } else { "bold" },
                    tag,
                    0.,
                )?;
                self.centered = false;
            }
            Block::Paragraph {
                inlines,
                r#abstract,
            } => {
                let _ = r#abstract;
                let tag = self.tag("P", None, None, None, false);
                self.paragraph(inlines, self.body_size, "regular", tag, 0.)?;
            }
            Block::List {
                ordered,
                start,
                items,
            } => {
                let list = self.tag("L", None, None, None, false);
                self.render_list(*ordered, *start, items, 0, list)?;
            }
            Block::Quote { paragraphs } => {
                let quote = self.tag("BlockQuote", None, None, None, false);
                for paragraph in paragraphs {
                    let tag = self.tag("P", Some(quote), None, None, false);
                    self.paragraph(paragraph, self.body_size, "italic", tag, 14.)?;
                }
            }
            Block::Code { value, .. } => {
                // Lay every source line out first so the frame can be sized,
                // then paint background, border and text in that order.
                let content_width = self.width() - 18.;
                let mut visual: Vec<(Vec<Piece>, bool)> = Vec::new();
                for line in value.lines() {
                    // Tab characters have no pinned glyph; expand to spaces.
                    let expanded = line.replace('\t', "    ");
                    let text = plain(if expanded.is_empty() { " " } else { &expanded });
                    let wrapped = self.lines(&[text], 8., "mono", content_width - 12.)?;
                    let empty = wrapped.is_empty();
                    for (index, line) in wrapped.into_iter().enumerate() {
                        // Apply the hanging indent to the line origin below;
                        // shaped advances must retain the source's word spacing.
                        visual.push((line, index > 0));
                    }
                    if empty {
                        visual.push((vec![], false));
                    }
                }
                let line_height = 8. * self.leading;
                let height = 8. + visual.len() as f32 * line_height + 8.;
                self.ensure(height)?;
                let top = self.cursor;
                self.draw(
                    None,
                    self.x(),
                    top - height,
                    DrawKind::Rect {
                        width: self.width(),
                        height,
                    },
                );
                self.cursor -= 4.;
                let tag = self.tag("Code", None, None, None, false);
                for (line, continuation) in &visual {
                    let ascent = line.iter().map(|p| p.ascent).fold(7.2, f32::max);
                    let descent = line.iter().map(|p| p.descent).fold(2., f32::max);
                    self.cursor -= (ascent + descent + 1.8).max(line_height);
                    let offset = if *continuation { 18. } else { 6. };
                    let positions = visual_positions(
                        line,
                        self.rtl(),
                        self.x() + 6. + offset,
                        content_width - offset,
                    );
                    for (piece, x) in line.iter().zip(positions) {
                        if let PieceKind::Text {
                            shaped,
                            text,
                            link,
                            target,
                            ..
                        } = &piece.kind
                        {
                            let text_tag =
                                self.text_tag(tag, text, link.is_some() || target.is_some());
                            self.draw(
                                Some(text_tag),
                                x,
                                self.cursor,
                                DrawKind::Text {
                                    shaped: shaped.clone(),
                                    size: piece.size,
                                    text: text.clone(),
                                    link: link.clone(),
                                    target: target.clone(),
                                },
                            );
                        }
                    }
                }
                self.cursor = top - height + 4.;
                self.cursor -= 6.;
            }
            Block::Math {
                value,
                display,
                label,
            } => {
                let tree = math::parse(value)?;
                let explicit = tree.equation_tag();
                let number = if *display {
                    Some(format!(
                        "({})",
                        explicit
                            .map(str::to_owned)
                            .unwrap_or_else(|| (self.equations + 1).to_string())
                    ))
                } else {
                    None
                };
                let rtl = self.rtl();
                let number_shape = number
                    .as_ref()
                    .map(|text| {
                        self.output
                            .fonts
                            .shape(text, self.body_size, "regular", rtl)
                    })
                    .transpose()?;
                let number_margin = number_shape
                    .as_ref()
                    .map_or(0., |shape| shape.width + self.body_size * 0.5);

                let mut b = math::layout(
                    &tree,
                    &mut self.output.fonts,
                    if *display {
                        self.body_size + 1.
                    } else {
                        self.body_size
                    },
                    [0.; 3],
                    if *display {
                        math::Style::Display
                    } else {
                        math::Style::Text
                    },
                )?;
                let scale = ((self.width() - 2. * number_margin) / b.width).min(1.);
                if scale < 0.65 {
                    return Err(Diagnostic::new(
                        "layout-overflow",
                        "A display equation exceeds the readable column width.",
                    ));
                }
                b.scale(scale);
                let h = b.ascent + b.descent + 18.;
                let descent = b.descent;
                self.ensure(h)?;
                self.cursor -= b.ascent + 6.;
                let x = self.x() + (self.width() - b.width) / 2.;
                let formula = match &tree {
                    publisle_math::Math::Tagged { body, .. } => body.as_ref(),
                    _ => &tree,
                };
                let tag=self.tag("Formula",None,Some(value.clone()),Some(format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">{}</math>",formula.mathml())),false);
                self.output.tags[tag].label = label.clone();
                self.draw(Some(tag), x, self.cursor, DrawKind::Math(b));
                if let (Some(text), Some(shaped)) = (number, number_shape) {
                    if explicit.is_none() {
                        self.equations += 1;
                    }
                    let n = self.tag("P", None, None, None, false);

                    self.draw(
                        Some(n),
                        self.x() + self.width() - shaped.width,
                        self.cursor,
                        DrawKind::Text {
                            shaped,
                            size: self.body_size,
                            text,
                            link: None,
                            target: None,
                        },
                    );
                }
                self.cursor -= descent + 12.;
            }
            Block::Figure {
                src,
                alt,
                caption,
                label,
            } => {
                let _ = label;
                if alt.trim().is_empty() {
                    return Err(Diagnostic::new(
                        "missing-alt",
                        "A PDF figure requires a nonempty alternative.",
                    ));
                }
                let asset = self
                    .input
                    .assets
                    .iter()
                    .find(|asset| asset.source == *src)
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "missing-asset",
                            format!("No validated local asset for {src}."),
                        )
                    })?;
                let meta = std::fs::metadata(&asset.path)
                    .map_err(|e| Diagnostic::new("missing-asset", e.to_string()))?;
                if meta.len() > 16 * 1024 * 1024 {
                    return Err(Diagnostic::new("image-limit", "Image file exceeds 16 MiB."));
                }
                let (kind, dw, dh) = if asset
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case("pdf") || s.eq_ignore_ascii_case("svg"))
                {
                    Self::vector_figure(
                        &asset.path,
                        &self.output.fonts,
                        self.width(),
                        asset
                            .path
                            .extension()
                            .is_some_and(|s| s.eq_ignore_ascii_case("svg")),
                    )?
                } else {
                    Self::raster_figure(&asset.path, &self.output.fonts, self.width())?
                };
                self.ensure(dh + 48.)?;
                self.cursor -= dh;
                let tag = self.tag("Figure", None, Some(alt.clone()), None, false);
                self.output.tags[tag].label = label.clone();
                self.draw(
                    Some(tag),
                    self.x() + (self.width() - dw) / 2.,
                    self.cursor,
                    kind,
                );
                self.cursor -= 10.;
                self.figures += 1;
                let tag = self.tag("Caption", Some(tag), None, None, false);
                let mut text = vec![plain(&format!(
                    "{} {}. ",
                    if self.rtl() { "الشكل" } else { "Figure" },
                    self.figures
                ))];
                text.extend(caption.iter().cloned());
                // Short captions centre under the figure; long ones wrap left.
                let fits = self
                    .lines(&text, 9., "regular", self.width())
                    .map(|lines| lines.len() == 1)
                    .unwrap_or(false);
                let centered = self.centered;
                self.centered = fits;
                self.paragraph(&text, 9., "regular", tag, 0.)?;
                self.centered = centered;
            }
            Block::Table {
                rows,
                caption,
                header_rows,
                align,
                label,
            } => {
                let _ = label;
                self.ensure(64.)?;
                self.tables += 1;
                let table = self.tag("Table", None, None, None, false);
                self.output.tags[table].label = label.clone();
                let tag = self.tag("Caption", Some(table), None, None, false);
                let mut text = vec![plain(&format!(
                    "{} {}. ",
                    if self.rtl() { "الجدول" } else { "Table" },
                    self.tables
                ))];
                text.extend(caption.iter().cloned());
                let fits = self
                    .lines(&text, 9., "regular", self.width())
                    .map(|lines| lines.len() == 1)
                    .unwrap_or(false);
                let centered = self.centered;
                self.centered = fits;
                self.paragraph(&text, 9., "regular", tag, 0.)?;
                self.centered = centered;
                let cols = rows.first().map(Vec::len).unwrap_or(0);
                if cols == 0 || rows.iter().any(|r| r.len() != cols) {
                    return Err(Diagnostic::new(
                        "invalid-table",
                        "Tables require rectangular nonempty rows.",
                    ));
                }
                if self.input.pdf_standard.is_some()
                    && (*header_rows == 0 || *header_rows > rows.len())
                {
                    return Err(Diagnostic::new(
                        "invalid-table",
                        "Tagged PDF tables require valid headerRows.",
                    ));
                }
                let cell_width = self.width() / cols as f32;
                for (r, row) in rows.iter().enumerate() {
                    let row_tag = self.tag("TR", Some(table), None, None, false);
                    let mut height = 18f32;
                    let mut cells = vec![];
                    let mut notes = BTreeMap::new();
                    for cell in row {
                        // Wide content scales down to fit, matching the
                        // reference renderer's narrow-but-complete behaviour.
                        let mut scale = 1.;
                        let lines = loop {
                            match self.lines(
                                cell,
                                9. * scale,
                                if r < *header_rows { "bold" } else { "regular" },
                                cell_width - 8.,
                            ) {
                                Ok(lines) => break lines,
                                Err(error) if error.code == "layout-overflow" && scale > 0.4 => {
                                    scale *= 0.8;
                                }
                                Err(error) => return Err(error),
                            }
                        };
                        let mut cell_height = 8.;
                        for line in &lines {
                            let ascent = line.iter().map(|p| p.ascent).fold(8.1, f32::max);
                            let descent = line.iter().map(|p| p.descent).fold(2.25, f32::max);
                            cell_height += (ascent + descent + 1.8).max(12.15);
                            if !self.rendering_notes {
                                for (index, h) in self.line_notes(line)? {
                                    notes.insert(index, h);
                                }
                            }
                        }
                        height = height.max(cell_height);
                        cells.push(lines);
                    }
                    self.ensure(height + notes.values().sum::<f32>())?;
                    for (index, note_height) in notes {
                        self.scheduled_notes.insert(index);
                        *self
                            .note_reservations
                            .entry((self.active_page, self.column))
                            .or_default() += note_height;
                        self.note_placements.push((
                            index,
                            self.active_page,
                            self.column,
                            note_height,
                        ));
                    }
                    let top = self.cursor;
                    self.cursor -= height;
                    let x0 = self.x();
                    let rtl = self.rtl();
                    for (c, lines) in cells.into_iter().enumerate() {
                        let header = r < *header_rows;
                        let cell = self.tag(
                            if header { "TH" } else { "TD" },
                            Some(row_tag),
                            None,
                            None,
                            header,
                        );
                        let col = if rtl { cols - 1 - c } else { c };
                        let column_align = align.get(col).copied();
                        let left = x0
                            + col as f32 * cell_width
                            + 4.
                            + match column_align {
                                Some(TableAlign::Center) => {
                                    let used = lines
                                        .iter()
                                        .map(|line| line.iter().map(|p| p.width).sum::<f32>())
                                        .fold(0., f32::max);
                                    ((cell_width - 8. - used) / 2.).max(0.)
                                }
                                Some(TableAlign::Right) => {
                                    let used = lines
                                        .iter()
                                        .map(|line| line.iter().map(|p| p.width).sum::<f32>())
                                        .fold(0., f32::max);
                                    (cell_width - 8. - used).max(0.)
                                }
                                _ => 0.,
                            };
                        let mut y = top - 4.;
                        for line in lines {
                            let ascent = line.iter().map(|p| p.ascent).fold(8.1, f32::max);
                            let descent = line.iter().map(|p| p.descent).fold(2.25, f32::max);
                            y -= ascent;
                            let positions = visual_positions(&line, rtl, left, cell_width - 8.);
                            for (piece, x) in line.into_iter().zip(positions) {
                                match piece.kind {
                                    PieceKind::Text {
                                        shaped,
                                        text,
                                        link,
                                        target,
                                        strike,
                                    } => {
                                        if strike {
                                            self.draw(
                                                None,
                                                x,
                                                y + 2.7,
                                                DrawKind::Rule {
                                                    width: piece.width,
                                                    thickness: 0.5,
                                                },
                                            );
                                        }
                                        let tag = self.text_tag(
                                            cell,
                                            &text,
                                            link.is_some() || target.is_some(),
                                        );
                                        self.draw(
                                            Some(tag),
                                            x,
                                            y + piece.raise,
                                            DrawKind::Text {
                                                shaped,
                                                size: piece.size,
                                                text,
                                                link,
                                                target,
                                            },
                                        );
                                    }
                                    PieceKind::Math(b, xml, alternative) => {
                                        let tag = self.tag(
                                            "Formula",
                                            Some(cell),
                                            Some(alternative),
                                            Some(xml),
                                            false,
                                        );
                                        self.draw(Some(tag), x, y, DrawKind::Math(b));
                                    }
                                    PieceKind::Break => {}
                                }
                            }
                            y -= descent + 1.8;
                        }
                    }
                    self.draw(
                        None,
                        x0,
                        self.cursor,
                        DrawKind::Rule {
                            width: self.width(),
                            thickness: 0.4,
                        },
                    );
                }
                self.cursor -= 10.;
            }
            Block::Footnote { id, paragraphs } => {
                let label = self.references.get(id).cloned().unwrap_or_default();
                let note = self.tag("FENote", None, None, None, false);
                self.output.tags[note].label = Some(id.clone());
                for (i, p) in paragraphs.iter().enumerate() {
                    let mut text = Vec::new();
                    if i == 0 {
                        text.push(plain(&format!("{label}. ")));
                    }
                    text.extend(p.iter().cloned());
                    self.paragraph(&text, 9., "regular", note, 0.)?;
                }
            }
            Block::Bibliography => self.bibliography()?,
        }
        Ok(())
    }
    fn bibliography(&mut self) -> Result<()> {
        if self.bibliography_written {
            return Ok(());
        }
        self.bibliography_written = true;
        if self.input.bibliography.is_empty() {
            return Ok(());
        }
        self.ensure(48.)?;
        let tag = self.tag("H1", None, None, None, false);
        self.paragraph(
            &[plain(if self.rtl() {
                "المراجع"
            } else {
                "References"
            })],
            14.,
            "bold",
            tag,
            0.,
        )?;
        let references = self.tag("Sect", None, None, None, false);
        for entry in &self.input.bibliography {
            let _ = (&entry.id, &entry.label);
            let tag = self.tag("P", Some(references), None, None, false);
            self.output.tags[tag].label = Some(format!("bib:{}", entry.id));
            self.paragraph(&[plain(&entry.text)], 9., "regular", tag, 0.)?;
        }
        Ok(())
    }
}

fn wrap(pieces: Vec<Piece>, width: f32) -> Result<Vec<Vec<Piece>>> {
    let mut lines: Vec<Vec<Piece>> = vec![];
    let mut line = vec![];
    let mut used = 0.;
    for piece in pieces {
        if matches!(piece.kind, PieceKind::Break) {
            lines.push(std::mem::take(&mut line));
            used = 0.;
            continue;
        }
        if piece.width > width {
            return Err(Diagnostic::new(
                "layout-overflow",
                "An unbreakable text or math span exceeds the column width.",
            ));
        }
        if used + piece.width > width && !line.is_empty() {
            let split = line
                .iter()
                .rposition(|p: &Piece| p.break_after)
                .map(|i| i + 1)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "layout-overflow",
                        "An unbreakable inline group exceeds the column width.",
                    )
                })?;
            let carry = line.split_off(split);
            lines.push(std::mem::replace(&mut line, carry));
            used = line.iter().map(|p| p.width).sum();
            if used + piece.width > width {
                return Err(Diagnostic::new(
                    "layout-overflow",
                    "An unbreakable inline group exceeds the column width.",
                ));
            }
        }
        used += piece.width;
        line.push(piece);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    Ok(lines)
}

// Resolve bidi order at line scope, while emitting draw records in logical
// source order for the structure tree and keeping math objects internally LTR.
fn visual_positions(line: &[Piece], rtl: bool, left: f32, width: f32) -> Vec<f32> {
    let mut value = String::new();
    let mut starts = vec![];
    for piece in line {
        starts.push(value.len());
        match &piece.kind {
            PieceKind::Text { text, .. } => value.push_str(text),
            _ => value.push('\u{fffc}'),
        }
    }
    let bidi = unicode_bidi::BidiInfo::new(
        &value,
        Some(if rtl {
            unicode_bidi::Level::rtl()
        } else {
            unicode_bidi::Level::ltr()
        }),
    );
    let mut order = vec![];
    if let Some(paragraph) = bidi.paragraphs.first() {
        let (_, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let mut indices: Vec<_> = starts
                .iter()
                .enumerate()
                .filter_map(|(i, start)| if run.contains(start) { Some(i) } else { None })
                .collect();
            if bidi.levels[run.start].is_rtl() {
                indices.reverse();
            }
            order.extend(indices);
        }
    }
    let mut positions = vec![left; line.len()];
    let mut x = left
        + if rtl {
            width - line.iter().map(|p| p.width).sum::<f32>()
        } else {
            0.
        };
    for i in order {
        positions[i] = x;
        x += line[i].width;
    }
    positions
}

fn heading_number(template: &str, counts: &[u32; 6], depth: usize) -> String {
    if template == "ieee-journal" {
        match depth {
            0 => format!("{}.", roman(counts[0])),
            1 => format!("{}.", alphabetic(counts[1])),
            _ => format!("{})", counts[depth]),
        }
    } else {
        counts[..=depth]
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}
fn roman(mut n: u32) -> String {
    let mut text = String::new();
    for (value, symbol) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while n >= value {
            text.push_str(symbol);
            n -= value;
        }
    }
    text
}
fn alphabetic(mut n: u32) -> String {
    let mut text = vec![];
    while n > 0 {
        n -= 1;
        text.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    text.into_iter().rev().collect()
}
/// Format an ISO date (or date-time) as a long date in the article language.
fn format_long_date(date: &str, language: &str) -> String {
    let day_part = date.get(0..10).unwrap_or(date);
    let parts: Vec<&str> = day_part.split('-').collect();
    let (year, month, day) = match parts.as_slice() {
        [year, month, day] if year.len() == 4 && month.len() == 2 && day.len() == 2 => {
            match (
                year.parse::<u32>(),
                month.parse::<u32>(),
                day.parse::<u32>(),
            ) {
                (Ok(y), Ok(m), Ok(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => {
                    (y, m, d)
                }
                _ => return date.to_string(),
            }
        }
        _ => return date.to_string(),
    };
    let arabic = language.starts_with("ar");
    static EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    static AR: [&str; 12] = [
        "\u{64a}\u{646}\u{627}\u{64a}\u{631}",
        "\u{641}\u{628}\u{631}\u{627}\u{64a}\u{631}",
        "\u{645}\u{627}\u{631}\u{633}",
        "\u{623}\u{628}\u{631}\u{64a}\u{644}",
        "\u{645}\u{627}\u{64a}\u{648}",
        "\u{64a}\u{648}\u{646}\u{64a}\u{648}",
        "\u{64a}\u{648}\u{644}\u{64a}\u{648}",
        "\u{623}\u{63a}\u{634}\u{637}\u{633}",
        "\u{633}\u{628}\u{62a}\u{645}\u{628}\u{631}",
        "\u{623}\u{643}\u{62a}\u{648}\u{628}\u{631}",
        "\u{646}\u{648}\u{641}\u{645}\u{628}\u{631}",
        "\u{62f}\u{64a}\u{633}\u{645}\u{628}\u{631}",
    ];
    if arabic {
        format!("{day} {} {year}", AR[month as usize - 1])
    } else {
        format!("{} {}, {year}", EN[month as usize - 1], day)
    }
}

fn note_list_items(
    items: &[ListItem],
    notes: &HashMap<&str, &[Vec<Inline>]>,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    for item in items {
        let (content, children) = match item {
            ListItem::Inlines(inlines) => (inlines, None),
            ListItem::Structured { content, children } => (content, Some(children)),
        };
        note_order(content, notes, seen, order);
        if let Some(children) = children {
            for child in children {
                note_list_items(&child.items, notes, seen, order, depth + 1);
            }
        }
    }
}

fn note_order(
    inlines: &[Inline],
    notes: &HashMap<&str, &[Vec<Inline>]>,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) {
    let mut stack: Vec<_> = inlines.iter().rev().collect();
    while let Some(inline) = stack.pop() {
        if let Inline::Text {
            reference: Some(reference),
            ..
        } = inline
            && reference.kind == "footnote"
            && seen.insert(reference.target.clone())
        {
            order.push(reference.target.clone());
            if let Some(paragraphs) = notes.get(reference.target.as_str()) {
                for paragraph in paragraphs.iter().rev() {
                    stack.extend(paragraph.iter().rev());
                }
            }
        }
    }
}

#[cfg(test)]
mod bidi_tests {
    use super::*;
    fn piece(text: &str) -> Piece {
        Piece {
            kind: PieceKind::Text {
                shaped: Shaped {
                    glyphs: vec![],
                    width: 10.,
                },
                text: text.into(),
                link: None,
                target: None,
                strike: false,
            },
            width: 10.,
            size: 9.,
            ascent: 8.,
            descent: 2.,
            raise: 0.,
            break_after: true,
        }
    }
    #[test]
    fn punctuation_stays_with_inline_math() {
        let mut prefix = piece("before ");
        prefix.width = 75.;
        let equation = Piece {
            kind: PieceKind::Math(BoxLayout::default(), String::new(), String::new()),
            width: 24.,
            size: 9.,
            ascent: 8.,
            descent: 2.,
            raise: 0.,
            break_after: false,
        };
        let mut dot = piece(".");
        dot.width = 3.;
        dot.break_after = false;
        let lines = wrap(vec![prefix, equation, dot], 100.).unwrap();
        assert_eq!(lines.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 2]);
    }
    #[test]
    fn cannot_break_inside_styled_words() {
        let mut first = piece("unbreak");
        first.break_after = false;
        assert!(wrap(vec![first, piece("able")], 15.).is_err());
    }
    #[test]
    fn mixed_script_words_keep_their_reading_direction() {
        let p = visual_positions(
            &[
                piece("Hello "),
                piece("مرحبا "),
                piece("بالعالم "),
                piece("world"),
            ],
            false,
            0.,
            100.,
        );
        assert!(p[0] < p[2] && p[2] < p[1] && p[1] < p[3]);
    }
    #[test]
    fn arabic_lines_are_right_aligned() {
        let p = visual_positions(&[piece("مرحبا "), piece("بالعالم")], true, 0., 100.);
        assert_eq!(p, [90., 80.]);
    }
}

#[cfg(test)]
mod equation_regressions {
    use super::*;
    #[test]
    fn every_display_equation_is_numbered_and_label_references_keep_ordinal() {
        let directory = std::path::PathBuf::from(
            std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned fonts required"),
        );
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("regression-math.json")).unwrap();
        let mut blocks = fixture["displayEquations"].as_array().unwrap().clone();
        for block in &mut blocks {
            block["kind"] = serde_json::json!("math");
        }
        blocks.push(serde_json::json!({"kind":"math","value":r"x=3\tag{A}","display":true,"label":"eq:custom"}));
        blocks.push(serde_json::json!({"kind":"math","value":"x=4","display":true}));
        blocks.push(serde_json::json!({"kind":"paragraph","inlines":[{"type":"text","value":"2","reference":{"kind":"cross","target":"eq:second"}}]}));
        blocks.push(serde_json::json!({"kind":"paragraph","inlines":[{"type":"text","value":"A","reference":{"kind":"cross","target":"eq:custom"}}]}));
        let input: Input = serde_json::from_value(serde_json::json!({"schemaVersion":1,"template":"article","templateVersion":"1","article":{"title":"Equations","authors":["Ada"],"blocks":blocks},"citations":[],"bibliography":[],"assets":[],"fonts":[{"path":directory.join("DejaVuSerif.ttf"),"role":"regular"}]})).unwrap();
        let (result, _) = build(&input).unwrap();
        let texts: Vec<_> = result
            .pages
            .iter()
            .flat_map(|p| &p.draws)
            .filter_map(|draw| {
                if let DrawKind::Text { text, size, .. } = &draw.kind {
                    Some((text.as_str(), *size))
                } else {
                    None
                }
            })
            .collect();
        let size = crate::profiles::profile("article").unwrap().body_size;
        assert!(texts.contains(&("(1)", size)));
        assert!(texts.contains(&("(2)", size)));
        assert!(texts.contains(&("(A)", size)));
        assert!(texts.contains(&("(3)", size)));
        assert!(!texts.iter().any(|(text, _)| *text == "(4)"));
        assert!(texts.iter().any(|(text, _)| *text == "A"));
        assert!(texts.iter().any(|(text, _)| *text == "2"));
    }
}

#[cfg(test)]
mod gap_regressions {
    use super::*;

    #[cfg(feature = "pdf-import")]
    #[test]
    fn heading_bookmarks_preserve_hierarchy_titles_and_page_destinations() {
        let heading = |level, text: &str| {
            serde_json::json!({
                "kind":"heading", "level":level, "inlines":[{"type":"text","value":text}]
            })
        };
        let mut blocks = vec![heading(1, "First"), heading(3, "Skipped level")];
        for _ in 0..90 {
            blocks.push(serde_json::json!({"kind":"paragraph","inlines":[{"type":"text","value":"A paragraph to force pagination."}]}));
        }
        blocks.push(heading(1, "Later"));
        let input = article_input(&directory(), serde_json::json!(blocks));
        let (bytes, _) = crate::compile(&input).unwrap();
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        let dict = |id| doc.get_object(id).unwrap().as_dict().unwrap();
        let root = dict(doc.trailer.get(b"Root").unwrap().as_reference().unwrap());
        assert_eq!(
            root.get(b"PageMode").unwrap().as_name().unwrap(),
            b"UseOutlines"
        );
        let outline_id = root.get(b"Outlines").unwrap().as_reference().unwrap();
        let outline = dict(outline_id);
        assert_eq!(outline.get(b"Count").unwrap().as_i64().unwrap(), 3);
        let first_id = outline.get(b"First").unwrap().as_reference().unwrap();
        let first = dict(first_id);
        assert_eq!(
            first.get(b"Parent").unwrap().as_reference().unwrap(),
            outline_id
        );
        let child_id = first.get(b"First").unwrap().as_reference().unwrap();
        let child = dict(child_id);
        assert_eq!(
            child.get(b"Parent").unwrap().as_reference().unwrap(),
            first_id
        );
        let last_id = outline.get(b"Last").unwrap().as_reference().unwrap();
        let last = dict(last_id);
        assert_eq!(first.get(b"Next").unwrap().as_reference().unwrap(), last_id);
        assert_eq!(last.get(b"Prev").unwrap().as_reference().unwrap(), first_id);
        for (item, title) in [
            (first, "1 First"),
            (child, "1.0.1 Skipped level"),
            (last, "2 Later"),
        ] {
            let actual = item.get(b"Title").unwrap().as_str().unwrap();
            // pdf-writer emits ASCII PDF strings for these titles.
            assert_eq!(actual, title.as_bytes());
            let dest = item.get(b"Dest").unwrap().as_array().unwrap();
            assert_eq!(dest[1].as_name().unwrap(), b"XYZ");
            assert!(
                doc.get_pages()
                    .values()
                    .any(|id| *id == dest[0].as_reference().unwrap())
            );
        }
        let first_dest = first.get(b"Dest").unwrap().as_array().unwrap();
        let last_dest = last.get(b"Dest").unwrap().as_array().unwrap();
        assert_ne!(first_dest[0], last_dest[0]);
    }

    #[cfg(feature = "pdf-import")]
    #[test]
    fn documents_without_headings_have_no_outline() {
        let input = article_input(
            &directory(),
            serde_json::json!([
                {"kind":"paragraph","inlines":[{"type":"text","value":"Body only."}]}
            ]),
        );
        let (bytes, _) = crate::compile(&input).unwrap();
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        let root = doc
            .get_object(doc.trailer.get(b"Root").unwrap().as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(root.get(b"Outlines").is_err());
        assert!(root.get(b"PageMode").is_err());
    }

    fn font(directory: &std::path::Path) -> serde_json::Value {
        serde_json::json!({"path": directory.join("DejaVuSerif.ttf"), "role": "regular"})
    }

    fn article_input(directory: &std::path::Path, blocks: serde_json::Value) -> Input {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "template": "article",
            "templateVersion": "1",
            "article": {
                "title": "Gap regressions",
                "authors": ["Ada"],
                "subjects": ["crisp", "typesetting"],
                "blocks": blocks
            },
            "citations": [],
            "bibliography": [],
            "assets": [],
            "fonts": [font(directory)]
        }))
        .unwrap()
    }

    fn directory() -> std::path::PathBuf {
        std::path::PathBuf::from(
            std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned fonts required"),
        )
    }

    #[test]
    fn keywords_flow_into_the_layout() {
        let blocks =
            serde_json::json!([{"kind":"paragraph","inlines":[{"type":"text","value":"Body."}]}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        assert_eq!(
            layout.keywords,
            vec!["crisp".to_string(), "typesetting".to_string()]
        );
    }

    #[test]
    fn footnote_markers_render_as_raised_linked_superscripts() {
        let blocks = serde_json::json!([
            {"kind":"paragraph","inlines":[
                {"type":"text","value":"Claimed"},
                {"type":"text","value":"1","reference":{"kind":"footnote","target":"n1"}},
                {"type":"text","value":" elsewhere."}
            ]},
            {"kind":"footnote","id":"n1","paragraphs":[[{"type":"text","value":"Source note."}]]}
        ]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let mut superscript: Option<(f32, f32)> = None;
        let mut body: Option<(f32, f32)> = None;
        for draw in &layout.pages[0].draws {
            if let DrawKind::Text {
                text, size, target, ..
            } = &draw.kind
            {
                if text == "Claimed" {
                    body = Some((*size, draw.y));
                }
                if text == "1" && target.is_some() {
                    superscript = Some((*size, draw.y));
                }
            }
        }
        let (body_size, base) = body.expect("body draw");
        let (size, y) = superscript.expect("superscript marker draw");
        assert!(
            (size - (body_size * 0.72)).abs() < 0.05,
            "marker size {size} vs body {body_size}"
        );
        assert!(y > base, "marker y {y} should sit above baseline {base}");
    }

    #[test]
    fn unsupported_characters_warn_instead_of_aborting() {
        let blocks = serde_json::json!([{"kind":"paragraph","inlines":[
            {"type":"text","value":"here \u{1f642} now"}
        ]}]);
        let input = article_input(&directory(), blocks);
        let (_, report) = crate::compile(&input).unwrap();
        assert!(
            report.diagnostics.iter().any(|d| d.code == "missing-glyph"),
            "expected a missing-glyph warning: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn tabs_in_code_blocks_expand_to_spaces() {
        let blocks = serde_json::json!([{"kind":"code","value":"a\tb"}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let texts: Vec<&str> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|text| text.starts_with('a') && text.trim_end() != *text),
            "expected the tab expanded to spaces in {texts:?}"
        );
    }

    #[test]
    fn wide_table_cells_scale_instead_of_aborting() {
        let long = "unbreakable".repeat(12);
        let blocks = serde_json::json!([{"kind":"table","headerRows":1,"rows":[
            [[{"type":"text","value":"header"}]],
            [[{"type":"text","value":long}]]
        ]}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let sizes: Vec<f32> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { size, text, .. } if text.contains("unbreakable") => Some(*size),
                _ => None,
            })
            .collect();
        assert!(!sizes.is_empty(), "cell text missing");
        assert!(
            sizes.iter().all(|size| *size < 9.),
            "expected the cell to shrink below 9pt, got {sizes:?}"
        );
    }

    #[test]
    fn ordered_lists_honour_start_and_nested_children() {
        let blocks = serde_json::json!([
            {"kind":"list","ordered":true,"start":3,"items":[
                {"content":[{"type":"text","value":"third"}]},
                {"content":[{"type":"text","value":"fourth"}],"children":[
                    {"ordered":false,"items":[{"content":[{"type":"text","value":"nested"}]}]}
                ]}
            ]}
        ]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let texts: Vec<&str> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|text| text.starts_with("3. ")),
            "expected the authored start ordinal, got {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("nested")),
            "expected nested list content, got {texts:?}"
        );
    }

    #[test]
    fn legacy_flat_list_items_still_project() {
        let blocks = serde_json::json!([{"kind":"list","ordered":false,"items":[
            [{"type":"text","value":"flat item"}]
        ]}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let texts: Vec<&str> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|text| text.contains("flat")),
            "expected the legacy flat item, got {texts:?}"
        );
    }

    #[test]
    fn wrapped_code_indents_once_and_preserves_shaped_word_spacing() {
        let value = (0..50)
            .map(|i| format!("token{i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut input = article_input(
            &directory(),
            serde_json::json!([
                {"kind":"code","value":value,"language":"typescript"}
            ]),
        );
        input.fonts.push(
            serde_json::from_value(serde_json::json!({
                "path":directory().join("DejaVuSansMono.ttf"),"role":"mono"
            }))
            .unwrap(),
        );
        let (layout, _) = build(&input).unwrap();
        let page = &layout.pages[0];
        let words: Vec<_> = page
            .draws
            .iter()
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, shaped, .. } if text.starts_with("token") => {
                    Some((draw.x, draw.y, shaped.width))
                }
                _ => None,
            })
            .collect();
        assert_eq!(words.len(), 50, "every source token survives wrapping");
        let continuation = words.iter().find(|word| word.1 < words[0].1).unwrap();
        assert!(
            (continuation.0 - words[0].0 - 12.).abs() < 0.05,
            "continuation has one hanging indent"
        );
        let frame_right = page
            .draws
            .iter()
            .find_map(|draw| match draw.kind {
                DrawKind::Rect { width, .. } => Some(draw.x + width),
                _ => None,
            })
            .unwrap();
        for word in &words {
            assert!(
                word.0 + word.2 <= frame_right,
                "wrapped code remains inside its frame"
            );
        }
        for pair in words.windows(2) {
            if pair[0].1 == pair[1].1 {
                assert!(
                    (pair[1].0 - pair[0].0 - pair[0].2).abs() < 0.05,
                    "indent must not add space after each shaped word"
                );
            }
        }
    }

    #[test]
    fn code_blocks_preserve_blank_lines_and_draw_a_frame() {
        let blocks = serde_json::json!([{"kind":"code","value":"first\n\nlast","language":"rust"}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let mut rects = 0;
        let mut saw_first = false;
        let mut saw_last = false;
        for draw in &layout.pages[0].draws {
            match &draw.kind {
                DrawKind::Rect { .. } => rects += 1,
                DrawKind::Text { text, .. } => {
                    if text.contains("first") {
                        saw_first = true;
                    }
                    if text.contains("last") {
                        saw_last = true;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(rects, 1, "expected exactly one code frame");
        assert!(saw_first && saw_last, "expected both code lines");
    }

    #[test]
    fn authored_dates_render_in_the_front_matter() {
        let mut input = article_input(
            &directory(),
            serde_json::json!([{"kind":"paragraph","inlines":[{"type":"text","value":"Body."}]}]),
        );
        input.article.date = Some("2026-10-25".into());
        let (layout, _) = build(&input).unwrap();
        let texts: Vec<&str> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let joined = texts.concat();
        assert!(
            joined.contains("October 25, 2026"),
            "expected a formatted date line, got {joined:?}"
        );
    }

    #[test]
    fn table_columns_align_per_the_authored_alignment() {
        let blocks = serde_json::json!([{"kind":"table","headerRows":0,"align":["left","right"],"rows":[
            [[{"type":"text","value":"a"}],[{"type":"text","value":"b"}]]
        ]}]);
        let (layout, _) = build(&article_input(&directory(), blocks)).unwrap();
        let xs: Vec<f32> = layout
            .pages
            .iter()
            .flat_map(|page| &page.draws)
            .filter_map(|draw| match &draw.kind {
                DrawKind::Text { text, .. } if text == "b" => Some(draw.x),
                _ => None,
            })
            .collect();
        let single = xs.first().copied().expect("aligned cell draw");
        // A right-aligned second column pushes the cell text right of the
        // left-aligned position (column left edge + 4pt padding).
        assert!(
            single > 4. + 1.,
            "expected right alignment to move the cell, got x={single}"
        );
    }
}
