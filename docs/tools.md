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

Inspection tools are read-only. Luma can combine them with `run_command` to
build, test, format, or otherwise verify a project. Teammates also have network
tools (`web_search`, `fetch_url`, and `http_request`) when their network
permission allows them.

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
