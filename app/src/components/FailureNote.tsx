import { useEffect, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n";
import type { Failure } from "../types";

/**
 * Engine errors in plain words: a known case (`aj_report::diagnose`) shows what happened and the
 * next step, with the full raw text under Details; anything else shows the raw text.
 */

export function FailureNote({
  failure,
  message,
}: {
  failure: Failure | null;
  message: string;
}) {
  const tf = useT().failures;
  if (!failure) return <span className="failure-raw">{message}</span>;
  const f = tf[failure];
  return (
    <div className="failure-note">
      <strong>{f.title}</strong>
      <span>{f.next}</span>
      <details>
        <summary>{tf.details}</summary>
        <pre>{message}</pre>
      </details>
    </div>
  );
}

/** The known case of an error message, asked of the back end; null until known or for others. */
function useFailure(message: string): Failure | null {
  const [failure, setFailure] = useState<{ message: string; kind: Failure | null } | null>(
    null,
  );
  useEffect(() => {
    let live = true;
    api
      .diagnoseError(message)
      .then((kind) => live && setFailure({ message, kind }))
      .catch(() => live && setFailure({ message, kind: null }));
    return () => {
      live = false;
    };
  }, [message]);
  return failure?.message === message ? failure.kind : null;
}

/** The workspace's error banner. Model problems offer a way to the Models settings. */
export function ErrorBanner({
  message,
  onDismiss,
  onOpenModels,
}: {
  message: string;
  onDismiss: () => void;
  onOpenModels: () => void;
}) {
  const tr = useT();
  const failure = useFailure(message);
  const toModels = failure !== null && failure !== "quota";
  return (
    <div className="banner banner-err">
      <FailureNote failure={failure} message={message} />
      <div className="banner-actions">
        {toModels && (
          <button type="button" className="btn btn-secondary btn-xs" onClick={onOpenModels}>
            {tr.failures.openModels}
          </button>
        )}
        <button type="button" className="btn btn-ghost btn-xs" onClick={onDismiss}>
          {tr.workspace.dismiss}
        </button>
      </div>
    </div>
  );
}
