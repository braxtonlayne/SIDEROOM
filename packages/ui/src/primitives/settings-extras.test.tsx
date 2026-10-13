import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { axeViolations } from "../test-axe";
import { dataCodewords, encodeQr, formatBits, rawCodewords, rsDivisor, rsRemainder } from "../util/qr";
import { KeybindRow } from "./Kbd/KeybindRow";
import { QRCode } from "./QRCode/QRCode";
import { SettingsLayout } from "./Settings/SettingsLayout";

/** Reads the text back out of a single-block symbol: format -> mask -> unmask -> zig-zag -> byte-mode header. */
function decodeSingleBlock(m: ReturnType<typeof encodeQr>, ecc: "L" | "M"): string {
  const { size, modules, version } = m;
  let fmt = 0;
  for (let i = 0; i <= 5; i++) fmt |= (modules[i]![8] ? 1 : 0) << i;
  fmt |= (modules[7]![8] ? 1 : 0) << 6;
  fmt |= (modules[8]![8] ? 1 : 0) << 7;
  fmt |= (modules[8]![7] ? 1 : 0) << 8;
  for (let i = 9; i < 15; i++) fmt |= (modules[8]![14 - i] ? 1 : 0) << i;
  const mask = [0, 1, 2, 3, 4, 5, 6, 7].find((k) => formatBits(ecc, k) === fmt);
  expect(mask).toBeDefined();

  const fn = (x: number, y: number) => {
    if (x === 6 || y === 6) return true;
    if (x < 9 && y < 9) return true;
    if (x >= size - 8 && y < 9) return true;
    if (x < 9 && y >= size - 8) return true;
    if (version >= 2) {
      const n = Math.floor(version / 7) + 2;
      const step = Math.floor((version * 8 + n * 3 + 5) / (n * 4 - 4)) * 2;
      const pos = [6];
      for (let p = size - 7; pos.length < n; p -= step) pos.splice(1, 0, p);
      for (let i = 0; i < n; i++)
        for (let j = 0; j < n; j++) {
          if ((i === 0 && j === 0) || (i === 0 && j === n - 1) || (i === n - 1 && j === 0)) continue;
          if (Math.abs(x - pos[i]!) <= 2 && Math.abs(y - pos[j]!) <= 2) return true;
        }
    }
    return false;
  };
  const inv = (x: number, y: number) =>
    [
      (x + y) % 2 === 0,
      y % 2 === 0,
      x % 3 === 0,
      (x + y) % 3 === 0,
      (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
      ((x * y) % 2) + ((x * y) % 3) === 0,
      (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
      (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
    ][mask!]!;
  const bits: number[] = [];
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < size; vert++)
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        const y = ((right + 1) & 2) === 0 ? size - 1 - vert : vert;
        if (!fn(x, y)) bits.push(modules[y]![x]! !== inv(x, y) ? 1 : 0);
      }
  }
  const take = (from: number, n: number) => bits.slice(from, from + n).reduce((a, b) => (a << 1) | b, 0);
  expect(take(0, 4)).toBe(0b0100);
  const ccBits = version <= 9 ? 8 : 16;
  const len = take(4, ccBits);
  const out: number[] = [];
  for (let i = 0; i < len; i++) out.push(take(4 + ccBits + i * 8, 8));
  // Error correction codewords after the data must match a fresh Reed-Solomon pass.
  const data = Array.from({ length: dataCodewords(version, ecc) }, (_, i) => take(i * 8, 8));
  const eccLen = rawCodewords(version) - data.length;
  const have = Array.from({ length: eccLen }, (_, i) => take((data.length + i) * 8, 8));
  expect(have).toEqual(rsRemainder(data, rsDivisor(eccLen)));
  return new TextDecoder().decode(new Uint8Array(out));
}

