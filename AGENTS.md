# Working on DevScope

Before making changes, read `README.md`, `docs/design.md`, `docs/roadmap.md`, and
`docs/decisions.md`. Confirm the applicable roadmap task before implementation.

- Record substantial design changes in `docs/decisions.md`.
- Update completed tasks in `docs/roadmap.md` using Markdown checkboxes.
- Verify behavior on Windows.
- After Rust code changes, run:

  ```powershell
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test
  ```

- Do not delete or disable tests to make checks pass.
- Do not add large, unrequested features.
- Keep dependencies to the minimum necessary and avoid `unsafe` Rust by default.
- Keep the UI layer separate from progress-analysis logic.
- Keep Codex-specific behavior out of the core; future integrations must be adapters.

Use the documents in `docs/` for detailed direction rather than expanding this file
into a specification.
