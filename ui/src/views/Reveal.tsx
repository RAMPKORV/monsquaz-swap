import { useEffect, useMemo, useState } from "react";
import { Button, Dialog, EmptyState, StatusPill } from "../components";
import { Icon } from "../icons";
import { previewUrl } from "../data/client";
import type {
  RevealAlbumTrack,
  RevealComment,
  RevealPackage,
  RevealVersion,
} from "../types";

export function RevealSkeleton() {
  return (
    <div className="view reveal-view" aria-busy="true">
      <div className="skeleton skeleton--title" />
      <div className="reveal-layout">
        <div className="skeleton skeleton--panel" />
        <div className="skeleton skeleton--panel" />
      </div>
    </div>
  );
}

export function RevealView({
  data,
  onSetTitle,
  onAddComment,
  onRedact,
  onPublish,
  onAction,
}: {
  data: RevealPackage;
  onSetTitle: (trackCode: string, title: string) => Promise<void>;
  onAddComment: (
    trackCode: string,
    participantId: string,
    kind: string,
    body: string,
  ) => Promise<void>;
  onRedact: (commentId: string) => Promise<void>;
  onPublish: () => Promise<void>;
  onAction: (message: string) => void;
}) {
  const [selectedCode, setSelectedCode] = useState(
    () => data.album[0]?.trackCode ?? "",
  );
  const [streamSafe, setStreamSafe] = useState(
    () => new URLSearchParams(window.location.search).get("stream") === "1",
  );
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
        ?.scrollIntoView({ block: "start" });
    }
  }, [data]);

  useEffect(() => {
    if (!commentChoices.some((person) => person.id === commentAuthor)) {
      setCommentAuthor(commentChoices[0]?.id ?? "");
    }
  }, [commentAuthor, commentChoices]);

  const seedVersion = selected?.versions[0];
  const finalVersion = selected?.versions[selected.versions.length - 1];

  function hiddenName(real: string, fallback: string) {
    return streamSafe ? fallback : real;
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
      await onPublish();
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
        <EmptyState
          icon="sparkles"
          title="The album is not ready"
          detail="The reveal show appears after the event has tracks. Start the event and open a round first."
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
          <Button
            variant={streamSafe ? "primary" : "secondary"}
            icon={streamSafe ? "eye-off" : "monitor"}
            aria-pressed={streamSafe}
            onClick={() => setStreamSafe((value) => !value)}
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
        <p className="reveal-safe-note">
          <Icon name="shield" size={16} /> Names and host tools are hidden. The
          album, credits, comments, and audio stay visible.
        </p>
      ) : null}

      <div className="reveal-layout">
        <nav className="card reveal-album" aria-label="Album order">
          <div className="card__header">
            <h2>Album</h2>
            <span className="card__meta">
              {data.album.length} tracks · {" "}
              {data.eventName}
            </span>
          </div>
          <ol className="reveal-album__list">
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
                  {hiddenName(selected.seedOwnerName, "the seed owner")}
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
                    name={hiddenName(seedVersion.holderName, "Seed owner")}
                  />
                ) : null}
                {finalVersion ? (
                  <AudioPlayer
                    label="Final"
                    version={finalVersion}
                    name={hiddenName(finalVersion.holderName, "Final editor")}
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
              <ol className="reveal-lineage">
                {selected.versions.map((version) => (
                  <li key={version.fileId}>
                    <span className="reveal-lineage__dot" aria-hidden="true" />
                    <div>
                      <strong>
                        {version.role === "seed" ? "Seed" : "Edit"} · v
                        {version.version}
                      </strong>
                      <span>
                        {hiddenName(
                          version.holderName,
                          version.role === "seed"
                            ? "Seed owner"
                            : `Editor ${version.version - 1}`,
                        )}
                        {version.round !== null ? ` · Round ${version.round + 1}` : ""}
                        {version.finalVersion ? " · Final" : ""}
                      </span>
                    </div>
                    <span className="reveal-lineage__kind">
                      .{version.extension}
                    </span>
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
              <ol className="reveal-credits">
                <li className="reveal-credits__seed">
                  <span>Seed</span>
                  <strong>
                    {hiddenName(selected.seedOwnerName, "The seed owner")}
                  </strong>
                  <span className="badge">Original track</span>
                </li>
                {selected.credits.map((credit, index) => (
                  <li key={`${credit.round}-${index}`}>
                    <span>Round {credit.round + 1}</span>
                    <strong>
                      {credit.status === "Submitted"
                        ? hiddenName(
                            credit.displayName,
                            `Editor ${credit.round + 1}`,
                          )
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
                  {selected.comments.length} visible
                </span>
              </div>
              {selected.comments.length === 0 ? (
                <p className="muted">
                  No comments or questionnaire answers for this track.
                </p>
              ) : (
                <ul className="reveal-comments">
                  {selected.comments.map((comment) => (
                    <li key={comment.id}>
                      <div>
                        <strong>
                          {hiddenName(comment.authorName, "Participant")}
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
                        onChange={(event) => setTitleDraft(event.target.value)}
                      />
                      <Button
                        variant="primary"
                        type="submit"
                        disabled={busy || titleDraft.trim().length === 0}
                      >
                        Save title
                      </Button>
                    </div>
                  </form>
                  <form
                    className="reveal-tool"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void saveComment();
                    }}
                  >
                    <label htmlFor="reveal-comment">Add a comment or answer</label>
                    <div className="reveal-tool__row">
                      <select
                        className="input"
                        aria-label="Comment author"
                        value={commentAuthor}
                        onChange={(event) => setCommentAuthor(event.target.value)}
                      >
                        {commentChoices.map((person) => (
                          <option key={person.id} value={person.id}>
                            {person.name}
                          </option>
                        ))}
                      </select>
                      <select
                        className="input"
                        aria-label="Comment kind"
                        value={commentKind}
                        onChange={(event) => setCommentKind(event.target.value)}
                      >
                        <option value="comment">Comment</option>
                        <option value="questionnaire">Questionnaire</option>
                      </select>
                    </div>
                    <textarea
                      id="reveal-comment"
                      className="input"
                      rows={3}
                      maxLength={1000}
                      value={commentBody}
                      placeholder="Add the quote or answer for this track"
                      onChange={(event) => setCommentBody(event.target.value)}
                    />
                    <Button
                      variant="secondary"
                      type="submit"
                      disabled={
                        busy ||
                        commentBody.trim().length === 0 ||
                        commentAuthor.length === 0
                      }
                    >
                      Add comment
                    </Button>
                  </form>
                </div>
              </section>
            ) : null}
          </article>
        ) : null}
      </div>

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
            {data.album.length} tracks · check the stream-safe setting and every
            comment before you publish.
          </p>
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
            aria-label={`${label} audio preview`}
            onError={() => setFailed(true)}
          />
          <span className="reveal-audio__note">
            Preview clip, up to 2 minutes.
          </span>
          {failed ? (
            <span className="reveal-audio__error">
              The preview did not load. Check that the renderer is available.
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
