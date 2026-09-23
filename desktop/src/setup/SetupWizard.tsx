import { AnimatePresence, motion } from "motion/react";

import { useEffect } from "react";

import { useSetupStore } from "../stores/setupStore";

import SetupWelcome from "./SetupWelcome";
import SetupProvider from "./SetupProvider";
import SetupModel from "./SetupModel";
import SetupComplete from "./SetupComplete";

interface SetupWizardProps {
  onComplete: () => void;
}

export default function SetupWizard({ onComplete }: SetupWizardProps) {
  const step = useSetupStore((state) => state.step);

  const finished = useSetupStore((state) => state.finished);

  const loadProviders = useSetupStore((state) => state.loadProviders);

  const loadConfiguration = useSetupStore((state) => state.loadConfiguration);

  useEffect(() => {
    async function initialize() {
      try {
        await loadProviders();
        await loadConfiguration();
      } catch {
        // Setup screens display errors.
      }
    }

    void initialize();
  }, [loadProviders, loadConfiguration]);

  useEffect(() => {
    if (finished) {
      onComplete();
    }
  }, [finished, onComplete]);

  return (
    <div className="h-screen w-screen overflow-hidden bg-[#08090b] text-zinc-100">
      <div className="flex h-full w-full flex-col">
        <header className="flex h-12 shrink-0 items-center border-b border-white/[0.06] pl-[84px] pr-5">
          <motion.div
            initial={{
              opacity: 0,
              x: -8,
            }}
            animate={{
              opacity: 1,
              x: 0,
            }}
            transition={{
              duration: 0.25,
            }}
            className="flex items-center gap-2.5"
          >
            <motion.div
              whileHover={{
                rotate: -4,
                scale: 1.04,
              }}
              className="flex h-6 w-6 items-center justify-center rounded-md bg-zinc-100 text-[10px] font-bold text-zinc-950"
            >
              L
            </motion.div>

            <span className="text-[13px] font-semibold tracking-tight text-zinc-200">
              Luma
            </span>
          </motion.div>

          <motion.div
            initial={{
              opacity: 0,
            }}
            animate={{
              opacity: 1,
            }}
            transition={{
              delay: 0.15,
            }}
            className="ml-auto text-[11px] text-zinc-600"
          >
            Setup
          </motion.div>
        </header>

        <main className="min-h-0 flex-1">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={step}
              className="h-full"
              initial={{
                opacity: 0,
                x: 18,
              }}
              animate={{
                opacity: 1,
                x: 0,
              }}
              exit={{
                opacity: 0,
                x: -12,
              }}
              transition={{
                duration: 0.25,
                ease: [0.22, 1, 0.36, 1],
              }}
            >
              {step === "welcome" && <SetupWelcome />}

              {step === "provider" && <SetupProvider />}

              {step === "model" && <SetupModel />}

              {step === "complete" && <SetupComplete />}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
    </div>
  );
}
