import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { InfoTip } from "./InfoTip";
import type {
  EngineTestResult,
  ModelSettings,
  ProviderInput,
  ProviderKind,
  ProviderPreset,
  ProviderSetting,
} from "../types";
import { useT } from "../i18n";

/** Settings > Models: providers (connections and keys), model aliases, and the default model. */

interface ModelsViewProps {
  /** Called after every successful change so the workbench can refresh its model list. */
  onChanged: (settings: ModelSettings) => void;
}

/** `reasoning_effort` hints for providers without a matching preset. */
const EFFORTS = ["none", "minimal", "low", "medium", "high", "max"];

const PRESET_GROUPS: ProviderPreset["group"][] = ["vendor", "aggregator", "local"];

function normalizeUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

interface ProviderForm {
  originalName: string | null;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  timeout: string;
  binary: string;
  keyInput: string;
  clearKey: boolean;
  hasKey: boolean;
}

interface ModelForm {
  originalAlias: string | null;
  alias: string;
  provider: string;
  model: string;
  effort: string;
}

interface FetchState {
  loading: boolean;
  ids: string[] | null;
  error: string | null;
}

const IDLE_FETCH: FetchState = { loading: false, ids: null, error: null };

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function providerInput(f: ProviderForm): ProviderInput {
  const timeout = parseInt(f.timeout, 10);
  return {
    kind: f.kind,
    baseUrl: f.kind === "openai_compat" ? f.baseUrl.trim() || null : null,
    timeoutSecs:
      f.kind === "openai_compat" && Number.isFinite(timeout) ? timeout : null,
    binary: f.kind === "claude_cli" ? f.binary.trim() || null : null,
  };
}

function providerToInput(p: ProviderSetting): ProviderInput {
  return {
    kind: p.kind,
    baseUrl: p.baseUrl,
    timeoutSecs: p.timeoutSecs,
    binary: p.binary,
  };
}

/** A model id turned into a usable alias: `deepseek/deepseek-chat` -> `deepseek-chat`. */
function aliasFromModelId(id: string): string {
  const tail = id.split("/").pop() ?? id;
  return tail.replace(/[^A-Za-z0-9._-]/g, "-").slice(0, 40);
}

/** Model alias for a fetched id that does not collide with existing aliases. */
function uniqueAlias(id: string, provider: string, taken: Set<string>): string {
  const base = aliasFromModelId(id);
  if (!taken.has(base)) return base;
  const withProvider = `${base}-${provider}`.slice(0, 40);
  if (!taken.has(withProvider)) return withProvider;
  for (let i = 2; ; i++) {
    const candidate = `${withProvider.slice(0, 36)}-${i}`;
    if (!taken.has(candidate)) return candidate;
  }
}

function TestBadge({ result }: { result: EngineTestResult }) {
  const tm = useT().models;
  return (
    <span className={"test-result " + (result.ok ? "ok" : "err")}>
      <svg
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        {result.ok ? (
          <polyline points="4 12.5 9.5 18 20 6" />
        ) : (
          <path d="M6 6l12 12M18 6L6 18" />
        )}
      </svg>
      {result.ok ? tm.success : tm.failed}
      <span className="test-result-time">
        {(result.elapsedMs / 1000).toFixed(1)}s
      </span>
    </span>
  );
}

