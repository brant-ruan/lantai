// Optional background pre-generation of slice digests while the daily view is idle, plus
// the agent source settings that decide which agents take part.

import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n";
import type { AgentSource, DayWorkbench } from "../types";

export interface PreDigestInputs {
  /** Whether pre-digesting may run now (daily view shown, no modal, nothing else busy). */
  allowed: boolean;
  workbench: DayWorkbench | null;
  engine: string;
  refreshDailyView: (date: string) => Promise<void>;
  runAction: (fn: () => Promise<void>) => Promise<void>;
}

export function usePreDigest({ allowed, workbench, engine, refreshDailyView, runAction }: PreDigestInputs) {
  const t = useT().preDigest;
  const [agentSources, setAgentSources] = useState<AgentSource[]>([]);
  // Set when a background digest fails; cleared when agent settings change.
  const [paused, setPaused] = useState(false);
  const [idleStatus, setIdleStatus] = useState<string | null>(null);
  // Also set while the user digests a slice from the drawer, so the two never overlap.
  const [digesting, setDigesting] = useState(false);
  const agents = useMemo(
    () => new Set(agentSources.filter((a) => a.preDigest).map((a) => a.agent)),
    [agentSources],
  );

  useEffect(() => {
    void runAction(async () => setAgentSources(await api.agentSources()));
  }, [runAction]);

  // Ticks every minute so live slices are picked up once they go quiet.
  const [clock, setClock] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setClock(Date.now()), 60_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    if (agents.size === 0 || paused || !allowed || digesting || !workbench) return;
    // Slices whose session may still be growing are left for later: a digest made now
    // would be outdated by the next turn.
    const quietMs = workbench.quietMinutes * 60_000;
    const candidates = workbench.slices.filter(
      (s) => !s.ignored && !s.hasDigest && agents.has(s.agent),
    );
    const pending = candidates.find((s) => clock - Date.parse(s.endedAt) >= quietMs);
    if (!pending) {
      setIdleStatus(
        candidates.length > 0
          ? t.waiting(candidates.length)
          : t.allCached,
      );
      return;
    }
    const timer = window.setTimeout(() => {
      void (async () => {
        setDigesting(true);
        setIdleStatus(t.running(pending.id));
        try {
          await api.digestSliceNow(pending.id, engine || null);
          await refreshDailyView(workbench.date);
          setIdleStatus(t.cached(pending.id));
        } catch (err) {
          setIdleStatus(t.paused(err instanceof Error ? err.message : String(err)));
          setPaused(true);
        } finally {
          setDigesting(false);
        }
      })();
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [agents, paused, allowed, digesting, workbench, engine, refreshDailyView, clock, t]);

  /** Apply new agent settings; a paused pre-digest gets another chance. */
  const updateAgentSources = (next: AgentSource[]) => {
    setAgentSources(next);
    setPaused(false);
    if (!next.some((a) => a.preDigest)) setIdleStatus(null);
  };

  return { agentSources, updateAgentSources, idleStatus, digesting, setDigesting };
}
