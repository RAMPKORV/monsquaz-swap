import { useEffect, useState } from "react";
import type {
  BackupItem,
  BackupStatus,
  RestoreDone,
  RestorePreview,
  RestoreSource,
} from "../types";
import { Button, Dialog, Field, Skeleton, StatusPill } from "../components";
import { Icon } from "../icons";
import {
  backupNow,
  chooseBackupFolder,
  chooseRestoreFolder,
  isDemoMode,
  loadBackupStatus,
  loadRestoreSource,
  previewRestore,
  purgeBackups,
  restoreBackup,
  takeOverWriter,
} from "../data/client";
import { formatDateTime } from "../i18n";

const kindLabel: Record<string, string> = {
  transition: "State change",
  daily: "Daily",
  manual: "Manual",
};

const stateLabel: Record<string, string> = {
  draft: "Draft",
  signup: "Signup",
  schedule_review: "Schedule review",
  active: "Active",
  paused: "Paused",
  reveal: "Reveal",
  archived: "Archived",
};

function message(error: unknown): string {
  return typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : "The command failed.";
}

function eventsText(item: { events: { name: string; state: string }[] }) {
  if (item.events.length === 0) {
    return "No events";
  }
  return item.events
    .map((event) => `${event.name} (${stateLabel[event.state] ?? event.state})`)
    .join(", ");
}

function backupCount(count: number): string {
  return `${count} ${count === 1 ? "backup" : "backups"}`;
}

function eventCount(count: number): string {
  return `${count} ${count === 1 ? "event" : "events"}`;
}

/** Keep Enter in a dialog field from submitting the settings form. */
function stopEnter(event: React.KeyboardEvent<HTMLInputElement>) {
  if (event.key === "Enter") {
    event.preventDefault();
  }
}