describe("qr encoder", () => {
  it("matches published Reed-Solomon and format-information vectors", () => {
    // Well-known worked example: "HELLO WORLD", 1-M.
    const data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236, 17, 236, 17];
    expect(rsRemainder(data, rsDivisor(10))).toEqual([196, 35, 39, 119, 235, 215, 231, 226, 93, 23]);
    expect(formatBits("M", 0)).toBe(0b101010000010010);
    expect(formatBits("L", 0)).toBe(0b111011111000100);
    expect(dataCodewords(1, "L")).toBe(19);
    expect(dataCodewords(5, "M")).toBe(86);
    expect(dataCodewords(10, "L")).toBe(274);
  });

  it("builds a well-formed symbol: size, finders, timing, dark module", () => {
    const m = encodeQr("hello", "M");
    expect(m.version).toBe(1);
    expect(m.size).toBe(21);
    for (const [ox, oy] of [
      [0, 0],
      [14, 0],
      [0, 14],
    ] as const) {
      for (let i = 0; i < 7; i++) {
        expect(m.modules[oy]![ox + i]).toBe(true);
        expect(m.modules[oy + 6]![ox + i]).toBe(true);
        expect(m.modules[oy + i]![ox]).toBe(true);
        expect(m.modules[oy + i]![ox + 6]).toBe(true);
      }
      expect(m.modules[oy + 3]![ox + 3]).toBe(true);
      expect(m.modules[oy + 1]![ox + 1]).toBe(false);
    }
    for (let i = 8; i < 13; i++) expect(m.modules[6]![i]).toBe(i % 2 === 0);
    expect(m.modules[m.size - 8]![8]).toBe(true);
  });

  it("round-trips text through the matrix for single-block versions", () => {
    for (const [text, ecc] of [
      ["hello", "M"],
      ["otpauth://totp/x?secret=ABC", "L"],
      ["The quick brown fox jumps over the lazy dog 0123456789", "L"],
      ["snow ☃ man", "M"],
    ] as const) {
      const m = encodeQr(text, ecc);
      expect(m.version).toBeLessThanOrEqual(4);
      expect(decodeSingleBlock(m, ecc)).toBe(text);
    }
  });

  it("grows the version with the payload and rejects oversize input", () => {
    expect(encodeQr("x".repeat(100), "M").version).toBe(6);
    expect(encodeQr("x".repeat(200), "M").version).toBe(10);
    expect(() => encodeQr("x".repeat(300), "M")).toThrow(RangeError);
    const big = encodeQr("otpauth://totp/SIDE%20ROOM:alice?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=SIDE%20ROOM", "M");
    expect(big.size).toBe(big.version * 4 + 17);
  });
});

describe("QRCode", () => {
  it("renders an accessible svg and is axe-clean", async () => {
    const { container } = render(<QRCode value="otpauth://totp/a?secret=B" label="QR code for your authenticator app" />);
    expect(screen.getByRole("img", { name: "QR code for your authenticator app" })).toBeInTheDocument();
    expect(container.querySelector("path")?.getAttribute("d")?.length).toBeGreaterThan(50);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("KeybindRow", () => {
  it("renders action and keys inside a description list", async () => {
    const { container } = render(
      <dl>
        <KeybindRow action="Quick switcher" keys={["Ctrl", "K"]} />
      </dl>,
    );
    expect(screen.getByText("Quick switcher")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Ctrl plus K" })).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("SettingsLayout nav", () => {
  const sections = [
    { id: "a", label: "Account", group: "User" },
    { id: "b", label: "Profile", group: "User" },
    { id: "c", label: "Appearance", group: "App" },
  ];
  it("uses a roving tabindex with arrow, Home and End keys, and renders the footer", async () => {
    const onSelect = vi.fn();
    render(
      <SettingsLayout
        inline
        title="User settings"
        sections={sections}
        activeId="b"
        onSelect={onSelect}
        onClose={() => {}}
        sidebarFooter={<button type="button">Log Out</button>}
      >
        body
      </SettingsLayout>,
    );
    const a = screen.getByRole("button", { name: "Account" });
    const b = screen.getByRole("button", { name: "Profile" });
    const c = screen.getByRole("button", { name: "Appearance" });
    expect([a.tabIndex, b.tabIndex, c.tabIndex]).toEqual([-1, 0, -1]);
    // The "Sections" toggle is the first tab stop in the DOM. CSS hides it on wide screens and only
    // shows it on narrow ones, but this environment loads no CSS, so step over it here.
    await userEvent.tab();
    expect(screen.getByRole("button", { name: /Sections/ })).toHaveFocus();
    await userEvent.tab();
    expect(b).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(c).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(a).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(c).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(a).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledWith("a");
    expect(screen.getByRole("button", { name: "Log Out" })).toBeInTheDocument();
  });
});
