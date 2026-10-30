export type Role = "user" | "assistant" | "tool" | "plan" | "system" | "error";

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

export type SessionInfo = {
  workspace: string;
  provider: string;
  model: string;
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
  session: SessionInfo;
  status: Status;
}

export const initialState: UiState = {
  messages: [],

  session: {
    workspace: process.cwd(),
    provider: "unknown",
    model: "unknown",
    contextSize: undefined,
    branch: undefined,
    dirty: false,
  },

  status: "idle",
};
