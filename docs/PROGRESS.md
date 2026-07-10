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

## Phase 6 — 1-bit sprites + palette

Replaced the per-tile solid-color fill in `framebuffer::draw` with
**bitplane-native 1-bit sprite blits** through a palette, keeping the aesthetic
total: sprites are themselves hex-editable binary files, no `image` crate, pure
`std`. Added the `.spr` format (`src/sprite.rs`, `docs/SPRITE.md`), designed to
mirror `.bm`: a 5-byte header (`magic "SP"` `0x53 0x50`, `version` 1, `width`,
`height` — both `u8` 1..=255), then exactly `ceil(width*height/8)` packed pixel
bytes, **MSB-first, row-major** — the same bit convention and little-endian +
magic-check discipline as the level planes (byte-swapped `"PS"`, unknown version,
zero dimension, and wrong-length files all rejected loudly, never panicking).
Bit `1` = ink, bit `0` = paper.

The `Palette` (plain struct, `Palette::DEFAULT`) maps the two 1-bit values to
`0x00RR_GGBB` colors **per tile role**: wall ink/paper, floor ink/paper, and
player ink — the player's paper is **transparent**. `draw` now blits one sprite
per tile, scaled to `tile_px` with nearest-neighbor sampling (any sprite size at
any tile size): a wall tile blits the wall sprite, a floor tile the floor
sprite, and the player tile blits floor then composites the player sprite over
it so the floor shows through the `@`'s gaps. `draw` stays **pure and headless**
(fills a `Vec<u32>`, opens no window); the blit lives in a small `Canvas` helper.
`window::run` gained `&Sprites, &Palette` params and passes them straight through
— the window shell is otherwise unchanged, and `--term` renders ASCII as before.

`bitmaze play` loads `wall.spr`/`floor.spr`/`player.spr` from `sprites/` via
`Sprites::load_from_dir`, falling back **per-sprite** to a compiled-in default
(`Sprite::default_wall/floor/player`) on any missing/corrupt file (noted to
stderr) so the game always has a full set. New tooling: `bitmaze sprite
<file.spr>` dumps a sprite as ASCII (`#` ink / `.` paper) — the headless
verification tool, the sprite counterpart of `dump` — and `bitmaze sprite gen
<dir>` writes the three default sprites (used to generate the committed
`sprites/`). The defaults are hand-designed 8×8: a brick-with-mortar wall, a
center-dot floor, and an `@`-ish player figure.

10 new tests (sprite round-trip, MSB-first bit order, `from_rows`, byte-swapped
magic / unknown version / zero dimension / wrong length / short header
rejection, ASCII dump vs a known pattern, missing-dir fallback) plus the two
rewritten `framebuffer` tests now assert palette colors at known sprite pixels
(wall ink vs mortar paper, floor dot vs floor paper, player ink vs floor showing
through transparent player paper) and full-buffer coverage under a non-1:1
`tile_px`. All 66 prior tests still green (76 total). `cargo build`, `cargo
clippy --all-targets` clean. Verified headless: `bitmaze sprite gen sprites`
then `bitmaze sprite sprites/wall.spr` prints the brick pattern; `xxd
sprites/wall.spr` shows `53 50 01 08 08 ff ee ee 00 bb bb 00 ff`. `bitmaze play`
with no display still exits 1 with the clear "cannot open a window … use
`--term`" message (window path unchanged); `--term` still walks `@` and quits.

## Phase 7 — Expansion & polish (project feature-complete)

The final phase, which exercises the "very expandable" promise: every deliverable
was **additive**, no format renumbering or core rewrite.

**Items plane + pickup ("add a plane -> add a mechanic").** Bitplane 1 is now an
ITEMS plane (`1` = item), a pure 1-bit plane exactly like walls — so the v1 `.bm`
format needed **no change** (`planes` reads `2`; the file stays v1). `World`
gained a `score: u32` (the single source of truth) and collects on move: `step`
clears the items bit at the tile just entered and increments `score` (spawn
collects too). `World::render` draws items as `*`; the terminal front-end shows a
live score; `dump` labels plane 1 `ITEMS`. Determinism preserved (pure, no
I/O/time/RNG).

**Four new opcodes ("add an opcode, not a language feature"), at previously
unused bytes, no renumbering.** Query group: `GET_ITEM` (0x42, `x y -> bit`,
reads the items plane, OOB-safe) and `SCORE` (0x43, `-> n`). New 0x7_ memory
group: `LOAD` (0x70, `addr -> value`) and `STORE` (0x71, `value addr ->`), the
first users of the 256-byte scratch RAM — addresses mask with `& 0xFF` so RAM
access is always in bounds. The `VmHost` trait grew `get_item`/`score`; `World`
and both test hosts implement them. The assembler gained four one-line
`SIMPLE`-table entries (`get_item score load store`); the ≤300-line guard stays
green at **225 lines**. `docs/VM.md` opcode table updated.

