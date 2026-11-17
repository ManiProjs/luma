import { useEffect, useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";

export default function CommandQueryDialog({ title, placeholder, onClose, onRun }: {
  title: string;
  placeholder: string;
  onClose: () => void;
  onRun: (query: string) => void;
}) {
  const [query, setQuery] = useState("");
  const panelRef = useFocusTrap<HTMLFormElement>(true);
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
    <form ref={panelRef} className="luma-command-query-panel" role="dialog" aria-modal="true" aria-label={title} onSubmit={(event) => {
      event.preventDefault();
      if (query.trim()) onRun(query.trim());
    }}>
      <label htmlFor="luma-command-query">{title}</label>
      <input id="luma-command-query" autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder={placeholder} />
      <footer><button type="button" onClick={onClose}>Cancel</button><button type="submit" disabled={!query.trim()}>Run</button></footer>
    </form>
  </div>;
}
