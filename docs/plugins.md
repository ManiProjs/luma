# Native Luma plugins

Native Luma plugins are **not MCP servers**. They use Luma's own manifest and
one-shot JSON process protocol. Keep plugin packages trusted: a plugin process
runs as the current OS user. Luma's approval checks govern tool invocation;
they are not an operating-system sandbox.

## Package layout

```text
example-plugin/
  plugin.toml
  main.py
```

`plugin.toml`:

```toml
schema_version = 1
id = "example-plugin"
name = "Example plugin"
version = "1.0.0"
description = "A small native Luma plugin"
command = "python3"
args = ["main.py"]

[[tools]]
name = "summarize_text"
description = "Summarize text supplied in the input object"
model_guide = "Use this for short text summaries. Pass the complete source text in the text field; do not use it to read files."
read_only = true # Informational only; Luma still asks before external actions.
requires_network = false
input_schema = { type = "object", properties = { text = { type = "string" } }, required = ["text"] }
```

`main.py` receives one JSON request on stdin and writes one JSON response to
stdout:

```python
import json
import sys

request = json.loads(sys.stdin.readline())
if request["protocol"] != "luma-plugin" or request["version"] != 1:
    raise SystemExit("unsupported Luma plugin protocol")

if request["tool"] == "summarize_text":
    text = request["input"].get("text", "")
    result = {"output": text[:500]}
else:
    result = {"ok": False, "error": "unknown tool"}

print(json.dumps(result))
```

The request contains `protocol`, `version`, `plugin`, `tool`, and `input`.
Return either `{"output":"..."}`, `{"result": ...}`, or
`{"ok":false,"error":"..."}`. Calls have a 30-second timeout and a 1 MiB
response limit. Plugin environment values can refer to inherited variables as
`${VARIABLE}`; only a small base environment is inherited by the child.

## Install and manage

```sh
luma plugins install ./example-plugin
luma plugins list
luma plugins enable example-plugin
luma plugins disable example-plugin
luma plugins uninstall example-plugin
```

Installation copies the package into Luma's config directory and leaves it
disabled until explicitly enabled. Enabled plugin tools are available to Code
and to Teammates that inherit the Global teammate's connectors. Plugin tools
require approval by default. Both `read_only` and
`requires_network` are descriptive only: Luma does not trust a child process
to certify its own safety or lack of network access. Every native plugin tool
therefore requires the agent's network permission as well as tool approval.
Each tool can provide a separate `model_guide` alongside its internal `name`
and user-facing `description`. Luma builds the model's available-tool prompt
from these registered values automatically; if `model_guide` is omitted, the
description is used as the model guidance.
Code grants the network capability when an enabled external integration exposes
a tool. Teammates follow their per-agent Internet access setting.

## Limitations

- Local package installation only; no marketplace or remote package installer.
- One process is started per tool call.
- A separate process is not a sandbox. Review the package before enabling it.
- Luma does not inject provider credentials into plugin environments.
