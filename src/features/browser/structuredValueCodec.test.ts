import { describe, expect, it } from "vitest";
import { decodeStructuredValue, MAX_STRUCTURED_BYTES, MAX_STRUCTURED_DEPTH, MAX_STRUCTURED_NODES } from "./structuredValueCodec";

describe("有界本地协议解码", () => {
  it("MessagePack保留64位整数、嵌套结构及二进制内容", () => {
    expect(JSON.parse(decodeStructuredValue(Uint8Array.from([0x82, 0xa1, 0x61, 0x92, 1, 2, 0xa1, 0x62, 0xc4, 2, 0, 255]), "msgpack"))).toEqual({ a: [1, 2], b: { $binary: "AP8=" } });
    expect(decodeStructuredValue(Uint8Array.from([0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), "msgpack")).toBe("18446744073709551615");
  });
  it("MessagePack-CSharp LZ4支持内联块和块数组", () => {
    const inline = Uint8Array.from([0xc7, 8, 99, 0, 0, 0, 3, 0x30, 0x92, 1, 2]);
    const array = Uint8Array.from([0x92, 0xd4, 98, 3, 0xc4, 4, 0x30, 0x92, 1, 2]);
    expect(JSON.parse(decodeStructuredValue(inline, "msgpack"))).toEqual([1, 2]);
    expect(JSON.parse(decodeStructuredValue(array, "msgpack"))).toEqual([1, 2]);
    // Literal a6 61, then five copies at offset 1: MessagePack string "aaaaaa".
    const withBackReference = Uint8Array.from([0xc7, 9, 99, 0, 0, 0, 7, 0x21, 0xa6, 0x61, 1, 0]);
    expect(JSON.parse(decodeStructuredValue(withBackReference, "msgpack"))).toBe("aaaaaa");
  });
  it("Protobuf按字段号展示无schema解析结果", () => {
    const result = JSON.parse(decodeStructuredValue(Uint8Array.from([8, 150, 1, 18, 3, 102, 111, 111]), "protobuf"));
    expect(result).toEqual([{ "1": 150 }, { "2": "foo" }]);
  });
  it("Protobuf字符串中的 UTF-8 BOM 保留为实际数据", () => {
    const bytes = Uint8Array.from([0x12, 4, 0xef, 0xbb, 0xbf, 0x78]);
    expect(JSON.parse(decodeStructuredValue(bytes, "protobuf"))).toEqual([{ "2": "\uFEFFx" }]);
  });
  it("Protobuf完整保留64位varint，不截断为32位值", () => {
    const large = Uint8Array.from([8, 0x81, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x10]);
    const maximum = Uint8Array.from([8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1]);
    expect(decodeStructuredValue(large, "protobuf")).toContain("9007199254740993");
    expect(decodeStructuredValue(maximum, "protobuf")).toContain("18446744073709551615");
    expect(decodeStructuredValue(Uint8Array.from([0x12, large.length, ...large]), "protobuf")).toContain("9007199254740993");
    expect(() => decodeStructuredValue(Uint8Array.from([0]), "protobuf")).toThrow();
    expect(() => decodeStructuredValue(Uint8Array.from([0x0b, 0x0c]), "protobuf")).toThrow();
  });
  it("损坏输入、深层值与过多节点拒绝输出", () => {
    expect(() => decodeStructuredValue(Uint8Array.from([0xc1]), "msgpack")).toThrow();
    expect(() => decodeStructuredValue(Uint8Array.from([8, 128]), "protobuf")).toThrow();
    expect(() => decodeStructuredValue(new Uint8Array(MAX_STRUCTURED_BYTES + 1), "msgpack")).toThrow();
    expect(() => decodeStructuredValue(Uint8Array.from([...Array(MAX_STRUCTURED_DEPTH + 2).fill(0x91), 0]), "msgpack")).toThrow();
    const count = MAX_STRUCTURED_NODES + 1;
    expect(() => decodeStructuredValue(Uint8Array.from([0xdc, count >> 8, count & 255, ...Array(count).fill(0)]), "msgpack")).toThrow();
  });
  it("LZ4拒绝超额声明与错误长度，不静默展示截断内容", () => {
    expect(() => decodeStructuredValue(Uint8Array.from([0xc7, 8, 99, 127, 255, 255, 255, 0x30, 0x92, 1, 2]), "msgpack")).toThrow();
    expect(() => decodeStructuredValue(Uint8Array.from([0xc7, 8, 99, 0, 0, 0, 8, 0x30, 0x92, 1, 2]), "msgpack")).toThrow();
  });
  it("LZ4损坏回溯不得产生伪造的零字节", () => {
    const outOfRange = Uint8Array.from([0xc7, 8, 99, 0, 0, 0, 5, 0x10, 0x94, 0xff, 0x7f]);
    const zeroOffset = Uint8Array.from([0xc7, 8, 99, 0, 0, 0, 5, 0x10, 0x94, 0, 0]);
    expect(() => decodeStructuredValue(outOfRange, "msgpack")).toThrow();
    expect(() => decodeStructuredValue(zeroOffset, "msgpack")).toThrow();
  });
});
