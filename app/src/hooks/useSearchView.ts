// State of the Search view: the query and filters, and the latest results. Searches run
// shortly after the user stops typing; a response to an older query is dropped.

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { readItem, storeItem } from "./storage";
import type { SearchLayer, SearchParams, SearchResults } from "../types";

const DEBOUNCE_MS = 250;
const LAYER_KEY = "aj-search-layer";

function initialLayer(): SearchLayer {
  const saved = readItem(LAYER_KEY);
  return saved === "reports" || saved === "sessions" ? saved : "all";
}

export function useSearchView() {
  const [params, setParams] = useState<SearchParams>(() => ({
    query: "",
    layer: initialLayer(),
    since: null,
    until: null,
    projectId: null,
    agent: null,
    includeIgnored: false,
    includeToolOutput: false,
  }));
  const [results, setResults] = useState<SearchResults | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Bumped to move keyboard focus to the search box (Cmd+F while already here). */
  const [focusSeq, setFocusSeq] = useState(0);
  const reqRef = useRef(0);

  const update = useCallback((patch: Partial<SearchParams>) => {
    if (patch.layer) storeItem(LAYER_KEY, patch.layer);
    setParams((p) => ({ ...p, ...patch }));
  }, []);

  const run = useCallback(async (p: SearchParams) => {
    const req = ++reqRef.current;
    if (!p.query.trim()) {
      setResults(null);
      setLoading(false);
      setError(null);
      return;
    }
    setLoading(true);
    try {
      const res = await api.searchAll(p);
      if (req === reqRef.current) {
        setResults(res);
        setError(null);
      }
    } catch (e) {
      if (req === reqRef.current) setError(String(e));
    } finally {
      if (req === reqRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void run(params), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [params, run]);

  /** Search again with the same query, e.g. after a scan or an edit elsewhere. */
  const refresh = useCallback(() => run(params), [params, run]);
  const focus = useCallback(() => setFocusSeq((n) => n + 1), []);

  return { params, update, results, loading, error, refresh, focusSeq, focus };
}

export type SearchView = ReturnType<typeof useSearchView>;
