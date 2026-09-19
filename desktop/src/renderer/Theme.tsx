import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type ThemeId = "luma" | "midnight" | "nord" | "dracula" | "solarized";

export type ThemeDefinition = {
  id: ThemeId;
  name: string;
  description: string;
  accent: string;
};

export const themes: ThemeDefinition[] = [
  {
    id: "luma",
    name: "Luma Dark",
    description: "The default Luma interface.",
    accent: "#8eb7ff",
  },
  {
    id: "midnight",
    name: "Midnight",
    description: "Deep blue-black with cool highlights.",
    accent: "#8b9cff",
  },
  {
    id: "nord",
    name: "Nord",
    description: "Cool blue-gray developer palette.",
    accent: "#88c0d0",
  },
  {
    id: "dracula",
    name: "Dracula",
    description: "Dark purple with vivid accents.",
    accent: "#bd93f9",
  },
  {
    id: "solarized",
    name: "Solarized",
    description: "Warm, muted colors for long sessions.",
    accent: "#268bd2",
  },
];

type ThemeContextValue = {
  theme: ThemeId;
  setTheme: (theme: ThemeId) => void;
  definition: ThemeDefinition;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = "luma.theme";

function getInitialTheme(): ThemeId {
  const stored = localStorage.getItem(STORAGE_KEY);

  if (
    stored === "luma" ||
    stored === "midnight" ||
    stored === "nord" ||
    stored === "dracula" ||
    stored === "solarized"
  ) {
    return stored;
  }

  return "luma";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeId>(getInitialTheme);

  const setTheme = useCallback((nextTheme: ThemeId) => {
    setThemeState(nextTheme);
    localStorage.setItem(STORAGE_KEY, nextTheme);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const definition = useMemo(
    () => themes.find((item) => item.id === theme) ?? themes[0],
    [theme],
  );

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme,
        definition,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);

  if (!context) {
    throw new Error("useTheme must be used inside ThemeProvider");
  }

  return context;
}
