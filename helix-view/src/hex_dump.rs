use std::io::{self, Read};
use std::path::Path;

use helix_core::Rope;

pub const BYTES_PER_LINE: usize = 16;
/// `"00000000  "`
pub const OFFSET_END: usize = 10;
/// First column of the ASCII column (after `"  "` separator).
pub const ASCII_START: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexField {
    Offset,
    Hex { byte_in_line: usize, nibble: u8 },
    Ascii { byte_in_line: usize },
    Gap,
}

/// Heuristic binary check over a leading chunk (NUL byte or known magic).
pub fn is_binary(buffer: &[u8]) -> bool {
    const BYTE_ORDER_MARKS: &[&[u8]] = &[
        &[0xEF, 0xBB, 0xBF],
        &[0x00, 0x00, 0xFE, 0xFF],
        &[0xFF, 0xFE, 0x00, 0x00],
        &[0xFE, 0xFF],
        &[0xFF, 0xFE],
    ];

    if BYTE_ORDER_MARKS.iter().any(|bom| buffer.starts_with(bom)) {
        return false;
    }

    let scan = &buffer[..buffer.len().min(1024)];
    scan.contains(&0) || buffer.starts_with(b"%PDF") || buffer.starts_with(b"\x89PNG")
}

pub fn read_file_bytes(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

pub fn bytes_to_rope(bytes: &[u8]) -> Rope {
    if bytes.is_empty() {
        return Rope::from("");
    }

    let mut lines = Vec::with_capacity(bytes.len().div_ceil(BYTES_PER_LINE));
    for (i, chunk) in bytes.chunks(BYTES_PER_LINE).enumerate() {
        lines.push(format_line(i * BYTES_PER_LINE, chunk));
    }
    Rope::from(lines.join("\n"))
}

pub fn format_line(offset: usize, chunk: &[u8]) -> String {
    let mut line = format!("{offset:08x}  ");
    for i in 0..BYTES_PER_LINE {
        if i == 8 {
            line.push(' ');
        }
        if let Some(&byte) = chunk.get(i) {
            use std::fmt::Write;
            let _ = write!(line, "{byte:02x} ");
        } else {
            line.push_str("   ");
        }
    }
    line.push(' ');
    for &byte in chunk {
        line.push(ascii_char(byte));
    }
    line
}

pub fn ascii_char(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        byte as char
    } else {
        '.'
    }
}

pub fn hex_col(byte_in_line: usize, nibble: usize) -> usize {
    debug_assert!(byte_in_line < BYTES_PER_LINE);
    debug_assert!(nibble < 2);
    OFFSET_END + byte_in_line * 3 + if byte_in_line >= 8 { 1 } else { 0 } + nibble
}

pub fn field_at_col(line_len: usize, col: usize) -> HexField {
    if col < 8 {
        HexField::Offset
    } else if col < OFFSET_END {
        HexField::Gap
    } else if col < ASCII_START {
        match col_to_hex(col) {
            Some((byte_in_line, nibble)) => HexField::Hex {
                byte_in_line,
                nibble,
            },
            None => HexField::Gap,
        }
    } else {
        let byte_in_line = col - ASCII_START;
        if byte_in_line < BYTES_PER_LINE && col < line_len {
            HexField::Ascii { byte_in_line }
        } else {
            HexField::Gap
        }
    }
}

pub fn col_to_hex(col: usize) -> Option<(usize, u8)> {
    if col < OFFSET_END || col >= ASCII_START {
        return None;
    }
    let mut rel = col - OFFSET_END;
    if rel >= 24 {
        rel -= 1;
    }
    let byte_in_line = rel / 3;
    let rem = rel % 3;
    if byte_in_line >= BYTES_PER_LINE || rem == 2 {
        return None;
    }
    Some((byte_in_line, rem as u8))
}

pub fn parse_hex_pair(line: &str, byte_in_line: usize) -> Option<u8> {
    let hi = line.as_bytes().get(hex_col(byte_in_line, 0)).copied()?;
    let lo = line.as_bytes().get(hex_col(byte_in_line, 1)).copied()?;
    let pair = [hi, lo];
    u8::from_str_radix(std::str::from_utf8(&pair).ok()?, 16).ok()
}

/// Peek the start of a file to decide whether it should open as a hex dump.
pub fn file_looks_binary(path: &Path) -> io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 1024];
    let n = file.read(&mut header)?;
    Ok(is_binary(&header[..n]))
}
