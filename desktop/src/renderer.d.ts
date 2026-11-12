import type {
  SetupConfig,
  SetupModel,
  SetupProvider,
} from "./stores/setupStore";
import type { LumaAgentEvent } from "./preload";
import type { AgentConfiguration, AgentKind, AgentRecord, ApprovalRecord, ChatSummary, MemoryEntry, MemoryScope, ScheduleRecord, TaskRecord, RuntimeEvent, PluginEntry, InstalledPlugin, McpServerConfig } from "../../shared/protocol";
import type { ChatAttachment } from "../../shared/attachments";

declare global {
  interface Window {
    luma: {
      platform: NodeJS.Platform;
      getWorkspace(): Promise<{ path: string }>;
      getDefaultWorkspace(): Promise<{ path: string }>;
      chooseProjectWorkspace(): Promise<{ path: string | null; canceled?: boolean }>;
      setWorkspace(path: string): Promise<{ path: string }>;
      chooseWorkspace(): Promise<{ path: string; canceled?: boolean }>;
      listWorkspaceFiles(): Promise<import("./App").WorkspaceNode[]>;
      getGitChanges(): Promise<{ branch: string; files: import("./App").GitChange[] }>;
      getGitDiff(path: string): Promise<string>;
      readWorkspaceFile(path: string): Promise<string>;
      chooseChatAttachments(): Promise<ChatAttachment[]>;
      writeWorkspaceFile(path: string, content: string): Promise<{ ok: boolean }>;
      getAgents(): Promise<AgentRecord[]>;
      createAgent(input: { name: string; description?: string | null; kind: AgentKind; configuration?: AgentConfiguration }): Promise<AgentRecord>;
      deleteAgent(id: string): Promise<boolean>;
      updateAgent(input: { id: string; name?: string | null; description?: string | null; configuration?: AgentConfiguration | null }): Promise<AgentRecord>;
      setAgentStatus(id: string, status: "idle" | "paused" | "stopped"): Promise<AgentRecord>;
      getTasks(agentId?: string): Promise<TaskRecord[]>;
      getRuntimeEvents(agentId: string): Promise<RuntimeEvent[]>;
      createTask(input: { agentId: string; request: string; workspace?: string }): Promise<TaskRecord>;
      cancelTask(id: string): Promise<TaskRecord>;
      retryTask(id: string): Promise<TaskRecord>;
      renameTask(id: string, title: string): Promise<TaskRecord>;
      deleteTask(id: string): Promise<boolean>;
      getSkills(): Promise<{ name: string; content: string }[]>;
      createSkill(name: string, description: string, instructions: string): Promise<{ name: string; content: string }>;
      openExtensionFolder(kind: "skills" | "plugins" | "mcp"): Promise<{ ok: boolean }>;
      choosePluginPackage(): Promise<string | null>;
      getPlugins(): Promise<PluginEntry[]>;
      installPlugin(path: string): Promise<InstalledPlugin>;
      setPluginEnabled(id: string, enabled: boolean): Promise<InstalledPlugin>;
      uninstallPlugin(id: string): Promise<boolean>;
      getMcpServers(): Promise<McpServerConfig[]>;
      addMcpStdio(input: { id: string; command: string; args: string[]; env: Record<string, string> }): Promise<McpServerConfig>;
      addMcpHttp(input: { id: string; url: string; headers: Record<string, string> }): Promise<McpServerConfig>;
      setMcpEnabled(id: string, enabled: boolean): Promise<McpServerConfig>;
      removeMcp(id: string): Promise<boolean>;
      getApprovals(): Promise<ApprovalRecord[]>;
      resolveApproval(id: string, allowed: boolean): Promise<ApprovalRecord>;
      getMemory(scope: MemoryScope, scopeId: string): Promise<MemoryEntry[]>;
      addMemory(scope: MemoryScope, scopeId: string, content: string): Promise<MemoryEntry>;
      deleteMemory(id: string): Promise<boolean>;
      getSchedules(agentId?: string): Promise<ScheduleRecord[]>;
      createSchedule(input: { agentId: string; title: string; prompt: string; everySeconds: number }): Promise<ScheduleRecord>;
      deleteSchedule(id: string): Promise<boolean>;
      getProviders(): Promise<SetupProvider[]>;

      getModels(
        provider: SetupProvider,
        options?: {
          apiKey?: string;
          endpoint?: string;
        },
      ): Promise<SetupModel[]>;

      getSetupConfig(): Promise<SetupConfig | null>;

      saveSetupConfig(config: SetupConfig): Promise<{
        ok: boolean;
      }>;
      setSessionModel(model: SetupConfig["model"]): Promise<{ ok: boolean }>;

      prompt(text: string): Promise<{
        ok: boolean;
      }>;

      confirm(allowed: boolean): Promise<{
        ok: boolean;
      }>;
      answerQuestion(questionId: string, answer: string): Promise<{ ok: boolean }>;

      cancel(): Promise<{
        ok: boolean;
      }>;

      getStatus(): Promise<{
        connected: boolean;
      }>;

      getSession(): Promise<{ ok: boolean }>;
      getHistory(): Promise<ChatSummary[]>;
      restoreChat(id: string): Promise<{ ok: boolean }>;
      clearSession(): Promise<{ ok: boolean }>;
      quit(): Promise<{ ok: boolean }>;

      onOpenSettings(callback: () => void): () => void;
      onNewChat(callback: () => void): () => void;
      onOpenWorkspace(callback: () => void): () => void;
      onToggleSidebar(callback: () => void): () => void;
      onWorkspaceChanged(callback: (path: string) => void): () => void;

      onEvent(callback: (event: LumaAgentEvent) => void): () => void;
    };
  }
}

export {};
