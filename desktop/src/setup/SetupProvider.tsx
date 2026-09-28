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
      <div className="border-b border-[var(--luma-border)] px-7 py-5">
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
          className="luma-kicker"
        >
          Step 2 of 4
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
          className="mt-1.5 text-[22px] font-semibold tracking-tight"
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
          className="mt-1 text-[13px] text-[var(--luma-text-secondary)]"
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
              className="luma-input flex-1"
            />

            <select
              value={filter}
              onChange={(event) => setFilter(event.target.value as Filter)}
              className="luma-input w-[132px]"
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
                className="mt-3 overflow-hidden rounded-lg border border-[var(--luma-danger)]/20 bg-[var(--luma-danger)]/5 px-3 py-2.5 text-[13px] text-[var(--luma-danger)]"
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
              <div className="flex items-center gap-2 text-[13px] text-[var(--luma-text-muted)]">
                <motion.div
                  animate={{
                    opacity: [0.25, 1, 0.25],
                  }}
                  transition={{
                    duration: 1.1,
                    repeat: Infinity,
                  }}
                  className="h-1.5 w-1.5 rounded-full bg-[var(--luma-text-muted)]"
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
              <div className="text-[13px] text-[var(--luma-text-secondary)]">
                No providers found
              </div>

              <div className="mt-1 text-[12px] text-[var(--luma-text-muted)]">
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
                        ? "border-[var(--luma-border-strong)] bg-[var(--luma-surface-hover)]"
                        : "border-[var(--luma-border)] bg-[var(--luma-surface)] hover:bg-[var(--luma-surface-hover)]",
                    ].join(" ")}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-[13px] font-medium">
                          {provider.name}
                        </div>

                        <div className="mt-1 text-[12px] leading-5 text-[var(--luma-text-muted)]">
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
                            className="shrink-0 text-[11px] text-[var(--luma-text-secondary)]"
                          >
                            Selected
                          </motion.div>
                        )}
                      </AnimatePresence>
                    </div>

                    <div className="mt-3 flex items-center gap-2">
                      <span className="rounded-md bg-[var(--luma-surface-raised)] px-1.5 py-0.5 text-[11px] text-[var(--luma-text-secondary)]">
                        {provider.provider_type}
                      </span>

                      <span className="truncate rounded-md bg-[var(--luma-surface-raised)] px-1.5 py-0.5 text-[11px] text-[var(--luma-text-muted)]">
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

      <div className="flex shrink-0 items-center justify-between border-t border-[var(--luma-border)] px-7 py-4">
        <motion.button
          type="button"
          onClick={back}
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
          className="luma-btn-primary"
        >
          Continue
        </motion.button>
      </div>
    </motion.div>
  );
}
