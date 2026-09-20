# Development Workflow

This guide is a small safety-oriented workflow for changes in the DevScope repository.
It complements `AGENTS.md`; it does not replace repository-specific instructions.

## 1. Orient before editing

Use the built executable when available:

```powershell
.\target\debug\devscope.exe context
```

Then read only the repository documents and source files relevant to the requested work.
Keep Plan, Current Work, and Evidence separate.

## 2. Make one logical edit at a time

For a precise text replacement, keep the expected old and new fragments in temporary
UTF-8 files outside the repository, then run a dry run before writing:

```powershell
.\scripts\replace-exact.ps1 `
  -File src\change.rs `
  -OldFile $env:TEMP\old-fragment.txt `
  -NewFile $env:TEMP\new-fragment.txt

.\scripts\replace-exact.ps1 `
  -File src\change.rs `
  -OldFile $env:TEMP\old-fragment.txt `
  -NewFile $env:TEMP\new-fragment.txt `
  -Apply
```

The helper preserves UTF-8 BOM presence and the target file's LF or CRLF line endings.
It refuses empty fragments, whole-file replacement, mixed target line endings, and any
old fragment that does not occur exactly once.

After each logical edit, run:

```powershell
.\scripts\check-change.ps1 -Rust
```

Use `-Rust` for Rust source changes. For documentation-only edits, omit it.

## 3. Distinguish validation from executable rebuilds

```text
cargo check                  source compilation only
cargo test                   tests
cargo build                  refreshes target\debug\devscope.exe
devscope verify build/test   final DevScope-observed Evidence boundary
```

Use `cargo build` before testing TUI behavior with the debug executable. `cargo check`
does not replace that executable.

## 4. Keep temporary diagnostics outside the repository

Write timing logs under `$env:TEMP`, not under the project. Record the log path and the
PID of every TUI process started for a diagnostic. At cleanup, stop only the recorded
PID, remove only the recorded temporary files, then run `git diff --check` and
`git status --short`.

## 5. Finish quality checks proportionately

For Rust changes, follow `AGENTS.md` and finish with:

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
.\target\debug\devscope.exe verify build
.\target\debug\devscope.exe verify test
git diff --check
```

For documentation-only changes, inspect the diff and run `git diff --check`.

## 6. Preflight a commit or push

Before a commit or push, review the exact destination and content:

```powershell
git status --short
git branch --show-current
git remote -v
git diff --cached --check
```

Keep staging explicit. A push is a separate action after the intended remote and branch
have been confirmed.
