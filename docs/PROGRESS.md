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
