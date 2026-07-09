# bit-maze sprite format — `.spr` 1-bit sprites (v1) + palette

Sprites are the level-plane idea taken to its conclusion: a sprite **is** a
hex-editable 1-bit binary file. This is the authoritative spec for the parser in
`src/sprite.rs`. Any format change bumps the version byte and updates this file
in the same commit. It mirrors `docs/FORMAT.md`: little-endian, magic + version,
bounded `u8` dimensions, MSB-first bits, exact-length validation.

All integers are **little-endian**. Bit order within a byte is **MSB-first**
(bit 7 = the leftmost pixel in its group of 8), row-major.

## Header — 5 bytes, fixed

| Offset | Size | Field     | Notes                                                    |
|-------:|-----:|-----------|----------------------------------------------------------|
| 0      | 2    | `magic`   | `0x53 0x50` = ASCII `"SP"`. Byte-swapped `"PS"` → reject. |
| 2      | 1    | `version` | `1`. Parser refuses unknown versions loudly.             |
| 3      | 1    | `width`   | pixels, 1..=255.                                         |
| 4      | 1    | `height`  | pixels, 1..=255.                                         |

## Body

```
[header: 5 bytes]
[pixels]   exactly ceil(width*height / 8) bytes, row-major, MSB-first.
           bit = 1 -> INK, bit = 0 -> PAPER.
```

The file must be **exactly** `5 + ceil(width*height/8)` bytes. A short or long
pixel section is an error (`WrongLength`).

## Pixel addressing

For pixel `(x, y)`: `idx = y*width + x`, `byte = idx/8`, `bit = 7 - (idx%8)`
(MSB-first) — identical to `.bm` bitplane addressing.

## The `xxd` contract

The default 8×8 wall sprite (`sprites/wall.spr`) is exactly `5 + 8 = 13` bytes:

```
53 50 01 08 08            SP, v1, 8x8
ff ee ee 00 bb bb 00 ff   one byte per row (MSB = leftmost pixel)
```

`bitmaze sprite <file.spr>` renders it as ASCII `#` (ink) / `.` (paper):

```
########
###.###.
###.###.
........
#.###.##
#.###.##
........
########
```

## Palette — ink/paper → color, by role

1 bit can only say ink-or-paper; the **palette** (a plain `Palette` struct,
`src/sprite.rs`) decides what those two values paint, per tile role. Kept as data
so it is trivially swappable:

| Role   | ink (`1`)                 | paper (`0`)                     |
|--------|---------------------------|---------------------------------|
| wall   | `wall_ink` (slate brick)  | `wall_paper` (dark mortar)      |
| floor  | `floor_ink` (dot)         | `floor_paper` (near-black)      |
| item   | `item_ink` (cyan gem)     | **transparent** (floor shows)   |
| hazard | `hazard_ink` (red spikes) | **transparent** (floor shows)   |
| player | `player_ink` (amber)      | **transparent** (floor shows)   |

Colors are `0x00RR_GGBB` (minifb's layout). `Palette::DEFAULT` is the compiled-in
default and continues the Phase 3 scheme.

## How tiles render (`framebuffer::draw`)

Each tile blits one 1-bit sprite through the palette, scaled to `tile_px` with
nearest-neighbor sampling (any sprite size works at any tile size):

- **wall tile** → wall sprite (ink → `wall_ink`, paper → `wall_paper`).
- **floor tile** → floor sprite (ink → `floor_ink`, paper → `floor_paper`).
- **hazard tile** (floor with the hazards-plane bit set, and not the player's
  tile) → floor sprite first, then the spike sprite composited over it: ink →
  `hazard_ink`, paper → **skipped** (transparent), so the floor shows through the
  spikes' gaps. Stepping onto it loses the game (see `docs/FORMAT.md`).
- **item tile** (floor with the items-plane bit set, and not the player's tile)
  → floor sprite first, then the item gem composited over it: ink → `item_ink`,
  paper → **skipped** (transparent), so the floor shows through the gem's gaps.
- **player tile** → floor sprite first, then the player sprite composited over
  it: ink → `player_ink`, paper → **skipped** (transparent), so the floor shows
  through the gaps of the `@` figure. The player draws on top even when standing
  on an item.

`draw` is pure and headless: it fills a `Vec<u32>`, opens no window, and is
unit-tested by asserting individual pixels.

## Load path & fallback

`bitmaze play` (window path) loads the role sprites from the `sprites/`
directory — `wall.spr`, `floor.spr`, `player.spr`, `item.spr`, `hazard.spr` — via
`Sprites::load_from_dir`. Each sprite that is **missing or fails to parse** falls
back **individually** to its compiled-in default (`Sprite::default_wall/floor/
player/item/hazard`), and a note is printed to stderr; a missing or corrupt file
never stops the game. The `--term` path renders ASCII and uses no sprites.

## Tooling

```
bitmaze sprite <file.spr>   dump a sprite as ASCII (# ink / . paper) — the
                            headless verification tool (sprite's `dump`).
bitmaze sprite gen <dir>    write the compiled-in default sprites into <dir>
                            (regenerates sprites/: wall/floor/player/item/hazard).
```

Because sprites are 1-bit binary files, they are equally editable in `xxd`: flip
a bit, flip a pixel.
