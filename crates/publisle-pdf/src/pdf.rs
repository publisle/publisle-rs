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
fn text(
    content: &mut Content,
    shaped: &Shaped,
    size: f32,
    position: [f32; 2],
    color: [f32; 3],
    cff_cids: &BTreeMap<(usize, u16), u16>,
) {
    content.set_fill_rgb(color[0], color[1], color[2]);
    content.begin_text();
    let mut font = usize::MAX;
    for glyph in &shaped.glyphs {
        if glyph.font != font {
            font = glyph.font;
            let name = font_name(font);
            content.set_font(Name(name.as_bytes()), size);
        }
        content.set_text_matrix([1., 0., 0., 1., position[0] + glyph.x, position[1] + glyph.y]);
        let cid = cff_cids
            .get(&(glyph.font, glyph.cid))
            .copied()
            .unwrap_or(glyph.cid);
        content.show(Str(&cid.to_be_bytes()));
        let _ = glyph.advance;
    }
    content.end_text();
}

pub fn write(layout: &Layout, standard: Option<&str>) -> Result<Vec<u8>> {
    let mut pdf = Pdf::new();
    pdf.set_version(2, 0);
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
    pdf.document_info(info)
        .title(TextStr(&layout.title))
        .author(TextStr(&layout.authors))
        .creator(TextStr("Publisle"))
        .producer(TextStr(concat!("publisle-pdf ", env!("CARGO_PKG_VERSION"))));
    // This records the requested standard; independent validation is still required.
    let standard_xmp = match standard {
        Some("ua-2") => "<pdfuaid:part>2</pdfuaid:part><pdfuaid:rev>2024</pdfuaid:rev>",
        Some("ua-1") => {
            return Err(Diagnostic::new(
                "unsupported-standard",
                "The native writer uses PDF 2.0; PDF/UA-1 is not supported.",
            ));
        }
        None => "",
        Some(_) => {
            return Err(Diagnostic::new(
                "unsupported-standard",
                "Unknown requested PDF standard.",
            ));
        }
    };
    let xmp = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\"><dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title><dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator><dc:language><rdf:Bag><rdf:li>{}</rdf:li></rdf:Bag></dc:language>{standard_xmp}</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
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
                    .begin_marked_content_with_properties(Name(layout.tags[tag].role.as_bytes()))
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
                        shaped,
                        *size,
                        [draw.x, draw.y],
                        [0.; 3],
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
                            &run.shaped,
                            run.size,
                            [draw.x + run.x, draw.y + run.y],
                            run.color,
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
            .pair(Name(b"S"), Name(tag.role.as_bytes()))
            .pair(
                Name(b"NS"),
                if ["BlockQuote", "Code", "Note", "BibEntry"].contains(&tag.role.as_str()) {
                    namespace_legacy
                } else {
                    namespace
                },
            )
            .pair(
                Name(b"P"),
                tag.parent.map(|p| tag_ids[p]).unwrap_or(document),
            );
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
        root.insert(Name(b"Namespaces"))
            .array()
            .items([namespace, namespace_legacy]);
    }
    {
        let mut doc = pdf.indirect(document).dict();
        doc.pair(Name(b"Type"), Name(b"StructElem"))
            .pair(Name(b"S"), Name(b"Document"))
            .pair(Name(b"NS"), namespace)
            .pair(Name(b"P"), structure);
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
    {
        let mut root = pdf.catalog(catalog);
        root.pages(page_tree)
            .lang(TextStr(&layout.language))
            .metadata(metadata);
        root.pair(Name(b"StructTreeRoot"), structure);
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
    let bytes = pdf.finish();
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Diagnostic::new(
            "pdf-limit",
            "The generated PDF exceeds 16 MiB.",
        ));
    }
    Ok(bytes)
}
