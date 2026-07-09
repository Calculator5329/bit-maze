//! bit-maze `.rec` replay format (Phase 7) — an input log *is* a binary file.
//!
//! Very on-theme: a run of the game is nothing but a level plus the exact
//! sequence of movement inputs. Because [`crate::world::World`] is fully
//! deterministic (no floats, no host time, no host randomness — see the ROADMAP
//! design rules), replaying that input sequence against the same level
//! reproduces the *exact* final state: player position, score, and every wall
//! the triggers mutated. So the whole run compresses to a magic byte, a version,
//! a level reference, and **2 bits per move**.
//!
//! ## On-disk layout (v1)
//!
//! All integers little-endian, matching `.bm`/`.spr`.
//!
//! | Offset | Size        | Field        | Notes                                         |
//! |-------:|------------:|--------------|-----------------------------------------------|
//! | 0      | 2           | `magic`      | `0x42 0x52` = ASCII `"BR"`. Byte-swap → reject.|
//! | 2      | 1           | `version`    | `1`.                                          |
//! | 3      | 1           | `name_len`   | length of the level reference, `0..=255`.     |
//! | 4      | `name_len`  | `name`       | UTF-8 level reference (e.g. the `.bm` path).  |
//! | ..     | 2           | `move_count` | `u16` LE: number of moves that follow.        |
//! | ..     | ⌈count·2/8⌉ | `moves`      | 2 bits per move, MSB-first, first move high.  |
//!
//! Move codes (2 bits): `00` Up, `01` Down, `10` Left, `11` Right. Only the four
//! directions are recordable — a no-op input (`Move::None`) is never logged, so
//! the packed stream is exactly the moves that were applied.

use crate::world::Move;
use std::fmt;

/// Magic bytes at offset 0: ASCII `"BR"` (bit-maze replay).
pub const MAGIC: [u8; 2] = [0x42, 0x52];
/// The only replay format version this build understands.
pub const VERSION: u8 = 1;
/// Fixed prefix before the level name: `magic` 2 + `version` 1 + `name_len` 1.
pub const HEADER_LEN: usize = 4;

/// A decoded replay: the level reference it was recorded against and the exact
/// sequence of moves that were applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay {
    /// The level reference stored at record time (typically the `.bm` path).
    pub level_ref: String,
    /// The recorded moves, in order (only the four directions).
    pub moves: Vec<Move>,
}

