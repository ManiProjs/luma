/**
 * Canonical TypeScript contract for the Rust core's JSONL protocol.
 * Keep this file aligned with `src/protocol.rs`, `src/event.rs`,
 * `src/session.rs`, `src/provider.rs`, and `src/config/mod.rs`.
 * Both the Ink and Electron clients import these definitions directly.
 */

export const PROTOCOL_VERSION = 2;

export type ProviderProtocol = "OpenAiCompatible" | "Anthropic" | "Gemini";
export type ProviderCategory = "Local" | "Cloud" | "Custom";

export type SupportedProvider = {
  id: string;
  name: string;
  description: string;
  category: ProviderCategory;
  protocol: ProviderProtocol;
  endpoint: string;
  requires_api_key: boolean;
  supports_model_discovery: boolean;
  api_key_env: string | null;
};

export type SupportedModel = {
  id: string;
  name: string;
  description: string | null;
  context_window: number | null;
};

export type ModelConfig = {
  provider: string;
  endpoint: string;
  name: string;
  api_key: string | null;
};

export type ModelConfigInput = Omit<ModelConfig, "api_key"> & {
  api_key?: string | null;
};

export type AppConfig = {
  model: ModelConfig;
  planner: ModelConfig;
  providers: SupportedProvider[];
  provider_api_keys: Record<string, string>;
};

export type AppConfigInput = {
  model: ModelConfigInput;
  planner: ModelConfigInput;
  providers?: SupportedProvider[];
  provider_api_keys?: Record<string, string>;
};

export type SessionInfo = {
  workspace: string;
  provider: string;
  model: string;
  tools: string[];
};

export type SessionMessage = {
  role: "user" | "assistant" | "tool" | "plan" | "preview" | "system" | "error";
  content: string;
  tool: string | null;
  running: boolean;
  succeeded: boolean | null;
  expanded: boolean;
  elapsed_ms: number | null;
};

