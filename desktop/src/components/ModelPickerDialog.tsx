import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import type { SetupConfig, SetupModel, SetupProvider } from "../stores/setupStore";
import type { AgentRecord } from "../../../shared/protocol";

type Props = {
  scope: "global" | "session" | "agent";
  agent?: AgentRecord;
  draftAgent?: boolean;
  onClose: () => void;
  onChanged: (config: SetupConfig) => void;
};

export default function ModelPickerDialog({ scope, agent, draftAgent = false, onClose, onChanged }: Props) {
  const dialogRef = useFocusTrap(true);
  const [config, setConfig] = useState<SetupConfig | null>(null);
  const [globalModelConfig, setGlobalModelConfig] = useState<SetupConfig["model"] | null>(null);
  const [providers, setProviders] = useState<SetupProvider[]>([]);
  const [provider, setProvider] = useState<SetupProvider | null>(null);
  const [models, setModels] = useState<SetupModel[]>([]);
  const [search, setSearch] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [endpointDraft, setEndpointDraft] = useState("");
  const [manualModel, setManualModel] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const modelRequestId = useRef(0);

  const requestModels = useCallback((target: SetupProvider, options: { apiKey?: string; endpoint?: string }) => {
    const requestId = ++modelRequestId.current;
    setLoading(true);
    setError("");
    void window.luma.getModels(target, options)
      .then((items) => { if (requestId === modelRequestId.current) setModels(items); })
      .catch((reason) => { if (requestId === modelRequestId.current) setError(reason instanceof Error ? reason.message : String(reason)); })
      .finally(() => { if (requestId === modelRequestId.current) setLoading(false); });
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !saving) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose, saving]);

  useEffect(() => {
    let active = true;
    Promise.all([window.luma.getSetupConfig(), window.luma.getProviders()]).then(([saved, available]) => {
      if (!active) return;
      if (!saved) throw new Error("No model configuration is available.");
      const initial = agent ? { ...saved, model: { ...saved.model, provider: agent.configuration.provider ?? saved.model.provider, name: agent.configuration.model ?? saved.model.name, endpoint: agent.configuration.endpoint ?? saved.model.endpoint } } : saved;
      setGlobalModelConfig(saved.model);
      setConfig(initial);
      setProviders(available);
      const current = available.find((item) => item.id === initial.model.provider) ?? available[0] ?? null;
      setProvider(current);
      setEndpointDraft(initial.model.endpoint || current?.endpoint || "");
      setLoading(false);
    }).catch((reason) => {
      if (active) { setError(reason instanceof Error ? reason.message : String(reason)); setLoading(false); }
    });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!provider || !config) return;
    let active = true;
    const savedApiKey = config.provider_api_keys?.[provider.id]
      ?? (config.model.provider === provider.id ? config.model.api_key ?? undefined : undefined);
    const savedProvider = config.providers?.find((item) => item.id === provider.id);
    const endpoint = endpointDraft.trim() || (provider.id === config.model.provider ? config.model.endpoint : savedProvider?.endpoint ?? provider.endpoint);
    const requestId = ++modelRequestId.current;
    setLoading(true);
    setError("");
    void window.luma.getModels(provider, { apiKey: savedApiKey, endpoint })
      .then((items) => { if (active && requestId === modelRequestId.current) setModels(items); })
      .catch((reason) => { if (active && requestId === modelRequestId.current) setError(reason instanceof Error ? reason.message : String(reason)); })
      .finally(() => { if (active && requestId === modelRequestId.current) setLoading(false); });
    return () => { active = false; if (requestId === modelRequestId.current) modelRequestId.current += 1; };
  }, [provider, config]);

  const filtered = useMemo(() => models.filter((item) => `${item.name} ${item.id}`.toLowerCase().includes(search.toLowerCase())), [models, search]);

  async function choose(name: string) {
    if (saving || !config || !provider || !name.trim()) return;
    setSaving(true);
    setError("");
    const selectedApiKey = apiKey || (config.provider_api_keys?.[provider.id]
      ?? (config.model.provider === provider.id ? config.model.api_key ?? null : null));
    const savedProvider = config.providers?.find((item) => item.id === provider.id);
    const endpoint = endpointDraft.trim() || (provider.id === config.model.provider ? config.model.endpoint : savedProvider?.endpoint ?? provider.endpoint);
    const providerKeys = { ...(config.provider_api_keys ?? {}), ...(selectedApiKey ? { [provider.id]: selectedApiKey } : {}) };
    const next: SetupConfig = {
      ...config,
      providers: [...(config.providers ?? []).filter((item) => item.id !== provider.id), { ...provider, endpoint }],
      provider_api_keys: providerKeys,
      model: { provider: provider.id, endpoint, name: name.trim(), api_key: selectedApiKey },
    };
    try {
      if (scope === "agent" && !agent && !draftAgent) throw new Error("Select a teammate before changing its model.");
      if (scope === "global") {
        await window.luma.saveSetupConfig(next);
        onChanged(next);
      } else {
        const providerSettings: SetupConfig = {
          ...config,
          providers: next.providers,
          provider_api_keys: next.provider_api_keys,
          model: globalModelConfig ?? config.model,
        };
        await window.luma.saveSetupConfig(providerSettings);
        if (scope === "session") await window.luma.setSessionModel(next.model);
        else if (agent) await window.luma.updateAgent({ id: agent.id, configuration: { ...agent.configuration, provider: provider.id, model: name.trim(), endpoint } });
        onChanged(scope === "agent" ? next : providerSettings);
      }
      onClose();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setSaving(false); }
  }

  return <div className={`fixed inset-0 ${draftAgent ? "z-[130]" : "z-[100]"} flex items-center justify-center bg-black/55 p-5`} role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !saving) onClose(); }}>
    <section ref={dialogRef as React.RefObject<HTMLElement>} role="dialog" aria-modal="true" aria-labelledby="model-picker-title" className="flex max-h-[calc(100vh-40px)] w-full max-w-[560px] flex-col overflow-hidden rounded-xl border border-[var(--luma-border-strong)] bg-[var(--luma-surface)] shadow-2xl">
      <header className="flex items-center justify-between border-b border-[var(--luma-border)] px-4 py-3"><div><h2 id="model-picker-title" className="text-[14px] font-semibold">Change {scope === "session" ? "session" : scope === "agent" ? `${agent?.name ?? "teammate"} model` : "default"} model</h2><p className="mt-0.5 text-[11px] text-[var(--luma-text-muted)]">{scope === "session" ? "Only this conversation changes." : scope === "agent" ? "This model will be used for this teammate's tasks." : "Choose the model used by new sessions."}</p></div><button type="button" onClick={onClose} disabled={saving} className="rounded px-2 py-1 text-[var(--luma-text-muted)] hover:bg-[var(--luma-surface-hover)] disabled:opacity-40" aria-label="Close">×</button></header>
      <div className="grid h-[420px] min-h-0 max-h-[calc(100vh-180px)] grid-cols-[180px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)]">
        <nav className="min-h-0 overflow-y-auto overscroll-contain border-r border-[var(--luma-border)] p-2" aria-label="Providers">{providers.map((item) => <button key={item.id} type="button" disabled={saving} aria-pressed={provider?.id === item.id} onClick={() => { setProvider(item); setEndpointDraft(item.id === config?.model.provider ? config.model.endpoint : config?.providers?.find((saved) => saved.id === item.id)?.endpoint ?? item.endpoint); setApiKey(""); }} className={`mb-1 block w-full rounded-md px-2.5 py-2 text-left text-[11px] ${provider?.id === item.id ? "bg-[var(--luma-accent-soft)] text-[var(--luma-text)]" : "text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)]"} disabled:opacity-50`}>{item.name}</button>)}</nav>
        <div className="flex min-h-0 min-w-0 flex-col p-3">
          {provider?.requires_api_key && <div className="mb-2 flex gap-1.5"><input aria-label="Provider API key" disabled={saving} value={apiKey} onChange={(event) => setApiKey(event.target.value)} type="password" placeholder="API key (optional if saved)" className="min-w-0 flex-1 rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] px-2.5 py-2 text-[11px] outline-none focus:border-[var(--luma-accent)] disabled:opacity-50" /><button type="button" onClick={() => { if (provider && apiKey.trim()) requestModels(provider, { apiKey: apiKey.trim(), endpoint: endpointDraft.trim() || provider.endpoint }); }} disabled={saving || !apiKey.trim() || loading} className="rounded-md border border-[var(--luma-border)] px-2 text-[10px] disabled:opacity-40">Load</button></div>}
          <div className="mb-2 flex gap-1.5"><input aria-label="Provider API endpoint URL" disabled={saving} value={endpointDraft} onChange={(event) => setEndpointDraft(event.target.value)} placeholder="API endpoint URL" className="min-w-0 flex-1 rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] px-2.5 py-2 text-[10px] outline-none focus:border-[var(--luma-accent)] disabled:opacity-50" /><button type="button" disabled={saving || !provider || loading || !endpointDraft.trim()} onClick={() => { if (!provider || !config) return; const savedKey = apiKey || config.provider_api_keys?.[provider.id] || (config.model.provider === provider.id ? config.model.api_key ?? "" : ""); requestModels(provider, { apiKey: savedKey || undefined, endpoint: endpointDraft.trim() }); }} className="rounded-md border border-[var(--luma-border)] px-2 text-[10px] disabled:opacity-40">Load models</button></div>
          <input aria-label="Search available models" disabled={saving} value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search models…" className="mb-2 rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] px-2.5 py-2 text-[11px] outline-none focus:border-[var(--luma-accent)] disabled:opacity-50" />
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">{loading ? <p className="p-3 text-[11px] text-[var(--luma-text-muted)]">Loading models…</p> : filtered.map((item) => <button key={item.id} type="button" disabled={saving} onClick={() => void choose(item.id)} className="mb-1 block w-full rounded-md px-2.5 py-2 text-left text-[11px] hover:bg-[var(--luma-surface-hover)] disabled:opacity-50"><span className="block font-medium text-[var(--luma-text)]">{item.name}</span>{item.id !== item.name && <span className="mt-0.5 block text-[10px] text-[var(--luma-text-muted)]">{item.id}</span>}</button>)}{!loading && filtered.length === 0 && <p className="p-3 text-[11px] text-[var(--luma-text-muted)]">No matching models. Enter an ID below.</p>}</div>
          <form className="mt-2 flex gap-1.5 border-t border-[var(--luma-border)] pt-2" onSubmit={(event) => { event.preventDefault(); void choose(manualModel); }}><input aria-label="Enter model ID" disabled={saving} value={manualModel} onChange={(event) => setManualModel(event.target.value)} placeholder="Model ID…" className="min-w-0 flex-1 rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] px-2 py-1.5 text-[11px] outline-none focus:border-[var(--luma-accent)] disabled:opacity-50" /><button type="submit" disabled={!manualModel.trim() || saving} className="rounded-md bg-[var(--luma-accent)] px-3 py-1.5 text-[11px] font-medium text-black disabled:opacity-40">Use</button></form>
        </div>
      </div>
      {error && <p role="alert" className="border-t border-[var(--luma-border)] px-4 py-2 text-[11px] text-[var(--luma-danger)]">{error}</p>}
      {saving && <p className="border-t border-[var(--luma-border)] px-4 py-2 text-[10px] text-[var(--luma-text-muted)]">Testing and applying model…</p>}
    </section>
  </div>;
}
