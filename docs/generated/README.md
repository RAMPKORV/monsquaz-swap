# Generated tracker files

These files are generated from the SQLite tracker. Do not edit them by hand.
Use `./bin/project` and `./bin/project export`.

The decision dump is not kept. `project export` writes the PRD, status,
generated features, and the ledger dump only. `ai-rules/DECISIONS.md` is the
single human-readable decision log.

The canonical hand-written documents are:

- `docs/PRD.md` — product contract
- `docs/EVENT_SETTINGS.md` — every event setting
- `ai-rules/DECISIONS.md` — decision index and rationale
- `specs/features/` — hand-written Gherkin

The tracker is the source for issues, evidence, sessions, and the append-only
ledger. It is not the product contract.
