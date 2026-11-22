// Turns horizontal trackpad swipes and touch swipes into single steps (-1 / +1).
//
// One step per trackpad gesture: after a step, wheel input stays locked while inertial
// scrolling decays; the lock lifts when events go quiet, or when the horizontal speed rises
// again after having decayed (a new swipe starting during the inertial tail).

import { useEffect, useRef } from "react";

export function useSwipeStep(onStep: (delta: 1 | -1) => void) {
  const wheel = useRef({
    accumX: 0,
    locked: false,
    peakAbs: 0,
    lastAbs: 0,
    decayed: false,
    idleTimer: null as number | null,
  });
  const touchStartX = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (wheel.current.idleTimer !== null) window.clearTimeout(wheel.current.idleTimer);
    },
    [],
  );

  const onWheel = (e: React.WheelEvent<HTMLElement>) => {
    const state = wheel.current;
    if (state.idleTimer !== null) window.clearTimeout(state.idleTimer);
    state.idleTimer = window.setTimeout(() => {
      state.accumX = 0;
      state.locked = false;
      state.idleTimer = null;
    }, 150);

    const abs = Math.abs(e.deltaX);
    if (state.locked) {
      state.peakAbs = Math.max(state.peakAbs, abs);
      if (abs < state.peakAbs * 0.5) state.decayed = true;
      const rising = abs >= 4 && abs > state.lastAbs * 1.6;
      state.lastAbs = abs;
      if (!(state.decayed && rising)) return;
      state.locked = false;
      state.accumX = 0;
    }
    if (abs < Math.abs(e.deltaY) * 0.7) return;

    state.accumX += e.deltaX;
    if (Math.abs(state.accumX) >= 25) {
      const step = state.accumX > 0 ? 1 : -1;
      state.accumX = 0;
      state.locked = true;
      state.peakAbs = abs;
      state.lastAbs = abs;
      state.decayed = false;
      onStep(step);
    }
  };

  const onTouchStart = (e: React.TouchEvent<HTMLElement>) => {
    touchStartX.current = e.touches[0]?.clientX ?? null;
  };

  const onTouchEnd = (e: React.TouchEvent<HTMLElement>) => {
    const startX = touchStartX.current;
    touchStartX.current = null;
    const endX = e.changedTouches[0]?.clientX;
    if (startX === null || endX === undefined) return;
    const deltaX = endX - startX;
    if (deltaX <= -36) onStep(1);
    else if (deltaX >= 36) onStep(-1);
  };

  return { onWheel, onTouchStart, onTouchEnd };
}
