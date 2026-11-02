## Summary
<!-- What does this PR do and why? -->

## Inventory items
<!-- e.g. LNG-MSG-014, LNG-MSG-015. Tick them in docs/inventory in this PR. -->

## Spec references
<!-- e.g. docs/specs/lounge-chat.md §6.2, D-121 -->

## How it was tested
- [ ] Rust unit/integration tests
- [ ] Web component tests
- [ ] E2E (Playwright), if user-visible
- [ ] Manually in Chrome / Firefox / Safari / mobile PWA

## Checklist
- [ ] `cargo fmt`, `cargo clippy -D warnings`, `cargo test` pass
- [ ] `pnpm lint`, `pnpm typecheck`, `pnpm test` pass
- [ ] Permissions enforced server-side
- [ ] Visual check (AGENTS.md section 4): new routes added to `e2e/tests/page-sweep.spec.ts`; every changed page shot at desktop and phone, light and dark, looked at and fixed (screenshot folder in the handoff), or this PR changes no visible UI
- [ ] Route matrix rows verified: new or changed routes have a hand-checked AUTHZ value in `crates/sr-api/tests/route_matrix.tsv` (no `?`; vocabulary in `route_matrix.rs`), or this PR adds no routes
- [ ] Docs updated (spec, API, decisions log if a decision changed)
- [ ] No new non-Rust runtime dependency (or ADR added)

## Open questions
<!-- Assumptions made where the spec was silent (Discord-parity default chosen). -->
