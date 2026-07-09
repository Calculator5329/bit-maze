//! bit-maze `.bm` level format v1 — parse and write.
//!
//! See `docs/FORMAT.md` for the authoritative on-disk layout. In brief:
//! an 8-byte header, then `planes` bitplanes (MSB-first, row-major), then an
//! optional byte-per-tile trigger plane and an optional script table.
//!
//! All multibyte integers are little-endian. Bit 7 of a plane byte is the
//! leftmost tile of its group of 8.

use std::fmt;

/// Magic bytes at offset 0: ASCII `"BM"`.
pub const MAGIC: [u8; 2] = [0x42, 0x4D];
/// The only format version this build understands.
pub const VERSION: u8 = 1;
/// Fixed header size in bytes.
pub const HEADER_LEN: usize = 8;

/// `flags` bit0: a byte trigger plane follows the bitplanes.
pub const FLAG_TRIGGERS: u8 = 0b0000_0001;
/// `flags` bit1: a script table follows the trigger plane.
pub const FLAG_SCRIPTS: u8 = 0b0000_0010;

/// A single trigger script stored in the level's script table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Trigger id (1..=255); trigger-plane bytes index scripts by this id.
    pub id: u8,
    /// Raw BitVM bytecode. Not executed in Phase 1 — stored faithfully.
    pub bytes: Vec<u8>,
}

/// A fully-parsed level. Owns all plane data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub width: u8,
    pub height: u8,
    /// One entry per bitplane; each is `ceil(width*height/8)` bytes.
    /// Plane 0 is always WALLS (1 = wall, 0 = floor).
    pub planes: Vec<Vec<u8>>,
    /// Byte-per-tile trigger plane, `width*height` bytes, iff present.
    pub triggers: Option<Vec<u8>>,
    /// Script table, iff present.
    pub scripts: Option<Vec<Script>>,
}

/// Everything that can go wrong loading or validating a `.bm` file.
#[derive(Debug)]
pub enum BmError {
    Io(std::io::Error),
    /// Header shorter than 8 bytes.
    ShortHeader(usize),
    /// Magic did not match. Carries the two bytes seen so we can spot `MB`.
    BadMagic([u8; 2]),
    UnknownVersion(u8),
    /// `reserved` header byte was nonzero.
    BadReserved(u8),
    /// width or height was 0 (valid range is 1..=255).
    BadDimension { width: u8, height: u8 },
    /// `planes` was 0 (at least the walls plane is required).
    NoPlanes,
    /// File ended before an expected field could be read.
    Truncated { needed: usize, had: usize, what: &'static str },
    /// A script table declared more bytes than the file contained.
    TruncatedScript { id: u8 },
    /// Trailing bytes remained after a complete parse.
    TrailingBytes { extra: usize },
}

impl fmt::Display for BmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BmError::Io(e) => write!(f, "I/O error: {e}"),
            BmError::ShortHeader(n) => {
                write!(f, "file too short: {n} bytes, need at least {HEADER_LEN} for the header")
            }
            BmError::BadMagic(m) => {
                if *m == [MAGIC[1], MAGIC[0]] {
                    write!(
                        f,
                        "bad magic {:#04x} {:#04x}: this looks like byte-swapped \"MB\" — \
                         the format is little-endian, expected \"BM\" ({:#04x} {:#04x})",
                        m[0], m[1], MAGIC[0], MAGIC[1]
                    )
                } else {
                    write!(
                        f,
                        "bad magic {:#04x} {:#04x}: not a bit-maze file (expected \"BM\" {:#04x} {:#04x})",
                        m[0], m[1], MAGIC[0], MAGIC[1]
                    )
                }
            }
            BmError::UnknownVersion(v) => {
                write!(f, "unknown format version {v}: this build only understands version {VERSION}")
            }
            BmError::BadReserved(r) => {
                write!(f, "reserved header byte must be 0 in v1, found {r:#04x}")
            }
            BmError::BadDimension { width, height } => {
                write!(f, "dimensions {width}x{height} out of range: width and height must be 1..=255")
            }
            BmError::NoPlanes => write!(f, "planes=0: a level needs at least the walls plane"),
            BmError::Truncated { needed, had, what } => {
                write!(f, "truncated file: needed {needed} bytes for {what}, only {had} remain")
            }
            BmError::TruncatedScript { id } => {
                write!(f, "truncated script table: script id {id} declares more bytes than remain in the file")
            }
            BmError::TrailingBytes { extra } => {
                write!(f, "{extra} trailing byte(s) after a complete level: file is longer than its declared contents")
            }
        }
    }
}

impl std::error::Error for BmError {}

impl From<std::io::Error> for BmError {
    fn from(e: std::io::Error) -> Self {
        BmError::Io(e)
    }
}

/// Bytes needed for one bitplane of `width`×`height` tiles: `ceil(w*h/8)`.
pub fn plane_len(width: u8, height: u8) -> usize {
    let tiles = width as usize * height as usize;
    tiles.div_ceil(8)
}

