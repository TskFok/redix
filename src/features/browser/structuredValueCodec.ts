import { Buffer } from "buffer";
import JSONBigInt from "json-bigint";
import { decompressBlock } from "lz4js";
import { addExtension, C1, Unpackr } from "msgpackr";
import { Reader } from "protobufjs/minimal";
import { decodePhpSerialized } from "./phpSerialized";

export type StructuredValueFormat = "msgpack" | "protobuf" | "php";
export const MAX_STRUCTURED_BYTES = 4 * 1024 * 1024;
export const MAX_STRUCTURED_DEPTH = 32;
export const MAX_STRUCTURED_NODES = 20_000;
export const MAX_STRUCTURED_TEXT_BYTES = 8 * 1024 * 1024;
const LZ4 = Symbol("MessagePack-CSharp LZ4");
interface Lz4Value { [LZ4]: true; size: number; bytes?: Uint8Array }
const fail = (): never => { throw new Error("数据无效或超过解码限额"); };
const validSize = (size: unknown): size is number => typeof size === "number" && Number.isSafeInteger(size) && size > 0 && size <= MAX_STRUCTURED_BYTES;
function inlineLz4(data: Uint8Array): Lz4Value {
  if (data.length < 5) return fail();
  const size = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0, false);
  if (!validSize(size)) return fail();
  return { [LZ4]: true, size, bytes: data.subarray(4) };
}
addExtension({ type: 99, unpack: inlineLz4 });
addExtension({ type: 98, unpack(data: Uint8Array) {
  // Size metadata is itself MessagePack; the library handles its integer encoding.
  if (data.length <= 5) {
    try { const size: unknown = new Unpackr().unpack(data); if (validSize(size)) return { [LZ4]: true, size }; }
    catch { /* An inline block is checked below. */ }
  }
  return inlineLz4(data);
} });
const isLz4 = (value: unknown): value is Lz4Value => typeof value === "object" && value !== null && LZ4 in value;

function decodeProtobuf(bytes: Uint8Array): unknown {
  const budgetExceeded = new Error("Protobuf 超过解码限额");
  let fields = 0;
  const parse = (input: Uint8Array, depth: number): unknown[] => {
    if (depth > MAX_STRUCTURED_DEPTH) throw budgetExceeded;
    const reader = Reader.create(input);
    const result: unknown[] = [];
    while (reader.pos < reader.len) {
      if (++fields > MAX_STRUCTURED_NODES) throw budgetExceeded;
      // Read through the library's 64-bit primitive before narrowing tags/lengths.
      // uint32() would silently drop high bits for malformed tags or wide values.
      const tag = BigInt(reader.uint64().toString());
      if (tag > 0xffff_ffffn || tag >> 3n === 0n) return fail();
      const field = String(tag >> 3n);
      const wire = Number(tag & 7n);
      let value: unknown;
      if (wire === 0) value = BigInt(reader.uint64().toString());
      else if (wire === 1) value = { $fixed64: BigInt(reader.fixed64().toString()) };
      else if (wire === 5) value = { $fixed32: reader.fixed32() };
      else if (wire === 2) {
        const size = BigInt(reader.uint64().toString());
        if (size > BigInt(reader.len - reader.pos)) return fail();
        const payload = reader.buf.subarray(reader.pos, reader.pos + Number(size));
        reader.pos += Number(size);
        let text: string | undefined;
        try { const candidate = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(payload); if (!/[\u0000-\u001f\u007f]/.test(candidate)) text = candidate; }
        catch { /* Binary content retains its raw bytes below. */ }
        if (text !== undefined) value = text;
        else {
          try { value = parse(payload, depth + 1); }
          catch (reason) { if (reason === budgetExceeded) throw reason; value = { $binary: Buffer.from(payload).toString("base64") }; }
        }
      } else {
        // Legacy group wire types require a schema-aware view; don't guess them.
        return fail();
      }
      result.push({ [field]: value });
    }
    return result;
  };
  return parse(bytes, 0);
}

