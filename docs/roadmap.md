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
- [x] Implement minimal read-only project File Browser
- [x] Refine contextual Detail actions and Evidence execution
- [ ] Support Build verification profiles for Debug and Release
- [x] Refine Preview content and density across focused panels
- [x] Refine TUI visual alignment for side-by-side Codex use
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

File Browser exploration and minimal implementation are complete, following the
[minimal contract](file-browser-proposal.md). The separate read-only view starts
at project root, with session-local last directory, bounded on-demand filesystem listing,
directory-first ordering, checked parent navigation, and no symlink/reparse traversal.
It uses Left/Right plus Enter for directory navigation, Esc Back, passive bounded text Preview,
and Ctrl+Up/Down scroll; Large/Medium split and Small list-only preserve root confinement
without file operations. Windows Terminal dogfood validated Ctrl+F entry, navigation,
Preview scrolling, browser-local reload, Overview restoration, and responsive resizing.
Browser visibility has its own built-in exclusions, not Plan/Activity/Verify policies.
Changed File inspection and File Browser reuse the same narrow safe-text reader;
no generic filesystem or Evidence API was introduced.

Contextual actions are implemented: the global footer owns navigation
and application-wide controls, while passive Detail / Preview shows only actions available
for the current selection. Space verification requires Evidence focus, an actually visible
Overview Preview, and a selected runnable Build/Test target, with no verification active.
Global b/t execution is removed without compatibility aliases; CLI verification is unchanged.
Enter remains Preview toggle and Ctrl+Enter deeper inspection. Running/Unavailable targets
and Artifact do not advertise Run. Windows Terminal dogfood accepted execution gating,
contextual hints, and the preserved navigation controls.

Fixed Preview hint rows keep actions visible while content scrolls, and scroll limits
exclude the hint row. Ctrl+Up/Down appears only for scrollable content, including File Browser;
Changed Files advertises Ctrl+Enter only for a selected file. Full Detail retains
Up/Down or j/k scrolling, Enter/Esc Back, and q Quit. Contextual shortcuts no longer clutter
the global footer. This task owns interaction and action placement; later visual alignment owns borders,
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

Preview density refinement is complete: compact metadata fields, source-grounded Task
context, separate Evidence outcome/freshness, and width-aware wrapping preserve detail
and fixed contextual actions. Half-screen Windows Terminal dogfood accepted the result;
Build profiles remain a separate pending task.

Visual alignment is complete after user-accepted side-by-side Windows Terminal dogfood.
Project Progress and Preview retain their frames; Tasks, Evidence, Changed Files, and
Recent Commits are flat sections separated by whitespace. `▌` plus a bold title marks
focus, independently of `>` selection. Navigation truncation uses `…`, and Changed Files
Preview no longer repeats its title path in a body field. A follow-up accepted in Windows
Terminal sizes Evidence to its items and lets Large Tasks/Changed Files use spare rows
while keeping useful Recent Commits space. Responsive thresholds, Preview geometry,
interaction, and footer ownership are preserved. No colors,
rounded corners, or additional help surface were introduced.

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

### Product clarity and onboarding

- [x] Align public documentation and first-use onboarding with current implementation
- [ ] Reassess UI and event-loop module boundaries after TUI refinement
- [ ] Explore package-manager distribution after onboarding refinement

The completed documentation slice covers current controls, implementation status, the Windows binary
first-use path, and the existing Skill workflow. Five-second project-state understanding
remains the separate TUI dogfood task above; module reassessment and distribution exploration
do not authorize refactoring or packaging implementation in this slice.

### External surfaces

- [ ] VS Code integration
- [ ] Web/API frontend
Exploratory implementation candidates are tracked in [backlog.md](backlog.md).
