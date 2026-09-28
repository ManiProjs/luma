import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type ThemeId =
  | "system"
  | "luma"
  | "luma-light"
  | "midnight"
  | "nord"
  | "dracula"
  | "solarized";

export type Appearance = "light" | "dark";

export type ThemeDefinition = {
  id: ThemeId;
  name: string;
  description: string;
  accent: string;
};

export const themes: ThemeDefinition[] = [
  {
    id: "system",
    name: "Match System",
    description: "Follows the macOS appearance.",
    accent: "#8eb7ff",
  },
  {
    id: "luma",
    name: "Luma Dark",
    description: "The default Luma interface.",
    accent: "#8eb7ff",
  },
  {
    id: "luma-light",
    name: "Luma Light",
    description: "A bright workspace for daylight.",
    accent: "#3b6fd4",
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
  appearance: Appearance;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = "luma.theme";

const THEME_IDS: ThemeId[] = [
  "system",
  "luma",
  "luma-light",
  "midnight",
  "nord",
  "dracula",
  "solarized",
];

function isThemeId(value: string | null): value is ThemeId {
  return THEME_IDS.includes(value as ThemeId);
}

function getInitialTheme(): ThemeId {
  const stored = localStorage.getItem(STORAGE_KEY);

  if (isThemeId(stored)) {
    return stored;
  }

  return "luma";
}

function systemPrefersLight() {
  return window.matchMedia("(prefers-color-scheme: light)").matches;
}

export function resolveTheme(theme: ThemeId): {
  palette: "luma" | "midnight" | "nord" | "dracula" | "solarized";
  appearance: Appearance;
} {
  if (theme === "system") {
    return {
      palette: "luma",
      appearance: systemPrefersLight() ? "light" : "dark",
    };
  }

  if (theme === "luma-light") {
    return {
      palette: "luma",
      appearance: "light",
    };
  }

  return {
    palette: theme,
    appearance: "dark",
  };
}

function applyResolvedTheme(theme: ThemeId) {
  const resolved = resolveTheme(theme);

  document.documentElement.dataset.theme = resolved.palette;
  document.documentElement.dataset.appearance = resolved.appearance;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeId>(getInitialTheme);
  const [appearance, setAppearance] = useState<Appearance>(
    () => resolveTheme(getInitialTheme()).appearance,
  );

  const setTheme = useCallback((nextTheme: ThemeId) => {
    setThemeState(nextTheme);
    localStorage.setItem(STORAGE_KEY, nextTheme);
  }, []);

  useEffect(() => {
    const apply = () => {
      applyResolvedTheme(theme);
      setAppearance(resolveTheme(theme).appearance);
    };

    apply();

    if (theme !== "system") {
      return;
    }

    const media = window.matchMedia("(prefers-color-scheme: light)");
    media.addEventListener("change", apply);

    return () => {
      media.removeEventListener("change", apply);
    };
  }, [theme]);

  const definition = useMemo(
    () => themes.find((item) => item.id === theme) ?? themes[1],
    [theme],
  );

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme,
        definition,
        appearance,
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
