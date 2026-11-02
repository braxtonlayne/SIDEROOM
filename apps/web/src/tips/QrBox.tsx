import { encodeQr, QRCode } from '@sideroom/ui';
import { useMemo } from 'react';

/**
 * A QR code, or nothing when the payload is longer than the generator takes (the text stays beside it). The encoder
 * is tried first: a throw inside `QRCode`'s own render could not be caught here.
 */
export function QrBox({
  value,
  label,
  size = 160,
}: {
  value: string;
  label: string;
  size?: number;
}) {
  const fits = useMemo(() => {
    try {
      encodeQr(value, 'L');
      return true;
    } catch {
      return false;
    }
  }, [value]);
  return fits ? <QRCode value={value} label={label} size={size} ecc="L" /> : null;
}
