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
- [x] Show file content when Changed File diff is unavailable
- [x] Explore read-only project File Browser with Preview
- [ ] Implement minimal read-only project File Browser
- [ ] Refine contextual Detail actions and Evidence execution
- [ ] Support Build verification profiles for Debug and Release
- [ ] Refine Preview content and density across focused panels
- [ ] Refine TUI visual alignment for side-by-side Codex use
- [ ] Dogfood five-second project-state understanding

The keyboard slice implements Left/Right panel navigation, Tab/Shift+Tab compatibility,
Up/Down or j/k selection, Enter/p Preview toggle, Ctrl+Up/Down passive Preview scrolling,
and Ctrl+Enter Full Detail. Plain Enter or Esc returns from Full Detail; Esc quits from
Overview, and q quits. Closure is pending Windows Terminal dogfood, especially distinguishing Ctrl+Enter
from plain Enter; no alternative shortcut is selected without that observation.

Changed File inspection now prefers Git diff and explicitly labels safe current UTF-8
file content when a diff is unavailable, including for Added files. Reads are read-only,
project-root confined, reject symlink/reparse-point components, and are bounded to 64 KiB
plus one detection byte. Truncation and unsupported/read-error reasons are explicit.
Windows Terminal dogfood confirmed content labeling, Preview scrolling, and Full Detail
navigation with an untracked UTF-8 text file.

File Browser exploration is complete; implementation remains separate and follows the
[minimal contract](file-browser-proposal.md). Build a separate read-only view starting
at project root, with session-local last directory, bounded on-demand filesystem listing,
directory-first ordering, checked parent navigation, and no symlink/reparse traversal.
Use Left/Right plus Enter for directory navigation, Esc Back, passive bounded text Preview,
and Ctrl+Up/Down scroll; Large/Medium split and Small list-only preserve root confinement
without file operations. Ctrl+F is the entry candidate for Windows implementation dogfood.
Browser visibility has its own built-in exclusions, not Plan/Activity/Verify policies.
Added-file inspection may inform minimal internal safe-text reuse at implementation time;
no shared public API is defined now.

After the inspection surfaces, plan contextual actions: the global footer owns navigation
and application-wide controls, while passive Detail / Preview shows only actions available
for the current selection. Proposed verification execution requires Evidence focus, an
actually visible Detail / Preview, and a selected executable target. Space is the candidate
Run key, subject to implementation dogfood; Enter remains Preview toggle and Ctrl+Enter
deeper inspection. Move away from global b/t execution, deciding during implementation
whether to remove those shortcuts immediately or temporarily retain Evidence-focus-only
compatibility. Running targets must disable Run or show Running instead.

Keep contextual scroll and action hints near their content, with availability matching
behavior: consider Ctrl+Up/Down for scrollable Preview and Ctrl+Enter for supported deeper
inspection, omitting unavailable actions and unnecessary scroll hints. Full Detail retains
Up/Down or j/k scrolling, Enter/Esc Back, and q Quit. Future File Browser Preview should
follow the same ownership without adding browser features here. This lightens the global
footer rather than listing every shortcut there; exact wording/layout remain open. This
task owns interaction and action placement, while later visual alignment owns borders,
spacing, typography, and exact footer styling.

Next, explore Build Debug and Build Release as independent verification targets on that
action model, keeping Test as one target. Distinguish each Build profile's state, latest
result, freshness, command, and persisted identity. Start with a small Build-plus-variant
extension rather than an arbitrary named-command framework, workflow engine, plugin
execution API, or stabilized generic Evidence API. Check possible future Test variants
without including their generalization in this task. Preserve source-specific Evidence
semantics and the UI / progress-analysis boundary.

The Config schema remains undecided. Investigate existing single-build `[verify.build]`
compatibility, Cargo auto-detection, command resolution, persistence keying and format
compatibility, per-profile freshness, and CLI/TUI consistency before choosing it. Keep
behavior toolchain-neutral: Cargo Debug/Release and .NET Debug/Release command differences
belong in configuration/resolution, not tool-specific core or UI models.

Refine common Preview density after inspection, contextual actions, and Build profiles, then evaluate visual
alignment for side-by-side Codex CLI use. The initial candidate keeps Project Progress
as a clear status card while reducing always-on borders around Tasks, Evidence, Changed
Files, and Recent Commits. Consider flatter navigation sections, restrained title or
local marker emphasis for focus, a distinct Preview / Detail inspection area without
excessive nested cards, and a lighter footer with optional future help-surface grouping.
Use color mainly for source/status cues, never as the only carrier of meaning. Rounded
Unicode corners on selected major frames are a candidate, not a contract; frame choice,
focus treatment, and Windows Terminal/font compatibility remain for implementation dogfood.

This is presentation refinement, not a Codex UI replica or an information-structure
redesign. Preserve NOW, Project Progress, focus versus selection, passive Preview,
Full Detail, source/status marker semantics, and the Overview-to-Preview-to-deeper-
inspection hierarchy. Preserve Large/Medium/Small behavior, hidden-panel focus
reconciliation, and Preview minimum width/height behavior without substantially widening
the minimum terminal size; validate narrow Windows Terminal and side-by-side use.
Themes, arbitrary color customization, syntax highlighting, animation, mouse interaction,
graphical widgets, and terminal-specific hacks are outside this task.

The final five-second dogfood follows visual refinement and tests whether Overview alone
reveals project state quickly: NOW, project health, and changed areas should be easy to
scan, selected detail easy to locate, and eye movement natural beside Codex. Preview
confirms a selection, while File Browser provides optional surrounding-file inspection.

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
