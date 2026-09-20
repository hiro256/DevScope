# Git Worktree Candidate Cache Experiment

## Status

Exploratory prototype result, 2026-09-20. This note evaluates a tracked-path cache for `GitWorktreeChangeDetector`. It does not change the runtime detector, polling, Activity collection, or `GitMetadataChangeDetector`.

## Question

The earlier Git-aware candidate proposal used:

```text
git ls-files -c -o --exclude-standard -z
```

on every tick, followed by metadata checks. This experiment asks whether caching the tracked `-c` paths and discovering only `-o --exclude-standard` paths on every tick is a useful next step on Windows.

The proposed cache remains only a candidate-set optimization. It must not become a generic filesystem watcher, parse `.gitignore`, or replace the separate Git metadata detector.

## Prototype boundary

A test-only prototype in `src/change.rs` adds no production API. It:

- reads tracked candidates with `git ls-files -c -z`;
- parses NUL-delimited Git output into `PathBuf` values;
- retains metadata stamps for those paths;
- refreshes that cache only after an index-changing Git transition; and
- reads `git ls-files -o --exclude-standard -z` every tick for non-ignored untracked paths.

It does not connect to the poll scheduler or alter the existing recursive runtime detector. `.git` remains outside the worktree detector; `GitMetadataChangeDetector` remains responsible for index, `HEAD`, refs, and repository-location transitions.

## Boundary tests

The prototype tests establish the following candidate-set behavior:

- tracked edit and deletion change the cached tracked metadata result;
- tracked rename and staged deletion require a cache refresh after the index transition;
- a branch switch at the same commit is detected by Git metadata while leaving tracked candidates unchanged;
- a staged addition changes index metadata and appears in `-c` after refresh;
- root, nested, and new top-level untracked files require the per-tick `-o` discovery path;
- root ignore rules, nested `.gitignore`, and negation are honored by Git;
- an explicitly tracked file under an ignored `target/` directory remains visible; and
- explicitly tracked `.devscope/work/current.md` and `.devscope/config.toml` remain visible despite the normal local-work ignore rule.

This keeps the intended DevScope boundary: ignored local state does not normally create Activity work, but deliberately tracked state is still Git-visible. Current Work and Config continue to have dedicated observers.

## Alternatives

| Option | Candidate discovery | Tick cost and correctness | Result |
| --- | --- | --- | --- |
| A. Full Git candidate query | `-c -o --exclude-standard` every tick | Correct Git ignore semantics; repeats tracked-path process output and parsing. | Baseline for comparison. |
| B. Cache tracked candidates | `-c` at baseline/index change; `-o --exclude-standard` every tick | Preserves correctness for new untracked paths, but still metadata-checks every tracked path. | Tested; not a standalone win. |
| C. Cache plus cheap structural detection | Cached paths plus a separate directory/entry signal | Would need a new correctness argument for untracked additions and filesystem topology. | Deferred; this experiment does not introduce it. |
| D. Continue current detector | Recursive baseline with the temporary `target` safeguard | No per-tick Git process; existing semantics and known generated-output limitation. | Keep for now. |

## Windows measurement

The reproducible ignored test `candidate_cache_timing_experiment` used a temporary Git repository with 5,001 tracked files, 5,000 ignored files, and 25 untracked files. It ran five samples on Windows and reports median and maximum. The phases separate Git process execution, NUL parsing plus `PathBuf` creation, metadata stamping, and total tick work.

| Strategy / phase | Median | Maximum |
| --- | ---: | ---: |
| A full: Git process | 41.24 ms | 54.10 ms |
| A full: parse / `PathBuf` | 1.36 ms | 1.44 ms |
| A full: metadata | 164.72 ms | 199.61 ms |
| A full: total | 204.30 ms | 255.13 ms |
| B cached: tracked metadata | 174.61 ms | 198.20 ms |
| B cached: untracked Git process | 44.98 ms | 49.74 ms |
| B cached: untracked parse / `PathBuf` | 0.02 ms | 0.02 ms |
| B cached: untracked metadata | 1.02 ms | 1.11 ms |
| B cached: total | 220.62 ms | 248.93 ms |

The cache removes most tracked-path output parsing, but parsing was already small. It does not remove the dominant tracked metadata walk, and it retains one Windows Git process for untracked discovery. In this fixture, B is slightly slower at the median than A.

For context, the preceding experiment measured the current DevScope worktree scan at 3.83 ms after the temporary `target` safeguard, while a 5,000-file generic recursive fixture took 223.20 ms. The cache prototype does not provide enough evidence to replace that safeguard or to justify an immediate runtime change.

## Decision

Do not adopt the tracked-candidate cache as an independent runtime optimization. It correctly retains the necessary Git ignore and untracked-path boundary, but its measured cost is dominated by per-file metadata checks and Windows process spawn.

Keep the existing runtime detector and its temporary `target` safeguard. Retain the test-only prototype as a reproducible boundary and timing reference. A later experiment may revisit a Git-aware candidate set only together with a demonstrably correct, cheaper signal for tracked metadata changes; it must re-measure a larger real repository and preserve the separate Git metadata detector.

No roadmap checkbox changes in this experiment.
