export type Role = "user" | "assistant" | "tool" | "plan" | "preview" | "system" | "error";

export type ToolStatus = "running" | "success" | "failed";

export type ToolEvent = {
  id: string;
  name: string;
  status: ToolStatus;
  output?: string;
  timestamp: number;
};

export type PlanItem = {
  id: string;
  text: string;
  status: "pending" | "active" | "done";
};

import type { SessionInfo } from "./protocol";
export type { SessionInfo } from "./protocol";

export type UiSessionInfo = SessionInfo & {
  contextSize?: number;
  branch?: string;
  dirty?: boolean;
};

export interface Message {
  id: string;
  role: Role;
  content: string;
  timestamp: number;
}

export type Status = "idle" | "thinking" | "working" | "streaming" | "waiting" | "done" | "error";

export interface UiState {
  messages: Message[];
  session: UiSessionInfo;
  status: Status;
  activeTool: string | null;
  confirmation: { name: string; input: string } | null;
  question: { question_id: string; prompt: string; choices: string[] } | null;
  modelPicker: {
    scope: "global" | "session";
    stage: "providers" | "models";
    providers: import("./protocol").SupportedProvider[];
    models: import("./protocol").SupportedModel[];
    selectedProvider: import("./protocol").SupportedProvider | null;
    selectedApiKey: string | null;
    config: import("./protocol").AppConfig | null;
    loading: boolean;
    error: string | null;
  } | null;
}

export const initialState: UiState = {
  messages: [],

  session: {
    workspace: process.cwd(),
    provider: "unknown",
    model: "unknown",
    tools: [],
  },

  status: "idle",
  activeTool: null,
  confirmation: null,
  question: null,
  modelPicker: null,
};
