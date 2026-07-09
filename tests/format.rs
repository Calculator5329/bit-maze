//! Integration tests for the `.bm` v1 format: round-trips and every rejection
//! path required by the Phase 1 spec.

use bitmaze::format::{BmError, Level, Script, HEADER_LEN, MAGIC, VERSION};

/// The exact 16 bytes from the ROADMAP `xxd` contract: an 8x8 walls-only maze.
const FIRST_BM: [u8; 16] = [
    0x42, 0x4d, 0x01, 0x00, 0x08, 0x08, 0x01, 0x00, // header: BM v1 flags=0 8x8 1 plane
    0xff, 0x81, 0xbd, 0xa5, 0xaf, 0x81, 0xfb, 0xff, // one byte per row
];

#[test]
fn header_round_trip() {
    let level = Level::blank(12, 7);
    let bytes = level.to_bytes();
    // Header fields land where the spec says.
    assert_eq!(&bytes[0..2], &MAGIC);
    assert_eq!(bytes[2], VERSION);
    assert_eq!(bytes[3], 0); // flags
    assert_eq!(bytes[4], 12); // width
    assert_eq!(bytes[5], 7); // height
    assert_eq!(bytes[6], 1); // planes
    assert_eq!(bytes[7], 0); // reserved
    let back = Level::from_bytes(&bytes).unwrap();
    assert_eq!(level, back);
}

#[test]
fn bitplane_is_msb_first() {
    // Setting the top-left tile (0,0) must set bit 7 (0x80) of byte 0.
    let mut level = Level::blank(8, 1);
    level.set_bit(0, 0, 0, true);
    assert_eq!(level.planes[0][0], 0x80, "tile (0,0) must be MSB of byte 0");

    // Setting tile (7,0) must set bit 0 (0x01).
    let mut level = Level::blank(8, 1);
    level.set_bit(0, 7, 0, true);
    assert_eq!(level.planes[0][0], 0x01, "tile (7,0) must be LSB of byte 0");

    // Round-trip a known pattern through bytes and back.
    let level = Level::from_bytes(&FIRST_BM).unwrap();
    assert!(level.get_bit(0, 0, 0)); // top-left wall
    assert!(!level.get_bit(0, 1, 1)); // interior floor (row 1 = 0x81)
    assert!(level.get_bit(0, 7, 0)); // top-right wall
    assert_eq!(level.to_bytes(), FIRST_BM);
}

#[test]
fn byte_swapped_magic_is_rejected() {
    let mut bad = FIRST_BM;
    bad[0] = 0x4d; // "MB" instead of "BM"
    bad[1] = 0x42;
    match Level::from_bytes(&bad) {
        Err(BmError::BadMagic(m)) => {
            assert_eq!(m, [0x4d, 0x42]);
            // Message must call out the byte-swap explicitly.
            assert!(BmError::BadMagic(m).to_string().contains("byte-swapped"));
        }
        other => panic!("expected BadMagic, got {other:?}"),
    }
}

#[test]
fn unknown_version_is_rejected() {
    let mut bad = FIRST_BM;
    bad[2] = 2;
    assert!(matches!(Level::from_bytes(&bad), Err(BmError::UnknownVersion(2))));
}

#[test]
fn zero_dimension_is_rejected() {
    let mut bad = FIRST_BM;
    bad[4] = 0; // width = 0
    assert!(matches!(
        Level::from_bytes(&bad),
        Err(BmError::BadDimension { width: 0, height: 8 })
    ));
}

#[test]
fn nonzero_reserved_is_rejected() {
    let mut bad = FIRST_BM;
    bad[7] = 0x99;
    assert!(matches!(Level::from_bytes(&bad), Err(BmError::BadReserved(0x99))));
}

#[test]
fn wrong_length_is_rejected() {
    // One byte too short: the last plane byte is missing.
    let short = &FIRST_BM[..FIRST_BM.len() - 1];
    assert!(matches!(Level::from_bytes(short), Err(BmError::Truncated { .. })));

    // One byte too long: trailing garbage after a complete level.
    let mut long = FIRST_BM.to_vec();
    long.push(0x00);
    assert!(matches!(
        Level::from_bytes(&long),
        Err(BmError::TrailingBytes { extra: 1 })
    ));
}

#[test]
fn short_header_is_rejected() {
    assert!(matches!(Level::from_bytes(&[0x42, 0x4d]), Err(BmError::ShortHeader(2))));
}

#[test]
fn trigger_plane_and_script_table_round_trip() {
    let mut level = Level::blank(3, 2); // 6 tiles
    level.set_bit(0, 0, 0, true);
    // One trigger byte per tile; two tiles fire scripts.
    level.triggers = Some(vec![0, 1, 0, 0, 7, 0]);
    level.scripts = Some(vec![
        Script { id: 1, bytes: vec![0x10, 0x05, 0x01] }, // push 5; halt
        Script { id: 7, bytes: vec![0x00] },             // nop
    ]);

    let bytes = level.to_bytes();
    // flags must advertise both optional sections.
    assert_eq!(bytes[3], 0b11);

    let back = Level::from_bytes(&bytes).unwrap();
    assert_eq!(level, back);
    assert_eq!(back.triggers.as_ref().unwrap().len(), 6);
    assert_eq!(back.scripts.as_ref().unwrap()[0].bytes, vec![0x10, 0x05, 0x01]);
    // Exact re-serialization is byte-identical.
    assert_eq!(back.to_bytes(), bytes);
}

#[test]
fn truncated_script_table_is_rejected() {
    // flags=0b10 (scripts only), but no table bytes follow.
    let mut bytes = vec![0x42, 0x4d, 0x01, 0b10, 0x02, 0x01, 0x01, 0x00];
    bytes.push(0x00); // plane byte for 2x1
    // No script count byte -> truncated.
    assert!(matches!(Level::from_bytes(&bytes), Err(BmError::Truncated { .. })));

    // Now a script that declares more bytes than exist.
    let mut bytes = vec![0x42, 0x4d, 0x01, 0b10, 0x02, 0x01, 0x01, 0x00];
    bytes.push(0x00); // plane
    bytes.push(0x01); // count = 1
    bytes.push(0x05); // id
    bytes.extend_from_slice(&99u16.to_le_bytes()); // len = 99, but nothing follows
    assert!(matches!(
        Level::from_bytes(&bytes),
        Err(BmError::TruncatedScript { id: 5 })
    ));
}

#[test]
fn header_len_constant_matches_layout() {
    assert_eq!(HEADER_LEN, 8);
    assert_eq!(Level::blank(1, 1).to_bytes().len(), HEADER_LEN + 1);
}
