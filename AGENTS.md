# Working on DevScope

Before making changes, read `README.md`, `docs/design.md`, `docs/roadmap.md`, and
`docs/decisions.md`. Confirm the applicable roadmap task before implementation.

- Record substantial design changes in `docs/decisions.md`.
- Update completed tasks in `docs/roadmap.md` using Markdown checkboxes.
- Verify behavior on Windows.
- After Rust code changes, use DevScope-observed verification for the final quality
  boundary when it is available:

  ```powershell
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  devscope verify build
  devscope verify test
  git diff --check
  ```

  `devscope verify build` runs `cargo check`, and `devscope verify test` runs
  `cargo test` while updating local Observed Evidence. Direct `cargo test` remains
  appropriate for isolated tests, debugging, or changes to the verify command itself;
  finish with `devscope verify test` when practical so Evidence reflects the final
  verification.

- Do not delete or disable tests to make checks pass.
- Do not add large, unrequested features.
- Keep dependencies to the minimum necessary and avoid `unsafe` Rust by default.
- Keep the UI layer separate from progress-analysis logic.
- Keep Codex-specific behavior out of the core; future integrations must be adapters.

Use the documents in `docs/` for detailed direction rather than expanding this file
into a specification.

`docs/backlog.md` contains uncommitted implementation candidates. Do not implement
backlog items unless they have been promoted to `docs/roadmap.md`.
