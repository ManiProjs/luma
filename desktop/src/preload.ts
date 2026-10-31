import { contextBridge, ipcRenderer } from "electron";
import { parseServerEvent, type ServerEvent } from "../../shared/protocol";

export type LumaAgentEvent = ServerEvent;

contextBridge.exposeInMainWorld("luma", {
  getWorkspace: () => ipcRenderer.invoke("luma:get-workspace"),
  chooseWorkspace: () => ipcRenderer.invoke("luma:choose-workspace"),
  listWorkspaceFiles: () => ipcRenderer.invoke("luma:list-workspace-files"),
  readWorkspaceFile: (path: string) => ipcRenderer.invoke("luma:read-workspace-file", path),
  writeWorkspaceFile: (path: string, content: string) => ipcRenderer.invoke("luma:write-workspace-file", path, content),
  getProviders: () => ipcRenderer.invoke("luma:get-providers"),

  getModels: (
    provider: unknown,
    options?: {
      apiKey?: string;
      endpoint?: string;
    },
  ) => ipcRenderer.invoke("luma:get-models", provider, options),

  getSetupConfig: () => ipcRenderer.invoke("luma:get-setup-config"),

  saveSetupConfig: (config: unknown) =>
    ipcRenderer.invoke("luma:save-setup-config", config),

  prompt: (text: string) => ipcRenderer.invoke("luma:prompt", text),

  confirm: (allowed: boolean) => ipcRenderer.invoke("luma:confirm", allowed),

  cancel: () => ipcRenderer.invoke("luma:cancel"),

  getStatus: () => ipcRenderer.invoke("luma:status"),
  getSession: () => ipcRenderer.invoke("luma:get-session"),

  onOpenSettings: (callback: () => void) => {
    const listener = () => {
      callback();
    };

    ipcRenderer.on("luma:open-settings", listener);

    return () => {
      ipcRenderer.removeListener("luma:open-settings", listener);
    };
  },

  onEvent: (callback: (event: ServerEvent) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: unknown) => {
      callback(parseServerEvent(JSON.stringify(data)));
    };

    ipcRenderer.on("luma:event", listener);

    return () => {
      ipcRenderer.removeListener("luma:event", listener);
    };
  },
});
