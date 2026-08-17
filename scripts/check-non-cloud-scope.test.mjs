import { describe, expect, it } from "vitest";
import { scanFiles } from "./check-non-cloud-scope.mjs";

describe("非 Cloud 范围检查", () => {
  it("没有排除功能入口时通过", () => {
    expect(
      scanFiles(["src/App.tsx", "src-tauri/src/lib.rs"].map(() => "local redis")),
    ).toEqual([]);
  });

  it("按出现顺序返回所有排除入口并去重", () => {
    expect(
      scanFiles([
        "Redis Cloud login",
        "redis-cloud; cloud-capi; Azure Managed Redis",
        "cloud login; cloud SDK; cloud API",
        "cloud endpoint; cloud account; cloud database discovery",
        "cloud api; REDIS CLOUD",
      ]),
    ).toEqual([
      "redis cloud",
      "cloud login",
      "redis-cloud",
      "cloud-capi",
      "azure managed redis",
      "cloud sdk",
      "cloud api",
      "cloud endpoint",
      "cloud account",
      "cloud database discovery",
    ]);
  });
});
