import { AnimatePresence, motion, type Variants } from "motion/react";

import { useMemo, useState } from "react";

import { useSetupStore } from "../stores/setupStore";

type Filter = "All" | "Local" | "Cloud" | "Custom";

const listVariants: Variants = {
  hidden: {},

  show: {
    transition: {
      staggerChildren: 0.035,
    },
  },
};

const itemVariants: Variants = {
  hidden: {
    opacity: 0,
    y: 8,
  },

  show: {
    opacity: 1,
    y: 0,

    transition: {
      duration: 0.22,
      ease: [0.22, 1, 0.36, 1],
    },
  },
};

export default function SetupProvider() {
  const providers = useSetupStore((state) => state.availableProviders);

  const selectedProvider = useSetupStore((state) => state.provider);

  const discovering = useSetupStore((state) => state.discoveringProviders);

  const error = useSetupStore((state) => state.error);

  const setProvider = useSetupStore((state) => state.setProvider);

  const next = useSetupStore((state) => state.next);

  const back = useSetupStore((state) => state.back);

  const [query, setQuery] = useState("");

  const [filter, setFilter] = useState<Filter>("All");

  const safeProviders = Array.isArray(providers) ? providers : [];

  const filteredProviders = useMemo(() => {
    const normalized = query.trim().toLowerCase();

    return safeProviders.filter((provider) => {
      /*
       * Rust's provider registry is the
       * source of truth.
       *
       * provider_type is the Rust field.
       */
      if (filter !== "All") {
        const type = provider.provider_type.toLowerCase();

        if (type !== filter.toLowerCase()) {
          return false;
        }
      }

      if (!normalized) {
        return true;
      }

      return (
        provider.name.toLowerCase().includes(normalized) ||
        provider.id.toLowerCase().includes(normalized) ||
        provider.description.toLowerCase().includes(normalized) ||
        provider.provider_type.toLowerCase().includes(normalized)
      );
    });
  }, [safeProviders, query, filter]);

  return (
    <motion.div
      initial={{
        opacity: 0,
      }}
      animate={{
        opacity: 1,
      }}
      exit={{
        opacity: 0,
      }}
      className="flex h-full min-h-0 flex-col"
    >
      <div className="border-b border-white/[0.06] px-7 py-5">
        <motion.div
          initial={{
            opacity: 0,
            y: 4,
          }}
          animate={{
            opacity: 1,
            y: 0,
          }}
          transition={{
            delay: 0.04,
          }}
          className="text-[11px] font-medium uppercase tracking-[0.14em] text-zinc-600"
        >
          Step 1 of 3
        </motion.div>

        <motion.h1
          initial={{
            opacity: 0,
            y: 5,
          }}
          animate={{
            opacity: 1,
            y: 0,
          }}
          transition={{
            delay: 0.08,
          }}
          className="mt-1.5 text-[22px] font-semibold tracking-tight text-zinc-100"
        >
          Choose a provider
        </motion.h1>

        <motion.p
          initial={{
            opacity: 0,
            y: 5,
          }}
          animate={{
            opacity: 1,
            y: 0,
          }}
          transition={{
            delay: 0.12,
          }}
          className="mt-1 text-[13px] text-zinc-500"
        >
          Select where Luma should send model requests.
        </motion.p>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 py-5">
        <div className="mx-auto max-w-3xl">
          <motion.div
            initial={{
              opacity: 0,
              y: 6,
            }}
            animate={{
              opacity: 1,
              y: 0,
            }}
            transition={{
              delay: 0.15,
            }}
            className="flex gap-2"
          >
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search providers..."
              className="h-9 min-w-0 flex-1 rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 text-[13px] text-zinc-200 outline-none placeholder:text-zinc-600 transition focus:border-white/[0.14]"
            />

            <select
              value={filter}
              onChange={(event) => setFilter(event.target.value as Filter)}
              className="h-9 rounded-lg border border-white/[0.07] bg-[#0c0d10] px-3 text-[12px] text-zinc-400 outline-none"
            >
              <option value="All">All</option>

              <option value="Local">Local</option>

              <option value="Cloud">Cloud</option>

              <option value="Custom">Custom</option>
            </select>
          </motion.div>

          <AnimatePresence>
            {error && (
              <motion.div
                initial={{
                  opacity: 0,
                  height: 0,
                  y: -5,
                }}
                animate={{
                  opacity: 1,
                  height: "auto",
                  y: 0,
                }}
                exit={{
                  opacity: 0,
                  height: 0,
                  y: -5,
                }}
                className="mt-3 overflow-hidden rounded-lg border border-red-500/10 bg-red-500/[0.04] px-3 py-2.5 text-[12px] text-red-400"
              >
                {error}
              </motion.div>
            )}
          </AnimatePresence>

          {discovering ? (
            <motion.div
              initial={{
                opacity: 0,
              }}
              animate={{
                opacity: 1,
              }}
              className="flex justify-center py-16"
            >
              <div className="flex items-center gap-2 text-[13px] text-zinc-600">
                <motion.div
                  animate={{
                    opacity: [0.25, 1, 0.25],
                  }}
                  transition={{
                    duration: 1.1,
                    repeat: Infinity,
                  }}
                  className="h-1.5 w-1.5 rounded-full bg-zinc-500"
                />
                Loading providers...
              </div>
            </motion.div>
          ) : filteredProviders.length === 0 ? (
            <motion.div
              initial={{
                opacity: 0,
              }}
              animate={{
                opacity: 1,
              }}
              className="py-16 text-center"
            >
              <div className="text-[13px] text-zinc-400">
                No providers found
              </div>

              <div className="mt-1 text-[11px] text-zinc-600">
                Try another search.
              </div>
            </motion.div>
          ) : (
            <motion.div
              variants={listVariants}
              initial="hidden"
              animate="show"
              className="mt-4 grid grid-cols-2 gap-2"
            >
              {filteredProviders.map((provider) => {
                const selected = selectedProvider?.id === provider.id;

                return (
                  <motion.button
                    key={provider.id}
                    variants={itemVariants}
                    whileHover={{
                      y: -1,
                    }}
                    whileTap={{
                      scale: 0.985,
                    }}
                    type="button"
                    onClick={() => setProvider(provider)}
                    className={[
                      "rounded-xl border p-3.5 text-left transition-colors",
                      selected
                        ? "border-white/[0.16] bg-white/[0.07]"
                        : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.10] hover:bg-white/[0.04]",
                    ].join(" ")}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-[13px] font-medium text-zinc-200">
                          {provider.name}
                        </div>

                        <div className="mt-1 text-[11px] leading-4 text-zinc-600">
                          {provider.description}
                        </div>
                      </div>

                      <AnimatePresence>
                        {selected && (
                          <motion.div
                            initial={{
                              opacity: 0,
                              scale: 0.7,
                            }}
                            animate={{
                              opacity: 1,
                              scale: 1,
                            }}
                            exit={{
                              opacity: 0,
                              scale: 0.7,
                            }}
                            className="shrink-0 text-[10px] text-zinc-400"
                          >
                            Selected
                          </motion.div>
                        )}
                      </AnimatePresence>
                    </div>

                    <div className="mt-3 flex items-center gap-2">
                      <span className="rounded-md bg-white/[0.04] px-1.5 py-0.5 text-[9px] text-zinc-500">
                        {provider.provider_type}
                      </span>

                      <span className="truncate rounded-md bg-white/[0.04] px-1.5 py-0.5 text-[9px] text-zinc-600">
                        {provider.endpoint}
                      </span>
                    </div>
                  </motion.button>
                );
              })}
            </motion.div>
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-white/[0.06] px-7 py-4">
        <motion.button
          type="button"
          onClick={back}
          whileHover={{
            x: -2,
          }}
          whileTap={{
            scale: 0.97,
          }}
          className="h-9 rounded-lg px-3 text-[12px] text-zinc-500 transition hover:bg-white/[0.04] hover:text-zinc-300"
        >
          Back
        </motion.button>

        <motion.button
          type="button"
          onClick={next}
          disabled={!selectedProvider}
          whileHover={
            selectedProvider
              ? {
                  y: -1,
                }
              : undefined
          }
          whileTap={
            selectedProvider
              ? {
                  scale: 0.97,
                }
              : undefined
          }
          className="h-9 rounded-lg bg-zinc-100 px-4 text-[12px] font-medium text-zinc-900 transition hover:bg-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          Continue
        </motion.button>
      </div>
    </motion.div>
  );
}
