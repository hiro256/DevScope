# Roadmap

## v0.1.0

### Project bootstrap

- [x] Rust project initialization
- [x] Ratatui/Crossterm setup
- [x] Basic application loop
- [x] Initial TUI layout

### Markdown progress

- [x] Markdown file discovery
- [x] Markdown task checkbox parsing
- [x] Completed/total task calculation
- [x] Multiple Markdown files support

### Git activity

- [x] Git repository detection
- [x] Git status
- [x] Changed file count
- [x] Recent commits

### TUI

- [x] Overview screen
- [x] Progress display
- [x] Task summary
- [x] Git activity panel
- [x] Keyboard navigation
- [x] Responsive terminal resize handling

### Quality

- [x] Unit tests for Markdown parser
- [x] Unit tests for progress calculation
- [x] Basic TUI rendering tests
- [x] Windows manual verification

## v0.2.0 - Live Observation

### Refresh infrastructure

- [x] Project snapshot / refresh core
- [x] Manual reload with `r`

### Change detection

- [x] Polling scheduler
- [x] Markdown change detection
- [x] Git/worktree change detection
- [x] Git metadata change detection

### Live updates

- [x] Selective automatic refresh
- [x] Refresh status / last update
- [x] Changed Files panel

### Quality

- [x] No-change polling avoids unnecessary Git collection
- [x] Markdown-only changes do not unnecessarily refresh Git
- [x] Git changes are reflected automatically
- [x] Windows live-update verification

## v0.3.0 - Build/Test Evidence

### Evidence model

- [x] Evidence architecture and execution model
- [x] Build/Test result/state model
- [x] Initial Evidence source boundary
- [x] Build/Test freshness / stale model

### Initial source

- [x] Cargo Build/Test evidence source
- [x] Non-blocking evidence execution
- [x] Manual evidence execution

### TUI

- [x] Evidence state integration
- [x] Evidence summary display

### Live behavior

- [x] Evidence becomes stale after relevant project changes

### Quality

- [x] Evidence model/source tests
- [x] Windows Build/Test evidence verification

## Post-MVP

### Completed experiments

- [x] Minimal read-only CLI experiment
- [x] Current Work CLI experiment
- [x] DevScope Skill experiment
- [x] Agent integration reassessment
- [x] Config file
- [x] TUI panel focus experiment
- [x] Current Work TUI experiment

### Workflow packaging

- [x] Package and dogfood DevScope as a Codex Skill

### TUI

- [x] Validate Detail View drill-down for focused panels

### TUI refinement

- [x] Refine NOW presentation from explicit Current Work Active state
- [x] Refine Project Progress visual hierarchy and progress indicators
- [ ] Refine keyboard navigation and Preview controls
- [ ] Show file content when Changed File diff is unavailable
- [ ] Explore read-only project File Browser with Preview
- [ ] Refine Preview content and density across focused panels
- [ ] Dogfood five-second project-state understanding

The planned keyboard slice keeps Preview passive: Left/Right move among focusable panels,
Tab/Shift+Tab remain compatible, and Up/Down or j/k select within a panel. Enter is a
candidate Preview toggle (with p retained), Ctrl+Up/Down scroll Preview without moving
focus, and Ctrl+Enter is the candidate Full Detail shortcut. Confirm event handling and
Windows Terminal behavior before finalizing the bindings; Esc still returns from Full
Detail and q quits.

Changed File inspection should label current file content separately from a Git diff
when a diff is unavailable, including for Added files. Keep it read-only, rooted in the
project, text-oriented, and bounded in reading and rendering; do not follow symlinks or
path traversal outside the root. Explain binary, oversized, unreadable, and read-error
cases rather than displaying misleading content. Exact limits remain for implementation.

The File Browser is an experiment in a separate read-only inspection view, not a
permanent Overview panel or a general file manager. Start at the project root and
evaluate directory/parent navigation, file selection, text Preview and scrolling,
responsive layout, and root confinement. Editing, file operations, staging, search,
syntax highlighting, image or archive Preview are outside the initial scope. Compare
filesystem-wide, Git-centric, and filesystem browsing with built-in safety exclusions;
the last is the initial candidate. Do not reuse plan, activity, or verify exclusions as
File Browser visibility policy. Added-file inspection may inform later safe text-file
observation reuse, without defining a shared API now.

Refine common Preview density after these inspection slices. The final five-second
dogfood tests whether Overview alone reveals project state quickly; Preview confirms a
selection, while File Browser provides optional surrounding-file inspection.

### AI workflow refinement

- [x] Define AI-maintained Config workflow
- [x] Dogfood Config maintenance through the DevScope Skill

### Core observation

- [x] Refine Plan source / task discovery semantics
- [x] Configurable Build/Test command resolution and freshness exclusions

- [x] Explore verification integration for Build/Test Evidence
- [x] Explore Artifact Evidence as a second Evidence source
- [x] Progress history experiment
- [x] Refine Git worktree change detection boundary

### External surfaces

- [ ] VS Code integration
- [ ] Web/API frontend
Exploratory implementation candidates are tracked in [backlog.md](backlog.md).
