import type { HostSettings } from "../types";
import { Button, Field, PageHeader, Skeleton, StatusPill, Toggle } from "../components";
import { Icon } from "../icons";

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
  { value: "dark", label: "Dark", detail: "Warm stage light. The default." },
  { value: "light", label: "Light", detail: "Paper and ink for bright rooms." },
  { value: "system", label: "Match system", detail: "Follow the OS setting." },
];

export function SettingsView({
  settings,
  dirty,
  saving,
  onSettings,
  onSave,
  onReset,
  onDelete,
  onTheme,
  onAction,
  onTestNotification,
}: {
  settings: HostSettings;
  dirty: boolean;
  saving: boolean;
  onSettings: (next: HostSettings) => void;
  onSave: () => void;
  onReset: () => void;
  onDelete: () => void;
  onTheme: (theme: HostSettings["theme"]) => void;
  onAction: (label: string) => void;
  onTestNotification: () => void;
}) {
  function update<K extends keyof HostSettings>(
    key: K,
    value: HostSettings[K],
  ) {
    onSettings({ ...settings, [key]: value });
  }

  return (
    <div className="view">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSave();
        }}
      >
        <PageHeader
          eyebrow="Settings"
          title="The quiet parts of the operation"
          description="Connection, channels, schedule defaults, and retention. Secrets stay on this computer and never appear in the window."
          actions={
            settings.botConfigured ? (
              dirty ? (
                <>
                  <Button variant="secondary" onClick={onReset}>
                    Discard changes
                  </Button>
                  <Button
                    type="submit"
                    variant="primary"
                    icon="check"
                    disabled={saving}
                  >
                    {saving ? "Saving…" : "Save changes"}
                  </Button>
                </>
              ) : (
                <span className="saved-state">
                  <Icon name="check" size={16} />
                  Saved
                </span>
              )
            ) : (
              <Button
                variant="primary"
                icon="bot"
                onClick={() => onAction("Add bot token")}
              >
                Add bot token
              </Button>
            )
          }
        />

        <p className="save-state" role="status">
          <Icon name={dirty ? "alert" : "check"} size={16} />
          {!settings.botConfigured
            ? "No bot token is stored. Add one to connect Discord."
            : saving
              ? "Saving settings to this computer."
              : dirty
                ? "Unsaved changes. Select Save changes when you are ready."
                : "Changes save only when you select Save changes."}
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
              <StatusPill tone={settings.botConfigured ? "ok" : "warn"}>
                {settings.botConfigured ? "Configured" : "Not configured"}
              </StatusPill>
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
                    : "No token is stored. Add one to invite the bot and send a test direct message."}
                </span>
              </div>
              {settings.botConfigured ? (
                <Button
                  variant="secondary"
                  onClick={() => onAction("Replace token")}
                >
                  Replace token
                </Button>
              ) : null}
            </div>

            <div className="form-grid">
              <Field
                label="Server"
                htmlFor="guild-name"
                helpId="guild-name-help"
                help="The one server this event belongs to."
              >
                <input
                  id="guild-name"
                  className="input"
                  value={settings.guildName}
                  title={settings.guildName}
                  placeholder="Choose a server"
                  aria-describedby="guild-name-help"
                  onChange={(event) => update("guildName", event.target.value)}
                />
              </Field>
              <div className="field">
                <span className="field-label">When direct messages fail</span>
                <Toggle
                  checked={settings.dmFallback}
                  onChange={(next) => update("dmFallback", next)}
                  label="Private-thread fallback"
                  description="If a direct message fails, the bot uses a private thread you approved. You get an audit entry. Only you and the participant can see it."
                />
              </div>
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
            </div>
            <div className="form-grid">
              <Field
                label="Announcement channel"
                htmlFor="announcement-channel"
                helpId="announcement-channel-help"
                help="Round openings, deadlines, and the commitment."
              >
                <input
                  id="announcement-channel"
                  className="input"
                  value={settings.announcementChannel}
                  title={settings.announcementChannel}
                  placeholder="#swap-announcements"
                  aria-describedby="announcement-channel-help"
                  onChange={(event) =>
                    update("announcementChannel", event.target.value)
                  }
                />
              </Field>
              <Field
                label="Reveal channel"
                htmlFor="reveal-channel"
                helpId="reveal-channel-help"
                help="The album, credits, comments, and keepsakes."
              >
                <input
                  id="reveal-channel"
                  className="input"
                  value={settings.revealChannel}
                  title={settings.revealChannel}
                  placeholder="#swap-reveal"
                  aria-describedby="reveal-channel-help"
                  onChange={(event) =>
                    update("revealChannel", event.target.value)
                  }
                />
              </Field>
            </div>
          </section>

          <section className="card" aria-labelledby="schedule-title">
            <div className="card__header">
              <div>
                <h2 id="schedule-title">Schedule and storage</h2>
                <p className="card__subtitle">
                  Defaults for new events. Each event can change its own
                  schedule before the commit.
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
                help="Days after the event. Backups follow the same window."
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
                  The dark theme is the default. Both themes meet WCAG AA
                  contrast.
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
                <span className="field-label">Delete local event data</span>
                <span className="field-help">
                  Removes the database, file library, and local backups for this
                  event. Exported albums are not touched.
                </span>
              </div>
              <Button variant="danger" icon="trash" onClick={onDelete}>
                Delete data
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
