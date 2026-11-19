import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type RichBlock =
  | { type: "image"; url: string; alt?: string; caption?: string }
  | { type: "gallery"; images: { url: string; alt?: string; caption?: string }[] }
  | { type: "table"; title?: string; columns: string[]; rows: (string | number | null)[][] }
  | { type: "stat"; label: string; value: string | number; detail?: string }
  | { type: "chart"; title?: string; labels: string[]; values: number[]; value_suffix?: string }
  | { type: "callout"; title?: string; text: string; tone?: "info" | "success" | "warning" }
  | { type: "link"; label: string; url: string; description?: string }
  | { type: "action"; label: string; prompt: string; description?: string; tone?: "default" | "primary" }
  | { type: "choices"; title?: string; description?: string; options: { label: string; prompt: string; description?: string }[] }
  | { type: "form"; title: string; description?: string; submit_label?: string; fields: { name: string; label: string; type: "text" | "textarea" | "select"; required?: boolean; placeholder?: string; options?: string[] }[]; prompt: string };

function safeUrl(value: unknown): string | null {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    return (url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password ? url.toString() : null;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function validateBlock(value: unknown): RichBlock | null {
  if (!isRecord(value) || typeof value.type !== "string") return null;
  if (value.type === "image") {
    const url = safeUrl(value.url);
    if (!url) return null;
    return { type: "image", url, alt: typeof value.alt === "string" ? value.alt : undefined, caption: typeof value.caption === "string" ? value.caption : undefined };
  }
  if (value.type === "gallery" && Array.isArray(value.images)) {
    const images = value.images.slice(0, 8).flatMap((item) => {
      if (!isRecord(item)) return [];
      const url = safeUrl(item.url);
      return url ? [{ url, alt: typeof item.alt === "string" ? item.alt : undefined, caption: typeof item.caption === "string" ? item.caption : undefined }] : [];
    });
    return images.length ? { type: "gallery", images } : null;
  }
  if (value.type === "table" && Array.isArray(value.columns) && Array.isArray(value.rows)) {
    const columns = value.columns.slice(0, 8).filter((item): item is string => typeof item === "string");
    if (!columns.length) return null;
    const rows = value.rows.slice(0, 50).filter(Array.isArray).map((row) => row.slice(0, columns.length).map((cell) => typeof cell === "string" || typeof cell === "number" ? cell : null));
    return { type: "table", title: typeof value.title === "string" ? value.title : undefined, columns, rows };
  }
  if (value.type === "stat" && typeof value.label === "string" && (typeof value.value === "string" || typeof value.value === "number")) {
    return { type: "stat", label: value.label, value: value.value, detail: typeof value.detail === "string" ? value.detail : undefined };
  }
  if (value.type === "chart" && Array.isArray(value.labels) && Array.isArray(value.values)) {
    const labels = value.labels.slice(0, 12).filter((item): item is string => typeof item === "string");
    const values = value.values.slice(0, labels.length).filter((item): item is number => typeof item === "number" && Number.isFinite(item));
    if (!labels.length || labels.length !== values.length) return null;
    return { type: "chart", title: typeof value.title === "string" ? value.title : undefined, labels, values, value_suffix: typeof value.value_suffix === "string" ? value.value_suffix : undefined };
  }
  if (value.type === "callout" && typeof value.text === "string") {
    const tone = value.tone === "success" || value.tone === "warning" ? value.tone : "info";
    return { type: "callout", title: typeof value.title === "string" ? value.title : undefined, text: value.text, tone };
  }
  if (value.type === "link" && typeof value.label === "string") {
    const url = safeUrl(value.url);
    if (!url) return null;
    return { type: "link", label: value.label, url, description: typeof value.description === "string" ? value.description : undefined };
  }
  if (value.type === "action" && typeof value.label === "string" && typeof value.prompt === "string" && value.prompt.trim()) {
    return { type: "action", label: value.label.slice(0, 100), prompt: value.prompt.slice(0, 8_000), description: typeof value.description === "string" ? value.description.slice(0, 500) : undefined, tone: value.tone === "primary" ? "primary" : "default" };
  }
  if (value.type === "choices" && Array.isArray(value.options)) {
    const options = value.options.slice(0, 8).flatMap((item) => isRecord(item) && typeof item.label === "string" && typeof item.prompt === "string" && item.prompt.trim() ? [{ label: item.label.slice(0, 100), prompt: item.prompt.slice(0, 8_000), description: typeof item.description === "string" ? item.description.slice(0, 300) : undefined }] : []);
    return options.length ? { type: "choices", title: typeof value.title === "string" ? value.title.slice(0, 160) : undefined, description: typeof value.description === "string" ? value.description.slice(0, 500) : undefined, options } : null;
  }
  if (value.type === "form" && typeof value.title === "string" && typeof value.prompt === "string" && value.prompt.trim() && Array.isArray(value.fields)) {
    const fields = value.fields.slice(0, 12).flatMap((item) => {
      if (!isRecord(item) || typeof item.name !== "string" || !/^[a-zA-Z][a-zA-Z0-9_]{0,31}$/.test(item.name) || typeof item.label !== "string") return [];
      const type: "text" | "textarea" | "select" = item.type === "textarea" || item.type === "select" ? item.type : "text";
      const options = type === "select" && Array.isArray(item.options) ? item.options.slice(0, 20).filter((option): option is string => typeof option === "string" && option.length <= 120) : undefined;
      if (type === "select" && !options?.length) return [];
      return [{ name: item.name, label: item.label.slice(0, 120), type, required: item.required === true, placeholder: typeof item.placeholder === "string" ? item.placeholder.slice(0, 160) : undefined, options }];
    });
    return fields.length ? { type: "form", title: value.title.slice(0, 160), description: typeof value.description === "string" ? value.description.slice(0, 500) : undefined, submit_label: typeof value.submit_label === "string" ? value.submit_label.slice(0, 60) : undefined, fields, prompt: value.prompt.slice(0, 8_000) } : null;
  }
  return null;
}

function RichBlockView({ block, onAction }: { block: RichBlock; onAction?: (prompt: string) => void }) {
  if (block.type === "image") return <figure className="my-3"><img src={block.url} alt={block.alt ?? ""} loading="lazy" className="max-h-[420px] max-w-full rounded-lg border border-[var(--luma-border)] object-contain" />{block.caption && <figcaption className="mt-1 text-[10px] text-[var(--luma-text-muted)]">{block.caption}</figcaption>}</figure>;
  if (block.type === "gallery") return <div className="my-3 grid grid-cols-2 gap-2 sm:grid-cols-3">{block.images.map((item, index) => <figure key={`${item.url}-${index}`} className="min-w-0"><img src={item.url} alt={item.alt ?? ""} loading="lazy" className="aspect-[4/3] w-full rounded-md border border-[var(--luma-border)] object-cover" />{item.caption && <figcaption className="mt-1 truncate text-[10px] text-[var(--luma-text-muted)]">{item.caption}</figcaption>}</figure>)}</div>;
  if (block.type === "table") return <div className="my-3 max-w-full overflow-x-auto">{block.title && <div className="mb-1 text-[11px] font-semibold text-[var(--luma-text)]">{block.title}</div>}<table className="w-full border-collapse text-left text-[11px]"><thead><tr>{block.columns.map((column, index) => <th key={index} className="border-b border-[var(--luma-border-strong)] px-2 py-1.5 font-semibold text-[var(--luma-text)]">{column}</th>)}</tr></thead><tbody>{block.rows.map((row, index) => <tr key={index}>{block.columns.map((_, cell) => <td key={cell} className="border-b border-[var(--luma-border)] px-2 py-1.5 text-[var(--luma-text-secondary)]">{row[cell] ?? "—"}</td>)}</tr>)}</tbody></table></div>;
  if (block.type === "stat") return <div className="my-2 inline-flex min-w-32 flex-col rounded-md border border-[var(--luma-border)] px-3 py-2"><span className="text-[10px] text-[var(--luma-text-muted)]">{block.label}</span><strong className="mt-0.5 text-xl font-semibold text-[var(--luma-text)]">{block.value}</strong>{block.detail && <span className="mt-0.5 text-[10px] text-[var(--luma-text-muted)]">{block.detail}</span>}</div>;
  if (block.type === "chart") {
    const max = Math.max(1, ...block.values.map((value) => Math.abs(value)));
    return <div className="my-3 rounded-md border border-[var(--luma-border)] p-3">{block.title && <div className="mb-3 text-[11px] font-semibold text-[var(--luma-text)]">{block.title}</div>}<div className="flex h-36 gap-2 px-1">{block.values.map((value, index) => {
      const height = `${Math.max(value === 0 ? 0 : 2, Math.abs(value) / max * 50)}%`;
      return <div key={index} className="flex min-w-0 flex-1 flex-col items-center">
        <span className="h-5 shrink-0 text-[9px] tabular-nums text-[var(--luma-text-muted)]">{value}{block.value_suffix ?? ""}</span>
        <div className="relative min-h-0 w-full flex-1">
          <div aria-hidden="true" className="absolute inset-x-0 top-1/2 border-t border-[var(--luma-border-strong)]" />
          {value !== 0 && <div className={`absolute left-1/2 w-full max-w-12 -translate-x-1/2 bg-[var(--luma-accent)] opacity-80 ${value > 0 ? "bottom-1/2 rounded-t-sm" : "top-1/2 rounded-b-sm"}`} style={{ height }} />}
        </div>
      </div>;
    })}</div><div className="mt-1 flex gap-2">{block.labels.map((label, index) => <span key={index} title={label} className="min-w-0 flex-1 truncate text-center text-[9px] text-[var(--luma-text-muted)]">{label}</span>)}</div></div>;
  }
  if (block.type === "callout") return <aside className={`my-3 rounded-md border px-3 py-2 ${block.tone === "warning" ? "border-amber-500/30 bg-amber-500/5" : block.tone === "success" ? "border-emerald-500/30 bg-emerald-500/5" : "border-[var(--luma-border)] bg-[var(--luma-surface)]"}`}><div className="text-[11px] font-semibold text-[var(--luma-text)]">{block.title ?? (block.tone === "warning" ? "Note" : block.tone === "success" ? "Done" : "Info")}</div><p className="mt-1 whitespace-pre-wrap text-[11px] leading-5 text-[var(--luma-text-secondary)]">{block.text}</p></aside>;
  if (block.type === "link") return <a href={block.url} target="_blank" rel="noreferrer" className="my-2 flex items-start gap-2 rounded-md border border-[var(--luma-border)] px-3 py-2 text-[var(--luma-accent)] hover:bg-[var(--luma-surface-hover)]"><span className="min-w-0 flex-1"><strong className="block text-[11px]">{block.label}</strong>{block.description && <span className="mt-0.5 block text-[10px] text-[var(--luma-text-muted)]">{block.description}</span>}</span><span aria-hidden="true">↗</span></a>;
  if (block.type === "action") return <div className="my-2 flex flex-wrap items-center gap-2"><button type="button" disabled={!onAction} onClick={() => onAction?.(block.prompt)} className={`rounded-md border px-3 py-2 text-[11px] font-medium disabled:opacity-50 ${block.tone === "primary" ? "border-[var(--luma-accent)] bg-[var(--luma-accent)] text-white" : "border-[var(--luma-border)] text-[var(--luma-text)] hover:bg-[var(--luma-surface-hover)]"}`}>{block.label}</button>{block.description && <span className="text-[10px] text-[var(--luma-text-muted)]">{block.description}</span>}</div>;
  if (block.type === "choices") return <div className="my-3 rounded-md border border-[var(--luma-border)] p-3">{block.title && <div className="text-[12px] font-semibold">{block.title}</div>}{block.description && <p className="mt-1 text-[10px] text-[var(--luma-text-muted)]">{block.description}</p>}<div className="mt-2 grid gap-1.5">{block.options.map((option, index) => <button key={`${option.label}-${index}`} type="button" disabled={!onAction} onClick={() => onAction?.(option.prompt)} className="flex items-center justify-between gap-3 rounded-md border border-[var(--luma-border)] px-3 py-2 text-left hover:bg-[var(--luma-surface-hover)] disabled:opacity-50"><span className="min-w-0"><strong className="block text-[11px] font-medium">{option.label}</strong>{option.description && <span className="mt-0.5 block text-[10px] text-[var(--luma-text-muted)]">{option.description}</span>}</span><span aria-hidden="true" className="text-[var(--luma-text-muted)]">→</span></button>)}</div></div>;
  return <InteractiveForm block={block} onAction={onAction} />;
}

function InteractiveForm({ block, onAction }: { block: Extract<RichBlock, { type: "form" }>; onAction?: (prompt: string) => void }) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [submitted, setSubmitted] = useState(false);
  return <form className="my-3 rounded-md border border-[var(--luma-border)] p-3" onSubmit={(event) => {
    event.preventDefault();
    if (!onAction || submitted) return;
    const missing = block.fields.some((field) => field.required && !values[field.name]?.trim());
    if (missing) return;
    const prompt = block.prompt.replace(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g, (_match, name: string) => values[name] ?? "");
    setSubmitted(true);
    onAction(prompt);
  }}>
    <div className="text-[12px] font-semibold">{block.title}</div>{block.description && <p className="mt-1 text-[10px] text-[var(--luma-text-muted)]">{block.description}</p>}
    <div className="mt-3 grid gap-2">{block.fields.map((field) => <label key={field.name} className="grid gap-1 text-[10px] font-medium text-[var(--luma-text-secondary)]">{field.label}{field.type === "select" ? <select required={field.required} value={values[field.name] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.name]: event.target.value }))} className="luma-input h-8 text-[11px]"><option value="">Choose…</option>{field.options?.map((option) => <option key={option} value={option}>{option}</option>)}</select> : field.type === "textarea" ? <textarea required={field.required} rows={3} maxLength={4000} value={values[field.name] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.name]: event.target.value }))} placeholder={field.placeholder} className="luma-input resize-y text-[11px]" /> : <input required={field.required} maxLength={1000} value={values[field.name] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.name]: event.target.value }))} placeholder={field.placeholder} className="luma-input h-8 text-[11px]" />}</label>)}</div>
    <div className="mt-3 flex justify-end"><button type="submit" disabled={!onAction || submitted} className="luma-btn-primary h-8 px-3 text-[11px]">{submitted ? "Submitted" : block.submit_label ?? "Continue"}</button></div>
  </form>;
}

