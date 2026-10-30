// SEC-ENT-004 / D-385: the Vite config `playwright.static.config.ts` previews the production build with. It is the
// web app's own config plus the preview gate (apps/web/vite/previewGate.ts), which picks the right shell for a path
// now that the build has one per public surface and an app shell. Kept here so the app's config stays the build's.
import base from '../apps/web/vite.config';
import { previewGate } from '../apps/web/vite/previewGate';

type Resolved = { plugins?: unknown[]; build?: Record<string, unknown> } & Record<string, unknown>;
type Factory = (env: { command: string; mode: string }) => Resolved | Promise<Resolved>;

export default async (env: { command: string; mode: string }) => {
  const cfg = base as unknown as Resolved | Factory;
  const resolved = typeof cfg === 'function' ? await cfg(env) : cfg;
  // The app build writes to dist/app; preview serves the whole dist (the public shells and the app shell).
  return {
    ...resolved,
    build: { ...resolved.build, outDir: 'dist' },
    plugins: [...(resolved.plugins ?? []), previewGate()],
  };
};
