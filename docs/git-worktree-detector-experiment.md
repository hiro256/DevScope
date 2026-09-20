# Git Worktree Detector Experiment

## Status

Exploratory design result, 2026-09-20. This document records an observation boundary and a follow-up implementation candidate; it does not change the runtime detector.

## Problem and current behavior

`GitWorktreeChangeDetector` is a heuristic that answers only whether Git Activity may need recollection. It is not a generic project filesystem watcher. Plan, Config, Current Work, Build/Test Freshness, Artifact observation, and Git metadata have separate observers.

The current detector keeps a sorted recursive baseline of entry paths, kinds, file length and modification time, and direct-child names for directories. It uses `symlink_metadata`, records but does not follow symlinks, compares known entries before a full rescan, and detects file edits, additions, deletions, and renames through path or entry differences. It skips `.git` and, as a temporary performance safeguard, directories named `target`. It does not otherwise implement Git ignore semantics.

`GitMetadataChangeDetector` separately observes the Git locator, `HEAD`, the resolved symbolic ref, `index`, and `packed-refs`. It covers staging, commits, branch switches, repository creation/removal, and linked-worktree metadata changes that a worktree path detector must not own.

## Fixture findings

A temporary Git fixture contained tracked `src/main.rs` and `docs/roadmap.md`; ignored `target`, `node_modules`, `bin`, `obj`, `.venv`, and `generated`; a nested `subdir/.gitignore` for `cache`; `*.log` with `!important.log`; and untracked `notes.txt`.

- Git reported only `important.log` and `notes.txt` as untracked. It omitted every ignored generated path.
- `git check-ignore` honored the root rules, nested `subdir/.gitignore`, and the negation rule: `important.log` and `notes.txt` were not ignored.
- A staged edit appeared as `M `; a staged rename and deletion appeared as `R` and `D`.
- A plain porcelain status does not by itself explain every metadata transition. The existing Git metadata detector remains the owner of index and ref transitions.

## Candidate assessment

| Candidate | Finding |
| --- | --- |
| Recursive scan plus hardcoded directories | Fast only while its exclusion list happens to match a project. It does not scale safely across ecosystems. |
| DevScope `.gitignore` parser | Rejected. Nested files, negation, `**`, `.git/info/exclude`, global excludes, and `core.excludesFile` belong to Git. |
| `git check-ignore` over scanner paths | Correct when batched, but still requires discovering every filesystem path first. Per-path subprocesses are not viable. |
| `git status --porcelain` as detector | Correctly follows Git state, but duplicates the first command of Activity collection and does not alone reveal content changes that preserve a status letter. |
| `git ls-files -c -o --exclude-standard -z` plus metadata stamps | Preferred future direction. Git provides the candidate set of tracked plus non-ignored untracked paths; DevScope can retain inexpensive path metadata checks for those candidates. |
| Poll full Activity collection | Rejected for now. It repeats status, numstat, HEAD, and log work every tick rather than remaining a detector. |

## Windows measurements

These are local exploratory medians over nine subprocess runs; they include Windows process-spawn cost and are not a universal performance contract.

| Case | Current scan / target safeguard | `git status` | `git ls-files` | batched `git check-ignore` |
| --- | ---: | ---: | ---: | ---: |
| Small fixture | 1.4 ms recursive scan (20 files) | 48.58 ms | 44.27 ms | 43.38 ms |
| Medium fixture | 223.20 ms recursive scan (5,000 files); 0.11 ms when they are under `target` | 45.87 ms | 41.83 ms | 113.78 ms for 5,000 paths |
| DevScope | 3.83 ms after `target` exclusion | 53.76 ms | 47.01 ms | 44.71 ms |

Before the `target` safeguard, DevScope's 24,501 `target` files made the periodic worktree check average about 2.02 seconds. The temporary safeguard remains necessary until Git-aware candidate discovery is delivered.

## DevScope local state boundary

- `.git` remains mandatory excluded: Git metadata has its own detector.
- `.devscope/evidence` and `.devscope/history` are DevScope-owned local observations and history. Their mutation alone should not trigger Activity recollection; a future slice should keep them out of Git-aware candidates and ensure their local-only policy is reflected in ignore setup.
- `.devscope/work` is not a mandatory Activity exclusion. Its normal local-only form is ignored, but if a user deliberately tracks it, Git Activity should remain able to show that tracked change. Current Work still has its dedicated detector.
- `.devscope/config.toml` is not excluded. It can be tracked and must remain visible to Git Activity; ConfigChangeDetector independently owns Config reload behavior.

These Activity exclusions do not apply to Build/Test Freshness, Plan discovery, or Artifact observation. Git ignore semantics are not Freshness semantics, and no `[activity].exclude` Config is proposed in this experiment.

## Recommendation and deferred slice

Choose a Git-aware detector based on `git ls-files -c -o --exclude-standard -z` plus existing-style metadata stamps for the returned paths. Retain `GitMetadataChangeDetector` as a separate low-cost complement. Do not add language-specific directory names, parse ignore files, poll full Activity collection, or add user Config first.

The next slice should prototype that candidate set behind the existing detector boundary, test tracked edits, untracked additions, ignored paths, nested ignore, negation, rename/deletion, staging, and branch switches, then repeat Windows measurements on a larger real repository. It must not alter Freshness exclusions or widen the detector into a generic watcher.
