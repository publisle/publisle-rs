use crate::{
    fonts::Shaped,
    layout::{DrawKind, Layout},
    math,
    model::{Diagnostic, Result},
};
use flate2::{Compression, write::ZlibEncoder};
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use std::{collections::BTreeMap, io::Write};

pub(crate) fn compressed(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
    z.write_all(bytes)
        .map_err(|e| Diagnostic::new("pdf-write", e.to_string()))?;
    z.finish()
        .map_err(|e| Diagnostic::new("pdf-write", e.to_string()))
}
struct Ids(i32);
type StructureEvent = (usize, Option<Ref>, Option<(Ref, i32)>);
impl Ids {
    fn next(&mut self) -> Ref {
        let r = Ref::new(self.0);
        self.0 += 1;
        r
    }
}
fn font_name(index: usize) -> String {
    format!("F{index}")
}
struct TextPaint<'a> {
    shaped: &'a Shaped,
    size: f32,
    position: [f32; 2],
    color: [f32; 3],
}
fn text(
    content: &mut Content,
    fonts: &crate::fonts::Fonts,
    advances: &mut AdvanceCache,
    paint: &TextPaint<'_>,
    cff_cids: &BTreeMap<(usize, u16), u16>,
) {
    if paint.shaped.glyphs.is_empty() {
        return;
    }
    content.set_fill_rgb(paint.color[0], paint.color[1], paint.color[2]);
    content.begin_text();
    // Emit positioned characters in visual order. Geometry-based readers then
    // apply bidi once to the whole word, including its combining marks. The
    // structure tree retains the authored logical text and reading order.
    // One TJ run covers a font and baseline; the adjustment undoes the font
    // width so the next glyph lands on its shaped x.
    let mut glyphs: Vec<_> = paint.shaped.glyphs.iter().collect();
    glyphs.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let mut index = 0;
    while index < glyphs.len() {
        let font = glyphs[index].font;
        let y = glyphs[index].y;
        let name = font_name(font);
        content.set_font(Name(name.as_bytes()), paint.size);
        content.set_text_matrix([
            1.,
            0.,
            0.,
            1.,
            paint.position[0] + glyphs[index].x,
            paint.position[1] + y,
        ]);
        let start = index;
        let mut pen = glyphs[index].x;
        let mut positioned = content.show_positioned();
        let mut items = positioned.items();
        while index < glyphs.len()
            && glyphs[index].font == font
            && (glyphs[index].y - y).abs() < 0.01
        {
            let glyph = glyphs[index];
            if index > start {
                let delta = glyph.x - pen;
                if delta.abs() > 0.01 {
                    items.adjust(-delta * 1000. / paint.size);
                }
            }
            let cid = cff_cids
                .get(&(glyph.font, glyph.cid))
                .copied()
                .unwrap_or(glyph.cid);
            let encoded = cid.to_be_bytes();
            items.show(Str(&encoded));
            pen = glyph.x + advances.advance(fonts, glyph, paint.size);
            index += 1;
        }
    }
    content.end_text();
}

struct AdvanceCache {
    faces: Vec<Option<(f32, BTreeMap<u16, i32>)>>,
}

impl AdvanceCache {
    fn advance(
        &mut self,
        fonts: &crate::fonts::Fonts,
        glyph: &crate::fonts::Glyph,
        size: f32,
    ) -> f32 {
        if self.faces.len() <= glyph.font {
            self.faces.resize_with(glyph.font + 1, || None);
        }
        if self.faces[glyph.font].is_none() {
            let font = &fonts.fonts[glyph.font];
            let face = ttf_parser::Face::parse(&font.bytes, font.index).expect("parsed face");
            let mut widths = BTreeMap::new();
            for (unit, cid) in &font.glyphs {
                let units = if unit.components.len() == 1
                    && unit.components[0].1 == 0
                    && unit.components[0].2 == 0
                {
                    i32::from(
                        face.glyph_hor_advance(ttf_parser::GlyphId(unit.components[0].0))
                            .unwrap_or(0),
                    )
                } else {
                    unit.advance
                };
                widths.insert(*cid, units);
            }
            self.faces[glyph.font] = Some((face.units_per_em() as f32, widths));
        }
        let (upem, widths) = self.faces[glyph.font].as_ref().unwrap();
        let units = widths
            .get(&glyph.cid)
            .copied()
            .expect("shaped glyph has a mapping");
        units as f32 * size / upem
    }
}

fn structure_role(role: &str, ua1: bool) -> &str {
    if ua1 {
        match role {
            "Title" => "H1",
            "FENote" => "Note",
            _ => role,
        }
    } else {
        role
    }
}

