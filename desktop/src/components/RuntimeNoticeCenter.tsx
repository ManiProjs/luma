import { useState } from "react";
import type { RuntimeEvent } from "../../../shared/protocol";

type RuntimeNoticeCenterProps = {
  notices: RuntimeEvent[];
  onDismiss: (eventId: string) => void;
  onDismissAll: () => void;
};

function noticeSummary(event: RuntimeEvent): string {
  const payload = typeof event.payload === "object" && event.payload !== null
    ? event.payload as Record<string, unknown>
    : null;
  const detail = payload
    ? [payload.message, payload.title, payload.name, payload.tool, payload.error, payload.reason, payload.content]
      .find((value): value is string => typeof value === "string" && value.trim().length > 0)
    : undefined;
  if (detail) return detail.length > 180 ? `${detail.slice(0, 177)}…` : detail;
  if (event.task_id) return `Task ${event.task_id}`;
  if (event.agent_id) return `Teammate ${event.agent_id}`;
  return "A runtime event was reported.";
}

function noticeTitle(event: RuntimeEvent): string {
  return event.event_type.split(/[._-]+/).filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ") || "Runtime notice";
}

function formatNoticeTime(timestamp: number): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) return "";
  return new Date(timestamp * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

export default function RuntimeNoticeCenter({ notices, onDismiss, onDismissAll }: RuntimeNoticeCenterProps) {
  const [open, setOpen] = useState(false);

  return <div className="luma-runtime-notice-center">
    <button
      type="button"
      className="luma-runtime-notice-trigger"
      aria-expanded={open}
      aria-controls="luma-runtime-notice-panel"
      aria-label={`Runtime notifications, ${notices.length} ${notices.length === 1 ? "notice" : "notices"}`}
      onClick={() => setOpen((value) => !value)}
    >
      <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9" />
        <path d="M10 21h4" />
      </svg>
      <span>Activity</span>
      {notices.length > 0 && <span className="luma-runtime-notice-count">{notices.length > 99 ? "99+" : notices.length}</span>}
    </button>
    {open && <section id="luma-runtime-notice-panel" className="luma-runtime-notice-panel" aria-label="Runtime notifications">
      <header>
        <strong>Runtime activity</strong>
        <div>
          {notices.length > 0 && <button type="button" onClick={onDismissAll}>Clear all</button>}
          <button type="button" aria-label="Close notifications" onClick={() => setOpen(false)}>×</button>
        </div>
      </header>
      {notices.length === 0 ? <p className="luma-runtime-notice-empty">You're all caught up.</p> : <ul>
        {notices.map((event) => <li key={event.event_id}>
          <div className="luma-runtime-notice-copy">
            <div className="luma-runtime-notice-heading">
              <strong>{noticeTitle(event)}</strong>
              <time>{formatNoticeTime(event.created_at)}</time>
            </div>
            <p>{noticeSummary(event)}</p>
          </div>
          <button type="button" aria-label={`Dismiss ${noticeTitle(event)} notice`} onClick={() => onDismiss(event.event_id)}>×</button>
        </li>)}
      </ul>}
    </section>}
  </div>;
}
