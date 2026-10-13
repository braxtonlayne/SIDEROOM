import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { axeViolations } from "../test-axe";
import { PermissionMatrix, TriStateToggle, type PermissionGroupDef, type TriState } from "./Permissions/Permissions";
import { SettingsLayout } from "./Settings/SettingsLayout";

const groups: PermissionGroupDef[] = [
  {
    id: "text",
    label: "Text",
    items: [
      { id: "send", label: "Send Messages", description: "Post in text channels." },
      { id: "embed", label: "Embed Links" },
    ],
  },
  { id: "adv", label: "Advanced", items: [{ id: "admin", label: "Administrator", description: "All powers.", danger: true }] },
];

describe("TriStateToggle", () => {
  it("is a radiogroup with a roving tabindex and arrow-key selection", async () => {
    const onChange = vi.fn();
    render(<TriStateToggle label="Send Messages" value="neutral" onChange={onChange} />);
    const group = screen.getByRole("radiogroup", { name: "Send Messages" });
    const radios = within(group).getAllByRole("radio");
    expect(radios).toHaveLength(3);
    expect(radios.map((r) => r.getAttribute("tabindex"))).toEqual(["-1", "0", "-1"]);
    expect(radios[1]).toHaveAttribute("aria-checked", "true");
    radios[1]!.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenLastCalledWith("allow");
    await userEvent.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenLastCalledWith("deny");
    await userEvent.click(within(group).getByRole("radio", { name: "Send Messages: Deny" }));
    expect(onChange).toHaveBeenLastCalledWith("deny");
  });
  it("does nothing when disabled", async () => {
    const onChange = vi.fn();
    render(<TriStateToggle label="X" value="allow" disabled onChange={onChange} />);
    await userEvent.click(screen.getByRole("radio", { name: "X: Deny" }));
    expect(onChange).not.toHaveBeenCalled();
  });
});

function Harness({ mode, lock }: { mode: "binary" | "tristate"; lock?: (id: string) => string | undefined }) {
  const [values, setValues] = useState<Record<string, TriState>>({});
  return (
    <>
      <PermissionMatrix groups={groups} values={values} mode={mode} onChange={(id, v) => setValues((s) => ({ ...s, [id]: v }))} {...(lock ? { lockedReason: lock } : {})} />
      <output data-testid="out">{JSON.stringify(values)}</output>
    </>
  );
}

describe("PermissionMatrix", () => {
  it("toggles binary permissions, groups them, and is axe clean", async () => {
    const { container } = render(<Harness mode="binary" />);
    expect(screen.getByRole("heading", { name: "Text" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: /Send Messages/ }));
    expect(screen.getByTestId("out")).toHaveTextContent('{"send":"allow"}');
    await userEvent.click(screen.getByRole("switch", { name: /Send Messages/ }));
    expect(screen.getByTestId("out")).toHaveTextContent('{"send":"neutral"}');
    expect(await axeViolations(container)).toEqual([]);
  });
  it("filters by label and description, with an empty state", async () => {
    render(<Harness mode="binary" />);
    const search = screen.getByRole("searchbox", { name: "Search permissions" });
    await userEvent.type(search, "all powers");
    expect(screen.getByRole("switch", { name: /Administrator/ })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /Send Messages/ })).toBeNull();
    await userEvent.clear(search);
    await userEvent.type(search, "zzz");
    expect(screen.getByText("No permissions match your search.")).toBeInTheDocument();
  });
  it("renders tri-state rows and locks rows with a reason", async () => {
    const { container } = render(<Harness mode="tristate" lock={(id) => (id === "admin" ? "You do not have this permission." : undefined)} />);
    await userEvent.click(screen.getByRole("radio", { name: "Embed Links: Deny" }));
    expect(screen.getByTestId("out")).toHaveTextContent('{"embed":"deny"}');
    expect(screen.getByRole("radio", { name: "Administrator: Allow" })).toBeDisabled();
    expect(screen.getByText("You do not have this permission.")).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("SettingsLayout (server settings additions)", () => {
  const sections = [
    { id: "a", label: "Overview", group: "G" },
    { id: "b", label: "Roles", group: "G" },
    { id: "c", label: "Bans", group: "H" },
  ];
  it("has one tab stop in the section list and moves with the arrow keys", async () => {
    render(
      <SettingsLayout inline title="Server settings" sections={sections} activeId="b" onSelect={() => {}} onClose={() => {}}>
        body
      </SettingsLayout>,
    );
    const tabbable = () => within(screen.getByRole("navigation", { name: "Server settings" })).getAllByRole("button").filter((b) => b.tabIndex === 0);
    expect(tabbable().map((b) => b.textContent)).toEqual(["Roles"]);
    screen.getByRole("button", { name: "Roles" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: "Bans" })).toHaveFocus();
    expect(tabbable().map((b) => b.textContent)).toEqual(["Bans"]);
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: "Overview" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("button", { name: "Bans" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("button", { name: "Overview" })).toHaveFocus();
  });
  it("shakes the bar instead of navigating when blockNavWhenDirty", async () => {
    const onSelect = vi.fn();
    render(
      <SettingsLayout inline dirty blockNavWhenDirty title="Server settings" sections={sections} activeId="a" onSelect={onSelect} onClose={() => {}}>
        body
      </SettingsLayout>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Roles" }));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveAttribute("data-nudge");
  });
  it("collapses the sections behind a toggle", async () => {
    render(
      <SettingsLayout inline title="Server settings" sections={sections} activeId="a" onSelect={() => {}} onClose={() => {}}>
        body
      </SettingsLayout>,
    );
    const toggle = screen.getByRole("button", { name: "Sections: Overview" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });
});
