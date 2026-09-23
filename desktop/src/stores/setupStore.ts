import { create } from "zustand";

export type SetupStep = "welcome" | "provider" | "model" | "complete";

/**
 * These types mirror Rust's provider.rs.
 *
 * React does NOT define the provider registry.
 * Rust sends the actual providers and models.
 */
export interface SetupProvider {
  id: string;
  provider_type: string;
  name: string;
  description: string;
  endpoint: string;
}

export interface SetupModel {
  id: string;
  name: string;
  description?: string;
  context_window: number;
}

export interface SetupConfig {
  model: {
    provider: string;
    endpoint: string;
    name: string;
    api_key?: string;
  };

  planner: {
    provider: string;
    endpoint: string;
    name: string;
    api_key?: string;
  };
}

interface SetupState {
  step: SetupStep;

  provider: SetupProvider | null;
  model: SetupModel | null;
  plannerModel: SetupModel | null;

  apiKey: string;
  endpoint: string;

  availableProviders: SetupProvider[];
  availableModels: SetupModel[];

  discoveringProviders: boolean;
  discoveringModels: boolean;
  loadingConfiguration: boolean;
  savingConfiguration: boolean;

  error: string | null;
  finished: boolean;

  setStep: (step: SetupStep) => void;

  setProvider: (provider: SetupProvider | null) => void;

  setModel: (model: SetupModel | null) => void;

  setPlannerModel: (model: SetupModel | null) => void;

  setApiKey: (apiKey: string) => void;

  setEndpoint: (endpoint: string) => void;

  setError: (error: string | null) => void;

  loadProviders: () => Promise<void>;

  loadModelsForProvider: (provider?: SetupProvider | null) => Promise<void>;

  loadConfiguration: () => Promise<void>;

  saveConfiguration: () => Promise<void>;

  next: () => void;

  back: () => void;

  reset: () => void;
}

const initialState = {
  step: "welcome" as SetupStep,

  provider: null,
  model: null,
  plannerModel: null,

  apiKey: "",
  endpoint: "",

  availableProviders: [],
  availableModels: [],

  discoveringProviders: false,
  discoveringModels: false,
  loadingConfiguration: false,
  savingConfiguration: false,

  error: null,

  finished: false,
};

