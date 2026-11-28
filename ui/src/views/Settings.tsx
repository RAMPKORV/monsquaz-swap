import type { HostSettings } from "../types";
import { Button, Field, PageHeader, Skeleton, StatusPill, Toggle } from "../components";
import { Icon } from "../icons";
import { BackupCard } from "./Backups";

const timezones = [
  "Europe/Stockholm",
  "Europe/London",
  "Europe/Berlin",
  "America/New_York",
  "America/Chicago",
  "America/Los_Angeles",
  "America/Argentina/Buenos_Aires",
  "Asia/Tokyo",
  "Australia/Sydney",
  "UTC",
];

const themes: Array<{
  value: HostSettings["theme"];
  label: string;
  detail: string;
}> = [
  { value: "dark", label: "Dark", detail: "Warm stage light." },
  { value: "light", label: "Light", detail: "Paper and ink for bright rooms." },
  { value: "system", label: "Match system", detail: "Follow the OS setting." },
];

export function SettingsView({
  settings,
  eventName,
  dirty,
  onSettings,
  onSave,
  onDelete,
  onTheme,
  onConfigureBot,
  onDiscard,
  onTestNotification,
  onRestored,
  onBotStatus,
}: {
  settings: HostSettings;
  eventName: string;
  dirty: boolean;
  onSettings: (next: HostSettings) => void;
  onSave: () => void;
  onDelete: () => void;
  onTheme: (theme: HostSettings["theme"]) => void;
  onConfigureBot: () => void;
  onDiscard: () => void;
  onTestNotification: () => void;
  onRestored: () => void;
  onBotStatus: () => void;
}) {
  function update<K extends keyof HostSettings>(
    key: K,
    value: HostSettings[K],
  ) {
    onSettings({ ...settings, [key]: value });
  }

  const connectionTone = !settings.botConfigured
    ? ("warn" as const)
    : settings.gateway?.state === "online"
      ? ("ok" as const)
      : settings.gateway?.state === "reconnecting" ||
          settings.gateway?.state === "starting"
        ? ("warn" as const)
        : settings.gateway
          ? ("danger" as const)
          : ("muted" as const);
  const connectionLabel = !settings.botConfigured
    ? "Not configured"
    : settings.gateway?.state === "online"
      ? "Connected"
      : settings.gateway?.state === "starting"
        ? "Connecting"
        : settings.gateway?.state === "reconnecting"
          ? "Reconnecting"
          : settings.gateway
            ? "Offline"
            : "Token stored";

  return (
    <div className="view">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSave();
        }}
      >
        <PageHeader
          title="Settings"
          description="Connection, channels, schedule defaults, retention, and backups. The app never shows the bot token again after you save it."
          actions={
            dirty ? (
              <div className="settings-header-actions">
                <Button variant="secondary" onClick={onDiscard}>
                  Discard changes
                </Button>
                <Button variant="primary" type="submit" icon="check">
                  Save changes
                </Button>
              </div>
            ) : (
              <Button
                variant={settings.botConfigured ? "secondary" : "primary"}
                icon="bot"
                onClick={onConfigureBot}
              >
                {settings.botConfigured ? "Reconnect bot" : "Add bot token"}
              </Button>
            )
          }
        />

        <p
          className={`save-state ${
            dirty || !settings.botConfigured ? "save-state--warn" : ""
          }`}
          role="status"
        >
          <Icon
            name={dirty || !settings.botConfigured ? "alert" : "check"}
            size={16}
          />
          {dirty
            ? "Unsaved changes. Save before you leave this page."
            : !settings.botConfigured
              ? "No bot token is stored."
              : settings.tokenStorage === "session"
                ? "Settings are saved. The bot token lasts for this session only."
                : "All changes are saved on this computer."}
        </p>

        <div className="settings-grid">
          <section className="card" aria-labelledby="connection-title">
            <div className="card__header">
              <div>
                <h2 id="connection-title">Discord connection</h2>
                <p className="card__subtitle">
                  The bot runs with your own token. The app never shows it
                  again after you save it.
                </p>
              </div>
              <StatusPill tone={connectionTone}>{connectionLabel}</StatusPill>
            </div>

            <div className="connection-row">
              <span className="connection-row__icon">
                <Icon name="bot" size={24} />
              </span>
              <div className="connection-row__copy">
                <span className="field-label">Bot identity</span>
                <span className="field-help">
                  {settings.botConfigured
                    ? settings.botIdentity
                    : "Not connected."}
                </span>
              </div>
            </div>

            <div className="connection-row">
              <span className="connection-row__icon">
                <Icon name="bot" size={24} />
              </span>
              <span className="visually-hidden" role="status" aria-live="polite" aria-atomic="true">
                Gateway {connectionLabel}.
              </span>
              <div className="connection-row__copy">
                <span className="field-label">Gateway</span>
                <span className="field-help">
                  {settings.gateway
                    ? settings.gateway.state === "online"
                      ? `Connected to Discord.${settings.gateway.attempt > 0 ? ` Connected after ${settings.gateway.attempt} ${settings.gateway.attempt === 1 ? "retry" : "retries"}.` : ""}`
                      : settings.gateway.state === "starting"
                        ? "Starting the gateway."
                        : settings.gateway.state === "reconnecting"
                          ? `Attempt ${settings.gateway.attempt}. Offers and replies wait in the local queue until the gateway reconnects.${settings.gateway.lastError ? ` ${settings.gateway.lastError}` : ""}`
                          : settings.gateway.lastError ?? "The gateway is not running."
                    : "The gateway is not running."}
                </span>
              </div>
              <StatusPill
                tone={
                  !settings.gateway
                    ? "muted"
                    : settings.gateway.state === "online"
                      ? "ok"
                      : settings.gateway.state === "failed" ||
                          settings.gateway.state === "stopped"
                        ? "danger"
                        : "warn"
                }
              >
                {!settings.gateway
                  ? "Not started"
                  : settings.gateway.state === "online"
                    ? "Online"
                    : settings.gateway.state === "reconnecting" ||
                        settings.gateway.state === "starting"
                      ? "Reconnecting"
                      : "Offline"}
              </StatusPill>
            </div>

            <div className="bot-storage">
              <div>
                <span className="field-label">Token storage</span>
                <span className="field-help">
                  {settings.tokenStorage === "keychain"
                    ? "Saved in the operating system keychain. The app never shows it again."
                    : settings.tokenStorage === "session"
                      ? "The token is stored only until you quit the app. The bot stops then. Add the token again after a restart."
                      : "Not stored."}
                </span>
              </div>
              {settings.keychainError ? (
                <span className="field-error" role="alert">
                  The OS keychain is locked. Unlock it before you send messages
                  or remove the connection.
                </span>
              ) : null}
              <div>
                <span className="field-label">Server</span>
                <span className="field-help">
                  {settings.guildName
                    ? `${settings.guildName} (${settings.guildId})`
                    : "No server selected yet."}
                </span>
              </div>
            </div>

            <div className="connection-fallback">
              <Toggle
                checked={settings.dmFallback}
                onChange={(next) => update("dmFallback", next)}
                label="Private-thread fallback"
                description="If a direct message fails, the bot opens a private thread with the participant and writes an audit entry. You and server moderators can open the thread. The bot never sends a track through a public channel."
              />
            </div>
          </section>

          <section className="card" aria-labelledby="channels-title">
            <div className="card__header">
              <div>
                <h2 id="channels-title">Channels</h2>
                <p className="card__subtitle">
                  Public posts show progress and the reveal. Files never travel
                  through a public channel.
                </p>
              </div>
              {settings.botConfigured ? (
                <Button variant="secondary" onClick={onConfigureBot}>
                  Map channels
                </Button>
              ) : null}
            </div>
            <div className="channel-map">
              <div className="channel-map__row">
                <div>
                  <span className="field-label">Announcement channel</span>
                  <span className="field-help">
                    Round openings, deadlines, and a record that the schedule
                    did not change.
                  </span>
                </div>
                <span className="channel-map__value">
                  {settings.announcementChannelName
                    ? `#${settings.announcementChannelName}`
                    : "Not selected"}
                  {settings.announcementChannelId ? (
                    <small>{settings.announcementChannelId}</small>
                  ) : null}
                </span>
              </div>
              <div className="channel-map__row">
                <div>
                  <span className="field-label">Reveal channel</span>
                  <span className="field-help">
                    The album, credits, comments, and keepsakes.
                  </span>
                </div>
                <span className="channel-map__value">
                  {settings.revealChannelName
                    ? `#${settings.revealChannelName}`
                    : "Not selected"}
                  {settings.revealChannelId ? (
                    <small>{settings.revealChannelId}</small>
                  ) : null}
                </span>
              </div>
            </div>
          </section>

          <section className="card" aria-labelledby="schedule-title">
            <div className="card__header">
              <div>
                <h2 id="schedule-title">Schedule and storage</h2>
                <p className="card__subtitle">
                  Defaults for new events. Each event can change its own
                  schedule before the event starts.
                </p>
              </div>
            </div>
            <div className="form-grid form-grid--schedule">
              <Field
                label="Timezone"
                htmlFor="timezone"
                helpId="timezone-help"
                help="Dates show in this zone. The database always stores UTC."
              >
                <select
                  id="timezone"
                  className="input"
                  value={settings.timezone}
                  title={settings.timezone}
                  aria-describedby="timezone-help"
                  onChange={(event) => update("timezone", event.target.value)}
                >
                  {timezones.map((zone) => (
                    <option value={zone} key={zone}>
                      {zone}
                    </option>
                  ))}
                </select>
              </Field>
              <Field
                label="Keep files for"
                htmlFor="retention"
                helpId="retention-help"
                help="Days after the reveal. This setting does not change backup retention."
              >
                <div className="number-control">
                  <input
                    id="retention"
                    className="input"
                    type="number"
                    min={1}
                    max={3650}
                    value={settings.retentionDays}
                    aria-describedby="retention-help"
                    onChange={(event) =>
                      update(
                        "retentionDays",
                        Number(event.target.value) || 1,
                      )
                    }
                  />
                  <span aria-hidden="true">days</span>
                </div>
              </Field>
            </div>
          </section>

          <section className="card" aria-labelledby="notifications-title">
            <div className="card__header">
              <div>
                <h2 id="notifications-title">Notifications</h2>
                <p className="card__subtitle">
                  Desktop notices while the app runs in the tray. Discord
                  messages are separate.
                </p>
              </div>
            </div>
            <div className="toggle-stack">
              <Toggle
                checked={settings.notifications.deadlines}
                onChange={(next) =>
                  update("notifications", {
                    ...settings.notifications,
                    deadlines: next,
                  })
                }
                label="Deadline decisions"
                description="Tell me when a track needs a host decision."
              />
              <Toggle
                checked={settings.notifications.botOffline}
                onChange={(next) =>
                  update("notifications", {
                    ...settings.notifications,
                    botOffline: next,
                  })
                }
                label="Bot offline"
                description="Tell me when the Discord connection drops or recovers."
              />
              <Toggle
                checked={settings.notifications.releases}
                onChange={(next) =>
                  update("notifications", {
                    ...settings.notifications,
                    releases: next,
                  })
                }
                label="Round releases"
                description="Tell me when the next offer batch opens."
              />
            </div>
            <Button
              variant="secondary"
              icon="mail"
              onClick={onTestNotification}
            >
              Send test notification
            </Button>
          </section>

          <section className="card" aria-labelledby="appearance-title">
            <div className="card__header">
              <div>
                <h2 id="appearance-title">Appearance</h2>
                <p className="card__subtitle">
                  Choose how the app looks.
                </p>
              </div>
            </div>
            <div
              className="theme-options"
              role="radiogroup"
              aria-labelledby="appearance-title"
            >
              {themes.map((theme) => (
                <label
                  key={theme.value}
                  className={`theme-option ${
                    settings.theme === theme.value ? "theme-option--active" : ""
                  }`}
                >
                  <input
                    className="visually-hidden"
                    type="radio"
                    name="theme"
                    value={theme.value}
                    checked={settings.theme === theme.value}
                    onChange={() => {
                      update("theme", theme.value);
                      onTheme(theme.value);
                    }}
                  />
                  <Icon
                    name={
                      theme.value === "light"
                        ? "sun"
                        : theme.value === "system"
                          ? "monitor"
                          : "moon"
                    }
                    size={20}
                  />
                  <span>
                    <strong>{theme.label}</strong>
                    <small>{theme.detail}</small>
                  </span>
                  {settings.theme === theme.value ? (
                    <Icon
                      name="check"
                      size={16}
                      className="theme-option__check"
                    />
                  ) : null}
                </label>
              ))}
            </div>
          </section>

          <BackupCard
            timezone={settings.timezone}
            tokenConfigured={settings.botConfigured}
            onEnterToken={onConfigureBot}
            onRestored={onRestored}
            onBotStatus={onBotStatus}
          />

          <section
            className="card danger-zone"
            aria-labelledby="danger-title"
          >
            <div className="card__header">
              <div>
                <h2 id="danger-title">Danger zone</h2>
                <p className="card__subtitle">
                  This removes local data. Discord messages stay where they are.
                </p>
              </div>
            </div>
            <div className="danger-zone__row">
              <div>
                <span className="field-label">
                  Delete {eventName} from this computer
                </span>
                <span className="field-help">
                  Removes the local event data and every stored track version
                  for {eventName}. Other events are not touched. The app does
                  not delete backups. To delete every backup, choose Delete all
                  backups. Discord messages and exported albums stay.
                </span>
              </div>
              <Button variant="danger" icon="trash" onClick={onDelete}>
                Delete event data
              </Button>
            </div>
          </section>
        </div>

      </form>

    </div>
  );
}

export function SettingsSkeleton() {
  return (
    <div
      className="view"
      role="status"
      aria-busy="true"
      aria-label="Loading settings"
    >
      <span className="visually-hidden">Loading settings</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="90px" height={12} />
          <Skeleton width="420px" height={38} className="skeleton--title" />
          <Skeleton width="min(100%, 560px)" height={16} />
        </div>
      </div>
      <div className="settings-grid">
        {Array.from({ length: 4 }, (_, index) => (
          <section className="card" key={index}>
            <Skeleton width="40%" height={22} />
            <div className="skeleton-stack">
              <Skeleton width="92%" height={14} />
              <Skeleton width="70%" height={14} />
              <Skeleton width="100%" height={44} />
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
