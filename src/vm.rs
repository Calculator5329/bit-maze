//! BitVM — the logic VM (Phase 4).
//!
//! A tiny stack machine run once per trigger. See `docs/VM.md` and the ROADMAP
//! for the authoritative machine model and opcode table. The whole point is
//! rule #5: the VM is *hard-capped* so any bytes — a real script or random
//! garbage — are a valid, inert program. It can never hang or crash the game;
//! worst case it halts and nothing happens.
//!
//! ## Determinism
//! Determinism is law. The VM has no floats, no host time, and no host RNG. Its
//! only randomness is a seeded xorshift32 register driven by the `RAND` opcode.
//! The seed is passed in ([`Vm::new`]), derived deterministically upstream from
//! the level bytes + a run seed, so the same inputs always produce the same
//! execution.
//!
//! ## Caps (rule #5, all enforced without ever panicking)
//! - **Stack:** [`STACK_MAX`] = 64 `u16` values. Pushing past it → [`Halt::StackOverflow`].
//! - **RAM:** [`RAM_SIZE`] = 256 fixed bytes of scratch, addressable 0..=255. No heap.
//! - **Budget:** [`BUDGET`] = 4096 instructions per run. Exceeding it → [`Halt::Budget`].
//!   This is the anti-hang guard: an infinite loop halts cleanly and fast.
//!
//! ## World seam
//! The VM never touches [`crate::world::World`] directly; it talks to a
//! [`VmHost`] trait (read/write the walls plane, read player x/y). That keeps
//! the VM unit-testable against a trivial mock host with no window and no full
//! level — and gives every world access a single bounds-checked chokepoint.

/// Maximum stack depth in `u16` values. Push past this → [`Halt::StackOverflow`].
pub const STACK_MAX: usize = 64;
/// Fixed scratch RAM size in bytes, addressable `0..=255`. No heap.
pub const RAM_SIZE: usize = 256;
/// Maximum instructions executed per run. Exceed → [`Halt::Budget`].
pub const BUDGET: u32 = 4096;

/// The controlled interface the VM uses to see and mutate the game world.
///
/// Coordinates are `u16` (the width of a stack cell). Out-of-range coordinates
/// are the host's responsibility to handle safely: reads return `false`, writes
/// are a no-op — never a panic. The blanket implementation for
/// [`crate::world::World`] does exactly that (see `world.rs`).
pub trait VmHost {
    /// Read the walls-plane bit at `(x, y)`. Out of bounds → `false`.
    fn get_wall(&self, x: u16, y: u16) -> bool;
    /// Set the walls-plane bit at `(x, y)`. Out of bounds → no-op.
    fn set_wall(&mut self, x: u16, y: u16, value: bool);
    /// Current player column.
    fn player_x(&self) -> u16;
    /// Current player row.
    fn player_y(&self) -> u16;
    /// Read the items-plane bit at `(x, y)` (Phase 7). Out of bounds, or a level
    /// with no items plane → `false`.
    fn get_item(&self, x: u16, y: u16) -> bool;
    /// Current collected-item score, saturated into a `u16` cell (Phase 7).
    fn score(&self) -> u16;
    /// Read the hazards-plane bit at `(x, y)` (Phase 8). Out of bounds, or a
    /// level with no hazards plane → `false`.
    fn get_hazard(&self, x: u16, y: u16) -> bool;
}

/// Why a script run stopped. Every stop is one of these — the VM never panics
/// and never runs away. [`Halt::Halt`] and [`Halt::EndOfScript`] are the two
/// "clean" outcomes; the rest are caps or malformed-bytecode guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halt {
    /// Hit an explicit `HALT` (0x01) opcode. The normal, intended stop.
    Halt,
    /// Program counter ran off the end of the script without a `HALT`. Clean.
    EndOfScript,
    /// An operand (PUSH8/PUSH16/JMP/JZ) was cut off at the end of the script.
    /// Clean halt, no OOB read.
    Truncated,
    /// A JMP/JZ target landed outside the script. Clean halt, no OOB read.
    BadJump,
    /// Encountered an opcode this VM version does not define. Carries the byte.
    BadOpcode(u8),
    /// Pushed past [`STACK_MAX`].
    StackOverflow,
    /// An opcode needed more operands than were on the stack.
    StackUnderflow,
    /// Executed [`BUDGET`] instructions without halting (the anti-hang cap).
    Budget,
}

