import { AnimatePresence, motion, type Variants } from "motion/react";

import { useEffect, useMemo, useState } from "react";

import { useSetupStore } from "../stores/setupStore";

const modelListVariants: Variants = {
  hidden: {},

  show: {
    transition: {
      staggerChildren: 0.025,
    },
  },
};

const modelItemVariants: Variants = {
  hidden: {
    opacity: 0,
    y: 5,
  },

  show: {
    opacity: 1,
    y: 0,

    transition: {
      duration: 0.2,
    },
  },
};

export default function SetupModel() {
  const provider = useSetupStore((state) => state.provider);

  const apiKey = useSetupStore((state) => state.apiKey);

  const endpoint = useSetupStore((state) => state.endpoint);

  const model = useSetupStore((state) => state.model);

  const availableModels = useSetupStore((state) => state.availableModels);

  const discoveringModels = useSetupStore((state) => state.discoveringModels);

  const error = useSetupStore((state) => state.error);

  const setModel = useSetupStore((state) => state.setModel);

  const setApiKey = useSetupStore((state) => state.setApiKey);

  const setEndpoint = useSetupStore((state) => state.setEndpoint);

  const loadModelsForProvider = useSetupStore(
    (state) => state.loadModelsForProvider,
  );

  const next = useSetupStore((state) => state.next);

  const back = useSetupStore((state) => state.back);

  const [search, setSearch] = useState("");

  const [manualModel, setManualModel] = useState("");

  const [showConnection, setShowConnection] = useState(false);

  useEffect(() => {
    if (!provider) {
      return;
    }

    let cancelled = false;

    async function discover() {
      try {
        if (cancelled) {
          return;
        }

        await loadModelsForProvider(provider);
      } catch {
        // The setup store contains
        // the discovery error.
      }
    }

    void discover();

    return () => {
      cancelled = true;
    };
  }, [provider, loadModelsForProvider]);

  const filteredModels = useMemo(() => {
    const query = search.trim().toLowerCase();

    if (!query) {
      return availableModels;
    }

    return availableModels.filter(
      (item) =>
        item.id.toLowerCase().includes(query) ||
        item.name.toLowerCase().includes(query) ||
        item.description?.toLowerCase().includes(query),
    );
  }, [availableModels, search]);

  if (!provider) {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-[var(--luma-text-muted)]">
        No provider selected.
      </div>
    );
  }

  const useManualModel = () => {
    const id = manualModel.trim();

    if (!id) {
      return;
    }

    setModel({
      id,
      name: id,
      description: "Manually specified model",
      context_window: 0,
    });

    setManualModel("");
  };

  const rediscover = async () => {
    try {
      await loadModelsForProvider(provider);
    } catch {
      // The store contains the error.
    }
  };

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
          className="luma-kicker"
        >
          Step 3 of 4
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
            delay: 0.04,
          }}
          className="mt-1.5 text-[22px] font-semibold tracking-tight"
        >
          Choose a model
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
            delay: 0.08,
          }}
          className="mt-1 text-[13px] text-[var(--luma-text-secondary)]"
        >
          Choose the model Luma should use with{" "}
          <span className="text-[var(--luma-text)]">{provider.name}</span>.
        </motion.p>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 py-5">
        <div className="mx-auto max-w-3xl">
          <motion.div
            initial={{
              opacity: 0,
              y: 5,
            }}
            animate={{
              opacity: 1,
              y: 0,
            }}
            transition={{
              delay: 0.1,
            }}
            className="flex gap-2"
          >
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search models..."
              className="luma-input flex-1"
            />

            <motion.button
              type="button"
              whileTap={{
                scale: 0.97,
              }}
              onClick={() => setShowConnection((value) => !value)}
              className="luma-btn-ghost"
              aria-expanded={showConnection}
            >
              Connection
            </motion.button>
          </motion.div>

          <AnimatePresence>
            {showConnection && (
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
                className="overflow-hidden"
              >
                <div className="luma-card mt-3 p-4">
                  <label className="luma-kicker" htmlFor="setup-endpoint">
                    Endpoint
                  </label>

                  <input
                    id="setup-endpoint"
                    value={endpoint}
                    onChange={(event) => setEndpoint(event.target.value)}
                    className="luma-input mt-1.5 w-full"
                  />

                  <div className="mt-3">
                    <label className="luma-kicker" htmlFor="setup-api-key">
                      API key
                    </label>

                    <input
                      id="setup-api-key"
                      type="password"
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      placeholder="API key (if required)"
                      autoComplete="off"
                      className="luma-input mt-1.5 w-full"
                    />
                  </div>

                  <motion.button
                    type="button"
                    whileTap={{
                      scale: 0.97,
                    }}
                    onClick={() => void rediscover()}
                    disabled={discoveringModels}
                    className="luma-btn-ghost mt-3"
                  >
                    {discoveringModels ? "Discovering..." : "Rediscover models"}
                  </motion.button>
                </div>
              </motion.div>
            )}
          </AnimatePresence>

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
                }}
                className="mt-3 overflow-hidden rounded-lg border border-[var(--luma-warning)]/20 bg-[var(--luma-warning)]/8 px-3 py-2.5 text-[13px] text-[var(--luma-warning)]"
              >
                {error}
              </motion.div>
            )}
          </AnimatePresence>

          <div className="luma-card mt-4 overflow-hidden">
            {discoveringModels ? (
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
                  Discovering models...
                </div>
              </motion.div>
            ) : filteredModels.length > 0 ? (
              <motion.div
                variants={modelListVariants}
                initial="hidden"
                animate="show"
                className="max-h-[390px] space-y-1 overflow-y-auto p-2"
              >
                {filteredModels.map((item) => {
                  const selected = model?.id === item.id;

                  return (
                    <motion.button
                      key={item.id}
                      type="button"
                      variants={modelItemVariants}
                      onClick={() => setModel(item)}
                      className={`w-full rounded-xl border p-4 text-left transition-colors ${
                        selected
                          ? "border-[var(--luma-border-strong)] bg-[var(--luma-surface-hover)]"
                          : "border-transparent bg-transparent hover:bg-[var(--luma-surface-hover)]"
                      }`}
                    >
                      <div className="flex items-center justify-between gap-4">
                        <div className="min-w-0">
                          <div className="truncate text-[13px] font-medium">
                            {item.name}
                          </div>

                          {item.description && (
                            <div className="mt-1 line-clamp-2 text-[12px] text-[var(--luma-text-secondary)]">
                              {item.description}
                            </div>
                          )}

                          <div className="mt-2 text-[12px] text-[var(--luma-text-muted)]">
                            {item.id}
                          </div>
                        </div>

                        {item.context_window > 0 && (
                          <div className="shrink-0 text-[12px] text-[var(--luma-text-muted)]">
                            {item.context_window.toLocaleString()} ctx
                          </div>
                        )}
                      </div>
                    </motion.button>
                  );
                })}
              </motion.div>
            ) : (
              <motion.div
                initial={{
                  opacity: 0,
                }}
                animate={{
                  opacity: 1,
                }}
                className="px-5 py-12 text-center"
              >
                <div className="text-[13px] text-[var(--luma-text-secondary)]">
                  No models discovered
                </div>

                <div className="mt-1 text-[12px] text-[var(--luma-text-muted)]">
                  Enter a model ID manually below.
                </div>
              </motion.div>
            )}
          </div>

          <motion.div
            initial={{
              opacity: 0,
              y: 5,
            }}
            animate={{
              opacity: 1,
              y: 0,
            }}
            transition={{
              delay: 0.15,
            }}
            className="mt-4"
          >
            <div className="luma-kicker mb-2">Manual model</div>

            <div className="flex gap-2">
              <input
                value={manualModel}
                onChange={(event) => setManualModel(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    useManualModel();
                  }
                }}
                placeholder="Enter model ID..."
                className="luma-input flex-1"
              />

              <motion.button
                type="button"
                whileTap={{
                  scale: 0.97,
                }}
                onClick={useManualModel}
                disabled={!manualModel.trim()}
                className="luma-btn-ghost"
              >
                Use
              </motion.button>
            </div>
          </motion.div>

          <AnimatePresence>
            {model && (
              <motion.div
                initial={{
                  opacity: 0,
                  y: 8,
                  scale: 0.99,
                }}
                animate={{
                  opacity: 1,
                  y: 0,
                  scale: 1,
                }}
                exit={{
                  opacity: 0,
                  y: -5,
                }}
                className="luma-card mt-4 px-4 py-3"
              >
                <div className="luma-kicker">Selected</div>

                <div className="mt-1 text-[13px] font-medium">{model.name}</div>

                <div className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">
                  {model.id}
                </div>

                {model.context_window > 0 && (
                  <div className="mt-2 text-[12px] text-[var(--luma-text-muted)]">
                    {model.context_window.toLocaleString()} token context
                  </div>
                )}
              </motion.div>
            )}
          </AnimatePresence>
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
          disabled={!model}
          whileHover={
            model
              ? {
                  y: -1,
                }
              : undefined
          }
          whileTap={
            model
              ? {
                  scale: 0.97,
                }
              : undefined
          }
          className="luma-btn-primary"
        >
          Review
        </motion.button>
      </div>
    </motion.div>
  );
}
