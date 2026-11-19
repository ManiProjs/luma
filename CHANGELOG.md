# Changelog

All notable changes to Luma are documented here.

## [1.0.0-beta.2] - 2026-11-19

This beta adds the packaged Desktop application, interactive responses in
Teammates conversations, and reusable routines learned from completed work.

### Added

- Electron Desktop is included in macOS, Linux, and Windows release bundles and
  installed with the `luma-desktop` launcher.
- Interactive Rich Responses now support action buttons, choice panels, and
  forms that continue a Teammate conversation with the submitted input.
- Teammates can discover and load workspace or user routines, and learn
  workflows into reviewable Markdown drafts with a redacted task-action
  reference. Drafts require review before installation or reuse.
- Release packaging verifies the Electron renderer, native `node-pty` module,
  and bundled Luma Core before producing platform archives.

### Improved

- Unified CI validates and packages the Desktop app for all supported release
  targets, alongside the Core, Ink client, and dashboard.
- Installers now verify and install the Desktop application, expose the
  `luma-desktop` command, and remove its launcher during uninstall.
- Installation and Teammates documentation now cover the expanded runtime and
  routine workflow.

### Release status

- This remains a beta prerelease. Platform packaging and installation should
  be exercised on each supported operating system before stable 1.0.
- No exactly-once guarantee is made for actions against external services.

## [1.0.0-beta.1] - 2026-11-18

First 1.0 beta release for hands-on evaluation of the unified Code and
Teammates experience.

### Added

- Rust Core shared by the Ink terminal client, local dashboard, and Electron
  desktop workspace.
- Persistent Teammates with memory, schedules, task delegation, specialist
  roles, and cross-agent Inbox.
- Durable SQLite task/action journals, restart recovery, explicit uncertain
  action reconciliation, and approval-aware execution plans.
- GitHub issue, pull request, review, and Actions integrations.
- Approval-gated Playwright browser automation with bounded network access.
- Agent Skills, native plugins, MCP integrations, and operational scorecards.
- Workspace inspection/editing, command execution, and review/approval gates.

### Release status

- This is a beta prerelease, not a stable 1.0 promise. Expect rough edges and
  breaking changes while installation, packaging, and cross-platform behavior
  are exercised.
- The GitHub release workflow currently publishes the Rust Core artifacts.
  Build the Ink client and desktop app from source using the README instructions;
  packaged desktop installers are not part of this release yet.
- No exactly-once guarantee is made for actions against external services.
