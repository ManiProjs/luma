import { contextBridge, ipcRenderer } from "electron";

export type LumaAgentEvent =
  | {
      type: "Ready";
    }
  | {
      type: "Agent";
      data: {
        type: string;
        data?: unknown;
      };
    }
  | {
      type: "Error";
      data: {
        message: string;
      };
    }
  | {
      type: "ProcessExited";
      data: {
        code: number | null;
        signal: string | null;
      };
    };

const luma = {
  sendPrompt(text: string): Promise<{ ok: true }> {
    return ipcRenderer.invoke("luma:prompt", text);
  },

  confirm(allowed: boolean): Promise<{ ok: true }> {
    return ipcRenderer.invoke("luma:confirm", allowed);
  },

  cancel(): Promise<{ ok: true }> {
    return ipcRenderer.invoke("luma:cancel");
  },

  status(): Promise<{
    connected: boolean;
  }> {
    return ipcRenderer.invoke("luma:status");
  },

  onEvent(callback: (event: LumaAgentEvent) => void) {
    const listener = (
      _event: Electron.IpcRendererEvent,
      payload: LumaAgentEvent,
    ) => {
      callback(payload);
    };

    ipcRenderer.on("luma:event", listener);

    return () => {
      ipcRenderer.removeListener("luma:event", listener);
    };
  },
};

contextBridge.exposeInMainWorld("luma", luma);
