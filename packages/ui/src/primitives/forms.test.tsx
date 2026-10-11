import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button/Button";
import { Checkbox, Radio, RadioGroup, Switch } from "./Controls/Controls";
import { PasswordInput, TextArea, TextInput, passwordStrength } from "./Input/TextInput";

describe("Button", () => {
  it("fires onClick and defaults to type=button", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Save</Button>);
    const b = screen.getByRole("button", { name: "Save" });
    expect(b).toHaveAttribute("type", "button");
    await userEvent.click(b);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("blocks activation while loading but stays focusable", async () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Save
      </Button>,
    );
    const b = screen.getByRole("button");
    expect(b).toHaveAttribute("aria-busy", "true");
    expect(b).not.toBeDisabled();
    await userEvent.click(b);
    await userEvent.keyboard("{Enter}");
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByRole("status", { name: "Loading" })).toBeInTheDocument();
  });

  it("does not fire when disabled", async () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Nope
      </Button>,
    );
    await userEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("icon-only buttons use aria-label", () => {
    render(
      <Button iconOnly aria-label="Close">
        <svg aria-hidden="true" />
      </Button>,
    );
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("applies variant data attribute", () => {
    render(<Button variant="danger">Delete</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-variant", "danger");
  });
});

describe("TextInput", () => {
  it("associates label, hint and error", () => {
    render(<TextInput label="Email" hint="We never share it" error="Required" />);
    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("We never share it Required");
  });
  it("is not marked invalid without error", () => {
    render(<TextInput label="Name" />);
    expect(screen.getByLabelText("Name")).not.toHaveAttribute("aria-invalid");
  });
  it("forwards refs", () => {
    const ref = { current: null as HTMLInputElement | null };
    render(<TextInput label="X" ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLInputElement);
  });
});

describe("TextArea", () => {
  it("emits changes and supports autoGrow without crashing", async () => {
    const onChange = vi.fn();
    render(<TextArea label="Bio" autoGrow onChange={onChange} />);
    await userEvent.type(screen.getByLabelText("Bio"), "hi");
    expect(onChange).toHaveBeenCalledTimes(2);
  });
});

describe("PasswordInput", () => {
  it("toggles visibility with an accessible pressed button", async () => {
    render(<PasswordInput label="Password" />);
    const input = screen.getByLabelText("Password");
    expect(input).toHaveAttribute("type", "password");
    const toggle = screen.getByRole("button", { name: "Show password" });
    await userEvent.click(toggle);
    expect(input).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "Hide password" })).toHaveAttribute("aria-pressed", "true");
  });
  it("shows a strength meter that updates", async () => {
    render(<PasswordInput label="Password" showStrength />);
    expect(screen.getByTestId("strength-label")).toHaveTextContent("Too weak");
    await userEvent.type(screen.getByLabelText("Password"), "Abcdefgh1!xyz");
    expect(screen.getByTestId("strength-label")).toHaveTextContent("Strong");
  });
  it("scores passwords", () => {
    expect(passwordStrength("")).toBe(0);
    expect(passwordStrength("abc")).toBe(0);
    expect(passwordStrength("abcdefgh")).toBe(1);
  });
});

describe("Checkbox / Switch / Radio", () => {
  it("toggles a checkbox by label click", async () => {
    render(<Checkbox label="Remember me" description="On this device" />);
    const cb = screen.getByRole("checkbox", { name: /Remember me/ });
    await userEvent.click(screen.getByText("Remember me"));
    expect(cb).toBeChecked();
    expect(cb).toHaveAccessibleDescription("On this device");
  });
  it("supports indeterminate", () => {
    render(<Checkbox label="All" indeterminate readOnly checked={false} />);
    const cb = screen.getByRole("checkbox") as HTMLInputElement;
    expect(cb.indeterminate).toBe(true);
    expect(cb).toBePartiallyChecked();
  });
  it("switch has role=switch and toggles with Space", async () => {
    render(<Switch label="Notifications" />);
    const sw = screen.getByRole("switch", { name: "Notifications" });
    sw.focus();
    await userEvent.keyboard(" ");
    expect(sw).toBeChecked();
  });
  it("radio group is a labelled fieldset with arrow-key selection", async () => {
    function Harness() {
      const [v, setV] = useState("a");
      return (
        <RadioGroup legend="Theme" value={v} onValueChange={setV}>
          <Radio value="a" label="Alpha" />
          <Radio value="b" label="Beta" />
        </RadioGroup>
      );
    }
    render(<Harness />);
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Alpha" })).toBeChecked();
    screen.getByRole("radio", { name: "Alpha" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: "Beta" })).toBeChecked();
  });
  it("uncontrolled RadioGroup respects defaultValue", async () => {
    render(
      <RadioGroup legend="Size" defaultValue="m">
        <Radio value="s" label="S" />
        <Radio value="m" label="M" />
      </RadioGroup>,
    );
    expect(screen.getByRole("radio", { name: "M" })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: "S" }));
    expect(screen.getByRole("radio", { name: "S" })).toBeChecked();
  });
  it("Radio outside a group throws", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Radio value="x" label="x" />)).toThrow();
    spy.mockRestore();
  });
});
