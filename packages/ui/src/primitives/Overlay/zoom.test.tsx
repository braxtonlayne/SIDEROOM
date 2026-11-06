import { render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { effectiveZoom, useFloating } from "./useFloating";

describe("effectiveZoom (D-540)", () => {
  it("reads the element's CSS zoom where the browser reports it", () => {
    const el = document.createElement("div");
    Object.defineProperty(el, "currentCSSZoom", { value: 1.5 });
    expect(effectiveZoom(el)).toBe(1.5);
  });

  it("is 1 without an element, without the property, or with a nonsense value", () => {
    expect(effectiveZoom(null)).toBe(1);
    expect(effectiveZoom(document.createElement("div"))).toBe(1);
    const bad = document.createElement("div");
    Object.defineProperty(bad, "currentCSSZoom", { value: 0 });
    expect(effectiveZoom(bad)).toBe(1);
  });
});

function Floating({ x, y }: { x: number; y: number }) {
  const f = useFloating({ open: true, anchor: { x, y }, placement: "bottom-start", offset: 0, flip: false, shift: false });
  return <div data-testid="f" ref={f.ref} style={f.style} />;
}

describe("useFloating under a zoomed box (D-540)", () => {
  afterEach(() => {
    delete (HTMLElement.prototype as { currentCSSZoom?: number }).currentCSSZoom;
  });

  it("places the element at the anchor's viewport point at 100 %", () => {
    const { getByTestId } = render(<Floating x={300} y={120} />);
    expect(getByTestId("f").style.transform).toBe("translate(300px, 120px)");
  });

  it("divides the viewport point by the zoom, because the translate is scaled by it", () => {
    Object.defineProperty(HTMLElement.prototype, "currentCSSZoom", { configurable: true, get: () => 1.5 });
    const { getByTestId } = render(<Floating x={300} y={120} />);
    expect(getByTestId("f").style.transform).toBe("translate(200px, 80px)");
  });
});
