// Message catalog and locale helpers.
//
// The catalog starts with the new host-facing strings. Move more strings here
// as screens are localized. Use `format` for values and `plural` for counts.
// Do not build sentences by joining translated fragments.

export const messages = {
  join: {
    title: "Invite participants",
    detail:
      "Send these instructions in the server before you lock the roster.",
    intro: "Join the swap:",
    commandLabel: "In the server, run /swap join.",
    command: "/swap join",
    options:
      "Choose your timezone (for example {timezone}) and max_handoffs (the number of exchange tracks you will edit, 1 to 20). The seed is not an edit.",
    seedLabel:
      "Make one track from the host pack and send it to the bot in a direct message before the seed deadline.",
    seedHelp: "A seed is a track you made. Round 1 is the seed round.",
    formats: "Allowed formats:",
    size: "Maximum size: {size} MiB.",
    next: "The host closes the seed round, locks the roster, and opens the exchange rounds.",
    copy: "Copy join instructions",
    copied: "Join instructions copied.",
    copyFailed: "Could not copy. Select the text below and copy it.",
    copyFallback: "Select this text and copy it:",
    closed:
      "The roster is locked. These are the instructions this event used.",
    open: "Join phase open",
    preview: "Draft preview",
    previewHelp:
      "Create the event first. Then copy the instructions on mission control.",
  },
  planner: {
    warnings: "{count} {label} to review",
    warning: "note",
    warningsPlural: "notes",
  },
  reveal: {
    comments: "{count} {label}",
    comment: "comment",
    commentsPlural: "comments",
  },
} as const;

export type MessageValues = Record<string, string | number>;

export function format(template: string, values: MessageValues): string {
  return template.replace(/\{(\w+)\}/g, (match, key: string) => {
    const value = values[key];
    return value === undefined ? match : String(value);
  });
}

export function plural(
  count: number,
  one: string,
  other: string,
  locale = "en",
): string {
  return new Intl.PluralRules(locale).select(count) === "one" ? one : other;
}

export function formatDateTime(
  seconds: number,
  timezone: string,
  options: Intl.DateTimeFormatOptions = {},
): string {
  return new Intl.DateTimeFormat("en", {
    timeZone: timezone,
    dateStyle: "medium",
    timeStyle: "short",
    ...options,
  }).format(new Date(seconds * 1000));
}

export function joinInstructions(
  exampleTimezone: string,
  formats: string[],
  maxMib: number,
): string {
  return [
    messages.join.intro,
    `1. ${messages.join.commandLabel}`,
    `2. ${format(messages.join.options, { timezone: exampleTimezone })}`,
    `3. ${messages.join.seedLabel} ${messages.join.seedHelp}`,
    `${messages.join.formats} ${formats.map((item) => `.${item}`).join(", ")}`,
    format(messages.join.size, { size: maxMib }),
    messages.join.next,
  ].join("\n");
}
