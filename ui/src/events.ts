import type { Message, ToolEvent, PlanItem } from "./state";

export type AgentEvent =
  | {
      type: "message:start";
      message: Message;
    }
  | {
      type: "message:delta";
      id: string;
      content: string;
    }
  | {
      type: "message:end";
      id: string;
    }
  | {
      type: "tool:update";
      tool: ToolEvent;
    }
  | {
      type: "plan:update";
      items: PlanItem[];
    }
  | {
      type: "status";
      streaming: boolean;
    }
  | {
      type: "error";
      message: string;
    };
