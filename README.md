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
bitmaze new   <w> <h> <out.bm>    generate a sample walls-only level
bitmaze dump  <level.bm>          ASCII-art every plane + trigger map (the debugger)
bitmaze check <level.bm>          validate header + all invariants, exit nonzero on bad
bitmaze play  <level.bm>          run the game               (not yet implemented)
bitmaze asm   <in.asm> <out.bin>  assemble a script          (not yet implemented)
```

Example:

```sh
cargo run -- dump levels/first.bm
cargo run -- check levels/first.bm
cargo run -- new 16 10 levels/mine.bm
```

## Status

Phases 0 and 1 are complete: the versioned `.bm` format, its loader/validator,
and the `dump` / `check` / `new` tools. The terminal renderer, `minifb` window,
BitVM, and assembler are still ahead — see `ROADMAP.md`.