function Markdown({ children }: { children: string }) {
  return <div className="prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{
    a({ href, children: linkChildren }) { const url = safeUrl(href); return url ? <a href={url} target="_blank" rel="noreferrer">{linkChildren}</a> : <>{linkChildren}</>; },
    img({ src, alt }) { const url = safeUrl(src); return url ? <img src={url} alt={alt ?? ""} loading="lazy" className="my-2 max-h-[420px] max-w-full rounded-lg border border-[var(--luma-border)] object-contain" /> : null; },
    pre({ children: code }) { return <pre className="my-2 overflow-x-auto rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] p-3">{code}</pre>; },
    p({ children: paragraph }) { return <p className="mb-2 last:mb-0">{paragraph}</p>; },
    ul({ children: list }) { return <ul className="mb-2 ml-4 list-disc">{list}</ul>; },
    ol({ children: list }) { return <ol className="mb-2 ml-4 list-decimal">{list}</ol>; },
    li({ children: item }) { return <li className="mb-1">{item}</li>; },
    h1({ children: heading }) { return <h1 className="mb-2 mt-3 text-[14px] font-semibold">{heading}</h1>; },
    h2({ children: heading }) { return <h2 className="mb-2 mt-3 text-[13px] font-semibold">{heading}</h2>; },
    h3({ children: heading }) { return <h3 className="mb-2 mt-3 text-[12px] font-semibold">{heading}</h3>; },
  }}>{children}</ReactMarkdown></div>;
}

/** Renders ordinary Markdown alongside validated, model-authored rich blocks. */
export default function RichResponse({ content, onAction }: { content: string; onAction?: (prompt: string) => void }) {
  const parts = content.split(/```luma\s*\n([\s\S]*?)```/gi);
  return <div className="min-w-0 space-y-1">{parts.map((part, index) => {
    if (index % 2 === 0) return part ? <Markdown key={index}>{part}</Markdown> : null;
    try {
      const block = validateBlock(JSON.parse(part.trim()));
      return block ? <RichBlockView key={index} block={block} onAction={onAction} /> : <Markdown key={index}>{`\`\`\`json\n${part}\n\`\`\``}</Markdown>;
    } catch {
      return <Markdown key={index}>{`\`\`\`json\n${part}\n\`\`\``}</Markdown>;
    }
  })}</div>;
}
