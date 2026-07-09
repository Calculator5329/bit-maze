//! bit-maze `.spr` 1-bit sprite format + palette — the Phase 6 renderer data.
//!
//! Sprites are the same idea as the `.bm` level planes taken to its conclusion:
//! a sprite *is* a hex-editable 1-bit binary file. See `docs/SPRITE.md` for the
//! authoritative on-disk layout. In brief: a tiny 5-byte header
//! (`magic "SP"`, `version`, `width`, `height`), then `ceil(width*height/8)`
//! packed bits, **MSB-first, row-major** — exactly the bit convention the `.bm`
//! bitplanes use (mirrors [`crate::format`]).
//!
//! A bit value of `1` is **ink**, `0` is **paper**. What those two values paint
//! is decided by a [`Palette`], which maps each tile role (wall / floor /
//! player) to an ink and a paper color. This module is pure and headless: it
//! parses, writes, and dumps sprites and holds the palette; the actual blit into
//! a pixel buffer lives in [`crate::framebuffer`].

use std::fmt;

/// Magic bytes at offset 0: ASCII `"SP"`.
pub const SPRITE_MAGIC: [u8; 2] = [0x53, 0x50];
/// The only sprite format version this build understands.
pub const SPRITE_VERSION: u8 = 1;
/// Fixed sprite header size in bytes (`magic` 2 + `version` 1 + `w` 1 + `h` 1).
pub const SPRITE_HEADER_LEN: usize = 5;

/// A single 1-bit sprite: `width`×`height` pixels, one bit each, MSB-first
/// row-major. Bit `1` = ink, bit `0` = paper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: u8,
    pub height: u8,
    /// Packed pixel bits, `ceil(width*height/8)` bytes.
    pub bits: Vec<u8>,
}

/// Everything that can go wrong parsing a `.spr` file. Mirrors the discipline of
/// [`crate::format::BmError`]: magic + version + bounds + exact-length checks,
/// with a byte-swap-aware message.
#[derive(Debug)]
pub enum SpriteError {
    /// File shorter than the 5-byte header.
    ShortHeader(usize),
    /// Magic did not match. Carries the two bytes seen so we can spot `"PS"`.
    BadMagic([u8; 2]),
    UnknownVersion(u8),
    /// width or height was 0 (valid range is 1..=255).
    BadDimension { width: u8, height: u8 },
    /// Pixel data length did not exactly match `ceil(width*height/8)`.
    WrongLength { needed: usize, had: usize },
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpriteError::ShortHeader(n) => write!(
                f,
                "sprite too short: {n} bytes, need at least {SPRITE_HEADER_LEN} for the header"
            ),
            SpriteError::BadMagic(m) => {
                if *m == [SPRITE_MAGIC[1], SPRITE_MAGIC[0]] {
                    write!(
                        f,
                        "bad sprite magic {:#04x} {:#04x}: this looks like byte-swapped \"PS\" — \
                         the format is little-endian, expected \"SP\" ({:#04x} {:#04x})",
                        m[0], m[1], SPRITE_MAGIC[0], SPRITE_MAGIC[1]
                    )
                } else {
                    write!(
                        f,
                        "bad sprite magic {:#04x} {:#04x}: not a bit-maze sprite \
                         (expected \"SP\" {:#04x} {:#04x})",
                        m[0], m[1], SPRITE_MAGIC[0], SPRITE_MAGIC[1]
                    )
                }
            }
            SpriteError::UnknownVersion(v) => write!(
                f,
                "unknown sprite version {v}: this build only understands version {SPRITE_VERSION}"
            ),
            SpriteError::BadDimension { width, height } => write!(
                f,
                "sprite dimensions {width}x{height} out of range: width and height must be 1..=255"
            ),
            SpriteError::WrongLength { needed, had } => write!(
                f,
                "sprite pixel data is {had} byte(s), header implies exactly {needed}"
            ),
        }
    }
}

