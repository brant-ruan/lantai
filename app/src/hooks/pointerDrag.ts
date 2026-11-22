// Page state while the pointer drags across reports (a box selection or a note drag): no text
// selection, and report content taken out of hit testing. Rows the pointer crosses with the
// button held would otherwise keep their hover look after the drag (WebKit leaves it behind
// until the pointer passes over them again). Code that needs to know what lies under the
// pointer meanwhile must compare rectangles, not use `elementFromPoint`.

export function setPointerDrag(on: boolean, cursor = "") {
  document.body.classList.toggle("is-pointer-dragging", on);
  document.body.style.userSelect = on ? "none" : "";
  document.body.style.cursor = on ? cursor : "";
}
