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
| 0x42 | `GET_ITEM` | x y → bit               | read items plane at (x,y) *(Phase 7)* |
| 0x43 | `SCORE`    | → n                     | push collected-item count *(Phase 7)* |
| 0x50 | `RAND`     | → r                     | push next xorshift32 value (low 16b)  |
| 0x60 | `JMP o`    | —                       | relative jump (i8 offset)             |
| 0x61 | `JZ o`     | v →                     | pop; if 0, relative jump (i8)         |
| 0x70 | `LOAD`     | addr → value            | push scratch RAM[addr & 0xFF] *(Ph.7)*|
| 0x71 | `STORE`    | value addr →            | RAM[addr & 0xFF] = value & 0xFF *(Ph.7)*|

Opcodes are grouped by category (0x0_ control, 0x1_ stack, 0x2_ arith, 0x3_
world, 0x4_ query, 0x5_ rng, 0x6_ flow, **0x7_ memory**) so there's room to grow
each without renumbering. The four Phase 7 opcodes were *added* at previously
unused bytes — no existing opcode was renumbered (design rule #6 in action:
grow by adding, never by breaking).

### Phase 7 opcode notes

- **`GET_ITEM` (0x42)** mirrors `GET_WALL`: pops `x y` (y on top) and pushes the
  items-plane bit at `(x,y)` as `0`/`1`. Out of bounds, or a level with no items
  plane, reads `0` — never a panic (bounds-checked in the `VmHost` impl).
- **`SCORE` (0x43)** pushes the world's live collected-item count, saturated into
  the `u16` cell.
- **`LOAD` (0x70) / `STORE` (0x71)** are the first opcodes to use the 256-byte
  scratch RAM. The address is masked with `& 0xFF`, so it always lands in
  `0..=255` — a script can never index RAM out of bounds. `STORE` writes the low
  byte of the value; `LOAD` zero-extends the byte back into a cell. Unwritten RAM
  reads `0`.

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
- **RAM.** The fixed 256-byte scratch buffer is read/written by `LOAD` (0x70) and
  `STORE` (0x71), added in Phase 7 by *adding* opcodes, not breaking the v0 set.
  Addresses mask with `& 0xFF` so RAM access is always in bounds.

## Trigger firing semantics (v0)

- A trigger fires **after** a successful move resolves, on the tile just entered
  (`World::step_triggered`). Blocked/idle steps fire nothing.
- **Stateless**: re-entering the same plate fires the script again every time.
  There is no one-shot latch in v0. Idempotent scripts (like the door, which
  clears an already-clear wall harmlessly) are naturally safe to re-fire.
- A zero trigger byte, or an id with no matching script-table entry, fires
  nothing. A malformed/looping script halts via a cap and the game continues.
