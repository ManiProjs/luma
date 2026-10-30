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
    };

export type SessionView = {
  info: SessionInfo;
  messages: Array<{
    role: "user" | "assistant" | "tool" | "plan" | "system" | "error";
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
    };
