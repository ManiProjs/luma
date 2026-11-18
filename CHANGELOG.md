# Changelog

All notable changes to Luma are documented here.

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
