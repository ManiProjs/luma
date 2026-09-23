import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("luma", {
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

  onEvent: (callback: (event: unknown) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: unknown) => {
      callback(data);
    };

    ipcRenderer.on("luma:event", listener);

    return () => {
      ipcRenderer.removeListener("luma:event", listener);
    };
  },
});
