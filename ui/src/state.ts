export type Role = "user" | "assistant" | "tool" | "plan" | "system" | "error";

export type ToolStatus = "running" | "success" | "failed";

export type Message = {
  id: string;
  role: Role;
  content: string;
  timestamp: number;
};

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

export type UiState = {
  messages: Message[];
  tools: ToolEvent[];
  plan: PlanItem[];

  input: string;

  streaming: boolean;
  waitingForConfirmation: boolean;

  provider: string;
  model: string;
  workspace: string;
};

export const initialState: UiState = {
  messages: [],
  tools: [],
  plan: [],

  input: "",

  streaming: false,
  waitingForConfirmation: false,

  provider: "local",
  model: "unknown",
  workspace: process.cwd(),
};
