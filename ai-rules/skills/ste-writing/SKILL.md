---
name: ste-writing
description: Write and lint Monsquaz Swap specifications in ASD-STE100 Simplified Technical English. Use when you write or edit Gherkin in specs/features, docs/EVENT_SETTINGS.md, the PRD, runbooks, ai-rules, CLI instructions, or error messages.
---

# STE writing

The rules are in `ai-rules/STE.md`. The glossary is in `ai-rules/DOMAIN.md`.
This skill tells you how to apply and check them.

## Write

1. Name the actor. Use the active voice: "The bot sends the file."
2. Write one instruction in one sentence.
3. Keep a procedure sentence or a Gherkin step at 20 words or fewer.
4. Keep a description sentence at 25 words or fewer.
5. Use the approved term from the terminology table in `ai-rules/STE.md`.
   Write "track", not "song". Write "participant", not "user".
6. Write `must` for a requirement and `can` for a capability. Do not write
   `should`.
7. State each time, unit, and zone. Write "24 hours" and "the host IANA zone".
8. Do not use contractions, `etc.`, `e.g.`, `i.e.`, `and/or`,
   `appropriate`, `reasonable`, `as needed`, or `robust`.
9. Put three or more conditions in a vertical list.

For Gherkin, also follow `ai-rules/GHERKIN.md`. A new scenario without a test
gets the `@pending` tag.

## Lint

```bash
node scripts/ste-lint.mjs            # gate + advisory summary
node scripts/ste-lint.mjs --verbose  # list every advisory finding
```

The linter is a hard gate for `specs/features/*.feature` and
`docs/EVENT_SETTINGS.md`. It is advisory for `docs/PRD.md`,
`docs/RUNBOOK.md`, and `ai-rules/*.md`. It ignores code spans, code blocks,
URLs, quoted UI strings in Gherkin, and table cells with one token.

## Fix a finding

1. Rewrite the sentence. This is the normal fix.
2. Split a long sentence into two sentences.
3. Replace a synonym with the glossary term.
4. If the word is an external name (a Discord API term or a module-format
   field), add a baseline entry. Run
   `node scripts/ste-lint.mjs --update-baseline --baseline-issue MSW-###`.
   Then write a reason in `scripts/ste-lint.baseline.json`. Every entry needs
   a reason and an issue or an expiry.

Do not add a baseline entry to hide a sentence that you can rewrite.

## Check before you finish

Use the review checklist at the end of `ai-rules/STE.md`. The linter checks
only part of it. It cannot check the voice, the pronouns, or one action per
sentence.
