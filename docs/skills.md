# Agent Skills

Teammates can use `propose_skill` to turn a reusable workflow or lesson into a
reviewable draft under `.luma/skill-drafts/`. Drafts are excluded from skill
discovery and cannot affect agent behavior until a person reviews them and
moves the skill directory into `.agents/skills/`.

Luma supports the open Agent Skills directory convention. A skill is a
directory containing a `SKILL.md` file with YAML frontmatter. The frontmatter
must declare a `name` matching the directory and a non-empty `description`.

```text
my-project/
└── .agents/
    └── skills/
        └── rust-testing/
            └── SKILL.md
```

Example `SKILL.md`:

```markdown
---
name: rust-testing
description: Run focused Rust tests and diagnose failures.
---

# Rust testing

Prefer focused tests first, then run the full suite before finalizing.
```

Luma discovers workspace skills in `.agents/skills/` and `.luma/skills/`, and
personal skills in `~/.agents/skills/` and Luma's config directory at
`skills/` (for example `~/.config/luma/skills/` on Linux). Workspace skills
override personal skills with the same name.

The ReAct agent uses `list_skills` to discover metadata and `load_skill` to
load a relevant skill only when needed. A skill can reference companion files
in its directory; use `read_file` to inspect those resources. Skills are
guidance and do not bypass approval or runtime safety boundaries.

Each Teammate can attach its own skills and optionally inherit the Global
teammate's shared skills and memory. See [Teammates](teammates.md) for details.
