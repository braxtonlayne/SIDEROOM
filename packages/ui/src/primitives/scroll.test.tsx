import { render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it } from "vitest";
import { computePosition } from "../util/position";
import { ScrollArea, VirtualGrid, VirtualList, type VirtualListHandle } from "./Scroll/ScrollArea";

describe("ScrollArea", () => {
  it("is keyboard-focusable and a labelled region when named", () => {
    render(
      <ScrollArea aria-label="Members" scrollbar="thin">
        content
      </ScrollArea>,
    );
    const el = screen.getByRole("region", { name: "Members" });
    expect(el).toHaveAttribute("tabindex", "0");
    expect(el.className).toContain("thin");
  });
});

describe("VirtualList", () => {
  it("mounts only a window of rows with list semantics", () => {
    const ref = createRef<VirtualListHandle>();
    render(
      <VirtualList ref={ref} aria-label="Messages" count={10000} estimateSize={40} renderItem={(i) => <span>Row {i}</span>} style={{ height: 400 }} />,
    );
    expect(screen.getByRole("list", { name: "Messages" })).toBeInTheDocument();
    const items = screen.getAllByRole("listitem");
    expect(items.length).toBeGreaterThan(5);
    expect(items.length).toBeLessThan(60);
    expect(items[0]).toHaveAttribute("aria-setsize", "10000");
    expect(items[0]).toHaveAttribute("aria-posinset", "1");
    expect(screen.getByText("Row 0")).toBeInTheDocument();
    expect(screen.queryByText("Row 9999")).toBeNull();
    expect(ref.current?.scrollToIndex).toBeTypeOf("function");
  });

  it("renders the empty slot", () => {
    render(<VirtualList count={0} estimateSize={40} renderItem={() => null} empty={<p>Nothing here</p>} />);
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
  });
});

describe("VirtualGrid", () => {
  it("lays cells into fixed columns and windows the rows", () => {
    render(<VirtualGrid aria-label="Posters" count={5000} columns={4} rowHeight={120} renderItem={(i) => <b>Cell {i}</b>} />);
    expect(screen.getByText("Cell 0")).toBeInTheDocument();
    expect(screen.getByText("Cell 3")).toBeInTheDocument();
    expect(screen.queryByText("Cell 4999")).toBeNull();
    const cells = screen.getAllByRole("listitem");
    expect(cells.length).toBeLessThan(200);
    expect(cells[0]).toHaveAttribute("aria-setsize", "5000");
    const row = cells[0]?.parentElement as HTMLElement;
    expect(row.style.gridTemplateColumns).toBe("repeat(4, minmax(0, 1fr))");
  });
});

describe("computePosition", () => {
  const vp = { width: 1000, height: 800 };
  const float = { width: 100, height: 50 };

  it("places on the preferred side, centered", () => {
    const r = computePosition({ x: 450, y: 400, width: 100, height: 20 }, float, vp, { placement: "bottom", offset: 8 });
    expect(r).toMatchObject({ x: 450, y: 428, side: "bottom", placement: "bottom" });
  });

  it("flips to the opposite side when the preferred side overflows", () => {
    const r = computePosition({ x: 450, y: 760, width: 100, height: 20 }, float, vp, { placement: "bottom", offset: 8 });
    expect(r.side).toBe("top");
    expect(r.y).toBe(760 - 50 - 8);
  });

  it("does not flip when it does not help or flipping is disabled", () => {
    const small = { width: 1000, height: 60 };
    const r = computePosition({ x: 450, y: 10, width: 100, height: 20 }, float, small, { placement: "bottom" });
    expect(["top", "bottom"]).toContain(r.side);
    const r2 = computePosition({ x: 450, y: 760, width: 100, height: 20 }, float, vp, { placement: "bottom", flip: false });
    expect(r2.side).toBe("bottom");
  });

  it("shifts along the cross axis to stay on screen", () => {
    const left = computePosition({ x: 0, y: 100, width: 20, height: 20 }, float, vp, { placement: "bottom", padding: 8 });
    expect(left.x).toBe(8);
    const right = computePosition({ x: 990, y: 100, width: 10, height: 20 }, float, vp, { placement: "top", padding: 8 });
    expect(right.x).toBe(1000 - 100 - 8);
  });

  it("flips submenus to the left and honours alignment", () => {
    const r = computePosition({ x: 900, y: 100, width: 100, height: 30 }, { width: 200, height: 100 }, vp, { placement: "right-start", offset: 2 });
    expect(r.side).toBe("left");
    expect(r.x).toBe(900 - 200 - 2);
    expect(r.y).toBe(100);
    expect(r.placement).toBe("left-start");
    const end = computePosition({ x: 400, y: 100, width: 100, height: 20 }, float, vp, { placement: "bottom-end" });
    expect(end.x).toBe(500 - 100);
  });
});
