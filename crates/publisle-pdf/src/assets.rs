use crate::{
    fonts::Fonts,
    model::{Diagnostic, Result},
};
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_writer::{Filter, Name, Obj, Pdf, Rect, Ref, Str};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new("invalid-image", error.to_string())
}
pub fn raster(path: &Path, fonts: &Fonts) -> Result<image::DynamicImage> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("svg"))
    {
        let bytes = std::fs::read(path).map_err(invalid)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid("SVG exceeds 4 MiB."));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(invalid)?
            .to_ascii_lowercase();
        if [
            "<!doctype",
            "<!entity",
            "<script",
            "<foreignobject",
            "<image",
            "@import",
        ]
        .iter()
        .any(|token| text.contains(token))
        {
            return Err(invalid(
                "SVG contains unsupported active or external content.",
            ));
        }
        let mut options = resvg::usvg::Options::default();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        options.font_family = "Noto Serif".into();
        for font in &fonts.fonts {
            options.fontdb_mut().load_font_data(font.bytes.clone());
        }
        let tree = resvg::usvg::Tree::from_data(&bytes, &options).map_err(invalid)?;
        let size = tree.size().to_int_size();
        if size.width() > 8000
            || size.height() > 8000
            || u64::from(size.width()) * u64::from(size.height()) > 8_000_000
        {
            return Err(invalid("SVG exceeds the pixel limit."));
        }
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or_else(|| invalid("Cannot allocate SVG image."))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let mut pixels = Vec::with_capacity(pixmap.data().len());
        for pixel in pixmap.pixels() {
            let p = pixel.demultiply();
            pixels.extend_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
        }
        let image = image::RgbaImage::from_raw(size.width(), size.height(), pixels)
            .ok_or_else(|| invalid("Invalid SVG raster."))?;
        return Ok(image::DynamicImage::ImageRgba8(image));
    }
    let mut reader = image::ImageReader::open(path).map_err(invalid)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8000);
    limits.max_image_height = Some(8000);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .with_guessed_format()
        .map_err(invalid)?
        .decode()
        .map_err(invalid)?;
    if image.width() as u64 * image.height() as u64 > 8_000_000 {
        return Err(invalid("Image exceeds eight million pixels."));
    }
    Ok(image)
}

