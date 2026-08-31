import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../../lib/types";

export interface CliSessionInput { connection_id: string; session_id: string }
export interface CliCommandInput extends CliSessionInput { command: string }
export interface CliReply {
  result: CommandResult | null;
  error_code: string | null;
  session_closed: boolean;
  truncated: boolean;
}
export function openCliSession(input: CliSessionInput): Promise<void> { return invoke("open_cli_session", { input }); }
export function executeCliCommand(input: CliCommandInput): Promise<CliReply> { return invoke("execute_cli_command", { input }); }
export function closeCliSession(input: CliSessionInput): Promise<void> { return invoke("close_cli_session", { input }); }