export const useSetupStore = create<SetupState>((set, get) => ({
  ...initialState,

  setStep: (step) => {
    set({
      step,
      error: null,
    });
  },

  setProvider: (provider) => {
    set({
      provider,
      model: null,
      plannerModel: null,
      availableModels: [],
      error: null,
      endpoint: provider?.endpoint ?? "",
    });
  },

  setModel: (model) => {
    set({
      model,
      error: null,
    });
  },

  setPlannerModel: (plannerModel) => {
    set({
      plannerModel,
      error: null,
    });
  },

  setApiKey: (apiKey) => {
    set({
      apiKey,
      error: null,
    });
  },

  setEndpoint: (endpoint) => {
    set({
      endpoint,
      error: null,
    });
  },

  setError: (error) => {
    set({ error });
  },

  loadProviders: async () => {
    set({
      discoveringProviders: true,
      error: null,
    });

    try {
      const providers = await window.luma.getProviders();

      if (!Array.isArray(providers)) {
        throw new Error("Luma returned an invalid provider list.");
      }

      set({
        availableProviders: providers,
        discoveringProviders: false,
      });
    } catch (error) {
      set({
        discoveringProviders: false,
        error:
          error instanceof Error ? error.message : "Failed to load providers.",
      });

      throw error;
    }
  },

  loadModelsForProvider: async (provider = get().provider) => {
    if (!provider) {
      set({
        availableModels: [],
        model: null,
        plannerModel: null,
      });

      return;
    }

    const { apiKey, endpoint } = get();

    set({
      discoveringModels: true,
      error: null,
    });

    try {
      const models = await window.luma.getModels(provider, {
        apiKey: apiKey.trim() || undefined,

        endpoint: endpoint.trim() || provider.endpoint || undefined,
      });

      if (!Array.isArray(models)) {
        throw new Error("Luma returned an invalid model list.");
      }

      set({
        availableModels: models,
        discoveringModels: false,
      });
    } catch (error) {
      set({
        discoveringModels: false,
        error:
          error instanceof Error ? error.message : "Failed to load models.",
      });

      throw error;
    }
  },

  loadConfiguration: async () => {
    set({
      loadingConfiguration: true,
      error: null,
    });

    try {
      const config = await window.luma.getSetupConfig();

      if (!config) {
        set({
          loadingConfiguration: false,
        });

        return;
      }

      const providers = get().availableProviders;

      const provider =
        providers.find(
          (item) =>
            item.id === config.model.provider ||
            item.provider_type === config.model.provider ||
            item.name === config.model.provider,
        ) ?? null;

      if (!provider) {
        set({
          loadingConfiguration: false,
        });

        return;
      }

      set({
        provider,
        apiKey: config.model.api_key ?? "",
        endpoint: config.model.endpoint || provider.endpoint || "",
      });

      let models: SetupModel[] = [];

      try {
        models = await window.luma.getModels(provider, {
          apiKey: config.model.api_key || undefined,

          endpoint: config.model.endpoint || provider.endpoint || undefined,
        });
      } catch {
        // The saved configuration can still
        // be restored even if model discovery
        // is temporarily unavailable.
      }

      const savedModel =
        models.find(
          (model) =>
            model.id === config.model.name || model.name === config.model.name,
        ) ?? null;

      const savedPlanner =
        models.find(
          (model) =>
            model.id === config.planner.name ||
            model.name === config.planner.name,
        ) ?? null;

      set({
        availableModels: models,

        model:
          savedModel ??
          (config.model.name
            ? {
                id: config.model.name,
                name: config.model.name,
                description: "Configured model",
                context_window: 0,
              }
            : null),

        plannerModel:
          savedPlanner ??
          (config.planner.name
            ? {
                id: config.planner.name,
                name: config.planner.name,
                description: "Configured planner model",
                context_window: 0,
              }
            : savedModel),
      });

      set({
        loadingConfiguration: false,
      });
    } catch (error) {
      set({
        loadingConfiguration: false,
        error:
          error instanceof Error
            ? error.message
            : "Failed to load configuration.",
      });

      throw error;
    }
  },

  saveConfiguration: async () => {
    const { provider, model, plannerModel, apiKey, endpoint } = get();

    if (!provider) {
      throw new Error("No provider selected.");
    }

    if (!model) {
      throw new Error("No model selected.");
    }

    const planner = plannerModel ?? model;

    set({
      savingConfiguration: true,
      error: null,
    });

    try {
      const config: SetupConfig = {
        model: {
          provider: provider.id,

          endpoint: endpoint.trim() || provider.endpoint,

          name: model.id,

          api_key: apiKey.trim() || undefined,
        },

        planner: {
          provider: provider.id,

          endpoint: endpoint.trim() || provider.endpoint,

          name: planner.id,

          api_key: apiKey.trim() || undefined,
        },
      };

      const result = await window.luma.saveSetupConfig(config);

      if (!result?.ok) {
        throw new Error("Luma failed to save the configuration.");
      }

      set({
        savingConfiguration: false,
        finished: true,
      });
    } catch (error) {
      set({
        savingConfiguration: false,
        error:
          error instanceof Error
            ? error.message
            : "Failed to save configuration.",
      });

      throw error;
    }
  },

  next: () => {
    const { step, provider, model } = get();

    if (step === "welcome") {
      set({
        step: "provider",
        error: null,
      });

      return;
    }

    if (step === "provider") {
      if (!provider) {
        set({
          error: "Select a provider first.",
        });

        return;
      }

      set({
        step: "model",
        error: null,
      });

      return;
    }

    if (step === "model") {
      if (!model) {
        set({
          error: "Select a model first.",
        });

        return;
      }

      set({
        step: "complete",
        error: null,
      });

      return;
    }
  },

  back: () => {
    const { step } = get();

    if (step === "provider") {
      set({
        step: "welcome",
        error: null,
      });

      return;
    }

    if (step === "model") {
      set({
        step: "provider",
        error: null,
      });

      return;
    }

    if (step === "complete") {
      set({
        step: "model",
        error: null,
      });
    }
  },

  reset: () => {
    set({
      ...initialState,
      availableProviders: get().availableProviders,
    });
  },
}));
