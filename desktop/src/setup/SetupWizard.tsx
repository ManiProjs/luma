import { useEffect, useRef, useState } from "react";

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
  const previousStep = useRef(step);
  const [hasNavigated, setHasNavigated] = useState(false);
  const stepOrder = ["welcome", "provider", "model", "complete"] as const;
  const direction = stepOrder.indexOf(step) >= stepOrder.indexOf(previousStep.current)
    ? "forward"
    : "backward";

  useEffect(() => {
    if (previousStep.current !== step) {
      setHasNavigated(true);
    }
    previousStep.current = step;
  }, [step]);

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
    <div id="setup-wizard" className="h-screen w-screen overflow-hidden bg-[var(--luma-bg)] text-[var(--luma-text)]">
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
          <div className="pointer-events-none">
            <LumaWordmark />
          </div>

          <div className="ml-auto text-[12px] text-[var(--luma-text-muted)]">
            {finished ? "Ready" : STEP_LABELS[step]}
          </div>
        </header>

        <main className="min-h-0 flex-1">
            <div
              key={step}
              className={`${hasNavigated ? `setup-page-transition setup-page-${direction}` : ""} h-full`}
            >
              {step === "welcome" && <SetupWelcome />}

              {step === "provider" && <SetupProvider />}

              {step === "model" && <SetupModel />}

              {step === "complete" && <SetupComplete />}
            </div>
        </main>
      </div>
    </div>
  );
}