// Resolve destinations from final draws, after pagination and column layout.
// Children such as links, formulae and bidi spans belong to their heading.
fn write_outlines(
    pdf: &mut Pdf,
    ids: &mut Ids,
    layout: &Layout,
    pages: &[Ref],
    tags: &[Ref],
    ua1: bool,
) -> Option<Ref> {
    let mut destinations = BTreeMap::new();
    for (page, content) in layout.pages.iter().enumerate() {
        for draw in &content.draws {
            let mut tag = draw.tag;
            while let Some(index) = tag {
                if layout.tags[index].outline.is_some() {
                    destinations.entry(index).or_insert((page, draw.y));
                    break;
                }
                tag = layout.tags[index].parent;
            }
        }
    }
    let entries: Vec<_> = layout
        .tags
        .iter()
        .enumerate()
        .filter_map(|(tag, value)| {
            let (depth, title) = value.outline.as_ref()?;
            let &(page, y) = destinations.get(&tag)?;
            Some((*depth, title, page, y, tag))
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let root = ids.next();
    let refs: Vec<_> = entries.iter().map(|_| ids.next()).collect();
    let mut children = vec![Vec::new(); entries.len() + 1];
    let mut parents = vec![entries.len(); entries.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (index, (depth, ..)) in entries.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|&parent| entries[parent].0 >= *depth)
        {
            stack.pop();
        }
        let parent = stack.last().copied().unwrap_or(entries.len());
        parents[index] = parent;
        children[parent].push(index);
        stack.push(index);
    }
    let mut descendants = vec![0i32; entries.len() + 1];
    for index in (0..entries.len()).rev() {
        descendants[parents[index]] += descendants[index] + 1;
    }
    pdf.indirect(root)
        .dict()
        .pair(Name(b"Type"), Name(b"Outlines"))
        .pair(Name(b"First"), refs[children[entries.len()][0]])
        .pair(
            Name(b"Last"),
            refs[*children[entries.len()].last().unwrap()],
        )
        .pair(Name(b"Count"), descendants[entries.len()]);
    for siblings in &children {
        for (position, &index) in siblings.iter().enumerate() {
            let (_, title, page, y, tag) = entries[index];
            let mut item = pdf.indirect(refs[index]).dict();
            item.pair(Name(b"Title"), TextStr(title)).pair(
                Name(b"Parent"),
                if parents[index] == entries.len() {
                    root
                } else {
                    refs[parents[index]]
                },
            );
            if position > 0 {
                item.pair(Name(b"Prev"), refs[siblings[position - 1]]);
            }
            if position + 1 < siblings.len() {
                item.pair(Name(b"Next"), refs[siblings[position + 1]]);
            }
            if let (Some(first), Some(last)) = (children[index].first(), children[index].last()) {
                item.pair(Name(b"First"), refs[*first])
                    .pair(Name(b"Last"), refs[*last])
                    .pair(Name(b"Count"), descendants[index]);
            }
            item.pair(Name(b"SE"), tags[tag]);
            if ua1 {
                item.insert(Name(b"Dest"))
                    .array()
                    .item(pages[page])
                    .item(Name(b"XYZ"))
                    .item(pdf_writer::Null)
                    .item(y)
                    .item(pdf_writer::Null);
            } else {
                let mut action = item.insert(Name(b"A")).dict();
                action.pair(Name(b"S"), Name(b"GoTo"));
                // PDF 2.0 readers use SD; D preserves navigation in older viewers.
                action
                    .insert(Name(b"D"))
                    .array()
                    .item(pages[page])
                    .item(Name(b"XYZ"))
                    .item(pdf_writer::Null)
                    .item(y)
                    .item(pdf_writer::Null);
                action
                    .insert(Name(b"SD"))
                    .array()
                    .item(tags[tag])
                    .item(Name(b"XYZ"))
                    .item(pdf_writer::Null)
                    .item(y)
                    .item(pdf_writer::Null);
            }
        }
    }
    Some(root)
}

pub fn write(layout: &Layout, standard: Option<&str>) -> Result<Vec<u8>> {
    let mut pdf = Pdf::new();
    let ua1 = standard == Some("ua-1");
    pdf.set_version(if ua1 { 1 } else { 2 }, if ua1 { 7 } else { 0 });
    let mut ids = Ids(1);
    let catalog = ids.next();
    let page_tree = ids.next();
    let structure = ids.next();
    let document = ids.next();
    let parent_tree = ids.next();
    let namespace = ids.next();
    let namespace_legacy = ids.next();
    let metadata = ids.next();
    let info = ids.next();
    let page_ids: Vec<Ref> = layout.pages.iter().map(|_| ids.next()).collect();
    let tag_ids: Vec<Ref> = layout.tags.iter().map(|_| ids.next()).collect();
    let mut fonts: BTreeMap<usize, Ref> = BTreeMap::new();
    let mut cff_cids = BTreeMap::new();
    for (index, font) in layout.fonts.fonts.iter().enumerate() {
        if font.glyphs.is_empty() {
            continue;
        }
        let font_id = ids.next();
        let cid_id = ids.next();
        let descriptor = ids.next();
        let data_id = ids.next();
        let cmap_id = ids.next();
        let gid_id = ids.next();
        fonts.insert(index, font_id);
        let face = ttf_parser::Face::parse(&font.bytes, font.index)
            .map_err(|e| Diagnostic::new("invalid-font", e.to_string()))?;
        let factor = 1000. / face.units_per_em() as f32;
        let mut mapper = subsetter::GlyphRemapper::new();
        for unit in font.glyphs.keys() {
            for (gid, _, _) in &unit.components {
                mapper.remap(*gid);
            }
        }
        let subset = subsetter::subset_with_variations(&font.bytes, font.index, &[], &mapper)
            .map_err(|e| Diagnostic::new("font-subset", e.to_string()))?;
        let (subset, cluster_gids) = crate::composite::build(subset, &font.glyphs, &mapper)?;
        let subset_face = ttf_parser::Face::parse(&subset, 0)
            .map_err(|e| Diagnostic::new("font-subset", e.to_string()))?;
        let cff = subset_face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF "));
        let name = format!(
            "PUBL{}{}+Publisle{}",
            (b'A' + (index / 26) as u8) as char,
            (b'A' + (index % 26) as u8) as char,
            index
        );
        pdf.type0_font(font_id)
            .base_font(Name(name.as_bytes()))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid_id)
            .to_unicode(cmap_id);
        let mut cid = pdf.cid_font(cid_id);
        cid.subtype(if cff.is_some() {
            CidFontType::Type0
        } else {
            CidFontType::Type2
        })
        .base_font(Name(name.as_bytes()))
        .system_info(SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        })
        .font_descriptor(descriptor);
        if cff.is_none() {
            cid.cid_to_gid_map_stream(gid_id);
        }
        let mut width = cid.widths();
        let mut cmap = UnicodeCmap::<u16>::new(
            Name(b"PublisleUnicode"),
            SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"UCS"),
                supplement: 0,
            },
        );
        let mut gids = vec![0u8; (font.glyphs.len() + 1) * 2];
        let mut mapped = std::collections::BTreeSet::new();
        for (unit, cid) in &font.glyphs {
            let remapped = cluster_gids[cid];
            let actual_cid = if cff.is_some() {
                cff_cids.insert((index, *cid), remapped);
                remapped
            } else {
                *cid
            };
            if !mapped.insert(actual_cid) {
                continue;
            }
            width.consecutive(
                actual_cid,
                [subset_face
                    .glyph_hor_advance(ttf_parser::GlyphId(remapped))
                    .unwrap_or(0) as f32
                    * factor],
            );
            gids[*cid as usize * 2..*cid as usize * 2 + 2].copy_from_slice(&remapped.to_be_bytes());
            cmap.pair_with_multiple(actual_cid, unit.unicode.chars());
        }
        width.finish();
        cid.finish();
        let bbox = subset_face.global_bounding_box();
        let mut fd = pdf.font_descriptor(descriptor);
        fd.name(Name(name.as_bytes()))
            .flags(FontFlags::SYMBOLIC)
            .bbox(Rect::new(
                bbox.x_min as f32 * factor,
                bbox.y_min as f32 * factor,
                bbox.x_max as f32 * factor,
                bbox.y_max as f32 * factor,
            ))
            .italic_angle(face.italic_angle())
            .ascent(face.ascender() as f32 * factor)
            .descent(face.descender() as f32 * factor)
            .cap_height(face.capital_height().unwrap_or(face.ascender()) as f32 * factor)
            .stem_v(80.);
        if cff.is_some() {
            fd.font_file3(data_id);
        } else {
            fd.font_file2(data_id);
        }
        fd.finish();
        let bytes = compressed(cff.unwrap_or(&subset))?;
        let mut stream = pdf.stream(data_id, &bytes);
        stream.filter(Filter::FlateDecode);
        if cff.is_some() {
            stream.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
        } else {
            stream.pair(Name(b"Length1"), subset.len() as i32);
        }
        stream.finish();
        let bytes = compressed(&gids)?;
        pdf.stream(gid_id, &bytes).filter(Filter::FlateDecode);
        let bytes = cmap.finish();
        pdf.stream(cmap_id, &bytes);
    }
    {
        let mut info = pdf.document_info(info);
        info.title(TextStr(&layout.title))
            .author(TextStr(&layout.authors))
            .creator(TextStr("Publisle"))
            .producer(TextStr(concat!("publisle-pdf ", env!("CARGO_PKG_VERSION"))));
        if !layout.keywords.is_empty() {
            info.keywords(TextStr(&layout.keywords.join(", ")));
        }
    }
    // This records the requested standard; independent validation is still required.
    let standard_xmp = match standard {
        Some("ua-2") => "<pdfuaid:part>2</pdfuaid:part><pdfuaid:rev>2024</pdfuaid:rev>",
        Some("ua-1") => "<pdfuaid:part>1</pdfuaid:part>",
        None => "",
        Some(_) => {
            return Err(Diagnostic::new(
                "unsupported-standard",
                "Unknown requested PDF standard.",
            ));
        }
    };
    let keywords_xmp = if layout.keywords.is_empty() {
        String::new()
    } else {
        let bag = layout
            .keywords
            .iter()
            .map(|keyword| format!("<rdf:li>{}</rdf:li>", math::xml(keyword)))
            .collect::<Vec<_>>()
            .join("");
        format!(
            "<dc:subject><rdf:Bag>{bag}</rdf:Bag></dc:subject>\
             <pdf:Keywords>{}</pdf:Keywords>",
            math::xml(&layout.keywords.join(", "))
        )
    };
    let xmp = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\"><dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title><dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator><dc:language><rdf:Bag><rdf:li>{}</rdf:li></rdf:Bag></dc:language>{keywords_xmp}{standard_xmp}</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
        math::xml(&layout.title),
        math::xml(&layout.authors),
        math::xml(&layout.language)
    );
    pdf.metadata(metadata, xmp.as_bytes());
    pdf.indirect(namespace)
        .dict()
        .pair(Name(b"Type"), Name(b"Namespace"))
        .pair(Name(b"NS"), TextStr("http://iso.org/pdf2/ssn"));
    pdf.indirect(namespace_legacy)
        .dict()
        .pair(Name(b"Type"), Name(b"Namespace"))
        .pair(Name(b"NS"), TextStr("http://iso.org/pdf/ssn"));
    let mut contents: Vec<Vec<(usize, Ref, i32)>> = vec![vec![]; layout.tags.len()];
    let mut page_parents: Vec<Vec<Ref>> = vec![];
    let mut rank = 0usize;
    let mut ranks = vec![usize::MAX; layout.tags.len()];
    let mut links: Vec<(Ref, Ref, usize)> = vec![];
    let mut advances = AdvanceCache { faces: Vec::new() };
    for (page_index, page) in layout.pages.iter().enumerate() {
        let mut content = Content::new();
        let content_id = ids.next();
        let mut mcid = 0i32;
        let mut parents = vec![];
        let mut images: Vec<(String, Ref)> = vec![];
        let mut annotations = vec![];
        for draw in &page.draws {
            if let Some(tag) = draw.tag {
                content
                    .begin_marked_content_with_properties(Name(
                        structure_role(&layout.tags[tag].role, ua1).as_bytes(),
                    ))
                    .properties()
                    .identify(mcid);
                contents[tag].push((rank, page_ids[page_index], mcid));
                ranks[tag] = ranks[tag].min(rank);
                parents.push(tag_ids[tag]);
                mcid += 1;
            } else {
                content.begin_marked_content(Name(b"Artifact"));
            }
            match &draw.kind {
                DrawKind::Text {
                    shaped,
                    size,
                    text: value,
                    link,
                    target,
                } => {
                    text(
                        &mut content,
                        &layout.fonts,
                        &mut advances,
                        &TextPaint {
                            shaped,
                            size: *size,
                            position: [draw.x, draw.y],
                            color: [0.; 3],
                        },
                        &cff_cids,
                    );
                    if link.is_some() || target.is_some() {
                        let id = ids.next();
                        let mut annotation = pdf.annotation(id);
                        annotation
                            .pair(Name(b"Subtype"), Name(b"Link"))
                            .pair(
                                Name(b"Rect"),
                                Rect::new(
                                    draw.x,
                                    draw.y - size * 0.25,
                                    draw.x + shaped.width,
                                    draw.y + size,
                                ),
                            )
                            .pair(Name(b"Contents"), TextStr(value))
                            .pair(Name(b"P"), page_ids[page_index])
                            .pair(Name(b"F"), 4);
                        annotation.insert(Name(b"Border")).array().items([0, 0, 0]);
                        if let Some(url) = link {
                            let mut action = annotation.insert(Name(b"A")).dict();
                            action
                                .pair(Name(b"S"), Name(b"URI"))
                                .pair(Name(b"URI"), Str(url.as_bytes()));
                        } else if let Some(target) = target {
                            let target_index = layout
                                .tags
                                .iter()
                                .position(|tag| tag.label.as_deref() == Some(target.as_str()))
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        "unresolved-reference",
                                        format!("No PDF destination for {target}."),
                                    )
                                })?;
                            let location = layout
                                .pages
                                .iter()
                                .enumerate()
                                .find_map(|(i, page)| {
                                    page.draws.iter().find_map(|draw| {
                                        let mut parent = draw.tag;
                                        while let Some(index) = parent {
                                            if index == target_index {
                                                return Some((i, draw.x, draw.y));
                                            }
                                            parent = layout.tags[index].parent;
                                        }
                                        None
                                    })
                                })
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        "unresolved-reference",
                                        format!("Empty destination {target}."),
                                    )
                                })?;
                            let mut action = annotation.insert(Name(b"A")).dict();
                            action.pair(Name(b"S"), Name(b"GoTo"));
                            action
                                .insert(Name(b"D"))
                                .array()
                                .item(page_ids[location.0])
                                .item(Name(b"XYZ"))
                                .item(location.1)
                                .item(location.2)
                                .item(0);
                            action
                                .insert(Name(b"SD"))
                                .array()
                                .item(tag_ids[target_index])
                                .item(Name(b"Fit"));
                        }
                        if let Some(tag) = draw.tag {
                            let parent_key = layout.pages.len() + links.len();
                            annotation.pair(Name(b"StructParent"), parent_key as i32);
                            links.push((id, page_ids[page_index], tag));
                        }
                        annotation.finish();
                        annotations.push(id);
                    }
                }
                DrawKind::Math(b) => {
                    for background in &b.backgrounds {
                        content
                            .save_state()
                            .set_fill_rgb(
                                background.color[0],
                                background.color[1],
                                background.color[2],
                            )
                            .rect(
                                draw.x + background.x,
                                draw.y + background.y,
                                background.width,
                                background.height,
                            )
                            .fill_nonzero();
                        if let Some(border) = background.border {
                            content
                                .set_stroke_rgb(border[0], border[1], border[2])
                                .set_line_width(0.5)
                                .rect(
                                    draw.x + background.x,
                                    draw.y + background.y,
                                    background.width,
                                    background.height,
                                )
                                .stroke();
                        }
                        content.restore_state();
                    }
                    for run in &b.runs {
                        text(
                            &mut content,
                            &layout.fonts,
                            &mut advances,
                            &TextPaint {
                                shaped: &run.shaped,
                                size: run.size,
                                position: [draw.x + run.x, draw.y + run.y],
                                color: run.color,
                            },
                            &cff_cids,
                        );
                    }
                    content.set_stroke_rgb(0., 0., 0.);
                    for rule in &b.rules {
                        content
                            .set_line_width(rule.thickness)
                            .move_to(draw.x + rule.x, draw.y + rule.y)
                            .line_to(draw.x + rule.x + rule.width, draw.y + rule.y)
                            .stroke();
                    }
                }
                #[cfg(feature = "pdf-import")]
                DrawKind::Pdf {
                    asset,
                    draw_width,
                    draw_height,
                } => {
                    let id = ids.next();
                    let name = format!("Im{}", images.len());
                    asset.write(&mut pdf, id, &mut || ids.next())?;
                    let sx = draw_width / (asset.bbox[2] - asset.bbox[0]);
                    let sy = draw_height / (asset.bbox[3] - asset.bbox[1]);
                    content
                        .save_state()
                        .transform([
                            sx,
                            0.,
                            0.,
                            sy,
                            draw.x - sx * asset.bbox[0],
                            draw.y - sy * asset.bbox[1],
                        ])
                        .x_object(Name(name.as_bytes()))
                        .restore_state();
                    images.push((name, id));
                }
                DrawKind::Rule { width, thickness } => {
                    content
                        .set_line_width(*thickness)
                        .move_to(draw.x, draw.y)
                        .line_to(draw.x + width, draw.y)
                        .stroke();
                }
                DrawKind::Rect { width, height } => {
                    // Light frame: subtle fill plus a hairline border.
                    content
                        .save_state()
                        .set_fill_rgb(0.965, 0.965, 0.965)
                        .rect(draw.x, draw.y, *width, *height)
                        .fill_nonzero()
                        .set_stroke_rgb(0.78, 0.78, 0.78)
                        .set_line_width(0.5)
                        .rect(draw.x, draw.y, *width, *height)
                        .stroke()
                        .restore_state();
                }
                DrawKind::Image {
                    pixels,
                    alpha,
                    width,
                    height,
                    draw_width,
                    draw_height,
                } => {
                    let id = ids.next();
                    let name = format!("Im{}", images.len());
                    let mask = if let Some(alpha) = alpha {
                        let mask = ids.next();
                        let bytes = compressed(alpha)?;
                        let mut image = pdf.image_xobject(mask, &bytes);
                        image
                            .width(*width as i32)
                            .height(*height as i32)
                            .bits_per_component(8)
                            .filter(Filter::FlateDecode);
                        image.color_space().device_gray();
                        image.finish();
                        Some(mask)
                    } else {
                        None
                    };
                    let bytes = compressed(pixels)?;
                    let mut image = pdf.image_xobject(id, &bytes);
                    image
                        .width(*width as i32)
                        .height(*height as i32)
                        .bits_per_component(8)
                        .filter(Filter::FlateDecode);
                    image.color_space().device_rgb();
                    if let Some(mask) = mask {
                        image.s_mask(mask);
                    }
                    image.finish();
                    content
                        .save_state()
                        .transform([*draw_width, 0., 0., *draw_height, draw.x, draw.y])
                        .x_object(Name(name.as_bytes()))
                        .restore_state();
                    images.push((name, id));
                }
            }
            content.end_marked_content();
            rank += 1;
        }
        page_parents.push(parents);
        let bytes = compressed(&content.finish())?;
        pdf.stream(content_id, &bytes).filter(Filter::FlateDecode);
        let mut writer = pdf.page(page_ids[page_index]);
        writer
            .parent(page_tree)
            .media_box(Rect::new(0., 0., page.width, page.height))
            .contents(content_id)
            .struct_parents(page_index as i32)
            .annotations(annotations);
        writer.pair(Name(b"Tabs"), Name(b"S"));
        {
            let mut resources = writer.resources();
            {
                let mut dict = resources.fonts();
                for (index, id) in &fonts {
                    let name = font_name(*index);
                    dict.pair(Name(name.as_bytes()), *id);
                }
            }
            {
                let mut dict = resources.x_objects();
                for (name, id) in images {
                    dict.pair(Name(name.as_bytes()), id);
                }
            }
        }
        writer.finish();
    }
    // Propagate the first drawing rank upwards, then interleave child elements and own MCIDs.
    for i in (0..layout.tags.len()).rev() {
        if let Some(parent) = layout.tags[i].parent {
            ranks[parent] = ranks[parent].min(ranks[i]);
        }
    }
    let mut math_files = vec![];
    for (i, tag) in layout.tags.iter().enumerate() {
        let attachment = if let Some(mathml) = &tag.mathml {
            let file = ids.next();
            let data = ids.next();
            let filename = format!("mathml-{}.xml", math_files.len() + 1);
            let bytes = compressed(mathml.as_bytes())?;
            pdf.embedded_file(data, &bytes)
                .filter(Filter::FlateDecode)
                .pair(Name(b"Subtype"), Name(b"application/mathml+xml"));
            pdf.file_spec(file)
                .path(Str(filename.as_bytes()))
                .unic_file(TextStr(&filename))
                .description(TextStr("Mathematical expression in MathML"))
                .embedded_file_with_unicode(data)
                .pair(Name(b"AFRelationship"), Name(b"Supplement"));
            math_files.push((filename, file));
            Some(file)
        } else {
            None
        };
        let mut writer = pdf.indirect(tag_ids[i]).dict();
        writer
            .pair(Name(b"Type"), Name(b"StructElem"))
            .pair(Name(b"S"), Name(structure_role(&tag.role, ua1).as_bytes()))
            .pair(
                Name(b"P"),
                tag.parent.map(|p| tag_ids[p]).unwrap_or(document),
            );
        if !ua1 {
            writer.pair(
                Name(b"NS"),
                if ["BlockQuote", "Code", "Note", "BibEntry"].contains(&tag.role.as_str()) {
                    namespace_legacy
                } else {
                    namespace
                },
            );
        } else if tag.role == "FENote" {
            writer.pair(Name(b"ID"), TextStr(&format!("note-{i}")));
        }
        if let Some(alt) = &tag.alt {
            writer.pair(Name(b"Alt"), TextStr(alt));
        }
        if let Some(actual) = &tag.actual_text {
            writer.pair(Name(b"ActualText"), TextStr(actual));
        }
        if tag.header {
            writer
                .insert(Name(b"A"))
                .dict()
                .pair(Name(b"O"), Name(b"Table"))
                .pair(Name(b"Scope"), Name(b"Column"));
        }
        if let Some(file) = attachment {
            writer.insert(Name(b"AF")).array().item(file);
        }
        let mut events: Vec<StructureEvent> = tag
            .children
            .iter()
            .map(|child| (ranks[*child], Some(tag_ids[*child]), None))
            .collect();
        events.extend(
            contents[i]
                .iter()
                .map(|(rank, page, mcid)| (*rank, None, Some((*page, *mcid)))),
        );
        events.sort_by_key(|event| event.0);
        {
            let mut children = writer.insert(Name(b"K")).array();
            for (_, child, marked) in events {
                if let Some(child) = child {
                    children.item(child);
                }
                if let Some((page, mcid)) = marked {
                    children
                        .push()
                        .dict()
                        .pair(Name(b"Type"), Name(b"MCR"))
                        .pair(Name(b"Pg"), page)
                        .pair(Name(b"MCID"), mcid);
                }
            }
            for (annotation, page, tag) in &links {
                if *tag == i {
                    children
                        .push()
                        .dict()
                        .pair(Name(b"Type"), Name(b"OBJR"))
                        .pair(Name(b"Obj"), *annotation)
                        .pair(Name(b"Pg"), *page);
                }
            }
        }
    }
    {
        let mut tree = pdf.indirect(parent_tree).dict();
        let mut nums = tree.insert(Name(b"Nums")).array();
        for (i, parents) in page_parents.iter().enumerate() {
            nums.item(i as i32);
            nums.push().array().items(parents.iter().copied());
        }
        for (i, (_, _, tag)) in links.iter().enumerate() {
            nums.item((layout.pages.len() + i) as i32);
            nums.item(tag_ids[*tag]);
        }
    }
    {
        let mut root = pdf.indirect(structure).dict();
        root.pair(Name(b"Type"), Name(b"StructTreeRoot"))
            .pair(Name(b"K"), document)
            .pair(Name(b"ParentTree"), parent_tree)
            .pair(
                Name(b"ParentTreeNextKey"),
                (layout.pages.len() + links.len()) as i32,
            );
        if !ua1 {
            root.insert(Name(b"Namespaces"))
                .array()
                .items([namespace, namespace_legacy]);
        }
    }
    {
        let mut doc = pdf.indirect(document).dict();
        doc.pair(Name(b"Type"), Name(b"StructElem"))
            .pair(Name(b"S"), Name(b"Document"))
            .pair(Name(b"P"), structure);
        if !ua1 {
            doc.pair(Name(b"NS"), namespace);
        }
        doc.insert(Name(b"K")).array().items(
            layout
                .tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| tag.parent.is_none())
                .map(|(i, _)| tag_ids[i]),
        );
    }
    pdf.pages(page_tree)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);
    let outlines = write_outlines(&mut pdf, &mut ids, layout, &page_ids, &tag_ids, ua1);
    {
        let mut root = pdf.catalog(catalog);
        root.pages(page_tree)
            .lang(TextStr(&layout.language))
            .metadata(metadata);
        root.pair(Name(b"StructTreeRoot"), structure);
        if let Some(outlines) = outlines {
            root.pair(Name(b"Outlines"), outlines)
                .pair(Name(b"PageMode"), Name(b"UseOutlines"));
        }
        root.mark_info().marked(true);
        root.viewer_preferences().display_doc_title(true);
        if !math_files.is_empty() {
            math_files.sort_by(|a, b| a.0.cmp(&b.0));
            let mut names = root.names();
            let mut files = names.insert(Name(b"EmbeddedFiles")).dict();
            let mut pairs = files.insert(Name(b"Names")).array();
            for (name, file) in &math_files {
                pairs.item(TextStr(name));
                pairs.item(*file);
            }
        }
    }
    // Measured before TJ runs, object streams, and the xref stream
    // (article template, DejaVu Serif, 2026-11-03): a 9-page plain document
    // was 430377 bytes and a one-page sample with a heading, inline math, and
    // one display equation was 27526 bytes.
    let bytes = pack_objects(&pdf.finish())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Diagnostic::new(
            "pdf-limit",
            "The generated PDF exceeds 16 MiB.",
        ));
    }
    Ok(bytes)
}

