import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { BackupCard } from "./views/Backups";
import {
  backupNow,
  chooseBackupFolder,
  loadBackupStatus,
  loadRestoreSource,
  openRecoveryFolder,
  previewRestore,
  purgeBackups,
  restoreBackup,
  takeOverWriter,
} from "./data/client";
import type { BackupStatus, RestorePreview } from "./types";

vi.mock("./data/client", () => ({
  isDemoMode: () => true,
  describeHostError: (error: unknown, fallback = "MSW-UI-001") => {
    if (
      error &&
      typeof error === "object" &&
      "code" in error &&
      "message" in error
    ) {
      const value = error as {
        code: string;
        message: string;
        detail?: string | null;
      };
      return {
        code: value.code,
        message: value.message,
        detail: value.detail ?? null,
      };
    }
    if (error instanceof Error) {
      return { code: fallback, message: error.message, detail: null };
    }
    return { code: fallback, message: String(error), detail: null };
  },
  backupNow: vi.fn(),
  chooseBackupFolder: vi.fn(),
  chooseRestoreFolder: vi.fn(),
  loadBackupStatus: vi.fn(),
  loadRestoreSource: vi.fn(),
  openRecoveryFolder: vi.fn().mockResolvedValue(undefined),
  previewRestore: vi.fn(),
  purgeBackups: vi.fn(),
  restoreBackup: vi.fn(),
  revealBackupFolder: vi.fn().mockResolvedValue(undefined),
  takeOverWriter: vi.fn(),
}));

const T0 = 1_800_000_000;

function status(overrides: Partial<BackupStatus> = {}): BackupStatus {
  return {
    folderName: "Swap backups",
    folderError: null,
    lastFailure: null,
    checkedAt: T0 - 15,
    keepTransition: 20,
    keepDaily: 14,
    backups: [
      {
        id: "backup-daily",
        kind: "daily",
        createdAt: T0,
        complete: true,
        events: [{ name: "Night Shift Vol. 2", state: "active" }],
      },
      {
        id: "backup-transition",
        kind: "transition",
        createdAt: T0 - 86_400,
        complete: true,
        events: [{ name: "Night Shift Vol. 2", state: "signup" }],
      },
    ],
    writer: {
      state: "own",
      heartbeatAt: T0 - 120,
      lostLockAt: null,
    },
    ...overrides,
  };
}

const preview: RestorePreview = {
  id: "backup-daily",
  kind: "daily",
  createdAt: T0,
  appVersion: "0.1.1",
  files: 31,
  events: [{ name: "Night Shift Vol. 2", state: "active" }],
  staleness: "older",
  newerEntries: 4,
};

function renderCard({
  onRestored = vi.fn(),
  onEnterToken = vi.fn(),
  onBotStatus = vi.fn(),
  tokenConfigured = true,
}: {
  onRestored?: () => void;
  onEnterToken?: () => void;
  onBotStatus?: () => void;
  tokenConfigured?: boolean;
} = {}) {
  return render(
    <BackupCard
      timezone="Europe/Stockholm"
      tokenConfigured={tokenConfigured}
      onRestored={onRestored}
      onEnterToken={onEnterToken}
      onBotStatus={onBotStatus}
    />,
  );
}

async function openRestore() {
  await screen.findByText("Swap backups");
  fireEvent.click(
    screen.getByRole("button", { name: "Restore from a backup" }),
  );
  return screen.findByRole("dialog", { name: "Restore from a backup" });
}

beforeEach(() => {
  window.history.replaceState({}, "", "/");
  vi.mocked(loadBackupStatus).mockReset().mockResolvedValue(status());
  vi.mocked(loadRestoreSource).mockReset().mockResolvedValue({
    folderName: "Swap backups",
    backups: status().backups,
  });
  vi.mocked(previewRestore).mockReset().mockResolvedValue(preview);
  vi.mocked(restoreBackup)
    .mockReset()
    .mockResolvedValue({ events: preview.events, keptPrevious: true });
  vi.mocked(backupNow).mockReset().mockResolvedValue(status());
  vi.mocked(chooseBackupFolder).mockReset().mockResolvedValue(status());
  vi.mocked(purgeBackups).mockReset().mockResolvedValue({
    ...status(),
    backups: [],
  });
  vi.mocked(takeOverWriter).mockReset().mockResolvedValue(status());
});

