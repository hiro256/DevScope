# Minimal File Browser contract

Status: minimal implementation complete; Windows Terminal dogfood accepted.
This document retains the accepted scope and implementation rationale. Implemented behavior
is summarized in [design.md](design.md); completion is tracked in [roadmap.md](roadmap.md).
Ctrl+F, navigation, Preview scrolling, local reload, Overview restoration, and responsive
resizing were accepted in user-operated Windows Terminal dogfood.

## Role and view lifetime

File Browser is optional, read-only inspection of surrounding project files. It is
neither a permanent Overview panel nor a Plan, Activity, or Evidence source. Browsing
does not classify files into those sources or modify their observation policies.

Open explicitly from Overview only. Ctrl+F is the initial shortcut candidate: it is
currently unused and avoids plain-f accidental activation, Enter Preview toggle, and
Ctrl+Enter Changed File Full Detail. Compared with another plain global key, it makes
the mode change deliberate. Confirm Windows Terminal delivery during implementation
dogfood; do not silently substitute a conflicting shortcut. No opening from Full Detail.

First open starts at the existing project root. Retain the last directory, selected
relative path, and Preview scroll in this session only; reopen refreshes that directory.
This needs only one retained browser state, not a navigation history. If the remembered
directory is missing or no longer safe, reset to root with a short notice. No persistence
or Config. A missing project root shows Unavailable and still allows exit.

Esc returns to Overview; Enter is directory navigation, not Back. Preserve Overview
focus, selections, Preview visibility and scroll independently. On return, apply existing
hidden-panel/removed-target reconciliation if resize or project refresh invalidated them.
The browser has no Overview panel-focus cycle and no new focusable Preview.

## Initial keys

Only Press events and the stated modifiers apply; one event has one meaning.

| Key in File Browser | Action |
| --- | --- |
| Up/Down, j/k | Move entry selection, clamped at list ends |
| Right, plain Enter | Enter selected directory; `..` goes to parent |
| Left | Go to parent; no-op at project root |
| Esc | Return to Overview |
| q | Quit application |
| Ctrl+Up/Down | Scroll visible text Preview by one line, without moving selection |
| r | Re-read current directory and selected Preview |

Right/Enter on a file or unsupported entry is a no-op: selection already previews it.
No external open or Full File Detail. Ctrl+Enter, p, Tab/Shift+Tab, b/t and other unmapped
keys do not fall through to Overview actions. Ctrl+F is not a browser toggle. Route
browser keys before existing global reload/verification handlers. Overview and Changed
File Full Detail keep their current bindings; no global b/t migration is included here.

## Discovery and listing

Choose filesystem listing with built-in safety exclusions. Unrestricted filesystem-wide
discovery exposes internals and pathological trees; Git-centric listing omits useful
ignored or non-Git files. The chosen policy supports both Git and non-Git projects without
Git queries, recursive preload, tree summaries, or directory watching.

Read only the current directory on entry, directory movement, and r. Never scan the
project in advance. Hide `.git` and `.devscope` entries at every level: VCS internals and
local operational state are not the initial inspection surface. Also hide directories
named `target` or `node_modules` to avoid common large generated/dependency trees.
Compare these built-in names ASCII-case-insensitively. Hide them before navigation;
they cannot be reached through a retained directory either. This is visibility policy,
not proof that their contents are generated or safe to delete.

Do not hide `bin` or `obj` initially: those names can hold meaningful project content.
Do not grow a toolchain-directory catalog. Other hidden files remain visible. Never
reuse `plan.exclude`, `activity.exclude`, `verify.exclude`, or Git ignore as browser
visibility policy. No configurable exclusions or filter UI in this slice.

Bound a listing attempt to 4,096 raw directory entries plus one overflow probe, counting
hidden entries too. On overflow, show an explicit incomplete-list notice and sort only
the retained visible subset; do not claim it is the globally first lexical page. This
bounds memory/enumeration work, not OS I/O latency. No paging or background scan framework.

Order: synthetic `..`, real directories, regular files, then unsupported/error entries.
Within each group, sort by case-sensitive native filename lexical order (not locale or
lossy display text); enumeration order is not the displayed order. Keep the original
relative path as identity even when its display needs escaping or lossy conversion.

At root omit `..`. Else place it first, and implement it as a checked parent transition,
never an unchecked `..` filesystem read. On directory change select the first real
entry; select `..` only if there are no real entries, or None for an empty root. Moving
Up can explicitly select `..`. On refresh/reopen retain the selected path and kind when
still present; otherwise apply the same first-real-entry rule. Do not keep per-directory
selection history.

