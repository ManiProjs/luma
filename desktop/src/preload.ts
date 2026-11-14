import { contextBridge, ipcRenderer } from "electron";
import { parseServerEvent, type ServerEvent, type AgentConfiguration, type AgentKind, type AgentRecord, type ApprovalRecord, type ChatSummary, type MemoryEntry, type MemoryScope, type ScheduleRecord, type TaskRecord, type RuntimeEvent, type PluginEntry, type InstalledPlugin, type McpServerConfig } from "../../shared/protocol";
import type { ChatAttachment } from "../../shared/attachments";

export type LumaAgentEvent = ServerEvent;

function cleanErrorMessage(reason: unknown): string {
  let message = reason instanceof Error ? reason.message : String(reason);
  for (let pass = 0; pass < 3; pass++) {
    message = message.replace(/^Error invoking remote method '[^']+':\s*/i, "");
    message = message.replace(/^Error occurred in handler for '[^']+':\s*/i, "");
    message = message.replace(/^Error:\s*/i, "");
  }
  message = message.trim();
  return message || "Something went wrong. Please try again.";
}

type IpcInvokeResult = ReturnType<typeof ipcRenderer.invoke>;

function invoke(channel: string, ...args: unknown[]): IpcInvokeResult {
  return ipcRenderer.invoke(channel, ...args).then((result: unknown) => {
    if (typeof result === "object" && result !== null && "__lumaIpcError" in result && result.__lumaIpcError === true) {
      throw new Error(cleanErrorMessage("message" in result ? result.message : "Something went wrong."));
    }
    return result;
  }).catch((reason: unknown) => {
    throw new Error(cleanErrorMessage(reason));
  });
}

