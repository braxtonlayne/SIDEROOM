# Wave 103 pilot handoff (D-985)

Tools builder: no repository file changed except this handoff. The six answers and the commands are in
`C:/Users/braxt/Desktop/SIDEROOM_wt/p103-pilot-report.md`. New scripts (outside the repository):
`cli-agents/world.sh`, `cli-agents/cell.sh`; scratch output in `visual-sweep-shots`-style folder `p103-pilot-shots/`.

## Answers in one line each
1. Built mode: yes. Build 18 s; seed 34 s, no FAIL; needs `SR_E2E_SERVER_PORT=<port>`.
2. Clone: yes, 1.6 s; `p102-logs-shots` (after) passes on the clone and on the seeded database with the same measures.
3. Second port: yes; `p102-arcadm-shots` (after) passes on 8814. The state holds no origin (the cookie has no port); the rewrite is not needed.
4. A second worktree gives its own `e2e/.state`; both cells up together; simultaneous specs NOT run (one slot only, 2.2 GB free).
5. Dev vs built: Library Browse toolbar identical; the Members filter row wraps differently (unattributed, the p102 shot is of an older tree); Lounge main view whole.
6. Cell cost: server about 0.14 GB idle; Chromium not isolated from the other builders' (0.5 GB machine total); not measured to the digit.

## Clock and rounds (STACK-ROUND)
- Start 12:05. Scripts written 12:06 (`bash -n` clean). Acquire retries 12:06 to 12:55.
- Round 1: 12:55 to 12:58, `status` line `pilot 12:55`: build, seed, clone a and b, spec on the clone and on the template.
- Round 2: 13:38 to 13:41, `status` line `pilot 13:38`: both cells, arcadm on port 8814, three built shots; released 13:40.
- `heavy.sh status` at the end: free (no hold in my name).

## TYPECHECK
Not needed (no `.ts`/`.tsx` edited).

## Clean-up
Databases `sr_p103_world`, `sr_p103_pilot_a`, `sr_p103_pilot_b` dropped; the lane 2 worktree removed and pruned;
`p103-pilot-bin`, the state copies and the cell folders deleted; ports 8813, 8814, 9513, 9514 free; this worktree's
`apps/web/dist` and `e2e/.state` removed. `p103-pilot-shots/` (logs and the three built PNGs) kept for the commander.