impl std::error::Error for SpriteError {}

/// Bytes needed to pack a `width`×`height` 1-bit sprite: `ceil(w*h/8)`.
pub fn bits_len(width: u8, height: u8) -> usize {
    (width as usize * height as usize).div_ceil(8)
}

impl Sprite {
    /// A blank (all-paper) sprite of the given size.
    pub fn blank(width: u8, height: u8) -> Sprite {
        Sprite { width, height, bits: vec![0u8; bits_len(width, height)] }
    }

    /// Build a sprite from `height` row bytes, one **byte per row** (so
    /// `width <= 8`). Handy for hand-designing the default sprites in source.
    pub fn from_rows(width: u8, rows: &[u8]) -> Sprite {
        assert!(width <= 8, "from_rows packs one byte per row: width must be <= 8");
        // Mask off bits past `width` so unused low bits stay paper.
        let mask = if width == 8 { 0xFF } else { !(0xFFu8 >> width) };
        let mut s = Sprite::blank(width, rows.len() as u8);
        for (y, &r) in rows.iter().enumerate() {
            // Row byte is already MSB-first (bit7 = leftmost pixel), and for a
            // full-width row the sprite's bit layout matches it byte-for-byte
            // only when width==8; otherwise pack pixel by pixel.
            if width == 8 {
                s.bits[y] = r & mask;
            } else {
                for x in 0..width {
                    let bit = 7 - x; // MSB-first within the row byte
                    if (r >> bit) & 1 == 1 {
                        s.set(x, y as u8, true);
                    }
                }
            }
        }
        s
    }

    /// Read the pixel at `(x, y)`; `true` = ink. Out-of-range reads as paper.
    pub fn get(&self, x: u8, y: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y as usize * self.width as usize + x as usize;
        let byte = idx / 8;
        let bit = 7 - (idx % 8); // MSB-first
        (self.bits[byte] >> bit) & 1 == 1
    }

    /// Set the pixel at `(x, y)`. Panics if out of range.
    pub fn set(&mut self, x: u8, y: u8, value: bool) {
        let idx = y as usize * self.width as usize + x as usize;
        let byte = idx / 8;
        let bit = 7 - (idx % 8); // MSB-first
        if value {
            self.bits[byte] |= 1 << bit;
        } else {
            self.bits[byte] &= !(1 << bit);
        }
    }

    /// Serialize to the exact on-disk `.spr` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SPRITE_HEADER_LEN + self.bits.len());
        out.extend_from_slice(&SPRITE_MAGIC);
        out.push(SPRITE_VERSION);
        out.push(self.width);
        out.push(self.height);
        out.extend_from_slice(&self.bits);
        out
    }

    /// Parse a `.spr` file, enforcing magic, version, bounds, and exact length.
    pub fn from_bytes(data: &[u8]) -> Result<Sprite, SpriteError> {
        if data.len() < SPRITE_HEADER_LEN {
            return Err(SpriteError::ShortHeader(data.len()));
        }
        let magic = [data[0], data[1]];
        if magic != SPRITE_MAGIC {
            return Err(SpriteError::BadMagic(magic));
        }
        let version = data[2];
        if version != SPRITE_VERSION {
            return Err(SpriteError::UnknownVersion(version));
        }
        let width = data[3];
        let height = data[4];
        if width == 0 || height == 0 {
            return Err(SpriteError::BadDimension { width, height });
        }
        let needed = bits_len(width, height);
        let had = data.len() - SPRITE_HEADER_LEN;
        if had != needed {
            return Err(SpriteError::WrongLength { needed, had });
        }
        Ok(Sprite { width, height, bits: data[SPRITE_HEADER_LEN..].to_vec() })
    }

    /// Render the sprite as ASCII art: ink `#`, paper `.`, one row per line
    /// (each row newline-terminated). The headless verification tool, the sprite
    /// counterpart of `bitmaze dump`.
    pub fn to_ascii(&self) -> String {
        let mut out = String::with_capacity((self.width as usize + 1) * self.height as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                out.push(if self.get(x, y) { '#' } else { '.' });
            }
            out.push('\n');
        }
        out
    }

    // ---- Compiled-in default sprites (the fallback when files are absent) ----

    /// Default 8×8 wall sprite: a brick pattern (ink = brick, paper = mortar).
    pub fn default_wall() -> Sprite {
        Sprite::from_rows(8, &[0xFF, 0xEE, 0xEE, 0x00, 0xBB, 0xBB, 0x00, 0xFF])
    }

    /// Default 8×8 floor sprite: near-blank with a small center dot.
    pub fn default_floor() -> Sprite {
        Sprite::from_rows(8, &[0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00])
    }

    /// Default 8×8 player sprite: an `@`-ish figure.
    pub fn default_player() -> Sprite {
        Sprite::from_rows(8, &[0x3C, 0x42, 0x9D, 0xA5, 0xA5, 0x9E, 0x40, 0x3C])
    }
}

