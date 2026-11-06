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
