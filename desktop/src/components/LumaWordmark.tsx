export function LumaMark({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M5 3.5h4.5v13h9.5V21H5z" />
    </svg>
  );
}

export default function LumaWordmark({
  compact = false,
  section,
}: {
  compact?: boolean;
  section?: "teammates";
}) {
  return (
    <div className={`luma-wordmark flex items-center gap-2 ${compact ? "is-compact" : ""}`}>
      <div
        className="luma-wordmark-mark flex h-6 w-6 items-center justify-center text-[var(--luma-accent)]"
        aria-hidden="true"
      >
        <LumaMark />
      </div>

      <span
        className="luma-wordmark-text"
      >
        LUMA
      </span>
      {section === "teammates" && <span className="luma-wordmark-section">teammates</span>}
    </div>
  );
}
