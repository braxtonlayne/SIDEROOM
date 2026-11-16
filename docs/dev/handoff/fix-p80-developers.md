# Wave 80 handoff: developers (SEC-AUD-324, D-825)

T = 07:30 on 2026-11-16. Clock times: no new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30.

## State (draft, updated as the work goes)
- Code written and committed: `developers.css` (select rule, phone nav), `DevelopersRoot.tsx` (the `.tsx` that renders `.dev-nav`), `e2e/tests/p80-developers-shots.spec.ts`.
- Web typecheck clean (with bars' hook copied in), e2e `tsc` clean, `check-css-tokens` clean, `portal.test.tsx` (renders the root) 45 passed.
- Bars' file `apps/web/src/study/useCurrentLinkInView.ts` was copied from `p80-bars` into this worktree for typecheck and runs; it is NOT committed (never `git add`ed).
- Lock round: pending (waiting for `p80-base-bin/BUILT_FROM.txt` and the lock order).
