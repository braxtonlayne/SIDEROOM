import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Markdown } from "./Markdown";
import { formatRelative, formatTimestamp } from "./timestamp";
import type { MdNode } from "./ast";
import { nodesToText, parseAstJson } from "./ast";

const t = (text: string): MdNode => ({ type: "text", text });

describe("basic nodes", () => {
  it("renders inline formatting", () => {
    const { container } = render(
      <Markdown
        nodes={[
          { type: "bold", children: [t("b")] },
          { type: "italic", children: [t("i")] },
          { type: "underline", children: [t("u")] },
          { type: "strike", children: [t("s")] },
          { type: "inline_code", code: "c" },
          { type: "line_break" },
        ]}
      />,
    );
    expect(container.querySelector("strong")).toHaveTextContent("b");
    expect(container.querySelector("em")).toHaveTextContent("i");
    expect(container.querySelector("u")).toHaveTextContent("u");
    expect(container.querySelector("s")).toHaveTextContent("s");
    expect(container.querySelector("code")).toHaveTextContent("c");
    expect(container.querySelector("br")).not.toBeNull();
  });

  it("renders headings, subtext, quotes and lists", () => {
    const { container } = render(
      <Markdown
        nodes={[
          { type: "heading", level: 2, children: [t("h")] },
          { type: "subtext", children: [t("s")] },
          { type: "quote", children: [t("q")] },
          { type: "list", ordered: true, start: 3, items: [[t("a")], [t("b")]] },
          { type: "list", ordered: false, start: null, items: [[t("c")]] },
        ]}
      />,
    );
    expect(container.querySelector("h2")).toHaveTextContent("h");
    expect(container.querySelector("small")).toHaveTextContent("s");
    expect(container.querySelector("blockquote")).toHaveTextContent("q");
    expect(container.querySelector("ol")).toHaveAttribute("start", "3");
    expect(container.querySelectorAll("li")).toHaveLength(3);
    expect(container.querySelector("ul")).toHaveTextContent("c");
  });

  it("ignores unknown node types", () => {
    const nodes = [t("a"), { type: "from_the_future", x: 1 } as unknown as MdNode, t("b")];
    const { container } = render(<Markdown nodes={nodes} />);
    expect(container).toHaveTextContent("ab");
  });
});

describe("code blocks", () => {
  it("renders language class and raw content", () => {
    const { container } = render(
      <Markdown nodes={[{ type: "code_block", lang: "rust", code: "fn x() {}\n<b>" }]} />,
    );
    const code = container.querySelector("pre code");
    expect(code).toHaveClass("language-rust");
    expect(code?.textContent).toBe("fn x() {}\n<b>");
    expect(container.querySelector("b")).toBeNull();
  });

  it("delegates to the highlighter hook", () => {
    render(
      <Markdown
        nodes={[{ type: "code_block", lang: "js", code: "x" }]}
        renderCodeBlock={({ lang, code }) => <div data-testid="hl">{`${lang}:${code}`}</div>}
      />,
    );
    expect(screen.getByTestId("hl")).toHaveTextContent("js:x");
  });
});

