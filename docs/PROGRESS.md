# Progress log

## Phase 0 — Scaffold

Initialized the `bitmaze` binary crate (edition 2021, zero dependencies — pure
`std`). Split the crate into a small library (`src/lib.rs` + `format`, `dump`,
`check`, `newlevel` modules) with a thin CLI binary (`src/main.rs`) over it, so
the game and the tests exercise the same code. Extracted the format spec into
`docs/FORMAT.md` and the VM spec into `docs/VM.md`, and wrote the top-level
`README.md`. CLI dispatch parses all five subcommands (`play dump check asm
new`); `play` and `asm` print "not yet implemented" placeholders for now.
`cargo build` succeeds.

## Phase 1 — Format + loader + dump + check + new

Implemented the v1 `.bm` format exactly per `docs/FORMAT.md`: 8-byte header,
MSB-first row-major bitplanes (`ceil(w*h/8)` bytes each), the optional
byte-per-tile trigger plane (flags bit0) and the optional script table (flags
bit1). Parse and write round-trip faithfully; scripts are stored but not
executed (that is Phase 4). The loader enforces every invariant: magic check
(byte-swapped `MB` rejected with an explicit message), unknown-version
rejection, `1..=255` dimensions, `planes ≥ 1`, `reserved == 0`, and an exact
total-length check (both truncated and trailing-byte files are rejected).

CLI commands: `new <w> <h> <out.bm>` generates a bordered walls-only maze;
`dump <level.bm>` renders every plane as `#`/`.` ASCII plus a hex trigger grid
and script listing; `check <level.bm>` prints an OK summary and exits 0, or a
specific `FAIL:` message and exits nonzero. Hand-authored `levels/first.bm`
matches the ROADMAP `xxd` contract (16 bytes) and renders as a recognizable
maze. Integration tests cover header round-trip, MSB-first bit ordering,
byte-swapped-magic rejection, unknown-version rejection, out-of-bounds
dimensions, wrong-length (short and trailing), and a trigger-plane + script-table
round-trip. `cargo build`, `cargo clippy` (clean), and `cargo test` all pass.

## Phase 2 — Terminal render + movement

Added a pure, headless game core so world logic is fully testable before any
windowing. `src/world.rs` holds the `World` (a loaded `Level` + player `px,py`),
built via `World::new`, which spawns the player on the first floor tile
(walls-plane bit == 0) scanning row-major, and fails cleanly with
`SpawnError::AllWalls` on an all-walls level. `World::step(Move) -> StepResult`
is pure — no I/O, no time, no randomness: `Up/Down/Left/Right` move one tile,
moving into a wall or off the map edge is a no-op (`StepResult::Blocked`), and
`Move::None` is `Idle`. `World::render()` draws the walls plane as `#`/`.`
(reusing the `dump` convention) with the player as `@`, one row per line.

The terminal front-end lives separately in `src/play.rs` (the *only* I/O layer):
`play::run(&mut world, reader, writer)` is generic over reader/writer, reading
`w/a/s/d` movement keys (case-insensitive) and `q` to quit, line-buffered with
std only — no raw-mode dependency (type a key, press Enter; multiple keys per
line and piped input both work). `bitmaze play <level.bm>` wires real
stdin/stdout into it. This boundary is deliberate: Phase 3 reuses `world`
unchanged and only replaces `play` with a minifb window.

Integration tests (`tests/world.rs`): spawn position on `first.bm` is (1,1);
all-walls level errors; a scripted `"ddss"` sequence ends at (3,1) with a full
`render()` snapshot asserted; moving into a wall and off the edge are both
no-ops; `Move::None` is idle; a successful move reports `Moved`. `cargo build`,
`cargo clippy --all-targets` (clean), and `cargo test` (18 tests: 11 + 7) all
pass. Verified interactively: `printf 'd\nd\ns\nq\n' | bitmaze play
levels/first.bm` walks `@` from (1,1) to (3,1), blocks on the wall below, quits.

## Phase 3 — minifb window

Added a real graphical window front-end that reuses the Phase 2 `World`/`step`
core **unchanged**. Split into two modules along a headless-testable seam:

