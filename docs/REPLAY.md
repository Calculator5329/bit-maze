# bit-maze replay format — `.rec` input logs (v1)

Added in Phase 7. The most on-theme file in the project: **a whole play session
is just a binary file** — a magic byte, a version, a level reference, and the
input sequence at **2 bits per move**. Because the world core is fully
deterministic (no floats, no host time, no host randomness — ROADMAP design rule
#4), replaying that input against the same level reproduces the *exact* final
state: player position, item score, and every wall a trigger mutated. Implemented
in `src/replay.rs`.

All integers are **little-endian**, matching `.bm` and `.spr`.

## Header + body — variable length

| Offset | Size          | Field        | Notes                                              |
|-------:|--------------:|--------------|----------------------------------------------------|
| 0      | 2             | `magic`      | `0x42 0x52` = ASCII `"BR"`. Byte-swapped `"RB"` → reject. |
| 2      | 1             | `version`    | `1`. Parser refuses unknown versions loudly.       |
| 3      | 1             | `name_len`   | length of the level reference, `0..=255`.          |
| 4      | `name_len`    | `name`       | UTF-8 level reference (typically the `.bm` path).   |
| ..     | 2             | `move_count` | `u16` LE: number of moves that follow.             |
| ..     | ⌈count·2 / 8⌉ | `moves`      | 2 bits per move, MSB-first, first move in the high bits. |

The file must be **exactly** that length; short or trailing files are rejected.

## Move encoding

Two bits per move: `00` = Up, `01` = Down, `10` = Left, `11` = Right. Moves pack
four-per-byte, MSB-first (move 0 in bits 7–6, move 1 in bits 5–4, …). Only the
four directions are recordable; a no-op input (`Move::None`) is never logged, so
the packed stream is exactly the moves that get applied on replay.

`Up,Down,Left,Right` packs into a single byte `00 01 10 11` = `0x1B`.

## Why 2 bits / move (chosen over 1 byte)

1 byte per move is simpler but wasteful; 2 bits is *maximally* on-theme (everything
is bits) and still trivial to pack/unpack. A 12-move garden run is 25 bytes total
(4-byte header + 16-byte name + 2-byte count + 3 packed bytes).

## CLI

```
bitmaze play --term --record <run.rec> <level.bm>   # log inputs while you play
bitmaze play --replay <run.rec> <level.bm>          # re-run and print final state
```

`--record` requires `--term` (headless, deterministic recording). `--replay`
needs no display: it applies the logged moves to a fresh world and prints the
reproduced final map, position, and score. A test
(`tests/replay.rs`) records a run, round-trips it through this format, replays it,
and asserts the final `World` state is byte-identical.