/// Everything that can go wrong decoding a `.rec` file. Mirrors the discipline of
/// the other formats: magic + version + exact-length checks, never a panic.
#[derive(Debug)]
pub enum ReplayError {
    /// File shorter than the fixed 4-byte prefix.
    ShortHeader(usize),
    /// Magic did not match. Carries the two bytes seen (spots byte-swapped `"RB"`).
    BadMagic([u8; 2]),
    UnknownVersion(u8),
    /// File ended before a declared field could be read.
    Truncated { needed: usize, had: usize, what: &'static str },
    /// Trailing bytes remained after a complete parse.
    TrailingBytes { extra: usize },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::ShortHeader(n) => {
                write!(f, "replay too short: {n} bytes, need at least {HEADER_LEN} for the header")
            }
            ReplayError::BadMagic(m) => {
                if *m == [MAGIC[1], MAGIC[0]] {
                    write!(
                        f,
                        "bad replay magic {:#04x} {:#04x}: looks like byte-swapped \"RB\" — \
                         expected \"BR\" ({:#04x} {:#04x})",
                        m[0], m[1], MAGIC[0], MAGIC[1]
                    )
                } else {
                    write!(
                        f,
                        "bad replay magic {:#04x} {:#04x}: not a bit-maze replay \
                         (expected \"BR\" {:#04x} {:#04x})",
                        m[0], m[1], MAGIC[0], MAGIC[1]
                    )
                }
            }
            ReplayError::UnknownVersion(v) => {
                write!(f, "unknown replay version {v}: this build only understands version {VERSION}")
            }
            ReplayError::Truncated { needed, had, what } => {
                write!(f, "truncated replay: needed {needed} bytes for {what}, only {had} remain")
            }
            ReplayError::TrailingBytes { extra } => {
                write!(f, "{extra} trailing byte(s) after a complete replay")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// The 2-bit code for a direction, or `None` for a non-recordable input.
fn move_code(m: Move) -> Option<u8> {
    match m {
        Move::Up => Some(0),
        Move::Down => Some(1),
        Move::Left => Some(2),
        Move::Right => Some(3),
        Move::None => None,
    }
}

/// The direction for a 2-bit code (only the low two bits are used).
fn move_from_code(code: u8) -> Move {
    match code & 0b11 {
        0 => Move::Up,
        1 => Move::Down,
        2 => Move::Left,
        _ => Move::Right,
    }
}

/// Packed-moves byte length for `count` moves: `ceil(count*2/8)`.
pub fn packed_len(count: usize) -> usize {
    (count * 2).div_ceil(8)
}

impl Replay {
    /// Build a replay from a level reference and a move sequence. Non-directional
    /// inputs (`Move::None`) are dropped, so the stored stream is exactly the
    /// moves that would be applied. `level_ref` is truncated to 255 bytes (the
    /// `name_len` field is a `u8`).
    pub fn new(level_ref: &str, moves: &[Move]) -> Replay {
        let mut name = level_ref.to_string();
        while name.len() > 255 {
            name.pop();
        }
        Replay {
            level_ref: name,
            moves: moves.iter().copied().filter(|m| move_code(*m).is_some()).collect(),
        }
    }

    /// Serialize to the exact on-disk `.rec` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let count = self.moves.len();
        let mut out = Vec::with_capacity(HEADER_LEN + self.level_ref.len() + 2 + packed_len(count));
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.push(self.level_ref.len() as u8);
        out.extend_from_slice(self.level_ref.as_bytes());
        out.extend_from_slice(&(count as u16).to_le_bytes());

        let mut packed = vec![0u8; packed_len(count)];
        for (i, &m) in self.moves.iter().enumerate() {
            let code = move_code(m).unwrap_or(0);
            let shift = (3 - (i % 4)) * 2; // MSB-first within the byte
            packed[i / 4] |= code << shift;
        }
        out.extend_from_slice(&packed);
        out
    }

    /// Parse a `.rec` file, enforcing magic, version, and exact length.
    pub fn from_bytes(data: &[u8]) -> Result<Replay, ReplayError> {
        if data.len() < HEADER_LEN {
            return Err(ReplayError::ShortHeader(data.len()));
        }
        let magic = [data[0], data[1]];
        if magic != MAGIC {
            return Err(ReplayError::BadMagic(magic));
        }
        if data[2] != VERSION {
            return Err(ReplayError::UnknownVersion(data[2]));
        }
        let name_len = data[3] as usize;

        let mut cursor = HEADER_LEN;
        let name_end = cursor + name_len;
        if name_end > data.len() {
            return Err(ReplayError::Truncated {
                needed: name_len,
                had: data.len() - cursor,
                what: "the level reference",
            });
        }
        let level_ref = String::from_utf8_lossy(&data[cursor..name_end]).into_owned();
        cursor = name_end;

        if cursor + 2 > data.len() {
            return Err(ReplayError::Truncated {
                needed: 2,
                had: data.len() - cursor,
                what: "the move count",
            });
        }
        let count = u16::from_le_bytes([data[cursor], data[cursor + 1]]) as usize;
        cursor += 2;

        let need = packed_len(count);
        if cursor + need > data.len() {
            return Err(ReplayError::Truncated {
                needed: need,
                had: data.len() - cursor,
                what: "the packed moves",
            });
        }
        let packed = &data[cursor..cursor + need];
        cursor += need;

        if cursor != data.len() {
            return Err(ReplayError::TrailingBytes { extra: data.len() - cursor });
        }

        let mut moves = Vec::with_capacity(count);
        for i in 0..count {
            let shift = (3 - (i % 4)) * 2;
            let code = (packed[i / 4] >> shift) & 0b11;
            moves.push(move_from_code(code));
        }
        Ok(Replay { level_ref, moves })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_moves_and_name() {
        let moves = vec![
            Move::Right,
            Move::Right,
            Move::Down,
            Move::Left,
            Move::Up,
            Move::Down, // 6 moves -> spans two packed bytes
        ];
        let r = Replay::new("levels/garden.bm", &moves);
        let bytes = r.to_bytes();
        assert_eq!(&bytes[0..2], &MAGIC);
        assert_eq!(bytes[2], VERSION);
        let back = Replay::from_bytes(&bytes).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.moves, moves);
        assert_eq!(back.level_ref, "levels/garden.bm");
    }

    #[test]
    fn none_moves_are_dropped_on_construction() {
        let r = Replay::new("x", &[Move::Up, Move::None, Move::Down]);
        assert_eq!(r.moves, vec![Move::Up, Move::Down]);
    }

    #[test]
    fn two_bits_per_move_packing_is_compact() {
        // 4 moves pack into exactly 1 byte; 5 into 2.
        assert_eq!(packed_len(4), 1);
        assert_eq!(packed_len(5), 2);
        // Up,Down,Left,Right = 00 01 10 11 = 0b00011011 = 0x1B in one byte.
        let r = Replay::new("", &[Move::Up, Move::Down, Move::Left, Move::Right]);
        let bytes = r.to_bytes();
        assert_eq!(*bytes.last().unwrap(), 0x1B, "MSB-first 2-bit packing");
    }

    #[test]
    fn empty_move_stream_round_trips() {
        let r = Replay::new("lvl", &[]);
        let bytes = r.to_bytes();
        let back = Replay::from_bytes(&bytes).unwrap();
        assert_eq!(back.moves.len(), 0);
        assert_eq!(back.level_ref, "lvl");
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        let mut bad = Replay::new("a", &[Move::Up]).to_bytes();
        bad.swap(0, 1); // "RB"
        assert!(matches!(Replay::from_bytes(&bad), Err(ReplayError::BadMagic(_))));

        let mut badv = Replay::new("a", &[Move::Up]).to_bytes();
        badv[2] = 9;
        assert!(matches!(Replay::from_bytes(&badv), Err(ReplayError::UnknownVersion(9))));
    }

    #[test]
    fn truncated_and_trailing_are_rejected() {
        let full = Replay::new("abc", &[Move::Up, Move::Down]).to_bytes();
        assert!(matches!(Replay::from_bytes(&full[..full.len() - 1]), Err(ReplayError::Truncated { .. })));
        let mut long = full.clone();
        long.push(0);
        assert!(matches!(Replay::from_bytes(&long), Err(ReplayError::TrailingBytes { .. })));
        // Too short for even the header.
        assert!(matches!(Replay::from_bytes(&[0x42]), Err(ReplayError::ShortHeader(1))));
    }
}