impl Halt {
    /// True for the two non-error stops (`HALT` opcode or clean end-of-script).
    pub fn is_clean(self) -> bool {
        matches!(self, Halt::Halt | Halt::EndOfScript)
    }
}

impl std::fmt::Display for Halt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Halt::Halt => write!(f, "HALT"),
            Halt::EndOfScript => write!(f, "ran off end of script"),
            Halt::Truncated => write!(f, "truncated operand"),
            Halt::BadJump => write!(f, "jump target out of bounds"),
            Halt::BadOpcode(op) => write!(f, "bad opcode {op:#04x}"),
            Halt::StackOverflow => write!(f, "stack overflow (depth > {STACK_MAX})"),
            Halt::StackUnderflow => write!(f, "stack underflow"),
            Halt::Budget => write!(f, "instruction budget exceeded ({BUDGET})"),
        }
    }
}

/// One BitVM execution context: stack, scratch RAM, PRNG register, and the
/// spent-instruction counter. Build with [`Vm::new`] and run bytecode with
/// [`Vm::run`]. A fresh `Vm` per trigger keeps runs stateless in v0.
pub struct Vm {
    stack: Vec<u16>,
    /// Fixed 256-byte scratch RAM, read/written by `LOAD` (0x70) / `STORE`
    /// (0x71) since Phase 7. Addresses wrap into `0..=255` (`addr & 0xFF`) so a
    /// script can never index out of bounds. No heap.
    ram: [u8; RAM_SIZE],
    /// xorshift32 state. Non-zero invariant is upheld in [`Vm::new`].
    rng: u32,
    /// Instructions executed so far this run (capped at [`BUDGET`]).
    instr: u32,
}

impl Vm {
    /// Create a VM seeded for its PRNG. xorshift32 is degenerate at state 0, so
    /// a `0` seed is remapped to a fixed non-zero constant; determinism holds.
    pub fn new(seed: u32) -> Vm {
        Vm {
            stack: Vec::with_capacity(STACK_MAX),
            ram: [0u8; RAM_SIZE],
            rng: if seed == 0 { 0xDEAD_BEEF } else { seed },
            instr: 0,
        }
    }

    /// Borrow the scratch RAM (for tests / future load-store opcodes).
    pub fn ram(&self) -> &[u8; RAM_SIZE] {
        &self.ram
    }