fn pdf_err(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("pdf-write", message)
}

/// Pack every non-stream object, including the catalog and structure tree,
/// into one compressed object stream and replace the classic xref with a
/// compressed xref stream. Stream objects stay as ordinary objects.
fn pack_objects(pdf: &[u8]) -> Result<Vec<u8>> {
    let xref_at = pdf
        .windows(6)
        .rposition(|window| window == b"\nxref\n")
        .ok_or_else(|| pdf_err("missing xref"))?
        + 1;
    let xref_text =
        std::str::from_utf8(&pdf[xref_at..]).map_err(|error| pdf_err(error.to_string()))?;
    let mut lines = xref_text.lines();
    if lines.next() != Some("xref") {
        return Err(pdf_err("xref header"));
    }
    let mut offsets: Vec<Option<usize>> = Vec::new();
    loop {
        let Some(line) = lines.next() else {
            break;
        };
        if line.starts_with("trailer") {
            break;
        }
        let mut parts = line.split_whitespace();
        let start: usize = parts
            .next()
            .ok_or_else(|| pdf_err("xref subsection"))?
            .parse()
            .map_err(|error: std::num::ParseIntError| pdf_err(error.to_string()))?;
        let count: usize = parts
            .next()
            .ok_or_else(|| pdf_err("xref subsection"))?
            .parse()
            .map_err(|error: std::num::ParseIntError| pdf_err(error.to_string()))?;
        if offsets.len() < start + count {
            offsets.resize(start + count, None);
        }
        for index in 0..count {
            let entry = lines.next().ok_or_else(|| pdf_err("truncated xref"))?;
            let mut fields = entry.split_whitespace();
            let offset: u64 = fields
                .next()
                .ok_or_else(|| pdf_err("xref entry"))?
                .parse()
                .map_err(|error: std::num::ParseIntError| pdf_err(error.to_string()))?;
            let _generation = fields.next().ok_or_else(|| pdf_err("xref entry"))?;
            let flag = fields.next().ok_or_else(|| pdf_err("xref entry"))?;
            if flag == "n" {
                offsets[start + index] =
                    Some(usize::try_from(offset).map_err(|error| pdf_err(error.to_string()))?);
            }
        }
    }
    let trailer_at = pdf[xref_at..]
        .windows(7)
        .position(|window| window == b"trailer")
        .ok_or_else(|| pdf_err("missing trailer"))?
        + xref_at;
    let dict_at = pdf[trailer_at..]
        .windows(2)
        .position(|window| window == b"<<")
        .ok_or_else(|| pdf_err("trailer dictionary"))?
        + trailer_at;
    let trailer_end = scan_dict(pdf, dict_at)?;
    let trailer = &pdf[dict_at..trailer_end];
    let root = trailer_reference(trailer, b"/Root")?;
    let info = trailer_reference(trailer, b"/Info").ok();

    let header_end = offsets.iter().flatten().copied().min().unwrap_or(pdf.len());
    let mut out = pdf[..header_end].to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }

    let objstm_id = offsets.len();
    let xref_id = offsets.len() + 1;
    let new_size = offsets.len() + 2;
    #[derive(Clone, Copy)]
    enum XRef {
        Free,
        InUse(usize),
        Compressed(usize),
    }
    let mut xref = vec![XRef::Free; new_size];
    let mut packed: Vec<(usize, Vec<u8>)> = Vec::new();
    for (id, offset) in offsets.iter().enumerate() {
        let Some(offset) = offset else {
            continue;
        };
        let (bytes, stream) = slice_object(pdf, *offset)?;
        if stream {
            xref[id] = XRef::InUse(out.len());
            out.extend_from_slice(&bytes);
            if !out.ends_with(b"\n") {
                out.push(b'\n');
            }
        } else {
            packed.push((id, object_value(&bytes)?));
        }
    }

    let mut index = String::new();
    let mut body = Vec::new();
    for (position, (id, value)) in packed.iter().enumerate() {
        if position > 0 {
            index.push(' ');
        }
        index.push_str(&format!("{id} {}", body.len()));
        body.extend_from_slice(value);
        body.push(b'\n');
        xref[*id] = XRef::Compressed(position);
    }
    index.push('\n');
    let first = index.len();
    let mut raw = index.into_bytes();
    raw.extend_from_slice(&body);
    let compressed_objects = compressed(&raw)?;
    let objstm_at = out.len();
    let objstm_header = format!(
        "{objstm_id} 0 obj\n<<\n  /Type /ObjStm\n  /N {}\n  /First {first}\n  /Filter /FlateDecode\n  /Length {}\n>>\nstream\n",
        packed.len(),
        compressed_objects.len()
    );
    out.extend_from_slice(objstm_header.as_bytes());
    out.extend_from_slice(&compressed_objects);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    xref[objstm_id] = XRef::InUse(objstm_at);

    let xref_at = out.len();
    if xref_at > u32::MAX as usize {
        return Err(pdf_err("xref offset exceeds 32 bits"));
    }
    xref[xref_id] = XRef::InUse(xref_at);
    let mut xref_data = Vec::with_capacity(new_size * 7);
    for entry in &xref {
        match entry {
            XRef::Free => {
                xref_data.push(0);
                xref_data.extend_from_slice(&0u32.to_be_bytes());
                xref_data.extend_from_slice(&65535u16.to_be_bytes());
            }
            XRef::InUse(offset) => {
                xref_data.push(1);
                xref_data.extend_from_slice(&(*offset as u32).to_be_bytes());
                xref_data.extend_from_slice(&0u16.to_be_bytes());
            }
            XRef::Compressed(index) => {
                xref_data.push(2);
                xref_data.extend_from_slice(&(objstm_id as u32).to_be_bytes());
                xref_data.extend_from_slice(&(*index as u16).to_be_bytes());
            }
        }
    }
    let compressed_xref = compressed(&xref_data)?;
    let mut info_line = String::new();
    if let Some(info) = info {
        info_line = format!("  /Info {info}\n");
    }
    let xref_header = format!(
        "{xref_id} 0 obj\n<<\n  /Type /XRef\n  /Size {new_size}\n  /W [1 4 2]\n  /Index [0 {new_size}]\n  /Root {root}\n{info_line}  /Filter /FlateDecode\n  /Length {}\n>>\nstream\n",
        compressed_xref.len()
    );
    out.extend_from_slice(xref_header.as_bytes());
    out.extend_from_slice(&compressed_xref);
    out.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    out.extend_from_slice(xref_at.to_string().as_bytes());
    out.extend_from_slice(b"\n%%EOF\n");
    Ok(out)
}

