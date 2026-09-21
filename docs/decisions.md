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

## 2026-09-05 — Evidence abstraction follows concrete source experiments

- **Date:** 2026-09-05
- **Decision:** Use Cargo Build/Test as the first concrete Evidence source, keep
  Evidence broader than process execution, and defer a stable generic Evidence
  Source extension contract until after a materially different second source
  experiment, likely Artifact Evidence.
- **Reason:** This avoids Build/Test overfitting and speculative abstraction. The
  earlier Evidence decision defines the initial v0.3 Build/Test source, not the
  permanent definition of every future Evidence source.

## 2026-09-19 — Configured Build/Test command boundary

- **Date:** 2026-09-19
- **Decision:** Add per-kind project Config commands as program plus argument vector, with configured commands overriding the Cargo default and project-root working directory. Do not parse shell command strings.
- **Reason:** This retains Cargo zero-config behavior while providing a small, tool-neutral process boundary. `verify.exclude` affects only Build/Test Freshness observation, so project-specific generated outputs can be excluded without changing command execution or other sources.

## 2026-09-19 - Repo-local DevScope Skill packaging

- **Decision:** Package the operational DevScope workflow at `.agents/skills/devscope`. Keep the Skill focused on DevScope operation, the Setup Guide on onboarding and configuration, and `AGENTS.md` on repository authority and quality rules.
- **Reason:** A repo-local Skill travels with a project and is automatically discoverable by Codex. A user-level link or installation can support local experimentation without replacing the portable project source.

## 2026-09-20 — Git-aware worktree detector boundary

- **Decision:** Treat Git worktree detection as a heuristic for Git Activity recollection, not as a generic filesystem watcher. Keep the temporary `target` safeguard, but make a future detector candidate set Git-aware through `git ls-files -c -o --exclude-standard -z` plus path metadata stamps. Retain Git metadata detection separately.
- **Reason:** Git owns nested ignore, negation, repository-local, and global exclude semantics. A recursive scanner with directory-name exclusions is both incomplete and vulnerable to generated-output scale, while polling full Activity would duplicate more expensive Git collection work.

## 2026-09-20 — Isolate Git worktree scanning from TUI responsiveness

- **Decision:** Run `GitWorktreeChangeDetector` in one std-thread worker that owns its detector baseline, and communicate scan requests/results through std::sync::mpsc channels while coalescing requests so at most one scan is in flight from the TUI side. Keep Git metadata detection and Activity collection outside that worker for now.
- **Reason:** A recursive worktree scan can take hundreds of milliseconds or longer on generated-output-heavy projects. Isolating it preserves TUI input/render responsiveness without introducing a generic worker framework, runtime dependency, or a new observation authority.
## 2026-09-20 — Coarse diagnostics only after a slow worktree scan

- **Decision:** Record every worktree scan duration and capture root-level subtree counts and approximate durations only on the scan after an initial 200ms heuristic is exceeded.
- **Reason:** This keeps the fast path small while providing bounded diagnostic context for a later exclusion workflow; the threshold is an internal heuristic, not a user policy or performance contract.

## 2026-09-20 — Explicit Activity scan exclusions

- **Decision:** Add `[activity].exclude` as literal project-relative policy for Git worktree scan suppression, separate from Git ignore and `[verify].exclude`. Rebuild the worker detector baseline whenever that setting changes, without changing the TUI's startup Build/Test Config snapshot.
- **Reason:** A human or future AI can apply a reviewed exclusion without adding per-tick Git tracked-file checks. Rebuilding the baseline prevents exclusion-set changes from becoming false Git Activity refresh hints while preserving existing Build/Test command and freshness-baseline semantics for the session.
