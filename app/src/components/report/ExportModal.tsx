import { useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { useT } from "../../i18n";
import { readItem, storeItem } from "../../storage";
import type { ExportResult } from "../../types";
import { errorText } from "../../errors";

const LAST_EXPORT_DIR_KEY = "aj-last-export-dir";

interface ExportModalProps {
  open: boolean;
  title: string;
  defaultExportPath: string;
  markdown: string;
  onClose: () => void;
  onExport: (params: {
    markdown: string;
    customPath: string | null;
    anchorMode: "keep" | "footnotes" | "strip";
    force: boolean;
  }) => Promise<ExportResult>;
  onImportConflictAsVersion?: (existingMarkdown: string) => Promise<void>;
}

function splitExportPath(fullPath: string): { dir: string; file: string } {
  const normalized = fullPath.replace(/\\/g, "/");
  const idx = normalized.lastIndexOf("/");
  if (idx < 0) {
    return { dir: "", file: fullPath || "report.md" };
  }
  return {
    dir: fullPath.slice(0, idx) || "/",
    file: fullPath.slice(idx + 1) || "report.md",
  };
}

function joinExportPath(dir: string, file: string): string {
  const cleanFile = file.trim() || "report.md";
  const cleanDir = dir.trim();
  if (!cleanDir) return cleanFile;
  if (cleanDir === "/" || cleanDir.endsWith("/")) {
    return `${cleanDir}${cleanFile}`;
  }
  return `${cleanDir}/${cleanFile}`;
}

export function ExportModal({
  open,
  title,
  defaultExportPath,
  markdown,
  onClose,
  onExport,
  onImportConflictAsVersion,
}: ExportModalProps) {
  const tr = useT();
  const t = tr.exportModal;
  const defaults = useMemo(
    () => splitExportPath(defaultExportPath),
    [defaultExportPath],
  );
  const [dirPath, setDirPath] = useState(
    () => readItem(LAST_EXPORT_DIR_KEY) || defaults.dir,
  );
  const [fileName, setFileName] = useState(defaults.file);
  const [anchorMode, setAnchorMode] = useState<"keep" | "footnotes" | "strip">(
    "keep",
  );
  const [picking, setPicking] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [conflict, setConflict] = useState<ExportResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setDirPath(readItem(LAST_EXPORT_DIR_KEY) || defaults.dir);
    setFileName(defaults.file);
    setConflict(null);
    setNotice(null);
    setErrorMsg(null);
  }, [open, defaults.dir, defaults.file]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const resolvedPath = joinExportPath(dirPath, fileName);

  const handleChooseFolder = async () => {
    setPicking(true);
    setErrorMsg(null);
    try {
      const picked = await api.pickExportDirectory(dirPath || defaults.dir);
      if (picked) {
        setDirPath(picked);
        setConflict(null);
        setNotice(null);
        storeItem(LAST_EXPORT_DIR_KEY, picked);
      }
    } catch (err) {
      setErrorMsg(errorText(err));
    } finally {
      setPicking(false);
    }
  };

  const handleTriggerExport = async (force: boolean) => {
    setExporting(true);
    setErrorMsg(null);
    try {
      const res = await onExport({
        markdown,
        customPath: resolvedPath || null,
        anchorMode,
        force,
      });
      if (res.status === "conflict") {
        setConflict(res);
      } else {
        setConflict(null);
        setNotice(t.exported(res.bytes ?? 0, res.path));
      }
    } catch (err) {
      setErrorMsg(errorText(err));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="export-modal" onClick={(e) => e.stopPropagation()}>
        <header className="settings-main-header">
          <div>
            <h2>{title}</h2>
          </div>
          <button
            type="button"
            className="btn btn-ghost btn-icon-sq"
            onClick={onClose}
            title={tr.common.close}
            aria-label={t.closeLabel}
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </header>

        <div className="export-modal-body">
          <div className="export-field">
            <div className="export-field-label">{t.folder}</div>
            <div className="export-folder-row">
              <div className="export-folder-display" title={dirPath}>
                <code>{dirPath || "(Default export folder)"}</code>
              </div>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                disabled={picking}
                onClick={() => void handleChooseFolder()}
              >
                {picking ? t.opening : t.chooseFolder}
              </button>
            </div>
          </div>

          <div className="export-field">
            <div className="export-field-label">{t.fileName}</div>
            <input
              type="text"
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
            />
            <div className="muted-line">
              Target: <code>{resolvedPath}</code>
            </div>
          </div>

          <div className="export-field">
            <div className="export-field-label">{t.anchors}</div>
            <div className="pill-group">
              <button
                type="button"
                className={`pill ${anchorMode === "keep" ? "active" : ""}`}
                onClick={() => setAnchorMode("keep")}
              >
                {t.keep}
              </button>
              <button
                type="button"
                className={`pill ${anchorMode === "footnotes" ? "active" : ""}`}
                onClick={() => setAnchorMode("footnotes")}
              >
                {t.footnotes}
              </button>
              <button
                type="button"
                className={`pill ${anchorMode === "strip" ? "active" : ""}`}
                onClick={() => setAnchorMode("strip")}
              >
                {t.hide}
              </button>
            </div>
          </div>

          {errorMsg && <div className="banner banner-err">{errorMsg}</div>}

          {conflict && (
            <div className="banner banner-warn">
              <div>
                <strong>{t.conflict}</strong> <code>{conflict.path}</code>{" "}
                {t.modifiedOutside}
                <details>
                  <summary>{t.previewExternal}</summary>
                  <pre className="compare-text">
                    {conflict.existingMarkdown ?? ""}
                  </pre>
                </details>
              </div>
              <div className="banner-actions">
                <button
                  type="button"
                  className="btn btn-danger btn-xs"
                  onClick={() => void handleTriggerExport(true)}
                >
                  {t.overwrite}
                </button>
                {onImportConflictAsVersion && (
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    onClick={async () => {
                      if (conflict.existingMarkdown !== null) {
                        await onImportConflictAsVersion(
                          conflict.existingMarkdown,
                        );
                        setConflict(null);
                        setNotice(t.imported);
                      }
                    }}
                  >
                    {t.importAsVersion}
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-ghost btn-xs"
                  onClick={() => setConflict(null)}
                >
                  {tr.common.cancel}
                </button>
              </div>
            </div>
          )}

          {notice && <div className="banner banner-info">{notice}</div>}
        </div>

        <footer className="export-modal-foot">
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={onClose}
          >
            {notice ? t.done : tr.common.cancel}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={exporting || !fileName.trim()}
            onClick={() => void handleTriggerExport(false)}
          >
            {exporting ? t.exporting : t.exportMarkdown}
          </button>
        </footer>
      </div>
    </div>
  );
}
