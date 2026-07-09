# The bit-maze assembler

Text → BitVM bytecode. Implemented in `src/asm.rs` (Phase 5). Invoked with:

```
bitmaze asm <in.asm> <out.bin>
```

It writes **raw script bytes** (not a `.bm` level) — the same bytes that go in a
level's script table. Opcode bytes mirror `src/vm.rs` / `docs/VM.md` exactly.

## Design rule #6 — the language stays tiny

The assembler is capped at **≤300 non-test lines**, enforced by the
`assembler_stays_under_300_lines` guard test (it counts lines of `src/asm.rs`
before the `#[cfg(test)]` marker and fails the build over 300). This is a hard
discipline: **no macros, includes, expression evaluator, preprocessor, or nested
scopes.** When a script wants something the assembler can't express, the fix is
to *add an opcode* in `src/vm.rs`, never a language feature here.

## Syntax

- **One instruction per line.** Blank lines are ignored.
- **Comments:** `;` starts a comment to end of line.
- **Mnemonics** are case-insensitive.
- **Labels:** a line that is just `name:` defines a label at the current byte
  offset. Labels live on their own line (not inline with an instruction).
  Identifiers are ASCII `[A-Za-z_][A-Za-z0-9_]*`.
- **Numbers:** decimal (`42`) or hex (`0x2a`), bounded to `u16` (0..=65535).

## Instructions

| Form | Emits | Notes |
|------|-------|-------|
| `nop halt pop dup add sub get_wall set_wall clr_wall player_x player_y rand` | one opcode byte | operand-less |
| `push N` | `0x10 N` if `N ≤ 255`, else `0x11 lo hi` | auto-picks PUSH8/PUSH16 (LE) |
| `push8 N` | `0x10 N` | forces 8-bit; errors if `N > 255` |
| `push16 N` | `0x11 lo hi` | forces 16-bit little-endian |
| `jmp label` | `0x60 off` | `off` = `i8` relative to the next instruction |
| `jz label` | `0x61 off` | pops; jumps if 0; same offset base |

Jump offsets are relative to the byte **after** the full instruction
(opcode + operand) — identical to the VM's convention — so `jmp self` (a label
on the jump itself) emits `60 FE`, the canonical self-loop.

## Errors

Every error carries the 1-based source line and the assembler never panics:
unknown mnemonic, invalid/duplicate/undefined label, jump offset out of `i8`
range, bad number, out-of-range operand, missing operand, and stray/extra
tokens.

## Examples

`scripts/door.asm` — the pressure-plate door (assembles to `10 04 10 03 32 01`):

```asm
; pressure plate: open the door at (4,3)
on_enter:
    push 4
    push 3
    clr_wall
    halt
```

`scripts/countdown.asm` — a label with forward `jz` and backward `jmp`
(assembles to `10 03 10 01 21 13 61 02 60 f8 01`):

```asm
    push 3
top:
    push 1
    sub
    dup
    jz done
    jmp top
done:
    halt
```
