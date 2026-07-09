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