- `src/framebuffer.rs` — a **pure** `draw(world, fb, fb_w, fb_h, tile_px)` that
  fills a flat `u32` pixel buffer (`0x00RR_GGBB`, minifb's layout) from a
  `World`: walls, floor, and the player's tile each get a solid color, one
  `tile_px`×`tile_px` block per tile (clipped, so a short buffer can't panic).
  It does no windowing and no I/O, so it is fully unit-testable without a
  display. `fb_width`/`fb_height` derive the window size from `level dims *
  tile_px`. In Phase 6, this per-tile solid-color fill is the single spot that
  1-bit sprite blits replace — the signature and the window shell stay put.
- `src/window.rs` — the thin minifb shell (only reached by the `play` command,
  never by tests). It opens a `Window`, maps key **presses** (edge-triggered,
  `KeyRepeat::No`) to `Move` via a pure `key_to_move` — **both** WASD and the
  arrow keys move, `Esc`/`Q` or closing the window quits — calls `world.step`,
  `framebuffer::draw`, and `update_with_buffer` each frame at a 60-FPS cap. If
  `Window::new` fails (e.g. no display) it returns `Err` with a clear message
  and the caller exits nonzero — it never hangs.

`minifb 0.28` is the only new dependency (`cargo add minifb`; builds fine). This
environment is headless, so the window can't actually be opened here; the
terminal loop is kept reachable via a new `--term` flag on `play`
(`bitmaze play --term <level.bm>`) for headless verification and CI. `play`
without `--term` is the graphical path.

Tests: `framebuffer` unit tests build a 3×2 world (top row walls, bottom row
floor, player spawns at (0,1)) and assert wall/floor/player pixel colors at tile
centers, full buffer coverage (poison-fill check), and that buffer dims are
`tiles * tile_px`; `window` unit tests assert the WASD+arrow `key_to_move`
mapping (no `Window` is ever created in tests). All 23 tests green (18 prior + 5
new), `cargo build` and `cargo clippy --all-targets` clean. Verified headless:
`bitmaze play levels/first.bm` with no display prints `cannot open a window …
Run headless with bitmaze play --term …` and exits 1 (no hang); `printf
'd\nd\nq\n' | bitmaze play --term levels/first.bm` still walks `@` to (3,1) and
quits.

## Phase 4 — BitVM + triggers

Implemented the BitVM stack machine (`src/vm.rs`) exactly per the ROADMAP opcode
table and machine model, and wired the trigger plane → script table → run on
tile-enter. The VM is a `u16` stack (max depth 64), 256 fixed bytes of scratch
RAM, a seeded xorshift32 PRNG register (the only randomness source), and a 4096
instruction/run budget. All caps are enforced without ever panicking: pushing
past 64 → `StackOverflow`; 4096 instructions → `Budget` (the anti-hang guard — a
tight `JMP -2` self-loop halts near-instantly); an unknown opcode → `BadOpcode`;
an op needing missing operands → `StackUnderflow`; a `PUSH8/16/JMP/JZ` operand
cut off at end of script → `Truncated`; a JMP/JZ target outside the script →
`BadJump`. All 15 v0 opcodes are implemented at their exact bytes (0x00 NOP …
0x61 JZ). JMP/JZ `i8` offsets are relative to the *next* instruction (so `60 FE`
is a self-loop). Determinism is law: no floats, no host time, no host RNG.

The VM reaches the world only through a small `VmHost` trait
(`get_wall`/`set_wall`/`player_x`/`player_y`), implemented for `World` — the
single bounds-checked chokepoint that makes out-of-range wall ops safe no-ops.
This keeps the VM unit-testable against a trivial mock host (no window, no
level). `World` gained a `seed` field (FNV-1a over the level bytes) and a
`step_triggered(Move) -> StepOutcome` method: it runs the pure `step` for
movement, then on a successful `Moved` looks up `triggers[y*w+x]`, finds the
matching `Script`, and runs it on a fresh VM seeded from the world seed mixed
with the plate coords. `StepOutcome { result, trigger: Option<TriggerRun> }`
surfaces the fired trigger (id, tile, halt reason) to both front-ends, which now
call `step_triggered` so a trigger-driven wall change is visible next frame
(render/draw read live plane data). Firing is after-move, stateless, and
documented (re-entering re-fires; the door script is idempotent).

Demo: `levels/door.bm` (90 bytes, hand-authored) is an 8×8 room split by a wall
divider at column 4, a pressure plate at (2,2) bound to script id 1, whose bytes
are the ROADMAP example `10 04 10 03 32 01` (`push 4; push 3; clr_wall; halt`) —
clearing the wall at (4,3) opens a door. `bitmaze check levels/door.bm` exits 0;
`dump` shows the trigger plane (a lone `01` at row 2, col 2) and the script.
`printf 'd\ns\ns\nd\nd\nd\nq\n' | bitmaze play --term levels/door.bm` walks onto
the plate — the frame prints `trigger #1 at (2,2) ran [HALT]` and row y=3 flips
from `#...#..#` to `#......#` (door open) — then walks `@` through (4,3) into the
right room at (5,3).

Tests: 26 new (21 VM unit + 5 trigger integration), all 23 prior still green (49
total). VM unit tests cover every opcode (push/pop/dup/add/sub wrapping,
get/set/clr wall incl. OOB no-op, player x/y, a known-sequence RAND determinism
check, jmp/jz control flow) and every cap explicitly: `pushing_forever →
StackOverflow`, `infinite JMP -2 → Budget` (completes instantly, proving no
hang), `bad_opcode → BadOpcode`, `stack_underflow`, and `truncated_operands`.
Integration tests load `levels/door.bm`, assert the committed file byte-matches
its programmatic build, step onto the plate and assert (4,3) cleared, walk the
opened door end-to-end, and confirm blocked/idle steps fire nothing.
`cargo build`, `cargo clippy --all-targets` clean, `cargo test` all green.

## Phase 5 — Assembler

Added the text→bytecode assembler (`src/asm.rs`, **221 non-test lines**, under
the rule-#6 ≤300 cap) and wired `bitmaze asm <in.asm> <out.bin>`, which writes
the raw script bytes (not a `.bm` level). It is a dead-simple, line-oriented,
two-pass assembler and stays that way on purpose: one instruction per line, `;`
comments to end of line, blank lines ignored, case-insensitive mnemonics, and
`name:` labels on their own line. No macros, includes, expression evaluator,
preprocessor, or nested scopes — when a script needs more, the answer is a new
opcode in `src/vm.rs`, never a language feature here (see `docs/ASM.md`).

Language surface: every operand-less opcode by mnemonic (`nop halt pop dup add
sub get_wall set_wall clr_wall player_x player_y rand`) → its exact VM byte;
`push N` auto-picks PUSH8 (`0x10`) for `N ≤ 255` else PUSH16 (`0x11`) + 2 LE
bytes, with explicit `push8`/`push16` to force a width; numbers are decimal or
`0x`-hex, bounded to `u16`. `jmp label`/`jz label` emit `0x60`/`0x61` + an `i8`
relative offset computed against the byte *after* the full instruction — the
same base `src/vm.rs` uses — so assembled jumps execute correctly (pass 1 lays
out sizes and records label offsets, pass 2 resolves). Every error path
(unknown mnemonic, undefined/duplicate label, offset out of i8 range, bad/oob
operand, stray operand) returns an `AsmError` carrying the 1-based source line;
the assembler never panics.

Proof: `scripts/door.asm` (the ROADMAP door example) assembles to exactly
`10 04 10 03 32 01`, byte-identical to `levels/door.bm` script id 1.
`scripts/countdown.asm` exercises a label with both a forward `jz` and a
backward `jmp`, assembling to `10 03 10 01 21 13 61 02 60 f8 01` (jz done = +2,
jmp top = -8/0xF8) — a test runs it through the VM and confirms it halts cleanly
via `HALT`, and an infinite `jmp self` halts via the budget cap (no hang). A
mechanical guard test (`assembler_stays_under_300_lines`) counts the lines of
`src/asm.rs` before the `#[cfg(test)]` marker and fails if it exceeds 300;
current count is 221. 17 new tests (door round-trip, every mnemonic, push
sizing, hex/decimal, forward/backward jumps, VM execution, all error cases, the
line guard); all 49 prior tests still green (66 total). `cargo build`,
`cargo clippy --all-targets` clean.
