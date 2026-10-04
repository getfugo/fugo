//! Kerning: the GPOS pair adjustments of OpenType fonts (coverage, class definitions, pair tables).

use super::*;

/// Coverage table: the coverage index of a glyph.
pub(super) enum Coverage<'a> {
    /// Sorted glyph ids.
    List(&'a [u8]),
    /// `(start, end, start index)` ranges, sorted.
    Ranges(&'a [u8]),
}

impl<'a> Coverage<'a> {
    pub(super) fn parse(t: &'a [u8]) -> Option<Self> {
        let n = usize::from(u16_at(t, 2)?);
        match u16_at(t, 0)? {
            1 => Some(Self::List(t.get(4..4 + 2 * n)?)),
            2 => Some(Self::Ranges(t.get(4..4 + 6 * n)?)),
            _ => None,
        }
    }

    pub(super) fn index(&self, g: u16) -> Option<usize> {
        match self {
            Self::List(ids) => {
                let n = ids.len() / 2;
                let (mut lo, mut hi) = (0, n);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let v = u16_at(ids, 2 * mid)?;
                    if v < g {
                        lo = mid + 1;
                    } else if v > g {
                        hi = mid;
                    } else {
                        return Some(mid);
                    }
                }
                None
            }
            Self::Ranges(r) => {
                let n = r.len() / 6;
                // The last range starting at or before `g`.
                let (mut lo, mut hi) = (0, n);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    if u16_at(r, 6 * mid)? <= g {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                let i = lo.checked_sub(1)?;
                let (start, end) = (u16_at(r, 6 * i)?, u16_at(r, 6 * i + 2)?);
                if g < start || g > end {
                    return None;
                }
                Some(usize::from(u16_at(r, 6 * i + 4)?) + usize::from(g - start))
            }
        }
    }
}

/// Class definition table: the class of a glyph (0 when not listed).
pub(super) enum ClassDef<'a> {
    /// First glyph and one class per glyph.
    Array(u16, &'a [u8]),
    /// `(start, end, class)` ranges, sorted.
    Ranges(&'a [u8]),
}

impl<'a> ClassDef<'a> {
    pub(super) fn parse(t: &'a [u8]) -> Option<Self> {
        match u16_at(t, 0)? {
            1 => {
                let n = usize::from(u16_at(t, 4)?);
                Some(Self::Array(u16_at(t, 2)?, t.get(6..6 + 2 * n)?))
            }
            2 => {
                let n = usize::from(u16_at(t, 2)?);
                Some(Self::Ranges(t.get(4..4 + 6 * n)?))
            }
            _ => None,
        }
    }

    pub(super) fn class(&self, g: u16) -> usize {
        let found = match self {
            Self::Array(start, values) => g
                .checked_sub(*start)
                .and_then(|i| u16_at(values, 2 * usize::from(i))),
            Self::Ranges(r) => (0..r.len() / 6).find_map(|i| {
                let (start, end) = (u16_at(r, 6 * i)?, u16_at(r, 6 * i + 2)?);
                (g >= start && g <= end)
                    .then(|| u16_at(r, 6 * i + 4))
                    .flatten()
            }),
        };
        found.map_or(0, usize::from)
    }
}

/// One GPOS pair-adjustment subtable whose values are the first glyph's X advance only (the
/// only kind `x/image` reads).
pub(super) enum PairTable<'a> {
    /// Format 1: per covered first glyph, `(second glyph, value)` records sorted by glyph.
    Glyphs {
        coverage: Coverage<'a>,
        sets: Vec<&'a [u8]>,
    },
    /// Format 2: a value per (class of the first glyph, class of the second).
    Classes {
        coverage: Coverage<'a>,
        first: ClassDef<'a>,
        second: ClassDef<'a>,
        second_count: usize,
        values: &'a [u8],
    },
}

impl<'a> PairTable<'a> {
    pub(super) fn parse(t: &'a [u8]) -> Option<Self> {
        let coverage = Coverage::parse(t.get(usize::from(u16_at(t, 2)?)..)?)?;
        // valueFormat1 = X_ADVANCE, valueFormat2 = none.
        if u16_at(t, 4)? != 0x0004 || u16_at(t, 6)? != 0 {
            return None;
        }
        match u16_at(t, 0)? {
            1 => {
                let n = usize::from(u16_at(t, 8)?);
                let sets = (0..n)
                    .map(|i| {
                        let set = t.get(usize::from(u16_at(t, 10 + 2 * i)?)..)?;
                        let count = usize::from(u16_at(set, 0)?);
                        set.get(2..2 + 4 * count)
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(Self::Glyphs { coverage, sets })
            }
            2 => {
                let first = ClassDef::parse(t.get(usize::from(u16_at(t, 8)?)..)?)?;
                let second = ClassDef::parse(t.get(usize::from(u16_at(t, 10)?)..)?)?;
                let first_count = usize::from(u16_at(t, 12)?);
                let second_count = usize::from(u16_at(t, 14)?);
                let values = t.get(16..16 + 2 * first_count * second_count)?;
                Some(Self::Classes {
                    coverage,
                    first,
                    second,
                    second_count,
                    values,
                })
            }
            _ => None,
        }
    }

    /// The value of pair `(a, b)`, `None` when this subtable does not cover it (the next one
    /// is asked then).
    pub(super) fn value(&self, a: u16, b: u16) -> Option<i16> {
        match self {
            Self::Glyphs { coverage, sets } => {
                let set = sets.get(coverage.index(a)?)?;
                for i in 0..set.len() / 4 {
                    let second = u16_at(set, 4 * i)?;
                    if second == b {
                        return i16_at(set, 4 * i + 2);
                    }
                    if second > b {
                        return None;
                    }
                }
                None
            }
            Self::Classes {
                coverage,
                first,
                second,
                second_count,
                values,
            } => {
                coverage.index(a)?;
                let i = first.class(a) * second_count + second.class(b);
                i16_at(values, 2 * i)
            }
        }
    }
}

/// The feature indices of the default language system of `script`.
pub(super) fn script_features(gpos: &[u8], script: &[u8; 4]) -> Option<Vec<usize>> {
    let list = gpos.get(usize::from(u16_at(gpos, 4)?)..)?;
    let count = usize::from(u16_at(list, 0)?);
    let offset = (0..count).find_map(|i| {
        (list.get(2 + 6 * i..6 + 6 * i)? == script).then(|| u16_at(list, 6 + 6 * i))?
    })?;
    let script = list.get(usize::from(offset)..)?;
    let default = usize::from(u16_at(script, 0)?);
    if default == 0 {
        return None;
    }
    let langsys = script.get(default..)?;
    let n = usize::from(u16_at(langsys, 4)?);
    (0..n)
        .map(|i| u16_at(langsys, 6 + 2 * i).map(usize::from))
        .collect()
}

/// The pair tables of the GPOS `kern` feature, in lookup order (`x/image`'s subset).
pub(super) fn gpos_kerning(gpos: &[u8]) -> Vec<PairTable<'_>> {
    let parse = || -> Option<Vec<PairTable<'_>>> {
        if u16_at(gpos, 0)? != 1 || u16_at(gpos, 2)? > 1 {
            return None;
        }
        let features = script_features(gpos, b"latn")
            .filter(|f| !f.is_empty())
            .or_else(|| script_features(gpos, b"DFLT"))?;
        let feature_list = gpos.get(usize::from(u16_at(gpos, 6)?)..)?;
        let lookup_list = gpos.get(usize::from(u16_at(gpos, 8)?)..)?;
        let mut lookups = Vec::new();
        for f in features {
            if feature_list.get(2 + 6 * f..6 + 6 * f)? != b"kern" {
                continue;
            }
            let feature = feature_list.get(usize::from(u16_at(feature_list, 6 + 6 * f)?)..)?;
            let n = usize::from(u16_at(feature, 2)?);
            for i in 0..n {
                lookups.push(usize::from(u16_at(feature, 4 + 2 * i)?));
            }
        }
        let mut tables = Vec::new();
        'lookups: for l in lookups {
            let lookup = lookup_list.get(usize::from(u16_at(lookup_list, 2 + 2 * l)?)..)?;
            let (kind, flags) = (u16_at(lookup, 0)?, u16_at(lookup, 2)?);
            let n = usize::from(u16_at(lookup, 4)?);
            let mut subtables = Vec::with_capacity(n);
            for i in 0..n {
                let sub = lookup.get(usize::from(u16_at(lookup, 6 + 2 * i)?)..)?;
                match kind {
                    2 => subtables.push(sub),
                    // Extension positioning: a 32-bit offset to the real subtable.
                    9 => {
                        if u16_at(sub, 0)? != 1 {
                            return None;
                        }
                        if u16_at(sub, 2)? != 2 {
                            continue 'lookups;
                        }
                        subtables.push(sub.get(usize::try_from(u32_at(sub, 4)?).ok()?..)?);
                    }
                    _ => continue 'lookups,
                }
            }
            // A mark filtering set is not supported.
            if flags & 0x0010 != 0 {
                continue;
            }
            tables.extend(subtables.into_iter().filter_map(PairTable::parse));
        }
        Some(tables)
    };
    parse().unwrap_or_default()
}

