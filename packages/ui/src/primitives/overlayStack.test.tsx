import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../theme/ThemeProvider";
import { Drawer } from "./Drawer/Drawer";
import { Modal } from "./Modal/Modal";
import { ContextMenu } from "./Menu/Menu";

function Stacked({ onModalClose, onDrawerClose }: { onModalClose: () => void; onDrawerClose: () => void }) {
  return (
    <ThemeProvider section="lounge">
      <Drawer open title="Drawer" onClose={onDrawerClose}>
        <Modal open title="Modal" onClose={onModalClose}>
          <button type="button">inner</button>
        </Modal>
      </Drawer>
    </ThemeProvider>
  );
}

describe("overlay stack (Q77)", () => {
  it("routes Escape only to the topmost overlay", async () => {
    const onModalClose = vi.fn();
    const onDrawerClose = vi.fn();
    render(<Stacked onModalClose={onModalClose} onDrawerClose={onDrawerClose} />);
    await userEvent.keyboard("{Escape}");
    expect(onModalClose).toHaveBeenCalledTimes(1);
    expect(onDrawerClose).not.toHaveBeenCalled();
  });

  it("locks body scroll while a modal is open and restores it after", () => {
    document.body.style.overflow = "scroll";
    const { rerender } = render(<Modal open={false} title="M" onClose={() => {}}>x</Modal>);
    rerender(
      <ThemeProvider section="lounge">
        <Modal open title="M" onClose={() => {}}>
          x
        </Modal>
      </ThemeProvider>,
    );
    expect(document.body.style.overflow).toBe("hidden");
    rerender(
      <ThemeProvider section="lounge">
        <Modal open={false} title="M" onClose={() => {}}>
          x
        </Modal>
      </ThemeProvider>,
    );
    expect(document.body.style.overflow).toBe("scroll");
    document.body.style.overflow = "";
  });

  it("keeps the topmost modal's Escape away from a menu opened beneath it", () => {
    const onClose = vi.fn();
    render(
      <ThemeProvider section="lounge">
        <ContextMenu label="Ctx" items={[{ label: "A" }]}>
          <p>region</p>
        </ContextMenu>
        <Modal open title="M" onClose={onClose}>
          <button type="button">ok</button>
        </Modal>
      </ThemeProvider>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
