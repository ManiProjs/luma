import type { AgentEvent } from "./events";

type Listener = (event: AgentEvent) => void;

export class EventEmitter {
  private listeners = new Set<Listener>();

  subscribe(listener: Listener) {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  }

  emit(event: AgentEvent) {
    for (const listener of this.listeners) {
      listener(event);
    }
  }
}
