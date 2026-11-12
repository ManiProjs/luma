import { contextBridge, ipcRenderer } from "electron";
import { parseServerEvent, type ServerEvent, type AgentConfiguration, type AgentKind, type AgentRecord, type ApprovalRecord, type ChatSummary, type MemoryEntry, type MemoryScope, type ScheduleRecord, type TaskRecord, type RuntimeEvent } from "../../shared/protocol";
import type { ChatAttachment } from "../../shared/attachments";

export type LumaAgentEvent = ServerEvent;

contextBridge.exposeInMainWorld("luma", {
  platform: process.platform,
  getWorkspace: () => ipcRenderer.invoke("luma:get-workspace"),
  getDefaultWorkspace: () => ipcRenderer.invoke("luma:get-default-workspace"),
  chooseProjectWorkspace: () => ipcRenderer.invoke("luma:choose-project-workspace"),
  setWorkspace: (path: string) => ipcRenderer.invoke("luma:set-workspace", path),
  chooseWorkspace: () => ipcRenderer.invoke("luma:choose-workspace"),
  listWorkspaceFiles: () => ipcRenderer.invoke("luma:list-workspace-files"),
  getGitChanges: () => ipcRenderer.invoke("luma:get-git-changes"),
  getGitDiff: (path: string) => ipcRenderer.invoke("luma:get-git-diff", path),
  readWorkspaceFile: (path: string) => ipcRenderer.invoke("luma:read-workspace-file", path),
  chooseChatAttachments: (): Promise<ChatAttachment[]> => ipcRenderer.invoke("luma:choose-chat-attachments"),
  writeWorkspaceFile: (path: string, content: string) => ipcRenderer.invoke("luma:write-workspace-file", path, content),
  getAgents: (): Promise<AgentRecord[]> => ipcRenderer.invoke("luma:get-agents"),
  createAgent: (input: { name: string; description?: string | null; kind: AgentKind; configuration?: AgentConfiguration }): Promise<AgentRecord> => ipcRenderer.invoke("luma:create-agent", input),
  deleteAgent: (id: string): Promise<boolean> => ipcRenderer.invoke("luma:delete-agent", id),
  updateAgent: (input: { id: string; name?: string | null; description?: string | null; configuration?: AgentConfiguration | null }): Promise<AgentRecord> => ipcRenderer.invoke("luma:update-agent", input),
  setAgentStatus: (id: string, status: "idle" | "paused" | "stopped"): Promise<AgentRecord> => ipcRenderer.invoke("luma:set-agent-status", id, status),
  getTasks: (agentId?: string): Promise<TaskRecord[]> => ipcRenderer.invoke("luma:get-tasks", agentId),
  getRuntimeEvents: (agentId: string): Promise<RuntimeEvent[]> => ipcRenderer.invoke("luma:get-runtime-events", agentId),
  createTask: (input: { agentId: string; request: string; workspace?: string }): Promise<TaskRecord> => ipcRenderer.invoke("luma:create-task", input),
  cancelTask: (id: string): Promise<TaskRecord> => ipcRenderer.invoke("luma:cancel-task", id),
  retryTask: (id: string): Promise<TaskRecord> => ipcRenderer.invoke("luma:retry-task", id),
  getApprovals: (): Promise<ApprovalRecord[]> => ipcRenderer.invoke("luma:get-approvals"),
  resolveApproval: (id: string, allowed: boolean): Promise<ApprovalRecord> => ipcRenderer.invoke("luma:resolve-approval", id, allowed),
  getMemory: (scope: MemoryScope, scopeId: string): Promise<MemoryEntry[]> => ipcRenderer.invoke("luma:get-memory", scope, scopeId),
  addMemory: (scope: MemoryScope, scopeId: string, content: string): Promise<MemoryEntry> => ipcRenderer.invoke("luma:add-memory", scope, scopeId, content),
  deleteMemory: (id: string): Promise<boolean> => ipcRenderer.invoke("luma:delete-memory", id),
  getSchedules: (agentId?: string): Promise<ScheduleRecord[]> => ipcRenderer.invoke("luma:get-schedules", agentId),
  createSchedule: (input: { agentId: string; title: string; prompt: string; everySeconds: number }): Promise<ScheduleRecord> => ipcRenderer.invoke("luma:create-schedule", input),
  deleteSchedule: (id: string): Promise<boolean> => ipcRenderer.invoke("luma:delete-schedule", id),
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

  setSessionModel: (model: unknown) => ipcRenderer.invoke("luma:set-session-model", model),

  prompt: (text: string) => ipcRenderer.invoke("luma:prompt", text),

  confirm: (allowed: boolean) => ipcRenderer.invoke("luma:confirm", allowed),
  answerQuestion: (questionId: string, answer: string) => ipcRenderer.invoke("luma:answer-question", questionId, answer),

  cancel: () => ipcRenderer.invoke("luma:cancel"),

  getStatus: () => ipcRenderer.invoke("luma:status"),
  getSession: () => ipcRenderer.invoke("luma:get-session"),
  getHistory: (): Promise<ChatSummary[]> => ipcRenderer.invoke("luma:get-history"),
  restoreChat: (id: string) => ipcRenderer.invoke("luma:restore-chat", id),
  clearSession: () => ipcRenderer.invoke("luma:clear-session"),
  quit: () => ipcRenderer.invoke("luma:quit"),

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
