// Update checks (docs/SPEC.md, "Distribution"): the automatic check (off by default; on start
// and every 24 hours when on), manual checks from the menu or the settings, and the banner
// that reports the result. Nothing is downloaded; the banner links to the release page.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { UpdateCheck, UpdateSettings } from "../types";
import { readItem, storeItem } from "./storage";

const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
const SKIPPED_VERSION_KEY = "update-skipped-version";

export type UpdateBanner =
  | { kind: "available"; check: Extract<UpdateCheck, { status: "available" }> }
  | { kind: "upToDate"; version: string }
  | { kind: "error"; message: string };

export interface UpdateCheckApi {
  settings: UpdateSettings | null;
  setAutoCheck: (on: boolean) => Promise<void>;
  checking: boolean;
  /** Check now. A manual check also reports "up to date" and errors; an automatic one
   * stays quiet unless a version the user has not skipped is available. */
  checkNow: (manual: boolean) => Promise<void>;
  banner: UpdateBanner | null;
  dismissBanner: () => void;
  /** Hide the available version until a newer one comes out. */
  skipVersion: () => void;
}

export function useUpdateCheck(): UpdateCheckApi {
  const [settings, setSettings] = useState<UpdateSettings | null>(null);
  const [checking, setChecking] = useState(false);
  const [banner, setBanner] = useState<UpdateBanner | null>(null);
  const checkingRef = useRef(false);

  useEffect(() => {
    void api
      .updateSettings()
      .then(setSettings)
      .catch(() => {});
  }, []);

  const checkNow = useCallback(async (manual: boolean) => {
    if (checkingRef.current) return;
    checkingRef.current = true;
    setChecking(true);
    try {
      const result = await api.checkForUpdates();
      if (result.status === "available") {
        if (manual || readItem(SKIPPED_VERSION_KEY) !== result.latest) {
          setBanner({ kind: "available", check: result });
        }
      } else if (manual) {
        setBanner({ kind: "upToDate", version: result.current });
      }
    } catch (err) {
      if (manual) setBanner({ kind: "error", message: String(err) });
    } finally {
      checkingRef.current = false;
      setChecking(false);
    }
  }, []);

  const autoCheck = settings?.checkUpdates ?? false;
  useEffect(() => {
    if (!autoCheck) return;
    void checkNow(false);
    const timer = window.setInterval(() => void checkNow(false), CHECK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [autoCheck, checkNow]);

  const setAutoCheck = useCallback(async (on: boolean) => {
    await api.setCheckUpdates(on);
    setSettings((s) => (s ? { ...s, checkUpdates: on } : s));
  }, []);

  const dismissBanner = useCallback(() => setBanner(null), []);

  const skipVersion = useCallback(() => {
    setBanner((b) => {
      if (b?.kind === "available") storeItem(SKIPPED_VERSION_KEY, b.check.latest);
      return null;
    });
  }, []);

  return { settings, setAutoCheck, checking, checkNow, banner, dismissBanner, skipVersion };
}
