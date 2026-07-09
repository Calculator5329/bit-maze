; countdown loop: start at 3, decrement to 0, then halt.
; Exercises a label with both jz (forward) and jmp (backward).
    push 3
top:
    push 1
    sub             ; n = n - 1
    dup
    jz done         ; if n == 0, exit the loop
    jmp top         ; else loop again
done:
    halt
