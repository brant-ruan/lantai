// The form that adds or edits a provider: an OpenAI-compatible API (address, key, timeout) or
// an agent CLI (its command).

import { useT } from "../../../i18n";
import type { ProviderKind, ProviderPreset } from "../../../types";
import { PRESET_GROUPS, defaultCommand, type FetchState, type ProviderForm } from "./forms";

interface ProviderFormCardProps {
  form: ProviderForm;
  setForm: (form: ProviderForm) => void;
  presets: ProviderPreset[];
  /** The result of "Fetch models", which also checks the connection. */
  fetchState: FetchState;
  busy: boolean;
  onFetch: () => void;
  onSave: () => void;
  onCancel: () => void;
}

export function ProviderFormCard({
  form,
  setForm,
  presets,
  fetchState,
  busy,
  onFetch,
  onSave,
  onCancel,
}: ProviderFormCardProps) {
  const tr = useT();
  const tm = tr.models;
  const presetLabel = (p: ProviderPreset) =>
    p.region ? tm.presetRegion(p.name, p.region) : p.name;
  return (
    <form
      className="inline-card stack"
      autoComplete="off"
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div className="form-grid-2">
        <label className="form-field">
          <span>{tm.name}</span>
          <input
            type="text"
            placeholder={tm.namePlaceholder}
            value={form.name}
            onChange={(e) =>
              setForm({ ...form, name: e.target.value })
            }
            required
          />
        </label>
        <label className="form-field">
          <span>{tm.type}</span>
          <select
            value={form.kind}
            onChange={(e) =>
              setForm({
                ...form,
                kind: e.target.value as ProviderKind,
              })
            }
          >
            <option value="openai_compat">{tm.openaiType}</option>
            <option value="claude_cli">{tm.claudeCliType}</option>
            <option value="codex_cli">{tm.codexCliType}</option>
          </select>
        </label>

        {form.kind === "openai_compat" ? (
          <>
            <label className="form-field">
              <span>{tm.baseUrl}</span>
              <input
                type="text"
                placeholder="https://host/v1"
                value={form.baseUrl}
                onChange={(e) =>
                  setForm({ ...form, baseUrl: e.target.value })
                }
                required
              />
            </label>
            <label className="form-field">
              <span>{tm.preset}</span>
              <select
                value=""
                onChange={(e) => {
                  const preset = presets.find((p) => p.id === e.target.value);
                  if (!preset) return;
                  setForm({
                    ...form,
                    baseUrl: preset.baseUrl,
                    name: form.name || preset.id,
                  });
                }}
              >
                <option value="">{tm.fillPreset}</option>
                {PRESET_GROUPS.map((group) => (
                  <optgroup key={group} label={tm.presetGroups[group]}>
                    {presets
                      .filter((p) => p.group === group)
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {presetLabel(p)}
                        </option>
                      ))}
                  </optgroup>
                ))}
              </select>
            </label>
            <label className="form-field">
              <span>
                {tm.apiKey}{" "}
                {form.hasKey && !form.clearKey && (
                  <span className="badge badge-ok">{tm.stored}</span>
                )}
              </span>
              <input
                type="password"
                autoComplete="new-password"
                spellCheck={false}
                placeholder={
                  form.hasKey
                    ? tm.keepKey
                    : tm.noAuth
                }
                value={form.keyInput}
                onChange={(e) =>
                  setForm({
                    ...form,
                    keyInput: e.target.value,
                    clearKey: false,
                  })
                }
              />
            </label>
            <label className="form-field">
              <span>{tm.timeout}</span>
              <input
                type="text"
                inputMode="numeric"
                placeholder="300"
                value={form.timeout}
                onChange={(e) =>
                  setForm({
                    ...form,
                    timeout: e.target.value.replace(/[^0-9]/g, ""),
                  })
                }
              />
            </label>
          </>
        ) : (
          <label className="form-field">
            <span>{tm.command}</span>
            <input
              type="text"
              placeholder={defaultCommand(form.kind)}
              value={form.binary}
              onChange={(e) =>
                setForm({ ...form, binary: e.target.value })
              }
            />
          </label>
        )}
      </div>

      {fetchState.error && (
        <div className="warning-list">{fetchState.error}</div>
      )}
      {fetchState.models && (
        <div className="test-result ok">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <polyline points="4 12.5 9.5 18 20 6" />
          </svg>
          {tm.connected(fetchState.models.length)}
        </div>
      )}

      <div className="settings-card-foot">
        <div className="toolbar-actions">
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            disabled={fetchState.loading}
            onClick={onFetch}
          >
            {fetchState.loading ? tm.fetching : tm.fetchModels}
          </button>
          {form.kind === "openai_compat" && form.hasKey && (
            <label className="toggle-row">
              <input
                type="checkbox"
                checked={form.clearKey}
                onChange={(e) =>
                  setForm({
                    ...form,
                    clearKey: e.target.checked,
                    keyInput: "",
                  })
                }
              />
              <span>{tm.removeKey}</span>
            </label>
          )}
        </div>
        <div className="toolbar-actions">
          <button
            type="button"
            className="btn btn-ghost btn-xs"
            onClick={onCancel}
          >
            {tr.common.cancel}
          </button>
          <button type="submit" className="btn btn-primary btn-xs" disabled={busy}>
            {tm.saveProvider}
          </button>
        </div>
      </div>
    </form>
  );
}
