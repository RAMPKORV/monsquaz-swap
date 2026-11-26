import { useState } from "react";
import { Button } from "../components";
import { format, joinInstructions, messages } from "../i18n";

export function JoinInstructions({
  formats,
  maxMib,
  timezone,
  closed = false,
  onAction,
}: {
  formats: string[];
  maxMib: number;
  timezone?: string;
  closed?: boolean;
  onAction?: (message: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  const zone =
    timezone?.trim() ||
    Intl.DateTimeFormat().resolvedOptions().timeZone ||
    "UTC";
  const handoffs = 4;
  const command = format(messages.join.command, {
    timezone: zone,
    handoffs,
  });
  const text = joinInstructions(zone, handoffs, formats, maxMib);

  async function copy() {
    if (!navigator.clipboard) {
      onAction?.(messages.join.copyFailed);
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2500);
      onAction?.(messages.join.copied);
    } catch {
      onAction?.(messages.join.copyFailed);
    }
  }

  return (
    <section className="card join-card" aria-labelledby="join-title">
      <div className="card__header">
        <h2 id="join-title">{messages.join.title}</h2>
        <span className="card__meta">
          {closed ? "Roster locked" : "Open for signup"}
        </span>
      </div>
      <p>{messages.join.detail}</p>
      {closed ? (
        <p className="field-help">{messages.join.closed}</p>
      ) : null}
      <ol className="join-steps">
        <li>
          <strong>{messages.join.commandLabel}</strong>
          <code>{command}</code>
        </li>
        <li>
          <strong>{messages.join.seedLabel}</strong>
          <p>
            {format(messages.join.formats, {
              formats: formats.map((item) => `.${item}`).join(", "),
            })}{" "}
            {format(messages.join.size, { size: maxMib })}
          </p>
        </li>
      </ol>
      <div className="join-actions">
        <Button variant="secondary" icon="mail" onClick={() => void copy()}>
          {messages.join.copy}
        </Button>
        {copied ? (
          <span className="join-copied" role="status">
            {messages.join.copied}
          </span>
        ) : null}
      </div>
    </section>
  );
}
