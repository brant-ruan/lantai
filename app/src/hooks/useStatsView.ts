// State of the statistics dashboard: range, ignore filter, and the latest loaded reports.
// Lantai's own state loads apart, since it collects every day's material and is slower.

import { useCallback, useRef, useState } from "react";
import { api } from "../api";
import { errorText } from "../errors";
import type { LantaiStats, StatsRangeKey, StatsReport } from "../types";

export function useStatsView() {
  const [range, setRange] = useState<StatsRangeKey>("30d");
  const [includeIgnored, setIncludeIgnored] = useState(false);
  const [data, setData] = useState<StatsReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [lantai, setLantai] = useState<LantaiStats | null>(null);
  const [lantaiLoading, setLantaiLoading] = useState(false);
  const [lantaiError, setLantaiError] = useState<string | null>(null);
  const reqRef = useRef(0);
  const lantaiReqRef = useRef(0);

  const refreshLantai = useCallback(async (r: StatsRangeKey) => {
    const req = ++lantaiReqRef.current;
    setLantaiLoading(true);
    try {
      const res = await api.getLantaiStats(r);
      if (req === lantaiReqRef.current) {
        setLantai(res);
        setLantaiError(null);
      }
    } catch (e) {
      if (req === lantaiReqRef.current) setLantaiError(errorText(e));
    } finally {
      if (req === lantaiReqRef.current) setLantaiLoading(false);
    }
  }, []);

  const refresh = useCallback(
    async (r: StatsRangeKey, inc: boolean) => {
      const req = ++reqRef.current;
      setLoading(true);
      // Not awaited: the rest of the dashboard should not wait for it.
      void refreshLantai(r);
      try {
        const res = await api.getStats(r, inc);
        if (req === reqRef.current) setData(res);
      } finally {
        if (req === reqRef.current) setLoading(false);
      }
    },
    [refreshLantai],
  );

  return {
    range,
    setRange,
    includeIgnored,
    setIncludeIgnored,
    data,
    loading,
    lantai,
    lantaiLoading,
    lantaiError,
    refresh,
  };
}