function PlusIcon() {
  return (
    <svg
      width="11"
      height="11"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.6"
      strokeLinecap="round"
      aria-hidden
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
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
  const [modelFilter, setModelFilter] = useState("");
  const [browsing, setBrowsing] = useState<string | null>(null);
  const [browseFetch, setBrowseFetch] = useState<FetchState>(IDLE_FETCH);
  const [browseFilter, setBrowseFilter] = useState("");
  const [tests, setTests] = useState<Record<string, EngineTestResult | "running">>(
    {},
  );
  const [presets, setPresets] = useState<ProviderPreset[]>([]);

  // The error banner sits at the top; bring it into view when it appears.
  const errorRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (error) errorRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [error]);

  useEffect(() => {
    api
      .modelSettings()
      .then(setSettings)
      .catch((e) => setError(errText(e)));
    api
      .providerPresets()
      .then(setPresets)
      .catch((e) => setError(errText(e)));
  }, []);

  const presetLabel = (p: ProviderPreset) =>
    p.region ? tm.presetRegion(p.name, p.region) : p.name;

  const apply = async (fn: () => Promise<ModelSettings>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const next = await fn();
      setSettings(next);
      onChanged(next);
      return true;
    } catch (e) {
      setError(errText(e));
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
    setProviderFetch({ loading: true, ids: null, error: null });
    try {
      const ids = await api.listProviderModels(
        providerForm.originalName,
        providerInput(providerForm),
        providerForm.keyInput.trim() || null,
      );
      setProviderFetch({ loading: false, ids, error: null });
    } catch (e) {
      setProviderFetch({ loading: false, ids: null, error: errText(e) });
    }
  };

  // ---- model form ----

  const openModelForm = (
    init: Partial<ModelForm> & { provider: string },
  ) => {
    setModelFetch(IDLE_FETCH);
    setModelFilter("");
    setModelForm({
      originalAlias: null,
      alias: "",
      model: "",
      effort: "",
      ...init,
    });
  };

  const fetchForModelForm = async (providerName: string) => {
    const p = providerOf(providerName);
    if (!p) return;
    setModelFetch({ loading: true, ids: null, error: null });
    try {
      const ids = await api.listProviderModels(p.name, providerToInput(p), null);
      setModelFetch({ loading: false, ids, error: null });
    } catch (e) {
      setModelFetch({ loading: false, ids: null, error: errText(e) });
    }
  };

  const saveModel = async () => {
    if (!modelForm) return;
    const f = modelForm;
    const isCli = providerOf(f.provider)?.kind === "claude_cli";
    const ok = await apply(() =>
      api.saveModel(f.originalAlias, f.alias.trim(), {
        provider: f.provider,
        model: f.model.trim() || null,
        reasoningEffort: isCli ? null : f.effort.trim() || null,
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
        [alias]: { ok: false, elapsedMs: 0, message: errText(e) },
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
    setBrowseFetch({ loading: true, ids: null, error: null });
    try {
      const ids = await api.listProviderModels(p.name, providerToInput(p), null);
      setBrowseFetch({ loading: false, ids, error: null });
    } catch (e) {
      setBrowseFetch({ loading: false, ids: null, error: errText(e) });
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
        api.saveModel(null, alias, { provider, model: id, reasoningEffort: null }),
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
    const ids = browseFetch.ids ?? [];
    const q = browseFilter.trim().toLowerCase();
    return q ? ids.filter((id) => id.toLowerCase().includes(q)) : ids;
  }, [browseFetch.ids, browseFilter]);

  const filteredIds = useMemo(() => {
    const ids = modelFetch.ids ?? [];
    const q = modelFilter.trim().toLowerCase();
    return q ? ids.filter((id) => id.toLowerCase().includes(q)) : ids;
  }, [modelFetch.ids, modelFilter]);

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

  const modelFormProvider = modelForm ? providerOf(modelForm.provider) : undefined;
  const modelFormPreset = modelFormProvider?.baseUrl
    ? presets.find(
        (p) => p.baseUrl === normalizeUrl(modelFormProvider.baseUrl ?? ""),
      )
    : undefined;
  const effortHints = modelFormPreset?.efforts ?? EFFORTS;

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
        </div>
      </div>

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
                provider:
                  providers.find((p) => !p.isBuiltin)?.name ??
                  providers[0]?.name ??
                  "",
              })
            }
          >
            <PlusIcon />
            {tm.addModel}
          </button>
        </div>

        {modelForm && (
          <form
            className="inline-card stack"
            onSubmit={(e) => {
              e.preventDefault();
              void saveModel();
            }}
          >
            <div className="form-grid-2">
              <label className="form-field">
                <span>{tm.alias}</span>
                <input
                  type="text"
                  placeholder={tm.aliasPlaceholder}
                  value={modelForm.alias}
                  disabled={modelForm.originalAlias === "claude"}
                  onChange={(e) =>
                    setModelForm({ ...modelForm, alias: e.target.value })
                  }
                  required
                />
              </label>
              <label className="form-field">
                <span>{tm.provider}</span>
                <select
                  value={modelForm.provider}
                  onChange={(e) => {
                    setModelFetch(IDLE_FETCH);
                    setModelForm({ ...modelForm, provider: e.target.value });
                  }}
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
                  {modelFormProvider?.kind === "claude_cli" && tm.optionalSuffix}
                </span>
                <div className="input-with-action">
                  <input
                    type="text"
                    list="aj-model-ids"
                    placeholder={
                      modelFormProvider?.kind === "claude_cli"
                        ? tm.cliDefault
                        : tm.modelIdPlaceholder
                    }
                    value={modelForm.model}
                    onChange={(e) =>
                      setModelForm({ ...modelForm, model: e.target.value })
                    }
                  />
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    disabled={modelFetch.loading || !modelForm.provider}
                    onClick={() => void fetchForModelForm(modelForm.provider)}
                  >
                    {modelFetch.loading ? tm.fetching : tm.fetchList}
                  </button>
                </div>
                <datalist id="aj-model-ids">
                  {(modelFetch.ids ?? []).map((id) => (
                    <option key={id} value={id} />
                  ))}
                </datalist>
              </label>
              {modelFormProvider?.kind !== "claude_cli" && (
                <label className="form-field">
                  <span>{tm.effort}</span>
                  <input
                    type="text"
                    list="aj-efforts"
                    placeholder={tm.serverDefault}
                    value={modelForm.effort}
                    onChange={(e) =>
                      setModelForm({ ...modelForm, effort: e.target.value })
                    }
                  />
                  <datalist id="aj-efforts">
                    {effortHints.map((x) => (
                      <option key={x} value={x} />
                    ))}
                  </datalist>
                </label>
              )}
            </div>

            {modelFetch.error && (
              <div className="warning-list">{modelFetch.error}</div>
            )}
            {modelFetch.ids && (
              <div className="stack">
                <input
                  type="text"
                  placeholder={tm.filter(modelFetch.ids.length)}
                  value={modelFilter}
                  onChange={(e) => setModelFilter(e.target.value)}
                />
                <div className="model-id-list">
                  {filteredIds.slice(0, 200).map((id) => (
                    <button
                      key={id}
                      type="button"
                      className={
                        "model-id-chip" +
                        (id === modelForm.model ? " selected" : "")
                      }
                      onClick={() =>
                        setModelForm({
                          ...modelForm,
                          model: id,
                          alias: modelForm.alias || aliasFromModelId(id),
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
                  onClick={() => setModelForm(null)}
                >
                  {tr.common.cancel}
                </button>
                <button type="submit" className="btn btn-primary btn-xs" disabled={busy}>
                  {tm.saveModel}
                </button>
              </div>
            </div>
          </form>
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
                    {[m.provider, m.model ?? tm.defaultModelId, m.reasoningEffort]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  {t && t !== "running" && !t.ok && (
                    <div className="test-result-msg">{t.message}</div>
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
                      })
                    }
                  >
                    {tr.common.edit}
                  </button>
                  {!m.isBuiltin && (
                    <button
                      type="button"
                      className="btn btn-ghost-danger btn-xs"
                      disabled={busy}
                      onClick={() => {
                        if (window.confirm(tm.deleteModel(m.alias))) {
                          void apply(() => api.deleteModel(m.alias));
                        }
                      }}
                    >
                      {tr.common.delete}
                    </button>
                  )}
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
          <form
            className="inline-card stack"
            autoComplete="off"
            onSubmit={(e) => {
              e.preventDefault();
              void saveProvider();
            }}
          >
            <div className="form-grid-2">
              <label className="form-field">
                <span>{tm.name}</span>
                <input
                  type="text"
                  placeholder={tm.namePlaceholder}
                  value={providerForm.name}
                  disabled={providerForm.originalName === "claude"}
                  onChange={(e) =>
                    setProviderForm({ ...providerForm, name: e.target.value })
                  }
                  required
                />
              </label>
              <label className="form-field">
                <span>{tm.type}</span>
                <select
                  value={providerForm.kind}
                  disabled={providerForm.originalName === "claude"}
                  onChange={(e) =>
                    setProviderForm({
                      ...providerForm,
                      kind: e.target.value as ProviderKind,
                    })
                  }
                >
                  <option value="openai_compat">{tm.openaiType}</option>
                  <option value="claude_cli">{tm.cliType}</option>
                </select>
              </label>

              {providerForm.kind === "openai_compat" ? (
                <>
                  <label className="form-field">
                    <span>{tm.baseUrl}</span>
                    <input
                      type="text"
                      placeholder="https://host/v1"
                      value={providerForm.baseUrl}
                      onChange={(e) =>
                        setProviderForm({ ...providerForm, baseUrl: e.target.value })
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
                        setProviderForm({
                          ...providerForm,
                          baseUrl: preset.baseUrl,
                          name: providerForm.name || preset.id,
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
                      {providerForm.hasKey && !providerForm.clearKey && (
                        <span className="badge badge-ok">{tm.stored}</span>
                      )}
                    </span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      spellCheck={false}
                      placeholder={
                        providerForm.hasKey
                          ? tm.keepKey
                          : tm.noAuth
                      }
                      value={providerForm.keyInput}
                      onChange={(e) =>
                        setProviderForm({
                          ...providerForm,
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
                      value={providerForm.timeout}
                      onChange={(e) =>
                        setProviderForm({
                          ...providerForm,
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
                    placeholder="claude"
                    value={providerForm.binary}
                    onChange={(e) =>
                      setProviderForm({ ...providerForm, binary: e.target.value })
                    }
                  />
                </label>
              )}
            </div>

            {providerFetch.error && (
              <div className="warning-list">{providerFetch.error}</div>
            )}
            {providerFetch.ids && (
              <div className="test-result ok">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <polyline points="4 12.5 9.5 18 20 6" />
                </svg>
                {tm.connected(providerFetch.ids.length)}
              </div>
            )}

            <div className="settings-card-foot">
              <div className="toolbar-actions">
                <button
                  type="button"
                  className="btn btn-secondary btn-xs"
                  disabled={providerFetch.loading}
                  onClick={() => void fetchForProviderForm()}
                >
                  {providerFetch.loading ? tm.fetching : tm.fetchModels}
                </button>
                {providerForm.kind === "openai_compat" && providerForm.hasKey && (
                  <label className="toggle-row">
                    <input
                      type="checkbox"
                      checked={providerForm.clearKey}
                      onChange={(e) =>
                        setProviderForm({
                          ...providerForm,
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
                  onClick={() => setProviderForm(null)}
                >
                  {tr.common.cancel}
                </button>
                <button type="submit" className="btn btn-primary btn-xs" disabled={busy}>
                  {tm.saveProvider}
                </button>
              </div>
            </div>
          </form>
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
                  {p.kind === "claude_cli" ? (p.binary ?? "claude") : p.baseUrl}
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
                {!p.isBuiltin && (
                  <button
                    type="button"
                    className="btn btn-ghost-danger btn-xs"
                    disabled={busy || p.usedBy.length > 0}
                    title={
                      p.usedBy.length > 0
                        ? tm.usedBy
                        : undefined
                    }
                    onClick={() => {
                      if (
                        window.confirm(
                          tm.deleteProvider(p.name),
                        )
                      ) {
                        void apply(() => api.deleteProvider(p.name));
                      }
                    }}
                  >
                    {tr.common.delete}
                  </button>
                )}
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
                {browseFetch.ids && (
                  <>
                    <div className="provider-models-head">
                      <input
                        type="text"
                        placeholder={tm.filter(browseFetch.ids.length)}
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
    </>
  );
}
