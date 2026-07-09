# BitVM — the logic VM

Extracted from `ROADMAP.md`. **Not implemented yet** — this is the spec for the
Phase 4 VM. Phase 1 only stores script bytes faithfully; it never executes them.

A stack machine executed per-trigger. When the player enters a tile whose
trigger byte is nonzero, the corresponding script runs to `HALT` (or until a cap
trips).

## Machine model

- **Stack:** `u16` values, max depth **64** (push past 64 → halt with `StackOverflow`).
- **Memory:** fixed **256 bytes** of scratch RAM, addressable 0..=255. No heap.
- **PRNG:** one xorshift32 register, seeded from the level + a run seed. `RAND` opcode only.
- **Budget:** max **4096 instructions per tick** (exceed → halt with `Budget`). No `Date`,
  no float, no I/O beyond the documented world-mutation opcodes.

Determinism is law: no floats, no host randomness, no wall-clock time. Any
randomness comes from the seeded xorshift PRNG only.

## Opcode set — v0 (grow by adding, never by breaking)

| Byte | Mnemonic   | Stack effect            | Meaning                               |
|-----:|------------|-------------------------|---------------------------------------|
| 0x00 | `NOP`      | —                       | do nothing                            |
| 0x01 | `HALT`     | —                       | stop this script                      |
| 0x10 | `PUSH8 b`  | → v                     | push next byte as u16                 |
| 0x11 | `PUSH16 w` | → v                     | push next u16 (LE)                    |
| 0x12 | `POP`      | v →                     | discard top                           |
| 0x13 | `DUP`      | v → v v                 | duplicate top                         |
| 0x20 | `ADD`      | a b → a+b               | wrapping add                          |
| 0x21 | `SUB`      | a b → a-b               | wrapping sub                          |
| 0x30 | `GET_WALL` | x y → bit               | read walls plane at (x,y)             |
| 0x31 | `SET_WALL` | x y →                   | set walls bit (place wall)            |
| 0x32 | `CLR_WALL` | x y →                   | clear walls bit (open a door)         |
| 0x40 | `PLAYER_X` | → x                     | push player x                         |
| 0x41 | `PLAYER_Y` | → y                     | push player y                         |
| 0x50 | `RAND`     | → r                     | push next xorshift32 value (low 16b)  |
| 0x60 | `JMP o`    | —                       | relative jump (i8 offset)             |
| 0x61 | `JZ o`     | v →                     | pop; if 0, relative jump (i8)         |

Opcodes are grouped by category (0x0_ control, 0x1_ stack, 0x2_ arith, 0x3_
world, 0x4_ query, 0x5_ rng, 0x6_ flow) so there's room to grow each without
renumbering.

Unknown opcode → halt with `BadOpcode` (never crash). Any random bytes are a
valid (if inert) script — this is what makes "mods are just files" safe.
