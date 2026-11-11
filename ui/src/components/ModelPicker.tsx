import { Box, Text, useInput } from "ink";
import { useEffect, useMemo, useState } from "react";

import type { SupportedModel, SupportedProvider } from "../protocol";
import { theme } from "../theme";

type Props = {
  providers: SupportedProvider[];
  scope: "global" | "session";
  models: SupportedModel[];
  stage: "providers" | "models";
  loading: boolean;
  error: string | null;
  savedApiKeys?: Record<string, string>;
  onSelectProvider: (provider: SupportedProvider, apiKey?: string) => void;
  onChooseModel: (model: string, provider?: SupportedProvider, save?: boolean, apiKey?: string | null) => void;
  onClose: () => void;
};

type CustomStage = "name" | "endpoint" | "key" | "model" | "save" | "providerKey";

export function ModelPicker({ providers, models, stage, loading, error, savedApiKeys = {}, scope, onSelectProvider, onChooseModel, onClose }: Props) {
  const [index, setIndex] = useState(0);
  const [customStage, setCustomStage] = useState<CustomStage | null>(null);
  const [name, setName] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [save, setSave] = useState(true);
  const [draftProvider, setDraftProvider] = useState<SupportedProvider | null>(null);
  const [pendingProvider, setPendingProvider] = useState<SupportedProvider | null>(null);
  const [search, setSearch] = useState("");

  const entries = useMemo(() => {
    if (stage === "providers") {
      return [...providers.filter((provider) => provider.id !== "custom-openai").map((provider) => ({ label: `${provider.name} — ${provider.description}`, provider })), { label: "Custom provider...", provider: null }];
    }

    const matchingModels = models.filter((item) => `${item.id} ${item.name} ${item.description ?? ""}`.toLowerCase().includes(search.toLowerCase()));
    const emptyMessage = models.length === 0
      ? "No models were returned by this provider"
      : matchingModels.length === 0 ? "No models match this search" : null;

    return [
      ...(emptyMessage ? [{ label: emptyMessage, model: null }] : []),
      ...matchingModels.map((item) => ({ label: item.id === item.name ? item.id : `${item.name} (${item.id})`, model: item.id })),
      { label: "Enter model ID manually...", model: "" },
    ];
  }, [providers, models, stage, search]);
  const visibleEntries = entries.slice(Math.max(0, Math.min(index - 6, entries.length - 12)), Math.max(12, index + 6));

  useEffect(() => { setIndex(0); setSearch(""); }, [stage]);

  useInput((input, key) => {
    if (/^\u001B?\[(?:<\d+;\d+;\d+[Mm]|\d+;\d+R)$/.test(input)) return;
    if (key.escape) { onClose(); return; }
    if (customStage) {
      if (key.backspace || key.delete) {
        if (customStage === "name") setName((value) => value.slice(0, -1));
        if (customStage === "endpoint") setEndpoint((value) => value.slice(0, -1));
        if (customStage === "key") setApiKey((value) => value.slice(0, -1));
        if (customStage === "providerKey") setApiKey((value) => value.slice(0, -1));
        if (customStage === "model") setModel((value) => value.slice(0, -1));
        return;
      }
      if (key.return) {
        if (customStage === "name" && name.trim()) setCustomStage("endpoint");
        else if (customStage === "endpoint" && endpoint.trim()) setCustomStage("key");
        else if (customStage === "providerKey" && pendingProvider) {
          onSelectProvider(pendingProvider, apiKey.trim());
          setApiKey("");
          setPendingProvider(null);
          setCustomStage(null);
        }
        else if (customStage === "key") {
          const provider: SupportedProvider = {
            id: `custom-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
            name: name.trim(),
            description: "Custom OpenAI-compatible API",
            category: "Custom",
            protocol: "OpenAiCompatible",
            endpoint: endpoint.trim().replace(/\/$/, ""),
            requires_api_key: Boolean(apiKey.trim()),
            supports_model_discovery: true,
            api_key_env: null,
          };
          setDraftProvider(provider);
          setCustomStage("model");
        }
        else if (customStage === "model" && model.trim()) {
          if (draftProvider && scope === "global") setCustomStage("save");
          else if (draftProvider) onChooseModel(model.trim(), draftProvider, false, apiKey.trim() || null);
          else onChooseModel(model.trim());
        }
        else if (customStage === "save" && draftProvider) onChooseModel(model.trim(), draftProvider, save, apiKey.trim() || null);
        return;
      }
      if (customStage === "save" && (input.toLowerCase() === "y" || input.toLowerCase() === "n")) setSave(input.toLowerCase() === "y");
      else if (!key.ctrl && !key.meta && input) {
        if (customStage === "name") setName((value) => value + input);
        if (customStage === "endpoint") setEndpoint((value) => value + input);
        if (customStage === "key") setApiKey((value) => value + input);
        if (customStage === "providerKey") setApiKey((value) => value + input);
        if (customStage === "model") setModel((value) => value + input);
      }
      return;
    }
    if (stage === "models" && !key.ctrl && !key.meta && !key.upArrow && !key.downArrow && !key.return && !key.escape) {
      if (key.backspace || key.delete) setSearch((value) => value.slice(0, -1));
      else if (input && input.length === 1) setSearch((value) => value + input);
      setIndex(0);
      return;
    }
    if (key.upArrow) setIndex((value) => Math.max(0, value - 1));
    if (key.downArrow) setIndex((value) => Math.min(entries.length - 1, value + 1));
    if (key.return && !loading) {
      const selected = entries[index];
      if (!selected) return;
      if (stage === "providers") {
        if ("provider" in selected && selected.provider) {
          if (selected.provider.requires_api_key && !savedApiKeys[selected.provider.id]) {
            setPendingProvider(selected.provider);
            setApiKey("");
            setCustomStage("providerKey");
          } else onSelectProvider(selected.provider);
        }
        else setCustomStage("name");
      } else if ("model" in selected && selected.model === null) return;
      else if ("model" in selected && selected.model) onChooseModel(selected.model);
      else setCustomStage("model");
    }
  });

  if (customStage) {
    const current = customStage === "name" ? name : customStage === "endpoint" ? endpoint : customStage === "key" ? apiKey : model;
    const prompt = customStage === "name" ? "Provider name" : customStage === "endpoint" ? "API base URL" : customStage === "key" ? "API key (optional)" : customStage === "providerKey" ? `API key for ${pendingProvider?.name ?? "provider"}` : customStage === "model" ? "Model ID" : "Save this provider for future use? (y/n)";
    return <Box flexDirection="column" borderStyle="round" borderColor={theme.accent} paddingX={1} marginY={1}>
      <Text bold color={theme.accent}>Custom provider</Text>
      <Text>{prompt}</Text>
      <Text color="white">{customStage === "save" ? (save ? "Yes" : "No") : customStage === "key" || customStage === "providerKey" ? "•".repeat(apiKey.length) : current}<Text color={theme.accent}>▌</Text></Text>
      <Text color="gray">Enter to continue · Esc to cancel</Text>
      {customStage === "save" ? <Text color="gray">Press y or n, then Enter</Text> : null}
    </Box>;
  }

  return <Box flexDirection="column" borderStyle="round" borderColor={theme.accent} paddingX={1} marginY={1}>
    <Text bold color={theme.accent}>{scope === "session" ? "Choose a session model" : stage === "providers" ? "Choose a provider" : "Choose a model"}</Text>
    {stage === "models" ? <Text color="gray">Search: {search || "type to filter models"}</Text> : null}
    {loading ? <Text color="gray">Loading…</Text> : visibleEntries.map((entry, visibleIndex) => {
      const itemIndex = Math.max(0, Math.min(index - 6, entries.length - 12)) + visibleIndex;
      return <Text key={entry.label} color={itemIndex === index ? theme.accent : "white"}>{itemIndex === index ? "› " : "  "}{entry.label}</Text>;
    })}
    {error ? <Text color="red">{error}</Text> : null}
    <Text color="gray">{stage === "models" ? "Type to search · ↑/↓ navigate · Enter select · Esc close" : "↑/↓ navigate · Enter select · Esc close"}</Text>
  </Box>;
}
