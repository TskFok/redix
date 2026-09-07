import { invoke } from "@tauri-apps/api/core";
import { isStructuredFormat, type ValueFormat } from "./codecFormats";
export type { BasicValueFormat, ValueFormat } from "./codecFormats";
export type ValueCompression = "none" | "gzip" | "zlib" | "deflate";
export interface StringValue { base64: string; total_bytes: number; ttl_ms: number; truncated: boolean }
export interface StringValueTarget { connection_id: string; key: string }
export interface DecodedStringValue { text: string; byte_length: number }
export interface StringValueSaved { byte_length: number; ttl_ms: number }
export const getStringValue = (input: StringValueTarget) => invoke<StringValue>("get_string_value", { input });
export const setStringValue = (input: StringValueTarget & { base64: string }) => invoke<StringValueSaved>("set_string_value", { input });
let activeDecoders = 0;
const MAX_DECODERS = 2;
const MAX_BYTES = 4 * 1024 * 1024;
const DECODE_TIMEOUT_MS = 2000;
const invalidCodec = () => ({ code: "INVALID_INPUT", message: "数据无效或超过解码限额" });

export async function decodeStringValue(input: { base64: string; format: ValueFormat; compression: ValueCompression }): Promise<DecodedStringValue> {
  if (!isStructuredFormat(input.format)) return invoke<DecodedStringValue>("decode_string_value", { input });
  const raw = await invoke<DecodedStringValue>("decode_string_value", { input: { ...input, format: "base64" } });
  if (raw.text.length > Math.ceil(MAX_BYTES / 3) * 4 || raw.byte_length > MAX_BYTES || activeDecoders >= MAX_DECODERS) throw invalidCodec();
  const decoded = atob(raw.text);
  const bytes = Uint8Array.from(decoded, (character) => character.charCodeAt(0));
  if (bytes.length > MAX_BYTES) throw invalidCodec();
  const worker = new Worker(new URL("./valueCodec.worker.ts", import.meta.url), { type: "module" });
  activeDecoders += 1;
  return new Promise((resolve, reject) => {
    let finished = false;
    const finish = (text?: string) => {
      if (finished) return;
      finished = true; clearTimeout(timeout); worker.terminate(); activeDecoders -= 1;
      if (typeof text === "string" && text.length <= 8 * 1024 * 1024) resolve({ text, byte_length: raw.byte_length });
      else reject(invalidCodec());
    };
    const timeout = setTimeout(() => finish(), DECODE_TIMEOUT_MS);
    worker.onmessage = (event: MessageEvent<{ ok: boolean; text?: string }>) => finish(event.data.ok ? event.data.text : undefined);
    worker.onerror = () => finish();
    worker.onmessageerror = () => finish();
    try { worker.postMessage({ bytes, format: input.format }, [bytes.buffer]); }
    catch { finish(); }
  });
}
export function encodeStringValue(input: { text: string; format: ValueFormat; compression: ValueCompression }): Promise<string> {
  if (isStructuredFormat(input.format)) return Promise.reject(invalidCodec());
  return invoke<string>("encode_string_value", { input });
}
