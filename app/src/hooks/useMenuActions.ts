// Carries out actions from the native menu bar (View > Daily … Statistics with Cmd+1…5,
// Lantai > Settings… with Cmd+, About Lantai and Check for Updates…). The menu and its shortcuts are defined in
// the Tauri backend (`menu.rs`), which forwards clicks as `menu-action` events.
import { useEffect, useRef } from "react";
import { api } from "../api";
import type { MenuAction, SettingsTab, ViewMode } from "../types";

interface MenuActionHandlers {
  setViewMode: (mode: ViewMode) => void;
  /** Open the settings modal, on the given category if one is passed. */
  openSettings: (tab?: SettingsTab) => void;
  closeSettings: () => void;
  /** Check for a new release now (a manual check reports its result either way). */
  checkUpdates: () => void;
}

export function useMenuActions(handlers: MenuActionHandlers) {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    const handle = (action: MenuAction) => {
      const h = handlersRef.current;
      switch (action.kind) {
        case "view":
          // The modal would hide the switch, so leave it first.
          h.closeSettings();
          h.setViewMode(action.mode);
          break;
        case "settings":
          h.openSettings();
          break;
        case "about":
          h.openSettings("about");
          break;
        case "checkUpdates":
          h.checkUpdates();
          break;
      }
    };
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void api.onMenuAction(handle).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}