fn slice_object(pdf: &[u8], start: usize) -> Result<(Vec<u8>, bool)> {
    let obj_at = pdf[start..]
        .windows(3)
        .position(|window| window == b"obj")
        .ok_or_else(|| pdf_err("object header"))?
        + start;
    let mut body_at = obj_at + 3;
    while pdf.get(body_at).is_some_and(u8::is_ascii_whitespace) {
        body_at += 1;
    }
    if !pdf[body_at..].starts_with(b"<<") {
        // Object streams in this writer pack dictionaries only. An array, such
        // as a structure-tree /K, stays a normal object instead of failing.
        let endobj = pdf[body_at..]
            .windows(6)
            .position(|window| window == b"endobj")
            .ok_or_else(|| pdf_err("missing endobj"))?
            + body_at;
        let mut end = endobj + b"endobj".len();
        if pdf.get(end) == Some(&b'\n') {
            end += 1;
        }
        return Ok((pdf[start..end].to_vec(), true));
    }
    let dict_end = scan_dict(pdf, body_at)?;
    if let Some(data_at) = stream_data_at(pdf, dict_end) {
        let length = direct_length(&pdf[body_at..dict_end])?;
        let mut end = data_at + length;
        if pdf.get(end) == Some(&b'\r') {
            end += 1;
        }
        if pdf.get(end) == Some(&b'\n') {
            end += 1;
        }
        if !pdf[end..].starts_with(b"endstream") {
            return Err(pdf_err("stream Length does not reach endstream"));
        }
        end += b"endstream".len();
        while pdf.get(end).is_some_and(u8::is_ascii_whitespace) {
            end += 1;
        }
        if !pdf[end..].starts_with(b"endobj") {
            return Err(pdf_err("stream is missing endobj"));
        }
        end += b"endobj".len();
        if pdf.get(end) == Some(&b'\n') {
            end += 1;
        }
        return Ok((pdf[start..end].to_vec(), true));
    }
    let endobj = pdf[dict_end..]
        .windows(6)
        .position(|window| window == b"endobj")
        .ok_or_else(|| pdf_err("missing endobj"))?
        + dict_end;
    let mut end = endobj + b"endobj".len();
    if pdf.get(end) == Some(&b'\n') {
        end += 1;
    }
    Ok((pdf[start..end].to_vec(), false))
}

