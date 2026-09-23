import { useSetupStore } from "../stores/setupStore";

export default function SetupWelcome() {
  const next = useSetupStore((state) => state.next);

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1 items-center justify-center px-6">
        <div className="w-full max-w-xl">
          <div className="mb-8 flex justify-center">
            <div className="flex h-16 w-16 items-center justify-center rounded-2xl border border-white/[0.08] bg-white/[0.03] text-2xl font-semibold text-zinc-100 shadow-2xl shadow-black/20">
              L
            </div>
          </div>

          <div className="text-center">
            <div className="text-[11px] font-medium uppercase tracking-[0.18em] text-zinc-600">
              Luma
            </div>

            <h1 className="mt-2 text-3xl font-semibold tracking-[-0.03em] text-zinc-100">
              Welcome to Luma
            </h1>

            <p className="mx-auto mt-3 max-w-md text-sm leading-6 text-zinc-500">
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

      <div className="flex shrink-0 justify-end border-t border-white/[0.06] px-7 py-4">
        <button
          type="button"
          onClick={next}
          className="h-9 rounded-lg bg-zinc-100 px-4 text-[12px] font-medium text-zinc-900 transition hover:bg-white"
        >
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
    <div className="rounded-xl border border-white/[0.06] bg-white/[0.02] p-3.5">
      <div className="text-[12px] font-medium text-zinc-300">{title}</div>

      <div className="mt-1 text-[11px] leading-4 text-zinc-600">
        {description}
      </div>
    </div>
  );
}
