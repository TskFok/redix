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

  it("拦截 RDI、Copilot、遥测分析与远程插件的精确入口或依赖", () => {
    expect(
      scanFiles([
        "src/rdi/pipeline; Redis Data Integration",
        "Redis Copilot; copilot/service",
        '"@segment/analytics-next"; "@sentry/react"',
        "analytics/send-event; telemetry/event",
        "remote-plugin/runtime; plugin-marketplace/client",
      ]),
    ).toEqual([
      "rdi entry",
      "redis data integration",
      "copilot entry",
      "segment analytics dependency",
      "sentry telemetry dependency",
      "analytics event endpoint",
      "telemetry entry",
      "remote plugin entry",
      "plugin marketplace entry",
    ]);
  });

  it("不误伤通用 AI、数据库分析、统计值或本地内建模块", () => {
    expect(
      scanFiles([
        "AI value decoder",
        "database analysis and primary statistics",
        "grid pipeline and cardinality",
        "local built-in plugin metadata",
        "analytics summary shown from Redis INFO",
      ]),
    ).toEqual([]);
  });
});
