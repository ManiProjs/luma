import { LumaMark } from "../components/LumaWordmark";
import { useSetupStore } from "../stores/setupStore";

export default function SetupWelcome() {
  const next = useSetupStore((state) => state.next);

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1 items-center justify-center px-6">
        <div className="setup-welcome w-full max-w-2xl">
          <div className="mb-7 flex">
            <div
              className="
                flex h-10 w-10 items-center justify-center
                rounded-lg
                border border-[var(--luma-border-strong)]
                bg-[var(--luma-accent-soft)]
                text-[var(--luma-accent)]
              "
            >
              <LumaMark size={22} />
            </div>
          </div>

          <div>
            <div className="luma-kicker">GETTING STARTED / LUMA</div>

            <h1 className="mt-3 text-[34px] font-semibold tracking-[-0.045em]">
              Set up your coding workspace.
            </h1>

            <p className="mt-3 max-w-lg text-[14px] leading-6 text-[var(--luma-text-secondary)]">
              Connect a provider, choose a model, and Luma will be ready to work with your code.
            </p>
          </div>

          <div className="setup-welcome-steps mt-9 grid grid-cols-3 gap-5">
            <Step number="01" title="Provider" description="Connect a compatible API." />
            <Step number="02" title="Model" description="Choose the model Luma uses." />
            <Step number="03" title="Workspace" description="Start working in your project." />
          </div>
        </div>
      </div>

      <div className="flex shrink-0 justify-end border-t border-[var(--luma-border)] px-7 py-4">
        <button type="button" onClick={next} className="luma-btn-primary">
          Get started
        </button>
      </div>
    </div>
  );
}

function Step({
  number,
  title,
  description,
}: {
  number: string;
  title: string;
  description: string;
}) {
  return (
    <div className="setup-welcome-step">
      <div className="font-mono text-[11px] text-[var(--luma-accent)]">{number}</div>
      <div className="mt-2 text-[13px] font-medium">{title}</div>
      <div className="mt-1 text-[12px] leading-5 text-[var(--luma-text-muted)]">{description}</div>
    </div>
  );
}
