import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/** The z-index of the first rule for `selector` in a primitive's stylesheet. */
function layer(file: string, selector: string): number {
  const css = readFileSync(join(__dirname, file), "utf8");
  const start = css.indexOf(`${selector} {`);
  expect(start, `${selector} in ${file}`).toBeGreaterThanOrEqual(0);
  const rule = css.slice(start, css.indexOf("}", start));
  const m = /z-index:\s*(\d+)/.exec(rule);
  expect(m, `z-index of ${selector}`).not.toBeNull();
  return Number(m![1]);
}

describe("overlay layers (XC-VIS-004)", () => {
  it("a Select list opens above dialogs and drawers, below toasts", () => {
    const popup = layer("Select/Select.module.css", ".popup");
    expect(popup).toBeGreaterThan(layer("Modal/Modal.module.css", ".backdrop"));
    expect(popup).toBeGreaterThan(layer("Drawer/Drawer.module.css", ".backdrop"));
    expect(popup).toBeLessThan(layer("Feedback/Feedback.module.css", ".region"));
  });
});

describe("menus and popovers above dialogs (wave 51, the Select fix of wave 50 for the rest)", () => {
  const backdrop = () => Math.max(layer("Modal/Modal.module.css", ".backdrop"), layer("Drawer/Drawer.module.css", ".backdrop"));
  const toasts = () => layer("Feedback/Feedback.module.css", ".region");

  it("a Menu (dropdown, context menu, submenu) opened inside a dialog shows above its backdrop", () => {
    const menu = layer("Menu/Menu.module.css", ".menu");
    expect(menu).toBeGreaterThan(backdrop());
    expect(menu).toBeLessThan(toasts());
    // The same layer as the Select list, so a Select inside a menu or the other way round never hides.
    expect(menu).toBe(layer("Select/Select.module.css", ".popup"));
  });

  it("a Tooltip and a Popover opened inside a dialog show above its backdrop, below toasts", () => {
    const pop = layer("Overlay/Overlay.module.css", ".tooltip,\n.popover");
    expect(pop).toBeGreaterThan(backdrop());
    expect(pop).toBeLessThan(toasts());
  });
});

describe("the app overlays (D-554)", () => {
  const TOOLS = "../../../../apps/web/src/tools/tools.css";
  const PLAYER = "../../../../apps/web/src/player/player.css";

  it("rail 908 < gadgets 909 < dock 910, and the rail with its panel open comes above all three, below modals", () => {
    const rail = layer(TOOLS, ".sr-tl-rail-wrap");
    const gadgets = layer(TOOLS, ".sr-tl-gadgets");
    const dock = layer(PLAYER, ".sr-ply-dock");
    const open = layer(TOOLS, ".sr-tl-rail-wrap[data-open]");
    expect([rail, gadgets, dock]).toEqual([908, 909, 910]);
    expect(open).toBeGreaterThan(dock);
    expect(open).toBeLessThan(layer("Modal/Modal.module.css", ".backdrop"));
  });
});

describe("viewport maxima under zoom (D-555)", () => {
  // Inside a zoomed box a viewport unit is zoomed too: every 100vw / NNvh / 100dvh in these must be divided by --ui-zoom.
  const files = [
    "Modal/Modal.module.css",
    "Menu/Menu.module.css",
    "Drawer/Drawer.module.css",
    "Feedback/Feedback.module.css",
    "Overlay/Overlay.module.css",
  ];
  it.each(files)("%s divides its viewport lengths by --ui-zoom", (file) => {
    const css = readFileSync(join(__dirname, file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    const units = css.match(/\d+d?v[wh]\b(?:\s*\/\s*var\(--ui-zoom, 1\))?/g) ?? [];
    expect(units.length).toBeGreaterThan(0);
    for (const u of units) expect(u, `${file}: ${u}`).toMatch(/\/\s*var\(--ui-zoom, 1\)$/);
  });

  it("a dialog at 150 % on an 812 px phone fits the screen", () => {
    const css = readFileSync(join(__dirname, "Modal/Modal.module.css"), "utf8");
    const rule = css.slice(css.indexOf(".dialog {"), css.indexOf("}", css.indexOf(".dialog {")));
    const decls = [...rule.matchAll(/max-height:\s*calc\(([^;]+)\);/g)].map((m) => m[1]!);
    const last = decls[decls.length - 1]!; // the dvh one wins where supported
    // Evaluate in the dialog's own (zoomed) pixels: inside zoom z, 100dvh is the screen height in own pixels before
    // the zoom scales it again, so the drawn height is (own value) * z.
    const own = (h: number, z: number) =>
      Number(
        new Function(
          `return ${last.replace(/100dvh/g, String(h)).replace(/var\(--ui-zoom, 1\)/g, String(z)).replace(/px/g, "")};`,
        )(),
      );
    expect(own(812, 1.5) * 1.5).toBeLessThanOrEqual(812);
    expect(own(812, 1)).toBe(812 - 32);
  });
});
