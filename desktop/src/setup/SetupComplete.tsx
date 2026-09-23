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
            className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl border border-white/[0.08] bg-white/[0.04] text-xl text-zinc-100"
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
            className="mt-5 text-2xl font-semibold tracking-tight text-zinc-100"
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
            className="mt-2 text-sm leading-6 text-zinc-500"
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
      <div className="border-b border-white/[0.06] px-7 py-5">
        <div className="text-[11px] font-medium uppercase tracking-[0.14em] text-zinc-600">
          Step 3 of 3
        </div>

        <h1 className="mt-1.5 text-[22px] font-semibold tracking-tight text-zinc-100">
          Review your setup
        </h1>

        <p className="mt-1 text-[13px] text-zinc-500">
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
                className="rounded-lg border border-red-500/10 bg-red-500/[0.04] px-3 py-2.5 text-[12px] text-red-400"
              >
                {error}
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-white/[0.06] px-7 py-4">
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
          className="h-9 rounded-lg px-3 text-[12px] text-zinc-500 transition hover:bg-white/[0.04] hover:text-zinc-300 disabled:opacity-30"
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
          className="h-9 rounded-lg bg-zinc-100 px-4 text-[12px] font-medium text-zinc-900 transition hover:bg-white disabled:cursor-not-allowed disabled:opacity-30"
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
      className="flex items-center justify-between gap-6 rounded-xl border border-white/[0.06] bg-white/[0.02] px-4 py-3.5"
    >
      <div className="text-[12px] text-zinc-500">{label}</div>

      <div className="min-w-0 text-right">
        <div className="truncate text-[13px] font-medium text-zinc-200">
          {value}
        </div>

        {detail && (
          <div className="mt-0.5 truncate text-[10px] text-zinc-600">
            {detail}
          </div>
        )}
      </div>
    </motion.div>
  );
}
