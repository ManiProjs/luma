import React from "react";
import { render } from "ink";
import { Command } from "commander";
import { spawn } from "bun";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { createInterface } from "node:readline/promises";

import { App } from "./App";
import { TeammatesManager } from "./TeammatesManager";
import { LumaClient } from "./client";
import { startDesktopServer, stopDesktopServer } from "./server";
import type { ClientMessage, ServerEvent } from "./protocol";
import type { AgentConfiguration, AgentKind, MemoryScope } from "./protocol";

type RequestEvent = Extract<ServerEvent, { data: { request_id: string } }>;

function launchInteractive(initialSetup = false) {
  render(
    <React.StrictMode>
      <App initialSetup={initialSetup} />
    </React.StrictMode>,
    { exitOnCtrlC: false },
  );
}

function launchTeammates() {
  render(<React.StrictMode><TeammatesManager /></React.StrictMode>, { exitOnCtrlC: false });
}

async function withClient<T>(run: (client: LumaClient) => Promise<T>): Promise<T> {
  const client = new LumaClient();
  try {
    startDesktopServer();
    await client.connect();
    return await run(client);
  } finally {
    client.disconnect();
    stopDesktopServer();
  }
}

function request<T extends RequestEvent["type"]>(
  client: LumaClient,
  message: ClientMessage,
  expectedType: T,
  requestId: string,
): Promise<Extract<RequestEvent, { type: T }>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timed out waiting for core response: ${expectedType}.`));
    }, 30_000);
    const unsubscribe = client.subscribe((event) => {
      if (event.type === "error" && (event.data.request_id === requestId || event.data.request_id === null)) {
        clearTimeout(timer);
        unsubscribe();
        reject(new Error(event.data.message));
        return;
      }
      if (event.type === expectedType && "request_id" in event.data && event.data.request_id === requestId) {
        clearTimeout(timer);
        unsubscribe();
        resolve(event as Extract<RequestEvent, { type: T }>);
      }
    });
    try {
      client.send(message);
    } catch (error) {
      clearTimeout(timer);
      unsubscribe();
      reject(error);
    }
  });
}

async function historyList() {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "history_list", data: { request_id: requestId } }, "history_list", requestId);
    if (event.data.chats.length === 0) {
      process.stdout.write("No saved chats yet.\n");
      return;
    }
    for (const chat of event.data.chats) {
      process.stdout.write(`${chat.id}  ${chat.title}  (${chat.messages} messages)\n`);
      if (chat.summary) process.stdout.write(`  Summary: ${chat.summary}\n`);
    }
  });
}

async function historyShow(id: string) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "history_show", data: { request_id: requestId, id } }, "history_chat", requestId);
    const chat = event.data.chat;
    process.stdout.write(`Chat ${chat.id} · ${chat.info.model} · ${chat.info.workspace}\n`);
    if (chat.summary) process.stdout.write(`\nSummary\n${chat.summary}\n`);
    for (const message of chat.messages) {
      process.stdout.write(`\n[${message.role}]\n${message.content}\n`);
    }
  });
}

async function historyDelete(id: string, options: { yes?: boolean }) {
  if (!options.yes) {
    if (!process.stdin.isTTY) throw new Error("Pass --yes to delete a chat in a non-interactive terminal.");
    const readline = createInterface({ input: process.stdin, output: process.stdout });
    try {
      const answer = await readline.question(`Permanently delete chat ${id}? [y/N] `);
      if (!/^y(es)?$/i.test(answer.trim())) {
        process.stdout.write("Cancelled.\n");
        return;
      }
    } finally {
      readline.close();
    }
  }

  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, {
      type: "history_delete",
      data: { request_id: requestId, id },
    }, "history_deleted", requestId);
    if (!event.data.deleted) throw new Error(`Saved chat not found: ${id}`);
    process.stdout.write(`Deleted chat ${id}.\n`);
  });
}

async function historyPrune(options: { yes?: boolean }) {
  if (!options.yes) {
    if (!process.stdin.isTTY) throw new Error("Pass --yes to prune saved chats in a non-interactive terminal.");
    const readline = createInterface({ input: process.stdin, output: process.stdout });
    try {
      const answer = await readline.question("Permanently delete EVERY saved Luma chat? This cannot be undone. [y/N] ");
      if (!/^y(es)?$/i.test(answer.trim())) {
        process.stdout.write("Cancelled.\n");
        return;
      }
    } finally {
      readline.close();
    }
  }

  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, {
      type: "history_prune",
      data: { request_id: requestId },
    }, "history_pruned", requestId);
    process.stdout.write(`Pruned ${event.data.removed} saved chat${event.data.removed === 1 ? "" : "s"}.\n`);
  });
}

async function historyExport(id: string, file?: string) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "history_show", data: { request_id: requestId, id } }, "history_chat", requestId);
    const output = `${JSON.stringify(event.data.chat, null, 2)}\n`;
    if (!file || file === "-") {
      process.stdout.write(output);
      return;
    }
    const target = resolve(file);
    await writeFile(target, output, { flag: "wx" });
    process.stdout.write(`Exported chat ${id} to ${target}.\n`);
  });
}

async function providersList() {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "get_providers", data: { request_id: requestId } }, "providers", requestId);
    for (const provider of event.data.providers) {
      process.stdout.write(`${provider.id}  ${provider.name}${provider.requires_api_key ? "  (API key required)" : ""}\n  ${provider.endpoint}\n`);
    }
    if (event.data.providers.length === 0) process.stdout.write("No providers are available.\n");
  });
}

async function pluginsList() {
  runCoreCommand(["plugins", "list"]);
}

async function pluginInstall(path: string) {
  runCoreCommand(["plugins", "install", resolve(path)]);
}

async function pluginToggle(id: string, enabled: boolean) {
  runCoreCommand(["plugins", enabled ? "enable" : "disable", id]);
}

async function pluginRemove(id: string) {
  runCoreCommand(["plugins", "uninstall", id]);
}

async function mcpList() {
  runCoreCommand(["mcp", "list"]);
}

async function mcpAdd(id: string, command: string, args: string[], options: { env?: string[] }) {
  const forwarded = ["mcp", "add", id, command, ...args];
  for (const item of options.env ?? []) forwarded.push("--env", item);
  runCoreCommand(forwarded);
}

async function mcpAddHttp(id: string, url: string, options: { header?: string[] }) {
  const forwarded = ["mcp", "add-http", id, url];
  for (const item of options.header ?? []) forwarded.push("--header", item);
  runCoreCommand(forwarded);
}

async function mcpToggle(id: string, enabled: boolean) {
  runCoreCommand(["mcp", enabled ? "enable" : "disable", id]);
}

async function mcpRemove(id: string) {
  runCoreCommand(["mcp", "remove", id]);
}

async function mcpTools(id: string) {
  runCoreCommand(["mcp", "tools", id]);
}

function runCoreCommand(args: string[]) {
  const localBuilds = [
    resolve(import.meta.dir, "../../target/debug/luma-core"),
    resolve(import.meta.dir, "../../target/release/luma-core"),
  ];
  const executable = process.env.LUMA_BINARY ?? process.env.LUMA_CORE_BIN ?? localBuilds.find(existsSync) ?? "luma-core";
  execFileSync(executable, args, { stdio: "inherit" });
}

function configPath() {
  const base = process.platform === "darwin"
    ? resolve(homedir(), "Library", "Application Support", "luma")
    : process.platform === "win32"
      ? resolve(process.env.APPDATA ?? resolve(homedir(), "AppData", "Roaming"), "luma")
      : resolve(process.env.XDG_CONFIG_HOME ?? resolve(homedir(), ".config"), "luma");
  process.stdout.write(`${resolve(base, "config.toml")}\n`);
}

async function summaryAdd(id: string, summary: string[]) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    await request(client, {
      type: "summary_add",
      data: { request_id: requestId, id, summary: summary.join(" ") },
    }, "summary_saved", requestId);
    process.stdout.write(`Summary saved for chat ${id}.\n`);
  });
}

const defaultAgentConfiguration = (): AgentConfiguration => ({
  system_prompt: "",
  mascot: "auto",
  use_global_context: true,
  skills: [],
  provider: null,
  model: null,
  endpoint: null,
  workspace: process.cwd(),
  allowed_workspaces: [process.cwd()],
  enabled_tools: [],
  network_access: true,
  permissions: {
    allowed_tools: ["read_file", "list_directory", "search_files", "git_status", "git_diff", "find_symbols", "find_references", "git_log", "git_show", "git_blame", "git_branches", "file_metadata", "find_todos", "count_lines", "inspect_project", "list_skills", "load_skill", "web_search", "fetch_url"],
    denied_tools: [],
    auto_approve_read_only: true,
    require_approval_for_mutations: true,
    allow_network: true,
  },
  max_concurrent_tasks: 1,
  recover_interrupted_tasks: false,
});

async function teammatesList() {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "agents_list", data: { request_id: requestId } }, "agents", requestId);
    if (event.data.agents.length === 0) process.stdout.write("No teammates yet. Create one with: luma teammates create <name>\n");
    for (const agent of event.data.agents) {
      process.stdout.write(`${agent.id}  ${agent.name}  [${agent.status}]  ${agent.kind}\n`);
    }
  });
}

async function teammatesCreate(name: string, kind: AgentKind, options: { workspace?: string; personality?: string; skill?: string[]; memory?: string }) {
  await withClient(async (client) => {
    const configuration = defaultAgentConfiguration();
    configuration.system_prompt = options.personality ?? "";
    configuration.skills = options.skill ?? [];
    if (options.workspace) {
      configuration.workspace = resolve(options.workspace);
      configuration.allowed_workspaces = [configuration.workspace];
    }
    const requestId = crypto.randomUUID();
    const event = await request(client, {
      type: "agent_create",
      data: { request_id: requestId, name, description: null, kind, configuration },
    }, "agent_saved", requestId);
    if (options.memory?.trim()) {
      const memoryRequestId = crypto.randomUUID();
      await request(client, { type: "memory_add", data: { request_id: memoryRequestId, scope: "agent", scope_id: event.data.agent.id, content: options.memory.trim(), source: "cli:create-teammate" } }, "memory_saved", memoryRequestId);
    }
    process.stdout.write(`Created ${kind} agent ${event.data.agent.name} (${event.data.agent.id}).\n`);
  });
}

async function teammatesDelete(id: string) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "agent_delete", data: { request_id: requestId, id } }, "agent_deleted", requestId);
    if (!event.data.deleted) throw new Error(`Teammate not found: ${id}`);
    process.stdout.write(`Deleted teammate ${id}.\n`);
  });
}

async function tasksList(agentId?: string) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "tasks_list", data: { request_id: requestId, agent_id: agentId ?? null, limit: 200 } }, "tasks", requestId);
    if (event.data.tasks.length === 0) process.stdout.write("No tasks.\n");
    for (const task of event.data.tasks) process.stdout.write(`${task.id}  [${task.status}]  ${task.title}\n  agent: ${task.agent_id}\n`);
  });
}

async function taskCreate(agentId: string, prompt: string[]) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "task_create", data: { request_id: requestId, agent_id: agentId, request: prompt.join(" "), title: null, workspace: process.cwd(), recover_on_restart: false } }, "task_saved", requestId);
    process.stdout.write(`Queued task ${event.data.task.id}.\n`);
  });
}

async function taskAction(id: string, action: "task_cancel" | "task_retry") {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: action, data: { request_id: requestId, id } }, "task_saved", requestId);
    process.stdout.write(`Task ${event.data.task.id}: ${event.data.task.status}.\n`);
  });
}

async function approvalsList() {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "approvals_list", data: { request_id: requestId } }, "approvals", requestId);
    if (event.data.approvals.length === 0) process.stdout.write("No pending approvals.\n");
    for (const approval of event.data.approvals) process.stdout.write(`${approval.id}  ${approval.tool}\n  task: ${approval.task_id}\n  ${approval.arguments}\n`);
  });
}

async function approvalResolve(id: string, allowed: boolean) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "approval_resolve", data: { request_id: requestId, id, allowed } }, "approval_resolved", requestId);
    process.stdout.write(`${event.data.approval.status}: ${event.data.approval.tool}\n`);
  });
}

async function memoryAdd(scope: MemoryScope, scopeId: string, content: string[]) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "memory_add", data: { request_id: requestId, scope, scope_id: scopeId, content: content.join(" "), source: "cli" } }, "memory_saved", requestId);
    process.stdout.write(`Saved memory ${event.data.entry.id} in ${scope}:${scopeId}.\n`);
  });
}

async function memoryList(scope: MemoryScope, scopeId: string) {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "memory_list", data: { request_id: requestId, scope, scope_id: scopeId, query: null, limit: 200 } }, "memory", requestId);
    for (const entry of event.data.entries) process.stdout.write(`${entry.id}  ${entry.content}\n`);
    if (event.data.entries.length === 0) process.stdout.write("No memory entries.\n");
  });
}

async function schedulesList() {
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "schedules_list", data: { request_id: requestId, agent_id: null } }, "schedules", requestId);
    for (const schedule of event.data.schedules) process.stdout.write(`${schedule.id}  ${schedule.title}  ${schedule.enabled ? "enabled" : "disabled"}  next ${new Date(schedule.next_run_at * 1000).toLocaleString()}\n`);
    if (event.data.schedules.length === 0) process.stdout.write("No schedules.\n");
  });
}

async function scheduleCreate(agentId: string, interval: string, prompt: string[]) {
  const everySeconds = Number(interval);
  if (!Number.isInteger(everySeconds) || everySeconds < 60 || everySeconds > 31_536_000) throw new Error("Schedule interval must be between 60 seconds and one year.");
  await withClient(async (client) => {
    const requestId = crypto.randomUUID();
    const event = await request(client, { type: "schedule_create", data: { request_id: requestId, agent_id: agentId, title: "Recurring task", prompt: prompt.join(" "), run_at: null, every_seconds: everySeconds } }, "schedule_saved", requestId);
    process.stdout.write(`Created schedule ${event.data.schedule.id}.\n`);
  });
}

async function startDevelopmentEnvironment(
  host: string,
  port: number,
  viteHost: string,
  vitePort: number,
) {
  const client = new LumaClient();
  let vite: ReturnType<typeof spawn> | null = null;
  let shutdown: (() => void) | null = null;

  const waitForShutdown = new Promise<void>((resolveShutdown) => {
    shutdown = resolveShutdown;
    process.once("SIGINT", resolveShutdown);
    process.once("SIGTERM", resolveShutdown);
  });

  try {
    const core = startDesktopServer();
    await client.connect();

    const requestId = crypto.randomUUID();
    const event = await request(client, {
      type: "start_dashboard",
      data: { request_id: requestId, host, port },
    }, "dashboard_started", requestId);

    const dashboardDirectory = resolve(import.meta.dir, "../../dashboard");
    vite = spawn([
      "npm", "run", "dev", "--prefix", dashboardDirectory,
      "--", "--host", viteHost, "--port", String(vitePort), "--strictPort",
    ], {
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
      env: {
        ...process.env,
        LUMA_CORE_URL: `http://${event.data.address}`,
      },
    });

    process.stdout.write(
      `Luma development environment\nCore API: http://${event.data.address}\nDashboard: http://${viteHost}:${vitePort}\nPress Ctrl+C to stop both servers.\n`,
    );

    await Promise.race([
      waitForShutdown,
      core.exited.then((code) => {
        throw new Error(`Luma core exited${code === 0 ? "" : ` with code ${code}`}.`);
      }),
      vite.exited.then((code) => {
        if (code !== 0) throw new Error(`Vite exited with code ${code}.`);
      }),
    ]);
  } finally {
    if (shutdown) {
      process.removeListener("SIGINT", shutdown);
      process.removeListener("SIGTERM", shutdown);
    }
    vite?.kill();
    client.disconnect();
    stopDesktopServer();
  }
}

