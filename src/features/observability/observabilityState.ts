import type {
  IpcError,
  ProfilerEvent,
  PubSubMessageEvent,
  PubSubTopic,
} from "../../lib/types";

export const MAX_PUBSUB_MESSAGES = 5_000;
export const MAX_PROFILER_EVENTS = 10_000;

export function parsePubSubTopics(value: string, pattern: boolean): PubSubTopic[] {
  const seen = new Set<string>();
  return value
    .split(/\r?\n/)
    .map((topic) => topic.trim())
    .filter((topic) => topic.length > 0)
    .filter((topic) => {
      const key = `${pattern ? "pattern" : "channel"}:${topic}`;
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    })
    .map((name) => ({ name, pattern }));
}

export function appendPubSubMessage(
  messages: PubSubMessageEvent[],
  message: PubSubMessageEvent,
): PubSubMessageEvent[] {
  const next = [...messages, message];
  return next.length > MAX_PUBSUB_MESSAGES
    ? next.slice(next.length - MAX_PUBSUB_MESSAGES)
    : next;
}

export function appendProfilerEvent(
  events: ProfilerEvent[],
  event: ProfilerEvent,
): ProfilerEvent[] {
  const next = [...events, event];
  return next.length > MAX_PROFILER_EVENTS
    ? next.slice(next.length - MAX_PROFILER_EVENTS)
    : next;
}

export function toUserFacingObservabilityError(error: unknown): string {
  const code = isIpcError(error) ? error.code : "";
  const messages: Record<string, string> = {
    CONNECTION_FAILED: "无法连接到 Redis 服务器，请检查连接状态。",
    AUTHENTICATION_FAILED: "Redis 身份验证失败，请检查凭据。",
    INVALID_CONNECTION: "连接配置无效，请重新连接。",
    INVALID_INPUT: "输入参数无效，请检查后重试。",
    COMMAND_FAILED: "Redis 运维操作失败，请稍后重试。",
    OPERATION_CANCELLED: "操作已取消。",
    IPC_ERROR: "桌面端调用失败，请稍后重试。",
  };
  return messages[code] ?? "运维观察操作失败，请稍后重试。";
}

function isIpcError(error: unknown): error is IpcError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
  );
}

export function formatSlowLogTime(seconds: number): string {
  if (!Number.isFinite(seconds)) {
    return "不可用";
  }
  return new Date(seconds * 1_000).toLocaleString("zh-CN", {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function formatSlowLogDuration(durationUs: number): string {
  if (durationUs < 1_000) {
    return `${durationUs} μs`;
  }
  if (durationUs < 1_000_000) {
    return `${(durationUs / 1_000).toFixed(2)} ms`;
  }
  return `${(durationUs / 1_000_000).toFixed(2)} s`;
}

export function formatPubSubTime(timestampMs: number): string {
  if (!Number.isFinite(timestampMs)) {
    return "不可用";
  }
  return new Date(timestampMs).toLocaleTimeString("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function formatProfilerTime(timestamp: string): string {
  const seconds = Number(timestamp);
  return formatSlowLogTime(seconds);
}

export function formatProfilerCommand(args: string[]): string {
  return args.map((arg) => JSON.stringify(arg)).join(" ");
}
