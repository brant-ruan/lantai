import { useCallback, useEffect, useState } from "react";
import { useT } from "../i18n";
import { readNumber, storeItem } from "../storage";

/**
 * Persistable numeric layout dimension with clamped range.
 */
export function useResizableValue(
  storageKey: string,
  initial: number,
  min: number,
  max: number,
) {
  const [value, setValue] = useState(() => readNumber(storageKey, initial, min, max));

  useEffect(() => {
    storeItem(storageKey, String(value));
  }, [storageKey, value]);

  const startDrag = useCallback(
    (computeNext: (ev: MouseEvent, startValue: number, startX: number) => number) =>
      (downEv: React.MouseEvent) => {
        downEv.preventDefault();
        const startX = downEv.clientX;
        const startValue = value;
        document.body.style.userSelect = "none";
        document.body.style.cursor = "col-resize";

        const onMove = (moveEv: MouseEvent) => {
          const raw = computeNext(moveEv, startValue, startX);
          setValue(Math.min(max, Math.max(min, raw)));
        };
        const onUp = () => {
          document.body.style.userSelect = "";
          document.body.style.cursor = "";
          window.removeEventListener("mousemove", onMove);
          window.removeEventListener("mouseup", onUp);
        };
        window.addEventListener("mousemove", onMove);
        window.addEventListener("mouseup", onUp);
      },
    [value, min, max],
  );

  const reset = useCallback(() => {
    setValue(initial);
  }, [initial]);

  return { value, startDrag, reset };
}

export function SplitHandle({
  onMouseDown,
  label,
}: {
  onMouseDown: (e: React.MouseEvent) => void;
  label?: string;
}) {
  const t = useT().resize;
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label ?? t.label}
      title={t.title}
      className="split-handle"
      onMouseDown={onMouseDown}
    />
  );
}
