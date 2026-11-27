import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../../api";
import { InfoTip } from "../../InfoTip";
import { FailureNote } from "../../FailureNote";
import { LocalToolsCard } from "./LocalToolsCard";
import { PlusIcon } from "../../icons";
import { isCliKind } from "../../../types";
import type { EngineTestResult, ModelSettings, ProviderPreset, ProviderSetting } from "../../../types";
import { useT } from "../../../i18n";
import { errorText } from "../../../errors";
import { DeleteConfirm, type PendingDelete } from "./DeleteConfirm";
import {
  IDLE_FETCH,
  LOADING_FETCH,
  defaultCommand,
  parseTemperature,
  providerInput,
  providerToInput,
  uniqueAlias,
  type FetchState,
  type ModelForm,
  type ProviderForm,
} from "./forms";
import { ModelFormCard } from "./ModelForm";
import { ProviderFormCard } from "./ProviderForm";
import { TestBadge } from "./TestBadge";

/** Settings > Models: the default model, agent CLIs on this Mac, model aliases and providers
 * (connections and keys). */

interface ModelsViewProps {
  /** Called after every successful change so the workbench can refresh its model list. */
  onChanged: (settings: ModelSettings) => void;
}

export function ModelsView({ onChanged }: ModelsViewProps) {
  const tr = useT();
  const tm = tr.models;
  const [settings, setSettings] = useState<ModelSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [providerForm, setProviderForm] = useState<ProviderForm | null>(null);
  const [providerFetch, setProviderFetch] = useState<FetchState>(IDLE_FETCH);
  const [modelForm, setModelForm] = useState<ModelForm | null>(null);
  const [modelFetch, setModelFetch] = useState<FetchState>(IDLE_FETCH);
  // Bumped by every model-form fetch and reset, so a slow list for a provider the form has
  // since left (a CLI fetch starts by itself) cannot overwrite the current one.
  const modelFetchSeq = useRef(0);
  const [modelFilter, setModelFilter] = useState("");
  const [browsing, setBrowsing] = useState<string | null>(null);
  const [browseFetch, setBrowseFetch] = useState<FetchState>(IDLE_FETCH);
  const [browseFilter, setBrowseFilter] = useState("");
  const [tests, setTests] = useState<Record<string, EngineTestResult | "running">>(
    {},
  );
  const [presets, setPresets] = useState<ProviderPreset[]>([]);
  const [pendingDelete, setPendingDelete] = useState<PendingDelete | null>(null);

  // The error banner sits at the top; bring it into view when it appears.
  const errorRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (error) errorRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [error]);

  useEffect(() => {
    api
      .modelSettings()
      .then(setSettings)
      .catch((e) => setError(errorText(e)));
    api
      .providerPresets()
      .then(setPresets)
      .catch((e) => setError(errorText(e)));
  }, []);

  const apply = async (fn: () => Promise<ModelSettings>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const next = await fn();
      setSettings(next);
      onChanged(next);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const providers = settings?.providers ?? [];
  const providerOf = (name: string) => providers.find((p) => p.name === name);

  // ---- provider form ----

  const openProviderForm = (p: ProviderSetting | null) => {
    setProviderFetch(IDLE_FETCH);
    setProviderForm(
      p
        ? {
            originalName: p.name,
            name: p.name,
            kind: p.kind,
            baseUrl: p.baseUrl ?? "",
            timeout: p.timeoutSecs ? String(p.timeoutSecs) : "",
            binary: p.binary ?? "",
            keyInput: "",
            clearKey: false,
            hasKey: p.hasKey,
          }
        : {
            originalName: null,
            name: "",
            kind: "openai_compat",
            baseUrl: "",
            timeout: "",
            binary: "",
            keyInput: "",
            clearKey: false,
            hasKey: false,
          },
    );
  };

  const saveProvider = async () => {
    if (!providerForm) return;
    const f = providerForm;
    const key = f.keyInput.trim()
      ? ({ mode: "set", value: f.keyInput.trim() } as const)
      : f.clearKey
        ? ({ mode: "clear" } as const)
        : ({ mode: "keep" } as const);
    const ok = await apply(() =>
      api.saveProvider(f.originalName, f.name.trim(), providerInput(f), key),
    );
    if (ok) setProviderForm(null);
  };

  const fetchForProviderForm = async () => {
    if (!providerForm) return;
    setProviderFetch(LOADING_FETCH);
    try {
      const models = await api.listProviderModels(
        providerForm.originalName,
        providerInput(providerForm),
        providerForm.keyInput.trim() || null,
      );
      setProviderFetch({ loading: false, models, error: null });
    } catch (e) {
      setProviderFetch({ loading: false, models: null, error: errorText(e) });
    }
  };

  // ---- model form ----

  const resetModelFetch = () => {
    modelFetchSeq.current += 1;
    setModelFetch(IDLE_FETCH);
  };

  const fetchForModelForm = async (providerName: string) => {
    const p = providerOf(providerName);
    if (!p) return;
    const seq = ++modelFetchSeq.current;
    const settle = (next: FetchState) => {
      if (seq === modelFetchSeq.current) setModelFetch(next);
    };
    setModelFetch(LOADING_FETCH);
    try {
      const models = await api.listProviderModels(p.name, providerToInput(p), null);
      settle({ loading: false, models, error: null });
    } catch (e) {
      settle({ loading: false, models: null, error: errorText(e) });
    }
  };

  /** Open the model form; for an agent CLI the model list (with efforts) is fetched right away,
   * since it comes from this Mac rather than a remote service. */
  const openModelForm = (
    init: Partial<ModelForm> & { provider: string },
  ) => {
    resetModelFetch();
    setModelFilter("");
    setModelForm({
      originalAlias: null,
      alias: "",
      model: "",
      effort: "",
      temperature: "",
      ...init,
    });
    if (isCliKind(providerOf(init.provider)?.kind)) void fetchForModelForm(init.provider);
  };

  const saveModel = async () => {
    if (!modelForm) return;
    const f = modelForm;
    const isApi = providerOf(f.provider)?.kind === "openai_compat";
    const temperature = isApi ? parseTemperature(f.temperature) : null;
    if (temperature === undefined) {
      setError(tm.badTemperature);
      return;
    }
    const ok = await apply(() =>
      api.saveModel(f.originalAlias, f.alias.trim(), {
        provider: f.provider,
        model: f.model.trim() || null,
        reasoningEffort: f.effort.trim() || null,
        temperature,
      }),
    );
    if (ok) setModelForm(null);
  };

  const runTest = async (alias: string) => {
    setTests((t) => ({ ...t, [alias]: "running" }));
    try {
      const res = await api.testModel(alias);
      setTests((t) => ({ ...t, [alias]: res }));
    } catch (e) {
      setTests((t) => ({
        ...t,
        [alias]: { ok: false, elapsedMs: 0, message: errorText(e), failure: null },
      }));
    }
  };

  const toggleBrowse = async (p: ProviderSetting) => {
    if (browsing === p.name) {
      setBrowsing(null);
      return;
    }
    setBrowsing(p.name);
    setBrowseFilter("");
    setBrowseFetch(LOADING_FETCH);
    try {
      const models = await api.listProviderModels(p.name, providerToInput(p), null);
      setBrowseFetch({ loading: false, models, error: null });
    } catch (e) {
      setBrowseFetch({ loading: false, models: null, error: errorText(e) });
    }
  };

  /** Enable a fetched model as an alias, or disable it by deleting its aliases. */
  const toggleModelId = async (provider: string, id: string) => {
    if (!settings) return;
    const existing = settings.models.filter(
      (m) => m.provider === provider && m.model === id,
    );
    if (existing.length === 0) {
      const taken = new Set(settings.models.map((m) => m.alias));
      const alias = uniqueAlias(id, provider, taken);
      await apply(() =>
        api.saveModel(null, alias, {
          provider,
          model: id,
          reasoningEffort: null,
          temperature: null,
        }),
      );
      return;
    }
    const def = existing.find((m) => m.isDefault);
    if (def) {
      setError(
        tm.defaultInUse(def.alias),
      );
      return;
    }
    for (const m of existing) {
      if (!(await apply(() => api.deleteModel(m.alias)))) return;
    }
  };

  const browseIds = useMemo(() => {
    const ids = (browseFetch.models ?? []).map((m) => m.id);
    const q = browseFilter.trim().toLowerCase();
    return q ? ids.filter((id) => id.toLowerCase().includes(q)) : ids;
  }, [browseFetch.models, browseFilter]);

  if (!settings) {
    return (
      <div className="settings-card">
        {error ? (
          <div className="warning-list">{error}</div>
        ) : (
          <div className="empty-state">{tm.loading}</div>
        )}
      </div>
    );
  }

  return (
    <>
      {error && (
        <div ref={errorRef} className="banner banner-err">
          <span>{error}</span>
          <button
            type="button"
            className="btn btn-ghost btn-xs"
            onClick={() => setError(null)}
          >
            {tm.dismiss}
          </button>
        </div>
      )}

      {/* Default model */}
      <div className="settings-card">
        <div className="settings-row clean">
          <div className="settings-row-title title-with-tip">
            {tm.defaultModel}
            <InfoTip>{tm.defaultModelTip}</InfoTip>
          </div>
          {settings.defaultModel === null ? (
            <span className="muted-line">{tm.noModel}</span>
          ) : (
            <select
              value={settings.defaultModel}
              disabled={busy}
              onChange={(e) => {
                const alias = e.target.value;
                void apply(() => api.setDefaultModel(alias));
              }}
            >
              {settings.models.map((m) => (
                <option key={m.alias} value={m.alias}>
                  {m.alias}
                </option>
              ))}
            </select>
          )}
        </div>
      </div>

      <LocalToolsCard busy={busy} apply={apply} />

      {/* Model aliases */}
      <div className="settings-card">
        <div className="settings-card-head">
          <h3 className="title-with-tip">
            {tm.models}
            <InfoTip>{tm.modelsTip}</InfoTip>
          </h3>
          <button
            type="button"
            className="btn btn-add btn-xs"
            disabled={busy}
            onClick={() =>
              openModelForm({
                provider: providers[0]?.name ?? "",
              })
            }
          >
            <PlusIcon />
            {tm.addModel}
          </button>
        </div>

        {modelForm && (
          <ModelFormCard
            form={modelForm}
            setForm={setModelForm}
            providers={providers}
            presets={presets}
            fetchState={modelFetch}
            filter={modelFilter}
            setFilter={setModelFilter}
            busy={busy}
            onProviderChange={(provider) => {
              resetModelFetch();
              setModelForm({ ...modelForm, provider });
              if (isCliKind(providerOf(provider)?.kind)) void fetchForModelForm(provider);
            }}
            onFetch={(provider) => void fetchForModelForm(provider)}
            onSave={() => void saveModel()}
            onCancel={() => setModelForm(null)}
          />
        )}

        <div className="settings-list">
          {settings.models.map((m) => {
            const t = tests[m.alias];
            return (
              <div key={m.alias} className="settings-item-row">
                <div className="settings-item-main">
                  <div className="slice-title-line">
                    <strong>{m.alias}</strong>
                    {m.isDefault && <span className="badge badge-ok">{tm.default}</span>}
                    {t && t !== "running" && <TestBadge result={t} />}
                  </div>
                  <div className="muted-line">
                    {[
                      m.provider,
                      m.model ?? tm.defaultModelId,
                      m.reasoningEffort,
                      m.temperature !== null && tm.temperatureValue(m.temperature),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  {t && t !== "running" && !t.ok && (
                    <div className="test-result-msg">
                      <FailureNote failure={t.failure} message={t.message} />
                    </div>
                  )}
                </div>
                <div className="settings-item-actions">
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    disabled={t === "running"}
                    onClick={() => void runTest(m.alias)}
                  >
                    {t === "running" ? tm.testing : tm.test}
                  </button>
                  {!m.isDefault && (
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs"
                      disabled={busy}
                      onClick={() => void apply(() => api.setDefaultModel(m.alias))}
                    >
                      {tm.setDefault}
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs"
                    disabled={busy}
                    onClick={() =>
                      openModelForm({
                        originalAlias: m.alias,
                        alias: m.alias,
                        provider: m.provider,
                        model: m.model ?? "",
                        effort: m.reasoningEffort ?? "",
                        temperature: m.temperature !== null ? String(m.temperature) : "",
                      })
                    }
                  >
                    {tr.common.edit}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost-danger btn-xs"
                    disabled={busy}
                    onClick={() => setPendingDelete({ kind: "model", name: m.alias })}
                  >
                    {tr.common.delete}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Providers */}
      <div className="settings-card">
        <div className="settings-card-head">
          <h3 className="title-with-tip">
            {tm.providers}
            <InfoTip>
              {tm.providersTipBefore}
              {settings.keyStore}
              {tm.providersTipAfter}
              <code>config.toml</code>.
              <br />
              {tm.config}
              <code>{settings.configPath}</code>
            </InfoTip>
          </h3>
          <button
            type="button"
            className="btn btn-add btn-xs"
            disabled={busy}
            onClick={() => openProviderForm(null)}
          >
            <PlusIcon />
            {tm.addProvider}
          </button>
        </div>

        {providerForm && (
          <ProviderFormCard
            form={providerForm}
            setForm={setProviderForm}
            presets={presets}
            fetchState={providerFetch}
            busy={busy}
            onFetch={() => void fetchForProviderForm()}
            onSave={() => void saveProvider()}
            onCancel={() => setProviderForm(null)}
          />
        )}

        <div className="settings-list">
          {providers.map((p) => (
            <Fragment key={p.name}>
            <div className="settings-item-row">
              <div className="settings-item-main">
                <div className="slice-title-line">
                  <strong>{p.name}</strong>
                  <span className="badge">
                    {tm.kindName(p.kind)}
                  </span>
                  {p.kind === "openai_compat" &&
                    (p.hasKey ? (
                      <span className="badge badge-ok">{tm.keyStored}</span>
                    ) : (
                      <span className="badge badge-warn">{tm.noKey}</span>
                    ))}
                </div>
                <div className="muted-line">
                  {isCliKind(p.kind) ? (p.binary ?? defaultCommand(p.kind)) : p.baseUrl}
                </div>
              </div>
              <div className="settings-item-actions">
                <button
                  type="button"
                  className={
                    "btn btn-xs " +
                    (browsing === p.name ? "btn-secondary" : "btn-ghost")
                  }
                  onClick={() => void toggleBrowse(p)}
                >
                  {browsing === p.name ? tm.hideModels : tm.browseModels}
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-xs"
                  disabled={busy}
                  onClick={() => openProviderForm(p)}
                >
                  {tr.common.edit}
                </button>
                <button
                  type="button"
                  className="btn btn-ghost-danger btn-xs"
                  disabled={busy}
                  onClick={() =>
                    setPendingDelete({ kind: "provider", name: p.name, hasKey: p.hasKey, usedBy: p.usedBy })
                  }
                >
                  {tr.common.delete}
                </button>
              </div>
            </div>
            {browsing === p.name && (
              <div className="provider-models-panel">
                {browseFetch.loading && (
                  <div className="muted-line">{tm.fetchingFrom(p.name)}</div>
                )}
                {browseFetch.error && (
                  <div className="warning-list">{browseFetch.error}</div>
                )}
                {browseFetch.models && (
                  <>
                    <div className="provider-models-head">
                      <input
                        type="text"
                        placeholder={tm.filter(browseFetch.models.length)}
                        value={browseFilter}
                        onChange={(e) => setBrowseFilter(e.target.value)}
                      />
                      <InfoTip>{tm.browseTip}</InfoTip>
                    </div>
                    <div className="model-id-list">
                      {browseIds.slice(0, 300).map((id) => {
                        const enabled = settings.models.some(
                          (m) => m.provider === p.name && m.model === id,
                        );
                        return (
                          <button
                            key={id}
                            type="button"
                            className={"model-id-chip" + (enabled ? " selected" : "")}
                            disabled={busy}
                            onClick={() => void toggleModelId(p.name, id)}
                          >
                            {enabled && (
                              <svg
                                width="10"
                                height="10"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                strokeWidth="3.2"
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                aria-hidden
                              >
                                <polyline points="4 12.5 9.5 18 20 6" />
                              </svg>
                            )}
                            {id}
                          </button>
                        );
                      })}
                      {browseIds.length === 0 && (
                        <span className="muted-line">{tm.noMatches}</span>
                      )}
                    </div>
                  </>
                )}
              </div>
            )}
            </Fragment>
          ))}
        </div>
      </div>
      {pendingDelete && (
        <DeleteConfirm
          target={pendingDelete}
          onClose={() => setPendingDelete(null)}
          onDelete={() => {
            const target = pendingDelete;
            setPendingDelete(null);
            void apply(() =>
              target.kind === "model"
                ? api.deleteModel(target.name)
                : api.deleteProvider(target.name),
            );
          }}
        />
      )}
    </>
  );
}
