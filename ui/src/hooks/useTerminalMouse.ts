import { measureElement, useInput, useStdin, useStdout } from "ink";
import type { DOMElement } from "ink";
import { useEffect, useRef } from "react";

export type TerminalMouseEvent = {
  x: number;
  y: number;
  button: number;
  action: "press" | "release" | "wheel";
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
  wheelDirection: "up" | "down" | null;
};

type MouseHandler = (event: TerminalMouseEvent, contains: (element: DOMElement | null) => boolean) => void;

let mouseModeUsers = 0;

/** Enables SGR mouse reporting and translates terminal coordinates into Ink's layout space. */
export function useTerminalMouse(root: { current: DOMElement | null }, onMouse: MouseHandler) {
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const originRow = useRef(0);
  const requestedLayout = useRef("");
  const handler = useRef(onMouse);
  handler.current = onMouse;

  useEffect(() => {
    if (!isRawModeSupported || !stdout.isTTY) return;
    if (mouseModeUsers++ === 0) stdout.write("\u001B[?1000h\u001B[?1006h");
    return () => {
      mouseModeUsers = Math.max(0, mouseModeUsers - 1);
      if (mouseModeUsers === 0) stdout.write("\u001B[?1000l\u001B[?1006l");
    };
  }, [isRawModeSupported, stdout]);

  useEffect(() => {
    // Re-query after mount, resize, or entering the alternate screen buffer.
    if (root.current && stdout.isTTY) {
      const { width, height } = measureElement(root.current);
      const layout = `${width}:${height}:${stdout.columns}:${stdout.rows}`;
      if (requestedLayout.current === layout) return;
      requestedLayout.current = layout;
      stdout.write("\u001B[6n");
    }
  });

  useInput((input) => {
    const cursor = /^\u001B?\[(\d+);(\d+)R$/.exec(input);
    if (cursor) {
      const height = root.current ? measureElement(root.current).height : 0;
      originRow.current = stdout.isTTY && height >= stdout.rows
        ? 0
        : Math.max(0, Number(cursor[1]) - height - 1);
      return;
    }

    const match = /^\u001B?\[<(\d+);(\d+);(\d+)([Mm])$/.exec(input);
    if (!match) return;
    const code = Number(match[1]);
    const event: TerminalMouseEvent = {
      x: Number(match[2]) - 1,
      y: Number(match[3]) - 1 - originRow.current,
      button: code & 3,
      action: (code & 64) !== 0 ? "wheel" : match[4] === "m" ? "release" : "press",
      shift: (code & 4) !== 0,
      alt: (code & 8) !== 0,
      ctrl: (code & 16) !== 0,
      wheelDirection: (code & 64) === 0 ? null : (code & 1) === 0 ? "up" : "down",
    };

    const contains = (element: DOMElement | null) => {
      if (!element) return false;
      let top = 0;
      let left = 0;
      for (let node: DOMElement | undefined = element; node; node = node.parentNode) {
        top += node.yogaNode?.getComputedTop() ?? 0;
        left += node.yogaNode?.getComputedLeft() ?? 0;
      }
      const { width, height } = measureElement(element);
      return event.x >= left && event.x < left + width
        && event.y >= top && event.y < top + height;
    };

    handler.current(event, contains);
  });
}
