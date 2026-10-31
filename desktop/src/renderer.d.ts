import type {
  SetupConfig,
  SetupModel,
  SetupProvider,
} from "./stores/setupStore";
import type { LumaAgentEvent } from "./preload";

declare global {
  interface Window {
    luma: {
      getWorkspace(): Promise<{ path: string }>;
      chooseWorkspace(): Promise<{ path: string; canceled?: boolean }>;
      listWorkspaceFiles(): Promise<import("./App").WorkspaceNode[]>;
      readWorkspaceFile(path: string): Promise<string>;
      writeWorkspaceFile(path: string, content: string): Promise<{ ok: boolean }>;
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

      getSession(): Promise<{ ok: boolean }>;

      onOpenSettings(callback: () => void): () => void;

      onEvent(callback: (event: LumaAgentEvent) => void): () => void;
    };
  }
}

export {};
