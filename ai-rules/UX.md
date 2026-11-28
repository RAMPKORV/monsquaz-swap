# UX and copy rules

The vibe is playful retro Monsquaz, but the job is serious: people need to know
what to do, when, and what is private. Clarity wins over cleverness.

## Voice

- Warm, a little irreverent, never mean. No public shaming for lateness or
  skipping.
- Every instruction follows ASD-STE100 (`ai-rules/STE.md`): short, active, one
  action per sentence. The playful voice is allowed around the instruction.
- Short sentences. One primary action per message.
- Use the event's language: seed, track, handoff, claim, skip, fill-in, reveal.
- Never imply perfect anonymity. Say: "Anonymous from other participants until
  the reveal. The host can see the mapping."
- Do not use ASCII art in critical instructions; it breaks screen readers.
- Localize dates/times; show UTC when precision matters.

## Discord copy patterns

**Offer (DM):**
> Track `S7F3A` is yours if you want it.
> Claim within 24h; the deadline starts when you claim.
> [Claim] [Not now]

**After claim:**
> Claimed. `S7F3A.it` is attached. Due <t:...:F> (<t:...:R>).
> [Submit] [Skip] [Need more time]

**Reminder:**
> 6h left on `S7F3A`. [Still working] [Need +6h] [Skip]
> Quiet hours respected. You get a nudge at 50%, 80%, and 95% of the time,
> and one at the deadline.

**Deadline passed:**
> The deadline passed on `S7F3A`. The host has been notified.
> [Finishing today] [Need an extension] [Skip]

**Skip after claim:**
> Skipped. The track goes back unchanged, and you will not receive it again.
> Your past credits are kept. You can leave the event now if you need to.

**No eligible fill-in / pass-through:**
> No one could take this turn. The track moves on unchanged. This is not a
> failure; the chain continues.

**Reveal:**
> The album is out. Here is who touched what, in order. Thank you for making
> something none of us could have made alone.

Rules for messages:
- Never include another participant's name in a private offer/submission DM.
- Use Discord timestamps for local time; avoid hard-coded timezone names.
- Buttons for the primary action; never require a typed command to submit.
- Aggregate public progress only ("Round 3 underway · next release in 6h").
- A quiet hours window is respected for non-urgent DMs; deadline prompts are
  scheduled earlier, not at 3am.

## Host app

- **Mission control** shows one next action, the event state, bot health, at-risk
  participants, and pending decisions. Avoid a wall of metrics.
- **Warnings** say what will happen, why, and the fix. Soft warnings for fun,
  size, overcrowding; hard blocks only for invariants.
- **Destructive actions** (reveal, replan, delete, restore) require a typed
  confirmation or a two-step dialog with consequences.
- **Status** uses text + icon + color, never color alone.
- **Stream-safe mode** is prominent before the reveal stream; it hides mapping
  and redacts IDs.
- **Empty states** explain the next step; do not show a blank table.
- **Errors** say what happened, what to do, and a copyable code. No stack traces,
  paths, tokens, or raw Discord payloads.
- **Accessibility:** WCAG AA contrast, keyboard navigation, focus rings, reduced
  motion, semantic headings, screen-reader labels, 200% zoom.
- **Theme:** dark playful default, light theme; no external fonts required.

## Fun without pressure

- A host-only fun score is a planning aid. Do not show it to participants as a
  grade.
- Post-event stats celebrate effort (edits, rescues, longest chain) without
  ranking people.
- Fill-ins get credit, not a "rescue" label in public.
- Skips are private by default; the reveal shows anonymous gaps unless the host
  chooses otherwise.
- The reveal is a show: lineage, R1 vs final, credits, a big thank-you. Keep it
  generous.

## Copy checklist

- Does the user know the next action in one read?
- Is the deadline in their local time?
- Is anything that should be private visible?
- Is a skip/leave framed without shame?
- Does an error tell them how to fix it?
- Would a first-time participant understand "claim" vs "submit"?

## UI quality gate

Every UI issue must produce visual evidence. The app must look intentional, calm,
and consistent. It must not look like a generated admin template.

### Design system

- One spacing scale: 4, 8, 12, 16, 24, 32, 48 px. Do not invent values.
- One type scale: display, h1, h2, h3, body, small, mono. Limit weights.
- One radius scale and one shadow scale.
- One density mode by default. Tables may use a compact mode, but rows must
  remain readable.
- Use CSS grid/flex and shared gaps. Do not position with magic margins.
- Buttons have a clear hierarchy: primary, secondary, quiet, danger.
- Status uses text + icon + color. Never color alone.
- Icons are from one set. Do not mix emoji and line icons in the same control.
- Use real sample data in screenshots. No lorem ipsum, no "Item 1".

### Layout and density

- One primary action per view. Secondary actions are visibly secondary.
- Maximum two levels of nested cards. Avoid a card inside a card inside a card.
- Tables show the columns the host needs. Move the rest into a detail panel.
- Long text wraps; it does not overflow. Truncate only with a tooltip or detail.
- No horizontal scroll at 1024 px unless the content is a real matrix.
- Every empty state explains the next action.
- Every loading state avoids layout shift.
- Every error state keeps the user's work and offers a fix.

### Screenshots, review waves, and reject list

`docs/UI_REVIEW.md` owns the screenshot matrix, the view list, the review
waves, the finding format, and the reject list. Do not copy them here.
`ai-rules/DESIGN.md` owns the deterministic design gate. The skill
`ai-rules/skills/design-review/SKILL.md` explains how to run the waves.