/// The palette: maps the two 1-bit values, per tile role, to `0x00RR_GGBB`
/// colors (minifb's layout). Kept as plain data so it is trivially swappable and
/// hex-editable-in-spirit. The player's paper is intentionally **transparent**
/// (the floor beneath shows through); every other role's paper is opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub wall_ink: u32,
    pub wall_paper: u32,
    pub floor_ink: u32,
    pub floor_paper: u32,
    /// The player figure's ink; its paper is transparent (floor shows through).
    pub player_ink: u32,
}

impl Palette {
    /// The compiled-in default palette (continues the Phase 3 color scheme).
    pub const DEFAULT: Palette = Palette {
        wall_ink: 0x0025_2B48,   // slate-blue brick
        wall_paper: 0x0012_1628, // darker mortar
        floor_ink: 0x0016_1622,  // subtle floor dot
        floor_paper: 0x000A_0A0F, // near-black floor
        player_ink: 0x00FF_B300, // bright amber
    };
}

impl Default for Palette {
    fn default() -> Self {
        Palette::DEFAULT
    }
}

/// The three role sprites the renderer needs. Loaded from files with a
/// per-sprite fallback to the compiled-in defaults, so `play` always has a full
/// set even if `sprites/` is missing or partial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprites {
    pub wall: Sprite,
    pub floor: Sprite,
    pub player: Sprite,
}

impl Default for Sprites {
    fn default() -> Self {
        Sprites {
            wall: Sprite::default_wall(),
            floor: Sprite::default_floor(),
            player: Sprite::default_player(),
        }
    }
}

/// The directory `play` looks in for the three role sprites.
pub const SPRITE_DIR: &str = "sprites";

