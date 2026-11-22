// Calls `onRollover` when the local clock enters a new hour while the app stays open (it lives
// in the Dock for days), so "today", the current week and month follow the clock. The logical
// day starts at the backend's `day_boundary_hour`, which the frontend does not know, so any
// hour change triggers a refresh and the backend decides what "today" is.
import { useEffect, useRef } from "react";

function localHourKey(): string {
  const d = new Date();
  return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}T${d.getHours()}`;
}

/** How often to compare; timers are paused while the Mac sleeps, so no exact boundary timer. */
const CHECK_INTERVAL_MS = 60_000;

export function useDateRollover(onRollover: () => void) {
  const callbackRef = useRef(onRollover);
  callbackRef.current = onRollover;

  useEffect(() => {
    let last = localHourKey();
    const check = () => {
      const now = localHourKey();
      if (now !== last) {
        last = now;
        callbackRef.current();
      }
    };
    const timer = window.setInterval(check, CHECK_INTERVAL_MS);
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", check);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", check);
    };
  }, []);
}
