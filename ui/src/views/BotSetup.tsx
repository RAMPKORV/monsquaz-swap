import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  BotPreview,
  ChannelOption,
  GuildOption,
  HostSettings,
} from "../types";
import {
  clearBotSetup,
  isDemoMode,
  loadBotChannels,
  loadBotGuilds,
  saveBotSetup,
  sendBotTestDm,
  validateBotToken,
} from "../data/client";
import { demoBotPreview, demoChannels, demoGuilds } from "../data/demo";
import { Button, Dialog, Field } from "../components";
import { Icon } from "../icons";

type Busy = "validate" | "channels" | "save" | "clear" | "test" | null;
type Where = "token" | "mapping" | "save" | "test" | "remove";

type BotError = {
  where: Where;
  code: string;
  message: string;
  focusId?: string;
};

type Notice = {
  tone: "ok" | "warn";
  text: string;
};

const PORTAL_URL = "https://discord.com/developers/applications";

export function BotSetupDialog({
  settings,
  onClose,
  onSaved,
}: {
  settings: HostSettings;
  onClose: () => void;
  onSaved: (settings: HostSettings) => void;
}) {
  const demo = isDemoMode();
  const demoErrorKey = demo
    ? new URLSearchParams(window.location.search).get("botError")
    : null;
  const demoError: BotError | null =
    demoErrorKey === "token"
      ? {
          where: "token",
          code: "MSW-BOT-001",
          message:
            "Discord rejected this token. Copy it again from the Bot tab in the Discord Developer Portal. Do not use the client secret.",
          focusId: "bot-token",
        }
      : demoErrorKey === "test"
        ? {
            where: "test",
            code: "MSW-BOT-007",
            message:
              "Enter the Discord user id. Open the person menu in Discord and select Copy User ID. Use only the number.",
            focusId: "bot-test-user",
          }
        : demoErrorKey === "confirm"
          ? null
          : demoErrorKey === "keychain"
            ? {
              where: "save",
              code: "MSW-BOT-002",
              message:
                "This computer does not provide a keychain. Select Use this session only to continue.",
            }
          : null;
  const demoToken =
    "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
  const [token, setToken] = useState(
    demoErrorKey === "keychain" || demoErrorKey === "token" ? demoToken : "",
  );
  const [validatedToken, setValidatedToken] = useState("");
  const [preview, setPreview] = useState<BotPreview | null>(() => {
    if (demoErrorKey === "token") {
      return null;
    }
    if (demo && (settings.botConfigured || demoErrorKey === "keychain")) {
      return demoBotPreview;
    }
    if (settings.botConfigured && settings.botIdentity) {
      return {
        botId: "",
        identity: settings.botIdentity,
        inviteUrl: "",
      };
    }
    return null;
  });
  const [guilds, setGuilds] = useState<GuildOption[]>(
    demo &&
    (settings.botConfigured || demoErrorKey === "keychain") &&
    demoErrorKey !== "token"
      ? demoGuilds
      : [],
  );
  const [channels, setChannels] = useState<ChannelOption[]>(
    demo &&
    (settings.botConfigured || demoErrorKey === "keychain") &&
    demoErrorKey !== "token"
      ? demoChannels
      : [],
  );
  const [guildId, setGuildId] = useState(settings.guildId);
  const [announcementId, setAnnouncementId] = useState(
    settings.announcementChannelId,
  );
  const [revealId, setRevealId] = useState(settings.revealChannelId);
  const [userId, setUserId] = useState(
    demoErrorKey === "test" ? "not-a-user" : "",
  );
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<BotError | null>(demoError);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [sessionOnly, setSessionOnly] = useState(false);
  const [keychainBlocked, setKeychainBlocked] = useState(
    demoErrorKey === "keychain",
  );
  const [inviteFailed, setInviteFailed] = useState(false);
  const [saved, setSaved] = useState(
    settings.botConfigured &&
      demoErrorKey !== "keychain" &&
      demoErrorKey !== "token",
  );
  const [justSaved, setJustSaved] = useState(false);
  const [confirmClear, setConfirmClear] = useState(
    demoErrorKey === "confirm",
  );
  const testInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (justSaved) {
      testInputRef.current?.focus();
      setJustSaved(false);
    }
  }, [justSaved]);

  useEffect(() => {
    if (!error) {
      return;
    }
    const target =
      error.focusId ??
      (error.where === "save" && keychainBlocked
        ? "bot-fallback"
        : error.where === "token"
          ? "bot-token"
          : error.where === "test"
            ? "bot-test-user"
            : "bot-setup-intro");
    document.getElementById(target)?.focus({ preventScroll: false });
  }, [error, keychainBlocked]);

  const editing = !saved || token.trim().length > 0;
  const canValidate = token.trim().length > 0 && busy === null;
  const canSave =
    error?.where !== "token" &&
    token.trim().length > 0 &&
    preview !== null &&
    guilds.length > 0 &&
    channels.length >= 2 &&
    guildId.length > 0 &&
    announcementId.length > 0 &&
    revealId.length > 0 &&
    announcementId !== revealId &&
    busy === null;
  const showSave = !saved || token.trim().length > 0;

  const busyLabel =
    busy === "validate"
      ? "Checking the bot token."
      : busy === "channels"
        ? "Loading the server channels."
        : busy === "save"
          ? "Saving the connection."
          : busy === "clear"
            ? "Removing the connection."
            : busy === "test"
              ? "Sending the test message."
              : "";

  const saveHelp =
    token.trim().length === 0
      ? "Paste and validate the bot token to save the connection."
      : preview === null
        ? "Validate the token before you save."
        : !guildId
          ? "Choose a server that the bot can join."
          : !announcementId || !revealId
            ? "Choose two text channels that the bot can see."
            : announcementId === revealId
              ? "Choose two different channels for announcements and the reveal."
              : "";

  function keepFocusInDialog() {
    document.getElementById("bot-setup-intro")?.focus();
  }

  const guildName =
    guilds.find((guild) => guild.id === guildId)?.name ?? settings.guildName;
  const announcementName =
    channels.find((channel) => channel.id === announcementId)?.name ??
    settings.announcementChannelName;
  const revealName =
    channels.find((channel) => channel.id === revealId)?.name ??
    settings.revealChannelName;

  async function fetchChannels(nextGuildId: string) {
    const list = await loadBotChannels(token, nextGuildId);
    setChannels(list);
    const announcement =
      list.find((channel) => channel.id === settings.announcementChannelId) ??
      list[0];
    const reveal =
      list.find((channel) => channel.id === settings.revealChannelId) ??
      list.find((channel) => channel.id !== announcement?.id);
    setAnnouncementId(announcement?.id ?? "");
    setRevealId(reveal?.id ?? "");
    return list;
  }

  async function handleValidate() {
    keepFocusInDialog();
    setBusy("validate");
    setError(null);
    setNotice(null);
    let where: Where = "token";
    try {
      const nextPreview = await validateBotToken(token);
      setValidatedToken(token);
      setPreview(nextPreview);
      where = "mapping";
      const nextGuilds = await loadBotGuilds(token);
      setGuilds(nextGuilds);
      if (nextGuilds.length === 0) {
        setGuildId("");
        setChannels([]);
        setAnnouncementId("");
        setRevealId("");
        setNotice({
          tone: "warn",
          text: "The bot is in no server yet. Select Open invite, add the bot, then select Refresh servers.",
        });
        return;
      }
      const nextGuild =
        nextGuilds.find((guild) => guild.id === settings.guildId) ??
        nextGuilds[0];
      setGuildId(nextGuild.id);
      const nextChannels = await fetchChannels(nextGuild.id);
      setNotice(
        nextChannels.length < 2
          ? channelWarning(nextChannels.length)
          : {
              tone: "ok",
              text: "Token accepted. Choose the server and two channels.",
            },
      );
    } catch (nextError) {
      setError(describeError(nextError, where));
    } finally {
      setBusy(null);
    }
  }

  async function selectGuild(nextGuildId: string) {
    setGuildId(nextGuildId);
    setBusy("channels");
    setError(null);
    setNotice(null);
    try {
      const list = await fetchChannels(nextGuildId);
      if (list.length < 2) {
        setNotice(channelWarning(list.length));
      }
    } catch (nextError) {
      setChannels([]);
      setAnnouncementId("");
      setRevealId("");
      setError(describeError(nextError, "mapping"));
    } finally {
      setBusy(null);
    }
  }

  async function handleRefresh() {
    keepFocusInDialog();
    if (!token.trim()) {
      setError({
        where: "token",
        code: "MSW-BOT-001",
        message:
          "Paste the bot token again to refresh the server list. The app does not keep it in the window.",
      });
      return;
    }
    setBusy("channels");
    setError(null);
    setNotice(null);
    try {
      const nextGuilds = await loadBotGuilds(token);
      setGuilds(nextGuilds);
      if (nextGuilds.length === 0) {
        setGuildId("");
        setChannels([]);
        setAnnouncementId("");
        setRevealId("");
        setNotice({
          tone: "warn",
          text: "The bot is in no server yet. Select Open invite, add the bot, then select Refresh servers.",
        });
        return;
      }
      const nextGuild =
        nextGuilds.find((guild) => guild.id === guildId) ?? nextGuilds[0];
      setGuildId(nextGuild.id);
      const list = await fetchChannels(nextGuild.id);
      setNotice(
        list.length < 2
          ? channelWarning(list.length)
          : { tone: "ok", text: "Server list refreshed." },
      );
    } catch (nextError) {
      setError(describeError(nextError, "mapping"));
    } finally {
      setBusy(null);
    }
  }

  async function handleSave() {
    keepFocusInDialog();
    setBusy("save");
    setError(null);
    setNotice(null);
    try {
      const nextSettings = await saveBotSetup({
        token,
        guildId,
        announcementChannelId: announcementId,
        revealChannelId: revealId,
        allowSessionOnly: sessionOnly,
      });
      setSaved(true);
      setJustSaved(true);
      setToken("");
      setValidatedToken("");
      setNotice({
        tone: "ok",
        text:
          nextSettings.tokenStorage === "session"
            ? "Connection saved for this session only. The bot stops when you quit the app. Send a test direct message to finish."
            : "Connection saved. Send a test direct message to finish.",
      });
      onSaved(nextSettings);
    } catch (nextError) {
      const nextErrorInfo = describeError(nextError, "save");
      const message = nextErrorInfo.message.toLowerCase();
      if (message.includes("keychain")) {
        setKeychainBlocked(true);
      }
      if (
        message.includes("channel") ||
        message.includes("server") ||
        message.includes("invite")
      ) {
        nextErrorInfo.where = "mapping";
      }
      setError(nextErrorInfo);
    } finally {
      setBusy(null);
    }
  }

  async function handleTestDm() {
    keepFocusInDialog();
    if (!/^\d{5,25}$/.test(userId.trim())) {
      setError({
        where: "test",
        code: "MSW-BOT-007",
        message:
          "Enter the Discord user id. Open the person menu in Discord and select Copy User ID. Use only the number.",
      });
      return;
    }
    setBusy("test");
    setError(null);
    setNotice(null);
    try {
      const messageId = await sendBotTestDm(userId);
      setNotice(
        demo
          ? {
              tone: "ok",
              text: "Test preview: this build does not send a message.",
            }
          : {
              tone: "ok",
              text: `Test message sent. Discord message ${messageId}.`,
            },
      );
    } catch (nextError) {
      setError(describeError(nextError, "test"));
    } finally {
      setBusy(null);
    }
  }

  function requestRemove() {
    keepFocusInDialog();
    setConfirmClear(true);
    window.requestAnimationFrame(() => {
      document.getElementById("bot-confirm-remove")?.focus();
    });
  }

  function cancelRemove() {
    setConfirmClear(false);
    window.requestAnimationFrame(() => {
      document.getElementById("bot-remove")?.focus();
    });
  }

  async function handleClear() {
    keepFocusInDialog();
    setBusy("clear");
    setError(null);
    setNotice(null);
    try {
      const nextSettings = await clearBotSetup();
      resetToCleared(nextSettings);
      setNotice({
        tone: "ok",
        text: "Connection removed. The bot stops until you connect it again. Discord messages stay on Discord.",
      });
    } catch (nextError) {
      setError(describeError(nextError, "remove"));
    } finally {
      setBusy(null);
    }
  }

  function resetToCleared(nextSettings: HostSettings) {
    setSaved(false);
    setPreview(null);
    setGuilds([]);
    setChannels([]);
    setGuildId("");
    setAnnouncementId("");
    setRevealId("");
    setToken("");
    setUserId("");
    setConfirmClear(false);
    setKeychainBlocked(false);
    setSessionOnly(false);
    setError(null);
    onSaved(nextSettings);
    window.requestAnimationFrame(() => {
      document.getElementById("bot-token")?.focus();
    });
  }

  async function handleInvite() {
    if (!preview?.inviteUrl) {
      return;
    }
    try {
      await openUrl(preview.inviteUrl);
      setInviteFailed(false);
      setError(null);
    } catch {
      setInviteFailed(true);
      setError({
        where: "mapping",
        code: "MSW-BOT-010",
        message:
          "The browser did not open. Copy the invite link into a browser.",
        focusId: "bot-invite-url",
      });
    }
  }

  async function handlePortal() {
    try {
      await openUrl(PORTAL_URL);
    } catch {
      setError({
        where: "token",
        code: "MSW-BOT-010",
        message:
          "The browser did not open. Open discord.com/developers/applications in a browser.",
      });
    }
  }

  return (
    <Dialog
      title="Connect the Discord bot"
      labelledBy="bot-setup-title"
      initialFocusId={
        error?.where === "token"
          ? "bot-token"
          : saved
            ? "bot-test-user"
            : "bot-token"
      }
      onClose={onClose}
      footer={
        showSave ? (
          <>
            <Button variant="secondary" onClick={onClose}>
              {saved ? "Done" : "Cancel"}
            </Button>
            <Button
              variant="primary"
              icon="check"
              onClick={handleSave}
              disabled={!canSave}
              describedBy={saveHelp ? "bot-save-help" : undefined}
            >
              {busy === "save"
                ? "Saving…"
                : sessionOnly
                  ? "Save for this session"
                  : "Save connection"}
            </Button>
          </>
        ) : (
          <Button variant="secondary" onClick={onClose}>
            Done
          </Button>
        )
      }
    >
      <p
        className="dialog__copy"
        id="bot-setup-intro"
        tabIndex={-1}
      >
        Create a bot in the Discord Developer Portal, then paste its token here.
        The app never shows the token again after you save it.
      </p>
      <p className="visually-hidden" role="status">
        {busyLabel}
      </p>
      <div className="bot-links">
        <Button variant="quiet" icon="arrow-right" onClick={handlePortal}>
          Open the Discord Developer Portal
        </Button>
        <details className="bot-guide">
          <summary>How to create a bot token</summary>
          <ol>
            <li>Select New Application and give it a name.</li>
            <li>Open the Bot tab.</li>
            <li>Select Reset Token, then Copy.</li>
            <li>Paste the token here. Do not use the client secret.</li>
          </ol>
        </details>
      </div>

      {error?.where === "save" ? <ErrorNote error={error} /> : null}

      <Field
        label="Bot token"
        htmlFor="bot-token"
        helpId="bot-token-help"
        help="The token stays on this computer. It never enters the event database or the audit log."
      >
        <div className="bot-token-row">
          <input
            id="bot-token"
            className="input"
            type="password"
            value={token}
            placeholder="Paste the bot token"
            autoComplete="off"
            spellCheck={false}
            aria-describedby={
              error?.where === "token"
                ? "bot-token-help bot-error-token"
                : "bot-token-help"
            }
            aria-invalid={error?.where === "token" || undefined}
            onChange={(event) => {
              const value = event.target.value;
              setToken(value);
              if (value !== validatedToken) {
                setPreview(null);
                setGuilds([]);
                setChannels([]);
                setGuildId("");
                setAnnouncementId("");
                setRevealId("");
                setNotice(null);
                setError(null);
              }
            }}
          />
          <Button
            variant="secondary"
            onClick={handleValidate}
            disabled={!canValidate}
          >
            {busy === "validate" ? "Checking…" : "Validate token"}
          </Button>
        </div>
      </Field>
      {error?.where === "token" ? <ErrorNote error={error} /> : null}

      {preview ? (
        <div className="bot-identity">
          <span className="connection-row__icon">
            <Icon name="bot" size={22} />
          </span>
          <div className="connection-row__copy" role="status">
            <span className="field-label">Bot identity</span>
            <span className="field-help">{preview.identity}</span>
          </div>
          {preview.inviteUrl ? (
            <div className="bot-identity__actions">
              <Button variant="secondary" icon="arrow-right" onClick={handleInvite}>
                Open invite
              </Button>
              <Button
                variant="secondary"
                onClick={handleRefresh}
                disabled={busy !== null || !token.trim()}
              >
                Refresh servers
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}

      {inviteFailed && preview?.inviteUrl ? (
        <input
          id="bot-invite-url"
          className="input bot-invite-url"
          readOnly
          value={preview.inviteUrl}
          aria-label="Bot invite link"
          onFocus={(event) => event.currentTarget.select()}
        />
      ) : null}

      {editing && guilds.length > 0 ? (
        <div className="bot-mapping">
          <Field
            label="Server"
            htmlFor="bot-guild"
            helpId="bot-guild-help"
            help="The bot must be a member of this server."
          >
            <select
              id="bot-guild"
              className="input"
              value={guildId}
              aria-describedby={
                error?.where === "mapping"
                  ? "bot-guild-help bot-error-mapping"
                  : "bot-guild-help"
              }
              aria-invalid={error?.focusId === "bot-guild" || undefined}
              onChange={(event) => void selectGuild(event.target.value)}
            >
              {guilds.map((guild) => (
                <option key={guild.id} value={guild.id}>
                  {guild.name}
                </option>
              ))}
            </select>
          </Field>

          <Field
            label="Announcement channel"
            htmlFor="bot-announcement"
            helpId="bot-announcement-help"
            help="Round openings, deadlines, and a record that the schedule did not change."
          >
            <select
              id="bot-announcement"
              className="input"
              value={announcementId}
              aria-describedby={
                announcementId === revealId
                  ? "bot-announcement-help bot-channel-error"
                  : error?.focusId === "bot-announcement"
                    ? "bot-announcement-help bot-error-mapping"
                    : "bot-announcement-help"
              }
              aria-invalid={
                (error?.focusId === "bot-announcement" ||
                  (announcementId.length > 0 &&
                    announcementId === revealId)) ||
                undefined
              }
              disabled={busy === "channels" || channels.length < 2}
              onChange={(event) => setAnnouncementId(event.target.value)}
            >
              {channels.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  #{channel.name}
                </option>
              ))}
            </select>
          </Field>

          <Field
            label="Reveal channel"
            htmlFor="bot-reveal"
            helpId="bot-reveal-help"
            help="The album, credits, comments, and keepsakes."
          >
            <select
              id="bot-reveal"
              className="input"
              value={revealId}
              aria-describedby={
                announcementId === revealId
                  ? "bot-reveal-help bot-channel-error"
                  : error?.focusId === "bot-announcement"
                    ? "bot-reveal-help bot-error-mapping"
                    : "bot-reveal-help"
              }
              aria-invalid={
                (error?.focusId === "bot-announcement" ||
                  (revealId.length > 0 && announcementId === revealId)) ||
                undefined
              }
              disabled={busy === "channels" || channels.length < 2}
              onChange={(event) => setRevealId(event.target.value)}
            >
              {channels.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  #{channel.name}
                </option>
              ))}
            </select>
          </Field>

          {announcementId && announcementId === revealId ? (
            <p className="field-error" id="bot-channel-error" role="alert">
              Choose two different channels for announcements and the reveal.
            </p>
          ) : null}
        </div>
      ) : null}

      {saved && !editing ? (
        <div className="bot-saved-map">
          <div>
            <span className="field-label">Server</span>
            <span className="field-help">{guildName || "Not selected"}</span>
          </div>
          <div>
            <span className="field-label">Announcement channel</span>
            <span className="field-help">
              {announcementName ? `#${announcementName}` : "Not selected"}
            </span>
          </div>
          <div>
            <span className="field-label">Reveal channel</span>
            <span className="field-help">
              {revealName ? `#${revealName}` : "Not selected"}
            </span>
          </div>
          <p className="field-help">
            To change the server or channels, paste the bot token again. The app
            does not keep it in the window.
          </p>
        </div>
      ) : null}

      {error?.where === "mapping" ? <ErrorNote error={error} /> : null}

      {keychainBlocked && (!saved || token.trim().length > 0) ? (
        <label className="bot-fallback">
          <input
            id="bot-fallback"
            type="checkbox"
            checked={sessionOnly}
            onChange={(event) => setSessionOnly(event.target.checked)}
          />
          <span>
            <strong>Use this session only</strong>
            <small>
              The token is stored only until you quit the app. The bot stops
              then. Add the token again after a restart.
            </small>
          </span>
        </label>
      ) : null}

      {saved && !editing ? (
        <div className="bot-test">
          <Field
            label="Send a test direct message"
            htmlFor="bot-test-user"
            helpId="bot-test-user-help"
            help="In Discord, open User Settings, select Advanced, and turn on Developer Mode. Right-click the person, select Copy User ID, and paste the number here."
          >
            <div className="bot-token-row">
              <input
                id="bot-test-user"
                className="input"
                inputMode="numeric"
                value={userId}
                placeholder="Discord user id"
                aria-describedby={
                  error?.where === "test"
                    ? "bot-test-user-help bot-error-test"
                    : "bot-test-user-help"
                }
                aria-invalid={error?.where === "test" || undefined}
                ref={testInputRef}
                onChange={(event) => setUserId(event.target.value)}
              />
              <Button
                variant="secondary"
                icon="mail"
                onClick={handleTestDm}
                disabled={!userId.trim() || busy !== null}
              >
                {busy === "test" ? "Sending…" : "Send test"}
              </Button>
            </div>
          </Field>
          {error?.where === "test" ? <ErrorNote error={error} /> : null}
        </div>
      ) : null}

      {saved ? (
        <div className="bot-remove">
          <div>
            <span className="field-label">Remove this connection</span>
            <span className="field-help">
              {settings.tokenStorage === "session"
                ? "Deletes the stored token for this session and the channel choices. "
                : "Deletes the stored token and the channel choices. "}
              The bot goes offline and stops sending messages until you connect
              it again. Discord messages stay on Discord.
            </span>
          </div>
          {confirmClear ? null : (
            <Button
              id="bot-remove"
              variant="danger"
              icon="trash"
              onClick={requestRemove}
              disabled={busy !== null}
            >
              Remove connection
            </Button>
          )}
        </div>
      ) : null}

      {confirmClear ? (
        <div className="bot-confirm" role="alert">
          <div>
            <strong>Remove the bot connection?</strong>
            <span className="field-help">
              The bot goes offline and stops sending event messages until you
              connect it again. Discord messages stay on Discord.
            </span>
          </div>
          <div className="bot-confirm__actions">
            <Button variant="secondary" onClick={cancelRemove}>
              Keep connection
            </Button>
            <Button
              id="bot-confirm-remove"
              variant="danger"
              icon="trash"
              ariaLabel="Confirm remove connection"
              onClick={handleClear}
              disabled={busy !== null}
            >
              {busy === "clear" ? "Removing…" : "Remove connection"}
            </Button>
          </div>
        </div>
      ) : null}
      {error?.where === "remove" ? <ErrorNote error={error} /> : null}

      {showSave && saveHelp ? (
        <p className="field-help" id="bot-save-help">
          {saveHelp}
        </p>
      ) : null}

      {notice ? (
        <p
          className={`bot-notice bot-notice--${notice.tone}`}
          role="status"
        >
          <Icon name={notice.tone === "warn" ? "alert" : "check"} size={16} />
          {notice.text}
        </p>
      ) : null}
    </Dialog>
  );
}

