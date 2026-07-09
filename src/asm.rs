//! The assembler — text `.asm` → BitVM bytecode (Phase 5).
//!
//! Deliberately tiny (design rule #6: **≤300 non-test lines**). It is a
//! dead-simple, line-oriented, two-pass assembler: one instruction per line,
//! `;` comments, case-insensitive mnemonics, labels for jumps. That is the
//! whole language. When a script wants something this can't express, the fix is
//! to **add an opcode** in `src/vm.rs`, never a language feature here — no
//! macros, includes, expressions, or preprocessor. A guard test counts the
//! lines of this file and fails the build if the non-test half exceeds 300.
//!
//! Opcode bytes mirror `src/vm.rs` exactly (see `docs/VM.md`). JMP/JZ offsets
//! are `i8` relative to the byte *after* the full instruction — the same base
//! the VM uses — so assembled jumps execute correctly.

use std::collections::HashMap;

/// An assembly error, always carrying the 1-based source line it occurred on so
/// messages point at the offending line. Never a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    /// 1-based line number in the source.
    pub line: usize,
    /// Human-readable description of what went wrong.
    pub msg: String,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for AsmError {}

/// An operand-less mnemonic mapped to its single opcode byte. Kept as a flat
/// table so adding an opcode is a one-line edit, matching rule #6.
const SIMPLE: &[(&str, u8)] = &[
    ("nop", 0x00),
    ("halt", 0x01),
    ("pop", 0x12),
    ("dup", 0x13),
    ("add", 0x20),
    ("sub", 0x21),
    ("get_wall", 0x30),
    ("set_wall", 0x31),
    ("clr_wall", 0x32),
    ("player_x", 0x40),
    ("player_y", 0x41),
    ("get_item", 0x42),
    ("score", 0x43),
    ("rand", 0x50),
    ("load", 0x70),
    ("store", 0x71),
];

/// One assembled item, pending final layout. Everything except jumps is already
/// fully byte-resolved in pass 1; jumps carry the label to resolve in pass 2.
enum Item {
    /// Fully resolved opcode (+ its operand bytes).
    Bytes(Vec<u8>),
    /// A JMP/JZ whose `i8` offset can only be computed once every label offset
    /// is known. `at` is this instruction's byte offset; the operand sits at
    /// `at + 1` and the jump base is `at + 2`.
    Jump { op: u8, label: String, at: usize, line: usize },
}

/// Assemble `src` into raw BitVM bytecode.
///
/// Two passes: pass 1 tokenizes each line, records label offsets, and lays out
/// every instruction's size (so `push` can pick PUSH8 vs PUSH16); pass 2 emits
/// bytes and resolves each jump's relative `i8`. Any problem returns an
/// [`AsmError`] with the source line — the assembler never panics.
pub fn assemble(src: &str) -> Result<Vec<u8>, AsmError> {
    let mut items: Vec<Item> = Vec::new();
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut pc: usize = 0;

    // ---- pass 1: tokenize, place labels, lay out instruction sizes --------
    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }

        // A `name:` line defines a label at the current byte offset.
        if let Some(name) = text.strip_suffix(':') {
            let name = name.trim();
            if name.is_empty() || !is_ident(name) {
                return Err(err(line, format!("invalid label name '{name}'")));
            }
            if labels.insert(name.to_ascii_lowercase(), pc).is_some() {
                return Err(err(line, format!("duplicate label '{name}'")));
            }
            continue;
        }

        // Otherwise: `mnemonic [operand]`.
        let mut parts = text.split_whitespace();
        let mnem = parts.next().unwrap().to_ascii_lowercase();
        let operand = parts.next();
        if parts.next().is_some() {
            return Err(err(line, format!("unexpected extra tokens after '{mnem}'")));
        }

        if let Some(&(_, byte)) = SIMPLE.iter().find(|(m, _)| *m == mnem) {
            no_operand(line, &mnem, operand)?;
            items.push(Item::Bytes(vec![byte]));
            pc += 1;
        } else if mnem == "push" {
            let bytes = encode_push(line, operand, None)?;
            pc += bytes.len();
            items.push(Item::Bytes(bytes));
        } else if mnem == "push8" {
            let bytes = encode_push(line, operand, Some(1))?;
            pc += bytes.len();
            items.push(Item::Bytes(bytes));
        } else if mnem == "push16" {
            let bytes = encode_push(line, operand, Some(2))?;
            pc += bytes.len();
            items.push(Item::Bytes(bytes));
        } else if mnem == "jmp" || mnem == "jz" {
            let label = operand
                .ok_or_else(|| err(line, format!("'{mnem}' needs a label")))?
                .to_ascii_lowercase();
            let op = if mnem == "jmp" { 0x60 } else { 0x61 };
            items.push(Item::Jump { op, label, at: pc, line });
            pc += 2;
        } else {
            return Err(err(line, format!("unknown mnemonic '{mnem}'")));
        }
    }

    // ---- pass 2: emit bytes, resolving each jump's relative i8 offset ------
    let mut out: Vec<u8> = Vec::with_capacity(pc);
    for item in items {
        match item {
            Item::Bytes(bytes) => out.extend_from_slice(&bytes),
            Item::Jump { op, label, at, line } => {
                let target = *labels
                    .get(&label)
                    .ok_or_else(|| err(line, format!("undefined label '{label}'")))?;
                // Offset base is the byte after the full instruction (at + 2),
                // exactly matching the VM's relative-jump convention.
                let rel = target as i64 - (at as i64 + 2);
                if !(-128..=127).contains(&rel) {
                    return Err(err(line, format!("jump to '{label}' is {rel} bytes, out of i8 range")));
                }
                out.push(op);
                out.push(rel as i8 as u8);
            }
        }
    }
    Ok(out)
}

