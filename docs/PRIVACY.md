# Privacy

The host is the data controller for their event. The app is a local tool. The
project does not operate a server and does not receive event data.

## Data the app stores

| Data | Why | Where |
|---|---|---|
| Discord user ID | identity, assignments, credits, audit | SQLite |
| Display name snapshot | credits and host recognition | SQLite |
| Timezone | deadlines, quiet hours | SQLite |
| Consent time | proof of consent | SQLite |
| Event role and status | scheduling | SQLite |
| Seed and submitted files | the event itself | local library |
| File hash, size, extension | integrity and dedup | SQLite |
| Assignment, offer, claim, skip | event state | SQLite |
| Comments and questionnaire answers | reveal and host review | SQLite |
| DM channel ID and message cursor | delivery and recovery | SQLite |
| Audit entries | accountability | SQLite |
| Local logs | support and debugging | local log files |
| Pack report | host review | SQLite |

The app does not collect email, phone, IP address, payment data, or precise
location.

## Purposes

- run the event and deliver files;
- compute and validate the schedule;
- send reminders and handle deadlines;
- produce credits, comments, and the reveal;
- recover from outages and mistakes;
- keep an audit trail of host actions.

## Retention

| Data | Default |
|---|---|
| Active event data | until the event is archived or deleted |
| Archived event | until the host deletes it |
| Backups | 90 days, then deleted by the app; pruning is implemented |
| Local logs | 30 days, then rotated |
| Transient DM payloads | dropped after ingest |
| Public reveal package | controlled by the host |

The host can change the archive retention. The app must show the current
retention and the backup age.

## Export and delete

- A participant can request an export of their records and submitted files.
  The export is a ZIP with `participant.json`, a README, and the original files
  that participant submitted. It does not contain another participant's rows
  or files.
- A participant can request deletion. The app replaces the Discord user ID and
  display name with a per-event salted tombstone, removes future plan rows,
  keeps past credits and event files, and appends a hash-only audit entry.
  An active assignment blocks deletion until the host resolves it.
- The host can delete an archived event. That flow is not implemented yet.
- The app reports what it removed and what it kept.

## Telemetry

- No analytics.
- No crash reporting by default.
- A host can export a diagnostics bundle. The bundle contains logs and system
  information. It must not contain the bot token, the author mapping, or
  absolute participant paths.
- Telemetry stays off unless the host explicitly opts in in a future version.

## Discord

Discord processes messages, attachments, IDs, and metadata under its own
privacy policy. The app cannot control Discord's retention or access.

- Server admins can read public channels and private threads.
- Discord staff can access platform data.
- A DM is private from other participants, not from Discord.
- The bot has no privileged intents and does not read public message content.

## Anonymity limits

- Participants are anonymous from each other until the reveal.
- The host and co-hosts can see the mapping.
- The host machine, Discord, and server admins are outside the anonymity
  boundary.
- Small events and distinctive musical style can reveal identity by inference.
- The rules must state these limits.

## Album content

- Participants keep authorship of their contributions.
- The default album license is CC BY-NC-SA 4.0.
- The host can choose a different license per event.
- The export states the license and credits the participants.
- The host must have permission to distribute any samples or third-party
  material in the album.

## Minors

Discord requires users to meet its minimum age. The host must follow local law
and Discord policy. The app does not verify age.

## Security

See `SECURITY.md` and `ai-rules/SECURITY.md`.

## Contact

For a privacy question about a specific event, contact the host of that event.
For a question about the app, use the repository security/contact channel.