function ErrorNote({ error }: { error: BotError }) {
  return (
    <p
      id={`bot-error-${error.where}`}
      className="field-error"
      role="alert"
      tabIndex={-1}
    >
      {error.message} <code>{error.code}</code>
    </p>
  );
}

function channelWarning(count: number): Notice {
  if (count === 0) {
    return {
      tone: "warn",
      text: "This server has no text channels that the bot can see. Create a text channel, then select Refresh servers.",
    };
  }
  return {
    tone: "warn",
    text: "This server has one text channel. Create a second text channel, then select Refresh servers.",
  };
}

function describeError(error: unknown, where: Where): BotError {
  const message =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "";
  const lower = message.toLowerCase();

  if (
    lower.includes("does not provide a keychain") ||
    lower.includes("keychain is not available") ||
    lower.includes("no keychain")
  ) {
    return {
      where,
      code: "MSW-BOT-002",
      message:
        "This computer does not provide a keychain. Select Use this session only to continue.",
    };
  }
  if (lower.includes("keychain is locked") || lower.includes("keychain refused")) {
    return {
      where,
      code: "MSW-BOT-003",
      message: "The OS keychain is locked. Unlock it and try again.",
    };
  }
  if (
    lower.includes("three-part") ||
    lower.includes("expected shape") ||
    lower.includes("token is empty") ||
    lower.includes("token is not valid") ||
    lower.includes("rejected the bot token")
  ) {
    return {
      where: "token",
      code: "MSW-BOT-001",
      message:
        "Discord rejected this token. Copy it again from the Bot tab in the Discord Developer Portal. Do not use the client secret.",
    };
  }
  if (lower.includes("shares no guild") || lower.includes("no mutual")) {
    if (where === "test") {
      return {
        where: "test",
        code: "MSW-BOT-006",
        message:
          "The bot and this person do not share a server. Choose a person in the server, or ask them to join the server.",
      };
    }
    return {
      where: "mapping",
      code: "MSW-BOT-004",
      message:
        "The bot is not in that server. Open the invite, add the bot, then select Refresh servers.",
      focusId: "bot-guild",
    };
  }
  if (lower.includes("choose a server")) {
    return {
      where: "mapping",
      code: "MSW-BOT-004",
      message:
        "The bot is not in that server. Open the invite, add the bot, then select Refresh servers.",
      focusId: "bot-guild",
    };
  }
  if (
    lower.includes("announcement channel") ||
    lower.includes("reveal channel") ||
    lower.includes("two different")
  ) {
    return {
      where: "mapping",
      code: "MSW-BOT-005",
      message: "Choose two different text channels that the bot can see.",
      focusId: "bot-announcement",
    };
  }
  if (lower.includes("does not exist") || lower.includes("unknown user")) {
    return {
      where: "test",
      code: "MSW-BOT-007",
      message:
        "Discord does not know that user id. Copy the user id again and send the test.",
    };
  }
  if (
    lower.includes("direct message") ||
    lower.includes("dmclosed") ||
    lower.includes("blocked")
  ) {
    return {
      where: "test",
      code: "MSW-BOT-006",
      message:
        "The person blocks direct messages from server members. Ask them to allow direct messages and send the test again.",
    };
  }
  if (lower.includes("user id") || lower.includes("snowflake")) {
    return {
      where: "test",
      code: "MSW-BOT-007",
      message: "Enter the Discord user id. Use only the number.",
    };
  }
  if (lower.includes("rate-limited") || lower.includes("rate limited")) {
    return {
      where,
      code: "MSW-BOT-008",
      message: "Discord asked the app to slow down. Wait one minute and try again.",
    };
  }
  if (
    lower.includes("transport") ||
    lower.includes("timed out") ||
    lower.includes("did not answer")
  ) {
    return {
      where,
      code: "MSW-BOT-009",
      message:
        "Discord did not answer. Check the internet connection and try again.",
    };
  }
  return {
    where,
    code: "MSW-BOT-099",
    message:
      "The setup request failed. Check the token and the connection, then try again.",
  };
}
