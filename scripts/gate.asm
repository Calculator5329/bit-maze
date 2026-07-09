; garden gate: stepping on the pressure plate at (2,2) opens the gate at (5,3),
; joining the two halves of the room so the far-side items become reachable.
; Assembles to: 10 05 10 03 32 01
on_plate:
    push 5
    push 3
    clr_wall        ; open the gate
    halt
