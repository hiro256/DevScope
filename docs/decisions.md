# Decision Log

## 2026-08-23 — Project-centric architecture

- **Date:** 2026-08-23
- **Decision:** Make observable project state, rather than agent self-reporting, the
  basis for progress observation.
- **Reason:** Project artifacts provide a durable, inspectable view of development.

## 2026-08-23 — Markdown as the first Plan source

- **Date:** 2026-08-23
- **Decision:** Start Plan ingestion with Markdown task lists and planning documents.
- **Reason:** Markdown is local, common in repositories, and requires no service.

## 2026-08-23 — Git as the first Activity source

- **Date:** 2026-08-23
- **Decision:** Start Activity observation with Git repository data.
- **Reason:** Git exposes changes and history that are directly relevant to progress.

## 2026-08-23 — Rust + Ratatui + Crossterm

- **Date:** 2026-08-23
- **Decision:** Use Rust, with Ratatui and Crossterm planned for the TUI.
- **Reason:** This supports a responsive local TUI and Windows as the primary target
  while retaining a cross-platform path.

## 2026-08-23 — Progress Core separated from TUI

- **Date:** 2026-08-23
- **Decision:** Keep progress analysis independent from presentation.
- **Reason:** The same core may later support a TUI, VS Code extension, Web UI, or
  JSON/API.

## 2026-08-23 — Agent integrations are adapters, not core dependencies

- **Date:** 2026-08-23
- **Decision:** Model Codex and other agents as optional adapters.
- **Reason:** The project remains useful without a particular agent and avoids
  coupling its core to vendor-specific session formats.

## 2026-08-29 — Git Activity via Git CLI

- **Date:** 2026-08-29
- **Decision:** Collect Git Activity through the installed Git CLI.
- **Reason:** It avoids a large embedded Git dependency and follows the user's Git behavior. DevScope therefore requires git.exe (or git) on PATH for this source.

## 2026-09-05 — Evidence comes from observed verification execution

- **Date:** 2026-09-05
- **Decision:** Base Evidence on Build/Test processes launched and observed by
  DevScope, keep Evidence Core tool-neutral, and begin with manual verification
  execution rather than automatic runs.
- **Reason:** Observed process outcomes provide reliable exit status and freshness
  boundaries without coupling Evidence to an agent or a specific tool.
