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

- [x] Minimal read-only CLI experiment
- [x] Current Work CLI experiment
- [x] DevScope Skill experiment
- [x] Agent integration reassessment
- [x] Config file
- [ ] Task weighting
- [ ] Progress history
- [ ] VS Code integration
- [ ] Web/API frontend

Exploratory implementation candidates are tracked in [backlog.md](backlog.md).
