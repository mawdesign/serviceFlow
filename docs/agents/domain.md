# Domain docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- `GLOSSARY.md` at the repo root, or
- `GLOSSARY-MAP.md` at the repo root if it exists, then read each relevant context file,
- `docs/adr/` for system-wide decisions.

If any of these files do not exist, proceed without flagging their absence. The relevant domain-modeling skills can create them lazily when a real vocabulary or decision is being clarified.

## Layout

This repo is a single-context repository.

```
/
├── GLOSSARY.md          # optional; repo-wide domain vocabulary
├── docs/adr/            # optional ADRs for repo-wide decisions
├── src/                 # application code
└── docs/agents/         # agent guidance and tracker metadata
```

A multi-context layout is not in use here unless a future monorepo signal appears.

## Use the glossary's vocabulary

When an issue title, PR description, refactor proposal, or ticket references a domain concept, prefer the terms defined in the glossary over ad hoc synonyms. If the concept is missing, treat that as a signal that the vocabulary either needs to be clarified or the work is using a concept not yet codified by the project.

## Flag ADR conflicts

If output contradicts an existing ADR, surface the conflict explicitly rather than silently overriding it.

> Contradicts ADR-0007, but worth revisiting because...
