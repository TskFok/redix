import { describe, expect, it } from "vitest";
import { decodeStructuredValue } from "./structuredValueCodec";

const decode = (text: string) => decodeStructuredValue(new TextEncoder().encode(text), "php");
const decodeBytes = (text: string) => decodeStructuredValue(Uint8Array.from(text, (character) => character.charCodeAt(0)), "php");
describe("PHP 序列化值只读解码", () => {
  it("保留字符串及属性名中的 UTF-8 BOM 字节", () => {
    expect(JSON.parse(decode('s:4:"\uFEFFx";'))).toBe("\uFEFFx");
    expect(JSON.parse(decode('a:2:{s:4:"\uFEFFx";i:1;s:1:"x";i:2;}')))
      .toEqual({ "\uFEFFx": 1, x: 2 });
  });
  it("二进制数组键保留为带类型 Map 键", () => {
    // Matches PHP serialize([chr(255) => "x"]), rather than a UTF-8 replacement character.
    expect(JSON.parse(decodeBytes('a:1:{s:1:"\xff";s:1:"x";}')))
      .toEqual({ $map: [[{ $binary: "/w==" }, "x"]] });
  });
  it("二进制对象属性名保留为 Map，不转换对象属性或展开引用", () => {
    expect(JSON.parse(decodeBytes('O:4:"User":1:{s:1:"\xff";R:1;}')))
      .toEqual({ $class: "User", $properties: { $map: [[{ $binary: "/w==" }, { $reference: 1, $kind: "value" }]] } });
  });
  it("重复二进制键按字节拒绝，不混淆不同二进制键或普通字符串", () => {
    expect(JSON.parse(decodeBytes('a:3:{s:1:"\xff";i:1;s:1:"\xfe";i:2;s:4:"/w==";i:3;}')))
      .toEqual({ $map: [[{ $binary: "/w==" }, 1], [{ $binary: "/g==" }, 2], ["/w==", 3]] });
    for (const prefix of ['a:2:{', 'O:4:"User":2:{']) {
      expect(() => decodeBytes(prefix + 's:1:"\xff";i:1;s:1:"\xff";i:2;}')).toThrow();
    }
  });
  it("使用字节长度读取 Unicode 字符串和混合类型数组，保留64位整数", () => {
    expect(decode('a:4:{i:0;s:6:"中文";i:1;i:9223372036854775807;i:2;b:1;i:3;N;}'))
      .toContain("9223372036854775807");
    expect(JSON.parse(decode('a:2:{i:0;s:6:"中文";i:1;b:0;}'))).toEqual(["中文", false]);
  });
  it("对象、引用与自定义序列化内容仅展示为数据，不实例化类", () => {
    expect(JSON.parse(decode('O:4:"User":1:{s:4:"name";s:3:"Ada";}')))
      .toEqual({ $class: "User", $properties: { name: "Ada" } });
    expect(JSON.parse(decode('a:1:{i:0;R:1;}'))).toEqual([{ $reference: 1, $kind: "value" }]);
    expect(JSON.parse(decode('C:4:"Demo":3:{abc}'))).toEqual({ $class: "Demo", $serialized: "abc" });
  });
  it("保留PHP映射键类型和原型敏感属性", () => {
    expect(JSON.parse(decode('a:2:{s:9:"__proto__";s:1:"x";s:11:"constructor";s:1:"y";}')))
      .toEqual(Object.fromEntries([["__proto__", "x"], ["constructor", "y"]]));
    expect(JSON.parse(decode('a:2:{i:8;s:1:"x";s:1:"8";s:1:"y";}')))
      .toEqual({ $map: [[8, "x"], ["8", "y"]] });
  });
  it("拒绝截断、多余内容、错误长度和声明超大集合", () => {
    for (const text of ['s:2:"中";', 'a:10000000:{}', 'i:1;N;', 'O:4:"User":1:{}', 'R:0;', 's:4:"abc";']) {
      expect(() => decode(text)).toThrow();
    }
  });
  it("在分配之前拒绝深层和大量节点，不执行类或加载外部引用", () => {
    expect(() => decode('a:1:{i:0;'.repeat(34) + 'N;' + '}'.repeat(34))).toThrow();
    expect(() => decode('S:4:"data";')).toThrow();
  });
});
