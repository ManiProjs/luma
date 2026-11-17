import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useFocusTrap } from "../hooks/useFocusTrap";
import type { CommandDescriptor } from "../../../shared/protocol";

type CommandPaletteProps = {
  commands: CommandDescriptor[];
  loading?: boolean;
  onClose: () => void;
  onSelect: (command: CommandDescriptor) => void;
};

function scoreMatch(value: string, query: string): number | null {
  const candidate = value.toLowerCase();
  const needle = query.toLowerCase().trim();
  if (!needle) return 0;
  if (candidate === needle) return 1000;
  if (candidate.startsWith(needle)) return 800 - (candidate.length - needle.length);
  const words = candidate.split(/[^a-z0-9]+/).filter(Boolean);
  const wordPrefix = words.findIndex((word) => word.startsWith(needle));
  if (wordPrefix >= 0) return 650 - wordPrefix;
  const substring = candidate.indexOf(needle);
  if (substring >= 0) return 500 - substring;

  let cursor = 0;
  let score = 0;
  let previous = -2;
  for (const character of needle) {
    const index = candidate.indexOf(character, cursor);
    if (index < 0) return null;
    score += index === previous + 1 ? 12 : Math.max(1, 8 - (index - cursor));
    if (index === 0 || /[^a-z0-9]/.test(candidate[index - 1] ?? "")) score += 8;
    previous = index;
    cursor = index + 1;
  }
  return score;
}

function shortcutParts(shortcut: string) {
  return shortcut.split(/\s*\+\s*/).map((part) => {
    const normalized = part.trim();
    return window.luma.platform === "darwin" && normalized.toLowerCase() === "ctrl" ? "⌘" : normalized;
  }).filter(Boolean);
}

export default function CommandPalette({ commands, loading = false, onClose, onSelect }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const panelRef = useFocusTrap(true);

  const filtered = useMemo(() => commands
    .map((command) => {
      const score = Math.max(
        scoreMatch(command.command, query) ?? -1,
        scoreMatch(command.description, query) ?? -1,
        scoreMatch(command.category, query) ?? -1,
        scoreMatch(command.id.replace(/[-_]/g, " "), query) ?? -1,
      );
      return { command, score };
    })
    .filter((item) => item.score >= 0)
    .sort((left, right) => right.score - left.score || left.command.category.localeCompare(right.command.category) || left.command.command.localeCompare(right.command.command)), [commands, query]);

  const groups = useMemo(() => {
    const result = new Map<string, CommandDescriptor[]>();
    for (const { command } of filtered) {
      const category = command.category || "General";
      const group = result.get(category) ?? [];
      group.push(command);
      result.set(category, group);
    }
    return [...result.entries()];
  }, [filtered]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-command-index="${activeIndex}"]`)?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Element && event.target.closest(".luma-command-palette-panel")) return;
      onClose();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [onClose]);

  const chooseActive = () => {
    const selected = filtered[activeIndex]?.command;
    if (selected) onSelect(selected);
  };

  return createPortal(
    <div className="luma-command-palette-backdrop" role="presentation">
      <section ref={panelRef} className="luma-command-palette-panel" role="dialog" aria-modal="true" aria-label="Command palette">
        <div className="luma-command-palette-search">
          <svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.8" /><path d="m16 16 4.2 4.2" /></svg>
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" && filtered.length) { event.preventDefault(); setActiveIndex((index) => Math.min(filtered.length - 1, index + 1)); }
              else if (event.key === "ArrowUp" && filtered.length) { event.preventDefault(); setActiveIndex((index) => Math.max(0, index - 1)); }
              else if (event.key === "Enter") { event.preventDefault(); chooseActive(); }
              else if (event.key === "Escape") { event.preventDefault(); onClose(); }
            }}
            placeholder="Search commands…"
            aria-label="Search commands"
            aria-autocomplete="list"
            aria-controls="luma-command-palette-list"
            aria-activedescendant={filtered[activeIndex] ? `luma-command-${filtered[activeIndex].command.id}` : undefined}
            autoComplete="off"
            spellCheck={false}
          />
          <kbd className="luma-command-palette-esc">esc</kbd>
        </div>

        <div className="luma-command-palette-list" id="luma-command-palette-list" ref={listRef} role="listbox" aria-label="Available commands">
          {loading ? <div className="luma-command-palette-empty">Loading commands…</div> : filtered.length === 0 ? <div className="luma-command-palette-empty">No commands match “{query}”.</div> : groups.map(([category, items]) => (
            <section className="luma-command-palette-group" key={category}>
              <div className="luma-command-palette-category">{category}</div>
              {items.map((command) => {
                const index = filtered.findIndex((item) => item.command.id === command.id);
                const active = index === activeIndex;
                return <button
                  id={`luma-command-${command.id}`}
                  key={command.id}
                  type="button"
                  role="option"
                  aria-selected={active}
                  data-command-index={index}
                  className={`luma-command-palette-item${active ? " is-active" : ""}`}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => onSelect(command)}
                >
                  <span className="luma-command-palette-item-copy">
                    <span className="luma-command-palette-name">{command.command}</span>
                    <span className="luma-command-palette-description">{command.description}</span>
                  </span>
                  {command.shortcut && <span className="luma-command-palette-shortcut">{shortcutParts(command.shortcut).map((part, partIndex) => <kbd key={`${part}-${partIndex}`}>{part}</kbd>)}</span>}
                </button>;
              })}
            </section>
          ))}
        </div>

        <footer className="luma-command-palette-footer">
          <span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span>
          <span><kbd>↵</kbd> Run command</span>
          <span><kbd>{window.luma.platform === "darwin" ? "⌥" : "Alt"}</kbd>+<kbd>←</kbd>/<kbd>→</kbd> Back/forward</span>
          <span><kbd>esc</kbd> Close</span>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