/// Drop everything from the first `;` to end of line.
fn strip_comment(line: &str) -> &str {
    match line.find(';') {
        Some(i) => &line[..i],
        None => line,
    }
}

/// A label/mnemonic identifier: ASCII alnum or `_`, not starting with a digit.
fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reject a stray operand on an operand-less mnemonic.
fn no_operand(line: usize, mnem: &str, operand: Option<&str>) -> Result<(), AsmError> {
    match operand {
        None => Ok(()),
        Some(op) => Err(err(line, format!("'{mnem}' takes no operand (got '{op}')"))),
    }
}

/// Encode a `push`/`push8`/`push16`. `width` forces a size (`Some(1|2)`); `None`
/// auto-selects PUSH8 for values that fit a `u8`, else PUSH16.
fn encode_push(line: usize, operand: Option<&str>, width: Option<usize>) -> Result<Vec<u8>, AsmError> {
    let operand = operand.ok_or_else(|| err(line, "'push' needs a number".to_string()))?;
    let value = parse_num(line, operand)?;
    match width {
        Some(1) => {
            if value > 0xFF {
                return Err(err(line, format!("push8 operand {value} does not fit in a byte")));
            }
            Ok(vec![0x10, value as u8])
        }
        Some(2) => {
            let [lo, hi] = (value as u16).to_le_bytes();
            Ok(vec![0x11, lo, hi])
        }
        _ if value <= 0xFF => Ok(vec![0x10, value as u8]),
        _ => {
            let [lo, hi] = (value as u16).to_le_bytes();
            Ok(vec![0x11, lo, hi])
        }
    }
}

/// Parse a decimal or `0x`-hex number, bounded to `u16` (the VM's cell width).
fn parse_num(line: usize, tok: &str) -> Result<u32, AsmError> {
    let parsed = if let Some(hex) = tok.strip_prefix("0x").or_else(|| tok.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        tok.parse::<u32>()
    };
    match parsed {
        Ok(v) if v <= 0xFFFF => Ok(v),
        Ok(v) => Err(err(line, format!("number {v} does not fit in u16 (max 65535)"))),
        Err(_) => Err(err(line, format!("bad number '{tok}'"))),
    }
}

