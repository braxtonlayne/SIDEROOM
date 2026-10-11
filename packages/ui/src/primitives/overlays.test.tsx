import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../theme/ThemeProvider";
import { Button } from "./Button/Button";
import { Notice, ToastProvider, useToast } from "./Feedback/Feedback";
import { ConfirmDialog, Modal } from "./Modal/Modal";
import { Popover, Tooltip } from "./Overlay/Overlay";

afterEach(() => vi.useRealTimers());

function ModalHarness({ onClose = () => {} }: { onClose?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Open</button>
      <Modal
        open={open}
        title="Settings"
        description="Change things"
        onClose={() => {
          onClose();
          setOpen(false);
        }}
        footer={<button>Save</button>}
      >
        <input aria-label="Name" />
      </Modal>
    </>
  );
}

describe("Modal", () => {
  it("renders an accessible dialog and moves focus inside", async () => {
    render(<ModalHarness />);
    await userEvent.click(screen.getByText("Open"));
    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription("Change things");
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("closes on Escape and restores focus to the opener", async () => {
    const onClose = vi.fn();
    render(<ModalHarness onClose={onClose} />);
    const opener = screen.getByText("Open");
    await userEvent.click(opener);
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(opener).toHaveFocus();
  });

  it("traps Tab focus", async () => {
    render(<ModalHarness />);
    await userEvent.click(screen.getByText("Open"));
    const close = screen.getByRole("button", { name: "Close" });
    const save = screen.getByRole("button", { name: "Save" });
    save.focus();
    await userEvent.tab();
    expect(close).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(save).toHaveFocus();
  });

  it("closes on backdrop click but not on dialog click", async () => {
    const onClose = vi.fn();
    render(
      <Modal open title="T" onClose={onClose}>
        body
      </Modal>,
    );
    await userEvent.click(screen.getByText("body"));
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("dialog").parentElement!);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("locks body scroll and releases it", () => {
    const { rerender } = render(
      <Modal open title="T" onClose={() => {}}>
        x
      </Modal>,
    );
    expect(document.body.style.overflow).toBe("hidden");
    rerender(
      <Modal open={false} title="T" onClose={() => {}}>
        x
      </Modal>,
    );
    expect(document.body.style.overflow).not.toBe("hidden");
  });

  it("only the topmost of stacked modals handles Escape", async () => {
    const outer = vi.fn();
    const inner = vi.fn();
    render(
      <>
        <Modal open title="Outer" onClose={outer}>
          o
        </Modal>
        <Modal open title="Inner" onClose={inner}>
          i
        </Modal>
      </>,
    );
    await userEvent.keyboard("{Escape}");
    expect(inner).toHaveBeenCalledTimes(1);
    expect(outer).not.toHaveBeenCalled();
  });

  it("portals into the theme root so tokens apply", () => {
    render(
      <ThemeProvider section="lounge" data-testid="root">
        <Modal open title="T" onClose={() => {}}>
          x
        </Modal>
      </ThemeProvider>,
    );
    expect(screen.getByTestId("root")).toContainElement(screen.getByRole("dialog"));
  });
});

describe("ConfirmDialog", () => {
  it("confirms and cancels", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog open title="Delete?" confirmLabel="Delete" danger onConfirm={onConfirm} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("typed confirmation gates the confirm button", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog open title="Delete server" confirmLabel="Delete" requireText="my server" onConfirm={onConfirm} onCancel={() => {}} />,
    );
    const confirm = screen.getByRole("button", { name: "Delete" });
    expect(confirm).toBeDisabled();
    await userEvent.type(screen.getByLabelText(/Type "my server"/), "my serve");
    expect(confirm).toBeDisabled();
    await userEvent.type(screen.getByLabelText(/Type "my server"/), "r");
    expect(confirm).toBeEnabled();
    await userEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalled();
  });
});

describe("Tooltip", () => {
  it("shows on focus, describes the trigger, hides on Escape", async () => {
    render(
      <Tooltip content="Mute microphone" delay={0}>
        <button>Mic</button>
      </Tooltip>,
    );
    await userEvent.tab();
    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("Mute microphone");
    expect(screen.getByRole("button", { name: "Mic" })).toHaveAccessibleDescription("Mute microphone");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("shows after hover delay and hides on leave", () => {
    vi.useFakeTimers();
    render(
      <Tooltip content="Hi" delay={300}>
        <button>B</button>
      </Tooltip>,
    );
    const b = screen.getByRole("button");
    act(() => {
      b.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    });
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    act(() => {
      b.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});

describe("Popover", () => {
  it("toggles with aria-expanded, closes on outside click and Escape", async () => {
    render(
      <div>
        <button>outside</button>
        <Popover label="Filters" trigger={<button>Filter</button>}>
          <button>Inside</button>
        </Popover>
      </div>,
    );
    const trigger = screen.getByRole("button", { name: "Filter" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("dialog", { name: "Filters" })).toBeInTheDocument();

    await userEvent.click(screen.getByText("outside"));
    expect(screen.queryByRole("dialog")).toBeNull();

    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });
});

describe("Notice", () => {
  it("uses alert role for danger and status for info", () => {
    render(
      <>
        <Notice variant="danger" title="Failed">
          Try again
        </Notice>
        <Notice variant="info">FYI</Notice>
      </>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Failed");
    expect(screen.getByRole("status")).toHaveTextContent("FYI");
  });
  it("can be dismissed", async () => {
    const onDismiss = vi.fn();
    render(<Notice onDismiss={onDismiss}>x</Notice>);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalled();
  });
});

describe("Toast", () => {
  function Trigger({ duration }: { duration?: number }) {
    const toast = useToast();
    return <Button onClick={() => toast.show({ title: "Saved", description: "All good", duration })}>Go</Button>;
  }

  it("shows a toast in a live region and dismisses manually", async () => {
    render(
      <ToastProvider>
        <Trigger duration={0} />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByText("Go"));
    expect(screen.getByRole("status", { name: "Notifications" })).toHaveTextContent("Saved");
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("auto-dismisses after the duration", () => {
    vi.useFakeTimers();
    render(
      <ToastProvider>
        <Trigger duration={1000} />
      </ToastProvider>,
    );
    act(() => {
      screen.getByText("Go").click();
    });
    expect(screen.getByText("Saved")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(1100);
    });
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("useToast outside provider throws", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Trigger />)).toThrow();
    spy.mockRestore();
  });
});