impl Sprites {
    /// Load `wall.spr`, `floor.spr`, and `player.spr` from `dir`. Each sprite
    /// that cannot be read or parsed falls back **individually** to its
    /// compiled-in default (so a missing or corrupt file never stops the game).
    /// Returns the set plus the list of human-readable fallback notes (empty if
    /// every file loaded cleanly) so the caller can report what happened.
    pub fn load_from_dir(dir: &str) -> (Sprites, Vec<String>) {
        let mut notes = Vec::new();
        let load_one = |name: &str, fallback: Sprite, notes: &mut Vec<String>| -> Sprite {
            let path = format!("{dir}/{name}");
            match std::fs::read(&path) {
                Ok(data) => match Sprite::from_bytes(&data) {
                    Ok(s) => s,
                    Err(e) => {
                        notes.push(format!("{path}: {e} — using built-in default"));
                        fallback
                    }
                },
                Err(_) => {
                    notes.push(format!("{path}: not found — using built-in default"));
                    fallback
                }
            }
        };
        let wall = load_one("wall.spr", Sprite::default_wall(), &mut notes);
        let floor = load_one("floor.spr", Sprite::default_floor(), &mut notes);
        let player = load_one("player.spr", Sprite::default_player(), &mut notes);
        (Sprites { wall, floor, player }, notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_bytes_and_pixels() {
        let s = Sprite::default_player();
        let bytes = s.to_bytes();
        // Header lands where the spec says.
        assert_eq!(&bytes[0..2], &SPRITE_MAGIC);
        assert_eq!(bytes[2], SPRITE_VERSION);
        assert_eq!(bytes[3], 8); // width
        assert_eq!(bytes[4], 8); // height
        let back = Sprite::from_bytes(&bytes).unwrap();
        assert_eq!(s, back);
        assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn pixels_are_msb_first() {
        // Setting the top-left pixel (0,0) sets bit 7 (0x80) of byte 0.
        let mut s = Sprite::blank(8, 1);
        s.set(0, 0, true);
        assert_eq!(s.bits[0], 0x80, "pixel (0,0) must be the MSB of byte 0");

        // Setting (7,0) sets bit 0 (0x01).
        let mut s = Sprite::blank(8, 1);
        s.set(7, 0, true);
        assert_eq!(s.bits[0], 0x01, "pixel (7,0) must be the LSB of byte 0");
    }

    #[test]
    fn from_rows_matches_expected_bits() {
        // The wall's first row is all ink (0xFF), the fourth is all paper.
        let wall = Sprite::default_wall();
        for x in 0..8 {
            assert!(wall.get(x, 0), "wall row 0 is solid brick");
            assert!(!wall.get(x, 3), "wall row 3 is a mortar line");
        }
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bad = Sprite::default_wall().to_bytes();
        bad.swap(0, 1); // "PS" instead of "SP"
        match Sprite::from_bytes(&bad) {
            Err(SpriteError::BadMagic(m)) => {
                assert_eq!(m, [SPRITE_MAGIC[1], SPRITE_MAGIC[0]]);
                assert!(SpriteError::BadMagic(m).to_string().contains("byte-swapped"));
            }
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bad = Sprite::default_wall().to_bytes();
        bad[2] = 2;
        assert!(matches!(Sprite::from_bytes(&bad), Err(SpriteError::UnknownVersion(2))));
    }

    #[test]
    fn zero_dimension_is_rejected() {
        let mut bad = Sprite::default_wall().to_bytes();
        bad[3] = 0; // width = 0
        assert!(matches!(
            Sprite::from_bytes(&bad),
            Err(SpriteError::BadDimension { width: 0, height: 8 })
        ));
    }

    #[test]
    fn wrong_length_is_rejected() {
        // One byte short.
        let full = Sprite::default_wall().to_bytes();
        let short = &full[..full.len() - 1];
        assert!(matches!(Sprite::from_bytes(short), Err(SpriteError::WrongLength { .. })));
        // One byte long.
        let mut long = full.clone();
        long.push(0);
        assert!(matches!(Sprite::from_bytes(&long), Err(SpriteError::WrongLength { .. })));
    }

    #[test]
    fn short_header_is_rejected() {
        assert!(matches!(Sprite::from_bytes(&[0x53, 0x50]), Err(SpriteError::ShortHeader(2))));
    }

    #[test]
    fn ascii_dump_matches_known_pattern() {
        // A tiny 3x2 sprite: ink at (0,0) and (2,1) only.
        let mut s = Sprite::blank(3, 2);
        s.set(0, 0, true);
        s.set(2, 1, true);
        assert_eq!(s.to_ascii(), "#..\n..#\n");
    }

    #[test]
    fn missing_dir_falls_back_to_defaults() {
        let (sprites, notes) = Sprites::load_from_dir("/nonexistent-sprite-dir-xyz");
        assert_eq!(sprites, Sprites::default());
        assert_eq!(notes.len(), 3); // all three fell back
    }
}
