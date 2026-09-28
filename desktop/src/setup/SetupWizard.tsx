import { AnimatePresence, motion } from "motion/react";

import { useEffect } from "react";

import LumaWordmark from "../components/LumaWordmark";

import { useSetupStore } from "../stores/setupStore";

import SetupWelcome from "./SetupWelcome";
import SetupProvider from "./SetupProvider";
import SetupModel from "./SetupModel";
import SetupComplete from "./SetupComplete";

interface SetupWizardProps {
  onComplete: () => void;
}

const STEP_LABELS = {
  welcome: "Step 1 of 4",
  provider: "Step 2 of 4",
  model: "Step 3 of 4",
  complete: "Step 4 of 4",
} as const;

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
    <div className="h-screen w-screen overflow-hidden bg-[var(--luma-bg)] text-[var(--luma-text)]">
      <div className="flex h-full w-full flex-col">
        <header
          className="
            drag-region
            flex h-[44px] shrink-0 items-center
            border-b border-[var(--luma-border)]
            bg-[var(--luma-surface)]
            pl-[82px] pr-5
          "
        >
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
            className="pointer-events-none"
          >
            <LumaWordmark />
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
            className="ml-auto text-[12px] text-[var(--luma-text-muted)]"
          >
            {finished ? "Ready" : STEP_LABELS[step]}
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
