# Working on DevScope

Use the `devscope` skill for DevScope workflow, Current Work, and DevScope-observed verification when applicable.

Before making changes, read only the project documentation relevant to the task.

Use:

* `docs/roadmap.md` when the task comes from the roadmap or changes roadmap status.
* `docs/design.md` when the task affects architecture or established design.
* `docs/decisions.md` when the task depends on or changes a recorded design decision.
* `README.md` when user-facing behavior, setup, or documented usage is relevant.

Do not read project documents broadly when the task context already identifies the relevant implementation and constraints.

Treat files and symbols identified in the task as the primary change surface. Inspect direct dependencies when necessary, but do not broaden the investigation without a concrete implementation reason.

* Record substantial design changes in `docs/decisions.md`.

* Update completed tasks in `docs/roadmap.md` using Markdown checkboxes when the work corresponds to a roadmap task.

* Verify behavior on Windows.

* After Rust code changes, use DevScope-observed verification for the final quality boundary when it is available:

  ```powershell
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  devscope verify build
  devscope verify test
  git diff --check
  ```

  `devscope verify build` runs `cargo check`, and `devscope verify test` runs `cargo test` while updating local Observed Evidence. Direct `cargo test` remains appropriate for isolated tests, debugging, or changes to the verify command itself; finish with `devscope verify test` when practical so Evidence reflects the final verification.

* Do not delete or disable tests to make checks pass.

* Do not add large, unrequested features.

* Keep dependencies to the minimum necessary and avoid `unsafe` Rust by default.

* Keep the UI layer separate from progress-analysis logic.

* Keep Codex-specific behavior out of the core; future integrations must be adapters.

Use the documents in `docs/` for detailed direction rather than expanding this file into a specification.

`docs/backlog.md` contains uncommitted implementation candidates. Do not implement backlog items unless they have been promoted to `docs/roadmap.md`.