function validateLz4Block(bytes: Uint8Array, expected: number): void {
  // lz4js does not validate back-references. Validate the block envelope before
  // delegating decompression, so invalid offsets cannot silently invent zeroes.
  let source = 0;
  let written = 0;
  function length(initial: number): number {
    let result = initial;
    if (initial === 15) {
      let next: number;
      do {
        if (source >= bytes.length) return fail();
        next = bytes[source++]; result += next;
        if (result > expected) return fail();
      } while (next === 255);
    }
    return result;
  }
  while (source < bytes.length) {
    const token = bytes[source++];
    const literals = length(token >>> 4);
    if (source + literals > bytes.length || written + literals > expected) return fail();
    source += literals; written += literals;
    if (source === bytes.length) break;
    if (source + 2 > bytes.length) return fail();
    const offset = bytes[source] | (bytes[source + 1] << 8); source += 2;
    if (offset === 0 || offset > written) return fail();
    const matches = length(token & 15) + 4;
    if (written + matches > expected) return fail();
    written += matches;
  }
  if (written !== expected) return fail();
}

export function decodeStructuredValue(bytes: Uint8Array, format: StructuredValueFormat): string {
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_STRUCTURED_BYTES) return fail();
  const unpacker = new Unpackr({ mapsAsObjects: false, useRecords: false, int64AsType: "bigint" });
  let nodes = 0;
  let expandedBytes = 0;
  const expand = (compressed: Uint8Array, size: number, depth: number): unknown => {
    expandedBytes += size;
    if (!validSize(size) || expandedBytes > MAX_STRUCTURED_BYTES) return fail();
    validateLz4Block(compressed, size);
    const destination = new Uint8Array(size);
    const actual = decompressBlock(compressed, destination, 0, compressed.length, 0);
    if (actual !== size) return fail();
    return normalize(unpacker.unpack(destination), depth + 1);
  };
  const normalize = (value: unknown, depth: number): unknown => {
    if (++nodes > MAX_STRUCTURED_NODES || depth > MAX_STRUCTURED_DEPTH) return fail();
    if (value === C1) return fail();
    if (isLz4(value)) { if (!value.bytes) return fail(); return expand(value.bytes, value.size, depth); }
    if (Array.isArray(value)) {
      if (value.length === 2 && isLz4(value[0]) && value[1] instanceof Uint8Array) return expand(value[1], value[0].size, depth);
      if (value.length > MAX_STRUCTURED_NODES - nodes) return fail();
      return value.map((item) => normalize(item, depth + 1));
    }
    if (value instanceof Uint8Array) return { $binary: Buffer.from(value).toString("base64") };
    if (value instanceof Date) return { $date: value.toISOString() };
    if (value instanceof Map) {
      if (value.size > MAX_STRUCTURED_NODES - nodes) return fail();
      if ([...value.keys()].every((key) => typeof key === "string")) {
        const result: Record<string, unknown> = Object.create(null);
        for (const [key, child] of value) result[key] = normalize(child, depth + 1);
        return result;
      }
      return { $map: [...value].map(([key, child]) => [normalize(key, depth + 1), normalize(child, depth + 1)]) };
    }
    if (value && typeof value === "object") {
      const keys = Object.keys(value);
      if (keys.length > MAX_STRUCTURED_NODES - nodes) return fail();
      const result: Record<string, unknown> = Object.create(null);
      for (const key of keys) result[key] = normalize((value as Record<string, unknown>)[key], depth + 1);
      return result;
    }
    if (typeof value === "number" && !Number.isFinite(value)) return { $number: String(value) };
    if (["function", "symbol", "undefined"].includes(typeof value)) return fail();
    return value;
  };
  const result = normalize(format === "msgpack" ? unpacker.unpack(bytes) : format === "php" ? decodePhpSerialized(bytes, MAX_STRUCTURED_DEPTH, MAX_STRUCTURED_NODES) : decodeProtobuf(bytes), 0);
  const text = JSONBigInt({ useNativeBigInt: true }).stringify(result, null, 2);
  if (new TextEncoder().encode(text).length > MAX_STRUCTURED_TEXT_BYTES) return fail();
  return text;
}