export function BackupCard({
  timezone,
  onEnterToken,
  onRestored,
}: {
  timezone: string;
  onEnterToken: () => void;
  onRestored: () => void;
}) {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [reload, setReload] = useState(0);
  const [restored, setRestored] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dialog, setDialog] = useState<"restore" | "purge" | "takeover" | null>(
    () => {
      const value = new URLSearchParams(window.location.search).get("dialog");
      return value === "restore" || value === "purge" || value === "takeover"
        ? value
        : null;
    },
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    loadBackupStatus()
      .then((next) => {
        if (!cancelled) {
          setStatus(next);
          setError(null);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setError(message(reason));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [reload]);

  useEffect(() => {
    if (new URLSearchParams(window.location.search).get("scroll") !== "backups") {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      const target = document.getElementById("backups-title");
      const content = document.getElementById("main");
      if (!target || !content) {
        return;
      }
      const top =
        target.getBoundingClientRect().top -
        content.getBoundingClientRect().top +
        content.scrollTop -
        16;
      content.scrollTo({ top, behavior: "instant" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  async function run(action: () => Promise<BackupStatus>) {
    setBusy(true);
    setError(null);
    try {
      setStatus(await action());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }

  const backups = status?.backups ?? [];
  const writer = status?.writer;
  const lockLost = Boolean(writer?.lostLock);
  const foreign = writer?.state === "foreign";
  const heartbeat = writer?.heartbeatAt
    ? formatDateTime(writer.heartbeatAt, timezone)
    : null;
  const writerTone = loading
    ? ("muted" as const)
    : !writer
      ? ("warn" as const)
      : lockLost || foreign
        ? ("danger" as const)
        : writer.state === "own"
          ? ("ok" as const)
          : ("muted" as const);
  const writerLabel = loading
    ? "Loading"
    : !writer
      ? "Unknown"
      : lockLost
        ? "Lock lost"
        : foreign
          ? "Another machine"
          : writer.state === "own"
            ? "This machine"
            : "Free";
  const writerHelp = loading
    ? "The app checks which machine holds the writer lock."
    : !writer
      ? "The app could not read the writer lock. Retry the request."
      : lockLost
        ? writer.lostLock
        : foreign
          ? `Machine ${writer.machine ?? "unknown"} sent a heartbeat ${heartbeat ?? "recently"}. The bot stays off here until you take over.`
          : writer.state === "own"
            ? `This machine holds the writer lock.${heartbeat ? ` The last heartbeat was ${heartbeat}.` : ""}`
            : "No machine holds the writer lock. The bot can start on this machine.";

  function closeRestore() {
    setDialog(null);
    setReload((value) => value + 1);
    if (restored) {
      setRestored(false);
      onRestored();
    }
  }

  function finishRestoreWithToken() {
    setDialog(null);
    setReload((value) => value + 1);
    if (restored) {
      setRestored(false);
      onRestored();
    }
    onEnterToken();
  }

  return (
    <section
      className="card backup-card"
      aria-labelledby="backups-title"
      aria-busy={loading || busy}
    >
      <div className="card__header">
        <div>
          <h2 id="backups-title">Backups and restore</h2>
          <p className="card__subtitle">
            The app backs up before each event state change and once a day.
            Backups hold participant data. Backups never hold the bot token.
            Times show in {timezone}.
          </p>
        </div>
        <StatusPill
          tone={
            loading
              ? "muted"
              : error && !status
                ? "danger"
                : status?.folderName
                  ? "ok"
                  : "warn"
          }
        >
          {loading
            ? "Loading"
            : error && !status
              ? "Error"
              : status?.folderName
                ? "On"
                : "Not set up"}
        </StatusPill>
      </div>

      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}

      <div className="channel-map">
        <div className="channel-map__row">
          <div>
            <span className="field-label">Backup folder</span>
            <span className="field-help">
              {status?.folderName
                ? "The app writes to the selected folder. Keep one copy off this machine."
                : "Select a folder on another disk or a USB drive."}{" "}
              The app keeps the newest {status?.keepTransition ?? 20}{" "}
              state-change backups and the newest {status?.keepDaily ?? 14}{" "}
              daily backups.
            </span>
          </div>
          <span className="channel-map__value">
            {loading ? (
              <Skeleton width="128px" height={14} />
            ) : error && !status ? (
              "Unavailable"
            ) : (
              status?.folderName ?? "No folder selected"
            )}
          </span>
        </div>
        <div className="channel-map__row">
          <div>
            <span className="field-label">Bot machine</span>
            <span className="field-help">{writerHelp}</span>
          </div>
          <StatusPill tone={writerTone}>{writerLabel}</StatusPill>
        </div>
      </div>

      <div className="backup-history">
        <div className="backup-history__head">
          <h3 id="backup-history-title">Backup history</h3>
          {!loading && status ? (
            <span>{backupCount(backups.length)}</span>
          ) : null}
        </div>
        {loading ? (
          <div className="backup-history__list" aria-hidden="true">
            {[0, 1, 2].map((row) => (
              <div className="backup-history__row" key={row}>
                <div className="backup-history__copy">
                  <Skeleton width="34%" height={12} />
                  <Skeleton width="52%" height={12} />
                </div>
                <Skeleton width="112px" height={12} />
              </div>
            ))}
          </div>
        ) : error && !status ? (
          <p className="backup-history__empty">
            The app could not read the backup folder. Select Retry.
          </p>
        ) : !status?.folderName ? (
          <p className="backup-history__empty">
            Select a backup folder. The app writes the first backup when you
            select Back up now.
          </p>
        ) : backups.length === 0 ? (
          <p className="backup-history__empty">
            No backup yet. Select Back up now to write the first backup.
          </p>
        ) : (
          <ul
            className="backup-history__list"
            aria-labelledby="backup-history-title"
          >
            {backups.map((item) => (
              <li className="backup-history__row" key={item.id}>
                <div className="backup-history__copy">
                  <span className="field-label">
                    {kindLabel[item.kind] ?? item.kind}
                    {item.complete ? "" : " · Incomplete"}
                  </span>
                  <span className="field-help">{eventsText(item)}</span>
                </div>
                <span className="backup-history__time">
                  {item.complete
                    ? formatDateTime(item.createdAt, timezone)
                    : "Not written"}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="settings-header-actions backup-card__actions">
        <Button
          variant="secondary"
          icon="folder"
          disabled={busy || loading}
          onClick={() => run(chooseBackupFolder)}
        >
          {status?.folderName ? "Change folder" : "Choose folder"}
        </Button>
        <Button
          variant="secondary"
          icon="download"
          disabled={busy || loading || !status?.folderName}
          onClick={() => run(backupNow)}
        >
          Back up now
        </Button>
        <Button
          variant="secondary"
          icon="refresh"
          disabled={busy || loading}
          onClick={() => setDialog("restore")}
        >
          Restore from a backup
        </Button>
        {foreign || lockLost ? (
          <Button
            variant="danger"
            icon="lock"
            disabled={busy || loading}
            onClick={() => setDialog("takeover")}
          >
            Take over
          </Button>
        ) : null}
        {error && !status ? (
          <Button
            variant="secondary"
            icon="refresh"
            disabled={busy || loading}
            onClick={() => setReload((value) => value + 1)}
          >
            Retry
          </Button>
        ) : null}
        <Button
          variant="danger"
          icon="trash"
          disabled={busy || loading || backups.length === 0}
          onClick={() => setDialog("purge")}
        >
          Delete all backups
        </Button>
      </div>

      {dialog === "restore" ? (
        <RestoreWizard
          timezone={timezone}
          onRestored={() => setRestored(true)}
          onClose={closeRestore}
          onEnterToken={finishRestoreWithToken}
        />
      ) : null}
      {dialog === "purge" ? (
        <TypedConfirm
          title="Delete every backup?"
          word="PURGE"
          action="Delete backups"
          onClose={() => setDialog(null)}
          onConfirm={(word) => run(() => purgeBackups(word))}
        >
          <p>
            This deletes every backup and every backup file in{" "}
            {status?.folderName ?? "the backup folder"}. Use it after a
            participant asks for deletion, so old backups do not keep their
            data. The current event data stays. This cannot be undone.
          </p>
        </TypedConfirm>
      ) : null}
      {dialog === "takeover" ? (
        <TypedConfirm
          title="Run the event on this machine?"
          word="TAKE OVER"
          action="Take over"
          onClose={() => setDialog(null)}
          onConfirm={(word) => run(() => takeOverWriter(word))}
        >
          <p>
            Stop the bot on machine {writer?.machine ?? "the other machine"}{" "}
            first. Two bots for one event send duplicate messages. The app
            records the takeover in the audit log.
          </p>
        </TypedConfirm>
      ) : null}
    </section>
  );
}

function TypedConfirm({
  title,
  word,
  action,
  children,
  onClose,
  onConfirm,
}: {
  title: string;
  word: string;
  action: string;
  children: React.ReactNode;
  onClose: () => void;
  onConfirm: (word: string) => void;
}) {
  const [typed, setTyped] = useState("");
  return (
    <Dialog
      title={title}
      labelledBy="typed-confirm-title"
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            disabled={typed.trim() !== word}
            onClick={() => {
              onClose();
              onConfirm(typed.trim());
            }}
          >
            {action}
          </Button>
        </>
      }
    >
      {children}
      <p className="dialog__warning">
        Type <code>{word}</code> to continue.
      </p>
      <Field label={`Type ${word} to confirm`} htmlFor="typed-confirm">
        <input
          id="typed-confirm"
          className="input"
          value={typed}
          autoComplete="off"
          onKeyDown={stopEnter}
          onChange={(event) => setTyped(event.target.value)}
        />
      </Field>
    </Dialog>
  );
}

const stalenessText: Record<RestorePreview["staleness"], string> = {
  none: "This computer has no event data. The restore adds the backup.",
  same: "The backup matches the data on this computer.",
  older:
    "This backup is older than the data on this computer. The restore removes the newer changes from the active database.",
  newer: "This backup has changes that this computer does not have.",
  diverged:
    "This backup comes from a different history than the data on this computer.",
};

function RestoreWizard({
  timezone,
  onClose,
  onRestored,
  onEnterToken,
}: {
  timezone: string;
  onClose: () => void;
  onRestored: () => void;
  onEnterToken: () => void;
}) {
  const [source, setSource] = useState<RestoreSource | null>(null);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [done, setDone] = useState<RestoreDone | null>(null);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function start() {
      try {
        const next = await loadRestoreSource();
        if (cancelled) {
          return;
        }
        setSource(next);
        setLoading(false);
        const step = isDemoMode()
          ? new URLSearchParams(window.location.search).get("restore")
          : null;
        if (step !== "preview" && step !== "done" && step !== "error") {
          return;
        }
        const first = next.backups.find((item) => item.complete);
        if (!first) {
          return;
        }
        setSelected(first.id);
        setBusy(true);
        try {
          const checked = await previewRestore(first.id);
          if (cancelled) {
            return;
          }
          setPreview(checked);
          if (step === "done") {
            const result = await restoreBackup(first.id, "RESTORE");
            if (!cancelled) {
              setDone(result);
              onRestored();
            }
          }
        } finally {
          if (!cancelled) {
            setBusy(false);
          }
        }
      } catch (reason) {
        if (!cancelled) {
          setError(message(reason));
          setLoading(false);
          setBusy(false);
        }
      }
    }
    void start();
    return () => {
      cancelled = true;
    };
    // The wizard reads its demo stage once. Host data arrives from the call.
  }, []);

  useEffect(() => {
    if (done) {
      document.getElementById("restore-add-token")?.focus();
    } else if (preview) {
      document.getElementById("restore-confirm")?.focus();
    } else if (source) {
      document
        .querySelector<HTMLInputElement>('input[name="restore-backup"]')
        ?.focus();
    }
  }, [source, preview, done]);

  function requestClose() {
    if (!busy) {
      onClose();
    }
  }

  async function step<T>(action: () => Promise<T>, apply: (value: T) => void) {
    setBusy(true);
    setError(null);
    try {
      apply(await action());
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }

  const complete: BackupItem[] =
    source?.backups.filter((item) => item.complete) ?? [];

  const footer = done ? (
    <>
      <Button variant="secondary" onClick={onClose}>
        Close
      </Button>
      <Button
        id="restore-add-token"
        variant="primary"
        icon="bot"
        onClick={onEnterToken}
      >
        Add bot token
      </Button>
    </>
  ) : preview ? (
    <>
      <Button
        variant="secondary"
        disabled={busy}
        onClick={() => {
          setPreview(null);
          setTyped("");
        }}
      >
        Back
      </Button>
      <Button
        variant="danger"
        disabled={busy || typed.trim() !== "RESTORE"}
        onClick={() =>
          step(
            () => restoreBackup(preview.id, typed.trim()),
            (result) => {
              setDone(result);
              onRestored();
            },
          )
        }
      >
        Restore
      </Button>
    </>
  ) : (
    <>
      <Button variant="secondary" disabled={busy} onClick={requestClose}>
        Cancel
      </Button>
      <Button
        variant="primary"
        disabled={loading || busy || !selected}
        onClick={() =>
          selected ? step(() => previewRestore(selected), setPreview) : undefined
        }
      >
        Check backup
      </Button>
    </>
  );

  return (
    <Dialog
      title={done ? "Backup restored" : "Restore from a backup"}
      labelledBy="restore-title"
      onClose={requestClose}
      footer={footer}
    >
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
      {loading ? (
        <p role="status">Reading the backup folder.</p>
      ) : null}
      {busy ? (
        <p role="status">Checking the backup files. This can take a minute.</p>
      ) : null}

      {done ? (
        <>
          <p>
            The app restored {eventCount(done.events.length)}. The bot is off.
          </p>
          {done.events.length > 0 ? (
            <ul className="restore-events">
              {done.events.map((event) => (
                <li key={`${event.name}-${event.state}`}>
                  {event.name} ({stateLabel[event.state] ?? event.state})
                </li>
              ))}
            </ul>
          ) : null}
          <p>
            Backups never hold the bot token. Add the token again to connect
            the bot.
            {done.keptPrevious
              ? " The app kept the replaced database in the pre-restore folder of the app data folder."
              : ""}
          </p>
        </>
      ) : preview ? (
        <>
          <p>
            {kindLabel[preview.kind] ?? preview.kind} backup from{" "}
            {formatDateTime(preview.createdAt, timezone)}. The check passed for
            the database and {preview.files}{" "}
            {preview.files === 1 ? "file" : "files"}. The backup was written by
            app version {preview.appVersion}.
          </p>
          <p>Events: {eventsText(preview)}.</p>
          {preview.staleness === "older" || preview.staleness === "diverged" ? (
            <div
              className={`warning warning--${
                preview.staleness === "diverged" ? "danger" : "warn"
              }`}
              role="alert"
            >
              <Icon name="alert" size={18} />
              <div>
                <p>
                  {stalenessText[preview.staleness]}
                  {preview.staleness === "older"
                    ? ` The current data has ${preview.newerEntries} newer audit ${preview.newerEntries === 1 ? "entry" : "entries"}.`
                    : ""}
                </p>
              </div>
            </div>
          ) : (
            <p>{stalenessText[preview.staleness]}</p>
          )}
          <p className="dialog__warning">
            The app stops the bot and replaces the event data. The app keeps the
            replaced database in a pre-restore folder. Type <code>RESTORE</code>{" "}
            to continue.
          </p>
          <Field label="Type RESTORE to confirm" htmlFor="restore-confirm">
            <input
              id="restore-confirm"
              className="input"
              value={typed}
              autoComplete="off"
              onKeyDown={stopEnter}
              onChange={(event) => setTyped(event.target.value)}
            />
          </Field>
        </>
      ) : (
        <>
          <p>
            The app checks the backup before it changes anything. The bot stops
            during the restore. Times show in {timezone}.
          </p>
          <p>
            Folder: {source?.folderName ?? "No folder selected"}.{" "}
            <button
              type="button"
              className="link-button"
              disabled={busy}
              onClick={() =>
                step(chooseRestoreFolder, (next) => {
                  setSource(next);
                  setSelected(null);
                })
              }
            >
              Choose another folder
            </button>
          </p>
          {complete.length === 0 ? (
            <p>
              No complete backup is in this folder. The app does not list a
              backup without a manifest.
            </p>
          ) : (
            <fieldset className="restore-options">
              <legend className="field-label">Backup</legend>
              {complete.map((item) => (
                <label key={item.id} className="restore-option">
                  <input
                    type="radio"
                    name="restore-backup"
                    value={item.id}
                    checked={selected === item.id}
                    onChange={() => setSelected(item.id)}
                  />
                  <span>
                    {formatDateTime(item.createdAt, timezone)} ·{" "}
                    {kindLabel[item.kind] ?? item.kind} · {eventsText(item)}
                  </span>
                </label>
              ))}
            </fieldset>
          )}
        </>
      )}
    </Dialog>
  );
}
