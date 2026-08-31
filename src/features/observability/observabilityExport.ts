import type { ProfilerEvent, PubSubMessageEvent, SlowLogEntry } from "../../lib/types";
import { formatProfilerCommand } from "./observabilityState";

export interface ObservabilityExport {
  filename: string;
  mimeType: string;
  content: string;
}

function matches(values: unknown[], query: string): boolean {
  return values.join(" ").toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
}

export function filterProfilerEvents(events: ProfilerEvent[], query: string): ProfilerEvent[] {
  return events.filter((event) => matches([`DB${event.database}`, event.source, ...event.args], query));
}

export function filterPubSubMessages(messages: PubSubMessageEvent[], query: string): PubSubMessageEvent[] {
  return messages.filter((message) => matches([message.channel, message.pattern, message.message], query));
}

export function filterSlowLogs(entries: SlowLogEntry[], query: string): SlowLogEntry[] {
  return entries.filter((entry) => matches([entry.id, entry.source, entry.client, ...entry.args], query));
}

export function buildProfilerExport(events: ProfilerEvent[], query = ""): ObservabilityExport {
  const escapeLine = (value: string) => JSON.stringify(value).slice(1, -1);
  const content = filterProfilerEvents(events, query).map((event) =>
    `${escapeLine(event.time)} [${event.database} ${escapeLine(event.source)}] ${formatProfilerCommand(event.args)}\n`,
  ).join("");
  return { filename: "profiler.log", mimeType: "text/plain;charset=utf-8", content };
}

export function buildPubSubExport(messages: PubSubMessageEvent[], query = ""): ObservabilityExport {
  return { filename: "pubsub.json", mimeType: "application/json;charset=utf-8", content: JSON.stringify(filterPubSubMessages(messages, query), null, 2) };
}

function csvCell(value: string | number): string {
  const raw = String(value);
  // Prevent spreadsheet formula evaluation for untrusted command/client fields.
  const safe = /^\s*[=+\-@\t\r]/.test(raw) ? `'${raw}` : raw;
  return `"${safe.replace(/"/g, '""')}"`;
}

export function buildSlowLogExport(entries: SlowLogEntry[], format: "csv" | "json", query = ""): ObservabilityExport {
  const filtered = filterSlowLogs(entries, query);
  if (format === "json") return { filename: "slowlog.json", mimeType: "application/json;charset=utf-8", content: JSON.stringify(filtered, null, 2) };
  const rows: (string | number)[][] = [["id", "time", "duration_us", "command", "source", "client"], ...filtered.map((entry) =>
    [entry.id, entry.time, entry.duration_us, formatProfilerCommand(entry.args), entry.source ?? "", entry.client ?? ""],
  )];
  return { filename: "slowlog.csv", mimeType: "text/csv;charset=utf-8", content: rows.map((row) => row.map(csvCell).join(",")).join("\r\n") + "\r\n" };
}

// Called only by explicit download buttons. Nothing is persisted on capture.
export function downloadObservabilityExport(output: ObservabilityExport): void {
  const url = URL.createObjectURL(new Blob([output.content], { type: output.mimeType }));
  const revoke = URL.revokeObjectURL.bind(URL);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = output.filename;
  document.body.append(anchor);
  try { anchor.click(); } finally {
    anchor.remove();
    setTimeout(() => revoke(url), 1000);
  }
}