/// Build an [`AsmError`] for `line`.
fn err(line: usize, msg: impl Into<String>) -> AsmError {
    AsmError { line, msg: msg.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{Halt, Vm, VmHost};

    fn asm(src: &str) -> Vec<u8> {
        assemble(src).expect("assembles cleanly")
    }

    #[test]
    fn door_roundtrip_exact_bytes() {
        // The ROADMAP door example must assemble byte-identical to levels/door.bm
        // script id 1: 10 04 10 03 32 01.
        let src = "\
; pressure plate: open the door at (4,3)
on_enter:
    push 4
    push 3
    clr_wall
    halt
";
        assert_eq!(asm(src), vec![0x10, 0x04, 0x10, 0x03, 0x32, 0x01]);
    }

    #[test]
    fn every_operandless_mnemonic_maps_to_its_byte() {
        for &(mnem, byte) in SIMPLE {
            assert_eq!(asm(mnem), vec![byte], "{mnem}");
            // Case-insensitive.
            assert_eq!(asm(&mnem.to_uppercase()), vec![byte], "{mnem} upper");
        }
    }

    #[test]
    fn push_picks_push8_vs_push16_by_magnitude() {
        assert_eq!(asm("push 0"), vec![0x10, 0x00]);
        assert_eq!(asm("push 255"), vec![0x10, 0xFF]);
        assert_eq!(asm("push 256"), vec![0x11, 0x00, 0x01]); // LE
        assert_eq!(asm("push 0x1234"), vec![0x11, 0x34, 0x12]);
        assert_eq!(asm("push 0xFF"), vec![0x10, 0xFF]); // hex under 256 -> PUSH8
    }

    #[test]
    fn explicit_push_widths() {
        assert_eq!(asm("push8 7"), vec![0x10, 0x07]);
        assert_eq!(asm("push16 7"), vec![0x11, 0x07, 0x00]); // forced 16-bit
    }

    #[test]
    fn hex_and_decimal_operands_agree() {
        assert_eq!(asm("push 16"), asm("push 0x10"));
    }

    #[test]
    fn comments_and_blank_lines_ignored() {
        let src = "\
; a comment

    push 1  ; trailing comment
halt
";
        assert_eq!(asm(src), vec![0x10, 0x01, 0x01]);
    }

    #[test]
    fn backward_jump_offset_is_correct() {
        // loop: nop; jmp loop  -> 00 60 FD. The jmp is at byte 1; base = 3;
        // target = 0; rel = 0 - 3 = -3 = 0xFD.
        assert_eq!(asm("loop:\n nop\n jmp loop\n"), vec![0x00, 0x60, 0xFD]);
        // The canonical self-loop: jmp onto itself is -2 (0xFE).
        assert_eq!(asm("self:\n jmp self\n"), vec![0x60, 0xFE]);
    }

    #[test]
    fn forward_jump_offset_is_correct() {
        // jz end; nop; end: halt -> 61 01 00 01. jz at 0, base = 2, target = 3,
        // rel = +1 = 0x01.
        assert_eq!(asm("jz end\n nop\nend:\n halt\n"), vec![0x61, 0x01, 0x00, 0x01]);
    }

    // A minimal host so an assembled loop can actually be executed.
    struct Host {
        walls: Vec<bool>,
    }
    impl VmHost for Host {
        fn get_wall(&self, x: u16, _y: u16) -> bool {
            *self.walls.get(x as usize).unwrap_or(&false)
        }
        fn set_wall(&mut self, x: u16, _y: u16, v: bool) {
            if let Some(slot) = self.walls.get_mut(x as usize) {
                *slot = v;
            }
        }
        fn player_x(&self) -> u16 {
            0
        }
        fn player_y(&self) -> u16 {
            0
        }
        fn get_item(&self, _x: u16, _y: u16) -> bool {
            false
        }
        fn score(&self) -> u16 {
            0
        }
    }

    #[test]
    fn assembled_countdown_loop_runs_and_terminates() {
        // Countdown: start at 3, decrement to 0, then halt. Proves labels +
        // jz/jmp assemble into bytecode the VM executes to a clean stop.
        //   push 3
        // top:
        //   push 1
        //   sub          ; n = n - 1
        //   dup
        //   jz done      ; if n == 0, exit
        //   jmp top
        // done:
        //   halt
        let bytes = asm("\
    push 3
top:
    push 1
    sub
    dup
    jz done
    jmp top
done:
    halt
");
        let mut host = Host { walls: vec![false; 4] };
        let halt = Vm::new(1).run(&bytes, &mut host);
        assert_eq!(halt, Halt::Halt, "the loop terminates cleanly via HALT");
    }

    #[test]
    fn infinite_assembled_loop_halts_via_budget_not_hang() {
        // jmp self must run to the VM's budget cap, proving no hang.
        let bytes = asm("l:\n jmp l\n");
        let mut host = Host { walls: vec![false; 1] };
        assert_eq!(Vm::new(1).run(&bytes, &mut host), Halt::Budget);
    }

    // ---- error cases: all return errors with line numbers, never panic -----

    #[test]
    fn unknown_mnemonic_errors_with_line() {
        let e = assemble("push 1\n frobnicate\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.msg.contains("unknown mnemonic"), "{}", e.msg);
    }

    #[test]
    fn undefined_label_errors() {
        let e = assemble("jmp nowhere\n").unwrap_err();
        assert!(e.msg.contains("undefined label"), "{}", e.msg);
    }

    #[test]
    fn duplicate_label_errors_with_line() {
        let e = assemble("a:\n nop\na:\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.msg.contains("duplicate label"), "{}", e.msg);
    }

    #[test]
    fn offset_out_of_i8_range_errors() {
        // 200 NOPs then jmp back to the top overflows i8 (< -128).
        let mut src = String::from("top:\n");
        for _ in 0..200 {
            src.push_str(" nop\n");
        }
        src.push_str(" jmp top\n");
        let e = assemble(&src).unwrap_err();
        assert!(e.msg.contains("out of i8 range"), "{}", e.msg);
    }

    #[test]
    fn bad_operands_error() {
        assert!(assemble("push notanumber\n").is_err());
        assert!(assemble("push\n").is_err()); // missing operand
        assert!(assemble("push 70000\n").is_err()); // > u16
        assert!(assemble("halt 5\n").is_err()); // stray operand
        assert!(assemble("push8 300\n").is_err()); // doesn't fit u8
        assert!(assemble("push 1 2\n").is_err()); // extra token
    }

    #[test]
    fn never_panics_on_garbage() {
        // A grab-bag of malformed lines: each returns Err, none panic.
        for bad in ["::", "1abc", ":", "jmp", "0x", "push 0xZZ"] {
            assert!(assemble(bad).is_err(), "{bad:?} should error");
        }
    }

    /// Design rule #6, enforced mechanically: the assembler proper (this file,
    /// excluding this `#[cfg(test)]` module) must stay ≤300 lines. Counting
    /// stops at the `mod tests` marker so the tests themselves don't count.
    #[test]
    fn assembler_stays_under_300_lines() {
        let src = include_str!("asm.rs");
        let marker = "#[cfg(test)]";
        let code = src.split(marker).next().unwrap();
        let count = code.lines().count();
        assert!(count <= 300, "assembler is {count} lines, must be <= 300 (rule #6)");
        // Surfaced in test output so the count is always visible.
        println!("assembler non-test line count: {count}");
    }
}
