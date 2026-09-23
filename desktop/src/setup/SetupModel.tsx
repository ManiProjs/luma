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
      <div className="flex h-full items-center justify-center text-[13px] text-zinc-600">
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
          className="text-[11px] font-medium uppercase tracking-[0.14em] text-zinc-600"
        >
          Step 2 of 3
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
          className="mt-1.5 text-[22px] font-semibold tracking-tight text-zinc-100"
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
          className="mt-1 text-[13px] text-zinc-500"
        >
          Choose the model Luma should use with{" "}
          <span className="text-zinc-300">{provider.name}</span>.
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
              className="h-9 min-w-0 flex-1 rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 text-[13px] text-zinc-200 outline-none placeholder:text-zinc-600 transition focus:border-white/[0.14]"
            />

            <motion.button
              type="button"
              whileTap={{
                scale: 0.97,
              }}
              onClick={() => setShowConnection((value) => !value)}
              className="h-9 rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 text-[12px] text-zinc-400 transition hover:bg-white/[0.05] hover:text-zinc-200"
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
                <div className="mt-3 rounded-xl border border-white/[0.06] bg-white/[0.02] p-4">
                  <label className="text-[10px] font-medium uppercase tracking-[0.12em] text-zinc-600">
                    Endpoint
                  </label>

                  <input
                    value={endpoint}
                    onChange={(event) => setEndpoint(event.target.value)}
                    className="mt-1.5 h-9 w-full rounded-lg border border-white/[0.07] bg-black/20 px-3 text-[12px] text-zinc-300 outline-none focus:border-white/[0.14]"
                  />

                  <div className="mt-3">
                    <label className="text-[10px] font-medium uppercase tracking-[0.12em] text-zinc-600">
                      API key
                    </label>

                    <input
                      type="password"
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      placeholder="API key (if required)"
                      className="mt-1.5 h-9 w-full rounded-lg border border-white/[0.07] bg-black/20 px-3 text-[12px] text-zinc-300 outline-none placeholder:text-zinc-700 focus:border-white/[0.14]"
                    />
                  </div>

                  <motion.button
                    type="button"
                    whileTap={{
                      scale: 0.97,
                    }}
                    onClick={() => void rediscover()}
                    disabled={discoveringModels}
                    className="mt-3 h-8 rounded-lg border border-white/[0.07] px-3 text-[11px] text-zinc-400 transition hover:bg-white/[0.05] disabled:opacity-40"
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
                className="mt-3 overflow-hidden rounded-lg border border-amber-500/10 bg-amber-500/[0.04] px-3 py-2.5 text-[12px] text-amber-400"
              >
                {error}
              </motion.div>
            )}
          </AnimatePresence>

          <div className="mt-4 overflow-hidden rounded-xl border border-white/[0.06] bg-white/[0.02]">
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
                  Discovering models...
                </div>
              </motion.div>
            ) : filteredModels.length > 0 ? (
              <motion.div
                variants={modelListVariants}
                initial="hidden"
                animate="show"
                className="max-h-[390px] overflow-y-auto p-2"
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
                          ? "border-white/20 bg-white/[0.08] text-white"
                          : "border-white/[0.06] bg-white/[0.025] text-zinc-100 hover:bg-white/[0.05]"
                      }`}
                    >
                      <div className="flex items-center justify-between gap-4">
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium text-white">
                            {item.name}
                          </div>

                          {item.description && (
                            <div className="mt-1 line-clamp-2 text-xs text-zinc-400">
                              {item.description}
                            </div>
                          )}

                          <div className="mt-2 text-[11px] text-zinc-500">
                            {item.id}
                          </div>
                        </div>

                        {item.context_window > 0 && (
                          <div className="shrink-0 text-[11px] text-zinc-500">
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
                <div className="text-[13px] text-zinc-400">
                  No models discovered
                </div>

                <div className="mt-1 text-[11px] text-zinc-600">
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
            <div className="mb-2 text-[10px] font-medium uppercase tracking-[0.12em] text-zinc-600">
              Manual model
            </div>

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
                className="h-9 min-w-0 flex-1 rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 text-[12px] text-zinc-300 outline-none placeholder:text-zinc-600 focus:border-white/[0.14]"
              />

              <motion.button
                type="button"
                whileTap={{
                  scale: 0.97,
                }}
                onClick={useManualModel}
                disabled={!manualModel.trim()}
                className="h-9 rounded-lg border border-white/[0.07] px-3 text-[11px] text-zinc-400 transition hover:bg-white/[0.05] disabled:cursor-not-allowed disabled:opacity-30"
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
                className="mt-4 rounded-xl border border-white/[0.06] bg-white/[0.02] px-4 py-3"
              >
                <div className="text-[10px] uppercase tracking-[0.12em] text-zinc-600">
                  Selected
                </div>

                <div className="mt-1 text-[13px] font-medium text-zinc-200">
                  {model.name}
                </div>

                <div className="mt-0.5 text-[10px] text-zinc-600">
                  {model.id}
                </div>

                {model.context_window > 0 && (
                  <div className="mt-2 text-[10px] text-zinc-600">
                    {model.context_window.toLocaleString()} token context
                  </div>
                )}
              </motion.div>
            )}
          </AnimatePresence>
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
          className="h-9 rounded-lg bg-zinc-100 px-4 text-[12px] font-medium text-zinc-900 transition hover:bg-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          Review
        </motion.button>
      </div>
    </motion.div>
  );
}
