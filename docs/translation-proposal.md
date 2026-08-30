# Translation and Explanation Proposal

## Status

This is an exploratory proposal. Translation and explanation are optional
presentation features and are not part of the current committed roadmap. DevScope
must continue to work fully without a translation provider.

## Motivation

Development planning documents are often written in English because that is common
in software development and efficient for AI agents. A human user may prefer
Japanese when reading task descriptions, status details, commit messages, Evidence
summaries, or other project information.

The goal is to improve human understanding without rewriting source Markdown or
creating translation-only Git diffs.

## Core principles

- Source Markdown remains unchanged.
- Translation is presentation-only and is not written to tracked project files by
  default.
- Translation is optional; failures never prevent normal DevScope operation.
- The Core remains independent of a particular AI or translation vendor.
- Providers are replaceable, and original text is always available.
- Translate only text that is displayed or explicitly requested, and cache results
  where practical.

```text
Project data
  -> Progress Core
  -> Presentation text
  -> Translation / Explanation Service
  -> TUI
```

Translation is outside the authoritative Plan, Activity, and Evidence model.

## Provider abstraction

DevScope should not directly depend on one translation service. Possible
interchangeable providers include local translation engines, local LLMs, the OpenAI
API, other cloud translation APIs, and custom external commands.

A conceptual provider interface is:

```text
Translator
  -> translate(text, target_language)

Explainer
  -> explain(text, target_language)
```

The exact Rust API is intentionally undecided.

## Initial provider direction

The first implementation should favor low coupling.

### External command

DevScope could send source text to a configured command and receive translated
text.

```text
[translation]
enabled = true
target = "ja"
command = "my-translator"
```

This avoids provider-specific SDKs, permits local or cloud integrations, and is
suitable for experiments.

### Local HTTP provider

An optional locally running translation or LLM service, such as LibreTranslate,
Ollama, or another local model server, may provide translation without per-request
cloud API charges.

### Cloud provider

Future optional providers may use OpenAI or other cloud translation APIs. Cloud use
must remain optional and is never required for normal DevScope operation.

## Free and local options

LibreTranslate or Argos Translate are translation-focused local options with a
simple HTTP interface. Ollama can support translation and explanation with multiple
models. Local solutions still consume machine resources and may differ in quality
and latency.

## Translation versus explanation

Translation and explanation are separate capabilities.

```text
Source:
No-change polling avoids unnecessary Git collection

Translation:
変更がない場合のポーリングでは、不要なGit情報の収集を行わない

Explanation:
When no project state has changed, DevScope should avoid running unnecessary Git
collection work. This reduces repeated processing.
```

Translation preserves meaning. Explanation prioritizes human understanding and may
add contextual interpretation, so it must never be treated as authoritative project
state. A future UI may offer Original, Translate, and Explain views.

## Initial scope

Do not translate entire Markdown documents initially. A small experiment should
translate only the currently selected task or detail text:

```text
User selects task
  -> DevScope requests translation
  -> provider translates in background
  -> result is displayed
  -> original remains available
```

Possible later targets include task and section descriptions, commit messages,
Evidence summaries, activity details, and selected Markdown content. Do not
translate raw logs, code, or large documents automatically.

## Asynchronous behavior and caching

Translation must not block the TUI. While a provider runs, the original text can
remain visible with a `Translating...` status; failures should show a small error or
fall back to original text.

Cache results to avoid repeated provider calls. A cache key may include source text,
target language, provider identity or configuration, and possibly provider or model
version. Changed source text makes a result stale. The persistence strategy remains
undecided.

## Markdown and code safety

Technical content should be preserved where possible. Do not modify or translate
inline code, code blocks, commands, file paths, URLs, or identifiers. An early
implementation can avoid complex Markdown translation by using already parsed task
text rather than arbitrary documents.

## Relationship to DevScope architecture

Translation is a presentation concern. It does not change the meaning of Plan,
Activity, Evidence, or Agent. Markdown, Git, and Build/Test data remain
authoritative; translation and explanation are derived views shown by the TUI.

## AI independence

DevScope must not require ChatGPT, Codex, Claude, or any other specific AI product.
Providers may support them through optional adapters or APIs. A user's ChatGPT
subscription must not be assumed to provide an API interface to DevScope.

## Initial experiment

Keep a first experiment deliberately small:

- Translation disabled by default.
- Configure one provider.
- Translate only the selected task, with Japanese as the first target language.
- Display original and translated text.
- Perform work asynchronously and cache successful results.
- Fall back safely when the provider is unavailable.

Evaluate usefulness, latency, cache behavior, local quality, whether explanation is
more useful than literal translation, provider abstraction complexity, and whether
the feature distracts from progress observation. Only then should broader document
translation or explanation be considered for the roadmap.

## Non-goals

The initial feature must not become:

- A Markdown localization system.
- An automatic source-file translator.
- A requirement for an AI API key.
- A replacement for original project text.
- A translation database stored in the repository.
- A core dependency on one external provider.
