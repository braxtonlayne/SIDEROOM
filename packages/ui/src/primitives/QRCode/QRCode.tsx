import { useMemo } from "react";
import { encodeQr, type QrEcc } from "../../util/qr";

export interface QRCodeProps {
  /** Text to encode (UTF-8). Up to ~200 bytes. */
  value: string;
  /** Accessible description, e.g. "QR code for your authenticator app". */
  label: string;
  /** Rendered edge length in px. */
  size?: number;
  ecc?: QrEcc;
  className?: string;
}

/**
 * A QR code as inline SVG (no network, no canvas). Always dark-on-white with a 4-module quiet zone,
 * independent of the theme, because scanners need the contrast.
 */
export function QRCode({ value, label, size = 192, ecc = "M", className }: QRCodeProps) {
  const { d, dim } = useMemo(() => {
    const m = encodeQr(value, ecc);
    const quiet = 4;
    let path = "";
    for (let y = 0; y < m.size; y++) {
      let x = 0;
      while (x < m.size) {
        if (!m.modules[y]![x]) {
          x++;
          continue;
        }
        let end = x;
        while (end < m.size && m.modules[y]![end]) end++;
        path += `M${x + quiet} ${y + quiet}h${end - x}v1h-${end - x}z`;
        x = end;
      }
    }
    return { d: path, dim: m.size + quiet * 2 };
  }, [value, ecc]);
  return (
    <svg
      role="img"
      aria-label={label}
      className={className}
      width={size}
      height={size}
      viewBox={`0 0 ${dim} ${dim}`}
      shapeRendering="crispEdges"
      style={{ background: "#fff", borderRadius: 4 }}
    >
      <path d={d} fill="#000" />
    </svg>
  );
}
