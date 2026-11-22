// The slice detail drawer: which slice is open, its loaded detail, and the highlighted turn.

import { useCallback, useRef, useState } from "react";
import { api } from "../api";
import type { SliceDetail } from "../types";

export function useSliceDrawer(runAction: (fn: () => Promise<void>) => Promise<void>) {
  const [sliceId, setSliceId] = useState<number | null>(null);
  const [detail, setDetail] = useState<SliceDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [highlightTurn, setHighlightTurn] = useState<number | null>(null);
  // Monotonic request id: a slower response for a previous slice must not overwrite the current one.
  const reqRef = useRef(0);
  const sliceIdRef = useRef(sliceId);
  sliceIdRef.current = sliceId;

  const open = useCallback(
    async (id: number, turnIndex?: number) => {
      const req = ++reqRef.current;
      setSliceId(id);
      setHighlightTurn(turnIndex ?? null);
      setDetail((prev) => (prev?.slice.id === id ? prev : null));
      setLoading(true);
      await runAction(async () => {
        const d = await api.sliceDetail(id);
        if (req === reqRef.current) setDetail(d);
      });
      if (req === reqRef.current) setLoading(false);
    },
    [runAction],
  );

  const refreshIfOpen = useCallback(async () => {
    const id = sliceIdRef.current;
    if (id === null) return;
    const req = reqRef.current;
    const d = await api.sliceDetail(id);
    if (req === reqRef.current) setDetail(d);
  }, []);

  const close = useCallback(() => {
    reqRef.current += 1;
    setLoading(false);
    setSliceId(null);
    setDetail(null);
    setHighlightTurn(null);
  }, []);

  return { sliceId, detail, loading, highlightTurn, setHighlightTurn, open, refreshIfOpen, close };
}
