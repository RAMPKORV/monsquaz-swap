# ADR 0004 — Reveal presenter, titles, comments, and audio preview

Date: 2026-11-26

## Status

Accepted for MSW-050.

## Context

The reveal is the product payoff. The host needs one screen that shows the
album in its reveal order, the lineage of every track, the R1 seed and final
audio, the credits, comments, and the statistics. The screen is host-only
until the host publishes the reveal. The host can share the screen on a
stream, so a stream-safe mode must hide the mapping without changing the
data.

The classic loop already stores versions in `files` and credits in
`assignments`. It has no final titles and no comments. The UI cannot receive
file bytes or absolute paths. Module files are not playable in a browser.

## Decisions

1. **The reveal package is a host-only read model.** `reveal_package` builds
   the package from the event, participants, assignments, files, titles, and
   comments. It does not change event state. The package contains names and
   comment text. It never contains file bytes, storage paths, or Discord URLs.
   The Tauri command is available only to the host window.

2. **The album order uses HMAC.** The order is
   `sort(HMAC-SHA256(event seed, track code), track code)`. It is stable for
   one event, independent of the input order, and never uses submission time.

3. **Titles and comments use additive migration 008.** `track_titles` stores
   one final title for each track. `track_comments` stores comments and
   questionnaire answers with attribution and a redaction time. A redacted
   comment is excluded from the package. The host adds comments before the
   reveal. The participant can set the final title with `/swap title` when
   the bot asks. The host can set or correct a title before the reveal. The
   database stores UTC seconds.

4. **Audio uses an opaque preview protocol.** The UI receives a
   `swap-preview` URL with a file id. The protocol reads the file by id from
   the content store. A module renders to WAV through openmpt123 for at most
   120 seconds and the WAV is cached by SHA-256. WAV, FLAC, MP3, and OGG
   return their bytes with the correct media type. The protocol supports byte
   ranges for seeking. An unknown id returns 404. No path from the URL reaches
   the file system.

5. **Stream-safe mode is a client-side view.** It replaces seed owner and
   contributor names with role labels and hides the host tools. It does not
   change the package, the database, or the published reveal. The host can
   turn it off and on during the stream.

6. **Publishing stays the existing atomic reveal.** The current reveal writes
   the credits message and moves the event to `Reveal` in one transaction.
   Final-file uploads, the schedule disclosure, the ZIP package, the runbook,
   and keepsakes stay in MSW-025. This ADR does not change custody, deadlines,
   or the seen ledger.

## Consequences

- The presenter is useful before the reveal and read-only after it.
- The host can check the exact audio and credits before the public reveal.
- A missing final title does not block the reveal. The presenter labels the
  track "Untitled" and the host can set a title before publish. The bot also
  asks the previous submitter when the final holder does not name the track.
  The ask contains the track code and the round in which that person
  submitted the track. The previous submitter uses `/swap title` with the
  track code. The final holder keeps priority until a title exists. The app
  records the fallback in the audit log.
- The first audio request for a long module can take seconds. The UI shows a
  native audio control and the render is cached for later requests.
- The preview protocol reads the database per request. This is acceptable for
  the host window. A cache is an optimization, not a correctness requirement.

## Rollback

Migration 008 is additive. An older build refuses a newer schema with
`SchemaTooNew`. To roll back, restore a pre-migration database backup. The
preview protocol and the presenter route can be removed without a data
migration. The reveal package reads existing rows only. No custody, deadline,
or seen state changes.

## Verification

- `crates/swap-core/src/reveal.rs`: HMAC test vector, stable order, seed
  sensitivity.
- `crates/swap-engine/tests/classic_flow.rs`: package contents, title holder
  rule, comment redaction, album order, version lineage.
- `crates/swap-db/tests/repository.rs`: version 6 to latest migration includes
  the title and comment tables.
- `src-tauri/src/preview.rs`: range parsing and extension classification.
- `ui/src/App.test.tsx`: reveal view, stream-safe redaction, title save,
  comment redaction.
- UI review with screenshots at 1280×800 and 1024×700 in light and dark.