**Deterministic replay files (`.rec`).** A new versioned binary format
(`src/replay.rs`, `docs/REPLAY.md`): magic `"BR"`, version, a level reference,
then the move stream at **2 bits per move** (`00`=Up..`11`=Right, MSB-first). The
terminal loop returns the applied-move sequence; `bitmaze play --term --record
<run.rec> <level.bm>` writes it, and `bitmaze play --replay <run.rec> <level.bm>`
re-runs it against a fresh world and prints the reproduced final state. Because
the world is deterministic, replay reproduces player position, score, and every
trigger-mutated wall exactly — a test records a garden run, round-trips it through
the format, replays it, and asserts byte-identical final `World` state.

**PPM screenshot export.** A pure `framebuffer::to_ppm` encodes the rendered
`u32` buffer as a binary **P6 PPM** (pure std, no image crate). `bitmaze shot
<level.bm> <out.ppm> [tile_px]` calls the *real* `framebuffer::draw` (sprites +
palette) into a headless buffer and writes a viewable image with no display —
visual proof the renderer works. `bitmaze shot levels/first.bm levels/first.ppm`
produces a valid `192x192` P6 (110607 bytes = 15-byte header + 192*192*3). A test
asserts the header and exact byte length.

**Richer sample content.** `scripts/gate.asm` and `scripts/vault.asm` are
assembled and embedded into `levels/garden.bm` (10x7, walls + 5 items + a plate
opening a gate) and `levels/vault.bm` (8x6, walls + items + a plate that bumps a
RAM counter via LOAD/STORE before opening a vault). `src/samples.rs` is the
documented "embed assembled bytecode into a `.bm` script table" helper the Phase
5 notes flagged as missing; `bitmaze gen-levels <dir>` writes the samples, and a
test asserts the committed files byte-match the builders and pass `check`. A
headless playthrough (`printf '...' | bitmaze play --term levels/garden.bm`) walks
`@`, collects items (score climbs), and fires the gate script.

**Docs & polish.** `README.md` rewritten into a proper front door (philosophy,
all three formats, full CLI with examples, design rules). New `docs/REPLAY.md`;
`docs/FORMAT.md` gained an items-plane section; `docs/VM.md` and `docs/ASM.md`
carry the new opcodes; this log and `ROADMAP.md` mark the project complete.

Tests: 20 new (7 VM opcode/cap tests, 6 replay-format unit tests, 1 PPM test, 2
sample unit tests, 2 replay integration + 2 sample integration) on top of the 76
prior — **96 total, all green**. `cargo build`, `cargo clippy --all-targets`
clean, no new external crates. The project is **feature-complete**.

**Post-Phase-7 polish: item sprite in the graphical renderer.** The ITEMS plane
was collectible and shown as `*` in the terminal, but graphical mode (`framebuffer`
+ `sprite` + palette) had no item sprite, so items were invisible in the minifb
window and `bitmaze shot`. Added an 8×8 `item` role (a solid cyan diamond gem,
`Sprite::default_item`, `sprites/item.spr`) with `item_ink` (`0x0000E5FF`) and
transparent paper, composited over floor exactly like the player (player still
draws on top when standing on an item). `sprite gen` writes `item.spr`;
`load_from_dir` gains per-file fallback for it. Two new tests (framebuffer item-ink
pixel assertion + item sprite round-trip) → **98 total, all green**; clippy clean.

## Phase 8 — the gameplay loop (post-roadmap expansion)

Turns the sandbox into a winnable/losable game. Every deliverable is **additive** —
no format renumbering, no `.bm` layout change (`.bm` stays **v1**), no core rewrite —
exercising the "add a plane / add an opcode" promise one more time.

**Hazards plane + lose ("add a plane").** Bitplane **2** is now a HAZARDS plane
(`1` = spike). A level that opts in has `planes = 3` and stays v1 (the multi-plane
mechanism was in the format from day one). Stepping onto a set hazard bit **loses**.
`World` gained a `GameState { Playing, Won, Lost }` field — the single source of
truth for the outcome — set purely from world logic in `World::step` (no time, no
RNG). Once not `Playing`, further moves are ignored (documented no-op). `dump`
labels plane 2 `HAZARDS`; `check` already handled N planes; the terminal renders
hazards as `^`. Graphically, a new 8×8 `hazard` sprite role (`Sprite::default_hazard`,
red spikes on a solid base, `sprites/hazard.spr`) with a transparent-paper
`hazard_ink` (`0x00FF3B30`) composites over floor exactly like the item gem;
`sprite gen` writes `hazard.spr`, `load_from_dir` gains per-file fallback for it.

**Win.** `World` counts the level's total items at construction (`total_items`); when
`score` reaches it, `GameState` becomes `Won`. A level with **zero** items has no
win-by-collection condition — endless/sandbox (so existing itemless samples never
insta-win). Both front-ends surface the outcome: the terminal prints a clear
`YOU WIN` / `GAME OVER` line and stops the loop; the window logs it to stderr and
updates the title (staying open). `--replay` prints the reproduced `state
WON`/`LOST`, reproduced deterministically.

