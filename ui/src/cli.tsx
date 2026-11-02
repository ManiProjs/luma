import React from "react";
import { render } from "ink";
import { Command } from "commander";

import { App } from "./App";
import { LumaClient } from "./client";
import { startDesktopServer, stopDesktopServer } from "./server";
import type { ClientMessage, ServerEvent } from "./protocol";

type RequestEvent = Extract<ServerEvent, { data: { request_id: string } }>;

function launchInteractive(initialSetup = false) {
  render(
    <React.StrictMode>
      <App initialSetup={initialSetup} />
    </React.StrictMode>,
  );
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

async function startDashboard(host: string, port: number) {
  const client = new LumaClient();
  try {
    startDesktopServer();
    await client.connect();
    const requestId = crypto.randomUUID();
    const event = await request(client, {
      type: "start_dashboard",
      data: { request_id: requestId, host, port },
    }, "dashboard_started", requestId);
    process.stdout.write(`Luma Dashboard: http://${event.data.address}\nPress Ctrl+C to stop.\n`);
    await new Promise<void>((resolve) => {
      const shutdown = () => {
        process.removeListener("SIGINT", shutdown);
        process.removeListener("SIGTERM", shutdown);
        resolve();
      };
      process.once("SIGINT", shutdown);
      process.once("SIGTERM", shutdown);
    });
  } finally {
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
    .description("Configure the model provider and planner")
    .action(() => launchInteractive(true));

  const history = program.command("history")
    .description("Browse saved conversations")
    .action(historyList);
  history.command("list").description("List saved chats").action(historyList);
  history.command("show <id>").description("Show a saved chat").action(historyShow);

  program.command("summary")
    .description("Manage saved conversation summaries")
    .command("add <id> <summary...>")
    .description("Save a summary for a chat")
    .action(summaryAdd);

  program.command("dashboard")
    .description("Start the browser dashboard")
    .option("--host <host>", "Address to bind", "127.0.0.1")
    .option("--port <port>", "Port to bind", "3000")
    .action((options: { host: string; port: string }) => {
      const port = Number(options.port);
      if (!Number.isInteger(port) || port < 0 || port > 65535) throw new Error("Port must be an integer from 0 to 65535.");
      return startDashboard(options.host, port);
    });

  program.command("ask <prompt...>")
    .description("Send a one-shot prompt to Luma")
    .action(ask);

  await program.parseAsync(argv, { from: "user" });
}
