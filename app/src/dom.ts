// Small DOM helpers shared by keyboard handlers.

/** Whether a key event comes from a field the user is typing in. */
export function isTextInput(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return (
    !!el &&
    (el.tagName === "INPUT" ||
      el.tagName === "TEXTAREA" ||
      el.tagName === "SELECT" ||
      el.isContentEditable)
  );
}

/** Whether a modal dialog is open (it owns Escape then). */
export function modalOpen(): boolean {
  return document.querySelector(".modal-backdrop") !== null;
}
