# Tools and change approval

Luma's agent tools cover workspace inspection, editing, execution, and project
navigation.

| Tool | Purpose |
| --- | --- |
| `list_directory` | Explore directories |
| `read_file` | Read source and project files |
| `search_files` | Search file contents and paths |
| `write_file` | Create or replace a complete file |
| `patch_file` | Apply a precise edit to existing text |
| `run_command` | Execute a shell command (approval required) |
| `find_symbols` | Find declarations |
| `find_references` | Find symbol usages |
| `git_status` / `git_diff` | Inspect working-tree changes |
| `git_log`, `git_show`, `git_blame` | Inspect Git history |
| `git_branches` | List branches |
| `file_metadata` | Inspect file metadata |
| `find_todos` | Find TODO-style markers |
| `count_lines` | Count files and lines |
| `inspect_project` | Inspect manifests and build targets |
| `list_skills` / `load_skill` | Discover and load Agent Skills |
| `execution_metrics` | Inspect persisted task, tool, duration, and token metrics |
| `evaluate_agent` | Build an operational reliability scorecard from task and tool outcomes |

Inspection tools are read-only. Luma can combine them with `run_command` to
build, test, format, or otherwise verify a project. Teammates also have network
tools (`web_search`, `fetch_url`, and `http_request`) when their network
permission allows them.

Teammates also receive first-party GitHub and browser tools when network access
is enabled:

| Tool | Purpose | Permission |
| --- | --- | --- |
| `github_get_repository` | Read repository metadata | Network, read-only |
| `github_list_issues` | List issues with bounded pagination | Network, read-only |
| `github_list_pull_requests` | Discover pull requests with bounded pagination | Network, read-only |
| `github_get_pull_request` | Inspect a pull request | Network, read-only |
| `github_get_pull_request_files` | Read changed files and available patches | Network, read-only |
| `github_list_pull_request_reviews` | Inspect review decisions and history | Network, read-only |
| `github_list_workflow_runs` | Inspect GitHub Actions runs | Network, read-only |
| `github_create_issue` | Create an issue | Network, approval required |
| `github_comment_on_issue` | Comment on an issue or pull request | Network, approval required |
| `github_create_pull_request` | Open a pull request | Network, approval required |
| `browser_automate` | Run a bounded Playwright sequence and optionally save a screenshot | Network, approval required |

GitHub authentication is read from `GH_TOKEN` or `GITHUB_TOKEN` at execution
time. Tokens are not stored in Teammate configuration or included in tool
outputs. Without a token, public repository reads use GitHub's unauthenticated
API limits; write operations require a valid token with repository access.

`browser_automate` runs a headless Playwright browser in a separate Node.js
process. A single invocation accepts up to 12 sequential navigate, snapshot,
click, fill, press, and screenshot steps. Screenshots must be written inside
the task workspace. Set `LUMA_NODE_BIN` or `LUMA_PLAYWRIGHT_MODULE` when a
packaged runtime cannot locate Node.js or Playwright. Install the Playwright
Chromium browser with `npx playwright install chromium` when it is not already
available. The browser session is isolated and closed at the end of each
invocation. Browser traffic passes through a local proxy that resolves each
destination once and connects to that exact IP, preventing a second Chromium
DNS lookup from changing the destination. Public HTTP/HTTPS use ports 80/443;
private LAN and reserved addresses are blocked. For a local development server,
set `allow_local:true` in the tool input; this only permits loopback and remains
subject to the tool's normal approval requirement.

## `patch_file`

`patch_file` is intended for small, targeted changes. The `old` text must match
exactly once, preventing an ambiguous replacement.

```json
{
  "path": "src/main.rs",
  "old": "let value = old_value;",
  "new": "let value = new_value;"
}
```

The typical editing workflow is:

1. Read and understand the relevant source.
2. Prepare a focused patch and show its diff.
3. Wait for approval or rejection.
4. Apply the approved edit.
5. Run appropriate verification and inspect the result.

Shell commands and external integration calls also follow the runtime's
approval and permission policies.
