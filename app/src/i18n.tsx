// Interface language: which language the interface is shown in, the texts for it, and
// locale-aware date helpers. Texts live in `locales/`; `en.ts` is the source of the keys
// and every other language falls back to it for keys it leaves out.

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { en, type DeepPartial, type Messages } from "./locales/en";
import { zhHans } from "./locales/zh-Hans";
import type { ReportLanguage } from "./types";

interface UiLocale {
  tag: string;
  messages: DeepPartial<Messages>;
  /** Languages to borrow missing texts from, before English. */
  fallback: string[];
}

/** Interface languages, in menu order. */
const UI_LOCALES: UiLocale[] = [
  { tag: "en", messages: en, fallback: [] },
  { tag: "zh-Hans", messages: zhHans, fallback: [] },
];

export const UI_LANGUAGES = UI_LOCALES.map((l) => ({
  tag: l.tag,
  nativeName: l.messages.meta?.nativeName ?? l.tag,
}));

/** The system's preferred languages, most preferred first. */
export function systemLanguages(): string[] {
  return [...(navigator.languages ?? [navigator.language])];
}

/**
 * The supported interface language a system locale names, if any. Only
 * Simplified Chinese counts as Chinese: Traditional Chinese (`zh-Hant`, or
 * `zh-TW` / `zh-HK` / `zh-MO` without a script) matches nothing. Same rule as
 * `aj_report::lang::match_locale`, which picks the initial report language.
 */
export function matchLocale(locale: string): string | null {
  const parts = locale.split(/[.@]/)[0].toLowerCase().replace(/_/g, "-").split("-");
  const [primary, ...rest] = parts;
  if (primary === "en") return "en";
  if (primary === "zh") {
    const traditional =
      rest.includes("hant") ||
      (!rest.includes("hans") && rest.some((p) => p === "tw" || p === "hk" || p === "mo"));
    return traditional ? null : "zh-Hans";
  }
  return null;
}

/**
 * The interface language for a setting: the chosen one, or for "auto" the
 * first supported language among the system's preferred ones, else English.
 */
export function resolveUiLanguage(setting: string, system: readonly string[]): string {
  if (setting !== "auto" && UI_LOCALES.some((l) => l.tag === setting)) return setting;
  for (const raw of system) {
    const tag = matchLocale(raw);
    if (tag) return tag;
  }
  return "en";
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function overlay<T>(base: T, over: DeepPartial<T> | undefined): T {
  if (!over) return base;
  const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [k, v] of Object.entries(over)) {
    if (v === undefined) continue;
    const b = out[k];
    out[k] = isPlainObject(b) && isPlainObject(v) ? overlay(b, v) : v;
  }
  return out as T;
}

/** Complete texts for `tag`: its own, then its fallbacks', then English. */
export function messagesFor(tag: string): Messages {
  const chain: UiLocale[] = [];
  const add = (t: string) => {
    const l = UI_LOCALES.find((x) => x.tag === t);
    if (!l || chain.includes(l)) return;
    chain.push(l);
    l.fallback.forEach(add);
  };
  add(tag);
  return chain.reverse().reduce<Messages>((acc, l) => overlay(acc, l.messages), en);
}

interface LanguageState {
  /** The interface language in use (a resolved tag, never "auto"). */
  lang: string;
  t: Messages;
  /** The interface language setting: "auto" or a tag. */
  uiSetting: string;
  setUiSetting: (value: string) => Promise<void>;
  reportLanguage: string;
  reportLanguages: ReportLanguage[];
  setReportLanguage: (tag: string) => Promise<void>;
}

const LanguageContext = createContext<LanguageState>({
  lang: "en",
  t: en,
  uiSetting: "auto",
  setUiSetting: async () => {},
  reportLanguage: "en",
  reportLanguages: [],
  setReportLanguage: async () => {},
});

/** Loads the language settings, provides the texts, and keeps the window title and native menu in step. */
export function LanguageProvider({ children }: { children: ReactNode }) {
  const [uiSetting, setUiSettingState] = useState("auto");
  const [reportLanguage, setReportLanguageState] = useState("en");
  const [reportLanguages, setReportLanguages] = useState<ReportLanguage[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    api
      .getLanguageSettings(systemLanguages())
      .then((s) => {
        setUiSettingState(s.uiLanguage);
        setReportLanguageState(s.reportLanguage);
        setReportLanguages(s.reportLanguages);
      })
      .catch((err) => console.error("failed to load language settings", err))
      .finally(() => setLoaded(true));
  }, []);

  const lang = resolveUiLanguage(uiSetting, systemLanguages());
  const t = useMemo(() => messagesFor(lang), [lang]);

  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = t.meta.appName;
    void getCurrentWindow()
      .setTitle(t.meta.appName)
      .catch(() => {});
    const menu = Object.fromEntries(
      Object.entries(t.menu).map(([k, v]) => [k, v.replace("{name}", t.meta.appName)]),
    ) as Messages["menu"];
    void api.setMenuLabels(menu).catch(() => {});
  }, [lang, t]);

  const setUiSetting = useCallback(async (value: string) => {
    await api.setUiLanguage(value);
    setUiSettingState(value);
  }, []);
  const setReportLanguage = useCallback(async (tag: string) => {
    await api.setReportLanguage(tag);
    setReportLanguageState(tag);
  }, []);

  const value = useMemo(
    () => ({
      lang,
      t,
      uiSetting,
      setUiSetting,
      reportLanguage,
      reportLanguages,
      setReportLanguage,
    }),
    [lang, t, uiSetting, setUiSetting, reportLanguage, reportLanguages, setReportLanguage],
  );
  // Render only once the settings are known, so the first screen is in the right language.
  if (!loaded) return null;
  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

/** The texts of the interface language. */
export function useT(): Messages {
  return useContext(LanguageContext).t;
}

export function useLanguage(): LanguageState {
  return useContext(LanguageContext);
}

function dateOf(ymd: string): Date {
  const [y, m, d] = ymd.split("-").map(Number);
  return new Date(y, (m || 1) - 1, d || 1);
}

/** Short weekday of a "YYYY-MM-DD" date ("Mon", "周一"). */
export function weekdayName(lang: string, ymd: string): string {
  return new Intl.DateTimeFormat(lang, { weekday: "short" }).format(dateOf(ymd));
}

/** Calendar column headers, Monday first ("M T W …", "一 二 三 …"). */
export function weekdayInitials(lang: string): string[] {
  const fmt = new Intl.DateTimeFormat(lang, { weekday: "narrow" });
  // 2024-01-01 was a Monday.
  return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2024, 0, 1 + i)));
}

/** Short month name of month index 0..11 ("Nov", "11月"). */
export function monthName(lang: string, month: number): string {
  return new Intl.DateTimeFormat(lang, { month: "short" }).format(new Date(2024, month, 1));
}

/** Local date and time of an ISO timestamp, in the interface language. */
export function formatDateTime(lang: string, iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeStyle: "short" }).format(d);
}