/// Where kerning values come from.
pub(super) enum Kerning<'a> {
    Gpos(Vec<PairTable<'a>>),
    /// The first subtable of a version 0 `kern` table: `(left << 16 | right, value)` pairs.
    Table(&'a [u8]),
    None,
}

impl<'a> Kerning<'a> {
    pub(super) fn of(font: &'a [u8]) -> Self {
        if let Some(gpos) = table(font, b"GPOS") {
            let tables = gpos_kerning(gpos);
            if !tables.is_empty() {
                return Self::Gpos(tables);
            }
        }
        let pairs = || -> Option<&'a [u8]> {
            let kern = table(font, b"kern")?;
            // Version 0, at least one subtable; the first one, horizontal, format 0.
            if u16_at(kern, 0)? != 0 || u16_at(kern, 2)? == 0 || u16_at(kern, 4)? != 0 {
                return None;
            }
            if *kern.get(8)? != 0 || *kern.get(9)? != 0x01 {
                return None;
            }
            let n = usize::from(u16_at(kern, 10)?);
            kern.get(18..18 + 6 * n)
        };
        pairs().map_or(Self::None, Self::Table)
    }

    /// The kerning of glyphs `a` then `b` in font units.
    pub(super) fn value(&self, a: u16, b: u16) -> i16 {
        match self {
            Self::Gpos(tables) => tables.iter().find_map(|t| t.value(a, b)).unwrap_or(0),
            Self::Table(pairs) => {
                let key = u32::from(a) << 16 | u32::from(b);
                let (mut lo, mut hi) = (0, pairs.len() / 6);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let Some(k) = u32_at(pairs, 6 * mid) else {
                        return 0;
                    };
                    if k < key {
                        lo = mid + 1;
                    } else if k > key {
                        hi = mid;
                    } else {
                        return i16_at(pairs, 6 * mid + 4).unwrap_or(0);
                    }
                }
                0
            }
            Self::None => 0,
        }
    }
}