fn object_value(object: &[u8]) -> Result<Vec<u8>> {
    let obj_at = object
        .windows(3)
        .position(|window| window == b"obj")
        .ok_or_else(|| pdf_err("object header"))?;
    let mut start = obj_at + 3;
    while object.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    let end = object
        .windows(6)
        .rposition(|window| window == b"endobj")
        .ok_or_else(|| pdf_err("missing endobj"))?;
    let mut body = object[start..end].to_vec();
    while body.last().is_some_and(u8::is_ascii_whitespace) {
        body.pop();
    }
    if !body.starts_with(b"<<") {
        return Err(pdf_err("object stream values must be dictionaries"));
    }
    Ok(body)
}

fn direct_length(dict: &[u8]) -> Result<usize> {
    let mut index = 0;
    while index + b"/Length".len() < dict.len() {
        if dict[index..].starts_with(b"/Length") {
            let next = dict[index + b"/Length".len()];
            if next.is_ascii_alphanumeric() {
                index += 1;
                continue;
            }
            let mut cursor = index + b"/Length".len();
            while dict.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            let digits = cursor;
            while dict.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            if cursor == digits {
                return Err(pdf_err("Length is not a direct integer"));
            }
            let text = std::str::from_utf8(&dict[digits..cursor])
                .map_err(|error| pdf_err(error.to_string()))?;
            return text
                .parse()
                .map_err(|error: std::num::ParseIntError| pdf_err(error.to_string()));
        }
        index += 1;
    }
    Err(pdf_err("stream is missing Length"))
}

