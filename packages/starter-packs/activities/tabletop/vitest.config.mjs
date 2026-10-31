// Tests run with the repo's vitest, from a workspace package that has it, for example
// `cd packages/embedded-app-sdk && pnpm exec vitest run --root ../starter-packs/activities/tabletop`.
// Globals avoid a bare `vitest` import, which this folder (outside the workspace) could not resolve.
export default { test: { environment: 'node', globals: true, include: ['tests/**/*.test.js'] } };