impl Level {
    /// Number of tiles in this level.
    pub fn tile_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Flags byte derived from which optional sections are present.
    pub fn flags(&self) -> u8 {
        let mut flags = 0;
        if self.triggers.is_some() {
            flags |= FLAG_TRIGGERS;
        }
        if self.scripts.is_some() {
            flags |= FLAG_SCRIPTS;
        }
        flags
    }

    /// Read bit at `(x, y)` in plane `plane`. Returns false if out of range.
    pub fn get_bit(&self, plane: usize, x: u8, y: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let Some(data) = self.planes.get(plane) else {
            return false;
        };
        let idx = y as usize * self.width as usize + x as usize;
        let byte = idx / 8;
        let bit = 7 - (idx % 8); // MSB-first
        (data[byte] >> bit) & 1 == 1
    }

    /// Set bit at `(x, y)` in plane `plane` to `value`. Panics if out of range.
    pub fn set_bit(&mut self, plane: usize, x: u8, y: u8, value: bool) {
        let width = self.width as usize;
        let data = &mut self.planes[plane];
        let idx = y as usize * width + x as usize;
        let byte = idx / 8;
        let bit = 7 - (idx % 8); // MSB-first
        if value {
            data[byte] |= 1 << bit;
        } else {
            data[byte] &= !(1 << bit);
        }
    }

    /// Build an empty (all-floor) walls-only level of the given size.
    pub fn blank(width: u8, height: u8) -> Level {
        let plen = plane_len(width, height);
        Level {
            width,
            height,
            planes: vec![vec![0u8; plen]],
            triggers: None,
            scripts: None,
        }
    }

    /// Serialize this level to its exact on-disk byte representation.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.planes.len() * plane_len(self.width, self.height));
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.push(self.flags());
        out.push(self.width);
        out.push(self.height);
        out.push(self.planes.len() as u8);
        out.push(0); // reserved
        for plane in &self.planes {
            out.extend_from_slice(plane);
        }
        if let Some(triggers) = &self.triggers {
            out.extend_from_slice(triggers);
        }
        if let Some(scripts) = &self.scripts {
            out.push(scripts.len() as u8);
            for s in scripts {
                out.push(s.id);
                out.extend_from_slice(&(s.bytes.len() as u16).to_le_bytes());
                out.extend_from_slice(&s.bytes);
            }
        }
        out
    }

    /// Parse a level from raw file bytes, enforcing every v1 invariant.
    pub fn from_bytes(data: &[u8]) -> Result<Level, BmError> {
        if data.len() < HEADER_LEN {
            return Err(BmError::ShortHeader(data.len()));
        }
        let magic = [data[0], data[1]];
        if magic != MAGIC {
            return Err(BmError::BadMagic(magic));
        }
        let version = data[2];
        if version != VERSION {
            return Err(BmError::UnknownVersion(version));
        }
        let flags = data[3];
        let width = data[4];
        let height = data[5];
        let planes_count = data[6];
        let reserved = data[7];

        if reserved != 0 {
            return Err(BmError::BadReserved(reserved));
        }
        if width == 0 || height == 0 {
            return Err(BmError::BadDimension { width, height });
        }
        if planes_count == 0 {
            return Err(BmError::NoPlanes);
        }

        let mut cursor = HEADER_LEN;
        let plen = plane_len(width, height);

        // Bitplanes.
        let mut planes = Vec::with_capacity(planes_count as usize);
        for _ in 0..planes_count {
            let end = cursor + plen;
            if end > data.len() {
                return Err(BmError::Truncated {
                    needed: plen,
                    had: data.len().saturating_sub(cursor),
                    what: "a bitplane",
                });
            }
            planes.push(data[cursor..end].to_vec());
            cursor = end;
        }

        // Trigger plane (byte-per-tile).
        let triggers = if flags & FLAG_TRIGGERS != 0 {
            let tiles = width as usize * height as usize;
            let end = cursor + tiles;
            if end > data.len() {
                return Err(BmError::Truncated {
                    needed: tiles,
                    had: data.len().saturating_sub(cursor),
                    what: "the trigger plane",
                });
            }
            let t = data[cursor..end].to_vec();
            cursor = end;
            Some(t)
        } else {
            None
        };

        // Script table.
        let scripts = if flags & FLAG_SCRIPTS != 0 {
            if cursor >= data.len() {
                return Err(BmError::Truncated { needed: 1, had: 0, what: "the script count" });
            }
            let count = data[cursor];
            cursor += 1;
            let mut scripts = Vec::with_capacity(count as usize);
            for _ in 0..count {
                if cursor + 3 > data.len() {
                    return Err(BmError::Truncated {
                        needed: 3,
                        had: data.len().saturating_sub(cursor),
                        what: "a script header",
                    });
                }
                let id = data[cursor];
                let len = u16::from_le_bytes([data[cursor + 1], data[cursor + 2]]) as usize;
                cursor += 3;
                let end = cursor + len;
                if end > data.len() {
                    return Err(BmError::TruncatedScript { id });
                }
                scripts.push(Script { id, bytes: data[cursor..end].to_vec() });
                cursor = end;
            }
            Some(scripts)
        } else {
            None
        };

        if cursor != data.len() {
            return Err(BmError::TrailingBytes { extra: data.len() - cursor });
        }

        Ok(Level { width, height, planes, triggers, scripts })
    }
}
