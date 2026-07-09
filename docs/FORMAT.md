# bit-maze file format — `.bm` level files (v1)

Extracted from `ROADMAP.md`. This is the authoritative spec for the loader in
`src/format.rs`. Any format change bumps the version byte and updates this file
in the same commit.

All integers are **little-endian**. Bit order within a plane byte is
**MSB-first** (bit 7 = the leftmost tile in its group of 8), row-major.

## Header — 8 bytes, fixed

| Offset | Size | Field      | Notes                                                               |
|-------:|-----:|------------|---------------------------------------------------------------------|
| 0      | 2    | `magic`    | `0x42 0x4D` = ASCII `"BM"`. Byte-swapped `"MB"` → reject.            |
| 2      | 1    | `version`  | `1`. Loader refuses unknown versions loudly.                        |
| 3      | 1    | `flags`    | bit0 = has trigger plane; bit1 = has script table; rest reserved 0. |
| 4      | 1    | `width`    | tiles, 1..=255.                                                     |
| 5      | 1    | `height`   | tiles, 1..=255.                                                     |
| 6      | 1    | `planes`   | number of bitplanes that follow (≥1).                              |
| 7      | 1    | `reserved` | must be 0 in v1.                                                    |

## Body

```
[header: 8 bytes]
[bitplanes]        planes × ceil(width*height / 8) bytes each, row-major, MSB-first.
                   plane 0 = WALLS   (1 = wall, 0 = floor)   -- always present
                   plane 1 = ITEMS   (optional, added later)
                   plane 2 = ...     (add a plane → add a mechanic)
[trigger plane]    present iff flags.bit0. width*height BYTES (one per tile).
                   Each byte = a script id (0 = no trigger). This is a BYTE plane,
                   not a bitplane — see "Trigger binding decision" below.
[script table]     present iff flags.bit1.
                   count: u8
                   then count × { id: u8, len: u16 (LE), bytes: [len] }
                   The trigger plane's script ids index into this table by `id`.
```

The file must be **exactly** the length implied by its header + declared
sections. Trailing bytes are an error; a short file is an error.

## Trigger binding decision (consciously breaking "1 bit/tile")

A bitplane can only say *"something is here"* — 1 bit cannot say *which* script.
Rather than hack around this, we **embrace a byte-per-tile trigger plane**: each
tile gets a full `u8` script id (0 = none, 1..=255 index the script table). This
is a deliberate, documented departure from pure 1-bit-per-tile, chosen because:

- It stays flat, contiguous, and hex-editable (tile (x,y) → one byte you can eyeball).
- It's trivially expandable to 255 distinct trigger scripts per level.
- The alternative (a sparse coord→offset sidecar table) is more compact but far
  harder to hand-edit, which defeats the whole point.

The wall/item/etc. planes stay pure 1-bit. Only triggers pay the byte-per-tile
cost, and only when `flags.bit0` is set.

## The `xxd` contract

A hand-authored 8×8 walls-only level is exactly `8 + 8 = 16` bytes
(`levels/first.bm`):

```
42 4d 01 00 08 08 01 00   BM, v1, flags=0, 8x8, 1 plane
ff 81 bd a5 af 81 fb ff   the maze, one byte per row (MSB=leftmost tile)
```

`bitmaze dump` renders that as ASCII `#`/`.`; `bitmaze check` validates every
invariant.

## Bit addressing

For tile `(x, y)` in a plane: `idx = y*width + x`, `byte = idx/8`,
`bit = 7 - (idx%8)` (MSB-first). A plane is `ceil(width*height/8)` bytes.
