// The Models settings' form state and the pure helpers around it: what a form sends, which
// reasoning efforts to offer, and aliases for fetched model ids.

import { isCliKind } from "../../../types";
import type {
  ModelInfo,
  ProviderInput,
  ProviderKind,
  ProviderPreset,
  ProviderSetting,
} from "../../../types";

/** `reasoning_effort` hints for providers without a matching preset. */
export const EFFORTS = ["none", "minimal", "low", "medium", "high", "max"];

export const PRESET_GROUPS: ProviderPreset["group"][] = ["vendor", "aggregator", "local"];

export function normalizeUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

export interface ProviderForm {
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

export interface ModelForm {
  originalAlias: string | null;
  alias: string;
  provider: string;
  model: string;
  effort: string;
  temperature: string;
}

export interface FetchState {
  loading: boolean;
  models: ModelInfo[] | null;
  error: string | null;
}

export const IDLE_FETCH: FetchState = { loading: false, models: null, error: null };
export const LOADING_FETCH: FetchState = { loading: true, models: null, error: null };

/** Command run when a CLI provider has none set. */
export function defaultCommand(kind: ProviderKind): string {
  return kind === "codex_cli" ? "codex" : "claude";
}

/** Reasoning effort values to suggest: the chosen model's, else every value a listed model takes. */
export function listedEfforts(models: ModelInfo[] | null, id: string): string[] {
  if (!models) return [];
  const chosen = models.find((m) => m.id === id);
  if (chosen) return chosen.efforts;
  return [...new Set(models.flatMap((m) => m.efforts))];
}

/** The efforts a model lists for itself, or null when it is not listed with any. */
export function ownEfforts(models: ModelInfo[] | null, id: string): string[] | null {
  const chosen = models?.find((m) => m.id === id);
  return chosen && chosen.efforts.length > 0 ? chosen.efforts : null;
}

/** Highest temperature accepted (`aj_report::config::MAX_TEMPERATURE`). */
const MAX_TEMPERATURE = 2;

/** A temperature typed in the form: a number, null when blank, undefined when invalid. */
export function parseTemperature(text: string): number | null | undefined {
  const t = text.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) && n >= 0 && n <= MAX_TEMPERATURE ? n : undefined;
}

export function providerInput(f: ProviderForm): ProviderInput {
  const timeout = parseInt(f.timeout, 10);
  return {
    kind: f.kind,
    baseUrl: f.kind === "openai_compat" ? f.baseUrl.trim() || null : null,
    timeoutSecs:
      f.kind === "openai_compat" && Number.isFinite(timeout) ? timeout : null,
    binary: isCliKind(f.kind) ? f.binary.trim() || null : null,
  };
}

export function providerToInput(p: ProviderSetting): ProviderInput {
  return {
    kind: p.kind,
    baseUrl: p.baseUrl,
    timeoutSecs: p.timeoutSecs,
    binary: p.binary,
  };
}

/** A model id turned into a usable alias: `deepseek/deepseek-chat` -> `deepseek-chat`. */
export function aliasFromModelId(id: string): string {
  const tail = id.split("/").pop() ?? id;
  return tail.replace(/[^A-Za-z0-9._-]/g, "-").slice(0, 40);
}

/** Model alias for a fetched id that does not collide with existing aliases. */
export function uniqueAlias(id: string, provider: string, taken: Set<string>): string {
  const base = aliasFromModelId(id);
  if (!taken.has(base)) return base;
  const withProvider = `${base}-${provider}`.slice(0, 40);
  if (!taken.has(withProvider)) return withProvider;
  for (let i = 2; ; i++) {
    const candidate = `${withProvider.slice(0, 36)}-${i}`;
    if (!taken.has(candidate)) return candidate;
  }
}
