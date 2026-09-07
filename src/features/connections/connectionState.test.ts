import { expect, it } from "vitest";
import { parseSeedNodes } from "./connectionState";

it("拒绝后端不接受的选项式主机名", () => {
  expect(parseSeedNodes("-host:7000")).toBeNull();
});

it.each([
  "",
  "host:0",
  "host:65536",
  "[garbage]:7000",
  "[1:2]:7000",
  "http://host:7000",
  "a:7000\nA:7000",
  "[::1]:7000\n[0:0:0:0:0:0:0:1]:7000",
  Array.from({ length: 33 }, (_, i) => `node-${i}:7000`).join("\n"),
])("拒绝无效或重复 Cluster seeds %s", (seeds) => {
  expect(parseSeedNodes(seeds)).toBeNull();
});
it("接受 32 个唯一种子及 IPv6 首节点", () => {
  const nodes = parseSeedNodes(
    [
      "[::1]:7000",
      ...Array.from({ length: 31 }, (_, i) => `node-${i}:7000`),
    ].join("\n"),
  );
  expect(nodes).toHaveLength(32);
  expect(nodes?.[0]).toEqual({ host: "::1", port: 7000 });
});
