; vault plate: count how many times the plate has been pressed this run in
; scratch RAM (address 0), then open the vault wall at (5,3) so the item behind
; it can be collected. Exercises the Phase 7 LOAD/STORE opcodes (0x70/0x71).
on_plate:
    push 0
    load            ; visits = ram[0]
    push 1
    add             ; visits + 1
    push 0
    store           ; ram[0] = visits + 1
    push 5
    push 3
    clr_wall        ; open the vault
    halt
