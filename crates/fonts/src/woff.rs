//! WOFF 1.0 (W3C, 2012): the tables of an OpenType font, each compressed with zlib when that
//! makes it smaller, after a 44-byte header and a directory of 20 bytes per table.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use fontcull_read_fonts::FontRef;

use crate::subset::SubsetError;

const HEADER_LEN: usize = 44;
const ENTRY_LEN: usize = 20;

/// The WOFF font of the OpenType font `sfnt`.
///
/// # Errors
/// `sfnt` is not an OpenType font.
pub fn encode(sfnt: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let font = FontRef::new(sfnt).map_err(|e| SubsetError::new("writing WOFF", e))?;
    let records = font.table_directory.table_records();
    let mut tables = Vec::with_capacity(records.len());
    // The OpenType font the WOFF font decodes to: its header, directory and padded tables.
    let mut sfnt_size = 12 + 16 * records.len();
    for r in records {
        let data = font
            .table_data(r.tag())
            .ok_or_else(|| SubsetError::new("writing WOFF", format!("no {} table", r.tag())))?;
        let data = data.as_bytes();
        sfnt_size += pad4(data.len());
        let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
        let compressed = z
            .write_all(data)
            .and_then(|()| z.finish())
            .map_err(|e| SubsetError::new("writing WOFF", e))?;
        let stored = if compressed.len() < data.len() {
            compressed
        } else {
            data.to_vec()
        };
        tables.push((r.tag().to_be_bytes(), r.checksum(), data.len(), stored));
    }
    let mut offset = HEADER_LEN + ENTRY_LEN * tables.len();
    let mut directory = Vec::with_capacity(ENTRY_LEN * tables.len());
    for (tag, checksum, orig_len, stored) in &tables {
        directory.extend_from_slice(tag);
        directory.extend_from_slice(&u32_of(offset)?.to_be_bytes());
        directory.extend_from_slice(&u32_of(stored.len())?.to_be_bytes());
        directory.extend_from_slice(&u32_of(*orig_len)?.to_be_bytes());
        directory.extend_from_slice(&checksum.to_be_bytes());
        offset += pad4(stored.len());
    }
    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&sfnt[..4]); // the flavor: TrueType or CFF outlines
    out.extend_from_slice(&u32_of(offset)?.to_be_bytes());
    out.extend_from_slice(
        &u16::try_from(tables.len())
            .unwrap_or(u16::MAX)
            .to_be_bytes(),
    );
    out.extend_from_slice(&[0, 0]); // reserved
    out.extend_from_slice(&u32_of(sfnt_size)?.to_be_bytes());
    out.extend_from_slice(&[0, 1, 0, 0]); // version 1.0
    out.extend_from_slice(&[0; 20]); // no metadata, no private data
    out.extend_from_slice(&directory);
    for (_, _, _, stored) in &tables {
        out.extend_from_slice(stored);
        out.resize(pad4(out.len()), 0);
    }
    Ok(out)
}

fn pad4(n: usize) -> usize {
    n.div_ceil(4) * 4
}

fn u32_of(n: usize) -> Result<u32, SubsetError> {
    u32::try_from(n).map_err(|e| SubsetError::new("writing WOFF", e))
}
