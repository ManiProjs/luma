import { useEffect, useMemo } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";

type CommandOutputDialogProps = {
  title: string;
  content: string;
  onClose: () => void;
  onRefresh?: () => void | Promise<void>;
  onOpenFile?: (path: string) => void;
};

export default function CommandOutputDialog({ title, content, onClose, onRefresh, onOpenFile }: CommandOutputDialogProps) {
  const panelRef = useFocusTrap(true);
  const fileResults = useMemo(() => {
    if (title !== "/find") return [] as Array<{ path: string; score: number }>;
    try {
      const parsed: unknown = JSON.parse(content);
      return Array.isArray(parsed) ? parsed.filter((item): item is { path: string; score: number } =>
        typeof item === "object" && item !== null && typeof item.path === "string" && typeof item.score === "number") : [];
    } catch {
      return [];
    }
  }, [content, title]);
  useEffect(() => {
    if (!onRefresh) return;
    const interval = window.setInterval(() => { void onRefresh(); }, 5_000);
    return () => window.clearInterval(interval);
  }, [onRefresh]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onClose(); }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  return <div className="luma-command-output-backdrop" role="presentation" onMouseDown={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}>
    <section ref={panelRef} className="luma-command-output-panel" role="dialog" aria-modal="true" aria-label={`${title} output`}>
      <header>
        <div><span>CORE COMMAND</span><h2>{title}</h2></div>
        <button type="button" onClick={onClose} aria-label="Close command output">×</button>
      </header>
      {title === "/find" && fileResults.length > 0 ? <div className="luma-command-file-results" role="list" aria-label="Matching workspace files">
        {fileResults.map((file) => <div key={file.path} role="listitem">
          <button type="button" onClick={() => onOpenFile?.(file.path)} disabled={!onOpenFile}>
            <span>{file.path}</span><small>{file.score}</small>
          </button>
        </div>)}
      </div> : <pre>{content || "No output."}</pre>}
      <footer><span>{onRefresh ? "Live Core data · refreshes every 5 seconds" : "Read-only result from Luma Core"}</span><button type="button" onClick={onClose}>Close</button></footer>
    </section>
  </div>;
}
