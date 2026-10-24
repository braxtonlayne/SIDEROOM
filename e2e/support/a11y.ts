import { createRequire } from 'node:module';
import type { Page } from '@playwright/test';

/**
 * Shared axe helper for the signed-in specs (RDY-A11Y-001). axe-core is already an e2e dependency; it is
 * injected as an inline script, which the app's own CSP refuses, so callers set `test.use({ bypassCSP: true })`.
 */
const axeSource = createRequire(import.meta.url).resolve('axe-core/axe.min.js');

export type Impact = 'minor' | 'moderate' | 'serious' | 'critical' | null;

export interface Finding {
  id: string;
  impact: Impact;
  help: string;
  targets: string[];
  /** Axe's own explanation for each failing node (colours, ratios), for the handoff. */
  details: string[];
}

export const AXE_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'];

export async function runAxe(page: Page, include?: string): Promise<Finding[]> {
  const loaded = await page.evaluate(() => 'axe' in window);
  if (!loaded) await page.addScriptTag({ path: axeSource });
  return page.evaluate(
    async ({ tags, scope }) => {
      const axe = (
        window as unknown as {
          axe: {
            run: (
              c: Document | string,
              o: object,
            ) => Promise<{
              violations: {
                id: string;
                impact: Impact;
                help: string;
                nodes: { target: unknown[]; any: { message: string }[]; none: { message: string }[] }[];
              }[];
            }>;
          };
        }
      ).axe;
      const r = await axe.run(scope ?? document, { runOnly: { type: 'tag', values: tags } });
      return r.violations.map((v) => ({
        id: v.id,
        impact: v.impact,
        help: v.help,
        targets: v.nodes.map((n) => n.target.flat(3).join(' ')),
        details: v.nodes.map((n) => [...n.any, ...n.none].map((c) => c.message).join('; ')),
      }));
    },
    { tags: AXE_TAGS, scope: include ?? null },
  );
}

export const blocking = (f: Finding[]) =>
  f.filter((v) => v.impact === 'serious' || v.impact === 'critical');

export const describe = (f: Finding[]) =>
  f.map((v) => `[${v.impact}] ${v.id}: ${v.help} -> ${v.targets.slice(0, 4).join(' | ')}`);
