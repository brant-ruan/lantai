// One session slice in a list: checkbox while selecting, title and meta line (click opens
// the slice), and the row's own controls on the right. `data-slice-id` lets the rectangle
// selection find the row.

import React from "react";
import { useT } from "../../i18n";
import { AgentBadge } from "../AgentBadge";
import type { SliceCard } from "../../types";

interface SliceRowProps {
  slice: SliceCard;
  /** Prefix the time with the slice's date. */
  showDate?: boolean;
  /** The slice is open in the drawer. */
  opened: boolean;
  /** The slice is in the batch selection. */
  checked: boolean;
  /** A batch selection exists, so every row shows its checkbox. */
  selecting: boolean;
  onToggleChecked: () => void;
  onClick: (e: React.MouseEvent) => void;
  children?: React.ReactNode;
}

export function SliceRow({
  slice,
  showDate = false,
  opened,
  checked,
  selecting,
  onToggleChecked,
  onClick,
  children,
}: SliceRowProps) {
  const t = useT().slice;
  const classes = [
    "slice-row",
    opened ? "selected" : "",
    checked ? "checked" : "",
    slice.ignored ? "ignored" : "",
  ];
  return (
    <div className={classes.join(" ")} data-slice-id={slice.id}>
      {selecting && (
        <input
          type="checkbox"
          checked={checked}
          onChange={onToggleChecked}
          aria-label={t.select(slice.title)}
        />
      )}
      <button type="button" className="slice-main" onClick={onClick}>
        <div className="slice-title-line">
          {slice.ignored && <span className="badge">{t.ignored}</span>}
          <span className="slice-title">{slice.title}</span>
          {slice.userNote && (
            <span className="badge" title={slice.userNote}>
              {t.note}
            </span>
          )}
        </div>
        <div className="slice-meta-line">
          <AgentBadge agent={slice.agent} name={slice.agentName} />
          <span>
            {showDate && `${slice.sliceDate.slice(5)} `}
            {slice.startTime}-{slice.endTime}
          </span>
          <span>{t.minutes(slice.activeMinutes)}</span>
          <span>{t.turns(slice.turnCount)}</span>
          {slice.projectOverrideId !== null && <span>{t.dayOverride}</span>}
        </div>
      </button>
      {children && <div className="slice-controls">{children}</div>}
    </div>
  );
}
