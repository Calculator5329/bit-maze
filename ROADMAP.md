# bit-maze — Roadmap & Spec

A tile game where **the game world *is* binary**. Maps, entities, and game logic are
packed bit-for-bit into files you can edit in a hex editor. The engine is a small,
optimized Rust host running a custom stack-based bytecode VM ("BitVM"). The theme
("everything is 1s and 0s") is the architecture, not decoration.

Runs fully offline on Linux. No network at runtime, ever.

> **Status: COMPLETE.** All phases 0–7 are done — the project is feature-complete
> (96 tests green, `clippy` clean, zero non-`minifb` dependencies). This document
> is the original plan; per-phase completion notes live in `docs/PROGRESS.md`.
>
> **Phase 8 (post-roadmap expansion) — the gameplay loop.** An *additive*
> expansion beyond the original plan (phases 0–7 remain COMPLETE and unchanged):
> a **hazards plane** (plane 2) + lose condition, a **win** condition (collect all
> items), a **one-shot trigger latch** (high-bit script ids fire once; a runtime
> `fired` bitset, no `.bm` change), a new **`GET_HAZARD` (0x44)** opcode, a
> `GameState { Playing, Won, Lost }` on `World`, the winnable `levels/trial.bm`,
> and a full docs pass. Every addition upheld the non-negotiable rules —
> determinism, `.bm` still v1, VM caps intact, assembler still ≤300 lines (226).
> Proof again that "add a plane / add an opcode" is additive, never a rewrite.
> See `docs/PROGRESS.md` "Phase 8".

---

## Non-negotiable design rules

These are hard constraints. Every phase must uphold them. If a phase can't, stop and flag it.

1. **Versioned format from day one.** Every binary file starts with a magic number and a
   version byte. We *will* change the format; one byte now avoids migration pain later.
2. **Little-endian, always.** All multi-byte integers are little-endian. The spec says so,
   and the loader's magic check must reject byte-swapped files with a clear error.
3. **Bounded dimensions.** Map width/height are `u8` (1..=255). 255×255 is enormous for
   this genre; a bitplane at that size is ~8 KB, not 536 MB. No 16-bit dimension footguns.
4. **Determinism is law (VM).** No floats. No host randomness. No wall-clock time. Any
   randomness comes from a seeded PRNG opcode (xorshift). This buys replays, testable
   levels, and future netplay for free.
5. **The VM is hard-capped.** Stack depth, instructions-per-tick, and memory size are all
   bounded. A malicious or buggy script *cannot* hang the game — worst case it halts and
   nothing happens. This is what makes "mods are just files" safe: any random bytes are a
   valid (if inert) script.
6. **The assembler stays tiny (~300 lines).** When scripts get ugly, the fix is a **new
   opcode**, never a new language feature. No macros, no includes, no expression parser,
   no preprocessor. The tiny-language dream stays tiny by discipline.
7. **`dump` and `check` exist from v0.** They are the project's debugger and are built in
   Phase 1, not "later."

---

## File format v1 — `.bm` level files

All integers little-endian. Bit order within a plane byte is **MSB-first** (bit 7 = leftmost
tile in the group of 8), row-major.

### Header — 8 bytes, fixed

| Offset | Size | Field      | Notes                                             |
|-------:|-----:|------------|---------------------------------------------------|
| 0      | 2    | `magic`    | `0x42 0x4D` = ASCII `"BM"`. Byte-swap → reject.    |
| 2      | 1    | `version`  | `1`. Loader refuses unknown versions loudly.      |
| 3      | 1    | `flags`    | bit0 = has trigger plane; bit1 = has script table; rest reserved 0. |
| 4      | 1    | `width`    | tiles, 1..=255.                                   |
| 5      | 1    | `height`   | tiles, 1..=255.                                   |
| 6      | 1    | `planes`   | number of bitplanes that follow (≥1).             |
| 7      | 1    | `reserved` | must be 0 in v1.                                  |

### Body

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

