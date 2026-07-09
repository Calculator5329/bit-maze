# BitVM — the logic VM

Extracted from `ROADMAP.md`. **Implemented in Phase 4** (`src/vm.rs`). Phase 1
only stores script bytes faithfully; Phase 4 executes them on tile-enter.

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

## Phase 4 implementation decisions

Details fixed during implementation (all consistent with the spec above):

- **Halt reasons** (`vm::Halt`, never a panic): `Halt` (hit `HALT`),
  `EndOfScript` (pc ran off the end without `HALT` — clean), `Truncated` (an
  operand was cut off at end of script), `BadJump` (JMP/JZ target outside the
  script), `BadOpcode(u8)`, `StackOverflow`, `StackUnderflow`, `Budget`.
  `Halt::is_clean()` is true only for `Halt`/`EndOfScript`.
- **JMP/JZ offset base.** The `i8` offset is relative to the byte **after the
  full instruction** (opcode + operand) — the address of the next instruction,
  the usual relative-jump convention. So `JMP -2` (`60 FE`) targets itself (the
  canonical self-loop), which the budget cap then stops. A target outside
  `0..=len` halts with `BadJump`; landing exactly on `len` is a clean
  end-of-script on the next tick.
- **Budget accounting.** One budget unit per executed instruction; the run stops
  the moment it would execute a 4097th. A tight `JMP -2` loop therefore halts
  after exactly 4096 instructions, near-instantly (proven by a test).
- **Stack ordering.** Binary/world ops pop right-to-left: for `x y` on the stack
  (`y` on top), `pop()` yields `y` then `x`. `SUB` is `a - b`. The door script
  `push 4; push 3; clr_wall` clears wall `(4,3)`.
- **World seam.** The VM talks to the world only through the `vm::VmHost` trait
  (`get_wall`/`set_wall`/`player_x`/`player_y`), implemented for `World`. That
  impl is the single bounds-checked chokepoint: out-of-range `GET_WALL` reads as
  `false`; out-of-range `SET_WALL`/`CLR_WALL` is a no-op. Coordinates are `u16`.
- **Determinism / seeding.** `Vm::new(seed)` seeds the xorshift32 register (a `0`
  seed is remapped to a fixed non-zero constant). `World` derives a run seed
  (FNV-1a over the level bytes) at construction; each trigger run mixes that with
  the plate `(x,y)` so different plates get different `RAND` streams and the same
  plate replays identically. `RAND` pushes the low 16 bits of the next state.
- **RAM.** The fixed 256-byte scratch buffer exists per the machine model, but no
  v0 opcode reads or writes it yet — that arrives with a future load/store
  opcode (Phase 7), by *adding* opcodes, not breaking these.

## Trigger firing semantics (v0)

- A trigger fires **after** a successful move resolves, on the tile just entered
  (`World::step_triggered`). Blocked/idle steps fire nothing.
- **Stateless**: re-entering the same plate fires the script again every time.
  There is no one-shot latch in v0. Idempotent scripts (like the door, which
  clears an already-clear wall harmlessly) are naturally safe to re-fire.
- A zero trigger byte, or an id with no matching script-table entry, fires
  nothing. A malformed/looping script halts via a cap and the game continues.
