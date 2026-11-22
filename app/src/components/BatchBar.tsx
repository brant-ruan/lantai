// Actions on the selected session slices. Each control applies at once and then clears the
// selection; there is no separate Apply step. Shown while the selection is non-empty.

import { useState } from "react";
import { useT } from "../i18n";
import type { ProjectOption, SliceCard } from "../types";
import { assignableProjects } from "../projectOptions";

const PLACEHOLDER = "";
const NONE_VALUE = "__none__";
const UNASSIGN_VALUE = "__unassign__";

interface BatchBarProps {
  slices: SliceCard[];
  visibleSliceIds: number[];
  selectedSliceIds: number[];
  projects: ProjectOption[];
  onChangeSelected: (sliceIds: number[]) => void;
  onBatchAssign: (sessionIds: number[], projectId: number | null) => Promise<void>;
  onBatchUnassign: (sessionIds: number[]) => Promise<void>;
  onBatchSetIgnoreState: (
    sliceIds: number[],
    state: "ignored" | "included",
  ) => Promise<void>;
  onClose: () => void;
}

export function BatchBar({
  slices,
  visibleSliceIds,
  selectedSliceIds,
  projects,
  onChangeSelected,
  onBatchAssign,
  onBatchUnassign,
  onBatchSetIgnoreState,
  onClose,
}: BatchBarProps) {
  const t = useT().batch;
  const [busy, setBusy] = useState(false);
  const selectedSet = new Set(selectedSliceIds);
  const selected = slices.filter((s) => selectedSet.has(s.id));
  const anyActive = selected.some((s) => !s.ignored);
  const anyIgnored = selected.some((s) => s.ignored);
  const allSelected =
    visibleSliceIds.length > 0 && visibleSliceIds.every((id) => selectedSet.has(id));

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      onClose();
    } finally {
      setBusy(false);
    }
  };

  const assign = (value: string) => {
    if (value === PLACEHOLDER) return;
    const sessionIds = [...new Set(selected.map((s) => s.sessionId))];
    void run(() =>
      value === UNASSIGN_VALUE
        ? onBatchUnassign(sessionIds)
        : onBatchAssign(sessionIds, value === NONE_VALUE ? null : Number(value)),
    );
  };

  const setIgnored = (state: "ignored" | "included") => {
    const ids = selected.filter((s) => s.ignored !== (state === "ignored")).map((s) => s.id);
    void run(() => onBatchSetIgnoreState(ids, state));
  };

  return (
    <div className="batch-bar" role="toolbar" aria-label={t.toolbar}>
      <span className="batch-count">{t.selected(selectedSliceIds.length)}</span>
      <button
        type="button"
        className="batch-link"
        disabled={busy || allSelected}
        onClick={() => onChangeSelected(visibleSliceIds)}
      >
        {t.selectAll}
      </button>
      {anyActive && (
        <select
          className="batch-select"
          value={PLACEHOLDER}
          disabled={busy}
          onChange={(e) => assign(e.target.value)}
          title={t.moveTitle}
        >
          <option value={PLACEHOLDER} disabled>
            {t.moveTo}
          </option>
          {assignableProjects(projects).map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
          <option value={NONE_VALUE}>{t.noProject}</option>
          <option value={UNASSIGN_VALUE}>{t.unassigned}</option>
        </select>
      )}
      {anyActive && (
        <button
          type="button"
          className="batch-action"
          disabled={busy}
          onClick={() => setIgnored("ignored")}
          title={t.ignoreTitle}
        >
          {t.ignore}
        </button>
      )}
      {anyIgnored && (
        <button
          type="button"
          className="batch-action"
          disabled={busy}
          onClick={() => setIgnored("included")}
        >
          {t.include}
        </button>
      )}
      <button
        type="button"
        className="batch-close"
        disabled={busy}
        onClick={onClose}
        title={t.exitTitle}
        aria-label={t.exit}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}
