// Shared parser for docs/inventory/*.md. An item is a checkbox line:
//   - [x] **XC-PWA-001** [FE] Web app manifest ...
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const inventoryDir = join(root, 'docs', 'inventory');

const ITEM =
  /^- \[( |x|X)\] \*\*([A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-\d{3})\*\*\s*((?:\[[A-Z]+\]\s*)*)(.*)$/;

export function parseInventoryText(text, file = '') {
  const items = [];
  let section = '';
  for (const line of text.split(/\r?\n/)) {
    const h = /^##\s+(.*)$/.exec(line);
    if (h) section = h[1].trim();
    const m = ITEM.exec(line);
    if (!m) continue;
    items.push({
      id: m[2],
      done: m[1] !== ' ',
      tags: [...m[3].matchAll(/\[([A-Z]+)\]/g)].map((t) => t[1]),
      title: m[4].trim(),
      section,
      file,
    });
  }
  return items;
}

export function loadInventory() {
  return readdirSync(inventoryDir)
    .filter((f) => /^\d\d-.*\.md$/.test(f))
    .sort()
    .flatMap((f) => parseInventoryText(readFileSync(join(inventoryDir, f), 'utf8'), f));
}
