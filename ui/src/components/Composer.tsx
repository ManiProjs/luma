import { Box, Text, useInput } from "ink";
import type { DOMElement } from "ink";
import { useEffect, useState, type RefObject } from "react";

import { glyphs, theme } from "../theme";
import { PromptHeader } from "./PromptHeader";

import type { Status } from "../state";
import type { ChatAttachment } from "../../../shared/attachments";

type Props = {
  status: Status;
  activeTool: string | null;
  toolCalls: number;
  turns: number;
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onOpenModelPicker: () => void;
  onToggleToolActivity: () => void;
  attachmentTriggerRef: RefObject<DOMElement | null>;
  attachments: ChatAttachment[];
  attachmentMode: boolean;
  attachmentError: string;
  onCancelAttachment: () => void;
  onRemoveAttachment: (index: number) => void;
  confirmation: { name: string; input: string } | null;
  onConfirm: (allowed: boolean) => void;
  disabled?: boolean;
};

const COMMANDS = [
  "/help",
  "/clear",
  "/quit",
  "/init",
  "/model",
  "/tools",
  "/history",
  "/restore",
  "/summary:add",
  "/plan",
  "/resume",
];

export function Composer({
  status,
  activeTool,
  toolCalls,
  turns,
  value,
  onChange,
  onSubmit,
  onOpenModelPicker,
  onToggleToolActivity,
  attachmentTriggerRef,
  attachments,
  attachmentMode,
  attachmentError,
  onCancelAttachment,
  onRemoveAttachment,
  confirmation,
  onConfirm,
  disabled = false,
}: Props) {
  const [cursor, setCursor] = useState(value.length);
  const [selectedSuggestion, setSelectedSuggestion] = useState(0);
  const commandPrefix = value.slice(0, cursor);
  const suggestions = commandPrefix.startsWith("/") && !/\s/.test(commandPrefix)
    ? COMMANDS.filter((command) => command.startsWith(commandPrefix.toLowerCase()))
    : [];

  // Keep the insertion point in sync when the parent clears the prompt after submit.
  useEffect(() => {
    if (value.length === 0) setCursor(0);
  }, [value]);

  useEffect(() => {
    setSelectedSuggestion(0);
  }, [commandPrefix]);

  useInput((input, key) => {
    if (disabled) return;
    if (/^\u001B\[(?:<\d+;\d+;\d+[Mm]|\d+;\d+R)$/.test(input)) return;
    if (attachmentMode && key.escape) {
      onCancelAttachment();
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "x" && attachments.length > 0) {
      onRemoveAttachment(attachments.length - 1);
      return;
    }
    if (confirmation) {
      if (input.toLowerCase() === "y" || key.return) onConfirm(true);
      else if (input.toLowerCase() === "n" || key.escape) onConfirm(false);
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "o") {
      onToggleToolActivity();
      return;
    }
    if (key.ctrl && input.toLowerCase() === "g") {
      onOpenModelPicker();
      return;
    }
    if (key.tab && suggestions.length > 0) {
      const suggestion = suggestions[selectedSuggestion];
      if (!suggestion) return;
      onChange(value.slice(0, cursor - commandPrefix.length) + suggestion + value.slice(cursor));
      setCursor(suggestion.length);
      setSelectedSuggestion(0);
      return;
    }
    if (key.upArrow && suggestions.length > 0) {
      setSelectedSuggestion((index) => (index + suggestions.length - 1) % suggestions.length);
      return;
    }
    if (key.downArrow && suggestions.length > 0) {
      setSelectedSuggestion((index) => (index + 1) % suggestions.length);
      return;
    }
    if (key.return) {
      onSubmit();
      return;
    }

    // Most macOS terminals send DEL (\x7f) for the physical Backspace key.
    // Ink exposes that byte as `delete`, while Ctrl+H is `backspace`.
    // Treat both as backward deletion; the forward Delete key is less common
    // and otherwise makes Backspace appear completely broken at end-of-line.
    if (key.backspace || key.delete) {
      if (cursor > 0) {
        onChange(value.slice(0, cursor - 1) + value.slice(cursor));
        setCursor(cursor - 1);
        setSelectedSuggestion(0);
      }
      return;
    }

    if (key.leftArrow) {
      setCursor((position) => Math.max(0, position - 1));
      return;
    }

    if (key.rightArrow) {
      setCursor((position) => Math.min(value.length, position + 1));
      return;
    }

    if (key.home) {
      setCursor(0);
      return;
    }

    if (key.end) {
      setCursor(value.length);
      return;
    }

    if (!key.ctrl && !key.meta && input) {
      onChange(value.slice(0, cursor) + input + value.slice(cursor));
      setCursor(cursor + input.length);
      setSelectedSuggestion(0);
    }
  }, { isActive: !disabled });

  const beforeCursor = value.slice(0, cursor);
  const afterCursor = value.slice(cursor);

  return (
    <Box flexDirection="column" marginTop={1}>
      {suggestions.length > 0 ? <Box flexDirection="column" marginBottom={1}>
        <Text color={theme.chrome}>COMMANDS  ·  Tab complete  ·  ↑/↓ select</Text>
        {suggestions.slice(0, 6).map((command, index) => <Text key={command} color={index === selectedSuggestion ? theme.accent : theme.muted}>
          {index === selectedSuggestion ? "› " : "  "}{command}
        </Text>)}
      </Box> : null}
      {confirmation ? <Box flexDirection="column" borderStyle="single" borderLeft borderColor={theme.warning} paddingLeft={1} marginBottom={1}>
        <Text color={theme.warning} bold>Approval required  ·  {confirmation.name}</Text>
        {confirmation.name !== "apply_changes" && confirmation.name !== "approve_plan" ? <Text>{confirmation.input}</Text> : null}
        <Text color={theme.muted}>{confirmation.name === "apply_changes" ? "Review the diff above" : confirmation.name === "approve_plan" ? "Review the plan above" : "Review this action"}  ·  Y approve  ·  N reject</Text>
      </Box> : null}
      <PromptHeader status={status} activeTool={activeTool} toolCalls={toolCalls} turns={turns} />

      {attachments.length > 0 ? <Box flexDirection="row" marginTop={1}>
        {attachments.map((attachment, index) => <Box key={`${attachment.name}-${index}`} marginRight={1}>
          <Text color={theme.chrome}>[{attachment.name}]</Text>
        </Box>)}
      </Box> : null}
      {attachmentError ? <Text color={theme.error}>{attachmentError}</Text> : null}

      <Box marginTop={1} flexDirection="row" alignItems="center">
        <Box ref={attachmentTriggerRef} borderStyle="single" borderColor={theme.chrome} paddingX={1} marginRight={1}>
          <Text color={theme.muted}>+</Text>
        </Box>
        {attachmentMode ? <Text color={theme.warning}>Attach workspace file path: </Text> : <Text color={theme.accent} bold>{glyphs.input}{" "}</Text>}

        <Text>
          {beforeCursor}
          <Text color={theme.accent} bold>▏</Text>
          {afterCursor}
        </Text>
      </Box>
      <Text color={theme.chrome}>{attachmentMode ? "Enter attach  ·  Esc cancel  ·  path relative to workspace" : "Enter send  ·  + attach file  ·  Ctrl+X remove last file  ·  Ctrl+G model"}</Text>
    </Box>
  );
}