fn stream_data_at(data: &[u8], dict_end: usize) -> Option<usize> {
    let mut index = dict_end;
    while matches!(data.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    if data[index..].starts_with(b"\r\n") {
        index += 2;
    } else if data.get(index) == Some(&b'\n') {
        index += 1;
    }
    if !data[index..].starts_with(b"stream") {
        return None;
    }
    index += b"stream".len();
    if data[index..].starts_with(b"\r\n") {
        index += 2;
    } else if data.get(index) == Some(&b'\n') {
        index += 1;
    } else {
        return None;
    }
    Some(index)
}

fn trailer_reference(trailer: &[u8], key: &[u8]) -> Result<String> {
    let position = trailer
        .windows(key.len())
        .position(|window| window == key)
        .ok_or_else(|| pdf_err(format!("trailer missing {}", String::from_utf8_lossy(key))))?;
    let mut index = position + key.len();
    while trailer.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let start = index;
    while trailer.get(index).is_some_and(|byte| *byte != b'R') {
        index += 1;
    }
    if trailer.get(index) != Some(&b'R') {
        return Err(pdf_err("trailer reference"));
    }
    let text =
        std::str::from_utf8(&trailer[start..=index]).map_err(|error| pdf_err(error.to_string()))?;
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn scan_dict(data: &[u8], start: usize) -> Result<usize> {
    if !data[start..].starts_with(b"<<") {
        return Err(pdf_err("expected a dictionary"));
    }
    let mut index = start + 2;
    let mut depth = 1;
    while index < data.len() && depth > 0 {
        if data[index] == b'(' {
            index = skip_literal(data, index)?;
        } else if data[index] == b'<' && data.get(index + 1) == Some(&b'<') {
            depth += 1;
            index += 2;
        } else if data[index] == b'>' && data.get(index + 1) == Some(&b'>') {
            depth -= 1;
            index += 2;
        } else if data[index] == b'<' {
            index = skip_hex(data, index)?;
        } else if data[index] == b'[' {
            index = skip_array(data, index)?;
        } else {
            index += 1;
        }
    }
    if depth != 0 {
        return Err(pdf_err("unclosed dictionary"));
    }
    Ok(index)
}

fn skip_array(data: &[u8], start: usize) -> Result<usize> {
    let mut index = start + 1;
    let mut depth = 1;
    while index < data.len() && depth > 0 {
        match data[index] {
            b'(' => index = skip_literal(data, index)?,
            b'[' => {
                depth += 1;
                index += 1;
            }
            b']' => {
                depth -= 1;
                index += 1;
            }
            b'<' if data.get(index + 1) == Some(&b'<') => index = scan_dict(data, index)?,
            b'<' => index = skip_hex(data, index)?,
            _ => index += 1,
        }
    }
    if depth != 0 {
        return Err(pdf_err("unclosed array"));
    }
    Ok(index)
}

fn skip_literal(data: &[u8], start: usize) -> Result<usize> {
    let mut index = start + 1;
    while index < data.len() {
        match data[index] {
            b'\\' => index += 2,
            b')' => return Ok(index + 1),
            _ => index += 1,
        }
    }
    Err(pdf_err("unclosed literal string"))
}

fn skip_hex(data: &[u8], start: usize) -> Result<usize> {
    let end = data[start + 1..]
        .iter()
        .position(|byte| *byte == b'>')
        .ok_or_else(|| pdf_err("unclosed hex string"))?;
    Ok(start + 1 + end + 1)
}
