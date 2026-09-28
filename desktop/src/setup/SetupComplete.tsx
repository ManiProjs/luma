import { AnimatePresence, motion } from "motion/react";

import { useSetupStore } from "../stores/setupStore";

export default function SetupComplete() {
  const provider = useSetupStore((state) => state.provider);

  const model = useSetupStore((state) => state.model);

  const plannerModel = useSetupStore((state) => state.plannerModel);

  const saving = useSetupStore((state) => state.savingConfiguration);

  const finished = useSetupStore((state) => state.finished);

  const error = useSetupStore((state) => state.error);

  const save = useSetupStore((state) => state.saveConfiguration);

  const back = useSetupStore((state) => state.back);

  if (finished) {
    return (
      <motion.div
        initial={{
          opacity: 0,
          scale: 0.98,
        }}
        animate={{
          opacity: 1,
          scale: 1,
        }}
        className="flex h-full items-center justify-center"
      >
        <div className="w-full max-w-md text-center">
          <motion.div
            initial={{
              opacity: 0,
              scale: 0.6,
            }}
            animate={{
              opacity: 1,
              scale: 1,
            }}
            transition={{
              type: "spring",
              stiffness: 300,
              damping: 22,
            }}
            className="
              mx-auto flex h-14 w-14 items-center justify-center
              rounded-2xl
              border border-[var(--luma-border-strong)]
              bg-[var(--luma-accent-soft)]
              text-xl text-[var(--luma-accent)]
            "
          >
            ✓
          </motion.div>

          <motion.h1
            initial={{
              opacity: 0,
              y: 8,
            }}
            animate={{
              opacity: 1,
              y: 0,
            }}
            transition={{
              delay: 0.1,
            }}
            className="mt-5 text-[22px] font-semibold tracking-tight"
          >
            Luma is ready
          </motion.h1>

          <motion.p
            initial={{
              opacity: 0,
              y: 8,
            }}
            animate={{
              opacity: 1,
              y: 0,
            }}
            transition={{
              delay: 0.15,
            }}
            className="mt-2 text-[13px] leading-6 text-[var(--luma-text-secondary)]"
          >
            Your provider and model configuration has been saved.
          </motion.p>
        </div>
      </motion.div>
    );
  }

  return (
    <motion.div
      initial={{
        opacity: 0,
      }}
      animate={{
        opacity: 1,
      }}
      className="flex h-full min-h-0 flex-col"
    >
      <div className="border-b border-[var(--luma-border)] px-7 py-5">
        <div className="luma-kicker">Step 4 of 4</div>

        <h1 className="mt-1.5 text-[22px] font-semibold tracking-tight">
          Review your setup
        </h1>

        <p className="mt-1 text-[13px] text-[var(--luma-text-secondary)]">
          Check your configuration before starting Luma.
        </p>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 py-5">
        <motion.div
          initial={{
            opacity: 0,
            y: 8,
          }}
          animate={{
            opacity: 1,
            y: 0,
          }}
          transition={{
            delay: 0.08,
          }}
          className="mx-auto max-w-2xl space-y-2"
        >
          <Summary
            label="Provider"
            value={provider?.name ?? "Not selected"}
            detail={provider?.id}
          />

          <Summary
            label="Model"
            value={model?.name ?? "Not selected"}
            detail={model?.id}
          />

          <Summary
            label="Planner"
            value={plannerModel?.name ?? model?.name ?? "Not selected"}
            detail={plannerModel?.id ?? model?.id}
          />

          <AnimatePresence>
            {error && (
              <motion.div
                initial={{
                  opacity: 0,
                  y: -5,
                }}
                animate={{
                  opacity: 1,
                  y: 0,
                }}
                exit={{
                  opacity: 0,
                  y: -5,
                }}
                className="rounded-lg border border-[var(--luma-danger)]/20 bg-[var(--luma-danger)]/5 px-3 py-2.5 text-[13px] text-[var(--luma-danger)]"
              >
                {error}
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-[var(--luma-border)] px-7 py-4">
        <motion.button
          type="button"
          onClick={back}
          disabled={saving}
          whileHover={{
            x: -2,
          }}
          whileTap={{
            scale: 0.97,
          }}
          className="luma-btn-ghost"
        >
          Back
        </motion.button>

        <motion.button
          type="button"
          onClick={() => void save()}
          disabled={!provider || !model || saving}
          whileHover={
            provider && model && !saving
              ? {
                  y: -1,
                }
              : undefined
          }
          whileTap={
            provider && model && !saving
              ? {
                  scale: 0.97,
                }
              : undefined
          }
          className="luma-btn-primary"
        >
          {saving ? "Saving..." : "Finish setup"}
        </motion.button>
      </div>
    </motion.div>
  );
}

function Summary({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail?: string;
}) {
  return (
    <motion.div
      whileHover={{
        x: 2,
      }}
      transition={{
        duration: 0.15,
      }}
      className="luma-card flex items-center justify-between gap-6 px-4 py-3.5"
    >
      <div className="text-[13px] text-[var(--luma-text-secondary)]">{label}</div>

      <div className="min-w-0 text-right">
        <div className="truncate text-[13px] font-medium">{value}</div>

        {detail && (
          <div className="mt-0.5 truncate text-[12px] text-[var(--luma-text-muted)]">
            {detail}
          </div>
        )}
      </div>
    </motion.div>
  );
}