async function ask(prompt: string[]) {
  const client = new LumaClient();
  try {
    startDesktopServer();
    await client.connect();
    await new Promise<void>((resolve, reject) => {
      const unsubscribe = client.subscribe((event) => {
        if (event.type === "error" && event.data.request_id === null) {
          process.stderr.write(`${event.data.message}\n`);
          unsubscribe();
          reject(new Error(event.data.message));
          return;
        }
        if (event.type !== "agent") return;
        if (event.data.event.type === "TextDelta") process.stdout.write(event.data.event.data);
        if (event.data.event.type === "Finished") {
          process.stdout.write("\n");
          unsubscribe();
          resolve();
        }
        if (event.data.event.type === "Error") {
          process.stderr.write(`${event.data.event.data}\n`);
          unsubscribe();
          reject(new Error(event.data.event.data));
        }
      });
      try {
        client.send({ type: "prompt", data: { text: prompt.join(" ") } });
      } catch (error) {
        unsubscribe();
        reject(error);
      }
    });
  } finally {
    client.disconnect();
    stopDesktopServer();
  }
}

export async function runCli(argv: string[]) {
  const program = new Command()
    .name("luma")
    .description("Luma — an AI coding agent")
    .version("0.1.0")
    .action(() => launchInteractive());

  program.command("setup")
    .description("Configure the model provider and ReAct model")
    .action(() => launchInteractive(true));

  const history = program.command("history")
    .description("Browse saved conversations")
    .action(historyList);
  history.command("list").description("List saved chats").action(historyList);
  history.command("show <id>").description("Show a saved chat").action(historyShow);
  history.command("delete <id>").description("Permanently delete a saved chat")
    .option("-y, --yes", "Skip the confirmation prompt")
    .action(historyDelete);
  history.command("prune").description("Permanently delete every saved chat")
    .option("-y, --yes", "Skip the confirmation prompt")
    .action(historyPrune);
  history.command("export <id> [file]").description("Export a saved chat as JSON (use - for stdout)").action(historyExport);

  const config = program.command("config").description("Inspect Luma configuration locations");
  config.command("path").description("Print the active config.toml path").action(configPath);

  program.command("providers").description("List model providers supported by this Luma build").action(providersList);

  const plugins = program.command("plugins").description("Manage native Luma plugins").action(pluginsList);
  plugins.command("list").description("List installed Luma plugins").action(pluginsList);
  plugins.command("install <path>").description("Install a local Luma plugin package (disabled by default)").action(pluginInstall);
  plugins.command("enable <id>").description("Enable a Luma plugin").action((id: string) => pluginToggle(id, true));
  plugins.command("disable <id>").description("Disable a Luma plugin").action((id: string) => pluginToggle(id, false));
  plugins.command("uninstall <id>").description("Uninstall a Luma plugin").action(pluginRemove);

  const mcp = program.command("mcp").description("Manage Model Context Protocol servers").action(mcpList);
  mcp.command("list").description("List MCP servers").action(mcpList);
  mcp.command("add <id> <command> [args...]").description("Register a local stdio MCP server (disabled by default)")
    .allowUnknownOption(true)
    .option("--env <key=value>", "Pass an environment variable (repeatable)", (value: string, previous: string[] = []) => [...previous, value], [])
    .action(mcpAdd);
  mcp.command("add-http <id> <url>").description("Register a Streamable HTTP MCP server (disabled by default)")
    .option("--header <key=value>", "HTTP request header (repeatable)", (value: string, previous: string[] = []) => [...previous, value], [])
    .action(mcpAddHttp);
  mcp.command("enable <id>").description("Enable an MCP server").action((id: string) => mcpToggle(id, true));
  mcp.command("disable <id>").description("Disable an MCP server").action((id: string) => mcpToggle(id, false));
  mcp.command("remove <id>").description("Remove an MCP server configuration").action(mcpRemove);
  mcp.command("tools <id>").description("Discover tools exposed by an MCP server").action(mcpTools);

  program.command("summary")
    .description("Manage saved conversation summaries")
    .command("add <id> <summary...>")
    .description("Save a summary for a chat")
    .action(summaryAdd);

  const teammates = program.command("teammates").description("Manage persistent Teammates agents").action(launchTeammates);
  teammates.command("list").description("List persistent agents").action(teammatesList);
  teammates.command("create <name>")
    .description("Create a persistent teammate")
    .option("--workspace <path>", "Workspace this teammate may access")
    .option("--personality <instructions>", "Custom personality / system instructions")
    .option("--skill <name>", "Attach a skill (repeatable)", (value: string, previous: string[] = []) => [...previous, value], [])
    .option("--memory <text>", "Initial agent memory")
    .action((name: string, options: { workspace?: string; personality?: string; skill: string[]; memory?: string }) => teammatesCreate(name, "teammates", options));
  teammates.command("delete <id>").description("Delete a teammate and its dependent runtime data").action(teammatesDelete);
  teammates.command("create-code <name>")
    .description("Create a persistent Code agent")
    .option("--workspace <path>", "Workspace this agent may access")
    .option("--personality <instructions>", "Custom personality / system instructions")
    .option("--skill <name>", "Attach a skill (repeatable)", (value: string, previous: string[] = []) => [...previous, value], [])
    .option("--memory <text>", "Initial agent memory")
    .action((name: string, options: { workspace?: string; personality?: string; skill: string[]; memory?: string }) => teammatesCreate(name, "code", options));

  const tasks = program.command("tasks").description("Manage persistent tasks");
  tasks.command("list [agent-id]").description("List tasks").action(tasksList);
  tasks.command("create <agent-id> <prompt...>").description("Queue a task for an agent").action(taskCreate);
  tasks.command("cancel <id>").description("Cancel a queued or running task").action((id: string) => taskAction(id, "task_cancel"));
  tasks.command("retry <id>").description("Retry a failed or interrupted task").action((id: string) => taskAction(id, "task_retry"));

  const approvals = program.command("approvals").description("Review persistent task approvals");
  approvals.command("list").description("List pending approvals").action(approvalsList);
  approvals.command("approve <id>").description("Approve a pending operation").action((id: string) => approvalResolve(id, true));
  approvals.command("deny <id>").description("Deny a pending operation").action((id: string) => approvalResolve(id, false));

  const memory = program.command("memory").description("Inspect and update scoped memory");
  memory.command("list <scope> <scope-id>").description("List memory entries").action((scope: MemoryScope, scopeId: string) => memoryList(scope, scopeId));
  memory.command("add <scope> <scope-id> <content...>").description("Add a memory entry").action(memoryAdd);

  const schedules = program.command("schedules").description("Manage recurring work");
  schedules.command("list").description("List schedules").action(schedulesList);
  schedules.command("create <agent-id> <seconds> <prompt...>").description("Schedule a recurring task").action(scheduleCreate);

  program.command("dashboard")
    .description("Start the Rust core and browser dashboard together")
    .option("--host <host>", "Core API bind address", "127.0.0.1")
    .option("--port <port>", "Core API port", "3000")
    .option("--vite-host <host>", "Dashboard bind address", "127.0.0.1")
    .option("--vite-port <port>", "Dashboard port", "5173")
    .action((options: { host: string; port: string; viteHost: string; vitePort: string }) => {
      const port = Number(options.port);
      const vitePort = Number(options.vitePort);
      for (const [name, value] of [["Core port", port], ["Dashboard port", vitePort]] as const) {
        const minimum = name === "Dashboard port" ? 1 : 0;
        if (!Number.isInteger(value) || value < minimum || value > 65535) {
          throw new Error(`${name} must be an integer from ${minimum} to 65535.`);
        }
      }
      return startDevelopmentEnvironment(options.host, port, options.viteHost, vitePort);
    });

  program.command("ask <prompt...>")
    .description("Send a one-shot prompt to Luma")
    .action(ask);

  await program.parseAsync(argv, { from: "user" });
}
