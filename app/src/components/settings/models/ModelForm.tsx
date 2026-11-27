// The form that adds or edits a model alias: provider, model id (typed or picked from the
// provider's list), reasoning effort and temperature.

import { useMemo } from "react";
import { InfoTip } from "../../InfoTip";
import { useT } from "../../../i18n";
import { isCliKind } from "../../../types";
import type { ProviderPreset, ProviderSetting } from "../../../types";
import {
  EFFORTS,
  aliasFromModelId,
  listedEfforts,
  normalizeUrl,
  ownEfforts,
  type FetchState,
  type ModelForm,
} from "./forms";

interface ModelFormCardProps {
  form: ModelForm;
  setForm: (form: ModelForm) => void;
  providers: ProviderSetting[];
  presets: ProviderPreset[];
  /** The provider's model list, fetched on request (at once for agent CLIs). */
  fetchState: FetchState;
  filter: string;
  setFilter: (filter: string) => void;
  busy: boolean;
  /** Switch the form to another provider (resets the list; fetches it for an agent CLI). */
  onProviderChange: (provider: string) => void;
  onFetch: (provider: string) => void;
  onSave: () => void;
  onCancel: () => void;
}

export function ModelFormCard({
  form,
  setForm,
  providers,
  presets,
  fetchState,
  filter,
  setFilter,
  busy,
  onProviderChange,
  onFetch,
  onSave,
  onCancel,
}: ModelFormCardProps) {
  const tr = useT();
  const tm = tr.models;
  const filteredIds = useMemo(() => {
    const ids = (fetchState.models ?? []).map((m) => m.id);
    const q = filter.trim().toLowerCase();
    return q ? ids.filter((id) => id.toLowerCase().includes(q)) : ids;
  }, [fetchState.models, filter]);

  const provider = providers.find((p) => p.name === form.provider);
  const preset = provider?.baseUrl
    ? presets.find((p) => p.baseUrl === normalizeUrl(provider.baseUrl ?? ""))
    : undefined;
  const isCli = isCliKind(provider?.kind);
  const listed = listedEfforts(fetchState.models, form.model.trim());
  const effortHints = listed.length > 0 ? listed : isCli ? [] : (preset?.efforts ?? EFFORTS);
  const listedDefault = fetchState.models?.find((m) => m.id === form.model.trim())?.defaultEffort;
  const effortPlaceholder = listedDefault
    ? tm.defaultIs(listedDefault)
    : isCli
      ? tm.cliDefault
      : tm.serverDefault;
  // Agent CLIs pick from their list (the back end refuses anything else); with no list (not
  // fetched, or the CLI could not give one) the effort is typed. APIs always type it, warned when
  // the chosen model lists efforts that do not include it.
  const effortChoices = isCli && listed.length > 0 ? listed : null;
  const typedEffort = form.effort.trim();
  const effortReference = isCli
    ? listed
    : (ownEfforts(fetchState.models, form.model.trim()) ?? []);
  const effortOff =
    typedEffort !== "" && effortReference.length > 0 && !effortReference.includes(typedEffort);

  return (
    <form
      className="inline-card stack"
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div className="form-grid-2">
        <label className="form-field">
          <span>{tm.alias}</span>
          <input
            type="text"
            placeholder={tm.aliasPlaceholder}
            value={form.alias}
            onChange={(e) =>
              setForm({ ...form, alias: e.target.value })
            }
            required
          />
        </label>
        <label className="form-field">
          <span>{tm.provider}</span>
          <select
            value={form.provider}
            onChange={(e) => onProviderChange(e.target.value)}
          >
            {providers.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name} ({tm.kindName(p.kind)})
              </option>
            ))}
          </select>
        </label>
        <label className="form-field">
          <span>
            {tm.modelId}
            {isCli && tm.optionalSuffix}
          </span>
          <div className="input-with-action">
            <input
              type="text"
              list="aj-model-ids"
              placeholder={isCli ? tm.cliDefault : tm.modelIdPlaceholder}
              value={form.model}
              onChange={(e) =>
                setForm({ ...form, model: e.target.value })
              }
            />
            <button
              type="button"
              className="btn btn-secondary btn-xs"
              disabled={fetchState.loading || !form.provider}
              onClick={() => onFetch(form.provider)}
            >
              {fetchState.loading ? tm.fetching : tm.fetchList}
            </button>
          </div>
          <datalist id="aj-model-ids">
            {(fetchState.models ?? []).map((m) => (
              <option key={m.id} value={m.id} />
            ))}
          </datalist>
        </label>
        <label className="form-field">
          <span>{tm.effort}</span>
          {effortChoices ? (
            <select
              value={typedEffort}
              onChange={(e) => setForm({ ...form, effort: e.target.value })}
            >
              <option value="">{effortPlaceholder}</option>
              {effortChoices.map((x) => (
                <option key={x} value={x}>
                  {x}
                </option>
              ))}
              {effortOff && (
                <option value={typedEffort}>{tm.effortUnlisted(typedEffort)}</option>
              )}
            </select>
          ) : (
            <>
              <input
                type="text"
                list="aj-efforts"
                placeholder={effortPlaceholder}
                value={form.effort}
                onChange={(e) => setForm({ ...form, effort: e.target.value })}
              />
              <datalist id="aj-efforts">
                {effortHints.map((x) => (
                  <option key={x} value={x} />
                ))}
              </datalist>
            </>
          )}
        </label>
        {provider?.kind === "openai_compat" && (
          <label className="form-field">
            <span className="title-with-tip">
              {tm.temperature}
              <InfoTip>{tm.temperatureTip}</InfoTip>
            </span>
            <input
              type="text"
              inputMode="decimal"
              placeholder={tm.serverDefault}
              value={form.temperature}
              onChange={(e) =>
                setForm({
                  ...form,
                  temperature: e.target.value.replace(/[^0-9.]/g, ""),
                })
              }
            />
          </label>
        )}
      </div>

      {effortOff && (
        <div className="warning-list">
          {(isCli ? tm.effortRefused : tm.effortMayFail)(
            typedEffort,
            effortReference.join(", "),
          )}
        </div>
      )}
      {fetchState.error && (
        <div className="warning-list">{fetchState.error}</div>
      )}
      {fetchState.models && (
        <div className="stack">
          <input
            type="text"
            placeholder={tm.filter(fetchState.models.length)}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <div className="model-id-list">
            {filteredIds.slice(0, 200).map((id) => (
              <button
                key={id}
                type="button"
                className={
                  "model-id-chip" +
                  (id === form.model ? " selected" : "")
                }
                onClick={() =>
                  setForm({
                    ...form,
                    model: id,
                    alias: form.alias || aliasFromModelId(id),
                  })
                }
              >
                {id}
              </button>
            ))}
            {filteredIds.length === 0 && (
              <span className="muted-line">{tm.noMatches}</span>
            )}
          </div>
        </div>
      )}

      <div className="settings-card-foot form-foot-end">
        <div className="toolbar-actions">
          <button
            type="button"
            className="btn btn-ghost btn-xs"
            onClick={onCancel}
          >
            {tr.common.cancel}
          </button>
          <button type="submit" className="btn btn-primary btn-xs" disabled={busy}>
            {tm.saveModel}
          </button>
        </div>
      </div>
    </form>
  );
}
