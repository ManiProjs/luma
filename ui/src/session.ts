import { useEffect, useRef, useState } from "react";

import { startDesktopServer, stopDesktopServer } from "./server";

import { LumaClient } from "./client";

import { initialState, type UiState } from "./state";

import type { AgentEvent, ServerEvent } from "./protocol";
import type { AppConfig, SupportedProvider } from "./protocol";

const client = new LumaClient();

export function useSession(initialSetup = false) {
  const [state, setState] = useState<UiState>(initialState);
  const pendingModel = useRef<{ provider: string; model: string } | null>(null);

  function addMessage(
    role: "user" | "assistant" | "error" | "system",
    content: string,
  ) {
    setState((current) => ({
      ...current,

      messages: [
        ...current.messages,

        {
          id: crypto.randomUUID(),
          role,
          content,
          timestamp: Date.now(),
        },
      ],
    }));
  }

  function handleAgentEvent(event: AgentEvent) {
    setState((current) => {
      const messages = [...current.messages];
      let status = current.status;
      let activeTool = current.activeTool;
      let confirmation = current.confirmation;
      let question = current.question;

      switch (event.type) {
        case "Thinking":
          status = "thinking";
          break;

        case "Status": {
          const state = event.data.state.toLowerCase();
          status = state === "doing" ? "working"
            : state === "talking" ? "streaming"
              : state === "waiting" ? "waiting"
                : state === "error" ? "error"
                  : state === "idle" ? "idle" : "thinking";
          break;
        }

        case "TextDelta": {
          status = "streaming";
          const delta = event.data;

          const lastIndex = messages.length - 1;
          const last = messages[lastIndex];

          if (last && last.role === "assistant") {
            messages[lastIndex] = { ...last, content: last.content + delta };
          } else {
            messages.push({
              id: crypto.randomUUID(),
              role: "assistant",
              content: delta,
              timestamp: Date.now(),
            });
          }
          break;
        }

        case "PlanGenerated":
          messages.push({
            id: crypto.randomUUID(),
            role: "plan",
            content: event.data,
            timestamp: Date.now(),
          });
          status = "thinking";
          break;

        case "ChangePreview":
          messages.push({
            id: crypto.randomUUID(),
            role: "preview",
            content: `${event.data.path}\n${event.data.diff}`,
            timestamp: Date.now(),
          });
          break;

        case "ToolStarted":
          messages.push({
            id: crypto.randomUUID(),
            role: "tool",
            content: `${event.data.name}: ${event.data.input}`,
            timestamp: Date.now(),
          });
          status = "working";
          activeTool = event.data.name;
          break;

        case "ToolFinished": {
          const tool = [...messages].reverse().find((message) => message.role === "tool");
          if (tool) {
            const index = messages.findIndex((message) => message.id === tool.id);
            messages[index] = {
              ...tool,
              content: `${tool.content}\n${event.data.name} — ${event.data.duration_ms} ms${event.data.failed ? " (failed)" : ""}${event.data.diff ? `\n${event.data.diff}` : ""}`,
            };
          }
          status = "thinking";
          activeTool = null;
          break;
        }

        case "ConfirmationRequired":
          status = "waiting";
          confirmation = { name: event.data.name, input: event.data.input };
          break;

        case "QuestionRequired":
          status = "waiting";
          question = event.data;
          break;

        case "SystemMessage":
          messages.push({
            id: crypto.randomUUID(),
            role: "system",
            content: event.data,
            timestamp: Date.now(),
          });
          break;

        case "Error":
          messages.push({
            id: crypto.randomUUID(),
            role: "error",
            content: event.data,
            timestamp: Date.now(),
          });
          status = "error";
          activeTool = null;
          question = null;
          break;

        case "Cancelled":
          status = "done";
          activeTool = null;
          confirmation = null;
          question = null;
          break;

        case "Finished":
          status = "done";
          activeTool = null;
          confirmation = null;
          question = null;
          break;
      }

      return {
        ...current,
        messages,
        status,
        activeTool,
        confirmation,
        question,
      };
    });
  }

  function handleEvent(event: ServerEvent) {
    switch (event.type) {
      case "ready":
        addMessage("system", `Connected to luma-core v${event.data.version}`);
        break;

      case "session":
        setState((current) => ({
          ...current,
          session: event.data.session.info,
          messages: event.data.session.messages.map((message) => ({
            id: crypto.randomUUID(),
            role: message.role,
            content: message.content,
            timestamp: Date.now(),
          })),
          status: event.data.session.busy ? "thinking" : "idle",
          confirmation: event.data.session.confirmation,
          question: event.data.session.question ?? null,
        }));
        break;

      case "agent":
        handleAgentEvent(event.data.event);
        break;

      case "model_changed":
        addMessage("system", `Model changed to ${event.data.model}`);
        setState((current) => ({
          ...current,
          session: {
            ...current.session,
            provider: event.data.provider,
            model: event.data.model,
          },
        }));
        break;

      case "providers":
        setState((current) => current.modelPicker ? ({
          ...current,
          modelPicker: { ...current.modelPicker, providers: event.data.providers },
        }) : current);
        break;

      case "models":
        setState((current) => current.modelPicker ? ({
          ...current,
          modelPicker: { ...current.modelPicker, models: event.data.models, stage: "models", loading: false },
        }) : current);
        break;

      case "setup_config":
        setState((current) => current.modelPicker ? ({
          ...current,
          modelPicker: { ...current.modelPicker, config: event.data.config, loading: false },
        }) : current);
        break;

      case "setup_saved":
        if (pendingModel.current) {
          client.send({ type: "set_model", data: { request_id: crypto.randomUUID(), model: pendingModel.current.model } });
          pendingModel.current = null;
          setState((current) => ({ ...current, modelPicker: null }));
        }
        break;

      case "error":
        addMessage("error", event.data.message);
        setState((current) => ({
          ...current,
          status: "error",
          modelPicker: current.modelPicker ? {
            ...current.modelPicker,
            stage: current.modelPicker.selectedProvider ? "models" : current.modelPicker.stage,
            loading: false,
            error: event.data.message,
          } : null,
        }));
        break;
    }
  }

  function openModelPicker(scope: "global" | "session" = "global") {
    setState((current) => ({
      ...current,
      modelPicker: {
        scope,
        stage: "providers",
        providers: [],
        models: [],
        selectedProvider: null,
        selectedApiKey: null,
        config: null,
        loading: true,
        error: null,
      },
    }));
    try {
      client.send({ type: "get_providers", data: { request_id: crypto.randomUUID() } });
      client.send({ type: "get_setup_config", data: { request_id: crypto.randomUUID() } });
    } catch (error) {
      addMessage("error", String(error));
      setState((current) => ({
        ...current,
        modelPicker: current.modelPicker ? {
          ...current.modelPicker,
          loading: false,
          error: String(error),
        } : null,
      }));
    }
  }

  useEffect(() => {
    let mounted = true;
    const unsubscribe = client.subscribe((event) => {
      if (mounted) handleEvent(event);
    });

    try {
      startDesktopServer();
      void client.connect().then(() => {
        if (mounted) {
          client.send({ type: "get_session" });
          if (initialSetup) openModelPicker();
        }
      }).catch((error) => {
        if (mounted) addMessage("error", error instanceof Error ? error.message : String(error));
      });
    } catch (error) {
      addMessage("error", String(error));
    }

    return () => {
      mounted = false;
      unsubscribe();
      client.disconnect();
      stopDesktopServer();
    };
  }, []);

  return {
    state,

    send(text: string) {
      if (!text.trim()) {
        return;
      }

      if (text.trim().toLowerCase() === "/cancel") {
        if (["thinking", "working", "streaming", "waiting"].includes(state.status)) {
          client.send({ type: "cancel" });
          setState((current) => ({ ...current, status: "cancelling" }));
        } else {
          addMessage("system", "There is no active request to cancel.");
        }
        return;
      }

      addMessage("user", text);

      const modelCommand = text.match(/^\/model(?:\s+(.+))?$/);
      if (modelCommand) {
        const requestedModel = modelCommand[1]?.trim();
        if (!requestedModel) {
          openModelPicker();
          return;
        }
        if (requestedModel.toLowerCase() === "session") {
          openModelPicker("session");
          return;
        }
        try {
          client.send({
            type: "set_model",
            data: { request_id: crypto.randomUUID(), model: requestedModel },
          });
        } catch (error) {
          addMessage("error", String(error));
          setState((current) => ({ ...current, status: "error" }));
        }
        return;
      }

      setState((current) => ({ ...current, status: "thinking" }));

      try {
        client.send({
          type: "prompt",
          data: { text },
        });
      } catch (error) {
        addMessage("error", String(error));
        setState((current) => ({ ...current, status: "error" }));
      }
    },
    openModelPicker,
    confirm(allowed: boolean) {
      client.send({ type: "confirm", data: { allowed } });
      setState((current) => ({ ...current, confirmation: null, status: "working" }));
    },
    answerQuestion(questionId: string, answer: string) {
      if (!answer.trim()) return;
      client.send({ type: "answer_question", data: { question_id: questionId, answer: answer.trim() } });
    },
    cancel() {
      client.send({ type: "cancel" });
      setState((current) => ({ ...current, status: "cancelling" }));
    },
    selectProvider(provider: SupportedProvider, apiKey?: string) {
      setState((current) => current.modelPicker ? ({
        ...current,
        modelPicker: { ...current.modelPicker, selectedProvider: provider, selectedApiKey: apiKey ?? null, loading: true, error: null },
      }) : current);
      const currentConfig = state.modelPicker?.config;
      const sameProvider = currentConfig?.model.provider === provider.id;
      const savedProvider = currentConfig?.providers?.find((item) => item.id === provider.id);
      client.send({
        type: "get_models",
        data: {
          request_id: crypto.randomUUID(),
          provider,
          api_key: apiKey ?? currentConfig?.provider_api_keys?.[provider.id] ?? (sameProvider ? currentConfig?.model.api_key ?? null : null),
          endpoint: sameProvider ? currentConfig?.model.endpoint ?? provider.endpoint : savedProvider?.endpoint ?? provider.endpoint,
        },
      });
    },
    chooseModel(model: string, providerOverride?: SupportedProvider, saveProvider = true, apiKey?: string | null) {
      const picker = state.modelPicker;
      const provider = providerOverride ?? picker?.selectedProvider;
      const currentConfig = picker?.config;
      if (!provider || !currentConfig) {
        addMessage("error", "Model configuration is not ready yet.");
        return;
      }
      const config: AppConfig = {
        ...currentConfig,
        provider_api_keys: {
          ...(currentConfig.provider_api_keys ?? {}),
          ...(apiKey === undefined ? {} : apiKey ? { [provider.id]: apiKey } : {}),
          ...(currentConfig.model.provider === provider.id && currentConfig.model.api_key
            ? { [provider.id]: currentConfig.model.api_key }
            : {}),
        },
        model: {
          provider: provider.id,
          endpoint: provider.endpoint,
          name: model,
          api_key: apiKey === undefined ? (picker.selectedApiKey ?? currentConfig.provider_api_keys?.[provider.id] ?? (currentConfig.model.provider === provider.id ? currentConfig.model.api_key : null)) : apiKey,
        },
        providers: saveProvider && !(currentConfig.providers ?? []).some((item) => item.id === provider.id)
          ? [...(currentConfig.providers ?? []), provider]
          : (currentConfig.providers ?? []).filter((item) => item.id !== provider.id || saveProvider),
      };
      if (picker.scope === "session") {
        client.send({ type: "set_session_model", data: { request_id: crypto.randomUUID(), model: config.model } });
        setState((current) => ({ ...current, modelPicker: null }));
        return;
      }
      pendingModel.current = { provider: provider.id, model };
      client.send({ type: "save_setup_config", data: { request_id: crypto.randomUUID(), config } });
      setState((current) => ({ ...current, modelPicker: current.modelPicker ? { ...current.modelPicker, loading: true } : null }));
    },
    closeModelPicker() {
      setState((current) => ({ ...current, modelPicker: null }));
    },
  };
}