    /// Advance the xorshift32 register and return its low 16 bits (the `RAND`
    /// result). The *only* source of randomness in the whole engine.
    fn next_rand(&mut self) -> u16 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x & 0xFFFF) as u16
    }

    /// Push, enforcing the stack-depth cap.
    fn push(&mut self, v: u16) -> Result<(), Halt> {
        if self.stack.len() >= STACK_MAX {
            return Err(Halt::StackOverflow);
        }
        self.stack.push(v);
        Ok(())
    }

    /// Run `script` against `host` until it halts, and return why it halted.
    ///
    /// JMP/JZ offsets are `i8`, **relative to the byte after the full
    /// instruction** (opcode + operand) — i.e. the address of the next
    /// instruction, the usual relative-jump convention. So `JMP -2` (`60 FE`)
    /// jumps back onto itself, the canonical self-loop, which the budget cap
    /// then stops. A target outside `0..=len` halts with [`Halt::BadJump`]
    /// rather than reading out of bounds.
    pub fn run<H: VmHost + ?Sized>(&mut self, script: &[u8], host: &mut H) -> Halt {
        let mut pc: usize = 0;
        loop {
            // Clean end: the pc walked off the script without a HALT.
            if pc >= script.len() {
                return Halt::EndOfScript;
            }
            // Anti-hang cap: every executed instruction costs one budget unit.
            if self.instr >= BUDGET {
                return Halt::Budget;
            }
            self.instr += 1;

            let op = script[pc];
            pc += 1;

            match op {
                0x00 => {} // NOP
                0x01 => return Halt::Halt,

                0x10 => {
                    // PUSH8 b
                    let Some(&b) = script.get(pc) else { return Halt::Truncated };
                    pc += 1;
                    if let Err(h) = self.push(b as u16) {
                        return h;
                    }
                }
                0x11 => {
                    // PUSH16 w (LE)
                    let (Some(&lo), Some(&hi)) = (script.get(pc), script.get(pc + 1)) else {
                        return Halt::Truncated;
                    };
                    pc += 2;
                    if let Err(h) = self.push(u16::from_le_bytes([lo, hi])) {
                        return h;
                    }
                }
                0x12 => {
                    // POP
                    if self.stack.pop().is_none() {
                        return Halt::StackUnderflow;
                    }
                }
                0x13 => {
                    // DUP
                    let Some(&top) = self.stack.last() else {
                        return Halt::StackUnderflow;
                    };
                    if let Err(h) = self.push(top) {
                        return h;
                    }
                }

                0x20 => {
                    // ADD (wrapping): a b -> a+b
                    let (Some(b), Some(a)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.stack.push(a.wrapping_add(b)); // net -1, room guaranteed
                }
                0x21 => {
                    // SUB (wrapping): a b -> a-b
                    let (Some(b), Some(a)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.stack.push(a.wrapping_sub(b));
                }

                0x30 => {
                    // GET_WALL: x y -> bit
                    let (Some(y), Some(x)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.stack.push(host.get_wall(x, y) as u16);
                }
                0x31 => {
                    // SET_WALL: x y ->
                    let (Some(y), Some(x)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    host.set_wall(x, y, true);
                }
                0x32 => {
                    // CLR_WALL: x y ->
                    let (Some(y), Some(x)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    host.set_wall(x, y, false);
                }

                0x40 => {
                    // PLAYER_X -> x
                    if let Err(h) = self.push(host.player_x()) {
                        return h;
                    }
                }
                0x41 => {
                    // PLAYER_Y -> y
                    if let Err(h) = self.push(host.player_y()) {
                        return h;
                    }
                }
                0x42 => {
                    // GET_ITEM: x y -> bit (read the items plane)
                    let (Some(y), Some(x)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.stack.push(host.get_item(x, y) as u16); // net -1, room guaranteed
                }
                0x43 => {
                    // SCORE -> current collected-item count
                    let s = host.score();
                    if let Err(h) = self.push(s) {
                        return h;
                    }
                }
                0x44 => {
                    // GET_HAZARD: x y -> bit (read the hazards plane, Phase 8)
                    let (Some(y), Some(x)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.stack.push(host.get_hazard(x, y) as u16); // net -1, room guaranteed
                }

                0x50 => {
                    // RAND -> r
                    let r = self.next_rand();
                    if let Err(h) = self.push(r) {
                        return h;
                    }
                }

                0x60 => {
                    // JMP o (i8 rel)
                    let Some(&off) = script.get(pc) else { return Halt::Truncated };
                    pc += 1;
                    match jump_target(pc, off, script.len()) {
                        Some(t) => pc = t,
                        None => return Halt::BadJump,
                    }
                }
                0x61 => {
                    // JZ o (i8 rel): pop; if 0, jump
                    let Some(&off) = script.get(pc) else { return Halt::Truncated };
                    pc += 1;
                    let Some(v) = self.stack.pop() else {
                        return Halt::StackUnderflow;
                    };
                    if v == 0 {
                        match jump_target(pc, off, script.len()) {
                            Some(t) => pc = t,
                            None => return Halt::BadJump,
                        }
                    }
                }

                0x70 => {
                    // LOAD: addr -> value. Reads the scratch RAM byte at
                    // `addr & 0xFF` (address wraps, so never out of bounds).
                    let Some(addr) = self.stack.pop() else {
                        return Halt::StackUnderflow;
                    };
                    let v = self.ram[(addr & 0xFF) as usize] as u16;
                    if let Err(h) = self.push(v) {
                        return h;
                    }
                }
                0x71 => {
                    // STORE: value addr -> . Writes the low byte of `value` into
                    // scratch RAM at `addr & 0xFF` (address wraps).
                    let (Some(addr), Some(value)) = (self.stack.pop(), self.stack.pop()) else {
                        return Halt::StackUnderflow;
                    };
                    self.ram[(addr & 0xFF) as usize] = (value & 0xFF) as u8;
                }

                other => return Halt::BadOpcode(other),
            }
        }
    }
}

/// Resolve a relative jump: `next_pc + off`, valid only if it lands in
/// `0..=len` (landing exactly on `len` is a clean end-of-script next tick).
/// Returns `None` for an out-of-bounds target so the caller can halt cleanly.
fn jump_target(next_pc: usize, off: u8, len: usize) -> Option<usize> {
    let target = next_pc as i64 + (off as i8) as i64;
    if target < 0 || target > len as i64 {
        None
    } else {
        Some(target as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial standalone host: a small wall grid plus a player position, so
    /// the VM is testable with no `World`, no level, and no window.
    struct TestHost {
        w: u16,
        h: u16,
        walls: Vec<bool>,
        items: Vec<bool>,
        hazards: Vec<bool>,
        px: u16,
        py: u16,
        score: u16,
    }

    impl TestHost {
        fn new(w: u16, h: u16) -> TestHost {
            let n = (w * h) as usize;
            TestHost {
                w,
                h,
                walls: vec![false; n],
                items: vec![false; n],
                hazards: vec![false; n],
                px: 0,
                py: 0,
                score: 0,
            }
        }
    }

    impl VmHost for TestHost {
        fn get_wall(&self, x: u16, y: u16) -> bool {
            if x >= self.w || y >= self.h {
                return false;
            }
            self.walls[(y * self.w + x) as usize]
        }
        fn set_wall(&mut self, x: u16, y: u16, value: bool) {
            if x >= self.w || y >= self.h {
                return; // OOB write is a documented no-op
            }
            self.walls[(y * self.w + x) as usize] = value;
        }
        fn player_x(&self) -> u16 {
            self.px
        }
        fn player_y(&self) -> u16 {
            self.py
        }
        fn get_item(&self, x: u16, y: u16) -> bool {
            if x >= self.w || y >= self.h {
                return false;
            }
            self.items[(y * self.w + x) as usize]
        }
        fn score(&self) -> u16 {
            self.score
        }
        fn get_hazard(&self, x: u16, y: u16) -> bool {
            if x >= self.w || y >= self.h {
                return false;
            }
            self.hazards[(y * self.w + x) as usize]
        }
    }

    /// Run `script` on a fresh VM (seed 1) against `host`, returning the halt
    /// reason. Exposes the stack via a trailing PUSH-inspection pattern is
    /// avoided — tests assert through the host or the returned reason.
    fn run_on(host: &mut TestHost, script: &[u8]) -> Halt {
        Vm::new(1).run(script, host)
    }

    // ---- stack ops -------------------------------------------------------

    #[test]
    fn push8_dup_add_writes_expected_wall() {
        // push 3; dup; add -> 6 on stack; then push 0; ... we prove the numeric
        // result by using it as a coordinate: push 3;dup;add = 6, push 0, we
        // want set_wall(6,0). Stack for SET_WALL is x y. So: push 6 (via
        // 3 dup add), push 0, set_wall.
        let mut host = TestHost::new(8, 1);
        let script = [
            0x10, 0x03, // push 3
            0x13, //       dup       -> [3,3]
            0x20, //       add       -> [6]
            0x10, 0x00, // push 0    -> [6,0]
            0x31, //       set_wall(6,0)
            0x01, //       halt
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(6, 0), "3+3=6 must select column 6");
    }

    #[test]
    fn push16_little_endian() {
        // push16 0x0102 = 258; but coords are u16 so clamp via a small grid:
        // instead prove LE decoding by subtracting. push16 0x0005 (05 00) -> 5.
        let mut host = TestHost::new(8, 1);
        let script = [0x11, 0x05, 0x00, 0x10, 0x00, 0x31, 0x01]; // set_wall(5,0)
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(5, 0));
    }

    #[test]
    fn pop_discards_top() {
        // push 1; push 0; pop -> [1]; push 0; set_wall(1,0)? Stack after pop is
        // [1]; push 0 -> [1,0]; set_wall(1,0).
        let mut host = TestHost::new(8, 1);
        let script = [0x10, 0x01, 0x10, 0x00, 0x12, 0x10, 0x00, 0x31, 0x01];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(1, 0));
    }

    #[test]
    fn sub_wraps() {
        // push 0; push 1; sub -> 0-1 wraps to 0xFFFF. Use as a column into a
        // grid: OOB set is a no-op, so instead read it back via GET_WALL bounds.
        // Simpler: 0-1=0xFFFF; get_wall(0xFFFF, 0) OOB -> false (no panic).
        let mut host = TestHost::new(4, 4);
        let script = [
            0x10, 0x00, // push 0
            0x10, 0x01, // push 1
            0x21, //       sub -> 0xFFFF
            0x10, 0x00, // push 0 (y)
            0x30, //       get_wall(0xFFFF, 0) OOB -> pushes 0
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
    }

    #[test]
    fn add_wraps() {
        // 0xFFFF + 1 = 0 (wrapping). Prove by using result as x for set_wall(0,0).
        let mut host = TestHost::new(4, 1);
        let script = [
            0x11, 0xFF, 0xFF, // push16 0xFFFF
            0x10, 0x01, //       push 1
            0x20, //             add -> 0 (wrapping)
            0x10, 0x00, //       push 0 (y)
            0x31, //             set_wall(0,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(0, 0), "0xFFFF+1 wraps to 0");
    }

    // ---- world ops -------------------------------------------------------

    #[test]
    fn set_get_clr_wall_roundtrip() {
        let mut host = TestHost::new(4, 4);
        // set_wall(2,1)
        assert_eq!(run_on(&mut host, &[0x10, 0x02, 0x10, 0x01, 0x31, 0x01]), Halt::Halt);
        assert!(host.get_wall(2, 1));
        // clr_wall(2,1)
        assert_eq!(run_on(&mut host, &[0x10, 0x02, 0x10, 0x01, 0x32, 0x01]), Halt::Halt);
        assert!(!host.get_wall(2, 1));
    }

    #[test]
    fn get_wall_reads_bit_via_setwall_of_result() {
        // Pre-set (1,1); GET_WALL(1,1) -> 1; then use that 1 as x for set_wall(1,0).
        let mut host = TestHost::new(4, 4);
        host.set_wall(1, 1, true);
        let script = [
            0x10, 0x01, 0x10, 0x01, // push 1,1
            0x30, //                   get_wall(1,1) -> 1
            0x10, 0x00, //             push 0 (y)
            0x31, //                   set_wall(1,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(1, 0), "GET_WALL of a set tile yields 1");
    }

    #[test]
    fn oob_wall_ops_are_safe_noops() {
        let mut host = TestHost::new(2, 2);
        // set_wall(1000, 1000) then get_wall(1000,1000) — no panic, stays clear.
        assert_eq!(
            run_on(&mut host, &[0x11, 0xE8, 0x03, 0x11, 0xE8, 0x03, 0x31, 0x01]),
            Halt::Halt
        );
        assert!(!host.get_wall(1000, 1000));
    }

    #[test]
    fn player_x_and_y() {
        // Put player at (3,2); PLAYER_X, PLAYER_Y -> set_wall(3,2).
        let mut host = TestHost::new(4, 4);
        host.px = 3;
        host.py = 2;
        let script = [0x40, 0x41, 0x31, 0x01]; // player_x, player_y, set_wall(3,2)
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(3, 2));
    }

    // ---- rng determinism -------------------------------------------------

    #[test]
    fn rand_is_deterministic_known_sequence() {
        // xorshift32 seeded with 1 yields low-16 = [0x2021, 0x0601, 0xa8c5, ...].
        // Verified with an independent reference implementation.
        let mut vm = Vm::new(1);
        assert_eq!(vm.next_rand(), 0x2021);
        assert_eq!(vm.next_rand(), 0x0601);
        assert_eq!(vm.next_rand(), 0xa8c5);
        assert_eq!(vm.next_rand(), 0x994f);

        // A different seed gives a different sequence.
        let mut vm2 = Vm::new(0x1234_5678);
        assert_eq!(vm2.next_rand(), 0x5aa5);
        assert_eq!(vm2.next_rand(), 0x24a3);

        // Same seed, same sequence — replays are reproducible.
        let mut a = Vm::new(42);
        let mut b = Vm::new(42);
        for _ in 0..16 {
            assert_eq!(a.next_rand(), b.next_rand());
        }
    }

    #[test]
    fn rand_opcode_pushes_low16_and_uses_it() {
        // RAND -> 0x2021 (col 0x21=33 masked into a grid); prove RAND lands on
        // the stack by consuming it as a coordinate (OOB set is a safe no-op).
        let mut host = TestHost::new(4, 4);
        let script = [0x50, 0x10, 0x00, 0x31, 0x01]; // rand, push 0, set_wall(r,0)
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        // 0x2021 is OOB for a 4x4 grid -> no panic, nothing set.
        assert!(!host.get_wall(0, 0));
    }

    // ---- control flow ----------------------------------------------------

    #[test]
    fn jmp_skips_forward() {
        // JMP +2 skips over a set_wall(0,0) we don't want, landing on set_wall(1,0).
        let mut host = TestHost::new(4, 1);
        let script = [
            0x60, 0x02, //             jmp +2 (skip the next 2 bytes)
            0x10, 0x00, //             (skipped) push 0
            0x10, 0x01, 0x10, 0x00, // push 1, push 0
            0x31, //                   set_wall(1,0)
            0x01,
        ];
        // next_pc after operand is 2; +2 -> 4, which is "push 1".
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(1, 0));
    }

    #[test]
    fn jz_jumps_when_zero_and_falls_through_when_nonzero() {
        // push 0; jz +3 -> jumps over push1/push0; lands on set_wall using... we
        // arrange the taken branch to set (2,0). Layout:
        //  0: 10 00      push 0
        //  2: 61 03      jz +3     (next_pc=4, +3 -> 7)
        //  4: 10 09      push 9   (skipped when taken)
        //  6: 00         nop      (skipped when taken)
        //  7: 10 02 10 00 31   push2 push0 set_wall(2,0)
        // 12: 01        halt
        let mut host = TestHost::new(4, 1);
        let taken = [
            0x10, 0x00, 0x61, 0x03, 0x10, 0x09, 0x00, 0x10, 0x02, 0x10, 0x00, 0x31, 0x01,
        ];
        assert_eq!(run_on(&mut host, &taken), Halt::Halt);
        assert!(host.get_wall(2, 0), "JZ with 0 must take the jump");

        // Non-zero: JZ pops the 1 and falls through; the following instructions
        // push a fresh (1,0) and set it, proving control continued past the JZ.
        let mut host2 = TestHost::new(4, 1);
        let not_taken = [
            0x10, 0x01, // push 1 (nonzero test value)
            0x61, 0x02, // jz +2 (NOT taken; consumes the 1, stack now empty)
            0x10, 0x01, // push 1 (x)
            0x10, 0x00, // push 0 (y)
            0x31, //       set_wall(1,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host2, &not_taken), Halt::Halt);
        assert!(host2.get_wall(1, 0), "JZ with nonzero falls through");
    }

    // ---- caps & malformed bytecode --------------------------------------

    #[test]
    fn pushing_forever_halts_with_stack_overflow() {
        // push 1; jmp -4 (back to start) — grows the stack without bound.
        let mut host = TestHost::new(1, 1);
        let script = [0x10, 0x01, 0x60, 0xFC]; // 0xFC = -4
        assert_eq!(run_on(&mut host, &script), Halt::StackOverflow);
    }

    #[test]
    fn infinite_loop_halts_with_budget_quickly() {
        // JMP -2 is a tight self-loop touching nothing. Must halt via the budget
        // cap — the whole point of rule #5. This test proves it does not hang.
        let mut host = TestHost::new(1, 1);
        let script = [0x60, 0xFE]; // 0xFE = -2 -> jumps onto itself
        assert_eq!(run_on(&mut host, &script), Halt::Budget);
    }

    #[test]
    fn bad_opcode_halts() {
        let mut host = TestHost::new(1, 1);
        assert_eq!(run_on(&mut host, &[0xEE, 0x01]), Halt::BadOpcode(0xEE));
    }

    #[test]
    fn stack_underflow_halts() {
        let mut host = TestHost::new(1, 1);
        assert_eq!(run_on(&mut host, &[0x20]), Halt::StackUnderflow); // ADD on empty
        assert_eq!(run_on(&mut host, &[0x12]), Halt::StackUnderflow); // POP on empty
        assert_eq!(run_on(&mut host, &[0x31, 0x01]), Halt::StackUnderflow); // SET_WALL
    }

    #[test]
    fn truncated_operands_halt_cleanly() {
        let mut host = TestHost::new(1, 1);
        assert_eq!(run_on(&mut host, &[0x10]), Halt::Truncated); // PUSH8, no byte
        assert_eq!(run_on(&mut host, &[0x11, 0x00]), Halt::Truncated); // PUSH16, 1 byte
        assert_eq!(run_on(&mut host, &[0x60]), Halt::Truncated); // JMP, no offset
        assert_eq!(run_on(&mut host, &[0x10, 0x00, 0x61]), Halt::Truncated); // JZ, no off
    }

    #[test]
    fn jump_out_of_bounds_halts_cleanly() {
        let mut host = TestHost::new(1, 1);
        // JMP -100 lands before the start.
        assert_eq!(run_on(&mut host, &[0x60, 0x9C]), Halt::BadJump); // 0x9C = -100
        // JMP +100 lands well past the end.
        assert_eq!(run_on(&mut host, &[0x60, 0x64]), Halt::BadJump); // +100
    }

    #[test]
    fn empty_and_halt_scripts_are_clean() {
        let mut host = TestHost::new(1, 1);
        assert_eq!(run_on(&mut host, &[]), Halt::EndOfScript);
        assert_eq!(run_on(&mut host, &[0x00, 0x00]), Halt::EndOfScript); // NOP NOP, runs off
        assert_eq!(run_on(&mut host, &[0x01]), Halt::Halt);
    }

    // ---- Phase 7 opcodes: query (items/score) & memory (load/store) ------

    #[test]
    fn get_item_reads_the_items_plane() {
        // Pre-place an item at (2,1); GET_ITEM(2,1) -> 1, used as x for set_wall(1,0).
        let mut host = TestHost::new(4, 4);
        host.items[4 + 2] = true; // (2,1) in a 4-wide grid
        let script = [
            0x10, 0x02, 0x10, 0x01, // push 2,1
            0x42, //                   get_item(2,1) -> 1
            0x10, 0x00, //             push 0 (y)
            0x31, //                   set_wall(1,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(1, 0), "GET_ITEM of a set tile yields 1");
        // An empty tile reads 0: get_item(3,3)=0; +5 -> 5; set_wall(5,0). If the
        // read were 1 it would land on column 6 instead.
        let mut host2 = TestHost::new(8, 1);
        let script2 = [
            0x10, 0x03, 0x10, 0x03, // push 3,3
            0x42, //                   get_item(3,3) -> 0
            0x10, 0x05, 0x20, //       push 5; add -> 5
            0x10, 0x00, 0x31, //       push 0; set_wall(5,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host2, &script2), Halt::Halt);
        assert!(host2.get_wall(5, 0), "GET_ITEM of an empty tile yields 0 (0+5=5)");
        assert!(!host2.get_wall(6, 0), "not 1 (would be 1+5=6)");
    }

    #[test]
    fn get_item_out_of_bounds_is_safe() {
        // GET_ITEM(1000,1000) on a 2x2 grid must not panic and reads 0.
        let mut host = TestHost::new(2, 2);
        let script = [0x11, 0xE8, 0x03, 0x11, 0xE8, 0x03, 0x42, 0x12, 0x01];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
    }

    #[test]
    fn score_pushes_the_hosts_score() {
        // SCORE -> 2; use it as column x for set_wall(2,0).
        let mut host = TestHost::new(4, 4);
        host.score = 2;
        let script = [0x43, 0x10, 0x00, 0x31, 0x01]; // score, push 0, set_wall(2,0)
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(2, 0), "SCORE pushes the current score");
    }

    #[test]
    fn get_hazard_reads_the_hazards_plane() {
        // Pre-place a hazard at (2,1); GET_HAZARD(2,1) -> 1, used as x for
        // set_wall(1,0). Mirrors the GET_ITEM/GET_WALL test pattern.
        let mut host = TestHost::new(4, 4);
        host.hazards[4 + 2] = true; // (2,1) in a 4-wide grid
        let script = [
            0x10, 0x02, 0x10, 0x01, // push 2,1
            0x44, //                   get_hazard(2,1) -> 1
            0x10, 0x00, //             push 0 (y)
            0x31, //                   set_wall(1,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(1, 0), "GET_HAZARD of a set tile yields 1");

        // An empty tile reads 0, and out-of-bounds is a safe 0 (no panic).
        let mut host2 = TestHost::new(2, 2);
        let script2 = [0x11, 0xE8, 0x03, 0x11, 0xE8, 0x03, 0x44, 0x12, 0x01]; // get_hazard(1000,1000); pop
        assert_eq!(run_on(&mut host2, &script2), Halt::Halt);
    }

    #[test]
    fn store_then_load_roundtrips_through_ram() {
        // store 7 at addr 42; load addr 42 -> 7; use as x for set_wall(7,0).
        let mut host = TestHost::new(8, 1);
        let script = [
            0x10, 0x07, 0x10, 0x2A, 0x71, // push 7; push 42; store  (ram[42]=7)
            0x10, 0x2A, 0x70, //             push 42; load   -> 7
            0x10, 0x00, 0x31, //             push 0; set_wall(7,0)
            0x01,
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(7, 0), "value stored then loaded round-trips via RAM");
    }

    #[test]
    fn load_from_unwritten_ram_is_zero_and_address_wraps() {
        // Unwritten RAM reads 0. Also, addr 0x1_2A wraps to 0x2A, so a store to
        // 0x12A then a load from 0x2A returns the value (address wrap proof).
        let mut host = TestHost::new(4, 1);
        // load addr 5 (never written) -> 0; set_wall(0,0) leaves it clear.
        assert_eq!(run_on(&mut host, &[0x10, 0x05, 0x70, 0x12, 0x01]), Halt::Halt);
        assert!(!host.get_wall(0, 0));
        // push 3; push16 0x012A; store -> ram[0x2A]=3; push 0x2A; load -> 3.
        let script = [
            0x10, 0x03, 0x11, 0x2A, 0x01, 0x71, // store 3 at 0x012A -> ram[0x2A]
            0x10, 0x2A, 0x70, //                   load 0x2A -> 3
            0x10, 0x00, 0x31, 0x01, //             set_wall(3,0)
        ];
        assert_eq!(run_on(&mut host, &script), Halt::Halt);
        assert!(host.get_wall(3, 0), "STORE/LOAD address wraps into 0..=255");
    }

    #[test]
    fn store_underflow_halts_cleanly() {
        let mut host = TestHost::new(1, 1);
        assert_eq!(run_on(&mut host, &[0x71]), Halt::StackUnderflow); // STORE, empty
        assert_eq!(run_on(&mut host, &[0x70]), Halt::StackUnderflow); // LOAD, empty
    }

    #[test]
    fn door_example_bytes_clear_the_wall() {
        // The ROADMAP door script: push X; push Y; clr_wall; halt = 10 04 10 03 32 01.
        let mut host = TestHost::new(8, 8);
        host.set_wall(4, 3, true);
        assert_eq!(run_on(&mut host, &[0x10, 0x04, 0x10, 0x03, 0x32, 0x01]), Halt::Halt);
        assert!(!host.get_wall(4, 3), "the door bit is cleared");
    }
}
