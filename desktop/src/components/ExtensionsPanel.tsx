import { useEffect, useState } from "react";
import type { McpServerConfig, PluginEntry } from "../../../shared/protocol";

export default function ExtensionsPanel() {
  const [skills, setSkills] = useState<{ name: string; content: string }[]>([]);
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [servers, setServers] = useState<McpServerConfig[]>([]);
  const [skillName, setSkillName] = useState("");
  const [skillDescription, setSkillDescription] = useState("");
  const [skillInstructions, setSkillInstructions] = useState("");
  const [serverId, setServerId] = useState("");
  const [serverCommand, setServerCommand] = useState("");
  const [serverArgs, setServerArgs] = useState("");
  const [serverEnv, setServerEnv] = useState("{}");
  const [httpUrl, setHttpUrl] = useState("");
  const [httpHeaders, setHttpHeaders] = useState("{}");
  const [transport, setTransport] = useState<"stdio" | "http">("stdio");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    const [nextSkills, nextPlugins, nextServers] = await Promise.all([window.luma.getSkills(), window.luma.getPlugins(), window.luma.getMcpServers()]);
    setSkills(nextSkills); setPlugins(nextPlugins); setServers(nextServers);
  }
  useEffect(() => { void refresh().catch((reason) => setError(reason instanceof Error ? reason.message : String(reason))); }, []);

  async function run(action: () => Promise<unknown>) {
    setBusy(true); setError("");
    try { await action(); await refresh(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  return <div className="space-y-5">
    <div><h2 className="text-[13px] font-medium">Extensions</h2><p className="mt-0.5 text-[11px] text-[var(--luma-text-muted)]">Create Agent Skills, install native plugins, and connect MCP servers. Changes are available to Code and eligible teammates.</p></div>
    {error && <p role="alert" className="rounded border border-[var(--luma-danger)]/30 p-2 text-[11px] text-[var(--luma-danger)]">{error}</p>}
    <section className="border-t border-[var(--luma-border)] pt-3">
      <div className="mb-2 flex items-center justify-between"><div><h3 className="text-[12px] font-semibold">Agent Skills <span className="font-normal text-[var(--luma-text-muted)]">{skills.length}</span></h3><p className="text-[10px] text-[var(--luma-text-muted)]">Reusable instructions loaded when relevant.</p></div><button type="button" onClick={() => void run(() => window.luma.openExtensionFolder("skills"))} className="luma-btn-secondary h-7 px-2 text-[10px]">Open folder</button></div>
      <div className="mb-3 flex flex-wrap gap-1.5">{skills.map((skill) => <span key={skill.name} className="rounded border border-[var(--luma-border)] px-2 py-1 text-[10px]">{skill.name}</span>)}{skills.length === 0 && <span className="text-[10px] text-[var(--luma-text-muted)]">No personal skills yet.</span>}</div>
      <form className="grid gap-2" onSubmit={(event) => { event.preventDefault(); void run(async () => { await window.luma.createSkill(skillName.trim(), skillDescription.trim(), skillInstructions); setSkillName(""); setSkillDescription(""); setSkillInstructions(""); }); }}>
        <div className="grid grid-cols-2 gap-2"><input required pattern="[a-z0-9][a-z0-9-]{0,63}" value={skillName} onChange={(event) => setSkillName(event.target.value)} placeholder="skill-name" className="luma-input h-8 text-[10px]" /><input required value={skillDescription} onChange={(event) => setSkillDescription(event.target.value)} placeholder="What this skill helps with" className="luma-input h-8 text-[10px]" /></div>
        <textarea required rows={3} value={skillInstructions} onChange={(event) => setSkillInstructions(event.target.value)} placeholder="Instructions for the model…" className="luma-input resize-y text-[10px]" />
        <div className="flex justify-end"><button type="submit" disabled={busy || !skillName.trim() || !skillDescription.trim() || !skillInstructions.trim()} className="luma-btn-primary h-7 px-3 text-[10px]">Create skill</button></div>
      </form>
    </section>
    <section className="border-t border-[var(--luma-border)] pt-3">
      <div className="mb-2 flex items-center justify-between"><div><h3 className="text-[12px] font-semibold">Native plugins <span className="font-normal text-[var(--luma-text-muted)]">{plugins.length}</span></h3><p className="text-[10px] text-[var(--luma-text-muted)]">Install a folder containing a valid plugin.toml.</p></div><button type="button" disabled={busy} onClick={() => void run(async () => { const path = await window.luma.choosePluginPackage(); if (path) await window.luma.installPlugin(path); })} className="luma-btn-secondary h-7 px-2 text-[10px]">Install plugin…</button></div>
      <div className="space-y-1">{plugins.map(([installed, manifest]) => <div key={installed.id} className="flex items-center gap-2 border-b border-[var(--luma-border)] py-2"><div className="min-w-0 flex-1"><div className="truncate text-[11px] font-medium">{manifest.name} <span className="font-normal text-[var(--luma-text-muted)]">v{manifest.version}</span></div><div className="truncate text-[10px] text-[var(--luma-text-muted)]">{manifest.description || installed.id}</div></div><button type="button" disabled={busy} onClick={() => void run(() => window.luma.setPluginEnabled(installed.id, !installed.enabled))} className="luma-btn-secondary h-7 px-2 text-[10px]">{installed.enabled ? "Disable" : "Enable"}</button><button type="button" disabled={busy} onClick={() => { if (window.confirm(`Uninstall ${manifest.name}?`)) void run(() => window.luma.uninstallPlugin(installed.id)); }} className="luma-btn-ghost h-7 px-2 text-[10px] text-[var(--luma-danger)]">Remove</button></div>)}{plugins.length === 0 && <p className="text-[10px] text-[var(--luma-text-muted)]">No native plugins installed.</p>}</div>
    </section>
    <section className="border-t border-[var(--luma-border)] pt-3">
      <div className="mb-2 flex items-center justify-between"><div><h3 className="text-[12px] font-semibold">MCP servers <span className="font-normal text-[var(--luma-text-muted)]">{servers.length}</span></h3><p className="text-[10px] text-[var(--luma-text-muted)]">Connect local stdio servers or remote Streamable HTTP endpoints.</p></div><button type="button" onClick={() => void run(() => window.luma.openExtensionFolder("mcp"))} className="luma-btn-secondary h-7 px-2 text-[10px]">Open config</button></div>
      <div className="mb-2 space-y-1">{servers.map((server) => <div key={server.id} className="flex items-center gap-2 border-b border-[var(--luma-border)] py-2"><div className="min-w-0 flex-1"><div className="text-[11px] font-medium">{server.name}</div><div className="truncate text-[10px] text-[var(--luma-text-muted)]">{server.transport === "http" ? server.url : `${server.command} ${server.args.join(" ")}`}</div></div><button type="button" disabled={busy} onClick={() => void run(() => window.luma.setMcpEnabled(server.id, !server.enabled))} className="luma-btn-secondary h-7 px-2 text-[10px]">{server.enabled ? "Disable" : "Enable"}</button><button type="button" disabled={busy} onClick={() => { if (window.confirm(`Remove MCP server ${server.name}?`)) void run(() => window.luma.removeMcp(server.id)); }} className="luma-btn-ghost h-7 px-2 text-[10px] text-[var(--luma-danger)]">Remove</button></div>)}{servers.length === 0 && <p className="text-[10px] text-[var(--luma-text-muted)]">No MCP servers configured.</p>}</div>
      <form className="grid gap-2" onSubmit={(event) => { event.preventDefault(); void run(async () => { if (transport === "stdio") await window.luma.addMcpStdio({ id: serverId.trim(), command: serverCommand.trim(), args: serverArgs.trim() ? JSON.parse(serverArgs) as string[] : [], env: JSON.parse(serverEnv) as Record<string, string> }); else await window.luma.addMcpHttp({ id: serverId.trim(), url: httpUrl.trim(), headers: JSON.parse(httpHeaders) as Record<string, string> }); setServerId(""); setServerCommand(""); setServerArgs(""); setServerEnv("{}"); setHttpUrl(""); }); }}>
        <div className="grid grid-cols-[110px_minmax(0,1fr)] gap-2"><select value={transport} onChange={(event) => setTransport(event.target.value as "stdio" | "http")} className="luma-input h-8 text-[10px]"><option value="stdio">Local stdio</option><option value="http">HTTP</option></select><input required pattern="[a-z0-9][a-z0-9_-]{0,63}" value={serverId} onChange={(event) => setServerId(event.target.value)} placeholder="server-id" className="luma-input h-8 text-[10px]" /></div>
        {transport === "stdio" ? <><input required value={serverCommand} onChange={(event) => setServerCommand(event.target.value)} placeholder="Command (e.g. npx)" className="luma-input h-8 text-[10px]" /><input value={serverArgs} onChange={(event) => setServerArgs(event.target.value)} placeholder='Arguments as JSON (e.g. ["-y", "@example/mcp"])' className="luma-input h-8 text-[10px]" /><textarea rows={2} value={serverEnv} onChange={(event) => setServerEnv(event.target.value)} placeholder='Environment variables as JSON (e.g. {"API_KEY":"${API_KEY}"})' className="luma-input resize-y text-[10px]" /></> : <><input required type="url" value={httpUrl} onChange={(event) => setHttpUrl(event.target.value)} placeholder="https://example.com/mcp" className="luma-input h-8 text-[10px]" /><textarea rows={2} value={httpHeaders} onChange={(event) => setHttpHeaders(event.target.value)} placeholder="HTTP headers as JSON" className="luma-input resize-y text-[10px]" /></>}
        <div className="flex justify-end"><button type="submit" disabled={busy || !serverId.trim() || (transport === "stdio" ? !serverCommand.trim() : !httpUrl.trim())} className="luma-btn-primary h-7 px-3 text-[10px]">Add MCP server</button></div>
      </form>
    </section>
  </div>;
}
