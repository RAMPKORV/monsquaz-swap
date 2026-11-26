import { useEffect, useId, useMemo, useState } from "react";
import {
  Button,
  Dialog,
  EmptyState,
  PageHeader,
  Skeleton,
  StatusPill,
} from "../components";
import { Icon } from "../icons";
import { isDemoMode, previewUrl } from "../data/client";
import type {
  RevealAlbumTrack,
  RevealComment,
  RevealPackage,
  RevealVersion,
} from "../types";

export function RevealSkeleton() {
  return (
    <div
      className="view reveal-view"
      role="status"
      aria-busy="true"
      aria-label="Loading the reveal show"
    >
      <span className="visually-hidden">Loading the reveal show</span>
      <h1 className="visually-hidden" tabIndex={-1}>
        Reveal show
      </h1>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="130px" height={12} />
          <Skeleton width="360px" height={44} className="skeleton--title" />
          <Skeleton width="min(100%, 540px)" height={16} />
        </div>
      </div>
      <div className="reveal-layout">
        <Skeleton width="100%" height={420} />
        <div className="reveal-detail">
          <Skeleton width="100%" height={150} />
          <Skeleton width="100%" height={220} />
          <Skeleton width="100%" height={220} />
        </div>
      </div>
    </div>
  );
}

export function RevealView({
  data,
  streamSafe,
  onStreamSafeChange,
  onSetTitle,
  onAddComment,
  onRedact,
  onPublish,
  onAction,
}: {
  data: RevealPackage;
  streamSafe: boolean;
  onStreamSafeChange: (value: boolean) => void;
  onSetTitle: (trackCode: string, title: string) => Promise<void>;
  onAddComment: (
    trackCode: string,
    participantId: string,
    kind: string,
    body: string,
  ) => Promise<void>;
  onRedact: (commentId: string) => Promise<void>;
  onPublish: () => Promise<{ published: boolean; blocked: string[] }>;
  onAction: (message: string) => void;
}) {
  const [selectedCode, setSelectedCode] = useState(
    () =>
      new URLSearchParams(window.location.search).get("track") ??
      data.album[0]?.trackCode ??
      "",
  );
  const [confirmStreamOff, setConfirmStreamOff] = useState(false);
  const [publishBlocked, setPublishBlocked] = useState<string[]>(() => {
    if (
      isDemoMode() &&
      new URLSearchParams(window.location.search).get("blocked") === "1"
    ) {
      return ["Track S7F3A · round 3"];
    }
    return [];
  });
  const [titleDraft, setTitleDraft] = useState("");
  const [commentBody, setCommentBody] = useState("");
  const [commentKind, setCommentKind] = useState("comment");
  const [commentAuthor, setCommentAuthor] = useState("");
  const [confirmPublish, setConfirmPublish] = useState(
    () => new URLSearchParams(window.location.search).get("dialog") === "publish",
  );
  const [redactTarget, setRedactTarget] = useState<RevealComment | null>(() =>
    new URLSearchParams(window.location.search).get("dialog") === "redact"
      ? (data.album[0]?.comments[0] ?? null)
      : null,
  );
  const [busy, setBusy] = useState(false);

  const selected =
    data.album.find((track) => track.trackCode === selectedCode) ??
    data.album[0];
  const people = useParticipants(data);
  const trackPeople = useMemo(() => {
    if (!selected) {
      return [];
    }
    const found = new Map<string, string>();
    found.set(selected.seedOwnerId, selected.seedOwnerName);
    for (const credit of selected.credits) {
      if (credit.participantId) {
        found.set(credit.participantId, credit.displayName);
      }
    }
    for (const comment of selected.comments) {
      if (comment.participantId) {
        found.set(comment.participantId, comment.authorName);
      }
    }
    return Array.from(found, ([id, name]) => ({ id, name })).sort(
      (left, right) => left.name.localeCompare(right.name),
    );
  }, [selected]);
  const commentChoices = trackPeople.length > 0 ? trackPeople : people;
  const previewOnly = !data.published;

  useEffect(() => {
    setTitleDraft(selected?.title ?? "");
  }, [selected?.trackCode, selected?.title]);

  useEffect(() => {
    const section = new URLSearchParams(window.location.search).get("section");
    if (section) {
      document
        .getElementById(`reveal-${section}-title`)
        ?.scrollIntoView({ block: section === "tools" ? "center" : "start" });
    }
  }, [data]);

  useEffect(() => {
    if (!commentChoices.some((person) => person.id === commentAuthor)) {
      setCommentAuthor(commentChoices[0]?.id ?? "");
    }
  }, [commentAuthor, commentChoices]);

  const seedVersion = selected?.versions[0];
  const finalVersion = selected?.versions[selected.versions.length - 1];

  function roleFor(participantId: string, realName: string) {
    if (!streamSafe) {
      return realName;
    }
    if (selected?.seedOwnerId === participantId) {
      return "Seed owner";
    }
    const credit = selected?.credits.find(
      (entry) =>
        entry.participantId === participantId && entry.status === "Submitted",
    );
    if (credit) {
      return `Editor ${credit.round + 1}`;
    }
    const unknown = Array.from(
      new Set(
        selected?.comments
          .map((comment) => comment.participantId)
          .filter(
            (id) =>
              id.length > 0 &&
              id !== selected?.seedOwnerId &&
              !selected?.credits.some(
                (entry) =>
                  entry.participantId === id && entry.status === "Submitted",
              ),
          ) ?? [],
      ),
    ).sort();
    const index = unknown.indexOf(participantId);
    return index >= 0 ? `Participant ${index + 1}` : "Participant";
  }

  async function saveTitle() {
    if (!selected || busy) {
      return;
    }
    setBusy(true);
    try {
      await onSetTitle(selected.trackCode, titleDraft);
      onAction("Track title saved.");
    } catch {
      // The app reports the error.
    } finally {
      setBusy(false);
    }
  }

  async function saveComment() {
    if (!selected || !commentAuthor || busy) {
      return;
    }
    setBusy(true);
    try {
      await onAddComment(
        selected.trackCode,
        commentAuthor,
        commentKind,
        commentBody,
      );
      setCommentBody("");
      onAction("Comment added to the reveal package.");
    } catch {
      // The app reports the error.
    } finally {
      setBusy(false);
    }
  }

  async function publish() {
    if (busy) {
      return;
    }
    setBusy(true);
    try {
      const result = await onPublish();
      if (!result.published) {
        setPublishBlocked(result.blocked);
        return;
      }
      setPublishBlocked([]);
      setConfirmPublish(false);
      onAction("Reveal published to the reveal channel.");
    } catch {
      // The app reports the error.
    } finally {
      setBusy(false);
    }
  }

  async function confirmRedact() {
    if (!redactTarget || busy) {
      return;
    }
    setBusy(true);
    try {
      await onRedact(redactTarget.id);
      setRedactTarget(null);
      onAction("Comment redacted. It will not appear in the reveal.");
    } catch {
      // The app reports the error.
    } finally {
      setBusy(false);
    }
  }

  if (data.album.length === 0) {
    return (
      <div className="view">
        <PageHeader
          title="Reveal show"
          description="The host-only presenter appears after the event has tracks. Start the event and open a round first."
        />
        <EmptyState
          icon="sparkles"
          title="The album is not ready"
          detail="The reveal show appears after the event has tracks. Start the event and open a round first."
          action={
            <Button
              variant="primary"
              onClick={() => onAction("Open event setup")}
            >
              Open event setup
            </Button>
          }
        />
      </div>
    );
  }

  return (
    <div className={`view reveal-view${streamSafe ? " reveal-view--safe" : ""}`}>
      <header className="reveal-header">
        <div>
          <p className="eyebrow">
            {data.published
              ? "Published reveal package"
              : "Host-only reveal package"}
          </p>
          <h1 tabIndex={-1}>Reveal show</h1>
          <p className="lede">
            {data.published
              ? "The reveal is published. This window is the read-only show for the stream."
              : "Check titles, credits, comments, and audio before you publish. Nothing is public yet."}
          </p>
        </div>
        <div className="reveal-header__actions">
          <StatusPill tone={data.published ? "ok" : "warn"} icon="eye-off">
            {data.published ? "Published" : "Not published"}
          </StatusPill>
          <span id="stream-safe-help" className="visually-hidden">
            Hides participant names and the host editing tools for screen
            sharing.
          </span>
          <Button
            variant="secondary"
            icon={streamSafe ? "eye-off" : "monitor"}
            pressed={streamSafe}
            ariaLabel="Stream-safe mode"
            describedBy="stream-safe-help"
            onClick={() => {
              if (streamSafe) {
                setConfirmStreamOff(true);
              } else {
                onStreamSafeChange(true);
              }
            }}
          >
            {streamSafe ? "Stream-safe is on" : "Stream-safe off"}
          </Button>
          {!data.published ? (
            <Button
              variant="primary"
              icon="sparkles"
              onClick={() => setConfirmPublish(true)}
            >
              Publish reveal
            </Button>
          ) : null}
        </div>
      </header>

      {streamSafe ? (
        <p className="reveal-safe-note" role="status">
          <Icon name="shield" size={16} />
          {data.published
            ? " Stream-safe hides the public credits. Turn it off to show the reveal."
            : " Name labels and the host editing tools are hidden. Comment text is shown as written. Publishing stays available."}
        </p>
      ) : data.published ? (
        <p className="reveal-public-note" role="status">
          <Icon name="check" size={16} /> The reveal is public. Share the names.
        </p>
      ) : (
        <p className="reveal-unsafe-note" role="status">
          <Icon name="alert" size={16} /> Names are visible. Turn on stream-safe
          mode before you share your screen.
        </p>
      )}

      <div className="reveal-layout">
        <p className="visually-hidden" role="status">
          Showing {selected?.title ?? "untitled"} track, position{" "}
          {selected?.position ?? 1} of {data.album.length}.
        </p>
        <nav className="card reveal-album" aria-label="Album order">
          <div className="card__header">
            <h2>Album</h2>
            <span className="card__meta">{data.album.length} tracks</span>
          </div>
          <ol className="reveal-album__list" role="list">
            {data.album.map((track) => (
              <li key={track.trackCode}>
                <button
                  type="button"
                  className={`reveal-album__item${
                    track.trackCode === selected?.trackCode
                      ? " reveal-album__item--current"
                      : ""
                  }`}
                  aria-current={
                    track.trackCode === selected?.trackCode ? "true" : undefined
                  }
                  onClick={() => setSelectedCode(track.trackCode)}
                >
                  <span className="reveal-album__position">
                    {String(track.position).padStart(2, "0")}
                  </span>
                  <span className="reveal-album__title">
                    <strong>{track.title ?? "Untitled"}</strong>
                    <span>
                      {track.trackCode} · {track.stats.edits} edits
                    </span>
                  </span>
                  <Icon name="arrow-right" size={15} />
                </button>
              </li>
            ))}
          </ol>
        </nav>

        {selected ? (
          <article className="reveal-detail" aria-labelledby="reveal-track-title">
            <div className="card reveal-track-head">
              <div>
                <p className="eyebrow">
                  Track {selected.position} of {data.album.length}
                </p>
                <h2 id="reveal-track-title">
                  {selected.title ?? "Untitled track"}
                </h2>
                <p className="reveal-track-code">
                  {selected.trackCode} · seeded by{" "}
                  {roleFor(selected.seedOwnerId, selected.seedOwnerName)}
                </p>
              </div>
              <dl className="reveal-stats">
                <div>
                  <dt>Edits</dt>
                  <dd>{selected.stats.edits}</dd>
                </div>
                <div>
                  <dt>Fill-ins</dt>
                  <dd>{selected.stats.fillIns}</dd>
                </div>
                <div>
                  <dt>Gaps</dt>
                  <dd>{selected.stats.skips}</dd>
                </div>
                <div>
                  <dt>People</dt>
                  <dd>{selected.stats.contributors}</dd>
                </div>
              </dl>
            </div>

            <section className="card" aria-labelledby="reveal-audio-title">
              <div className="card__header">
                <h2 id="reveal-audio-title">Listen</h2>
                <span className="card__meta">R1 seed and final version</span>
              </div>
              <div className="reveal-audio-grid">
                {seedVersion ? (
                  <AudioPlayer
                    label="R1 seed"
                    version={seedVersion}
                    name={roleFor(seedVersion.holderId, seedVersion.holderName)}
                  />
                ) : null}
                {finalVersion ? (
                  <AudioPlayer
                    label="Final"
                    version={finalVersion}
                    name={roleFor(finalVersion.holderId, finalVersion.holderName)}
                  />
                ) : null}
              </div>
            </section>

            <section className="card" aria-labelledby="reveal-lineage-title">
              <div className="card__header">
                <h2 id="reveal-lineage-title">Lineage</h2>
                <span className="card__meta">
                  {selected.versions.length} versions
                </span>
              </div>
              <ol className="reveal-lineage" role="list">
                {selected.versions.map((version) => (
                  <li key={version.fileId}>
                    <span className="reveal-lineage__dot" aria-hidden="true" />
                    <div>
                      <strong>
                        {version.role === "seed" ? "Seed" : "Edit"} · v
                        {version.version}
                      </strong>
                      <span>
                        {roleFor(version.holderId, version.holderName)}
                        {version.round !== null ? ` · Round ${version.round + 1}` : ""}
                        {version.finalVersion ? " · Final" : ""}
                      </span>
                    </div>
                    <span className="reveal-lineage__kind">
                      .{version.extension}
                    </span>
                  </li>
                ))}
                {selected.credits
                  .filter((credit) => credit.status !== "Submitted")
                  .map((credit) => (
                    <li key={`gap-${credit.round}`} className="reveal-lineage__gap">
                      <span
                        className="reveal-lineage__dot reveal-lineage__dot--gap"
                        aria-hidden="true"
                      />
                      <div>
                        <strong>Gap · no edit</strong>
                        <span>Round {credit.round + 1} · the pair is anonymous</span>
                      </div>
                      <span className="reveal-lineage__kind">No file</span>
                    </li>
                  ))}
              </ol>
            </section>

            <section className="card" aria-labelledby="reveal-credits-title">
              <div className="card__header">
                <h2 id="reveal-credits-title">Credits</h2>
                <span className="card__meta">
                  Seed owner and every round
                </span>
              </div>
              <ol className="reveal-credits" role="list">
                <li className="reveal-credits__seed">
                  <span>Seed</span>
                  <strong>
                    {roleFor(selected.seedOwnerId, selected.seedOwnerName)}
                  </strong>
                  <span className="badge">Original track</span>
                </li>
                {selected.credits.map((credit, index) => (
                  <li key={`${credit.round}-${index}`}>
                    <span>Round {credit.round + 1}</span>
                    <strong>
                      {credit.status === "Submitted"
                        ? roleFor(credit.participantId, credit.displayName)
                        : "No edit"}
                    </strong>
                    <span className="reveal-credits__marks">
                      {credit.fillIn ? (
                        <span className="badge badge--info">Fill-in</span>
                      ) : null}
                      {credit.status !== "Submitted" ? (
                        <span className="badge">Gap</span>
                      ) : null}
                    </span>
                  </li>
                ))}
              </ol>
            </section>

            <section className="card" aria-labelledby="reveal-comments-title">
              <div className="card__header">
                <h2 id="reveal-comments-title">Comments and answers</h2>
                <span className="card__meta">
                  {selected.comments.length}{" "}
                  {selected.comments.length === 1 ? "comment" : "comments"}
                </span>
              </div>
              {selected.comments.length === 0 ? (
                <p className="muted">
                  No comments or questionnaire answers for this track.
                </p>
              ) : (
                <ul className="reveal-comments" role="list">
                  {selected.comments.map((comment) => (
                    <li key={comment.id}>
                      <div>
                        <strong>
                          {roleFor(comment.participantId, comment.authorName)}
                        </strong>
                        <span className="badge">
                          {comment.kind === "questionnaire"
                            ? "Questionnaire"
                            : "Comment"}
                        </span>
                      </div>
                      <p>{comment.body}</p>
                      {previewOnly && !streamSafe ? (
                        <Button
                          variant="quiet"
                          icon="eye-off"
                          ariaLabel={`Redact ${comment.authorName}'s comment`}
                          onClick={() => setRedactTarget(comment)}
                        >
                          Redact
                        </Button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {previewOnly && !streamSafe ? (
              <section className="card" aria-labelledby="reveal-tools-title">
                <div className="card__header">
                  <h2 id="reveal-tools-title">Host tools</h2>
                  <span className="card__meta">Before publish only</span>
                </div>
                <div className="reveal-tools">
                  <form
                    className="reveal-tool"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void saveTitle();
                    }}
                  >
                    <label htmlFor="reveal-title">Track title</label>
                    <div className="reveal-tool__row">
                      <input
                        id="reveal-title"
                        className="input"
                        value={titleDraft}
                        maxLength={80}
                        placeholder="Name the final track"
                        aria-describedby="reveal-title-limit"
                        onChange={(event) => setTitleDraft(event.target.value)}
                      />
                      <Button
                        variant="secondary"
                        type="submit"
                        disabled={titleDraft.trim().length === 0}
                      >
                        Save title
                      </Button>
                    </div>
                    <span id="reveal-title-limit" className="reveal-tool__hint">
                      Up to 80 characters. The host can change this until the
                      reveal.
                    </span>
                  </form>
                  <form
                    className="reveal-tool"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void saveComment();
                    }}
                  >
                    <fieldset className="reveal-tool__fieldset">
                      <legend>Add a comment or answer</legend>
                      <div className="reveal-tool__row">
                        <label htmlFor="comment-author">Author</label>
                        <select
                          id="comment-author"
                          className="input"
                          value={commentAuthor}
                          onChange={(event) =>
                            setCommentAuthor(event.target.value)
                          }
                        >
                          {commentChoices.map((person) => (
                            <option key={person.id} value={person.id}>
                              {person.name}
                            </option>
                          ))}
                        </select>
                        <label htmlFor="comment-kind">Kind</label>
                        <select
                          id="comment-kind"
                          className="input"
                          value={commentKind}
                          onChange={(event) => setCommentKind(event.target.value)}
                        >
                          <option value="comment">Comment</option>
                          <option value="questionnaire">Questionnaire answer</option>
                        </select>
                      </div>
                      <label htmlFor="reveal-comment">Comment or answer</label>
                      <textarea
                        id="reveal-comment"
                        className="input"
                        rows={3}
                        maxLength={1000}
                        value={commentBody}
                        placeholder="Add the quote or answer for this track"
                        aria-describedby="reveal-comment-limit"
                        onChange={(event) => setCommentBody(event.target.value)}
                      />
                      <span id="reveal-comment-limit" className="reveal-tool__hint">
                        Up to 1,000 characters. Redact it before the reveal if it
                        must not appear.
                      </span>
                      <Button
                        variant="secondary"
                        type="submit"
                        disabled={
                          commentBody.trim().length === 0 ||
                          commentAuthor.length === 0
                        }
                      >
                        Add comment
                      </Button>
                    </fieldset>
                  </form>
                </div>
              </section>
            ) : null}
          </article>
        ) : null}
      </div>

      {confirmStreamOff ? (
        <Dialog
          title="Turn off stream-safe mode?"
          initialFocusId="stream-off-cancel"
          onClose={() => setConfirmStreamOff(false)}
          footer={
            <>
              <Button
                id="stream-off-cancel"
                variant="secondary"
                onClick={() => setConfirmStreamOff(false)}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                className="dialog__danger-action"
                onClick={() => {
                  onStreamSafeChange(false);
                  setConfirmStreamOff(false);
                }}
              >
                Turn off stream-safe
              </Button>
            </>
          }
        >
          <p>
            {data.published
              ? "Participant names, titles, credits, and comments become visible to the stream. This is the published reveal."
              : "Participant names and the host editing tools become visible. Only turn this off when the screen is not shared."}
          </p>
        </Dialog>
      ) : null}

      {confirmPublish ? (
        <Dialog
          title="Publish the reveal?"
          initialFocusId="publish-cancel"
          onClose={() => setConfirmPublish(false)}
          footer={
            <>
              <Button
                id="publish-cancel"
                variant="secondary"
                onClick={() => setConfirmPublish(false)}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                className="dialog__danger-action"
                onClick={() => void publish()}
              >
                Publish reveal
              </Button>
            </>
          }
        >
          <p>
            The bot publishes the titles, credits, and comments to the reveal
            channel. This ends anonymity and cannot be undone.
          </p>
          <p className="dialog__meta">
            The album has {data.album.length} tracks. Check stream-safe and every
            comment before you publish.
          </p>
          {publishBlocked.length > 0 ? (
            <div className="dialog__blocked" role="alert">
              <strong>The reveal is still blocked.</strong>
              <ul>
                {publishBlocked.map((blocked) => (
                  <li key={blocked}>{blocked}</li>
                ))}
              </ul>
              <span>Resolve these assignments in mission control.</span>
            </div>
          ) : null}
        </Dialog>
      ) : null}

      {redactTarget ? (
        <Dialog
          title="Redact this comment?"
          initialFocusId="redact-cancel"
          onClose={() => setRedactTarget(null)}
          footer={
            <>
              <Button
                id="redact-cancel"
                variant="secondary"
                onClick={() => setRedactTarget(null)}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                className="dialog__danger-action"
                onClick={() => void confirmRedact()}
              >
                Redact comment
              </Button>
            </>
          }
        >
          <p>
            The comment will not appear in the reveal, the presenter, or the
            album package. This cannot be undone.
          </p>
          <p className="dialog__meta">“{redactTarget.body}”</p>
        </Dialog>
      ) : null}
    </div>
  );
}

function AudioPlayer({
  label,
  version,
  name,
}: {
  label: string;
  version: RevealVersion;
  name: string;
}) {
  const url = previewUrl(version.fileId);
  const [failed, setFailed] = useState(false);
  const noteId = useId();
  return (
    <div className="reveal-audio">
      <div className="reveal-audio__head">
        <strong>{label}</strong>
        <span>
          v{version.version} · {name}
        </span>
      </div>
      {url ? (
        <>
          <audio
            controls
            preload="none"
            src={url}
            aria-label={`${label} audio preview, version ${version.version}, ${name}`}
            aria-describedby={noteId}
            onError={() => setFailed(true)}
          />
          <span id={noteId} className="reveal-audio__note">
            Preview clip, up to 2 minutes.
          </span>
          {failed ? (
            <span className="reveal-audio__error" role="alert">
              The preview could not be prepared. Check the audio tools and try
              again.
            </span>
          ) : null}
        </>
      ) : (
        <p className="muted">Audio preview opens in the desktop app.</p>
      )}
      <span className="reveal-audio__kind">.{version.extension}</span>
    </div>
  );
}

function useParticipants(data: RevealPackage) {
  return useMemo(() => {
    const found = new Map<string, string>();
    for (const track of data.album) {
      found.set(track.seedOwnerId, track.seedOwnerName);
      for (const credit of track.credits) {
        if (credit.participantId) {
          found.set(credit.participantId, credit.displayName);
        }
      }
      for (const comment of track.comments) {
        if (comment.participantId) {
          found.set(comment.participantId, comment.authorName);
        }
      }
    }
    return Array.from(found, ([id, name]) => ({ id, name })).sort((left, right) =>
      left.name.localeCompare(right.name),
    );
  }, [data]);
}

export function revealInitials(track: RevealAlbumTrack): string {
  return track.title?.slice(0, 2).toUpperCase() ?? track.trackCode.slice(0, 2);
}
