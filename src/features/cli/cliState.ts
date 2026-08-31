export interface CliTranscriptEntry { command: string; output: string; error: boolean }
export const CLI_MAX_ENTRIES = 200;
export const CLI_MAX_BYTES = 2 * 1024 * 1024;
const CLI_MAX_OUTPUT_BYTES = 256 * 1024;

export function appendCliTranscript(entries: CliTranscriptEntry[], entry: CliTranscriptEntry): CliTranscriptEntry[] {
  const encoder = new TextEncoder();
  // Pretty-printed nested RESP can be much larger than the bounded IPC payload.
  const bounded = {
    ...entry,
    output: encoder.encode(entry.output).byteLength > CLI_MAX_OUTPUT_BYTES
      ? "输出超过 256 KiB，已省略；请使用范围命令缩小结果。"
      : entry.output,
  };
  // Invalid oversized input may reach the error transcript without executing.
  // Do not let one entry evict the entire existing transcript.
  if (encoder.encode(bounded.command + bounded.output).byteLength > CLI_MAX_BYTES) return entries;
  const next = [...entries, bounded].slice(-CLI_MAX_ENTRIES);
  let total = 0;
  let start = next.length;
  for (let index = next.length - 1; index >= 0; index -= 1) {
    const size = encoder.encode(next[index].command + next[index].output).byteLength;
    if (total + size > CLI_MAX_BYTES) break;
    total += size;
    start = index;
  }
  return next.slice(start);
}

export function cliError(reason: unknown): string {
  const code = typeof reason === "object" && reason !== null && "code" in reason ? String(reason.code) : "IPC_ERROR";
  const messages: Record<string, string> = {
    UNSUPPORTED_FEATURE: "CLI 不支持订阅、MONITOR 或改变回复模式的命令；订阅与监控请使用运维观察。",
    INVALID_INPUT: "命令必须是单行、引号完整，且不超过 16 KiB。",
    CONNECTION_FAILED: "CLI 连接已失效，请重新连接 CLI。",
    COMMAND_FAILED: "Redis 命令执行失败，请检查语法、权限和事务状态。",
    COMMAND_TIMEOUT: "命令超过 5 秒，CLI 会话已关闭；命令可能已在服务器执行，请核实后再操作。",
    OPERATION_CANCELLED: "CLI 会话已关闭。",
  };
  return Object.hasOwn(messages, code)
    ? `${code}：${messages[code]}`
    : "IPC_ERROR：CLI 调用失败，请重新连接。";
}
