import { Buffer } from "buffer";

/** IPC values stay compatible with text; the object form contains exact bytes. */
export type RedisBytes = string | { base64: string };
export type ByteFormat = "utf8" | "hex" | "base64";
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const encoder = new TextEncoder();
const KEY_PREFIX = "\0redix:bytes:";

function utf8(text: string): Uint8Array {
  const bytes = encoder.encode(text);
  if (decoder.decode(bytes) !== text) throw new Error("UTF-8 文本包含无效 Unicode 字符。");
  return bytes;
}

export function isRedisBytes(value: unknown): value is RedisBytes {
  if (typeof value === "string") return true;
  return typeof value === "object" && value !== null && Object.keys(value).length === 1
    && "base64" in value && typeof value.base64 === "string";
}

export function rawBytes(value: RedisBytes): Uint8Array {
  if (typeof value === "string") return utf8(value);
  if (!isRedisBytes(value) || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value.base64)) {
    throw new Error("Base64 格式无效，请使用标准编码并保留末尾的 =。");
  }
  const bytes = Buffer.from(value.base64, "base64");
  if (bytes.toString("base64") !== value.base64) throw new Error("Base64 编码不是规范格式。");
  return bytes;
}

function fromRaw(bytes: Uint8Array): RedisBytes {
  try { return decoder.decode(bytes); }
  catch { return { base64: Buffer.from(bytes).toString("base64") }; }
}

export function bytesFromInput(text: string, format: ByteFormat): RedisBytes {
  if (format === "utf8") { utf8(text); return text; }
  if (format === "base64") return fromRaw(rawBytes({ base64: text }));
  // Permit whitespace between whole bytes, never inside one byte.
  if (!/^(?:\s*[\da-fA-F]{2})*\s*$/.test(text)) throw new Error("Hex 必须由完整的两位十六进制字节组成。");
  return fromRaw(Buffer.from(text.replace(/\s/g, ""), "hex"));
}

export function bytesToInput(value: RedisBytes, format: ByteFormat): string {
  const bytes = rawBytes(value);
  if (format === "base64") return Buffer.from(bytes).toString("base64");
  if (format === "hex") return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join(" ");
  try { return decoder.decode(bytes); }
  catch { throw new Error("这些字节不是有效 UTF-8，请使用 Hex 或 Base64。"); }
}

/** Native input elements strip CR/LF; keep those bytes in an encoded view. */
export function bytesToSingleLineInput(value: RedisBytes, format: ByteFormat): string {
  const text = bytesToInput(value, format);
  if (format === "utf8" && /[\r\n]/.test(text)) {
    throw new Error("这些字节包含换行，单行输入请使用 Hex 或 Base64。");
  }
  return text;
}

/** Textareas preserve LF, but normalize CR and CRLF on the next edit. */
export function bytesToMultilineInput(value: RedisBytes, format: ByteFormat): string {
  const text = bytesToInput(value, format);
  if (format === "utf8" && text.includes("\r")) {
    throw new Error("这些字节包含回车，文本输入会改变换行字节，请使用 Hex 或 Base64。");
  }
  return text;
}

export function byteLength(value: RedisBytes): number { return rawBytes(value).length; }
export function byteIdentity(value: RedisBytes): string { return bytesToInput(value, "base64"); }
export function isBinary(value: RedisBytes): boolean {
  try {
    const text = typeof value === "string" ? value : bytesToInput(value, "utf8");
    return /[\u0000-\u001f\u007f]/.test(text) || (text.length > 0 && text.trim().length === 0);
  } catch { return true; }
}
export function displayBytes(value: RedisBytes): string {
  return isBinary(value) ? `Hex: ${bytesToInput(value, "hex")}` : typeof value === "string" ? value : bytesToInput(value, "utf8");
}

/** Browser state uses stable primitive identities, not labels or object references.
 * Text that starts with the reserved prefix is escaped too, preventing collisions.
 * Never persist these identities or construct one from user input directly.
 */
export function keyFromBytes(value: RedisBytes): string {
  const normalized = fromRaw(rawBytes(value));
  return typeof normalized === "string" && !normalized.startsWith(KEY_PREFIX)
    ? normalized : KEY_PREFIX + byteIdentity(value);
}
export function keyFromText(text: string): string { return keyFromBytes(text); }
export function keyToBytes(key: string): RedisBytes {
  return key.startsWith(KEY_PREFIX) ? bytesFromInput(key.slice(KEY_PREFIX.length), "base64") : key;
}
export function displayKey(key: string): string { return displayBytes(keyToBytes(key)) || "（空键）"; }