describe("spoilers", () => {
  const nodes: MdNode[] = [{ type: "spoiler", children: [t("secret")] }];

  it("hides content until clicked", async () => {
    render(<Markdown nodes={nodes} />);
    const btn = screen.getByRole("button", { name: /spoiler/i });
    expect(btn).toHaveAttribute("data-revealed", "false");
    expect(screen.getByText("secret")).toHaveAttribute("aria-hidden", "true");
    await userEvent.click(btn);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("secret").closest(".sr-md-spoiler")).toHaveAttribute(
      "data-revealed",
      "true",
    );
  });

  it("reveals with the keyboard", async () => {
    render(<Markdown nodes={nodes} />);
    screen.getByRole("button").focus();
    await userEvent.keyboard("{Enter}");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("reveals with space", async () => {
    render(<Markdown nodes={nodes} />);
    screen.getByRole("button").focus();
    await userEvent.keyboard(" ");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("can be force-revealed", () => {
    render(<Markdown nodes={nodes} revealSpoilers />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("reveals nested spoilers independently", async () => {
    const nested: MdNode[] = [
      { type: "spoiler", children: [t("outer "), { type: "spoiler", children: [t("inner")] }] },
    ];
    render(<Markdown nodes={nested} />);
    await userEvent.click(screen.getByRole("button"));
    const inner = screen.getByRole("button");
    expect(inner).toHaveTextContent("inner");
    await userEvent.click(inner);
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("mentions", () => {
  const nodes: MdNode[] = [
    { type: "user_mention", id: "1" },
    { type: "role_mention", id: "2" },
    { type: "channel_mention", id: "3" },
    { type: "everyone" },
    { type: "here" },
  ];

  it("renders placeholders without resolvers", () => {
    const { container } = render(<Markdown nodes={nodes} />);
    expect(container).toHaveTextContent("@unknown-user@deleted-role#unknown-channel@everyone@here");
    expect(container.querySelector('[data-id="1"]')).toHaveAttribute("data-unresolved", "true");
  });

  it("uses injected resolvers", () => {
    const user = vi.fn((id: string) => (id === "1" ? { name: "alice", color: "red" } : undefined));
    render(
      <Markdown
        nodes={nodes}
        resolvers={{
          user,
          role: () => ({ name: "mods" }),
          channel: () => ({ name: "general" }),
        }}
      />,
    );
    expect(user).toHaveBeenCalledWith("1");
    const alice = screen.getByText("@alice");
    expect(alice).toHaveStyle({ color: "rgb(255, 0, 0)" });
    expect(screen.getByText("@mods")).toBeInTheDocument();
    expect(screen.getByText("#general")).toBeInTheDocument();
  });

  it("keeps large snowflakes exact as strings", () => {
    const user = vi.fn(() => ({ name: "x" }));
    render(
      <Markdown
        nodes={[{ type: "user_mention", id: "18446744073709551615" }]}
        resolvers={{ user }}
      />,
    );
    expect(user).toHaveBeenCalledWith("18446744073709551615");
  });

  it("fires the click hook", async () => {
    const onMentionClick = vi.fn();
    render(<Markdown nodes={nodes} onMentionClick={onMentionClick} />);
    await userEvent.click(screen.getByText("@unknown-user"));
    expect(onMentionClick).toHaveBeenCalledWith({ kind: "user", id: "1" });
  });
});

describe("emoji", () => {
  it("renders custom emoji via the resolver", () => {
    render(
      <Markdown
        nodes={[{ type: "custom_emoji", name: "blob", id: 9, animated: true }]}
        resolvers={{ customEmojiUrl: (id, a) => `/e/${id}.${a ? "gif" : "png"}` }}
      />,
    );
    const img = screen.getByAltText(":blob:");
    expect(img).toHaveAttribute("src", "/e/9.gif");
    expect(img).toHaveAttribute("data-animated", "true");
  });

  it("falls back to text for custom emoji without a resolver", () => {
    const { container } = render(
      <Markdown nodes={[{ type: "custom_emoji", name: "blob", id: 9, animated: false }]} />,
    );
    expect(container).toHaveTextContent(":blob:");
  });

  it("renders unicode shortcodes accessibly", () => {
    render(<Markdown nodes={[{ type: "emoji", name: "thumbs_up", emoji: "👍" }]} />);
    expect(screen.getByRole("img", { name: "thumbs up" })).toHaveTextContent("👍");
  });

  it("marks jumbo messages", () => {
    const { container } = render(
      <Markdown jumbo nodes={[{ type: "emoji", name: "fire", emoji: "🔥" }]} />,
    );
    expect(container.firstChild).toHaveClass("sr-md-jumbo");
    expect(container.firstChild).toHaveAttribute("data-jumbo", "true");
  });
});

describe("timestamps", () => {
  const opts = { locale: "en-US", timeZone: "UTC", now: 1_700_000_000_000 };

  it("formats each style", () => {
    const u = 1_700_000_000;
    expect(formatTimestamp(u, "short_time", opts)).toBe("10:13 PM");
    expect(formatTimestamp(u, "long_time", opts)).toBe("10:13:20 PM");
    expect(formatTimestamp(u, "short_date", opts)).toBe("11/14/2023");
    expect(formatTimestamp(u, "long_date", opts)).toBe("November 14, 2023");
    expect(formatTimestamp(u, null, opts)).toContain("November 14, 2023");
    expect(formatTimestamp(u, "long_date_time", opts)).toContain("Tuesday");
  });

  it("formats relative times", () => {
    expect(formatRelative(1_700_000_000 - 3600 * 3, opts)).toBe("3 hours ago");
    expect(formatRelative(1_700_000_000 + 86400 * 2, opts)).toBe("in 2 days");
    expect(formatRelative(1_700_000_000, opts)).toBe("now");
    expect(formatRelative(1_700_000_000 - 86400 * 800, opts)).toBe("2 years ago");
  });

  it("re-renders relative times once a minute (shared ticker)", () => {
    vi.useFakeTimers({ now: 1_700_000_000_000 });
    try {
      const { container } = render(
        <Markdown nodes={[{ type: "timestamp", unix: 1_700_000_000 - 3600, style: "relative" }]} />,
      );
      expect(container.querySelector("time")).toHaveTextContent("1 hour ago");
      act(() => {
        vi.advanceTimersByTime(3_600_000);
      });
      expect(container.querySelector("time")).toHaveTextContent("2 hours ago");
    } finally {
      vi.useRealTimers();
    }
  });

  it("renders a <time> element with a title", () => {
    const { container } = render(
      <Markdown nodes={[{ type: "timestamp", unix: 1_700_000_000, style: "short_date" }]} {...opts} />,
    );
    const el = container.querySelector("time");
    expect(el).toHaveTextContent("11/14/2023");
    expect(el).toHaveAttribute("datetime", "2023-11-14T22:13:20.000Z");
    expect(el?.getAttribute("title")).toContain("Tuesday");
  });

  it("does not throw on out-of-range values", () => {
    const { container } = render(
      <Markdown nodes={[{ type: "timestamp", unix: 999_999_999_999_999, style: null }]} />,
    );
    expect(container).toHaveTextContent("Invalid Date");
  });
});

describe("links", () => {
  const masked: MdNode[] = [
    {
      type: "link",
      url: "https://evil.example/x",
      kind: "masked",
      children: [t("https://good.example")],
    },
  ];

  it("renders safe attributes", () => {
    render(<Markdown nodes={masked} />);
    const a = screen.getByRole("link");
    expect(a).toHaveAttribute("href", "https://evil.example/x");
    expect(a).toHaveAttribute("target", "_blank");
    expect(a.getAttribute("rel")).toContain("noopener");
    expect(a).toHaveAttribute("title", "https://evil.example/x");
  });

  it("does not render non-http urls as anchors", () => {
    render(
      <Markdown
        nodes={[{ type: "link", url: "javascript:alert(1)", kind: "masked", children: [t("x")] }]}
      />,
    );
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("asks for confirmation before opening masked links", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const confirmLink = vi.fn().mockResolvedValue(true);
    render(<Markdown nodes={masked} confirmLink={confirmLink} />);
    await userEvent.click(screen.getByRole("link"));
    expect(confirmLink).toHaveBeenCalledWith({
      url: "https://evil.example/x",
      kind: "masked",
      label: "https://good.example",
    });
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith("https://evil.example/x", "_blank", "noopener,noreferrer"),
    );
    open.mockRestore();
  });

  it("does not open when the confirmation is declined", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const confirmLink = vi.fn().mockResolvedValue(false);
    render(<Markdown nodes={masked} confirmLink={confirmLink} />);
    await userEvent.click(screen.getByRole("link"));
    await waitFor(() => expect(confirmLink).toHaveBeenCalled());
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });

  it("does not gate auto links", async () => {
    const confirmLink = vi.fn().mockResolvedValue(false);
    render(
      <Markdown
        nodes={[
          { type: "link", url: "https://a.example", kind: "auto", children: [t("https://a.example")] },
        ]}
        confirmLink={confirmLink}
      />,
    );
    const a = screen.getByRole("link");
    a.addEventListener("click", (e) => e.preventDefault());
    await userEvent.click(a);
    expect(confirmLink).not.toHaveBeenCalled();
  });
});

describe("SIDE ROOM tokens", () => {
  it("renders cards from resolvers and chips otherwise", () => {
    const { rerender } = render(
      <Markdown nodes={[{ type: "cinema", id: "tt1" }, { type: "library", id: "s1" }]} />,
    );
    expect(screen.getByText("Cinema title")).toHaveAttribute("data-id", "tt1");
    expect(screen.getByText("Library series")).toHaveAttribute("data-id", "s1");
    rerender(
      <Markdown
        nodes={[{ type: "cinema", id: "tt1" }]}
        resolvers={{ cinema: (id) => <div data-testid="card">{id}</div> }}
      />,
    );
    expect(screen.getByTestId("card")).toHaveTextContent("tt1");
  });

  it("renders slash commands and guild navigation", () => {
    const { container } = render(
      <Markdown
        nodes={[
          { type: "slash_command", name: "play", id: 1 },
          { type: "guild_nav", kind: "customize", id: null },
        ]}
      />,
    );
    expect(container).toHaveTextContent("/playCustomize");
  });
});

describe("ast helpers", () => {
  it("keeps ids above 2^53 exact through JSON", () => {
    const big = "18446744073709551615";
    const nodes = parseAstJson(`[{"type":"user_mention","id":"${big}"}]`);
    const user = vi.fn(() => ({ name: "x" }));
    render(<Markdown nodes={nodes} resolvers={{ user }} />);
    expect(user).toHaveBeenCalledWith(big);
  });

  it("parses AST JSON defensively", () => {
    expect(parseAstJson('[{"type":"text","text":"a"}]')).toHaveLength(1);
    expect(parseAstJson("nope")).toEqual([]);
    expect(parseAstJson('{"a":1}')).toEqual([]);
  });

  it("extracts plain text", () => {
    expect(
      nodesToText([
        t("a "),
        { type: "bold", children: [t("b")] },
        { type: "emoji", name: "fire", emoji: "🔥" },
      ]),
    ).toBe("a b🔥");
  });
});
