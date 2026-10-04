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

- [x] Evidence Full Detailの失敗出力の初期表示位置を改善する

- [x] Refine NOW presentation from explicit Current Work Active state
- [x] Refine Project Progress visual hierarchy and progress indicators
- [x] Refine keyboard navigation and Preview controls
- [x] Show file content when Changed File diff is unavailable
- [x] Explore read-only project File Browser with Preview
- [x] Implement minimal read-only project File Browser
- [x] Refine contextual Detail actions and Evidence execution
- [x] Support Build verification profiles for Debug and Release
- [x] Refine Preview content and density across focused panels
- [x] Refine TUI visual alignment for side-by-side Codex use
- [x] Dogfood five-second project-state understanding
- [x] Experiment with aligned Evidence columns and transient process-state cues

The follow-up narrows width-free bold emphasis from full content to state columns,
state text without its symbol, freshness (or visible status text), then normal over three seconds.
Windows Terminal dogfood accepted this five-phase refinement, including the return to normal.
Windows Terminal dogfood completed for path-keyed shrinking emphasis for changed/new Git
rows, without changing Activity semantics, ordering, layout, or the Evidence runtime.
The user has confirmed visual acceptance of the Changed Files emphasis.
Follow-up: emphasize newly added incomplete Tasks using source-path/text occurrence counts,
without treating line-number shifts as additions. Windows Terminal checks confirmed new-task
emphasis and no emphasis on remaining rows after completion. The user confirmed Tasks display acceptance.
Follow-up: emphasize newly prepended Recent Commits by ID, quietly resetting the baseline
when the previous head is absent. Windows Terminal checks confirmed new-commit emphasis
and expiry, quiet unchanged reload/history switch, and emphasis for a new commit after
that switch. The user confirmed Recent Commits visual acceptance. The recovery fix keeps
Unavailable/NotRepository distinct from available empty history, restoring a quiet baseline
without suppressing a genuinely first commit. Recovery, multiple-addition, and independent-timing
checks passed automatically; no separate manual coverage of these cases is claimed.
The transient-emphasis experiment is complete.

Keyboard refinement is complete after user-confirmed Windows Terminal dogfood in the
real DevScope repository. Left/Right and Tab/Shift+Tab panel navigation, local Up/Down/j/k
selection, and reliable Enter/p Preview toggle versus Ctrl+Enter Full View were confirmed.
Ctrl+Up/Down scrolls passive Preview without moving focus/selection; target changes reset
scroll while hiding/showing the same target preserves it. Full View returns to its originating
Overview or Browser with Enter/Esc, without input leakage. Browser-local controls,
Evidence Space execution gates, removed global b/t execution, and side-by-side responsive
focus/Preview behavior passed. Unavailable-target gating was checked by the existing
unit test, not manual dogfood; the current repository has available Build/Test commands.
No Rust changes or alternative shortcuts were needed for this closure.

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

Build profiles are implemented as three fixed process targets: Build Debug, Build Release,
and Test, each with independent state, command, freshness, and latest persisted result.
Existing `[verify.build]` and `verify build` retain Debug semantics; `[verify.build.release]`
and `verify build release` select Release. Cargo defaults are `cargo check`,
`cargo check --release`, and `cargo test`; configured commands are never given inferred
toolchain flags. The existing v1 state file keeps legacy `build`/`test` and adds
`build-release`. CLI/context and TUI selection expose the three targets independently,
with one active verification process. No generic command registry or Test profiles were added.
User-confirmed Windows Terminal dogfood passed: all targets remain visible side-by-side,
Space executes the selected command with independent results and a single-active gate,
a source change stales all completed targets, and rerunning only Release makes only
Release Fresh. Restart restored Debug/Stale, Release/Fresh, and Test/Stale correctly.

Preview density refinement is complete: compact metadata fields, source-grounded Task
context, separate Evidence outcome/freshness, and width-aware wrapping preserve detail
and fixed contextual actions. Half-screen Windows Terminal dogfood accepted the result;
Build profiles were completed separately as recorded above.

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

Windows Terminal side-by-side five-second dogfood and follow-up confirmation passed
for active/unset NOW, partial Plan/Work progress, Passed/Fresh, Passed/Stale, Test failure,
multiple Tasks/Changed Files, Preview ON/OFF, and Large/Medium layouts. NOW drew attention
first; `(stale)` and Passed/Failed markers distinguished freshness from outcome.
The observed Work-marker ambiguity was resolved with `[Work parent]` for association,
while NOW alone denotes explicit Active. The header now identifies the observed directory
on the left and shows refresh source plus recorded local clock time on the right.
The user accepted the clearer layout without additional Overview panels or color.
Changed paths are discoverable; understanding unfamiliar areas or actual edits still
requires project knowledge or diff inspection, not a five-second Overview summary.

### AI workflow refinement

- [x] Define AI-maintained Config workflow
- [x] Dogfood Config maintenance through the DevScope Skill
- [x] Refine DevScope Skill work lifecycle

### Core observation

- [x] Refine Plan source / task discovery semantics
- [x] Configurable Build/Test command resolution and freshness exclusions

- [x] Explore verification integration for Build/Test Evidence
- [x] Explore Artifact Evidence as a second Evidence source
- [x] Progress history experiment
- [x] Refine Git worktree change detection boundary

### Product clarity and onboarding

- [x] Align public documentation and first-use onboarding with current implementation
- [x] Reassess UI and event-loop module boundaries after TUI refinement
- [ ] Explore package-manager distribution after onboarding refinement

The completed documentation slice covers current controls, implementation status, the Windows binary
first-use path, and the existing Skill workflow. Five-second project-state understanding
remains the separate TUI dogfood task above; module reassessment and distribution exploration
do not authorize refactoring or packaging implementation in this slice.

### External surfaces

- [ ] VS Code integration
- [ ] Web/API frontend
Exploratory implementation candidates are tracked in [backlog.md](backlog.md).
