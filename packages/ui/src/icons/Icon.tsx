import type { SVGAttributes } from "react";
import { ICON_PATHS, type IconName } from "./paths";

export type IconSize = 16 | 20 | 24;

/** Stroke weight in viewBox units, tuned so the rendered line stays ~1.5-1.75px at each optical size. */
const STROKE: Record<IconSize, number> = { 24: 1.75, 20: 2, 16: 2.25 };

export interface IconProps extends Omit<SVGAttributes<SVGSVGElement>, "children" | "name"> {
  name: IconName;
  size?: IconSize;
  /** Accessible name. Omit for decorative icons (rendered aria-hidden). */
  label?: string;
}

/** DS-PRIM-014: renders one icon from the original set. Colour follows `currentColor`. */
export function Icon({ name, size = 24, label, ...rest }: IconProps) {
  const d = ICON_PATHS[name];
  return (
    <svg
      {...rest}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={STROKE[size]}
      strokeLinecap="round"
      strokeLinejoin="round"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
      data-icon={name}
    >
      <path d={d} />
    </svg>
  );
}
