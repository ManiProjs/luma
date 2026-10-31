import type { SessionInfo } from "./state";

export type ClientMessage =
  | {
      type: "prompt";
      data: {
        text: string;
      };
    }
  | {
      type: "confirm";
      data: {
        allowed: boolean;
      };
    }
  | {
      type: "cancel";
    }
  | {
      type: "rollback";
      data: {
        index: number;
      };
    }
  | {
      type: "toggle_expanded";
      data: {
        index: number;
      };
    }
  | {
      type: "get_session";
    }
  | {
      type: "set_model";
      data: { request_id: string; model: string };
    }
  | { type: "get_providers"; data: { request_id: string } }
  | { type: "get_setup_config"; data: { request_id: string } }
  | { type: "get_models"; data: { request_id: string; provider: SupportedProvider; api_key: string | null; endpoint: string | null } }
  | { type: "save_setup_config"; data: { request_id: string; config: AppConfig } };

export type SupportedProvider = {
  id: string;
  name: string;
  description: string;
  category: string;
  protocol: string;
  endpoint: string;
  requires_api_key: boolean;
  supports_model_discovery: boolean;
  api_key_env: string | null;
};

export type SupportedModel = { id: string; name: string; description: string | null; context_window: number | null };
export type AppConfig = {
  model: { provider: string; endpoint: string; name: string; api_key: string | null };
  planner: { provider: string; endpoint: string; name: string; api_key: string | null };
  providers?: SupportedProvider[];
  provider_api_keys?: Record<string, string>;
};
export type SessionView = {
  info: SessionInfo;
  messages: Array<{
    role: "user" | "assistant" | "tool" | "plan" | "preview" | "system" | "error";
    content: string;
    tool: string | null;
    running: boolean;
    succeeded: boolean | null;
    expanded: boolean;
    elapsed_ms: number | null;
  }>;
  confirmation: { name: string; input: string } | null;
  busy: boolean;
  fresh: boolean;
  status: string;
  usage: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
    cost_usd_micros: number;
  };
  elapsed_ms: number | null;
};

export type ServerEvent =
  | {
      type: "ready";
      data: {
        version: number;
      };
    }
  | {
      type: "session";
      data: {
        session: SessionView;
      };
    }
  | {
      type: "agent";
      data: unknown;
    }
  | {
      type: "error";
      data: {
        request_id: string | null;
        message: string;
        category: string;
      };
    }
  | {
      type: "model_changed";
      data: { request_id: string; provider: string; model: string };
    }
  | { type: "providers"; data: { request_id: string; providers: SupportedProvider[] } }
  | { type: "models"; data: { request_id: string; models: SupportedModel[] } }
  | { type: "setup_config"; data: { request_id: string; config: AppConfig } }
  | { type: "setup_saved"; data: { request_id: string } }
  | { type: "provider_tested"; data: { request_id: string } };