export type SessionView = {
  info: SessionInfo;
  messages: SessionMessage[];
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

export type ChatSummary = {
  id: string;
  created_at: number;
  updated_at: number;
  workspace: string;
  title: string;
  messages: number;
  summary: string | null;
};

export type ArchivedMessage = SessionMessage;

export type ChatArchive = {
  id: string;
  created_at: number;
  updated_at: number;
  info: SessionInfo;
  messages: ArchivedMessage[];
  summary: string | null;
};

export type ClientMessage =
  | { type: "prompt"; data: { text: string } }
  | { type: "confirm"; data: { allowed: boolean } }
  | { type: "cancel" }
  | { type: "rollback"; data: { index: number } }
  | { type: "toggle_expanded"; data: { index: number } }
  | { type: "get_session" }
  | { type: "get_setup_config"; data: { request_id: string } }
  | { type: "save_setup_config"; data: { request_id: string; config: AppConfigInput } }
  | { type: "get_providers"; data: { request_id: string } }
  | {
      type: "get_models";
      data: {
        request_id: string;
        provider: SupportedProvider;
        api_key: string | null;
        endpoint: string | null;
      };
    }
  | {
      type: "test_provider";
      data: {
        request_id: string;
        provider: SupportedProvider;
        api_key: string | null;
        endpoint: string | null;
        model: string;
      };
    }
  | { type: "set_model"; data: { request_id: string; model: string } }
  | { type: "history_list"; data: { request_id: string } }
  | { type: "history_show"; data: { request_id: string; id: string } }
  | { type: "summary_add"; data: { request_id: string; id: string; summary: string } }
  | { type: "start_dashboard"; data: { request_id: string; host: string; port: number } };

export type AgentEvent =
  | { type: "Status"; data: { state: "idle" | "thinking" | "doing" | "talking" | "waiting" | "error" } }
  | { type: "TextDelta"; data: string }
  | { type: "Thinking" }
  | { type: "PlanGenerated"; data: string }
  | { type: "ChangePreview"; data: { path: string; diff: string } }
  | { type: "ToolStarted"; data: { name: string; input: string } }
  | { type: "ToolFinished"; data: { name: string; duration_ms: number; failed: boolean } }
  | { type: "ConfirmationRequired"; data: { name: string; input: string } }
  | { type: "SystemMessage"; data: string }
  | { type: "Error"; data: string }
  | { type: "Usage"; data: { prompt_tokens: number; completion_tokens: number; total_tokens: number; cost_usd: number } }
  | { type: "Finished" }
  | { type: "ToolProgress"; data: { name: string; output: string; progress_percent: number | null } }
  | { type: "SessionInfo"; data: { workspace: string; model: string; provider: string; tool_count: number; messages_count: number } }
  | { type: "ThinkingContent"; data: string };

export type ServerEvent =
  | { type: "ready"; data: { version: number } }
  | { type: "agent"; data: { event: AgentEvent } }
  | { type: "session"; data: { session: SessionView } }
  | { type: "error"; data: { request_id: string | null; message: string; category: "transient" | "fatal" | "initialization_failed" } }
  | { type: "setup_config"; data: { request_id: string; config: AppConfig } }
  | { type: "setup_saved"; data: { request_id: string } }
  | { type: "providers"; data: { request_id: string; providers: SupportedProvider[] } }
  | { type: "models"; data: { request_id: string; models: SupportedModel[] } }
  | { type: "provider_tested"; data: { request_id: string } }
  | { type: "model_changed"; data: { request_id: string; provider: string; model: string } }
  | { type: "history_list"; data: { request_id: string; chats: ChatSummary[] } }
  | { type: "history_chat"; data: { request_id: string; chat: ChatArchive } }
  | { type: "summary_saved"; data: { request_id: string; id: string } }
  | { type: "dashboard_started"; data: { request_id: string; address: string } };

export function encodeClientMessage(message: ClientMessage): string {
  return `${JSON.stringify(message)}\n`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNullableString(value: unknown): value is string | null {
  return value === null || isString(value);
}

export function isSupportedProvider(value: unknown): value is SupportedProvider {
  if (!isRecord(value)) return false;
  return isString(value.id) && isString(value.name) && isString(value.description)
    && ["Local", "Cloud", "Custom"].includes(String(value.category))
    && ["OpenAiCompatible", "Anthropic", "Gemini"].includes(String(value.protocol))
    && isString(value.endpoint) && typeof value.requires_api_key === "boolean"
    && typeof value.supports_model_discovery === "boolean"
    && isNullableString(value.api_key_env);
}

function isModel(value: unknown): value is SupportedModel {
  if (!isRecord(value)) return false;
  return isString(value.id) && isString(value.name)
    && isNullableString(value.description)
    && (value.context_window === null || isNumber(value.context_window));
}

function isModelConfig(value: unknown): value is ModelConfig {
  if (!isRecord(value)) return false;
  return isString(value.provider) && isString(value.endpoint)
    && isString(value.name) && isNullableString(value.api_key);
}

function isModelConfigInput(value: unknown): value is ModelConfigInput {
  if (!isRecord(value)) return false;
  return isString(value.provider) && isString(value.endpoint)
    && isString(value.name)
    && (value.api_key === undefined || isNullableString(value.api_key));
}

export function isAppConfig(value: unknown): value is AppConfig {
  if (!isRecord(value)) return false;
  return isModelConfig(value.model) && isModelConfig(value.planner)
    && Array.isArray(value.providers) && value.providers.every(isSupportedProvider)
    && isRecord(value.provider_api_keys)
    && Object.values(value.provider_api_keys).every(isString);
}

export function isAppConfigInput(value: unknown): value is AppConfigInput {
  if (!isRecord(value) || !isModelConfigInput(value.model) || !isModelConfigInput(value.planner)) {
    return false;
  }
  const validProviders = value.providers === undefined
    || (Array.isArray(value.providers) && value.providers.every(isSupportedProvider));
  const validKeys = value.provider_api_keys === undefined
    || (isRecord(value.provider_api_keys) && Object.values(value.provider_api_keys).every(isString));
  return validProviders && validKeys;
}

function isAgentEvent(value: unknown): value is AgentEvent {
  if (!isRecord(value) || !isString(value.type)) return false;
  const data = value.data;
  switch (value.type) {
    case "Thinking":
    case "Finished":
      return data === undefined;
    case "TextDelta":
    case "PlanGenerated":
    case "SystemMessage":
    case "Error":
    case "ThinkingContent":
      return isString(data);
    case "Status":
      return isRecord(data) && ["idle", "thinking", "doing", "talking", "waiting", "error"].includes(String(data.state));
    case "ChangePreview":
      return isRecord(data) && isString(data.path) && isString(data.diff);
    case "ToolStarted":
    case "ConfirmationRequired":
      return isRecord(data) && isString(data.name) && isString(data.input);
    case "ToolFinished":
      return isRecord(data) && isString(data.name) && isNumber(data.duration_ms)
        && typeof data.failed === "boolean";
    case "Usage":
      return isRecord(data) && isNumber(data.prompt_tokens)
        && isNumber(data.completion_tokens) && isNumber(data.total_tokens)
        && isNumber(data.cost_usd);
    case "ToolProgress":
      return isRecord(data) && isString(data.name) && isString(data.output)
        && (data.progress_percent === null || isNumber(data.progress_percent));
    case "SessionInfo":
      return isRecord(data) && isString(data.workspace) && isString(data.model)
        && isString(data.provider) && isNumber(data.tool_count)
        && isNumber(data.messages_count);
    default:
      return false;
  }
}

function isSessionView(value: unknown): value is SessionView {
  if (!isRecord(value) || !isRecord(value.info) || !isString(value.info.workspace)
    || !isString(value.info.provider) || !isString(value.info.model)
    || !Array.isArray(value.info.tools) || !value.info.tools.every(isString)
    || !Array.isArray(value.messages) || !value.messages.every((message) => {
      if (!isRecord(message)) return false;
      return ["user", "assistant", "tool", "plan", "preview", "system", "error"].includes(String(message.role))
        && isString(message.content) && isNullableString(message.tool)
        && typeof message.running === "boolean"
        && (message.succeeded === null || typeof message.succeeded === "boolean")
        && typeof message.expanded === "boolean"
        && (message.elapsed_ms === null || isNumber(message.elapsed_ms));
    })) return false;
  const confirmation = value.confirmation;
  const validConfirmation = confirmation === null || (isRecord(confirmation)
    && isString(confirmation.name) && isString(confirmation.input));
  if (!validConfirmation || typeof value.busy !== "boolean" || typeof value.fresh !== "boolean"
    || !isString(value.status) || !isRecord(value.usage) || !isNumber(value.usage.prompt_tokens)
    || !isNumber(value.usage.completion_tokens) || !isNumber(value.usage.total_tokens)
    || !isNumber(value.usage.cost_usd_micros)
    || !(value.elapsed_ms === null || isNumber(value.elapsed_ms))) return false;
  return true;
}

function isChatSummary(value: unknown): value is ChatSummary {
  return isRecord(value) && isString(value.id) && isNumber(value.created_at)
    && isNumber(value.updated_at) && isString(value.workspace) && isString(value.title)
    && isNumber(value.messages) && isNullableString(value.summary);
}

function isChatArchive(value: unknown): value is ChatArchive {
  return isRecord(value) && isString(value.id) && isNumber(value.created_at)
    && isNumber(value.updated_at) && isRecord(value.info)
    && isString(value.info.workspace) && isString(value.info.provider)
    && isString(value.info.model) && Array.isArray(value.info.tools)
    && value.info.tools.every(isString) && Array.isArray(value.messages)
    && value.messages.every((message) => isRecord(message)
      && ["user", "assistant", "tool", "plan", "preview", "system", "error"].includes(String(message.role))
      && isString(message.content) && isNullableString(message.tool)
      && typeof message.running === "boolean"
      && (message.succeeded === null || typeof message.succeeded === "boolean")
      && typeof message.expanded === "boolean"
      && (message.elapsed_ms === null || isNumber(message.elapsed_ms)))
    && isNullableString(value.summary);
}

/** Parse and validate the complete JSONL event payload before either client handles it. */
export function parseServerEvent(line: string): ServerEvent {
  const value: unknown = JSON.parse(line);
  if (!isRecord(value) || !isString(value.type) || !isRecord(value.data)) {
    throw new Error("Core sent a message without a valid protocol envelope.");
  }
  const data = value.data;
  let valid = false;
  switch (value.type) {
    case "ready": valid = isNumber(data.version) && Number.isInteger(data.version); break;
    case "agent": valid = isAgentEvent(data.event); break;
    case "session": valid = isSessionView(data.session); break;
    case "error": valid = isNullableString(data.request_id) && isString(data.message)
      && ["transient", "fatal", "initialization_failed"].includes(String(data.category)); break;
    case "setup_config": valid = isString(data.request_id) && isAppConfig(data.config); break;
    case "setup_saved":
    case "provider_tested": valid = isString(data.request_id); break;
    case "providers": valid = isString(data.request_id) && Array.isArray(data.providers)
      && data.providers.every(isSupportedProvider); break;
    case "models": valid = isString(data.request_id) && Array.isArray(data.models)
      && data.models.every(isModel); break;
    case "model_changed": valid = isString(data.request_id) && isString(data.provider)
      && isString(data.model); break;
    case "history_list": valid = isString(data.request_id) && Array.isArray(data.chats)
      && data.chats.every(isChatSummary); break;
    case "history_chat": valid = isString(data.request_id) && isChatArchive(data.chat); break;
    case "summary_saved": valid = isString(data.request_id) && isString(data.id); break;
    case "dashboard_started": valid = isString(data.request_id) && isString(data.address); break;
    default: throw new Error(`Core sent an unsupported protocol event: ${value.type}.`);
  }
  if (!valid) throw new Error(`Core sent an invalid ${value.type} event payload.`);
  return value as unknown as ServerEvent;
}

export function assertProtocolVersion(version: number): void {
  if (version !== PROTOCOL_VERSION) {
    throw new Error(
      `Core protocol mismatch: client supports v${PROTOCOL_VERSION}, core uses v${version}. Update Luma's core and clients together.`,
    );
  }
}
