import type { EngineTestResult } from "../../../types";
import { useT } from "../../../i18n";

/** The result of a model's connection test: success or failure, and how long it took. */
export function TestBadge({ result }: { result: EngineTestResult }) {
  const tm = useT().models;
  return (
    <span className={"test-result " + (result.ok ? "ok" : "err")}>
      <svg
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        {result.ok ? (
          <polyline points="4 12.5 9.5 18 20 6" />
        ) : (
          <path d="M6 6l12 12M18 6L6 18" />
        )}
      </svg>
      {result.ok ? tm.success : tm.failed}
      <span className="test-result-time">
        {(result.elapsedMs / 1000).toFixed(1)}s
      </span>
    </span>
  );
}
