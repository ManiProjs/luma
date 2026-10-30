import { EventEmitter } from "../emitter";
import type { Message } from "../state";

export class Agent {
  constructor(private events: EventEmitter) {}

  async run(prompt: string) {
    const user: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: prompt,
      timestamp: Date.now(),
    };

    this.events.emit({
      type: "message:start",
      message: user,
    });

    this.events.emit({
      type: "status",
      streaming: true,
    });

    const assistantId = crypto.randomUUID();

    this.events.emit({
      type: "message:start",
      message: {
        id: assistantId,
        role: "assistant",
        content: "",
        timestamp: Date.now(),
      },
    });

    const response =
      "Luma agent loop is running. Provider integration comes next.";

    for (const chunk of response.split(" ")) {
      this.events.emit({
        type: "message:delta",
        id: assistantId,
        content: chunk + " ",
      });

      await sleep(60);
    }

    this.events.emit({
      type: "message:end",
      id: assistantId,
    });

    this.events.emit({
      type: "status",
      streaming: false,
    });
  }
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
