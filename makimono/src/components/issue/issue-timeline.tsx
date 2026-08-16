"use client";
// Issue Timeline — Phase 33
// Interleaved comments + events in chronological order

import type { IssueComment, IssueEvent } from "@/lib/issue-api";
import { MentionRenderer } from "@/components/ui/mention-renderer";

interface Props {
  comments: IssueComment[];
  events: IssueEvent[];
}

type TimelineItem =
  | { type: "comment"; data: IssueComment; ts: number }
  | { type: "event"; data: IssueEvent; ts: number };

const eventLabels: Record<string, string> = {
  opened: "opened this issue",
  closed: "closed this issue",
  reopened: "reopened this issue",
  commented: "commented",
  title_changed: "changed the title",
  body_changed: "edited the description",
  label_added: "added a label",
  label_removed: "removed a label",
  assigned: "assigned this issue",
  unassigned: "unassigned this issue",
  mentioned: "mentioned someone",
};

const eventIcons: Record<string, string> = {
  opened: "🎯",
  closed: "🔒",
  reopened: "🔓",
  commented: "💬",
  title_changed: "✏️",
  body_changed: "📝",
  label_added: "🏷️",
  label_removed: "🏷️",
  assigned: "👤",
  unassigned: "👤",
  mentioned: "📣",
};

export function IssueTimeline({ comments, events }: Props) {
  // Merge and sort by timestamp
  const items: TimelineItem[] = [
    ...comments.map((c) => ({
      type: "comment" as const,
      data: c,
      ts: new Date(c.created_at).getTime(),
    })),
    ...events
      .filter((e) => e.event_type !== "commented") // Avoid duplicating comments
      .map((e) => ({
        type: "event" as const,
        data: e,
        ts: new Date(e.created_at).getTime(),
      })),
  ].sort((a, b) => a.ts - b.ts);

  if (items.length === 0) {
    return <div className="issue-timeline-empty">No activity yet</div>;
  }

  return (
    <div className="issue-timeline">
      {items.map((item) => {
        if (item.type === "comment") {
          const c = item.data;
          return (
            <div key={`comment-${c.id}`} className="issue-timeline-comment">
              <div className="issue-comment-header">
                <span className="issue-comment-avatar">💬</span>
                <span className="issue-comment-author">{c.author_handle ?? c.author_id.slice(0, 8)}</span>
                <span className="issue-comment-date">
                  {new Date(c.created_at).toLocaleDateString("fr-FR", {
                    day: "numeric", month: "short", year: "numeric",
                    hour: "2-digit", minute: "2-digit",
                  })}
                </span>
              </div>
              <div className="issue-comment-body"><MentionRenderer text={c.body} /></div>
            </div>
          );
        } else {
          const e = item.data;
          return (
            <div key={`event-${e.id}`} className="issue-timeline-event">
              <span className="issue-event-icon">
                {eventIcons[e.event_type] ?? "📌"}
              </span>
              <span className="issue-event-actor">{e.actor_handle ?? e.actor_id.slice(0, 8)}</span>
              <span className="issue-event-label">
                {eventLabels[e.event_type] ?? e.event_type}
              </span>
              <span className="issue-event-date">
                {new Date(e.created_at).toLocaleDateString("fr-FR", {
                  day: "numeric", month: "short",
                  hour: "2-digit", minute: "2-digit",
                })}
              </span>
            </div>
          );
        }
      })}
    </div>
  );
}