/// A local PDF figure is imported as a Form XObject, preserving vector artwork.
/// Page annotations, actions, attachments, and source structure tags are excluded.
pub struct PdfAsset {
    doc: Document,
    resources: Dictionary,
    content: Vec<u8>,
    pub bbox: [f32; 4],
}
impl PdfAsset {
    pub fn load(path: &Path) -> Result<Self> {
        let doc = Document::load(path).map_err(invalid)?;
        if doc.is_encrypted() || doc.objects.len() > 10000 {
            return Err(invalid("PDF figures must be unencrypted and bounded."));
        }
        let page = *doc
            .get_pages()
            .values()
            .next()
            .ok_or_else(|| invalid("PDF figure has no page."))?;
        let inherited = |key: &[u8]| -> Result<Object> {
            let mut id = page;
            for _ in 0..64 {
                let dict = doc
                    .get_object(id)
                    .and_then(Object::as_dict)
                    .map_err(invalid)?;
                if let Ok(value) = dict.get(key) {
                    return Ok(value.clone());
                }
                id = dict
                    .get(b"Parent")
                    .and_then(Object::as_reference)
                    .map_err(invalid)?;
            }
            Err(invalid("PDF page inheritance exceeds 64 levels."))
        };
        if let Ok(Object::Integer(rotation)) = inherited(b"Rotate")
            && rotation % 360 != 0
        {
            return Err(invalid(
                "Rotate the PDF figure page to zero degrees before importing.",
            ));
        }
        let media = inherited(b"MediaBox")?;
        let media = match &media {
            Object::Reference(id) => doc.get_object(*id).map_err(invalid)?,
            other => other,
        };
        let values = media.as_array().map_err(invalid)?;
        if values.len() != 4 {
            return Err(invalid("Invalid PDF figure MediaBox."));
        }
        let mut bbox = [0f32; 4];
        for (out, value) in bbox.iter_mut().zip(values) {
            *out = value.as_float().map_err(invalid)?;
        }
        if bbox.iter().any(|n| !n.is_finite()) || bbox[2] <= bbox[0] || bbox[3] <= bbox[1] {
            return Err(invalid("Invalid PDF figure dimensions."));
        }
        let (direct, refs) = doc.get_page_resources(page).map_err(invalid)?;
        let mut resources = Dictionary::new();
        for id in refs.iter().rev() {
            for (key, value) in doc
                .get_object(*id)
                .and_then(Object::as_dict)
                .map_err(invalid)?
                .iter()
            {
                resources.set(key.clone(), value.clone());
            }
        }
        if let Some(direct) = direct {
            for (key, value) in direct.iter() {
                resources.set(key.clone(), value.clone());
            }
        }
        let bytes = doc.get_page_content(page).map_err(invalid)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(invalid("Decoded PDF figure exceeds 16 MiB."));
        }
        let mut operations = lopdf::content::Content::decode(&bytes).map_err(invalid)?;
        operations.operations.retain(|operation| {
            !["BMC", "BDC", "EMC", "MP", "DP"].contains(&operation.operator.as_str())
        });
        let content = operations.encode().map_err(invalid)?;
        Ok(Self {
            doc,
            resources,
            content,
            bbox,
        })
    }
    pub fn write(&self, pdf: &mut Pdf, id: Ref, next: &mut impl FnMut() -> Ref) -> Result<()> {
        let mut reachable = BTreeSet::new();
        collect(
            &Object::Dictionary(self.resources.clone()),
            &self.doc,
            &mut reachable,
            0,
        )?;
        let mapping: BTreeMap<ObjectId, Ref> =
            reachable.into_iter().map(|old| (old, next())).collect();
        for (old, new) in &mapping {
            let object = self.doc.get_object(*old).map_err(invalid)?;
            if let Object::Stream(stream) = object {
                if stream.dict.get(b"F").is_ok() {
                    return Err(invalid("External PDF streams are unsupported."));
                }
                let mut writer = pdf.stream(*new, &stream.content);
                for (key, value) in stream.dict.iter() {
                    if key != b"Length" {
                        write_object(writer.insert(Name(key)), value, &mapping)?;
                    }
                }
            } else {
                write_object(pdf.indirect(*new), object, &mapping)?;
            }
        }
        let bytes = crate::pdf::compressed(&self.content)?;
        let mut form = pdf.stream(id, &bytes);
        form.filter(Filter::FlateDecode)
            .pair(Name(b"Type"), Name(b"XObject"))
            .pair(Name(b"Subtype"), Name(b"Form"))
            .pair(Name(b"FormType"), 1)
            .pair(
                Name(b"BBox"),
                Rect::new(self.bbox[0], self.bbox[1], self.bbox[2], self.bbox[3]),
            );
        write_object(
            form.insert(Name(b"Resources")),
            &Object::Dictionary(self.resources.clone()),
            &mapping,
        )?;
        Ok(())
    }
}
fn collect(
    object: &Object,
    doc: &Document,
    seen: &mut BTreeSet<ObjectId>,
    depth: usize,
) -> Result<()> {
    if depth > 64 || seen.len() > 4096 {
        return Err(invalid("PDF figure resource graph exceeds its limits."));
    }
    match object {
        Object::Reference(id) => {
            if seen.insert(*id) {
                collect(doc.get_object(*id).map_err(invalid)?, doc, seen, depth + 1)?;
            }
        }
        Object::Array(items) => {
            for item in items {
                collect(item, doc, seen, depth + 1)?;
            }
        }
        Object::Dictionary(dict) => {
            for (_, item) in dict.iter() {
                collect(item, doc, seen, depth + 1)?;
            }
        }
        Object::Stream(stream) => {
            for (key, item) in stream.dict.iter() {
                if key != b"Length" {
                    collect(item, doc, seen, depth + 1)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn write_object(writer: Obj<'_>, object: &Object, mapping: &BTreeMap<ObjectId, Ref>) -> Result<()> {
    match object {
        Object::Null => writer.primitive(pdf_writer::Null),
        Object::Boolean(value) => writer.primitive(*value),
        Object::Integer(value) => writer.primitive(i32::try_from(*value).map_err(invalid)?),
        Object::Real(value) => {
            if !value.is_finite() {
                return Err(invalid("Non-finite PDF number."));
            }
            writer.primitive(*value)
        }
        Object::Name(value) => writer.primitive(Name(value)),
        Object::String(value, _) => writer.primitive(Str(value)),
        Object::Reference(id) => writer.primitive(
            *mapping
                .get(id)
                .ok_or_else(|| invalid("Unmapped PDF resource."))?,
        ),
        Object::Array(values) => {
            let mut array = writer.array();
            for value in values {
                write_object(array.push(), value, mapping)?;
            }
        }
        Object::Dictionary(values) => {
            let mut dict = writer.dict();
            for (key, value) in values.iter() {
                write_object(dict.insert(Name(key)), value, mapping)?;
            }
        }
        Object::Stream(_) => return Err(invalid("A PDF stream must be an indirect resource.")),
    }
    Ok(())
}
