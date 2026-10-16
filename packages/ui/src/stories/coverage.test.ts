import { describe, expect, it } from "vitest";
import * as ui from "../index";

// DS-PRIM-016: "Ladle stories for every primitive". The axe run in stories.test.tsx covers every story that
// exists; this one makes sure a component cannot be exported without one. A component that is shown only
// inside another one's story, or that needs the whole app around it, goes in EXEMPT with the reason.
const EXEMPT: Record<string, string> = {
  MenuSurface: "the open panel inside DropdownMenu and ContextMenu, which have stories; it needs a floating anchor",
};

// `import.meta.glob` is Vite's (it rewrites the call at transform time); this package has no Vite types, so the
// one signature used here is declared.
declare global {
  interface ImportMeta {
    glob<T>(pattern: string, options: { query: string; import: string; eager: true }): Record<string, T>;
  }
}
const source = Object.values(import.meta.glob<string>("./*.stories.tsx", { query: "?raw", import: "default", eager: true })).join("\n");

/** Whole-word match, written with character classes so it needs no escapes. */
const mentions = (name: string) => new RegExp(`(^|[^A-Za-z0-9_])${name}([^A-Za-z0-9_]|$)`).test(source);

const components = Object.entries(ui as Record<string, unknown>)
  .filter(([name, value]) => /^[A-Z][a-z]/.test(name) && (typeof value === "function" || (typeof value === "object" && value !== null)))
  .map(([name]) => name)
  .sort();

describe("story coverage (DS-PRIM-016)", () => {
  it("finds the exported components", () => {
    expect(components.length).toBeGreaterThan(60);
    expect(components).toContain("Button");
  });

  it.each(components)("%s appears in a story", (name) => {
    if (name in EXEMPT) return;
    expect(mentions(name), `${name} has no story: add it to a *.stories.tsx file`).toBe(true);
  });

  it("keeps the exemption list honest", () => {
    for (const name of Object.keys(EXEMPT)) {
      expect(components, `${name} is exempt but no longer exported`).toContain(name);
      expect(mentions(name), `${name} now has a story; drop the exemption`).toBe(false);
    }
  });
});
