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
    commandLabel: "1. Run this command in the server.",
    command: "/swap join timezone:{timezone} max_handoffs:{handoffs}",
    seedLabel: "2. Send one seed file in a direct message.",
    formats: "Allowed formats: {formats}.",
    size: "Maximum size: {size} MiB.",
    copy: "Copy join instructions",
    copied: "Join instructions copied.",
    copyFailed: "Could not copy. Select the text and copy it.",
    closed:
      "The roster is locked. These are the instructions this event used.",
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
  timezone: string,
  handoffs: number,
  formats: string[],
  maxMib: number,
): string {
  return [
    format(messages.join.command, { timezone, handoffs }),
    format(messages.join.seedLabel, {}),
    format(messages.join.formats, {
      formats: formats.map((item) => `.${item}`).join(", "),
    }),
    format(messages.join.size, { size: maxMib }),
  ].join("\n");
}
