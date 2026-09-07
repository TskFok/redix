import { Buffer } from "buffer";

// A structural reader only: object classes and references remain tagged data.
// No PHP runtime, class constructors, callbacks, or reference hydration is used.
export function decodePhpSerialized(bytes: Uint8Array, maxDepth: number, maxNodes: number): unknown {
  let offset = 0, nodes = 0;
  const invalid = (): never => { throw new Error("PHP 数据无效或超过解码限额"); };
  const expect = (char: string) => { if (bytes[offset++] !== char.charCodeAt(0)) invalid(); };
  const token = (end: string): string => {
    const start = offset;
    while (offset < bytes.length && bytes[offset] !== end.charCodeAt(0)) {
      if (offset - start >= 256 || bytes[offset] > 127) return invalid();
      offset++;
    }
    if (offset >= bytes.length) return invalid();
    const result = new TextDecoder().decode(bytes.subarray(start, offset));
    offset++;
    return result;
  };
  const count = (end: string, limit: number): number => {
    const value = token(end);
    if (!/^\d{1,10}$/.test(value)) return invalid();
    const size = Number(value);
    if (!Number.isSafeInteger(size) || size > limit) return invalid();
    return size;
  };
  const payload = (length: number): string | { $binary: string } => {
    if (length > bytes.length - offset) return invalid();
    const value = bytes.subarray(offset, offset + length); offset += length;
    try { return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(value); }
    catch { return { $binary: Buffer.from(value).toString("base64") }; }
  };
  const string = () => {
    const length = count(":", bytes.length); expect('"');
    const result = payload(length); expect('"');
    return result;
  };
  const parse = (depth: number): unknown => {
    if (++nodes > maxNodes || depth > maxDepth || offset >= bytes.length) return invalid();
    const type = String.fromCharCode(bytes[offset++]);
    if (type === "N") { expect(";"); return null; }
    expect(":");
    if (type === "s" || type === "E") {
      const result = string(); expect(";");
      return type === "E" ? { $enum: result } : result;
    }
    if (type === "b") {
      const value = token(";");
      if (value !== "0" && value !== "1") return invalid();
      return value === "1";
    }
    if (type === "i") {
      const value = token(";");
      if (!/^-?\d{1,19}$/.test(value)) return invalid();
      const integer = BigInt(value);
      if (integer < -9223372036854775808n || integer > 9223372036854775807n) return invalid();
      return integer >= BigInt(Number.MIN_SAFE_INTEGER) && integer <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(integer) : integer;
    }
    if (type === "d") {
      const value = token(";");
      if (value === "NAN") return NaN;
      if (value === "INF") return Infinity;
      if (value === "-INF") return -Infinity;
      if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(value) || !Number.isFinite(Number(value))) return invalid();
      return Number(value);
    }
    if (type === "r" || type === "R") {
      const reference = count(";", maxNodes);
      if (reference === 0) return invalid();
      return { $reference: reference, $kind: type === "r" ? "object" : "value" };
    }
    if (type === "C") {
      const className = string(); expect(":");
      if (typeof className !== "string") return invalid();
      const length = count(":", bytes.length); expect("{");
      const serialized = payload(length); expect("}");
      return { $class: className, $serialized: serialized };
    }
    if (type === "a" || type === "O") {
      const className = type === "O" ? string() : null;
      if (type === "O") { if (typeof className !== "string") return invalid(); expect(":"); }
      const length = count(":", Math.floor((maxNodes - nodes) / 2)); expect("{");
      const entries = new Map<unknown, unknown>();
      const binaryKeys = new Set<string>();
      let sequential = type === "a";
      for (let index = 0; index < length; index++) {
        const key = parse(depth + 1);
        const binaryKey = typeof key === "object" && key !== null && Object.hasOwn(key, "$binary")
          && typeof (key as { $binary: unknown }).$binary === "string" ? (key as { $binary: string }).$binary : null;
        if (binaryKey === null && (!["string", "number", "bigint"].includes(typeof key) || (type === "O" && typeof key !== "string"))) return invalid();
        // Every binary string has a new wrapper object. Compare its canonical bytes,
        // while keeping the wrapper as a Map key for lossless, read-only display.
        if (entries.has(key) || (binaryKey !== null && binaryKeys.has(binaryKey))) return invalid();
        if (binaryKey !== null) binaryKeys.add(binaryKey);
        sequential &&= key === index;
        entries.set(key, parse(depth + 1));
      }
      expect("}");
      if (type === "O") return { $class: className, $properties: entries };
      return sequential ? [...entries.values()] : entries;
    }
    return invalid();
  };
  const result = parse(0);
  if (offset !== bytes.length) return invalid();
  return result;
}
