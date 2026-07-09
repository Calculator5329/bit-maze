; trial gate: the pressure plate at (3,1) opens the gate at (5,3), joining the
; open room to the walled-off right column so its items become reachable. The
; plate is bound to trigger id 128 (0x80) in levels/trial.bm, so it is a Phase 8
; *one-shot* trigger — the high-bit id means it fires only the first time the
; player steps on it (see docs/VM.md "trigger firing semantics"). The script
; itself is the same idempotent clr_wall a repeating plate would use.
on_plate:
    push 5
    push 3
    clr_wall        ; open the gate at (5,3)
    halt