### Trigger binding decision (consciously breaking "1 bit/tile")

A bitplane can only say *"something is here"* — 1 bit cannot say *which* script. Rather than
hack around this, we **embrace a byte-per-tile trigger plane**: each tile gets a full `u8`
script id (0 = none, 1..=255 index the script table). This is a deliberate, documented
departure from pure 1-bit-per-tile, chosen because:

- It stays flat, contiguous, and hex-editable (tile (x,y) → one byte you can eyeball).
- It's trivially expandable to 255 distinct trigger scripts per level.
- The alternative (a sparse coord→offset sidecar table) is more compact but far harder to
  hand-edit, which defeats the whole point.

The wall/item/etc. planes stay pure 1-bit. Only triggers pay the byte-per-tile cost, and
only when `flags.bit0` is set (levels with no triggers pay nothing).

### The `xxd` contract

A hand-authored 8×8 walls-only level is exactly `8 + 8 = 16` bytes and looks like:

```
42 4d 01 00 08 08 01 00   BM, v1, flags=0, 8x8, 1 plane
ff 81 bd a5 af 81 fb ff   the maze, one byte per row (MSB=leftmost tile)
```

`bitmaze dump` renders that as ASCII `#`/`.`; `bitmaze check` validates every invariant.

---

## BitVM — the logic VM

A stack machine executed per-trigger. When the player enters a tile whose trigger byte is
nonzero, the corresponding script runs to `HALT` (or until a cap trips).

### Machine model

- **Stack:** `u16` values, max depth **64** (push past 64 → halt with `StackOverflow`).
- **Memory:** fixed **256 bytes** of scratch RAM, addressable 0..=255. No heap.
- **PRNG:** one xorshift32 register, seeded from the level + a run seed. `RAND` opcode only.
- **Budget:** max **4096 instructions per tick** (exceed → halt with `Budget`). No `Date`,
  no float, no I/O beyond the documented world-mutation opcodes.

### Opcode set — v0 (grow by adding, never by breaking)

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

Opcodes are grouped by category (0x0_ control, 0x1_ stack, 0x2_ arith, 0x3_ world,
0x4_ query, 0x5_ rng, 0x6_ flow) so there's room to grow each without renumbering.

Unknown opcode → halt with `BadOpcode` (never crash). This is rule #5 in practice.

---

## Assembler — the "custom language" (Phase 5, ~300 line cap)

Human-readable text → BitVM bytecode. Deliberately minimal: labels for jumps, one
instruction per line, `;` comments, symbolic constants for opcodes and plane names. That's it.

```asm
; pressure plate: step here -> open the door at (5,3)
on_enter:
    push 5
    push 3
    clr_wall        ; clear walls bit -> door opens
    halt
```

Assembles to `10 05 10 03 32 01`. If a script wants something the assembler can't express,
we add an **opcode**, not a language feature. The assembler must never exceed ~300 lines;
CI-style check counts lines and fails the build if it does.

---

## CLI surface

```
bitmaze play  <level.bm>          run the game (minifb window)
bitmaze dump  <level.bm>          ASCII-art every plane + trigger map (the debugger)
bitmaze check <level.bm>          validate header + all invariants, exit nonzero on bad
bitmaze asm   <in.asm> <out.bin>  assemble a script  (Phase 5)
bitmaze new   <w> <h> <out.bm>    generate a blank/sample level  (helper)
```

---

## Renderer plan

- **Phase 2 (first):** a *hardcoded terminal renderer* + WASD movement. No windowing. This
  exists to make the world testable before any blitting complexity. Deterministic, snapshot-testable.
