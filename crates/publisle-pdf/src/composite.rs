//! Preserve many-glyph Unicode clusters as single PDF characters. The subset is
//! trusted parser output, but every generated TrueType field remains bounded.
use crate::{
    fonts::Unit,
    model::{Diagnostic, Result},
};
use std::collections::BTreeMap;
use ttf_parser::{Face, GlyphId, Tag};

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}
fn invalid() -> Diagnostic {
    Diagnostic::new(
        "font-composite",
        "Shaping cluster exceeds TrueType limits; choose a compatible pinned font.",
    )
}

pub fn build(
    subset: Vec<u8>,
    units: &BTreeMap<Unit, u16>,
    mapper: &subsetter::GlyphRemapper,
) -> Result<(Vec<u8>, BTreeMap<u16, u16>)> {
    let face = Face::parse(&subset, 0).map_err(|_| invalid())?;
    if face.raw_face().table(Tag::from_bytes(b"glyf")).is_none() {
        let mut gids = BTreeMap::new();
        let mut mapped = BTreeMap::new();
        for (unit, cid) in units {
            if unit.components.len() != 1 || unit.components[0].1 != 0 || unit.components[0].2 != 0
            {
                return Err(invalid());
            }
            let gid = mapper.get(unit.components[0].0).ok_or_else(invalid)?;
            if mapped
                .insert(gid, &unit.unicode)
                .is_some_and(|previous| previous != &unit.unicode)
            {
                return Err(invalid());
            }
            gids.insert(*cid, gid);
        }
        return Ok((subset, gids));
    }
    let mut tables = BTreeMap::<[u8; 4], Vec<u8>>::new();
    for i in 0..u16_at(&subset, 4) as usize {
        let at = 12 + i * 16;
        let tag = subset[at..at + 4].try_into().unwrap();
        let start = u32_at(&subset, at + 8) as usize;
        let len = u32_at(&subset, at + 12) as usize;
        tables.insert(
            tag,
            subset.get(start..start + len).ok_or_else(invalid)?.to_vec(),
        );
    }
    let mut head = tables.remove(b"head").ok_or_else(invalid)?;
    let mut maxp = tables.remove(b"maxp").ok_or_else(invalid)?;
    let mut hhea = tables.remove(b"hhea").ok_or_else(invalid)?;
    let mut glyf = tables.remove(b"glyf").ok_or_else(invalid)?;
    let loca = tables.remove(b"loca").ok_or_else(invalid)?;
    let count = face.number_of_glyphs();
    let mut offsets = Vec::new();
    for i in 0..=count as usize {
        offsets.push(if u16_at(&head, 50) == 0 {
            u16_at(&loca, i * 2) as u32 * 2
        } else {
            u32_at(&loca, i * 4)
        });
    }
    // Replace all horizontal metrics with the unambiguous full form.
    let mut metrics = Vec::new();
    for gid in 0..count {
        metrics.extend_from_slice(
            &face
                .glyph_hor_advance(GlyphId(gid))
                .unwrap_or(0)
                .to_be_bytes(),
        );
        metrics.extend_from_slice(
            &face
                .glyph_hor_side_bearing(GlyphId(gid))
                .unwrap_or(0)
                .to_be_bytes(),
        );
    }
    glyf.truncate(*offsets.last().ok_or_else(invalid)? as usize);
    let mut gids = BTreeMap::new();
    let mut next = count;
    let mut max_components = u16_at(&maxp, 28);
    let mut max_depth = u16_at(&maxp, 30);
    let mut advance_max = u16_at(&hhea, 10);
    for (unit, cid) in units {
        if unit.components.len() == 1 && unit.components[0].1 == 0 && unit.components[0].2 == 0 {
            gids.insert(*cid, mapper.get(unit.components[0].0).ok_or_else(invalid)?);
            continue;
        }
        if next == u16::MAX {
            return Err(invalid());
        }
        let mut bbox = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        for (old, x, y) in &unit.components {
            let gid = mapper.get(*old).ok_or_else(invalid)?;
            if let Some(rect) = face.glyph_bounding_box(GlyphId(gid)) {
                let bounds = [
                    rect.x_min as i32 + x,
                    rect.y_min as i32 + y,
                    rect.x_max as i32 + x,
                    rect.y_max as i32 + y,
                ];
                for (i, bound) in bounds.into_iter().enumerate() {
                    let bound = i16::try_from(bound).map_err(|_| invalid())?;
                    bbox[i] = if i < 2 {
                        bbox[i].min(bound)
                    } else {
                        bbox[i].max(bound)
                    };
                }
            }
        }
        if bbox[0] == i16::MAX {
            bbox = [0; 4];
        }
        glyf.extend_from_slice(&(-1i16).to_be_bytes());
        for bound in bbox {
            glyf.extend_from_slice(&bound.to_be_bytes());
        }
        for (i, (old, x, y)) in unit.components.iter().enumerate() {
            let flags = 0x0003u16
                | if i + 1 < unit.components.len() {
                    0x0020
                } else {
                    0
                };
            glyf.extend_from_slice(&flags.to_be_bytes());
            glyf.extend_from_slice(&mapper.get(*old).ok_or_else(invalid)?.to_be_bytes());
            glyf.extend_from_slice(&i16::try_from(*x).map_err(|_| invalid())?.to_be_bytes());
            glyf.extend_from_slice(&i16::try_from(*y).map_err(|_| invalid())?.to_be_bytes());
        }
        while !glyf.len().is_multiple_of(4) {
            glyf.push(0);
        }
        offsets.push(glyf.len() as u32);
        let advance = u16::try_from(unit.advance).map_err(|_| invalid())?;
        metrics.extend_from_slice(&advance.to_be_bytes());
        metrics.extend_from_slice(&bbox[0].to_be_bytes());
        advance_max = advance_max.max(advance);
        max_components = max_components.max(unit.components.len() as u16);
        // Source composites can themselves contain composites.
        max_depth = max_depth.max(u16_at(&maxp, 30).checked_add(1).ok_or_else(invalid)?);
        for (i, bound) in bbox.into_iter().enumerate() {
            let at = 36 + i * 2;
            let old = u16_at(&head, at) as i16;
            put16(
                &mut head,
                at,
                if i < 2 {
                    old.min(bound)
                } else {
                    old.max(bound)
                } as u16,
            );
        }
        gids.insert(*cid, next);
        next += 1;
    }
    put16(&mut head, 50, 1);
    put32(&mut head, 8, 0);
    put16(&mut maxp, 4, next);
    put16(&mut maxp, 28, max_components);
    put16(&mut maxp, 30, max_depth);
    // Upper bounds for composite points/contours, including nested source outlines.
    let points = u16_at(&maxp, 6).max(u16_at(&maxp, 10));
    let contours = u16_at(&maxp, 8).max(u16_at(&maxp, 12));
    put16(&mut maxp, 10, points.saturating_mul(max_components));
    put16(&mut maxp, 12, contours.saturating_mul(max_components));
    put16(&mut hhea, 34, next);
    put16(&mut hhea, 10, advance_max);
    tables.insert(*b"head", head);
    tables.insert(*b"maxp", maxp);
    tables.insert(*b"hhea", hhea);
    tables.insert(*b"glyf", glyf);
    tables.insert(
        *b"loca",
        offsets.into_iter().flat_map(u32::to_be_bytes).collect(),
    );
    tables.insert(*b"hmtx", metrics);
    let n = tables.len() as u16;
    let power = 1u16 << n.ilog2();
    let mut output = vec![0; 12 + tables.len() * 16];
    put32(&mut output, 0, 0x00010000);
    put16(&mut output, 4, n);
    put16(&mut output, 6, power * 16);
    put16(&mut output, 8, power.ilog2() as u16);
    put16(&mut output, 10, n * 16 - power * 16);
    let mut head_offset = 0;
    for (i, (tag, bytes)) in tables.into_iter().enumerate() {
        let at = 12 + i * 16;
        let offset = output.len();
        output[at..at + 4].copy_from_slice(&tag);
        put32(&mut output, at + 4, checksum(&bytes));
        put32(&mut output, at + 8, offset as u32);
        put32(&mut output, at + 12, bytes.len() as u32);
        output.extend_from_slice(&bytes);
        while !output.len().is_multiple_of(4) {
            output.push(0);
        }
        if tag == *b"head" {
            head_offset = offset;
        }
    }
    let adjustment = 0xB1B0AFBAu32.wrapping_sub(checksum(&output));
    put32(&mut output, head_offset + 8, adjustment);
    Face::parse(&output, 0).map_err(|_| invalid())?;
    Ok((output, gids))
}