**One-shot trigger latch (by script-id convention).** A trigger whose script id has
the **high bit set** (`0x80..=0xFF`) is *one-shot* — it fires only the first time the
player enters that tile this run; ids `1..=127` *repeat* (the pre-Phase-8 behavior).
The latch is a per-tile **runtime** `fired` bitset on `World`, **not** stored in
`.bm`, so it is deterministic and replay-safe and costs no format change. This
convention (rather than "all triggers one-shot") was chosen so the existing
`door`/`garden`/`vault` samples — all id 1 — keep firing every entry, and **no
existing trigger test needed changing**.

**New opcode `GET_HAZARD` (0x44) ("add an opcode").** Slotted into the free space in
the 0x4_ query group beside `GET_ITEM`/`SCORE`; pops `x y`, pushes the hazards-plane
bit (OOB-safe, reads `0`). The `VmHost` trait grew `get_hazard`; `World` and both
test hosts implement it. One new line in the assembler `SIMPLE` table
(`get_hazard`); the ≤300-line guard stays green at **226 lines** (was 225).
`docs/VM.md` opcode table updated.

**Winnable sample `levels/trial.bm`.** An 8×6 room (via `src/samples.rs::trial`,
script from `scripts/trial.asm`, embedded through the assembler like garden/vault):
walls + 3 items + a spike hazard at (2,3) + a one-shot plate at (3,1) (id `0x80`)
that opens a gate at (5,3) to the walled-off right column. Wired into `gen-levels`;
a test asserts the committed file byte-matches its builder and passes `check`. A
winning `--term` playthrough collects all three items and prints `YOU WIN`; a losing
one steps on the spike and prints `GAME OVER`.

**Tests & docs.** New tests: hazard-lose + win integration, one-shot vs repeating
latch, itemless-is-endless, `GET_HAZARD` opcode unit + `World`-host bounds, hazard
sprite round-trip, framebuffer hazard-pixel, trial win/lose routes, and replay
reproduces `GameState` (`tests/phase8.rs`) — plus the trial in `samples::all()` so
the existing committed-file check covers it. **111 total, all green** (98 prior +
13); `cargo build` + `cargo clippy --all-targets` clean; no new external crates.
One existing test intentionally relaxed: `samples.rs::samples_are_valid_and_reparse`
asserted exactly 2 planes → now `>= 2`, because `trial` adds the hazards plane.

## Browser feedback build

Added a responsive Next.js front end for the `trial` level so the game can be
played from a shared link as well as through the native Rust binary. The browser
engine parses the exact committed `levels/trial.bm` byte payload and mirrors the
deterministic world rules: wall collision, item collection, hazards, terminal
win/lose state, the one-shot trigger latch, and the BitVM gate script. Rendering
uses the same packed 8x8 sprite rows and palette, with a visible violet plate to
make the trigger discoverable during playtesting. Controls cover W/A/S/D, arrow
keys, and touch/click input; reset, move count, score, state, and contextual
feedback are built into the game shell.

Web verification adds a byte-for-byte assertion against the committed `.bm`
file plus complete winning and losing playthroughs. `npm run test:web`, the
optimized `npm run build`, `cargo test` (now 112 tests), `cargo clippy --all-targets
-- -D warnings`, and a headless Chromium production-page render all pass.
The production build exports static assets to `dist/` and adds the hosting
service's required Worker entrypoint plus copied project manifest.
The local development command explicitly selects Next's development environment
and retains the normal `.next/` directory; production builds alone use `dist/`
for the hosting artifact, avoiding a dev-server/output-directory collision.

## Larger-level scaling pass

Added `levels/circuit.bm`, a 24x16 three-sector level: 384 tiles, 12 items, 9
hazards, and two one-shot plates whose independently assembled six-byte BitVM
programs open successive divider gates. The complete level is 555 bytes. A
reachability test treats hazards as blocked and proves the safe progression from
spawn to plate A, through gate A to plate B, then through gate B to every item.

The browser now reads the committed `.bm` files during the static build instead
of duplicating level bytes in JavaScript, offers Trial/Circuit selection, sizes
the pixel canvas adaptively, and opens on Circuit by default. Browser tests parse
both committed files and execute both circuit gate scripts. `docs/SIZING.md`
records the storage formula, exact artifact sizes, core microbenchmarks, and the
point at which a viewport becomes preferable to full-map rendering.
Docs updated: `FORMAT.md` (hazards plane), `VM.md` (`GET_HAZARD` + one-shot
semantics), `SPRITE.md` (hazard role/palette), `README.md` (win/lose, hazards,
one-shot, the trial), this log, and a `ROADMAP.md` Phase 8 note.
