import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Trash2 } from "lucide-react";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useTranscriptionJobsStore } from "@/stores/transcriptionJobsStore";
import { formatDateTime } from "@/utils/dateFormat";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { copyToClipboard } from "../history/clipboard";

/** Entries per page when reading History to find meetings. */
const PAGE_SIZE = 100;

interface MeetingGroup {
  id: string;
  timestamp: number;
  /** Microphone ("me") first, then system audio ("them"). */
  tracks: HistoryEntry[];
}

const TRACK_ORDER = ["me", "them"];

/** Groups History entries that belong to a meeting, newest meeting first. */
export function groupMeetings(entries: HistoryEntry[]): MeetingGroup[] {
  const groups = new Map<string, MeetingGroup>();
  for (const entry of entries) {
    if (!entry.meeting_id) continue;
    const group = groups.get(entry.meeting_id);
    if (group) {
      group.tracks.push(entry);
      group.timestamp = Math.max(group.timestamp, entry.timestamp);
    } else {
      groups.set(entry.meeting_id, {
        id: entry.meeting_id,
        timestamp: entry.timestamp,
        tracks: [entry],
      });
    }
  }
  for (const group of groups.values()) {
    group.tracks.sort(
      (a, b) =>
        TRACK_ORDER.indexOf(a.meeting_track ?? "") -
        TRACK_ORDER.indexOf(b.meeting_track ?? ""),
    );
  }
  return [...groups.values()].sort((a, b) => b.timestamp - a.timestamp);
}

/** Reads every History page and keeps only meeting tracks. */
async function loadMeetingEntries(): Promise<HistoryEntry[]> {
  const found: HistoryEntry[] = [];
  let cursor: number | null = null;
  for (;;) {
    const result = await commands.getHistoryEntries(cursor, PAGE_SIZE);
    if (result.status !== "ok") break;
    const page = result.data.entries;
    found.push(...page.filter((entry) => entry.meeting_id));
    const last = page[page.length - 1];
    if (!result.data.has_more || !last) break;
    cursor = last.id;
  }
  return found;
}

export const MeetingsSettings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const jobs = useTranscriptionJobsStore((state) => state.jobs);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    commands.isMeetingRecording().then((active) => {
      if (!cancelled) setRecording(active);
    });
    loadMeetingEntries().then((loaded) => {
      if (!cancelled) setEntries(loaded);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // New and updated tracks arrive as history events.
  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added" || payload.action === "updated") {
        const { entry } = payload;
        if (!entry.meeting_id) return;
        setEntries((prev) => [
          entry,
          ...prev.filter((existing) => existing.id !== entry.id),
        ]);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const groups = useMemo(() => groupMeetings(entries), [entries]);

  const toggleRecording = useCallback(async () => {
    setBusy(true);
    try {
      if (recording) {
        const result = await commands.stopMeetingRecording();
        if (result.status === "ok") setRecording(false);
        else console.error("Stopping the meeting failed:", result.error);
      } else {
        const result = await commands.startMeetingRecording();
        if (result.status === "ok") setRecording(true);
        else console.error("Starting the meeting failed:", result.error);
      }
    } finally {
      setBusy(false);
    }
  }, [recording]);

  const deleteMeeting = async (group: MeetingGroup) => {
    const results = await Promise.all(
      group.tracks.map((track) => commands.deleteHistoryEntry(track.id)),
    );
    const removed = new Set(group.tracks.map((track) => track.id));
    if (results.every((result) => result.status === "ok")) {
      setEntries((prev) => prev.filter((entry) => !removed.has(entry.id)));
    } else {
      console.error("Deleting the meeting failed:", results);
    }
  };

  const trackLabel = (track: HistoryEntry) =>
    track.meeting_track === "them"
      ? t("meetings.trackThem")
      : t("meetings.trackMe");

  const copyMeeting = (group: MeetingGroup) => {
    const text = group.tracks
      .filter((track) => track.transcription_text.trim().length > 0)
      .map((track) => `${trackLabel(track)}:\n${track.transcription_text}`)
      .join("\n\n");
    void copyToClipboard(text);
  };

  const trackStatus = (track: HistoryEntry) => {
    const job = jobs[track.id];
    if (job?.state === "running") {
      return t("meetings.transcribing", { percent: Math.round(job.percent) });
    }
    if (job?.state === "queued") return t("meetings.pending");
    if (track.transcription_text.trim().length === 0) {
      return t("meetings.noSpeech");
    }
    return null;
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("meetings.title")}>
        <div className="px-4 py-3 space-y-3">
          <p className="text-sm text-ink-3">{t("meetings.description")}</p>
          <div className="flex items-center gap-3">
            <Button
              variant={recording ? "danger" : "primary"}
              size="md"
              onClick={toggleRecording}
              disabled={busy}
            >
              {recording ? t("meetings.stop") : t("meetings.start")}
            </Button>
            {recording && (
              <span className="flex items-center gap-2 text-sm">
                <span className="inline-block h-2 w-2 rounded-full bg-red-500 animate-pulse" />
                {t("meetings.recording")}
              </span>
            )}
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup>
        {groups.length === 0 && (
          <div className="px-4 py-3 text-sm text-ink-3">
            {t("meetings.empty")}
          </div>
        )}
        {groups.map((group) => (
          <div key={group.id} className="px-4 py-3 space-y-3">
            <div className="flex items-center justify-between gap-2">
              <span className="text-sm font-medium">
                {formatDateTime(String(group.timestamp), i18n.language)}
              </span>
              <div className="flex items-center gap-1">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => copyMeeting(group)}
                  title={t("meetings.copy")}
                >
                  <Copy width={14} height={14} />
                </Button>
                <Button
                  variant="danger-ghost"
                  size="sm"
                  onClick={() => void deleteMeeting(group)}
                  title={t("meetings.delete")}
                >
                  <Trash2 width={14} height={14} />
                </Button>
              </div>
            </div>
            {group.tracks.map((track) => {
              const status = trackStatus(track);
              return (
                <div key={track.id} className="space-y-1">
                  <div className="text-xs font-medium uppercase tracking-wide text-ink-3">
                    {trackLabel(track)}
                  </div>
                  {status ? (
                    <div className="text-sm text-ink-3">{status}</div>
                  ) : (
                    <p className="text-sm whitespace-pre-wrap select-text">
                      {track.transcription_text}
                    </p>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </SettingsGroup>
    </div>
  );
};
