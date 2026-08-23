# DevScope

DevScope is a project-centric progress observer for AI-assisted software development.
It derives a project's status from observable project information instead of relying
on an agent's self-reported state.

DevScope models progress through four concepts:

- **Plan** — what is intended, starting with Markdown task lists and planning documents.
- **Activity** — what has changed, starting with Git status, diffs, and history.
- **Evidence** — what can be verified, such as build and test results.
- **Agent** — what an agent is currently doing.

The first MVP focuses on Markdown, Git, and a terminal user interface (TUI). Windows
is the primary environment, while the project keeps a cross-platform structure for
Linux and macOS. DevScope is currently in its initial development stage.

See the [design](docs/design.md), [roadmap](docs/roadmap.md), and
[decision log](docs/decisions.md) for project details.
