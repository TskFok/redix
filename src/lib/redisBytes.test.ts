import { describe, expect, it } from "vitest";
import { byteIdentity, byteLength, bytesFromInput, bytesToInput, bytesToSingleLineInput, bytesToMultilineInput, displayBytes, displayKey, keyFromBytes, keyFromText, keyToBytes } from "./redisBytes";

describe("Redis 原始字节", () => {
  it("所有256字节在Hex/Base64间无损转换，UTF8拒绝有损解码", () => {
    const hex = Array.from({ length: 256 }, (_, index) => index.toString(16).padStart(2, "0")).join(" ");
    const bytes = bytesFromInput(hex, "hex");
    expect(byteLength(bytes)).toBe(256);
    expect(bytesToInput(bytes, "hex")).toBe(hex);
    expect(bytesFromInput(bytesToInput(bytes, "base64"), "base64")).toEqual(bytes);
    expect(() => bytesToInput(bytes, "utf8")).toThrow();
  });
  it("保留BOM、NUL、空值和纯空格，不默默修复非法输入", () => {
    for (const text of ["", " ", "\ufeffhello", "a\0b"]) {
      expect(bytesToInput(bytesFromInput(text, "utf8"), "utf8")).toBe(text);
    }
    expect(bytesFromInput("", "hex")).toBe("");
    expect(bytesFromInput("", "base64")).toBe("");
    for (const hex of ["f", "gg", "0x00", "00-ff"]) expect(() => bytesFromInput(hex, "hex")).toThrow();
    for (const base64 of ["/w", "/x==", " /w==", "a===", "===="]) expect(() => bytesFromInput(base64, "base64")).toThrow();
    expect(() => bytesFromInput("\ud800", "utf8")).toThrow();
  });
  it("文本控件只接受能无损编辑的换行形式", () => {
    for (const text of ["a\rb", "a\r\nb"]) {
      expect(() => bytesToMultilineInput(text, "utf8")).toThrow("回车");
      expect(() => bytesToSingleLineInput(text, "utf8")).toThrow("换行");
      expect(bytesToMultilineInput(text, "base64")).toBe(bytesToInput(text, "base64"));
    }
    expect(() => bytesToSingleLineInput("a\nb", "utf8")).toThrow("换行");
    expect(bytesToMultilineInput("a\nb", "utf8")).toBe("a\nb");
    expect(bytesToSingleLineInput("a\nb", "hex")).toBe("61 0a 62");
  });
  it("字节身份无碰撞，字面标记不会误识别为二进制键", () => {
    const binary = { base64: "/wA=" };
    const id = keyFromBytes(binary);
    const literal = keyFromText(id);
    expect(literal).not.toBe(id);
    expect(keyToBytes(id)).toEqual(binary);
    expect(keyToBytes(literal)).toBe(id);
    expect(keyFromBytes("user:1")).toBe("user:1");
    expect(keyFromBytes({ base64: "dXNlcjox" })).toBe("user:1");
    expect(byteIdentity("a")).toBe(byteIdentity({ base64: "YQ==" }));
    expect(keyToBytes(keyFromBytes(""))).toBe("");
    expect(displayKey(id)).toBe("Hex: ff 00");
    expect(displayKey(literal)).not.toBe(displayKey(id));
    expect(displayBytes("\0")).toBe("Hex: 00");
    expect(displayKey("")).toBe("（空键）");
    expect(displayKey(" ")).toBe("Hex: 20");
    expect(displayBytes({ base64: "IA==" })).toBe("Hex: 20");
    expect(displayKey("  ")).toBe("Hex: 20 20");
  });
});
