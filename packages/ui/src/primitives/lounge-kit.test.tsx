import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { ComingSoon } from "../sideroom/ComingSoon";
import { MOBILE_QUERY, useMediaQuery } from "../util/useMediaQuery";
import { observeRectWithFallback } from "../util/virtual";
import { RailPill } from "./Rail/RailPill";
import { ResizeHandle } from "./Resize/ResizeHandle";

describe("RailPill", () => {
  it("exposes its state as a data attribute and is decorative", () => {
    const { container } = render(<RailPill state="unread" />);
    const pill = container.querySelector("span");
    expect(pill).toHaveAttribute("data-state", "unread");
    expect(pill).toHaveAttribute("aria-hidden", "true");
  });
});

describe("ResizeHandle", () => {
  // jsdom has no PointerEvent, so testing-library would drop clientX; MouseEvent carries it.
  const hadPointerEvent = "PointerEvent" in window;
  beforeAll(() => {
    if (!hadPointerEvent) (window as unknown as { PointerEvent: unknown }).PointerEvent = MouseEvent;
  });
  afterAll(() => {
    if (!hadPointerEvent) delete (window as unknown as { PointerEvent?: unknown }).PointerEvent;
  });

  it("is a labelled vertical separator with slider-like values", () => {
    render(<ResizeHandle value={300} min={200} max={500} onChange={() => {}} label="Resize panel" />);
    const sep = screen.getByRole("separator", { name: "Resize panel" });
    expect(sep).toHaveAttribute("aria-orientation", "vertical");
    expect(sep).toHaveAttribute("aria-valuenow", "300");
    expect(sep).toHaveAttribute("aria-valuemin", "200");
    expect(sep).toHaveAttribute("aria-valuemax", "500");
  });

  it("grows and shrinks with arrow keys (inverted by default) and jumps with Home/End", () => {
    const onChange = vi.fn();
    render(<ResizeHandle value={300} min={200} max={500} onChange={onChange} label="Resize" />);
    const sep = screen.getByRole("separator");
    fireEvent.keyDown(sep, { key: "ArrowLeft" });
    expect(onChange).toHaveBeenLastCalledWith(316);
    fireEvent.keyDown(sep, { key: "ArrowRight" });
    expect(onChange).toHaveBeenLastCalledWith(284);
    fireEvent.keyDown(sep, { key: "Home" });
    expect(onChange).toHaveBeenLastCalledWith(200);
    fireEvent.keyDown(sep, { key: "End" });
    expect(onChange).toHaveBeenLastCalledWith(500);
  });

  it("follows the arrow direction when not inverted", () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle value={300} min={200} max={500} onChange={onChange} label="Resize" invert={false} />,
    );
    fireEvent.keyDown(screen.getByRole("separator"), { key: "ArrowRight" });
    expect(onChange).toHaveBeenLastCalledWith(316);
  });

  it("drags by pointer delta, reversed when inverted", () => {
    const onChange = vi.fn();
    render(<ResizeHandle value={300} min={200} max={500} onChange={onChange} label="Resize" />);
    const sep = screen.getByRole("separator");
    fireEvent.pointerDown(sep, { clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(sep, { clientX: 130 });
    expect(onChange).toHaveBeenLastCalledWith(270);
    fireEvent.pointerUp(sep);
    fireEvent.pointerMove(sep, { clientX: 200 });
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

describe("ComingSoon", () => {
  it("shows the default copy and calls onClose from its button", () => {
    const onClose = vi.fn();
    render(<ComingSoon open title="Notifications" onClose={onClose} />);
    expect(screen.getByRole("dialog", { name: "Notifications" })).toBeInTheDocument();
    expect(screen.getByText("This part of the Lounge is not built yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Got it" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("renders custom children and nothing when closed", () => {
    const { rerender } = render(
      <ComingSoon open={false} title="Directory" onClose={() => {}}>
        <p>Later.</p>
      </ComingSoon>,
    );
    expect(screen.queryByText("Later.")).not.toBeInTheDocument();
    rerender(
      <ComingSoon open title="Directory" onClose={() => {}}>
        <p>Later.</p>
      </ComingSoon>,
    );
    expect(screen.getByText("Later.")).toBeInTheDocument();
  });
});

describe("useMediaQuery", () => {
  it("reports the current match and follows change events", () => {
    const listeners = new Set<() => void>();
    let matches = true;
    const mq = {
      get matches() {
        return matches;
      },
      addEventListener: (_: string, cb: () => void) => listeners.add(cb),
      removeEventListener: (_: string, cb: () => void) => listeners.delete(cb),
    };
    const spy = vi.spyOn(window, "matchMedia").mockImplementation(
      () => mq as unknown as MediaQueryList,
    );
    try {
      const { result } = renderHook(() => useMediaQuery(MOBILE_QUERY));
      expect(result.current).toBe(true);
      matches = false;
      act(() => listeners.forEach((cb) => cb()));
      expect(result.current).toBe(false);
      expect(spy).toHaveBeenCalledWith(MOBILE_QUERY);
    } finally {
      spy.mockRestore();
    }
  });

  it("is false where matchMedia is missing", () => {
    const original = window.matchMedia;
    // @ts-expect-error simulate an environment without matchMedia
    window.matchMedia = undefined;
    try {
      const { result } = renderHook(() => useMediaQuery(MOBILE_QUERY));
      expect(result.current).toBe(false);
    } finally {
      window.matchMedia = original;
    }
  });
});

describe("observeRectWithFallback", () => {
  // The observer only reads the scroll element's offset size and the window it was given.
  const instanceFor = (el: HTMLElement) =>
    ({ scrollElement: el, targetWindow: window }) as unknown as Parameters<
      ReturnType<typeof observeRectWithFallback>
    >[0];

  it("reports the fallback while the element has no layout", () => {
    const el = document.createElement("div");
    const cb = vi.fn();
    observeRectWithFallback({ width: 0, height: 480 })(instanceFor(el), cb);
    expect(cb).toHaveBeenCalledWith({ width: 0, height: 480 });
  });

  it("reports the real rect once the element has layout", () => {
    const el = document.createElement("div");
    Object.defineProperty(el, "offsetWidth", { value: 320 });
    Object.defineProperty(el, "offsetHeight", { value: 200 });
    const cb = vi.fn();
    observeRectWithFallback({ width: 0, height: 480 })(instanceFor(el), cb);
    expect(cb).toHaveBeenCalledWith({ width: 320, height: 200 });
  });
});