- **Phase 3:** `minifb` — a raw pixel framebuffer you blit into directly (maximally on-theme:
  you're literally writing pixels). Grid render of the walls plane, player as `@`.
- **Phase 6:** **bitplane-native 1-bit sprites** with a palette lookup — sprites are also
  hex-editable binary files. This keeps the aesthetic total and is *easier* than PNG import.
  The `image` crate / PNG path is a strictly-later, format-preserving add-on, never required.

Dependencies stay lean: `minifb` for the window/framebuffer; everything else std. `image`
is opt-in and late.

---

## Phases (each executed by a background subagent; I orchestrate + verify)

Ordering deliberately front-loads the format and tooling, and resists the temptation to start
with the fun VM — a VM with no world to act on can't be tested meaningfully.

- **Phase 0 — Scaffold.** `cargo init`, module layout, `Cargo.toml` (edition, minimal deps),
  README, this roadmap, a `docs/FORMAT.md` + `docs/VM.md` extracted from here.
  *Done when:* `cargo build` succeeds with an empty CLI dispatch.

- **Phase 1 — Format + loader + `dump` + `check`.** Header parse/write, bitplane read/write
  (MSB-first, LE), the byte trigger plane + script table (parse only for now), `dump` ASCII
  render, `check` validator, `new` level generator. Handwritten sample `levels/first.bm`.
  Unit tests for round-trip, bit ordering, byte-swap rejection, bounds.
  *Done when:* `bitmaze dump levels/first.bm` prints the maze; `check` passes it and rejects a
  corrupted copy; tests green.

- **Phase 2 — Terminal render + movement.** Load walls plane, render to terminal, WASD moves
  the player with wall collision. Deterministic; snapshot test of a scripted input sequence.
  *Done when:* a canned input string produces a stable, asserted final map/player state.

- **Phase 3 — minifb window.** Real window, framebuffer blit of the walls grid + player,
  keyboard input, clean shutdown. Reuses Phase 2's world/step logic unchanged.
  *Done when:* `bitmaze play levels/first.bm` opens a window you can walk around in.

- **Phase 4 — BitVM + triggers.** Implement the VM (all v0 opcodes), all caps (stack, budget,
  memory), determinism, seeded PRNG. Wire trigger plane → script table → run on tile enter.
  Prove it with a hand-assembled "door" script.
  *Done when:* stepping on a pressure-plate tile opens a wall elsewhere; cap tests show an
  infinite-loop script halts cleanly without hanging.

- **Phase 5 — Assembler.** Text → bytecode, labels + comments + symbolic opcodes, ≤300 lines,
  `bitmaze asm`. Reassemble the door script from source and confirm byte-identical output.
  *Done when:* `asm` reproduces the Phase 4 door bytecode; line-count guard passes.

- **Phase 6 — 1-bit sprites + palette.** Bitplane-native sprite format (hex-editable), palette
  lookup, render player/tiles from sprite files instead of hardcoded pixels.
  *Done when:* the player and walls render from sprite files you can edit in `xxd`.

- **Phase 7 — Expansion & polish.** ✅ **DONE.** Items plane (bitplane 1, no format
  bump), four added opcodes (`GET_ITEM 0x42`, `SCORE 0x43`, `LOAD 0x70`, `STORE
  0x71` — none renumbered), deterministic **replay files** (`.rec`, 2 bits/move),
  PPM screenshot export (`bitmaze shot`), richer sample levels
  (`levels/garden.bm`, `levels/vault.bm`) with assembler-built triggers, and a
  full docs pass. The "very expandable" promise exercised: every addition was
  additive. Assembler still ≤300 lines (225). See `docs/PROGRESS.md`.

Each phase ends with: tests green, `cargo build` + `cargo clippy` clean, a one-paragraph note
in `docs/PROGRESS.md`, and a git commit.

---

## Risks & how we hold the line

- **Assembler creep → real compiler.** Mitigation: the ≤300-line guard + "add an opcode, not a
  feature" rule, both enforced mechanically.
- **Format churn.** Mitigation: version byte + `check` invariants; any format change bumps the
  version and updates `docs/FORMAT.md` in the same commit.
- **VM as a hang/crash vector.** Mitigation: the caps in rule #5, with explicit cap tests.
- **Renderer complexity leaking into the format.** Mitigation: graphics only ever *read* planes;
  they never define them.
