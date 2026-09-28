export function SparkIcon({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 2l1.7 6.3L20 10l-6.3 1.7L12 18l-1.7-6.3L4 10l6.3-1.7L12 2z" />
      <path d="M19 16l.7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7L19 16z" />
    </svg>
  );
}

export default function LumaWordmark({
  compact = false,
}: {
  compact?: boolean;
}) {
  return (
    <div className="flex items-center gap-2">
      <div
        className="
          flex h-6 w-6 items-center justify-center
          rounded-md
          bg-[var(--luma-accent-soft)]
          text-[var(--luma-accent)]
        "
        aria-hidden="true"
      >
        <SparkIcon />
      </div>

      <span
        className={
          compact
            ? "text-[13px] font-medium tracking-tight"
            : "text-[13px] font-semibold tracking-tight"
        }
      >
        Luma
      </span>
    </div>
  );
}
