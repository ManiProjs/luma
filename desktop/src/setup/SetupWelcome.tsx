import { SparkIcon } from "../components/LumaWordmark";
import { useSetupStore } from "../stores/setupStore";

export default function SetupWelcome() {
  const next = useSetupStore((state) => state.next);

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1 items-center justify-center px-6">
        <div className="w-full max-w-xl">
          <div className="mb-8 flex justify-center">
            <div
              className="
                flex h-16 w-16 items-center justify-center
                rounded-2xl
                border border-[var(--luma-border-strong)]
                bg-[var(--luma-accent-soft)]
                text-[var(--luma-accent)]
              "
            >
              <SparkIcon size={28} />
            </div>
          </div>

          <div className="text-center">
            <div className="luma-kicker">Luma</div>

            <h1 className="mt-2 text-[28px] font-semibold tracking-[-0.03em]">
              Welcome to Luma
            </h1>

            <p className="mx-auto mt-3 max-w-md text-[13px] leading-6 text-[var(--luma-text-secondary)]">
              Connect Luma to an AI provider and choose the model you want to
              use for your coding workspace.
            </p>
          </div>

          <div className="mt-8 grid grid-cols-3 gap-2">
            <Feature title="Providers" description="Cloud and local APIs" />

            <Feature title="Models" description="Discover or enter manually" />

            <Feature title="Agent" description="Ready for coding" />
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

function Feature({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="luma-card p-3.5">
      <div className="text-[13px] font-medium">{title}</div>

      <div className="mt-1 text-[12px] leading-5 text-[var(--luma-text-muted)]">
        {description}
      </div>
    </div>
  );
}
