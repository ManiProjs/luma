import { create } from "zustand";

const STORAGE_KEY = "luma.user-name";

function readName(): string {
  try {
    return window.localStorage.getItem(STORAGE_KEY)?.trim() ?? "";
  } catch {
    return "";
  }
}

interface ProfileState {
  name: string;
  setName: (name: string) => void;
}

export const useProfileStore = create<ProfileState>((set) => ({
  name: typeof window === "undefined" ? "" : readName(),
  setName: (name) => {
    const nextName = name.slice(0, 48);
    const persistedName = nextName.trim();
    try {
      if (persistedName) window.localStorage.setItem(STORAGE_KEY, persistedName);
      else window.localStorage.removeItem(STORAGE_KEY);
    } catch {
      // Keep the in-memory preference for this session if storage is unavailable.
    }
    set({ name: nextName });
  },
}));

export function getProfileDisplayName(name: string): string {
  return name.trim() || "User";
}
