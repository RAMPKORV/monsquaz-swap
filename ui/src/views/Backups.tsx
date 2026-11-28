import { useEffect, useState } from "react";
import type {
  BackupItem,
  BackupStatus,
  HostProblem,
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
  describeHostError,
  isDemoMode,
  loadBackupStatus,
  loadRestoreSource,
  openRecoveryFolder,
  previewRestore,
  purgeBackups,
  restoreBackup,
  revealBackupFolder,
  takeOverWriter,
} from "../data/client";
import { formatDateTime } from "../i18n";

const kindLabel: Record<string, string> = {
  transition: "Before state change",
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

function focusById(id: string) {
  window.requestAnimationFrame(() => {
    document.getElementById(id)?.focus();
  });
}

/** Keep Enter in a dialog field from submitting the settings form. */
function stopEnter(event: React.KeyboardEvent<HTMLInputElement>) {
  if (event.key === "Enter") {
    event.preventDefault();
  }
}

function ProblemView({
  problem,
  tone = "error",
}: {
  problem: HostProblem;
  tone?: "error" | "warning";
}) {
  if (tone === "warning") {
    return (
      <div className="warning warning--warn" role="status">
        <Icon name="alert" size={18} />
        <div>
          <p>{problem.message}</p>
          <p className="problem-detail">
            Error code <code>{problem.code}</code>
          </p>
          {problem.detail ? (
            <p className="problem-detail">Technical detail: {problem.detail}</p>
          ) : null}
        </div>
      </div>
    );
  }
  return (
    <div className="field-error" role="alert">
      <p>{problem.message}</p>
      <p className="error-code">
        Error code <code>{problem.code}</code>
      </p>
      {problem.detail ? (
        <p className="problem-detail">Technical detail: {problem.detail}</p>
      ) : null}
    </div>
  );
}

export function BackupCard({
  timezone,
  tokenConfigured,
  onEnterToken,
  onRestored,
  onBotStatus,
}: {
  timezone: string;
  tokenConfigured: boolean;
  onEnterToken: () => void;
  onRestored: () => void;
  onBotStatus: () => void;
}) {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [reload, setReload] = useState(0);
  const [restored, setRestored] = useState(false);
  const [problem, setProblem] = useState<HostProblem | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
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
          setProblem(null);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setProblem(describeHostError(reason));
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
      if (typeof content.scrollTo === "function") {
        content.scrollTo({ top, behavior: "instant" });
      } else {
        content.scrollTop = top;
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  async function apply(action: () => Promise<BackupStatus>): Promise<BackupStatus> {
    setBusy(true);
    setProblem(null);
    setNotice(null);
    try {
      const next = await action();
      setStatus(next);
      return next;
    } catch (reason) {
      const next = describeHostError(reason);
      setProblem(next);
      loadBackupStatus().then(setStatus).catch(() => undefined);
      throw next;
    } finally {
      setBusy(false);
    }
  }

  async function run(action: () => Promise<BackupStatus>) {
    try {
      return await apply(action);
    } catch {
      return null;
    }
  }

  async function handleChooseFolder() {
    const previous = status?.folderName ?? null;
    const next = await run(chooseBackupFolder);
    if (!next) {
      return;
    }
    if (next.folderName !== previous) {
      setNotice("Backup folder changed.");
    }
    focusById("backup-folder-button");
  }

  async function handleBackupNow() {
    if (!(await run(backupNow))) {
      return;
    }
    setNotice("Backup written.");
    focusById("backup-now-button");
  }

  async function handleCheckLock() {
    if (!(await run(loadBackupStatus))) {
      return;
    }
    setNotice("Bot lock checked.");
    focusById("check-bot-lock");
  }

  async function handleRevealFolder() {
    setProblem(null);
    try {
      await revealBackupFolder();
    } catch (reason) {
      setProblem(describeHostError(reason));
    }
  }

  const backups = status?.backups ?? [];
  const complete = backups.filter((item) => item.complete);
  const incomplete = backups.length - complete.length;
  const folderError = status?.folderError ?? null;
  const lastFailure = status?.lastFailure ?? null;
  const writer = status?.writer;
  const lostLockAt = writer?.lostLockAt ?? null;
  const foreign = writer?.state === "foreign";
  const writerLabel = loading
    ? "Loading"
    : !writer
      ? "Unknown"
      : lostLockAt
        ? "Another computer took over"
        : writer.state === "own"
          ? "This computer"
          : writer.state === "foreign"
            ? "Another computer"
            : "No computer";
  const writerTone = loading
    ? ("muted" as const)
    : !writer
      ? ("warn" as const)
      : foreign || lostLockAt
        ? ("danger" as const)
        : ("muted" as const);
  const writerHelp = loading
    ? "The app checks which computer holds the bot lock."
    : !writer
      ? "The app could not read the bot lock. Choose Check bot lock."
      : lostLockAt
        ? `Another computer took over at ${formatDateTime(lostLockAt, timezone)}. The bot stopped on this computer. Do not take over unless you stopped the bot on the other computer.`
        : writer.state === "own"
          ? `This computer can run the bot. This does not mean the bot is online. Checked at ${formatDateTime(status?.checkedAt ?? 0, timezone)}.`
          : writer.state === "foreign"
            ? `Another computer checked in at ${formatDateTime(writer.heartbeatAt ?? 0, timezone)}. The bot stays off on this computer while the other computer checks in. The lock clears 5 minutes after the last check. Choose Take over only if you stopped the bot on that computer. Then choose Reconnect bot to start the bot.`
            : `No computer holds the bot lock. The bot can start on this computer. Checked at ${formatDateTime(status?.checkedAt ?? 0, timezone)}.`;
  const statusTone = loading
    ? ("muted" as const)
    : folderError
      ? ("danger" as const)
      : problem && !status
        ? ("danger" as const)
        : lastFailure || lostLockAt
          ? ("warn" as const)
          : status?.folderName
            ? ("ok" as const)
            : ("warn" as const);
  const statusLabel = loading
    ? "Loading"
    : folderError
      ? "Folder error"
      : problem && !status
        ? "Error"
        : lastFailure || lostLockAt
          ? "Attention"
          : status?.folderName
            ? "On"
            : "Not set up";

  function closeRestore() {
    setDialog(null);
    setReload((value) => value + 1);
    if (restored) {
      setRestored(false);
      onRestored();
      focusById("backups-title");
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
          <h2 id="backups-title" tabIndex={-1}>
            Backups and restore
          </h2>
          <p className="card__subtitle">
            A backup record holds participant data, stored track files, and the
            audit log. The app backs up before each event state change and once
            a day while the app runs. Backups never hold the bot token. Do not
            use a cloud-synced folder. Times show in {timezone}.
          </p>
        </div>
        <StatusPill tone={statusTone}>{statusLabel}</StatusPill>
      </div>

      {notice ? (
        <p className="save-state" role="status">
          <Icon name="check" size={16} />
          {notice}
        </p>
      ) : null}

      {folderError ? <ProblemView problem={folderError} /> : null}
      {problem && !folderError ? <ProblemView problem={problem} /> : null}
      {lastFailure && !folderError && !problem ? (
        <ProblemView
          problem={{
            ...lastFailure,
            message: `The last backup failed at ${formatDateTime(lastFailure.at, timezone)}. ${lastFailure.message}`,
          }}
          tone="warning"
        />
      ) : null}

      <div className="channel-map">
        <div className="channel-map__row">
          <div>
            <span className="field-label">Backup folder</span>
            <span className="field-help">
              {loading
                ? "The app reads the backup folder."
                : folderError
                  ? "The app cannot read the chosen folder."
                  : status?.folderName
                    ? "The app writes to the chosen folder. Keep one copy off this computer."
                    : "Choose a folder on another disk or a USB drive."}{" "}
              The app keeps the newest {status?.keepTransition ?? 20}{" "}
              state-change and manual backups and the newest{" "}
              {status?.keepDaily ?? 14} daily backups.
            </span>
          </div>
          <span className="channel-map__value">
            {loading ? (
              <Skeleton width="128px" height={14} />
            ) : folderError ? (
              "Unavailable"
            ) : (
              status?.folderName ?? "No folder chosen"
            )}
          </span>
        </div>
        {!folderError ? (
          <div className="channel-map__row">
            <div>
              <span className="field-label">Bot lock</span>
              <span className="field-help">{writerHelp}</span>
            </div>
            <div className="bot-lock">
              <StatusPill tone={writerTone} icon="lock">
                {writerLabel}
              </StatusPill>
              {!loading ? (
                <button
                  id="check-bot-lock"
                  type="button"
                  className="link-button"
                  disabled={busy}
                  onClick={handleCheckLock}
                >
                  Check bot lock
                </button>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>

      <div className="backup-history">
        <div className="backup-history__head">
          <h3 id="backup-history-title">Backup history</h3>
          {!loading && status && !folderError ? (
            <span>
              {incomplete > 0
                ? `${complete.length} complete, ${incomplete} incomplete`
                : backupCount(backups.length)}
            </span>
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
        ) : folderError ? (
          <p className="backup-history__empty">
            The app cannot show the backups until it reads the folder.
          </p>
        ) : !status?.folderName ? (
          <p className="backup-history__empty">
            No backups yet. Choose a folder, then choose Back up now.
          </p>
        ) : backups.length === 0 ? (
          <p className="backup-history__empty">
            No backups in this folder. Choose Back up now to write the first
            backup.
          </p>
        ) : (
          <>
            {incomplete > 0 ? (
              <p className="backup-history__note">
                The app did not finish an incomplete backup. It cannot restore
                it. The next backup replaces it.
              </p>
            ) : null}
            <ul
              className="backup-history__list"
              role="list"
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
          </>
        )}
      </div>

      <div className="settings-header-actions backup-card__actions">
        <Button
          id="backup-folder-button"
          variant={
            !status?.folderName && !folderError && !(problem && !status)
              ? "primary"
              : "secondary"
          }
          icon="folder"
          disabled={busy || loading}
          onClick={handleChooseFolder}
        >
          {status?.folderName ? "Change folder" : "Choose folder"}
        </Button>
        <Button
          id="backup-now-button"
          variant={lastFailure ? "primary" : "secondary"}
          icon="download"
          disabled={busy || loading || !status?.folderName || Boolean(folderError)}
          onClick={handleBackupNow}
        >
          Back up now
        </Button>
        {!folderError && status?.folderName ? (
          <Button
            variant="secondary"
            icon="folder"
            disabled={busy || loading}
            onClick={handleRevealFolder}
          >
            Reveal backup folder
          </Button>
        ) : null}
        {!folderError ? (
          <Button
            id="restore-open-button"
            variant="secondary"
            icon="refresh"
            disabled={busy || loading}
            onClick={() => setDialog("restore")}
          >
            Restore from a backup
          </Button>
        ) : null}
        {!folderError && (foreign || lostLockAt) ? (
          <Button
            variant="danger"
            icon="lock"
            disabled={busy || loading}
            onClick={() => setDialog("takeover")}
          >
            Take over
          </Button>
        ) : null}
        {(folderError || (problem && !status)) ? (
          <Button
            id="backup-retry-button"
            variant="primary"
            icon="refresh"
            disabled={busy || loading}
            onClick={() => setReload((value) => value + 1)}
          >
            Retry
          </Button>
        ) : null}
        {backups.length > 0 && !foreign && !lostLockAt && !folderError ? (
          <Button
            variant="danger"
            icon="trash"
            disabled={busy || loading}
            onClick={() => setDialog("purge")}
          >
            Delete all backups
          </Button>
        ) : null}
      </div>

      {dialog === "restore" ? (
        <RestoreWizard
          timezone={timezone}
          tokenConfigured={tokenConfigured}
          returnFocusId={restored ? "backups-title" : undefined}
          onRestored={() => {
            setRestored(true);
            onBotStatus();
          }}
          onClose={closeRestore}
          onEnterToken={finishRestoreWithToken}
        />
      ) : null}
      {dialog === "purge" ? (
        <TypedConfirm
          title="Delete all backups?"
          word="PURGE"
          action="Delete all backups"
          returnFocusId="backups-title"
          onClose={() => setDialog(null)}
          onSuccess={() => setNotice("All backups deleted.")}
          onConfirm={async (word) => {
            await apply(() => purgeBackups(word));
          }}
        >
          <p>
            This deletes {backupCount(backups.length)} in{" "}
            {status?.folderName ?? "the backup folder"} for every event,
            including backups for other events. The current event data stays.
            You cannot undo this.
          </p>
        </TypedConfirm>
      ) : null}
      {dialog === "takeover" ? (
        <TypedConfirm
          title="Run the event on this computer?"
          word="TAKE OVER"
          action="Take over"
          returnFocusId="backups-title"
          onClose={() => setDialog(null)}
          onSuccess={() => setNotice("This computer now holds the bot lock.")}
          onConfirm={async (word) => {
            await apply(() => takeOverWriter(word));
          }}
        >
          <p>
            Stop the bot on the other computer first. Two bots for the same
            event send duplicate messages. The app records the takeover for
            every event in this data folder. Then choose Reconnect bot to start
            the bot.
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
  returnFocusId,
  onClose,
  onSuccess,
  onConfirm,
}: {
  title: string;
  word: string;
  action: string;
  children: React.ReactNode;
  returnFocusId: string;
  onClose: () => void;
  onSuccess: () => void;
  onConfirm: (word: string) => Promise<void>;
}) {
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<HostProblem | null>(null);
  const matches = typed.trim() === word;

  function requestClose() {
    if (!busy) {
      onClose();
    }
  }

  async function submit() {
    if (!matches || busy) {
      return;
    }
    setBusy(true);
    setProblem(null);
    try {
      await onConfirm(typed.trim());
      onClose();
      onSuccess();
    } catch (reason) {
      setProblem(describeHostError(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      title={title}
      labelledBy="typed-confirm-title"
      describedBy="typed-confirm-instructions"
      onClose={requestClose}
      closeDisabled={busy}
      returnFocusId={returnFocusId}
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={requestClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            disabled={busy || !matches}
            onClick={submit}
          >
            {action}
          </Button>
        </>
      }
    >
      {children}
      {problem ? <ProblemView problem={problem} /> : null}
      <p className="dialog__warning" id="typed-confirm-instructions">
        Type <code>{word}</code> in capital letters to continue.
      </p>
      <Field label="Confirmation" htmlFor="typed-confirm">
        <input
          id="typed-confirm"
          className="input"
          value={typed}
          autoComplete="off"
          aria-invalid={typed.length > 0 && !matches}
          aria-describedby={
            typed.length > 0 && !matches ? "typed-confirm-hint" : undefined
          }
          onKeyDown={(event) => {
            stopEnter(event);
            if (event.key === "Enter" && matches) {
              void submit();
            }
          }}
          onChange={(event) => setTyped(event.target.value)}
        />
      </Field>
      {typed.length > 0 && !matches ? (
        <p className="field-error" id="typed-confirm-hint" role="alert">
          Type {word} in capital letters.
        </p>
      ) : null}
    </Dialog>
  );
}

const stalenessText: Record<RestorePreview["staleness"], string> = {
  none: "This computer has no event data. The restore adds the backup.",
  same: "The backup matches the event data on this computer.",
  older:
    "This backup is older than the event data on this computer. The restore removes claims and submissions after the backup. A participant can receive a track that they already had. Restore only if you must.",
  newer: "This backup has changes that this computer does not have.",
  diverged:
    "This backup comes from a different history. The record of who had which track can be wrong. A participant can receive a track that they already had. Restore only if you must.",
};

function RestoreWizard({
  timezone,
  tokenConfigured,
  returnFocusId,
  onClose,
  onRestored,
  onEnterToken,
}: {
  timezone: string;
  tokenConfigured: boolean;
  returnFocusId?: string;
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
  const [busy, setBusy] = useState<"folder" | "preview" | "restore" | null>(
    null,
  );
  const [problem, setProblem] = useState<HostProblem | null>(null);
  const [recoveryProblem, setRecoveryProblem] = useState<HostProblem | null>(
    null,
  );

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
        setBusy("preview");
        try {
          const checked = await previewRestore(first.id);
          if (cancelled) {
            return;
          }
          setPreview(checked);
          if (step === "done") {
            setBusy("restore");
            const result = await restoreBackup(first.id, "RESTORE");
            if (!cancelled) {
              setDone(result);
              onRestored();
            }
          }
        } finally {
          if (!cancelled) {
            setBusy(null);
          }
        }
      } catch (reason) {
        if (!cancelled) {
          setProblem(describeHostError(reason));
          setLoading(false);
          setBusy(null);
        }
      }
    }
    void start();
    return () => {
      cancelled = true;
    };
  }, []);

  const complete: BackupItem[] =
    source?.backups.filter((item) => item.complete) ?? [];

  useEffect(() => {
    if (done) {
      document.getElementById("restore-add-token")?.focus();
    } else if (preview) {
      document.getElementById("restore-confirm")?.focus();
    } else if (source && complete.length === 0) {
      document.getElementById("restore-choose-folder")?.focus();
    } else if (source) {
      const checked = document.querySelector<HTMLInputElement>(
        'input[name="restore-backup"]:checked',
      );
      (checked ??
        document.querySelector<HTMLInputElement>(
          'input[name="restore-backup"]',
        ))?.focus();
    }
  }, [source, preview, done, complete.length]);

  function requestClose() {
    if (!busy) {
      onClose();
    }
  }

  async function step<T>(
    phase: "folder" | "preview" | "restore",
    action: () => Promise<T>,
    apply: (value: T) => void,
  ) {
    setBusy(phase);
    setProblem(null);
    try {
      apply(await action());
    } catch (reason) {
      setProblem(describeHostError(reason));
    } finally {
      setBusy(null);
    }
  }

  async function chooseFolder() {
    await step("folder", chooseRestoreFolder, (next) => {
      setSource(next);
      setSelected(null);
      setPreview(null);
      setTyped("");
    });
  }

  async function submitRestore() {
    if (!preview || typed.trim() !== "RESTORE") {
      return;
    }
    await step(
      "restore",
      () => restoreBackup(preview.id, typed.trim()),
      (result) => {
        setDone(result);
        onRestored();
      },
    );
  }

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
        {tokenConfigured ? "Reconnect bot" : "Add bot token"}
      </Button>
    </>
  ) : preview ? (
    <>
      <Button
        variant="secondary"
        disabled={busy !== null}
        onClick={() => {
          setPreview(null);
          setTyped("");
        }}
      >
        Back
      </Button>
      <Button
        variant="danger"
        disabled={busy !== null || typed.trim() !== "RESTORE"}
        onClick={submitRestore}
      >
        Restore
      </Button>
    </>
  ) : (
    <>
      <Button variant="secondary" disabled={busy !== null} onClick={requestClose}>
        Cancel
      </Button>
      {source && complete.length > 0 ? (
        <Button
          variant="primary"
          disabled={busy !== null || loading || !selected}
          describedBy={!selected ? "restore-check-help" : undefined}
          onClick={() =>
            selected
              ? step("preview", () => previewRestore(selected), setPreview)
              : undefined
          }
        >
          Check backup
        </Button>
      ) : (
        <Button
          id="restore-choose-folder"
          variant="primary"
          icon="folder"
          disabled={busy !== null || loading}
          onClick={chooseFolder}
        >
          {source?.folderName ? "Choose another folder" : "Choose folder"}
        </Button>
      )}
    </>
  );

  const previewWarning =
    preview &&
    (preview.staleness === "older" || preview.staleness === "diverged");
  const doneKind =
    preview?.kind === "transition"
      ? "state-change"
      : preview
        ? (kindLabel[preview.kind] ?? preview.kind)
        : null;

  return (
    <Dialog
      title={done ? "Backup restored" : "Restore from a backup"}
      labelledBy="restore-title"
      describedBy="restore-instructions"
      onClose={requestClose}
      closeDisabled={busy !== null}
      returnFocusId={returnFocusId}
      footer={footer}
    >
      {problem ? <ProblemView problem={problem} /> : null}
      {done ? (
        <p id="restore-instructions">The restore is complete.</p>
      ) : loading ? (
        <p role="status" id="restore-instructions">
          The app reads the backup folder.
        </p>
      ) : (
        <p id="restore-instructions">
          The app checks the backup before it changes anything. The bot stops
          during the restore. Times show in {timezone}.
        </p>
      )}
      {busy === "folder" ? <p role="status">The app reads the folder.</p> : null}
      {busy === "preview" ? (
        <p role="status">The app checks the backup. This can take a minute.</p>
      ) : null}
      {busy === "restore" ? (
        <p role="status">The app restores the backup. Do not close the app.</p>
      ) : null}

      {done ? (
        <>
          <p role="status">
            {eventCount(done.events.length)} restored
            {preview
              ? ` from the ${doneKind} backup at ${formatDateTime(preview.createdAt, timezone)}`
              : ""}
            . The bot is off.
          </p>
          {done.events.length === 1 ? (
            <p>
              {done.events[0].name} (
              {stateLabel[done.events[0].state] ?? done.events[0].state})
            </p>
          ) : done.events.length > 1 ? (
            <ul className="restore-events">
              {done.events.map((event) => (
                <li key={`${event.name}-${event.state}`}>
                  {event.name} ({stateLabel[event.state] ?? event.state})
                </li>
              ))}
            </ul>
          ) : null}
          {tokenConfigured ? (
            <p>
              The app stopped the bot. Choose Reconnect bot to start the bot.
              The bot token stays in the operating system keychain.
            </p>
          ) : (
            <p>
              This computer has no bot token. Add the token to connect the bot.
            </p>
          )}
          <p>
            If the bot does not start, go to Bot lock. Stop the bot on the
            other computer, then choose Take over.
          </p>
          {done.keptPrevious ? (
            <p>
              The app kept the replaced event data in a recovery folder.{" "}
              <button
                type="button"
                className="link-button"
                onClick={async () => {
                  setRecoveryProblem(null);
                  try {
                    await openRecoveryFolder();
                  } catch (reason) {
                    setRecoveryProblem(describeHostError(reason));
                  }
                }}
              >
                Open recovery folder
              </button>
            </p>
          ) : null}
          {recoveryProblem ? <ProblemView problem={recoveryProblem} /> : null}
        </>
      ) : preview ? (
        <>
          <p>
            {kindLabel[preview.kind] ?? preview.kind} backup from{" "}
            {formatDateTime(preview.createdAt, timezone)} ({timezone}). The
            check passed for the event data and {preview.files}{" "}
            {preview.files === 1 ? "file" : "files"}. The app version was{" "}
            {preview.appVersion}.
          </p>
          <p>Events: {eventsText(preview)}.</p>
          {previewWarning ? (
            <div
              className={`warning warning--${
                preview.staleness === "diverged" ? "danger" : "warn"
              }`}
              role="status"
            >
              <Icon name="alert" size={18} />
              <div>
                <p>
                  {stalenessText[preview.staleness]}
                  {preview.staleness === "older"
                    ? ` The current event data has ${preview.newerEntries} newer audit ${preview.newerEntries === 1 ? "entry" : "entries"}.`
                    : ""}
                </p>
              </div>
            </div>
          ) : (
            <p role="status">{stalenessText[preview.staleness]}</p>
          )}
          <p className="dialog__warning">
            The app stops the bot and replaces the event data. The app keeps the
            replaced event data in a recovery folder. Type{" "}
            <code>RESTORE</code> in capital letters to continue.
          </p>
          <Field label="Confirmation" htmlFor="restore-confirm">
            <input
              id="restore-confirm"
              className="input"
              value={typed}
              autoComplete="off"
              aria-invalid={typed.length > 0 && typed.trim() !== "RESTORE"}
              aria-describedby={
                typed.length > 0 && typed.trim() !== "RESTORE"
                  ? "restore-confirm-hint"
                  : undefined
              }
              onKeyDown={(event) => {
                stopEnter(event);
                if (event.key === "Enter" && typed.trim() === "RESTORE") {
                  void submitRestore();
                }
              }}
              onChange={(event) => setTyped(event.target.value)}
            />
          </Field>
          {typed.length > 0 && typed.trim() !== "RESTORE" ? (
            <p className="field-error" id="restore-confirm-hint" role="alert">
              Type RESTORE in capital letters.
            </p>
          ) : null}
        </>
      ) : (
        <>
          {source?.folderName ? (
            <>
              <p>
                Folder: {source.folderName}.{" "}
                {complete.length > 0 ? (
                  <button
                    type="button"
                    className="link-button"
                    disabled={busy !== null}
                    onClick={chooseFolder}
                  >
                    Choose another folder
                  </button>
                ) : null}
              </p>
              <p className="field-help">
                Choose a different restore folder. This does not change the
                backup folder.
              </p>
            </>
          ) : null}
          {complete.length === 0 ? (
            <p>
              {source?.folderName
                ? "This folder has no complete backup. Choose another folder. If the backup is on another computer, copy the folder to this computer first. The app lists only backups it can check."
                : "No backup folder is selected. Choose a folder."}
            </p>
          ) : (
            <>
              <p className="field-help" id="restore-check-help">
                Choose a backup to check.
              </p>
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
            </>
          )}
        </>
      )}
    </Dialog>
  );
}
