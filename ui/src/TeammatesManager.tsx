import { useEffect, useState } from "react";
import { Box, Text, useApp, useInput } from "ink";
import { randomUUID } from "node:crypto";
import { LumaClient } from "./client";
import { startDesktopServer, stopDesktopServer } from "./server";
import type { AgentRecord, ClientMessage, ServerEvent } from "./protocol";
import { Mascot } from "./components/Mascot";

type RequestEvent = Extract<ServerEvent, { data: { request_id: string } }>;

function request<T extends RequestEvent["type"]>(
  client: LumaClient,
  message: ClientMessage,
  expectedType: T,
  requestId: string,
): Promise<Extract<RequestEvent, { type: T }>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { unsubscribe(); reject(new Error(`Timed out waiting for ${expectedType}.`)); }, 15_000);
    const unsubscribe = client.subscribe((event) => {
      if (event.type === "error" && (event.data.request_id === requestId || event.data.request_id === null)) {
        clearTimeout(timer); unsubscribe(); reject(new Error(event.data.message)); return;
      }
      if (event.type === expectedType && "request_id" in event.data && event.data.request_id === requestId) {
        clearTimeout(timer); unsubscribe(); resolve(event as Extract<RequestEvent, { type: T }>);
      }
    });
    try { client.send(message); }
    catch (error) { clearTimeout(timer); unsubscribe(); reject(error); }
  });
}

export function TeammatesManager() {
  const { exit } = useApp();
  const [client] = useState(() => new LumaClient());
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [selected, setSelected] = useState(0);
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    const requestId = randomUUID();
    const response = await request(client, { type: "agents_list", data: { request_id: requestId } }, "agents", requestId);
    setAgents(response.data.agents);
    setSelected((index) => Math.min(index, Math.max(0, response.data.agents.length - 1)));
  }

  useEffect(() => {
    let active = true;
    startDesktopServer();
    void client.connect().then(async () => {
      if (!active) return;
      setConnected(true);
      await refresh();
    }).catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)); });
    return () => { active = false; client.disconnect(); stopDesktopServer(); };
  }, [client]);

  async function deleteSelected() {
    const agent = agents[selected];
    if (!agent) return;
    setBusy(true); setError("");
    try {
      const requestId = randomUUID();
      const response = await request(client, { type: "agent_delete", data: { request_id: requestId, id: agent.id } }, "agent_deleted", requestId);
      if (!response.data.deleted) throw new Error("Teammate no longer exists.");
      setConfirmDelete(false);
      await refresh();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  useInput((input, key) => {
    if (busy) return;
    if (confirmDelete) {
      if (input.toLowerCase() === "y") void deleteSelected();
      else if (input.toLowerCase() === "n" || key.escape) setConfirmDelete(false);
      return;
    }
    if (input === "q" || key.escape) { exit(); return; }
    if (key.upArrow) setSelected((index) => Math.max(0, index - 1));
    else if (key.downArrow) setSelected((index) => Math.min(agents.length - 1, index + 1));
    else if (input.toLowerCase() === "d" && agents[selected]) setConfirmDelete(true);
    else if (input.toLowerCase() === "r") void refresh().catch((reason) => setError(String(reason)));
  });

  const agent = agents[selected];
  return <Box flexDirection="column" paddingX={1}>
    <Box marginBottom={1} justifyContent="space-between">
      <Box flexDirection="row" alignItems="center" gap={1}>
        <Mascot seed="luma-crew" compact />
        <Text bold color="cyan">LUMA / TEAMMATES</Text>
      </Box>
      <Text color={connected ? "green" : "red"}>{connected ? "Core connected" : "Connecting…"}</Text>
    </Box>
    <Box borderStyle="round" borderColor="gray" paddingX={1} flexDirection="column">
      <Text bold>Your teammates ({agents.length})</Text>
      {agents.length === 0 ? <Text color="gray">No teammates yet. Create one with `luma teammates create &lt;name&gt;`.</Text> : agents.map((item, index) => <Box key={item.id}>
        <Box flexDirection="row" gap={1}>
          <Mascot seed={item.id} compact />
          <Text color={index === selected ? "cyan" : "white"}>{index === selected ? "› " : "  "}{item.name}</Text>
        </Box>
        <Text color="gray">  {item.status} · {item.kind}</Text>
      </Box>)}
    </Box>
    {agent && <Box marginTop={1} flexDirection="column">
      <Box flexDirection="row" alignItems="center" gap={2}>
        <Mascot seed={agent.id} />
        <Box flexDirection="column"><Text bold>{agent.name}</Text><Text color="gray">{agent.status} · {agent.kind}</Text></Box>
      </Box>
      <Text color="gray">{agent.description || "No description"}</Text>
      <Text>Skills: {agent.configuration.skills.length ? agent.configuration.skills.join(", ") : "none attached"}</Text>
      <Text>Workspace: {agent.configuration.workspace || "default"}</Text>
    </Box>}
    {confirmDelete && agent && <Box marginTop={1}><Text color="yellow">Delete {agent.name} and its completed history, schedules, approvals and agent memory? Cancel active tasks first. (y/N)</Text></Box>}
    {error ? <Box marginTop={1}><Text color="red">{error}</Text></Box> : null}
    <Box marginTop={1}><Text color="gray">↑/↓ select  ·  d delete  ·  r refresh  ·  q quit</Text></Box>
  </Box>;
}
