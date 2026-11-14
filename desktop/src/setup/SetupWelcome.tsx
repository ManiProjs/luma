import { LumaMark } from "../components/LumaWordmark";
import { useSetupStore } from "../stores/setupStore";
import { useProfileStore } from "../stores/profileStore";

export default function SetupWelcome() {
  const next = useSetupStore((state) => state.next);
  const name = useProfileStore((state) => state.name);
  const setName = useProfileStore((state) => state.setName);

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
              A workspace that feels like yours.
            </h1>

            <p className="mt-3 max-w-lg text-[14px] leading-6 text-[var(--luma-text-secondary)]">
              Give Luma a name to call you, connect a provider, and choose the model you want to work with.
            </p>
          </div>

          <label className="setup-name-field mt-7 block max-w-md">
            <span className="mb-2 block text-[12px] font-medium text-[var(--luma-text)]">What should Luma call you?</span>
            <input value={name} onChange={(event) => setName(event.target.value)} maxLength={48} placeholder="User" autoComplete="nickname" className="luma-input h-11 w-full px-3 text-[13px]" />
            <span className="mt-1.5 block text-[11px] text-[var(--luma-text-muted)]">You can change this anytime in Settings.</span>
          </label>

          <div className="setup-welcome-steps mt-8 grid grid-cols-3 gap-5">
            <Step number="01" title="Connect" description="Choose your model provider." />
            <Step number="02" title="Choose" description="Pick a model for Luma." />
            <Step number="03" title="Create" description="Start building together." />
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
