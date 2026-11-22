// The rectangle drawn while drag-selecting rows (see useSliceSelection).

import type { MarqueeRect as Rect } from "../hooks/useSliceSelection";

export function MarqueeRect({ rect }: { rect: Rect | null }) {
  if (!rect) return null;
  return (
    <div
      className="marquee-rect"
      style={{ left: rect.left, top: rect.top, width: rect.width, height: rect.height }}
    />
  );
}
