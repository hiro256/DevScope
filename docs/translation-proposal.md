# Translation Proposal

## Status

This remains exploratory and is not a committed roadmap item. DevScope must work
normally without translated documents.

## Motivation

Project Markdown may intentionally remain in English because it is convenient for
AI-assisted development and common development tooling. Humans may still prefer
Japanese versions in DevScope, VS Code Markdown preview, GitHub, and ordinary
Markdown viewers. Translated Markdown should therefore be useful independently of
the DevScope TUI.

## Primary direction: pre-generated translations

Prefer generating translations ahead of time and storing them as ordinary Markdown
files in the repository.

```text
docs/
  roadmap.md
  design.md
  cli-proposal.md

translations/
  ja/
    docs/
      roadmap.md
      design.md
      cli-proposal.md
```

The English source remains authoritative. Files under `translations/` are derived,
human-readable views. They may be committed to Git so users can read them in normal
Markdown tools without DevScope or a translation service at viewing time.

## Source of truth

Translated Markdown must never become authoritative Plan data.

```text
English Markdown
    -> authoritative Plan source

Japanese Markdown
    -> derived translation
```

DevScope must avoid counting translated task lists as additional project tasks. The
translation directory, for example `translations/**`, should be excluded from normal
Markdown Plan discovery. This prevents an English task and its Japanese translation
from being counted twice.

## Translation synchronization

Treat translation as a synchronization workflow, not as a runtime TUI service.
Possible commands include:

```text
devscope translate
devscope translate status
devscope translate pending
devscope translate check
```

Exact command names remain undecided. DevScope's responsibility is to determine
which Markdown source files are translatable, the corresponding translation path,
and whether a translation is missing or stale. It does not need to perform AI
translation in the initial design.

## No API requirement

The initial workflow must not require an OpenAI API, another cloud API, or a
provider-specific SDK. For example:

```text
devscope translate pending

docs/roadmap.md
  -> translations/ja/docs/roadmap.md
  status: stale
```

An AI agent, sub-agent, local model, translation engine, or human can update the
translated file. `devscope translate check` can then verify synchronization. The
CLI manages translation state without knowing who performed the translation.

## Translation worker independence

Possible translation workers include:

- A Codex translation sub-agent.
- Another AI agent.
- Ollama or another local LLM.
- LibreTranslate or Argos Translate.
- Another translation service.
- A human translator.

```text
DevScope
  -> identifies translation work

Translation worker
  -> performs translation

DevScope
  -> verifies synchronization
```

Provider choice stays outside the authoritative Progress Core.

## AI Skill workflow

A DevScope-oriented Skill may automate translation during AI-assisted development:

1. The agent performs implementation work.
2. The agent updates source Markdown where appropriate.
3. At a logical work boundary, before committing, the Skill checks translation
   status.
4. If relevant Markdown changed, it delegates translation to a focused sub-agent.
5. The translated files under `translations/<language>/` are updated.
6. The Skill verifies synchronization, reviews the diff, and commits source and
   translated Markdown together.

Do not translate after every file modification. Multiple edits during one task
should normally be translated once near the work boundary.

## Translation sub-agent

Translation is a suitable narrowly scoped task for a smaller or cheaper model. The
worker should:

- Preserve Markdown structure, headings, and task checkbox state.
- Preserve code blocks, inline code, commands, file paths, URLs, and identifiers.
- Use concise natural Japanese suitable for technical documentation.
- Preserve task meaning without making design decisions.
- Modify only the target translated documents.

The exact model or agent is intentionally unspecified.

## Human workflow

The same mechanism must work for direct human edits:

1. A human edits `docs/roadmap.md`.
2. They run `devscope translate` or `devscope translate pending`.
3. DevScope reports the stale or missing Japanese translation.
4. The result is handed to ChatGPT, Codex, another AI, or a human translator.
5. The translation file is updated.
6. `devscope translate check` verifies synchronization.

The workflow does not depend on an AI being able to invoke DevScope itself.

## Trigger model and Git hooks

File changes and workflow boundaries are distinct:

```text
File changed
  -> mark translation as potentially stale
  -> do not immediately invoke translation

Logical task completion or before commit
  -> check pending translations
  -> update translations
```

If Git hooks are explored, they should be deterministic and lightweight. A
pre-commit hook may verify whether required translations are current, but must not
start an AI model or remote translation service. Commits should not unexpectedly
wait for AI generation, network failures, or usage limits.

```text
Skill or human workflow
    -> generates translation

pre-commit
    -> verifies translation state
```

## Staleness detection

DevScope should detect whether a translation corresponds to the current source
without AI assistance. Source content hashes are a likely mechanism. A translated
document may record metadata conceptually equivalent to:

```text
source: docs/roadmap.md
source-hash: <hash>
```

The storage format is undecided. Avoid commit SHA as the primary mechanism because
source and translation are normally committed together, which creates awkward
self-reference. Metadata must not make translated Markdown authoritative state.

## Markdown viewer use case

Git-tracked translated Markdown is intentionally useful outside DevScope. Users
should be able to open `translations/ja/docs/roadmap.md` directly in VS Code,
GitHub, or other ordinary Markdown viewers. This is a key reason to prefer
pre-generated translation over runtime-only translation.

## Runtime translation

Runtime or on-demand translation may remain a future extension, but it is no longer
the primary proposal. Possible providers include local translation engines, local
LLMs, cloud AI APIs, and custom external commands. If added, it remains optional
and provider-neutral; the initial implementation must not be designed around it.

## Translation versus explanation

Translation attempts to preserve source meaning. AI explanation can add
interpretation, so it remains a derived, non-authoritative presentation feature.
Explanation is future work and must not complicate the initial pre-generated
translation experiment.

## Initial experiment

Validate a deliberately small experiment with:

- English as the source language and Japanese as the target language.
- One configured translation directory and source-to-translation path mapping.
- Missing and stale detection.
- Translation-directory exclusion from Plan discovery.
- A Skill workflow using a translation sub-agent.
- Git-tracked translated Markdown.
- Synchronization checks that call no API.

Potential first CLI responsibilities are:

```text
devscope translate status
devscope translate pending
devscope translate check
```

Do not assume DevScope needs a `translate update` command that calls an AI. The
first experiment can let the Skill, AI, or human perform the actual translation.

Evaluate convenience outside DevScope, preservation of Markdown structure,
reliability of stale detection, workflow timing, translation overhead, Plan
discovery exclusion, and whether a runtime provider is needed at all.

## Non-goals

The initial design should not become:

- A mandatory localization system.
- A replacement for English source documents.
- An AI API dependency.
- A translation provider framework inside Progress Core.
- Automatic AI execution from Git hooks.
- A separate project-management data store.
- A system that treats translations as authoritative Plan data.
