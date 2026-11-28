import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { BackupCard } from "./views/Backups";
import {
  backupNow,
  chooseBackupFolder,
  loadBackupStatus,
  loadRestoreSource,
  previewRestore,
  restoreBackup,
  takeOverWriter,
} from "./data/client";
import type { BackupStatus, RestorePreview } from "./types";

vi.mock("./data/client", () => ({
  isDemoMode: () => true,
  backupNow: vi.fn(),
  chooseBackupFolder: vi.fn(),
  chooseRestoreFolder: vi.fn(),
  loadBackupStatus: vi.fn(),
  loadRestoreSource: vi.fn(),
  previewRestore: vi.fn(),
  purgeBackups: vi.fn(),
  restoreBackup: vi.fn(),
  takeOverWriter: vi.fn(),
}));

const T0 = 1_800_000_000;

function status(overrides: Partial<BackupStatus> = {}): BackupStatus {
  return {
    folderName: "Swap backups",
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
      machine: "4f2a9c1e",
      heartbeatAt: T0 - 120,
      lostLock: null,
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

function renderCard(onRestored = vi.fn(), onEnterToken = vi.fn()) {
  return render(
    <BackupCard
      timezone="Europe/Stockholm"
      onRestored={onRestored}
      onEnterToken={onEnterToken}
    />,
  );
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
  vi.mocked(takeOverWriter).mockReset().mockResolvedValue(status());
});

describe("BackupCard", () => {
  it("writes a backup and shows it in the history", async () => {
    renderCard();
    expect(await screen.findByText("Swap backups")).toBeInTheDocument();
    expect(screen.getByText("2 backups")).toBeInTheDocument();
    expect(screen.getByText("Daily")).toBeInTheDocument();

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
    expect(backupNow).toHaveBeenCalledTimes(1);
  });

  it("shows the empty first-run state", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({ folderName: null, backups: [] }),
    );
    renderCard();

    expect(await screen.findByText("No folder selected")).toBeInTheDocument();
    expect(
      screen.getByText(/Select a backup folder/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Back up now" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Delete all backups" }),
    ).toBeDisabled();
    expect(screen.getByText("Not set up")).toBeInTheDocument();
  });

  it("walks the restore wizard from choice to done", async () => {
    const onRestored = vi.fn();
    const onEnterToken = vi.fn();
    renderCard(onRestored, onEnterToken);
    await screen.findByText("Swap backups");

    fireEvent.click(
      screen.getByRole("button", { name: "Restore from a backup" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Restore from a backup",
    });
    const radios = within(dialog).getAllByRole("radio");
    expect(radios).toHaveLength(2);
    const check = within(dialog).getByRole("button", { name: "Check backup" });
    expect(check).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    expect(check).toBeEnabled();
    fireEvent.click(check);

    expect(
      await within(dialog).findByText(/The check passed for the database and 31 files/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/This backup is older than the data/i),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/4 newer audit entries/i),
    ).toBeInTheDocument();

    const input = within(dialog).getByLabelText("Type RESTORE to confirm");
    expect(input).toHaveFocus();
    const restore = within(dialog).getByRole("button", { name: "Restore" });
    expect(restore).toBeDisabled();
    fireEvent.change(input, { target: { value: "RESTOR" } });
    expect(restore).toBeDisabled();
    fireEvent.change(input, { target: { value: "RESTORE" } });
    expect(restore).toBeEnabled();
    fireEvent.click(restore);

    expect(
      await within(dialog).findByText(/The app restored 1 event/i),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/Night Shift Vol. 2 \(Active\)/)).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Add bot token" }));
    expect(onEnterToken).toHaveBeenCalledTimes(1);
    expect(onRestored).toHaveBeenCalledTimes(1);
  });

  it("keeps the wizard open when the backup is damaged", async () => {
    vi.mocked(previewRestore).mockRejectedValue(
      new Error(
        "The backup is damaged. The file S7F3A.it does not match its hash. The app did not change the current data.",
      ),
    );
    renderCard();
    await screen.findByText("Swap backups");
    fireEvent.click(
      screen.getByRole("button", { name: "Restore from a backup" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Restore from a backup",
    });
    fireEvent.click(within(dialog).getByRole("radio", { name: /Daily/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Check backup" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "The backup is damaged.",
    );
    expect(within(dialog).getByRole("radio", { name: /Daily/ })).toBeChecked();
    expect(restoreBackup).not.toHaveBeenCalled();
  });

  it("shows a recoverable load error", async () => {
    vi.mocked(loadBackupStatus)
      .mockRejectedValueOnce(
        new Error("The backup folder is not readable. Select the folder again."),
      )
      .mockResolvedValue(status());
    renderCard();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The backup folder is not readable.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Swap backups")).toBeInTheDocument();
  });

  it("shows another machine and takes over the writer lock", async () => {
    vi.mocked(loadBackupStatus).mockResolvedValue(
      status({
        writer: {
          state: "foreign",
          machine: "a91c3f07",
          heartbeatAt: T0 - 42,
          lostLock: null,
        },
      }),
    );
    renderCard();
    expect(await screen.findByText("Another machine")).toBeInTheDocument();
    expect(screen.getByText(/Machine a91c3f07 sent a heartbeat/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Take over" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Run the event on this machine?",
    });
    const input = within(dialog).getByLabelText("Type TAKE OVER to confirm");
    const confirm = within(dialog).getByRole("button", { name: "Take over" });
    expect(confirm).toBeDisabled();
    fireEvent.change(input, { target: { value: "TAKE OVER" } });
    fireEvent.click(confirm);

    expect(await screen.findByText("This machine")).toBeInTheDocument();
    expect(takeOverWriter).toHaveBeenCalledWith("TAKE OVER");
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
