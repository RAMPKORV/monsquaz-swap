# Specification language — ASD-STE100

Use **ASD-STE100 Simplified Technical English (Issue 9, 2025)** for
specifications and technical instructions. The goal is one meaning per word and
one action per sentence. The official standard is at
<https://www.asd-ste100.org/>. This file summarizes the rules that we apply; it
is not a copy of the standard and it is not a certification claim.

## Scope

Use STE for:

- `docs/PRD.md`, requirements, acceptance criteria, and event settings;
- `specs/features/*.feature` (Gherkin);
- test plans, runbooks, CLI/release instructions, and error messages;
- architecture and security rules where they give instructions.

User-facing marketing copy and the playful Discord voice can use the product
tone. Instructions inside that copy still follow STE: short, active, one action.

## Core rules

1. **One word, one meaning, one part of speech.** Use the glossary in
   `ai-rules/DOMAIN.md`. Do not rotate synonyms.
2. **Use the approved word when one exists.** Prefer simple, common words.
   Technical nouns and verbs are allowed when the glossary defines them.
3. **Use the active voice.** Name the actor. Do not use the passive voice.
4. **Use simple tenses.** Use present, past, or future. Do not use the perfect
   tenses. Avoid `-ing` verb forms when a simple verb works.
5. **Keep sentences short.** Procedures: maximum 20 words. Descriptions:
   maximum 25 words. Notes: maximum 25 words.
6. **One instruction per sentence.** If two actions are not simultaneous, write
   two sentences.
7. **One topic per paragraph.** Maximum six sentences per paragraph.
8. **Do not omit words.** Use articles (`a`, `an`, `the`) and subjects. Do not
   use contractions.
9. **Do not use ambiguous pronouns.** Replace `it`, `this`, `they`, and `them`
   with the noun when the reference is not clear.
10. **Do not use idioms, metaphors, slang, or marketing adjectives.** Do not use
    `etc.`, `e.g.`, `i.e.`, `and/or`, `as needed`, `appropriate`, `reasonable`,
    or `robust` without a definition.
11. **Use lists for conditions.** If a rule has three or more conditions, write
    a vertical list.
12. **Be explicit about time, units, and zones.** Write `UTC`, an IANA zone, a
    number, and a unit. Do not write "soon" or "later" in a requirement.
13. **Define a term before you use it.** Link to the glossary. Do not use a
    synonym for a defined term.

## Requirement keywords

| Word | Meaning |
|---|---|
| **must** | Mandatory. The release is not acceptable without it. |
| **must not** | Prohibited. |
| **can** | Capability that is allowed. |
| **will** | Future behaviour or a consequence. |
| **optional** | The host or participant can enable it. |
| **should** | Avoid in requirements. If used, it means a recommendation, not a release gate. |

EARS `SHALL` is allowed as a synonym for `must` inside a requirement statement.
Do not mix `SHALL` and `must` in the same document without a note.

## Gherkin rules

- Write `Given` as state, `When` as one action, `Then` as one observable result.
- Use domain nouns, not UI labels, SQL, function names, or API fields.
- Do not use `valid`, `proper`, `correct`, or `normal` without criteria.
- Do not assert exact prose. Assert the decision or effect.
- Use a `Scenario Outline` only for real data tables.
- One scenario has one behaviour. Split compound `Then` clauses.
- Use `must`/`can` in the rule text; do not use `should`.
- Tag with `@v1`, `@scheduler`, `@anonymity`, and the relevant domain tag.

## Project terminology

Use these exact terms. The full definitions are in `ai-rules/DOMAIN.md`.

| Use | Do not use |
|---|---|
| event, swap | project, game, run |
| participant | user, player, member (unless you mean a Discord guild member) |
| host | admin, organizer, owner (unless you mean a track owner) |
| fill-in | substitute, backup, replacement |
| track | song, file, tune (use "file" only for the bytes on disk) |
| seed | initial submission, starting track |
| offer | assignment (an offer is not yet claimed) |
| claim | accept, take |
| submit | upload, send back (use "submit" for the completed edit) |
| skip | pass, decline (use "skip" for the recorded turn) |
| reveal | launch, release (use "release stream" for the external show) |
| seen ledger | history, touch log |

## Examples

Non-STE:
> When the participant has clicked the claim button, the file will be sent to
> them and the deadline will be started.

STE:
> The participant clicks Claim. The bot sends the file. The deadline starts at
> the claim time.

Non-STE:
> The scheduler should try to make sure that nobody gets the same song twice.

STE:
> The scheduler must not assign the same track to the same participant twice.

## Review checklist

- [ ] Every sentence is 20 words or fewer (procedure) or 25 words or fewer
      (description).
- [ ] Every instruction has one action.
- [ ] The actor is named; the voice is active.
- [ ] Defined terms match `ai-rules/DOMAIN.md`; no synonyms.
- [ ] No contractions, idioms, `etc.`, `e.g.`, `i.e.`, or `and/or`.
- [ ] Every condition is explicit; no "appropriate" or "as needed".
- [ ] Every time, unit, and zone is explicit.
- [ ] No sentence depends on an unclear pronoun.
- [ ] The Gherkin has one behaviour per scenario and no UI/implementation nouns.
