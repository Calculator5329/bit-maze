# bit-maze

A tile game where **the game world *is* binary**. Maps, entities, and game logic
are packed bit-for-bit into files you can edit in a hex editor. The engine is a
small Rust host running a custom stack-based bytecode VM ("BitVM"). The theme —
everything is 1s and 0s — is the architecture, not decoration.

Runs fully offline on Linux. No network at runtime, ever.

A hand-authored 8×8 level is exactly 16 bytes:

```
42 4d 01 00 08 08 01 00   BM, v1, flags=0, 8x8, 1 plane
ff 81 bd a5 af 81 fb ff   the maze, one byte per row (MSB = leftmost tile)
```

See [`docs/FORMAT.md`](docs/FORMAT.md) for the `.bm` file format,
[`docs/VM.md`](docs/VM.md) for the BitVM spec, and [`ROADMAP.md`](ROADMAP.md) for
the full plan and design rules.

## Build

```sh
cargo build --release
```

## Run

```
bitmaze new   <w> <h> <out.bm>       generate a sample walls-only level
bitmaze dump  <level.bm>             ASCII-art every plane + trigger map (the debugger)
bitmaze check <level.bm>             validate header + all invariants, exit nonzero on bad
bitmaze play  [--term] <level.bm>    play the game (window, or --term for terminal)
bitmaze asm   <in.asm> <out.bin>     assemble a script          (not yet implemented)
```

Example:

```sh
cargo run -- dump levels/first.bm
cargo run -- check levels/first.bm
cargo run -- new 16 10 levels/mine.bm
```

### Playing

`bitmaze play <level.bm>` opens a **graphical window** (via `minifb`): walls,
floor, and the player render as colored tiles. Move with **W/A/S/D or the arrow
keys**; press **Esc/Q** (or close the window) to quit. This needs a display.

On a headless machine (no X11/Wayland), the window can't open — `play` prints a
clear error and exits nonzero rather than hang. Use the terminal fallback there:

```sh
bitmaze play --term levels/first.bm      # line-buffered w/a/s/d + q, works over pipes
printf 'd\nd\nq\n' | bitmaze play --term levels/first.bm
```

Both paths drive the exact same world/step logic; only the I/O shell differs.

## Status

Phases 0–3 are complete: the versioned `.bm` format with its loader/validator
and the `dump` / `check` / `new` tools; a pure headless world core with terminal
render + movement; and the `minifb` graphical window. The BitVM, assembler, and
1-bit sprites are still ahead — see `ROADMAP.md`.
