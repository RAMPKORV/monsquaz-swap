import { useEffect, useRef, useState } from "react";
import { Button } from "../components";
import { format, joinInstructions, messages } from "../i18n";

export function JoinInstructions({
  formats,
  maxMib,
  exampleTimezone,
  closed = false,
  preview = false,
  headingLevel = 2,
}: {
  formats: string[];
  maxMib: number;
  exampleTimezone?: string;
  closed?: boolean;
  preview?: boolean;
  headingLevel?: 2 | 3;
}) {
  const [status, setStatus] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<number | null>(null);
  const exampleOverride = new URLSearchParams(window.location.search).get(
    "joinTimezone",
  );
  const zone =
    exampleOverride?.trim() ||
    exampleTimezone?.trim() ||
    Intl.DateTimeFormat().resolvedOptions().timeZone ||
    "Europe/Stockholm";
  const text = joinInstructions(zone, formats, maxMib);
  const Heading = headingLevel === 3 ? "h3" : "h2";
  const meta = preview
    ? messages.join.preview
    : closed
      ? messages.join.closed
      : messages.join.open;

  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
      }
    };
  }, []);

  async function copy() {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    if (!navigator.clipboard) {
      setStatus("failed");
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setStatus("copied");
      timer.current = window.setTimeout(() => {
        setStatus("idle");
        timer.current = null;
      }, 2500);
    } catch {
      setStatus("failed");
    }
  }

  return (
    <section className="card join-card" aria-labelledby="join-title">
      <div className="card__header">
        <Heading id="join-title">{messages.join.title}</Heading>
        <span className="card__meta">{meta}</span>
      </div>
      <p>{messages.join.detail}</p>
      {preview ? (
        <p className="field-help">{messages.join.previewHelp}</p>
      ) : closed ? (
        <p className="field-help">{messages.join.closed}</p>
      ) : null}
      <ol className="join-steps" role="list">
        <li>
          <strong>{messages.join.commandLabel}</strong>
          <code>{messages.join.command}</code>
        </li>
        <li>
          <strong>
            {format(messages.join.options, { timezone: zone })}
          </strong>
        </li>
        <li>
          <strong>{messages.join.seedLabel}</strong>
          <p>{messages.join.seedHelp}</p>
        </li>
      </ol>
      <div className="join-formats">
        <span>{messages.join.formats}</span>
        <ul role="list">
          {formats.map((item) => (
            <li key={item}>
              <code>.{item}</code>
            </li>
          ))}
        </ul>
        <span>{format(messages.join.size, { size: maxMib })}</span>
      </div>
      <p className="join-next">{messages.join.next}</p>
      <div className="join-actions">
        <Button
          variant="secondary"
          icon="copy"
          disabled={preview}
          onClick={() => void copy()}
        >
          {messages.join.copy}
        </Button>
        <span
          className={`join-copied${
            status === "failed" ? " join-copied--error" : ""
          }`}
          role="status"
        >
          {status === "copied"
            ? messages.join.copied
            : status === "failed"
              ? messages.join.copyFailed
              : ""}
        </span>
      </div>
      {status === "failed" ? (
        <div className="join-fallback">
          <p>{messages.join.copyFallback}</p>
          <pre tabIndex={0}>{text}</pre>
        </div>
      ) : null}
    </section>
  );
}
