import { useState } from "react";
import { useInput } from "ink";
import { cycleTheme, getThemeName } from "../theme";

export function useTerminalTheme(enabled = true) {
  const [name, setName] = useState(getThemeName);

  useInput((input, key) => {
    if (!enabled || !key.ctrl || input.toLowerCase() !== "t") return;
    setName(cycleTheme());
  });

  return name;
}
