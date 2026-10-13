import { render, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CodeBlock, highlightCode, resolveLanguage } from "./highlight";

describe("resolveLanguage", () => {
  it("maps aliases and ignores case", () => {
    expect(resolveLanguage("JS")).toBe("javascript");
    expect(resolveLanguage("rs")).toBe("rust");
    expect(resolveLanguage("C++")).toBe("cpp");
    expect(resolveLanguage(" yml ")).toBe("yaml");
    expect(resolveLanguage("console")).toBe("bash");
  });

  it("returns null for languages outside the shipped set", () => {
    expect(resolveLanguage("brainfuck")).toBeNull();
    expect(resolveLanguage("")).toBeNull();
  });
});

describe("highlightCode", () => {
  it("tokenises a known language with a colour per theme", async () => {
    const lines = await highlightCode("fn main() {\n    let x = 1;\n}", "rust");
    expect(lines).not.toBeNull();
    expect(lines).toHaveLength(3);
    const flat = lines!.flat();
    expect(flat.map((t) => t.content).join("")).toBe("fn main() {    let x = 1;}");
    expect(flat.some((t) => t.light && t.dark && t.light !== t.dark)).toBe(true);
  });

  it("highlights the other shipped grammars without throwing", async () => {
    const samples: [string, string][] = [
      ["typescript", "const a: number = 1;"],
      ["python", "def f():\n    return 1"],
      ["json", '{"a": [1, 2]}'],
      ["bash", "echo $HOME | grep x"],
      ["sql", "select * from t where a = 1;"],
      ["html", "<div class='a'>x</div>"],
      ["css", "a { color: red; }"],
      ["yaml", "a:\n  - b"],
      ["diff", "- a\n+ b"],
    ];
    for (const [lang, code] of samples) {
      const lines = await highlightCode(code, lang);
      expect(lines, lang).not.toBeNull();
      expect(lines!.flat().map((t) => t.content).join(""), lang).toBe(code.replace(/\n/g, ""));
    }
  }, 30_000);

  it("returns null for an unknown language", async () => {
    expect(await highlightCode("x", "nope-lang")).toBeNull();
  });
});

describe("CodeBlock", () => {
  it("shows the plain code first and the highlighted tokens after the grammar loads", async () => {
    const code = "let answer = 42;";
    const { container } = render(<CodeBlock lang="js" code={code} />);
    const pre = container.querySelector("pre")!;
    expect(pre.textContent).toBe(code);
    await waitFor(() => expect(pre).toHaveAttribute("data-highlighted", "true"));
    expect(pre.textContent).toBe(code);
    expect(pre.querySelectorAll(".sr-md-tok").length).toBeGreaterThan(2);
  });

  it("stays plain for unknown languages and when there is no language", async () => {
    const a = render(<CodeBlock lang="nope-lang" code="x y" />);
    const b = render(<CodeBlock lang={null} code="x y" />);
    await new Promise((r) => setTimeout(r, 50));
    expect(a.container.querySelector("pre")).not.toHaveAttribute("data-highlighted");
    expect(b.container.querySelector("pre")).not.toHaveAttribute("data-highlighted");
  });
});