## Safety, Preview, and errors

Validate every directory transition and content read against the fixed project root.
Keep normalized project-relative state; reject absolute paths, non-normal components,
and root escape. The synthetic parent action is the only parent-navigation operation.
Use the existing root interpretation; inspect components without following links before
listing/opening, recheck on use, and verify physical root confinement. Do not rely solely
on canonicalize followed by starts_with. This is an observation boundary, not a claim of
a sandbox against concurrent hostile filesystem replacement.

Show symlink/reparse entries as unsupported links, including internal links, dangling
links, and Windows junctions. Do not navigate, read through them, or resolve their target
for display. Built-in hidden names remain hidden. Show special objects and metadata
failures with concise unsupported/error reasons; never open special files.

Regular file selection provides a passive current-content Preview with a project-relative
path/title and explicit `Mode: File content`. No Git diff or Markdown rendering. Use the
same 64 KiB prefix plus one detection byte as Changed File inspection, NUL detection,
UTF-8 validation, safe handling of a split UTF-8 boundary, and an explicit truncation
marker. Validation covers the bounded sample, not unread bytes. Bound rendered content
as well; display control characters safely rather than executing terminal controls.
Explain binary/invalid UTF-8, missing, unsafe, unsupported, or unreadable content without
dumping absolute OS paths. Directory selection shows only `Directory` and relative path;
`..` shows parent-navigation context. No recursive child summary.

A directory-read failure shows an in-view error, clears stale entries/content, and keeps
Left/Esc/q and a synthetic parent when below root available. For individual metadata failures,
retain a named error entry when possible. A listing iterator error marks the result
incomplete; it must not look like a complete empty directory. r retries. Do not crash or
reuse old Preview text as if a failed observation succeeded.

## Responsive and refresh contract

Large/Medium use file list left and passive Preview right when existing usable pane
widths allow it; start with the current 45/55 split and Preview availability thresholds.
Small or narrower layouts show the list only. Too-small terminals use compact safety
messaging with Esc Back and q Quit. Do not raise Overview minimum dimensions or add a
narrow-screen Full Preview mode.

Browser Preview has its own scroll and no visibility toggle. It does not share Overview
`preview_visible` or `preview_scroll`. Reset scroll on directory/selection change or a
refreshed target identity (relative path plus kind) change. For the same target, retain
and clamp against the current text and viewport. Resize preserves directory/selection;
hidden Preview ignores Ctrl+Up/Down and retains its offset for clamping when visible again.
File reads happen on selection/refresh, not render or scroll; a newly visible Preview
may load if not yet observed. No automatic browser listing/content refresh from polls or
Git changes. Existing project observation can continue independently.

Use view-local navigation hints. Preview scroll hints should be placeable beside content
for the later contextual-actions task; no action framework or footer redesign now.

## Implementation seam and acceptance

Current App uses `DetailTarget` for Changed File Full Detail and independent Overview
selection/Preview state. Add only concrete Overview / File Browser / Changed File Detail
dispatch, optionally a small view enum; no generic navigation stack. Browser-local state
needs current relative directory, entries (path/name/kind), optional selection, Preview
observation, scroll, and listing error/truncation state. Parent, directory, regular file,
unsupported link, and unsupported/error are sufficient entry categories. Names are not
a public API contract. Route I/O through event handling, keeping render free of reads.

Changed File inspection and File Browser are two concrete safe-text consumers. At
implementation time consider a minimal internal extraction of the current reader from
`src/progress/git.rs`; do not call `collect_git_file_inspection` from the browser, because
it collects Git diff first. Keep Git preference in its existing wrapper. No shared public
filesystem, plugin, or generic Evidence API is defined by this proposal.

Implementation acceptance needs tests for root/parent/order/exclusions, bounded listing,
link and read-error handling, bounded UTF-8 Preview, view-local keys, selection/scroll
reset, Overview restoration, and responsive resize. Windows dogfood must confirm Ctrl+F,
directory navigation, Preview scrolling, r recovery, and return to unchanged Overview
state. Exploration closure is not implementation or dogfood completion.

## Non-goals

No edit/create/delete/rename/copy/move/chmod, staging or other Git operations, external
open/editor launch, search/fuzzy search, sort options/filter UI, bookmarks/tabs/history
stack, tree view/recursive preload, watcher, syntax highlighting, image/archive Preview,
Markdown rendering, binary hex view, arbitrary exclusion Config, or theme work. No Full
File Detail, new Overview panel, or file-manager replacement.
