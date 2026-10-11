import { useRef } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';

/**
 * Missing from @sideroom/ui: a vertical separator that resizes the panel to its right.
 * Pointer drag and Arrow keys; announces its value as a slider-like separator.
 */
export interface ResizeHandleProps {
  value: number;
  min: number;
  max: number;
  onChange: (width: number) => void;
  label: string;
  /** Dragging left grows a right-hand panel. */
  invert?: boolean;
}

export function ResizeHandle({
  value,
  min,
  max,
  onChange,
  label,
  invert = true,
}: ResizeHandleProps) {
  const start = useRef<{ x: number; w: number } | null>(null);
  const down = (e: PointerEvent<HTMLDivElement>) => {
    start.current = { x: e.clientX, w: value };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const dx = e.clientX - start.current.x;
    onChange(start.current.w + (invert ? -dx : dx));
  };
  const up = () => {
    start.current = null;
  };
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    const grow = invert ? 'ArrowLeft' : 'ArrowRight';
    const shrink = invert ? 'ArrowRight' : 'ArrowLeft';
    if (e.key === grow) onChange(value + 16);
    else if (e.key === shrink) onChange(value - 16);
    else if (e.key === 'Home') onChange(min);
    else if (e.key === 'End') onChange(max);
    else return;
    e.preventDefault();
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      className="lng-resize"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onKeyDown={key}
    />
  );
}
