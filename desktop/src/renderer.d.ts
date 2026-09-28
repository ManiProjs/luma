import type {
  SetupConfig,
  SetupModel,
  SetupProvider,
} from "./stores/setupStore";

declare global {
  interface Window {
    luma: {
      getProviders(): Promise<SetupProvider[]>;

      getModels(
        provider: SetupProvider,
        options?: {
          apiKey?: string;
          endpoint?: string;
        },
      ): Promise<SetupModel[]>;

      getSetupConfig(): Promise<SetupConfig | null>;

      saveSetupConfig(config: SetupConfig): Promise<{
        ok: boolean;
      }>;

      prompt(text: string): Promise<{
        ok: boolean;
      }>;

      confirm(allowed: boolean): Promise<{
        ok: boolean;
      }>;

      cancel(): Promise<{
        ok: boolean;
      }>;

      getStatus(): Promise<{
        connected: boolean;
      }>;

      onOpenSettings(callback: () => void): () => void;

      onEvent(callback: (event: unknown) => void): () => void;
    };
  }
}

export {};
