import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu, DropdownMenu, type MenuEntry } from "./Menu/Menu";

function Harness({ onSelect = () => {}, onTheme = () => {} }: { onSelect?: (id: string) => void; onTheme?: (v: string) => void }) {
  const [checked, setChecked] = useState(false);
  const [theme, setTheme] = useState("dark");
  const items: MenuEntry[] = [
    { label: "Copy", onSelect: () => onSelect("copy") },
    { label: "Paste", disabled: true, onSelect: () => onSelect("paste") },
    { type: "checkbox", label: "Show hidden", checked, onCheckedChange: setChecked, keepOpen: true },
    { type: "separator" },
    {
      type: "radio-group",
      label: "Theme",
      value: theme,
      onValueChange: (v) => {
        setTheme(v);
        onTheme(v);
      },
      options: [
        { value: "dark", label: "Dark" },
        { value: "light", label: "Light" },
      ],
    },
    {
      type: "submenu",
      label: "Share",
      items: [
        { label: "Email", onSelect: () => onSelect("email") },
        { type: "submenu", label: "More", items: [{ label: "Deep", onSelect: () => onSelect("deep") }] },
      ],
    },
    { label: "Delete", danger: true, onSelect: () => onSelect("delete") },
  ];
  return <DropdownMenu label="Actions" items={items} trigger={<button>Open</button>} />;
}

describe("DropdownMenu", () => {
  it("opens with aria wiring, focuses first enabled item, and closes on Escape returning focus", async () => {
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Open" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(trigger);
    const menu = screen.getByRole("menu", { name: "Actions" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(trigger).toHaveAttribute("aria-controls", menu.id);
    expect(screen.getByRole("menuitem", { name: "Copy" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("opens with ArrowDown on the trigger", async () => {
    render(<Harness />);
    screen.getByRole("button", { name: "Open" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("navigates with arrows (skipping disabled, wrapping), Home/End and typeahead", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitemcheckbox", { name: /Show hidden/ })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Copy" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "Copy" })).toHaveFocus();
    await userEvent.keyboard("l");
    expect(screen.getByRole("menuitemradio", { name: "Light" })).toHaveFocus();
  });

  it("activates items with Enter and closes; disabled items do nothing", async () => {
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByRole("menuitem", { name: "Paste" })).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(screen.getByRole("menuitem", { name: "Paste" }));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledWith("copy");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("supports checkbox and radio items", async () => {
    const onTheme = vi.fn();
    render(<Harness onTheme={onTheme} />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const cb = screen.getByRole("menuitemcheckbox");
    expect(cb).toHaveAttribute("aria-checked", "false");
    await userEvent.click(cb);
    expect(screen.getByRole("menuitemcheckbox")).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menu")).toBeInTheDocument(); // keepOpen
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByRole("menuitemradio", { name: "Dark" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("menuitemradio", { name: "Light" }));
    expect(onTheme).toHaveBeenCalledWith("light");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens nested submenus with ArrowRight and closes them with ArrowLeft/Escape", async () => {
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const share = screen.getByRole("menuitem", { name: /Share/ });
    expect(share).toHaveAttribute("aria-haspopup", "menu");
    share.focus();
    await userEvent.keyboard("{ArrowRight}");
    const sub = screen.getByRole("menu", { name: "Share" });
    expect(share).toHaveAttribute("aria-expanded", "true");
    expect(within(sub).getByRole("menuitem", { name: "Email" })).toHaveFocus();

    await userEvent.keyboard("{ArrowDown}{ArrowRight}");
    expect(screen.getByRole("menu", { name: "More" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Deep" })).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.queryByRole("menu", { name: "More" })).toBeNull();
    expect(within(sub).getByRole("menuitem", { name: /More/ })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Share" })).toBeNull();
    expect(share).toHaveFocus();
    expect(screen.getByRole("menu", { name: "Actions" })).toBeInTheDocument();

    await userEvent.keyboard("{ArrowRight}{Enter}");
    expect(onSelect).toHaveBeenCalledWith("email");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens submenus on hover and closes on outside click", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.hover(screen.getByRole("menuitem", { name: /Share/ }));
    // Q60: submenus open after a 150 ms hover intent.
    expect(await screen.findByRole("menu", { name: "Share" })).toBeInTheDocument();
    await userEvent.hover(screen.getByRole("menuitem", { name: "Copy" }));
    expect(screen.queryByRole("menu", { name: "Share" })).toBeNull();
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("ContextMenu", () => {
  it("opens at the pointer on right-click and runs an item", async () => {
    const onSelect = vi.fn();
    render(
      <ContextMenu label="Message actions" items={[{ label: "Reply", onSelect }, { label: "Pin" }]}>
        <p>Hello world</p>
      </ContextMenu>,
    );
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.contextMenu(screen.getByText("Hello world"), { clientX: 40, clientY: 60 });
    const menu = screen.getByRole("menu", { name: "Message actions" });
    expect(menu.style.transform).toContain("translate(");
    expect(screen.getByRole("menuitem", { name: "Reply" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{ArrowUp}{Enter}");
    expect(onSelect).toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens on a 500 ms touch long-press and not on a short tap or a scroll", () => {
    vi.useFakeTimers();
    try {
      render(
        <ContextMenu label="Touch actions" items={[{ label: "Reply" }]}>
          <p>Touch me</p>
        </ContextMenu>,
      );
      const target = screen.getByText("Touch me");
      fireEvent.touchStart(target, { touches: [{ clientX: 10, clientY: 10 }] });
      act(() => vi.advanceTimersByTime(499));
      expect(screen.queryByRole("menu")).toBeNull();
      fireEvent.touchEnd(target);
      act(() => vi.advanceTimersByTime(600));
      expect(screen.queryByRole("menu")).toBeNull();

      fireEvent.touchStart(target, { touches: [{ clientX: 10, clientY: 10 }] });
      fireEvent.touchMove(target, { touches: [{ clientX: 40, clientY: 10 }] });
      act(() => vi.advanceTimersByTime(600));
      expect(screen.queryByRole("menu")).toBeNull();

      fireEvent.touchStart(target, { touches: [{ clientX: 10, clientY: 10 }] });
      act(() => vi.advanceTimersByTime(500));
      expect(screen.getByRole("menu", { name: "Touch actions" })).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does nothing when disabled", () => {
    render(
      <ContextMenu label="x" disabled items={[{ label: "A" }]}>
        <p>Region</p>
      </ContextMenu>,
    );
    fireEvent.contextMenu(screen.getByText("Region"));
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
