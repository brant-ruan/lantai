// Scroll area for report documents, centred at the reading width chosen by the viewer (never
// wider than the room available). Either edge of the document can be dragged to change that
// width while the report stays in view; the document stays centred, so it grows on both sides.
// Double-clicking an edge restores the default.
import React, { useRef, useState } from "react";
import { useT } from "../../i18n";
import { resetReportWidth, setReportWidth, useReportWidth } from "../../hooks/useReportWidth";

export function ReportCanvas({ children }: { children: React.ReactNode }) {
  const width = useReportWidth();
  const t = useT().report;
  const contentRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);

  const startDrag = (side: -1 | 1) => (down: React.MouseEvent) => {
    const content = contentRef.current;
    if (down.button !== 0 || !content) return;
    down.preventDefault();
    const style = getComputedStyle(content);
    const room =
      content.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    // Start from the width on screen, which a narrow window may hold below the stored one.
    const startWidth = Math.min(width, room);
    const startX = down.clientX;
    setDragging(true);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
    const onMove = (ev: MouseEvent) => {
      setReportWidth(startWidth + side * 2 * (ev.clientX - startX));
    };
    const onUp = () => {
      setDragging(false);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const handle = (side: -1 | 1) => (
    <div
      className={`report-width-handle ${side < 0 ? "is-left" : "is-right"}`}
      title={t.widthHandle}
      aria-hidden="true"
      onMouseDown={startDrag(side)}
      onDoubleClick={resetReportWidth}
    />
  );

  return (
    <div
      className={`report-canvas${dragging ? " is-resizing" : ""}`}
      style={{ "--report-width": `${width}px` } as React.CSSProperties}
    >
      <div ref={contentRef} className="report-content">
        {children}
      </div>
      {handle(-1)}
      {handle(1)}
      {dragging && <div className="report-width-readout">{width}px</div>}
    </div>
  );
}
