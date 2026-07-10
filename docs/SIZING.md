# Size and performance notes

Measurements below were taken from the 2026-07-09 feedback build on an Intel
Core Ultra 7 265KF, Node 24.18.0, and Rust 1.96.1. Microbenchmarks are directional,
not cross-machine guarantees.

## Level storage

For `N = width * height`, `P` bitplanes, an optional trigger plane, and scripts:

```text
8-byte header
+ P * ceil(N / 8)                     bitplanes
+ N                                    trigger bytes, when enabled
+ 1 + sum(3 + script_byte_length)      script table, when enabled
```

| Level | Tiles | Planes | Triggers | Script bytes | File size |
|---|---:|---:|---:|---:|---:|
| `first.bm` | 64 | 1 | none | 0 | 16 B |
| `trial.bm` | 48 | 3 | 48 B | 6 | 84 B |
| `circuit.bm` | 384 | 3 | 384 B | 12 | 555 B |

The circuit's three gameplay bitplanes occupy only 144 bytes. Its flat trigger
map occupies 384 bytes even though only two tiles use it. This is intentional:
one byte per tile makes `(x,y) -> script id` direct and hex-editable. A sparse
trigger list would save space on quiet maps, but would be harder to edit and
would need searching or an index at runtime.

The first size column is header plus three bitplanes only. The triggered column
adds a flat trigger map and two six-byte scripts:

| Dimensions | Tiles | No triggers | Flat trigger map |
|---|---:|---:|---:|
| 24x16 | 384 | 152 B | 555 B |
| 64x64 | 4,096 | 1,544 B | 5,659 B |
| 128x128 | 16,384 | 6,152 B | 22,555 B |
| 255x255 (v1 maximum) | 65,025 | 24,395 B | 89,439 B |

Those figures exclude unusually large script bodies. Each script length is a
`u16`, so deliberately maxing all 255 scripts could dwarf the map data; normal
gate scripts are six bytes.

## Game and web build size

- Native optimized Linux executable: 1,581,288 B (1.51 MiB).
- Same executable after symbol stripping: 1,127,352 B (1.08 MiB).
- Five committed 8x8 sprite files: 65 B total (13 B each).
- Static web artifact on disk: 814,121 B. Most is the Next/React runtime.
- Browser's reported first-load JavaScript: about 106 kB compressed.
- Game-specific page chunk: 10.4 kB raw / about 4.0 kB gzip.
- CSS: 6.6 kB raw / about 2.2 kB gzip.
- HTML, including both serialized level payloads: 8.5 kB raw / about 2.9 kB gzip.

The binary level data is negligible beside either runtime. The Rust executable
and React framework dominate distribution size; maps and sprites do not.

## Runtime scaling

World initialization scans the map to count items and find the spawn, so it is
`O(N)`. A normal move is `O(1)`: a few bounds checks and bit reads/writes. A
trigger additionally runs its script, hard-capped at 4,096 instructions.

Node microbenchmark for the 24x16 browser engine, excluding rendering:

- Parse: roughly 7.5 million level parses/second.
- Initialize: roughly 5.5 microseconds per world.
- Movement: roughly 70 million simple move attempts/second.

These numbers mainly demonstrate headroom; human keyboard input is normally
below a few dozen moves per second.

Rendering grows with the visible area. At the native renderer's 24-pixel tiles,
the circuit framebuffer is 576x384x4 = 884,736 bytes. The adaptive browser
canvas uses 768x512x4 = 1,572,864 bytes. The browser currently redraws the whole
map after each move, about 24,576 base sprite-pixel operations for the circuit,
plus sparse overlays.

Full-map rendering is comfortable at the current scale. For maps much larger
than roughly 64x64, the next architectural step should be a camera/viewport and
cached tile images. At the v1 maximum, world data remains under 90 kB for a
typical three-plane triggered level, but drawing every tile at once would waste
tens or hundreds of megabytes of framebuffer memory.
