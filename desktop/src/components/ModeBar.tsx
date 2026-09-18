import { useEffect, useRef, useState, type ReactNode } from "react";

import { Glass } from "@samasante/liquid-glass";

export type AppMode = "agent" | "ide" | "changes";

type ModeBarProps = {
  mode: AppMode;
  onModeChange: (mode: AppMode) => void;
};

type ModeItem = {
  id: AppMode;
  label: string;
  icon: ReactNode;
};

function AgentIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 2.8l1.5 6.2L19.7 10.5l-6.2 1.5L12 18.2l-1.5-6.2-6.2-1.5L10.5 9 12 2.8z" />
    </svg>
  );
}

function IDEIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x="3.5" y="4" width="17" height="16" rx="2.5" />

      <path d="M8 8.5l-2 2 2 2" />
      <path d="M11 13h5" />
    </svg>
  );
}

function ChangesIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 4v11" />
      <path d="M6 15l-2.5-2.5" />
      <path d="M6 15l2.5 2.5" />

      <path d="M18 20V9" />
      <path d="M18 9l-2.5 2.5" />
      <path d="M18 9l2.5 2.5" />
    </svg>
  );
}

const items: ModeItem[] = [
  {
    id: "agent",
    label: "Agent",
    icon: <AgentIcon />,
  },
  {
    id: "ide",
    label: "IDE",
    icon: <IDEIcon />,
  },
  {
    id: "changes",
    label: "Changes",
    icon: <ChangesIcon />,
  },
];

export default function ModeBar({ mode, onModeChange }: ModeBarProps) {
  const itemRefs = useRef<Record<AppMode, HTMLButtonElement | null>>({
    agent: null,
    ide: null,
    changes: null,
  });

  const barRef = useRef<HTMLDivElement>(null);

  const [hovered, setHovered] = useState<AppMode | null>(null);

  const [pressed, setPressed] = useState(false);

  const [lens, setLens] = useState({
    left: 0,
    width: 118,
  });

  const updateLens = (targetMode: AppMode) => {
    const target = itemRefs.current[targetMode];
    const bar = barRef.current;

    if (!target || !bar) {
      return;
    }

    const targetRect = target.getBoundingClientRect();

    const barRect = bar.getBoundingClientRect();

    const width = Math.max(118, targetRect.width + 2);

    setLens({
      left: targetRect.left - barRect.left + targetRect.width / 2 - width / 2,
      width,
    });
  };

  useEffect(() => {
    updateLens(mode);

    const handleResize = () => {
      updateLens(hovered ?? mode);
    };

    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
    };
  }, [mode, hovered]);

  const targetMode = hovered ?? mode;

  useEffect(() => {
    updateLens(targetMode);
  }, [targetMode]);

  return (
    <div
      ref={barRef}
      className="
        relative
        h-[66px]
        w-max
        shrink-0
      "
    >
      {/* Base Liquid Glass */}
      <Glass
        className="
          absolute
          inset-0
          overflow-hidden
          rounded-[33px]
          border
          border-white/[0.14]
          shadow-[0_16px_50px_rgba(0,0,0,0.42)]
        "
        style={{
          background: "rgba(255,255,255,0.055)",
          backdropFilter: "blur(24px) saturate(150%)",
          WebkitBackdropFilter: "blur(24px) saturate(150%)",
        }}
        optics={{
          depth: 0.9,
          curvature: 0.45,
          dispersion: 0.14,
        }}
      >
        <div className="h-full w-full" />
      </Glass>

      {/* Moving Liquid Glass selection lens */}
      <div
        className="
          pointer-events-none
          absolute
          top-[9px]
          h-[48px]
          overflow-hidden
          rounded-[25px]
          transition-[left,width]
          duration-300
          ease-[cubic-bezier(0.22,1,0.36,1)]
        "
        style={{
          left: lens.left,
          width: lens.width,
        }}
      >
        <Glass
          className="
            absolute
            inset-0
            overflow-hidden
            rounded-[25px]
            border
            border-white/[0.22]
          "
          style={{
            background: pressed
              ? "rgba(255,255,255,0.18)"
              : "rgba(255,255,255,0.105)",

            backdropFilter: "blur(15px) saturate(180%)",

            WebkitBackdropFilter: "blur(15px) saturate(180%)",

            boxShadow: [
              "inset 0 1px 0 rgba(255,255,255,0.32)",
              "inset 0 -1px 0 rgba(255,255,255,0.06)",
              "0 3px 16px rgba(0,0,0,0.16)",
            ].join(","),
          }}
          optics={{
            depth: pressed ? 1.15 : 0.95,
            curvature: pressed ? 0.52 : 0.44,
            dispersion: pressed ? 0.2 : 0.14,
          }}
        >
          <div className="h-full w-full" />
        </Glass>

        {/* Moving reflection */}
        <div
          className={`
            pointer-events-none
            absolute
            -inset-y-8
            left-1/2
            w-[38px]
            -translate-x-1/2
            rotate-[20deg]
            rounded-full
            bg-white/[0.22]
            blur-[9px]
            transition-opacity
            duration-150
            ${pressed ? "opacity-100" : "opacity-45"}
          `}
        />

        {/* Top edge reflection */}
        <div
          className="
            pointer-events-none
            absolute
            left-4
            right-4
            top-0
            h-px
            bg-white/[0.42]
            blur-[0.5px]
          "
        />

        {/* Bottom edge */}
        <div
          className="
            pointer-events-none
            absolute
            bottom-0
            left-5
            right-5
            h-px
            bg-white/[0.10]
            blur-[1px]
          "
        />
      </div>

      {/* Buttons */}
      <div
        className="
          relative
          z-20
          flex
          h-full
          items-center
          gap-1
          px-2
        "
      >
        {items.map((item) => {
          const active = mode === item.id;
          const isHovered = hovered === item.id;

          return (
            <button
              key={item.id}
              ref={(element) => {
                itemRefs.current[item.id] = element;
              }}
              type="button"
              aria-label={item.label}
              aria-pressed={active}
              onPointerEnter={() => {
                setHovered(item.id);
              }}
              onPointerLeave={() => {
                setHovered(null);
              }}
              onPointerDown={(event) => {
                if (event.button === 0) {
                  setPressed(true);
                }
              }}
              onPointerUp={(event) => {
                if (event.button === 0) {
                  setPressed(false);
                  onModeChange(item.id);
                }
              }}
              onPointerCancel={() => {
                setPressed(false);
              }}
              onContextMenu={(event) => {
                event.preventDefault();
              }}
              className={`
                relative
                flex
                h-12
                min-w-[118px]
                items-center
                justify-center
                gap-2.5
                rounded-[25px]
                px-4
                text-[13px]
                font-medium
                select-none
                outline-none
                transition-[transform,color]
                duration-200
                ease-out

                ${isHovered || active ? "text-white" : "text-zinc-500"}

                ${pressed && isHovered ? "scale-[0.94]" : "scale-100"}

                focus-visible:ring-1
                focus-visible:ring-white/30
              `}
            >
              <span
                className={`
                  relative
                  z-30
                  transition-transform
                  duration-200
                  ${
                    isHovered || active
                      ? "scale-105 text-white"
                      : "text-zinc-500"
                  }
                `}
              >
                {item.icon}
              </span>

              <span className="relative z-30">{item.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
