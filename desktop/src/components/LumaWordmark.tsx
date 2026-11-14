export function LumaMark({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 256 256"
      fill="none"
      style={{ isolation: "isolate", flex: "none" }}
      aria-hidden="true"
    >
      <rect width="256" height="256" rx="58" fill="#0F0F0E" />
      <rect x="0.5" y="0.5" width="255" height="255" rx="57.5" fill="none" stroke="#FFFFFF" strokeOpacity="0.14" />
      <circle cx="100" cy="128" r="64" fill="#5B7DF5" />
      <circle cx="156" cy="128" r="64" fill="#F58A47" style={{ mixBlendMode: "screen" }} />
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