contextBridge.exposeInMainWorld("luma", {
  platform: process.platform,
  getWorkspace: () => invoke("luma:get-workspace"),
  getDefaultWorkspace: () => invoke("luma:get-default-workspace"),
  chooseProjectWorkspace: () => invoke("luma:choose-project-workspace"),
  setWorkspace: (path: string) => invoke("luma:set-workspace", path),
  chooseWorkspace: () => invoke("luma:choose-workspace"),
  listWorkspaceFiles: () => invoke("luma:list-workspace-files"),
  getGitChanges: () => invoke("luma:get-git-changes"),
  getGitDiff: (path: string) => invoke("luma:get-git-diff", path),
  stageWorkspaceFile: (path: string) => invoke("luma:stage-workspace-file", path),
  revertWorkspaceFile: (path: string) => invoke("luma:revert-workspace-file", path),
  openWorkspaceTerminal: () => invoke("luma:open-workspace-terminal"),
  startTerminal: (cols: number, rows: number): Promise<{ id: string }> => invoke("luma:terminal-start", cols, rows),
  writeTerminal: (id: string, data: string): Promise<{ ok: boolean }> => invoke("luma:terminal-write", id, data),
  resizeTerminal: (id: string, cols: number, rows: number): Promise<{ ok: boolean }> => invoke("luma:terminal-resize", id, cols, rows),
  closeTerminal: (id: string): Promise<{ ok: boolean }> => invoke("luma:terminal-close", id),
  readWorkspaceFile: (path: string) => invoke("luma:read-workspace-file", path),
  chooseChatAttachments: (): Promise<ChatAttachment[]> => invoke("luma:choose-chat-attachments"),
  writeWorkspaceFile: (path: string, content: string) => invoke("luma:write-workspace-file", path, content),
  getAgents: (): Promise<AgentRecord[]> => invoke("luma:get-agents"),
  createAgent: (input: { name: string; description?: string | null; kind: AgentKind; configuration?: AgentConfiguration }): Promise<AgentRecord> => invoke("luma:create-agent", input),
  deleteAgent: (id: string): Promise<boolean> => invoke("luma:delete-agent", id),
  updateAgent: (input: { id: string; name?: string | null; description?: string | null; configuration?: AgentConfiguration | null }): Promise<AgentRecord> => invoke("luma:update-agent", input),
  setAgentStatus: (id: string, status: "idle" | "paused" | "stopped"): Promise<AgentRecord> => invoke("luma:set-agent-status", id, status),
  getTasks: (agentId?: string): Promise<TaskRecord[]> => invoke("luma:get-tasks", agentId),
  getRuntimeEvents: (agentId: string): Promise<RuntimeEvent[]> => invoke("luma:get-runtime-events", agentId),
  createTask: (input: { agentId: string; request: string; workspace?: string }): Promise<TaskRecord> => invoke("luma:create-task", input),
  cancelTask: (id: string): Promise<TaskRecord> => invoke("luma:cancel-task", id),
  answerTaskQuestion: (id: string, questionId: string, answer: string): Promise<TaskRecord> => invoke("luma:answer-task-question", id, questionId, answer),
  retryTask: (id: string): Promise<TaskRecord> => invoke("luma:retry-task", id),
  renameTask: (id: string, title: string): Promise<TaskRecord> => invoke("luma:rename-task", id, title),
  deleteTask: (id: string): Promise<boolean> => invoke("luma:delete-task", id),
  getSkills: (): Promise<{ name: string; content: string }[]> => invoke("luma:get-skills"),
  createSkill: (name: string, description: string, instructions: string): Promise<{ name: string; content: string }> => invoke("luma:create-skill", name, description, instructions),
  saveSkill: (name: string, content: string): Promise<{ name: string; content: string }> => invoke("luma:save-skill", name, content),
  deleteSkill: (name: string): Promise<{ deleted: boolean }> => invoke("luma:delete-skill", name),
  openExtensionFolder: (kind: "skills" | "plugins" | "mcp"): Promise<{ ok: boolean }> => invoke("luma:open-extension-folder", kind),
  choosePluginPackage: (): Promise<string | null> => invoke("luma:choose-plugin-package"),
  getPlugins: (): Promise<PluginEntry[]> => invoke("luma:get-plugins"),
  installPlugin: (path: string): Promise<InstalledPlugin> => invoke("luma:install-plugin", path),
  setPluginEnabled: (id: string, enabled: boolean): Promise<InstalledPlugin> => invoke("luma:set-plugin-enabled", id, enabled),
  uninstallPlugin: (id: string): Promise<boolean> => invoke("luma:uninstall-plugin", id),
  getMcpServers: (): Promise<McpServerConfig[]> => invoke("luma:get-mcp-servers"),
  addMcpStdio: (input: { id: string; command: string; args: string[]; env: Record<string, string> }): Promise<McpServerConfig> => invoke("luma:add-mcp-stdio", input),
  addMcpHttp: (input: { id: string; url: string; headers: Record<string, string> }): Promise<McpServerConfig> => invoke("luma:add-mcp-http", input),
  setMcpEnabled: (id: string, enabled: boolean): Promise<McpServerConfig> => invoke("luma:set-mcp-enabled", id, enabled),
  updateMcp: (server: McpServerConfig): Promise<McpServerConfig> => invoke("luma:update-mcp", server),
  removeMcp: (id: string): Promise<boolean> => invoke("luma:remove-mcp", id),
  getApprovals: (): Promise<ApprovalRecord[]> => invoke("luma:get-approvals"),
  resolveApproval: (id: string, allowed: boolean): Promise<ApprovalRecord> => invoke("luma:resolve-approval", id, allowed),
  getMemory: (scope: MemoryScope, scopeId: string): Promise<MemoryEntry[]> => invoke("luma:get-memory", scope, scopeId),
  addMemory: (scope: MemoryScope, scopeId: string, content: string): Promise<MemoryEntry> => invoke("luma:add-memory", scope, scopeId, content),
  deleteMemory: (id: string): Promise<boolean> => invoke("luma:delete-memory", id),
  getSchedules: (agentId?: string): Promise<ScheduleRecord[]> => invoke("luma:get-schedules", agentId),
  createSchedule: (input: { agentId: string; title: string; prompt: string; everySeconds: number }): Promise<ScheduleRecord> => invoke("luma:create-schedule", input),
  deleteSchedule: (id: string): Promise<boolean> => invoke("luma:delete-schedule", id),
  setScheduleEnabled: (id: string, enabled: boolean): Promise<ScheduleRecord> => invoke("luma:set-schedule-enabled", id, enabled),
  getProviders: () => invoke("luma:get-providers"),

  getModels: (
    provider: unknown,
    options?: {
      apiKey?: string;
      endpoint?: string;
    },
  ) => invoke("luma:get-models", provider, options),

  getSetupConfig: () => invoke("luma:get-setup-config"),

  saveSetupConfig: (config: unknown) =>
    invoke("luma:save-setup-config", config),

  setSessionModel: (model: unknown) => invoke("luma:set-session-model", model),

  prompt: (text: string) => invoke("luma:prompt", text),

  confirm: (allowed: boolean) => invoke("luma:confirm", allowed),
  answerQuestion: (questionId: string, answer: string) => invoke("luma:answer-question", questionId, answer),

  cancel: () => invoke("luma:cancel"),

  getStatus: () => invoke("luma:status"),
  retryConnection: () => invoke("luma:retry-connection"),
  getCoreBinary: () => invoke("luma:get-core-binary"),
  chooseCoreBinary: (): Promise<string | null> => invoke("luma:choose-core-binary"),
  setCoreBinary: (settings: { mode: "bundled" | "path" | "custom"; customPath: string | null }) => invoke("luma:set-core-binary", settings),
  getGatewayInfo: (): Promise<{ directory: string; endpoint: string; runtimeDatabase: string; logFile: string }> => invoke("luma:get-gateway-info"),
  gatewayCommand: (command: "status" | "start" | "stop" | "restart"): Promise<string> => invoke("luma:gateway-command", command),
  getGatewayLogs: (lines?: number): Promise<string> => invoke("luma:get-gateway-logs", lines),
  openGatewayLog: (): Promise<{ ok: boolean }> => invoke("luma:open-gateway-log"),
  getSession: () => invoke("luma:get-session"),
  getHistory: (): Promise<ChatSummary[]> => invoke("luma:get-history"),
  restoreChat: (id: string) => invoke("luma:restore-chat", id),
  clearSession: () => invoke("luma:clear-session"),
  quit: () => invoke("luma:quit"),

  onOpenSettings: (callback: () => void) => {
    const listener = () => {
      callback();
    };

    ipcRenderer.on("luma:open-settings", listener);

    return () => {
      ipcRenderer.removeListener("luma:open-settings", listener);
    };
  },

  onNewChat: (callback: () => void) => {
    const listener = () => callback();
    ipcRenderer.on("luma:new-chat", listener);
    return () => ipcRenderer.removeListener("luma:new-chat", listener);
  },

  onOpenWorkspace: (callback: () => void) => {
    const listener = () => callback();
    ipcRenderer.on("luma:open-workspace", listener);
    return () => ipcRenderer.removeListener("luma:open-workspace", listener);
  },

  onToggleSidebar: (callback: () => void) => {
    const listener = () => callback();
    ipcRenderer.on("luma:toggle-sidebar", listener);
    return () => ipcRenderer.removeListener("luma:toggle-sidebar", listener);
  },

  onWorkspaceChanged: (callback: (path: string) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: { path: string }) => callback(data.path);
    ipcRenderer.on("luma:workspace-changed", listener);
    return () => ipcRenderer.removeListener("luma:workspace-changed", listener);
  },

  onTitlebarStatus: (callback: (status: { text: string; color: string } | null) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, status: { text: string; color: string } | null) => callback(status);
    ipcRenderer.on("luma:titlebar-status", listener);
    return () => ipcRenderer.removeListener("luma:titlebar-status", listener);
  },

  onTerminalData: (callback: (event: { id: string; data: string }) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, value: { id: string; data: string }) => callback(value);
    ipcRenderer.on("luma:terminal-data", listener);
    return () => ipcRenderer.removeListener("luma:terminal-data", listener);
  },

  onTerminalExit: (callback: (event: { id: string; code: number | null; signal: NodeJS.Signals | null }) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, value: { id: string; code: number | null; signal: NodeJS.Signals | null }) => callback(value);
    ipcRenderer.on("luma:terminal-exit", listener);
    return () => ipcRenderer.removeListener("luma:terminal-exit", listener);
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
