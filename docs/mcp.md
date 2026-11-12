# Model Context Protocol (MCP)

MCP support is a separate integration system from native Luma plugins. Luma
currently supports local **stdio** MCP servers using JSON-RPC, `initialize`,
`notifications/initialized`, `tools/list`, and `tools/call`.

## Configure a server

```sh
luma mcp add filesystem npx -y @modelcontextprotocol/server-filesystem /path/to/workspace
luma mcp tools filesystem
luma mcp enable filesystem
luma mcp list
luma mcp disable filesystem
luma mcp remove filesystem
```

The server is registered disabled. Inspect its discovered tools before
enabling it. Add environment variables with repeatable `--env KEY=VALUE`
options. Quote references such as `--env 'API_KEY=${API_KEY}'` so your shell
does not expand the variable before Luma can read it from the launching
environment.
The server registry is stored with owner-only file permissions on Unix.

Enabled MCP tools are exposed as `mcp__<server-id>__<tool-name>` to Code and
to Teammates that inherit the Global teammate's connectors. Each tool call
starts the stdio server, initializes the session, calls the tool, and shuts
the process down. MCP tools require approval by
default. Every MCP tool is treated as network-capable and requires the agent's
network permission; server metadata cannot prove that a process is offline.
Code grants this capability when an enabled external integration exposes a
tool, while Teammates follow their per-agent Internet access setting. Each
MCP tool invocation still requires approval.

## Security and limitations

- Only stdio transport is implemented; Streamable HTTP is not implemented.
- MCP servers are arbitrary local programs running as the current OS user.
  Process separation is not sandboxing, and server initialization can execute
  code before a tool is called. Only enable servers you trust.
- Luma's network permission check controls whether a declared network-capable
  tool can be invoked. It does not OS-sandbox a server that has other ways to
  access the network.
- Tools are discovered when the core loads enabled servers. Restart Luma after
  changing server configuration so the active registry is rebuilt.