describe("BackupCard", () => {
  // scenario: The host makes a backup on request
  it("writes a backup, announces it, and restores focus", async () => {
    renderCard();
    expect(await screen.findByText("Swap backups")).toBeInTheDocument();
    expect(screen.getByText("2 backups")).toBeInTheDocument();
    expect(screen.getAllByText("Before state change")[0]).toBeInTheDocument();

    const created = status();
    created.backups = [
      {
        id: "backup-manual",
        kind: "manual",
        createdAt: T0 + 60,
        complete: true,
        events: [{ name: "Night Shift Vol. 2", state: "active" }],
      },
      ...created.backups,
    ];
    vi.mocked(backupNow).mockResolvedValue(created);
    fireEvent.click(screen.getByRole("button", { name: "Back up now" }));

    expect(await screen.findByText("3 backups")).toBeInTheDocument();
    const notice = screen.getByText("Backup written.");
    expect(notice.closest('[role="status"]')).not.toBeNull();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Back up now" })).toHaveFocus(),
    );
  });

  it("shows the empty first-run state without a dead danger action", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({ folderName: null, backups: [] }),
    );
    renderCard();

    expect(await screen.findByText("No folder chosen")).toBeInTheDocument();
    expect(
      screen.getByText(/Choose a folder on another disk/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Back up now" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Delete all backups" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Not set up")).toBeInTheDocument();
  });

  it("shows one folder recovery path and keeps the bot lock out", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({
        folderError: {
          code: "MSW-BACKUP-001",
          message:
            "The app cannot read the backup folder. Connect the drive, then choose Retry. If the folder moved, choose Change folder. The app does not change the event state while the backup fails.",
          detail: "The folder is not readable.",
        },
      }),
    );
    renderCard();

    expect(await screen.findByText("Folder error")).toBeInTheDocument();
    expect(screen.getByText(/does not change the event state/i)).toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "Back up now" }),
    ).toBeDisabled();
    expect(screen.queryByText("Bot lock")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delete all backups" }),
    ).not.toBeInTheDocument();
  });

  // scenario: The app shows the last backup failure
  it("shows a last backup failure instead of On", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({
        lastFailure: {
          at: T0 - 1_200,
          code: "MSW-BACKUP-003",
          message:
            "The app could not write the backup. The app did not change the event state. Connect the drive, then choose Back up now.",
          detail: "No space left on the drive.",
        },
      }),
    );
    renderCard();

    expect(await screen.findByText("Attention")).toBeInTheDocument();
    expect(
      screen.getByText(/The last backup failed at/i),
    ).toBeInTheDocument();
    expect(screen.getByText("MSW-BACKUP-003")).toBeInTheDocument();
    expect(screen.queryByText("On")).not.toBeInTheDocument();
  });

  it("walks the restore wizard from choice to done", async () => {
    const onRestored = vi.fn();
    const onEnterToken = vi.fn();
    const onBotStatus = vi.fn();
    renderCard({ onRestored, onEnterToken, onBotStatus });
    const dialog = await openRestore();
    const radios = within(dialog).getAllByRole("radio");
    expect(radios).toHaveLength(2);
    await waitFor(() => expect(radios[0]).toHaveFocus());
    expect(radios[0]).not.toBeChecked();
    expect(radios[0]).not.toHaveAttribute("checked");

    const check = within(dialog).getByRole("button", { name: "Check backup" });
    expect(check).toBeDisabled();
    expect(
      within(dialog).getByText("Choose a backup to check."),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    expect(check).toBeEnabled();
    fireEvent.click(check);

    expect(
      await within(dialog).findByText(
        /The check passed for the event data and 31 files/i,
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/This backup is older than the event data/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/A participant can receive a track that they already had/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/4 newer audit entries/i),
    ).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Back" }));
    await waitFor(() =>
      expect(within(dialog).getByRole("radio", { name: /Daily/ })).toHaveFocus(),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "Check backup" }));

    const input = await within(dialog).findByLabelText("Confirmation");
    await waitFor(() => expect(input).toHaveFocus());
    const restore = within(dialog).getByRole("button", { name: "Restore" });
    expect(restore).toBeDisabled();
    fireEvent.change(input, { target: { value: "restore" } });
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(
      within(dialog).getByText("Type RESTORE in capital letters."),
    ).toBeInTheDocument();
    expect(restore).toBeDisabled();
    fireEvent.change(input, { target: { value: "RESTORE" } });
    expect(restore).toBeEnabled();
    fireEvent.keyDown(input, { key: "Enter" });

    const restored = await within(dialog).findByText(
      /1 event restored from the Daily backup/i,
    );
    expect(restored.closest('[role="status"]')).not.toBeNull();
    expect(
      within(dialog).getByText(/Night Shift Vol\. 2 \(Active\)/),
    ).toBeInTheDocument();
    expect(onBotStatus).toHaveBeenCalledTimes(1);
    expect(onRestored).not.toHaveBeenCalled();
    expect(
      within(dialog).getByRole("button", { name: "Reconnect bot" }),
    ).toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Open recovery folder" }),
    );
    await waitFor(() => expect(openRecoveryFolder).toHaveBeenCalledTimes(1));

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(onRestored).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Backups and restore" }),
      ).toHaveFocus(),
    );
    expect(onEnterToken).not.toHaveBeenCalled();
  });

  it("disables the close control while the restore runs", async () => {
    let finish: (value: { events: typeof preview.events; keptPrevious: boolean }) => void =
      () => undefined;
    vi.mocked(restoreBackup).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    renderCard();
    const dialog = await openRestore();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Check backup" }));
    const input = await within(dialog).findByLabelText("Confirmation");
    fireEvent.change(input, { target: { value: "RESTORE" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    const close = within(dialog).getByRole("button", {
      name: "Close is unavailable while the action runs",
    });
    expect(close).toBeDisabled();
    expect(
      within(dialog).getByText(/The app restores the backup/i),
    ).toBeInTheDocument();
    finish({ events: preview.events, keptPrevious: true });
    const restored = await within(dialog).findByText(
      /1 event restored from the Daily backup/i,
    );
    expect(restored.closest('[role="status"]')).not.toBeNull();
  });

  // scenario: A restore keeps the bot token
  it("offers Add bot token when no token exists", async () => {
    renderCard({ tokenConfigured: false });
    const dialog = await openRestore();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Check backup" }));
    const input = await within(dialog).findByLabelText("Confirmation");
    fireEvent.change(input, { target: { value: "RESTORE" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    expect(
      await within(dialog).findByText(/This computer has no bot token/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "Add bot token" }),
    ).toBeInTheDocument();
  });

  it("keeps the wizard open when the backup is damaged", async () => {
    vi.mocked(previewRestore).mockRejectedValue({
      code: "MSW-BACKUP-002",
      message:
        "The app cannot use this backup. One file is damaged. The app did not change the event data. Choose another backup, then choose Check backup.",
      detail: "The file S7F3A.it is damaged.",
    });
    renderCard();
    const dialog = await openRestore();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Check backup" }));

    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent("The app cannot use this backup");
    expect(alert).toHaveTextContent("MSW-BACKUP-002");
    expect(alert).toHaveTextContent("The file S7F3A.it is damaged.");
    expect(within(dialog).getByRole("radio", { name: /Daily/ })).toBeChecked();
    expect(restoreBackup).not.toHaveBeenCalled();
  });

  it("shows a recoverable command error", async () => {
    vi.mocked(loadBackupStatus)
      .mockRejectedValueOnce({
        code: "MSW-BACKUP-003",
        message:
          "The app could not write the backup. The app did not change the event state. Connect the drive, then choose Back up now.",
        detail: "The app data folder is not writable.",
      })
      .mockResolvedValue(status());
    renderCard();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("MSW-BACKUP-003");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Swap backups")).toBeInTheDocument();
  });

  it("shows another computer and restores focus after takeover", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({
        writer: {
          state: "foreign",
          heartbeatAt: T0 - 42,
          lostLockAt: null,
        },
      }),
    );
    renderCard();
    expect(await screen.findByText("Another computer")).toBeInTheDocument();
    expect(
      screen.getByText(/The lock clears 5 minutes after the last check/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/a91c3f07/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delete all backups" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Take over" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Run the event on this computer?",
    });
    expect(
      within(dialog).getByText(/for every event in this data/i),
    ).toBeInTheDocument();
    const input = within(dialog).getByLabelText("Confirmation");
    const confirm = within(dialog).getByRole("button", { name: "Take over" });
    expect(confirm).toBeDisabled();
    fireEvent.change(input, { target: { value: "TAKE OVER" } });
    fireEvent.click(confirm);

    expect(
      await screen.findByText("This computer now holds the bot lock."),
    ).toBeInTheDocument();
    expect(takeOverWriter).toHaveBeenCalledWith("TAKE OVER");
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Backups and restore" }),
      ).toHaveFocus(),
    );
  });

  it("clears a lost lock when the bot lock is checked again", async () => {
    const locked = status({
      writer: {
        state: "foreign",
        heartbeatAt: T0 - 600,
        lostLockAt: T0 - 540,
      },
    });
    vi.mocked(loadBackupStatus)
      .mockResolvedValueOnce(locked)
      .mockResolvedValue(status());
    renderCard();

    expect(await screen.findByText("Another computer took over")).toBeInTheDocument();
    expect(
      screen.getByText(/Do not take over unless you stopped the bot/i),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Check bot lock" }));
    expect(await screen.findByText("Bot lock checked.")).toBeInTheDocument();
    expect(await screen.findByText("This computer")).toBeInTheDocument();
    expect(
      screen.queryByText("Another computer took over"),
    ).not.toBeInTheDocument();
  });

  // scenario: The restore wizard starts without a backup folder
  it("shows the no-folder restore state with a footer recovery action", async () => {
    vi.mocked(loadRestoreSource).mockResolvedValue({
      folderName: null,
      backups: [],
    });
    vi.mocked(chooseBackupFolder).mockResolvedValue(status());
    renderCard();
    const dialog = await openRestore();
    expect(
      within(dialog).getByText(/No backup folder is selected/i),
    ).toBeInTheDocument();
    const choose = within(dialog).getByRole("button", { name: "Choose folder" });
    expect(choose).toBeEnabled();
    await waitFor(() => expect(choose).toHaveFocus());
  });

  it("shows the empty restore state and purge scope", async () => {
    vi.mocked(loadRestoreSource).mockResolvedValue({
      folderName: "Swap backups",
      backups: [],
    });
    renderCard();
    const dialog = await openRestore();
    expect(
      within(dialog).getByText(/This folder has no complete backup/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/This does not change the backup folder/i),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    const deleteButton = screen.getByRole("button", {
      name: "Delete all backups",
    });
    await waitFor(() => expect(deleteButton).toBeEnabled());
    fireEvent.click(deleteButton);
    const purge = await screen.findByRole("dialog", {
      name: "Delete all backups?",
    });
    expect(
      within(purge).getByText(/for every event, including backups for other events/i),
    ).toBeInTheDocument();
    const input = within(purge).getByLabelText("Confirmation");
    expect(
      within(purge).getByRole("button", { name: "Delete all backups" }),
    ).toBeDisabled();
    fireEvent.change(input, { target: { value: "PURGE" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(purgeBackups).toHaveBeenCalledWith("PURGE"),
    );
    expect(await screen.findByText("All backups deleted.")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "Backups and restore" }),
      ).toHaveFocus(),
    );
  });

  it("keeps the delete dialog open when the command fails", async () => {
    vi.mocked(purgeBackups).mockRejectedValue({
      code: "MSW-BACKUP-003",
      message:
        "The app could not delete every backup. Connect the drive, then choose Delete backups again.",
      detail: "The folder is read-only.",
    });
    renderCard();
    await screen.findByText("Swap backups");
    fireEvent.click(screen.getByRole("button", { name: "Delete all backups" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Delete all backups?",
    });
    const input = within(dialog).getByLabelText("Confirmation");
    fireEvent.change(input, { target: { value: "PURGE" } });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Delete all backups" }),
    );
    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent("MSW-BACKUP-003");
    expect(
      screen.getByRole("dialog", { name: "Delete all backups?" }),
    ).toBeInTheDocument();
  });

  it("announces a matching preview without a warning", async () => {
    vi.mocked(previewRestore).mockResolvedValue({
      ...preview,
      staleness: "same",
    });
    renderCard();
    const dialog = await openRestore();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Check backup" }),
    );
    const status = await within(dialog).findByText(
      /The backup matches the event data/i,
    );
    expect(status.closest('[role="status"]')).not.toBeNull();
  });

  it("makes the app shell inert while a dialog is open", async () => {
    const root = document.createElement("div");
    root.id = "root";
    document.body.appendChild(root);
    render(
      <BackupCard
        timezone="Europe/Stockholm"
        tokenConfigured
        onRestored={vi.fn()}
        onEnterToken={vi.fn()}
        onBotStatus={vi.fn()}
      />,
      { container: root },
    );
    const dialog = await openRestore();
    expect(root).toHaveAttribute("inert");
    expect(root).toHaveAttribute("aria-hidden", "true");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(root).not.toHaveAttribute("inert"));
    root.remove();
  });

  it("keeps the layout busy while the status loads", () => {
    vi.mocked(loadBackupStatus).mockImplementation(
      () => new Promise<BackupStatus>(() => undefined),
    );
    const { container } = renderCard();
    expect(screen.getAllByText("Loading").length).toBeGreaterThan(0);
    expect(container.querySelector(".backup-card")).toHaveAttribute(
      "aria-busy",
      "true",
    );
    expect(screen.getByRole("button", { name: "Back up now" })).toBeDisabled();
  });
});
