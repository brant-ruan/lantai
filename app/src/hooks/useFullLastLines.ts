import { type RefObject, useLayoutEffect } from "react";

/**
 * Keeps the last line of each paragraph in a container at least half the paragraph's width,
 * by nudging its spacing a little: letter spacing for CJK text, word spacing for spaced text.
 * The smallest nudge that works wins; a paragraph no nudge within the limits fixes keeps
 * its normal spacing (the copy has to change then).
 */

/** The share of the paragraph width the last line must reach. */
const MIN_LAST_LINE = 0.5;
/** Nudges tried, in em, smallest first; each is tried tighter and looser. */
const STEPS = 10;
const LETTER_STEP = 0.004;
const WORD_STEP = 0.02;

/** The width of the last rendered line as a share of the paragraph's content width. */
export function lastLineShare(p: HTMLElement): number {
  const range = document.createRange();
  range.selectNodeContents(p);
  let lastTop = -Infinity;
  let left = 0;
  let right = 0;
  for (const r of range.getClientRects()) {
    if (r.width < 1) continue;
    if (r.top > lastTop + r.height / 2) {
      lastTop = r.top;
      left = r.left;
      right = r.right;
    } else if (Math.abs(r.top - lastTop) < r.height / 2) {
      left = Math.min(left, r.left);
      right = Math.max(right, r.right);
    }
  }
  const style = getComputedStyle(p);
  const width = p.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  return width > 0 ? (right - left) / width : 1;
}

/** Whether a paragraph is mostly CJK, where letter spacing (not word spacing) moves text. */
function isCjk(text: string): boolean {
  const cjk = text.match(/[\u3000-\u303f\u3400-\u9fff\uff00-\uffef]/g)?.length ?? 0;
  return cjk * 2 > text.replace(/\s/g, "").length;
}

/** Gives one paragraph the smallest spacing nudge that fills its last line, or none. */
export function fitLastLine(p: HTMLElement) {
  p.style.letterSpacing = "";
  p.style.wordSpacing = "";
  if (lastLineShare(p) >= MIN_LAST_LINE) return;
  const cjk = isCjk(p.textContent ?? "");
  const step = cjk ? LETTER_STEP : WORD_STEP;
  const set = (em: number) => {
    if (cjk) p.style.letterSpacing = em ? `${em}em` : "";
    else p.style.wordSpacing = em ? `${em}em` : "";
  };
  for (let i = 1; i <= STEPS; i++) {
    for (const em of [-i * step, i * step]) {
      set(em);
      if (lastLineShare(p) >= MIN_LAST_LINE) return;
    }
  }
  set(0);
}

/** Fits every `p` in the container now, once web fonts are ready, and when its width changes. */
export function useFullLastLines(container: RefObject<HTMLElement | null>, contentKey: string) {
  useLayoutEffect(() => {
    const root = container.current;
    if (!root) return;
    const fitAll = () => root.querySelectorAll("p").forEach(fitLastLine);
    fitAll();
    let alive = true;
    document.fonts.ready.then(() => alive && fitAll());
    let width = root.clientWidth;
    const observer = new ResizeObserver(() => {
      if (root.clientWidth === width) return;
      width = root.clientWidth;
      fitAll();
    });
    observer.observe(root);
    return () => {
      alive = false;
      observer.disconnect();
    };
  }, [container, contentKey]);
}
