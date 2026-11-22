// Pill-style tab switcher shown at the left of a workspace toolbar.

export interface WorkspaceTab<K extends string> {
  key: K;
  label: string;
}

export function WorkspaceTabs<K extends string>({
  label,
  tabs,
  active,
  onChange,
}: {
  label: string;
  tabs: WorkspaceTab<K>[];
  active: K;
  onChange: (key: K) => void;
}) {
  return (
    <div className="pill-group workspace-tabs" role="tablist" aria-label={label}>
      {tabs.map((t) => (
        <button
          key={t.key}
          type="button"
          role="tab"
          aria-selected={active === t.key}
          className={`pill ${active === t.key ? "active" : ""}`}
          onClick={() => onChange(t.key)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
