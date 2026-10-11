import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Combobox, MultiSelect, Select, type SelectOption } from "./Select/Select";

const fruit: SelectOption[] = [
  { value: "apple", label: "Apple" },
  { value: "banana", label: "Banana", disabled: true },
  { value: "cherry", label: "Cherry" },
  { value: "date", label: "Date" },
];

describe("Select", () => {
  it("opens a listbox, navigates (skipping disabled) and selects with the keyboard", async () => {
    const onValueChange = vi.fn();
    render(<Select aria-label="Fruit" options={fruit} placeholder="Pick one" onValueChange={onValueChange} />);
    const box = screen.getByRole("combobox", { name: "Fruit" });
    expect(box).toHaveTextContent("Pick one");
    expect(box).toHaveAttribute("aria-expanded", "false");
    box.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(box).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(box).toHaveAttribute("aria-activedescendant", screen.getAllByRole("option")[0]?.id);
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("option", { name: /Cherry/ })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("{Enter}");
    expect(onValueChange).toHaveBeenCalledWith("cherry");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(box).toHaveTextContent("Cherry");
    expect(box).toHaveFocus();
  });

  it("selects by mouse, marks aria-selected, closes on Escape", async () => {
    render(<Select aria-label="Fruit" options={fruit} defaultValue="date" />);
    const box = screen.getByRole("combobox");
    await userEvent.click(box);
    expect(screen.getByRole("option", { name: /Date/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: /Banana/ })).toHaveAttribute("aria-disabled", "true");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    await userEvent.click(box);
    await userEvent.click(screen.getByRole("option", { name: /Apple/ }));
    expect(box).toHaveTextContent("Apple");
  });

  it("type-ahead moves the active option", async () => {
    render(<Select aria-label="Fruit" options={fruit} />);
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("c");
    expect(screen.getByRole("option", { name: /Cherry/ })).toHaveAttribute("data-active", "true");
  });

  it("virtualizes large option sets", async () => {
    const many = Array.from({ length: 5000 }, (_, i) => ({ value: `v${i}`, label: `Option ${i}` }));
    render(<Select aria-label="Big" options={many} />);
    await userEvent.click(screen.getByRole("combobox"));
    const rendered = screen.getAllByRole("option");
    expect(rendered.length).toBeGreaterThan(0);
    expect(rendered.length).toBeLessThan(40);
    expect(rendered[0]).toHaveAttribute("aria-setsize", "5000");
  });
});

describe("Combobox", () => {
  it("filters as you type, shows an empty state, and commits the choice", async () => {
    const onValueChange = vi.fn();
    render(<Combobox aria-label="Fruit" options={fruit} onValueChange={onValueChange} />);
    const input = screen.getByRole("combobox");
    await userEvent.click(input);
    expect(screen.getAllByRole("option")).toHaveLength(4);
    await userEvent.type(input, "ch");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    await userEvent.keyboard("{Enter}");
    expect(onValueChange).toHaveBeenCalledWith("cherry");
    expect(input).toHaveValue("Cherry");

    await userEvent.clear(input);
    await userEvent.type(input, "zzz");
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByRole("status")).toHaveTextContent("No results");
  });

  it("closes when focus leaves and restores the selected label", async () => {
    render(
      <>
        <Combobox aria-label="Fruit" options={fruit} defaultValue="apple" />
        <button>after</button>
      </>,
    );
    const input = screen.getByRole("combobox");
    await userEvent.click(input);
    await userEvent.type(input, "x");
    await userEvent.click(screen.getByText("after"));
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(input).toHaveValue("Apple");
  });
});

describe("MultiSelect", () => {
  function Harness() {
    const [values, setValues] = useState<string[]>(["apple"]);
    return <MultiSelect aria-label="Fruit" options={fruit} values={values} onValuesChange={setValues} />;
  }

  it("toggles several values, keeps the list open, and removes chips", async () => {
    render(<Harness />);
    const input = screen.getByRole("combobox");
    expect(screen.getByRole("button", { name: "Remove Apple" })).toBeInTheDocument();
    await userEvent.click(input);
    expect(screen.getByRole("listbox")).toHaveAttribute("aria-multiselectable", "true");
    await userEvent.click(screen.getByRole("option", { name: /Cherry/ }));
    await userEvent.click(screen.getByRole("option", { name: /Date/ }));
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove Cherry" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Date/ })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(screen.getByRole("option", { name: /Apple/ }));
    expect(screen.queryByRole("button", { name: "Remove Apple" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Remove Date" }));
    expect(screen.queryByRole("button", { name: "Remove Date" })).toBeNull();
  });

  it("Backspace on an empty query removes the last chip", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("combobox"));
    await userEvent.keyboard("{Backspace}");
    expect(screen.queryByRole("button", { name: "Remove Apple" })).toBeNull();
  });
});
