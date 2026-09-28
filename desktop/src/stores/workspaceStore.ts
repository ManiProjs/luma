import { create } from "zustand";

import type { AgentMessage } from "../renderer/hooks/useLuma";

export type Chat = {
  id: string;
  title: string;
  projectId: string | null;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
  messages: AgentMessage[];
};

export type Project = {
  id: string;
  name: string;
  createdAt: number;
};

type WorkspaceSnapshot = {
  chats: Chat[];
  projects: Project[];
  activeChatId: string | null;
  activeProjectId: string | null;
  draft: Chat | null;
};

type WorkspaceState = WorkspaceSnapshot & {
  createChat: (projectId?: string | null) => Chat;
  selectChat: (chatId: string) => void;
  renameChat: (chatId: string, title: string) => void;
  togglePin: (chatId: string) => void;
  deleteChat: (chatId: string) => void;
  setChatMessages: (chatId: string, messages: AgentMessage[]) => void;
  createProject: (name: string) => Project;
  selectProject: (projectId: string | null) => void;
  renameProject: (projectId: string, name: string) => void;
  deleteProject: (projectId: string) => void;
};

const STORAGE_KEY = "luma.workspace";

function createId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `id-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function createEmptyChat(projectId: string | null = null): Chat {
  const now = Date.now();

  return {
    id: createId(),
    title: "New chat",
    projectId,
    pinned: false,
    createdAt: now,
    updatedAt: now,
    messages: [],
  };
}

function createDefaultSnapshot(): WorkspaceSnapshot {
  return {
    chats: [],
    projects: [],
    activeChatId: null,
    activeProjectId: null,
    draft: null,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseMessages(value: unknown): AgentMessage[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const messages: AgentMessage[] = [];

  for (const item of value) {
    if (!isRecord(item)) {
      continue;
    }

    if (typeof item.id !== "string") {
      continue;
    }

    if (item.role !== "user" && item.role !== "assistant") {
      continue;
    }

    if (typeof item.content !== "string") {
      continue;
    }

    messages.push({
      id: item.id,
      role: item.role,
      content: item.content,
    });
  }

  return messages;
}

function parseChats(value: unknown): Chat[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const chats: Chat[] = [];

  for (const item of value) {
    if (!isRecord(item)) {
      continue;
    }

    if (typeof item.id !== "string" || typeof item.title !== "string") {
      continue;
    }

    if (typeof item.createdAt !== "number" || typeof item.updatedAt !== "number") {
      continue;
    }

    chats.push({
      id: item.id,
      title: item.title,
      projectId: typeof item.projectId === "string" ? item.projectId : null,
      pinned: item.pinned === true,
      createdAt: item.createdAt,
      updatedAt: item.updatedAt,
      messages: parseMessages(item.messages),
    });
  }

  return chats;
}

function parseProjects(value: unknown): Project[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const projects: Project[] = [];

  for (const item of value) {
    if (!isRecord(item)) {
      continue;
    }

    if (typeof item.id !== "string" || typeof item.name !== "string") {
      continue;
    }

    if (typeof item.createdAt !== "number") {
      continue;
    }

    projects.push({
      id: item.id,
      name: item.name,
      createdAt: item.createdAt,
    });
  }

  return projects;
}

function loadSnapshot(): WorkspaceSnapshot {
  if (typeof window === "undefined") {
    return createDefaultSnapshot();
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);

    if (!raw) {
      return createDefaultSnapshot();
    }

    const parsed = JSON.parse(raw) as unknown;

    if (!isRecord(parsed)) {
      return createDefaultSnapshot();
    }

    const chats = parseChats(parsed.chats);
    const projects = parseProjects(parsed.projects);
    const projectIds = new Set(projects.map((project) => project.id));

    const nextChats = chats
      .map((chat) => ({
        ...chat,
        projectId: chat.projectId && projectIds.has(chat.projectId) ? chat.projectId : null,
      }))
      .filter((chat) => chat.messages.length > 0);

    const activeChatId =
      typeof parsed.activeChatId === "string" &&
      nextChats.some((chat) => chat.id === parsed.activeChatId)
        ? parsed.activeChatId
        : null;

    const activeProjectId =
      typeof parsed.activeProjectId === "string" && projectIds.has(parsed.activeProjectId)
        ? parsed.activeProjectId
        : null;

    return {
      chats: nextChats,
      projects,
      activeChatId,
      activeProjectId,
      draft: null,
    };
  } catch {
    return createDefaultSnapshot();
  }
}

function persist(state: WorkspaceSnapshot) {
  if (typeof window === "undefined") {
    return;
  }

  const snapshot: WorkspaceSnapshot = {
    chats: state.chats,
    projects: state.projects,
    activeChatId: state.activeChatId,
    activeProjectId: state.activeProjectId,
    draft: null,
  };

  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
}

function titleFromMessages(messages: AgentMessage[], fallback: string): string {
  const firstUser = messages.find((message) => message.role === "user");

  if (!firstUser) {
    return fallback;
  }

  const condensed = firstUser.content.replace(/\s+/g, " ").trim();

  if (!condensed) {
    return fallback;
  }

  return condensed.length > 42 ? `${condensed.slice(0, 41)}…` : condensed;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  ...loadSnapshot(),

  createChat: (projectId) => {
    const state = get();
    const resolvedProjectId =
      projectId === undefined ? state.activeProjectId : projectId;
    const chat = createEmptyChat(resolvedProjectId);

    const next: WorkspaceSnapshot = {
      chats: state.chats,
      projects: state.projects,
      activeChatId: chat.id,
      activeProjectId: resolvedProjectId,
      draft: chat,
    };

    persist(next);
    set(next);

    return chat;
  },

  selectChat: (chatId) => {
    const state = get();
    const chat = state.chats.find((item) => item.id === chatId);

    if (!chat) {
      return;
    }

    const next: WorkspaceSnapshot = {
      chats: state.chats,
      projects: state.projects,
      activeChatId: chat.id,
      activeProjectId: chat.projectId,
      draft: null,
    };

    persist(next);
    set(next);
  },

  renameChat: (chatId, title) => {
    const trimmed = title.trim();

    if (!trimmed) {
      return;
    }

    const state = get();

    const next: WorkspaceSnapshot = {
      ...state,
      chats: state.chats.map((chat) =>
        chat.id === chatId
          ? {
              ...chat,
              title: trimmed,
              updatedAt: Date.now(),
            }
          : chat,
      ),
    };

    persist(next);
    set(next);
  },

  togglePin: (chatId) => {
    const state = get();

    const next: WorkspaceSnapshot = {
      ...state,
      chats: state.chats.map((chat) =>
        chat.id === chatId
          ? {
              ...chat,
              pinned: !chat.pinned,
              updatedAt: Date.now(),
            }
          : chat,
      ),
    };

    persist(next);
    set(next);
  },

  deleteChat: (chatId) => {
    const state = get();
    const chats = state.chats.filter((chat) => chat.id !== chatId);

    const next: WorkspaceSnapshot = {
      chats,
      projects: state.projects,
      activeChatId: state.activeChatId === chatId ? null : state.activeChatId,
      activeProjectId: state.activeProjectId,
      draft: state.draft?.id === chatId ? null : state.draft,
    };

    persist(next);
    set(next);
  },

  setChatMessages: (chatId, messages) => {
    const state = get();
    const current = state.chats.find((chat) => chat.id === chatId);

    if (!current) {
      if (messages.length === 0) {
        return;
      }

      const draft =
        state.draft && state.draft.id === chatId
          ? state.draft
          : createEmptyChat(state.activeProjectId);

      const title = titleFromMessages(messages, draft.title);

      const next: WorkspaceSnapshot = {
        chats: [
          {
            ...draft,
            id: chatId,
            title,
            messages,
            updatedAt: Date.now(),
          },
          ...state.chats.filter((chat) => chat.id !== chatId),
        ],
        projects: state.projects,
        activeChatId: chatId,
        activeProjectId: draft.projectId,
        draft: null,
      };

      persist(next);
      set(next);

      return;
    }

    const unchanged =
      current.messages.length === messages.length &&
      current.messages.every(
        (message, index) =>
          message.id === messages[index]?.id &&
          message.content === messages[index]?.content,
      );

    if (unchanged) {
      return;
    }

    const nextTitle =
      current.title === "New chat" ? titleFromMessages(messages, current.title) : current.title;

    const next: WorkspaceSnapshot = {
      ...state,
      chats: state.chats.map((chat) =>
        chat.id === chatId
          ? {
              ...chat,
              title: nextTitle,
              messages,
              updatedAt: Date.now(),
            }
          : chat,
      ),
    };

    persist(next);
    set(next);
  },

  createProject: (name) => {
    const trimmed = name.trim();
    const project: Project = {
      id: createId(),
      name: trimmed || "Untitled project",
      createdAt: Date.now(),
    };

    const state = get();

    const next: WorkspaceSnapshot = {
      chats: state.chats,
      projects: [...state.projects, project],
      activeChatId: state.activeChatId,
      activeProjectId: project.id,
      draft: state.draft,
    };

    persist(next);
    set(next);

    return project;
  },

  selectProject: (projectId) => {
    const state = get();

    if (projectId !== null && !state.projects.some((project) => project.id === projectId)) {
      return;
    }

    const next: WorkspaceSnapshot = {
      chats: state.chats,
      projects: state.projects,
      activeChatId: state.activeChatId,
      activeProjectId: projectId,
      draft: state.draft,
    };

    persist(next);
    set(next);
  },

  renameProject: (projectId, name) => {
    const trimmed = name.trim();

    if (!trimmed) {
      return;
    }

    const state = get();

    const next: WorkspaceSnapshot = {
      ...state,
      projects: state.projects.map((project) =>
        project.id === projectId
          ? {
              ...project,
              name: trimmed,
            }
          : project,
      ),
    };

    persist(next);
    set(next);
  },

  deleteProject: (projectId) => {
    const state = get();

    const next: WorkspaceSnapshot = {
      chats: state.chats.map((chat) =>
        chat.projectId === projectId
          ? {
              ...chat,
              projectId: null,
            }
          : chat,
      ),
      projects: state.projects.filter((project) => project.id !== projectId),
      activeChatId: state.activeChatId,
      activeProjectId: state.activeProjectId === projectId ? null : state.activeProjectId,
      draft:
        state.draft?.projectId === projectId
          ? {
              ...state.draft,
              projectId: null,
            }
          : state.draft,
    };

    persist(next);
    set(next);
  },
}));
