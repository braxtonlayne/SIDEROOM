import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ArrowButton, ArrowLink, DisplayHeading, Eyebrow, GoldButton, OutlineButton } from "../sideroom/Editorial";
import { Avatar, initials } from "./Avatar/Avatar";
import { Badge, CounterBadge } from "./Badge/Badge";
import { Skeleton, Spinner } from "./Loading/Spinner";
import { Tabs } from "./Tabs/Tabs";

const items = [
  { id: "one", label: "One", content: "First panel" },
  { id: "two", label: "Two", content: "Second panel" },
  { id: "three", label: "Three", content: "Third panel", disabled: true },
  { id: "four", label: "Four", content: "Fourth panel" },
];

describe("Tabs", () => {
  it("wires roles, selection and panel", () => {
    render(<Tabs label="Sections" items={items} />);
    expect(screen.getByRole("tablist", { name: "Sections" })).toBeInTheDocument();
    const one = screen.getByRole("tab", { name: "One" });
    expect(one).toHaveAttribute("aria-selected", "true");
    expect(one).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "Two" })).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("One");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("First panel");
  });

  it("navigates with arrows, skipping disabled, wrapping, Home/End", async () => {
    render(<Tabs label="S" items={items} />);
    screen.getByRole("tab", { name: "One" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Two" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Four" })).toHaveFocus();
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Fourth panel");
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "One" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Four" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "One" })).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Four" })).toHaveFocus();
  });

  it("supports controlled value and click", async () => {
    const onValueChange = vi.fn();
    render(<Tabs label="S" items={items} value="one" onValueChange={onValueChange} variant="pill" />);
    await userEvent.click(screen.getByRole("tab", { name: "Two" }));
    expect(onValueChange).toHaveBeenCalledWith("two");
    expect(screen.getByRole("tab", { name: "One" })).toHaveAttribute("aria-selected", "true");
  });
});

describe("Avatar", () => {
  it("derives initials", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("  mono ")).toBe("M");
    expect(initials("")).toBe("?");
  });
  it("falls back to initials when there is no image or it fails", () => {
    const { rerender } = render(<Avatar name="Ada Lovelace" />);
    // The initials are generated content (CSS ::before from data-initials), not text of the control.
    expect(screen.getByRole("img", { name: "Ada Lovelace" })).toHaveAttribute("data-initials", "AL");
    rerender(<Avatar name="Ada Lovelace" src="/broken.png" />);
    const img = screen.getByRole("img", { name: "Ada Lovelace" });
    expect(img.tagName).toBe("IMG");
    act(() => {
      img.dispatchEvent(new Event("error"));
    });
    // The initials are generated content (CSS ::before from data-initials), not text of the control.
    expect(screen.getByRole("img", { name: "Ada Lovelace" })).toHaveAttribute("data-initials", "AL");
  });
  it("draws the Patron ring only when asked (D-256)", () => {
    render(<Avatar name="Bo" ring data-testid="r" />);
    expect(screen.getByTestId("r")).toHaveAttribute("data-ring", "true");
    render(<Avatar name="Al" data-testid="n" />);
    expect(screen.getByTestId("n")).not.toHaveAttribute("data-ring");
  });

  it("shows an accessible status and speaking ring", () => {
    render(<Avatar name="Bo" status="dnd" speaking data-testid="a" />);
    expect(screen.getByRole("img", { name: "Do not disturb" })).toBeInTheDocument();
    expect(screen.getByTestId("a")).toHaveAttribute("data-speaking", "true");
  });
});

describe("Badge", () => {
  it("renders variants", () => {
    render(<Badge variant="accent">New</Badge>);
    expect(screen.getByText("New")).toBeInTheDocument();
  });
  it("counter caps and labels", () => {
    const { rerender } = render(<CounterBadge count={7} noun="mentions" />);
    expect(screen.getByLabelText("7 mentions")).toHaveTextContent("7");
    rerender(<CounterBadge count={150} />);
    expect(screen.getByLabelText("150 notifications")).toHaveTextContent("99+");
    rerender(<CounterBadge count={0} />);
    expect(screen.queryByLabelText(/notifications/)).toBeNull();
  });
});

describe("Spinner / Skeleton", () => {
  it("spinner is a labelled status", () => {
    render(<Spinner label="Loading titles" />);
    expect(screen.getByRole("status", { name: "Loading titles" })).toBeInTheDocument();
  });
  it("skeleton is hidden from assistive tech", () => {
    render(<Skeleton data-testid="s" width={40} height={12} />);
    expect(screen.getByTestId("s")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("s")).toHaveStyle({ width: "40px", height: "12px" });
  });
});

describe("Side Room editorial kit", () => {
  it("Eyebrow renders text with a decorative rule", () => {
    const { container } = render(<Eyebrow>Good things stay between us.</Eyebrow>);
    expect(screen.getByText("Good things stay between us.")).toBeInTheDocument();
    expect(container.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });
  it("DisplayHeading renders the level and italic accent", () => {
    render(
      <DisplayHeading as="h2" accent="closed.">
        The internet, with the door
      </DisplayHeading>,
    );
    const h = screen.getByRole("heading", { level: 2 });
    expect(h).toHaveTextContent("The internet, with the door closed.");
    expect(screen.getByText("closed.").tagName).toBe("EM");
  });
  it("ArrowLink hides the arrow from the accessible name", () => {
    render(<ArrowLink href="/login">Member access</ArrowLink>);
    expect(screen.getByRole("link", { name: "Member access" })).toHaveAttribute("href", "/login");
  });
  it("GoldButton / OutlineButton / ArrowButton work as buttons", async () => {
    const onClick = vi.fn();
    render(
      <>
        <GoldButton arrow onClick={onClick}>
          Check invitation
        </GoldButton>
        <OutlineButton>Edit lineup</OutlineButton>
        <ArrowButton arrow>Member access</ArrowButton>
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Check invitation" }));
    expect(onClick).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Edit lineup" })).toHaveAttribute("type", "button");
    expect(screen.getByRole("button", { name: "Member access" })).toBeInTheDocument();
  });
});

describe("Avatar animated frame (Q70)", () => {
  it("renders the animated frame over the static one, hidden from assistive tech", () => {
    const { container } = render(<Avatar name="Ada Lovelace" src="/a.png" animatedSrc="/a.gif" />);
    const imgs = container.querySelectorAll("img");
    expect(imgs).toHaveLength(2);
    expect(imgs[0]).toHaveAttribute("alt", "Ada Lovelace");
    expect(imgs[1]).toHaveAttribute("alt", "");
    expect(imgs[1]).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("img", { name: "Ada Lovelace" })).toBeInTheDocument();
  });

  it("has no animated frame without an image", () => {
    const { container } = render(<Avatar name="Ada Lovelace" animatedSrc="/a.gif" />);
    expect(container.querySelectorAll("img")).toHaveLength(0);
  });
});
