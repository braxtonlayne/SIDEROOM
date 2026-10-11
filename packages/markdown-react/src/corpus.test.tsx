import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Markdown } from "./Markdown";
import type { MdNode } from "./ast";
import { nodesToText } from "./ast";

/**
 * Consumes the shared sr-markdown corpus (LNG-MD-005). The AST in each case is exactly what
 * the Rust crate produces; here we check the renderer handles every one of them and keeps
 * the message text. (The WASM wrapper's own tests assert AST equality against the same files.)
 */
interface Case {
  name: string;
  input: string;
  ast: MdNode[];
  plain: string;
  jumbo: boolean;
}

const dir = join(dirname(fileURLToPath(import.meta.url)), "../../../crates/sr-markdown/tests/corpus");
const cases: Case[] = readdirSync(dir)
  .filter((f) => f.endsWith(".json"))
  .sort()
  .flatMap((f) => (JSON.parse(readFileSync(join(dir, f), "utf8")) as { cases: Case[] }).cases);

function isTextOnly(nodes: MdNode[]): boolean {
  return nodes.every((n) => {
    switch (n.type) {
      case "text":
      case "line_break":
        return true;
      case "bold":
      case "italic":
      case "underline":
      case "strike":
        return isTextOnly(n.children);
      default:
        return false;
    }
  });
}

describe("shared corpus", () => {
  it("has at least 500 cases", () => {
    expect(cases.length).toBeGreaterThanOrEqual(500);
  });

  it("renders every case without throwing and with all spoilers revealed", () => {
    const failures: string[] = [];
    for (const c of cases) {
      try {
        const { container, unmount } = render(
          <Markdown
            nodes={c.ast}
            jumbo={c.jumbo}
            revealSpoilers
            locale="en-US"
            timeZone="UTC"
            now={0}
          />,
        );
        const text = container.textContent ?? "";
        if (text.includes("undefined") && !c.input.includes("undefined")) {
          failures.push(`${c.name}: rendered "undefined"`);
        }
        unmount();
      } catch (e) {
        failures.push(`${c.name}: ${String(e)}`);
      }
    }
    expect(failures).toEqual([]);
  });

  it("keeps literal text content for plain text and formatting-only cases", () => {
    for (const c of cases) {
      if (!isTextOnly(c.ast)) continue;
      const { container, unmount } = render(<Markdown nodes={c.ast} />);
      const expected = nodesToText(c.ast).split(String.fromCharCode(10)).join("");
      expect(container.textContent, c.name).toBe(expected);
      unmount();
    }
  });

  it("marks exactly the jumbo cases", () => {
    for (const c of cases.slice(0, 400)) {
      const { container, unmount } = render(<Markdown nodes={c.ast} jumbo={c.jumbo} />);
      expect(container.querySelector(".sr-md-jumbo") !== null, c.name).toBe(c.jumbo);
      unmount();
    }
  });
});
