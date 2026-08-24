import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { analyzeDatabase } from "../../lib/tauri";
import type { DatabaseAnalysisReport } from "../../lib/types";
import DatabaseAnalysisPage from "./DatabaseAnalysisPage";

vi.mock("../../lib/tauri", () => ({
  analyzeDatabase: vi.fn(),
}));

const analyzeDatabaseMock = vi.mocked(analyzeDatabase);

const report: DatabaseAnalysisReport = {
  database: 0,
  pattern: "*",
  delimiter: ":",
  progress: { scanned: 3, processed: 3, max_keys: 100000, truncated: false },
  total_keys: {
    total: 3,
    observed: 3,
    types: [{ type: "string", total: 2 }, { type: "hash", total: 1 }],
  },
  total_memory: {
    total: 384,
    observed: 2,
    types: [{ type: "string", total: 128 }, { type: "hash", total: 256 }],
  },
  top_keys_by_length: [],
  top_keys_by_memory: [
    { key: "user:2", key_type: "hash", length: 4, memory_bytes: 256, ttl_seconds: 120 },
  ],
  top_namespaces_by_keys: [
    { namespace: "user", keys: 2, memory_bytes: 384, types: [{ type: "string", total: 1 }] },
  ],
  top_namespaces_by_memory: [],
  expiration_groups: [{ label: "No Expiry", keys: 1, memory_bytes: 128 }],
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("DatabaseAnalysisPage", () => {
  it("提交默认参数并展示分析摘要和 Top Key", async () => {
    analyzeDatabaseMock.mockResolvedValue(report);
    render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);

    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));

    await waitFor(() =>
      expect(analyzeDatabaseMock).toHaveBeenCalledWith({
        connection_id: "local",
        pattern: "*",
        delimiter: ":",
        max_keys: 100000,
      }),
    );
    expect(await screen.findByText("总键数")).toBeInTheDocument();
    expect(screen.getByText("user:2")).toBeInTheDocument();
  });

  it("显示截断提示和固定连接错误", async () => {
    analyzeDatabaseMock.mockResolvedValue({
      ...report,
      progress: { ...report.progress, truncated: true },
    });
    const { rerender } = render(
      <DatabaseAnalysisPage connectionId="local" activeDatabase={0} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    expect(
      await screen.findByText("结果已达到扫描上限，可能不完整"),
    ).toBeInTheDocument();

    analyzeDatabaseMock.mockRejectedValueOnce({ code: "CONNECTION_FAILED" });
    rerender(<DatabaseAnalysisPage connectionId="local-2" activeDatabase={0} />);
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "无法连接 Redis，请检查连接状态。",
    );
  });

  it("非法 max_keys 不调用 IPC", () => {
    render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
    fireEvent.change(screen.getByLabelText("最大扫描键数"), {
      target: { value: "999" },
    });
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    expect(analyzeDatabaseMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "扫描键数必须在 1000 到 1000000 之间。",
    );
  });

  it("连接切换后忽略旧分析响应", async () => {
    let resolveFirst!: (value: DatabaseAnalysisReport) => void;
    let resolveSecond!: (value: DatabaseAnalysisReport) => void;
    analyzeDatabaseMock
      .mockReturnValueOnce(
        new Promise<DatabaseAnalysisReport>((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockReturnValueOnce(
        new Promise<DatabaseAnalysisReport>((resolve) => {
          resolveSecond = resolve;
        }),
      );

    const { rerender } = render(
      <DatabaseAnalysisPage connectionId="old" activeDatabase={0} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    rerender(<DatabaseAnalysisPage connectionId="new" activeDatabase={1} />);
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));

    resolveSecond({ ...report, database: 1, top_keys_by_memory: [] });
    await waitFor(() => expect(screen.getByText("数据库 1")).toBeInTheDocument());
    resolveFirst({
      ...report,
      top_keys_by_memory: [
        {
          key: "old:key",
          key_type: "string",
          length: 3,
          memory_bytes: 32,
          ttl_seconds: -1,
        },
      ],
    });
    await waitFor(() => expect(screen.queryByText("old:key")).not.toBeInTheDocument());
  });

  it("新提交后忽略前一次分析响应", async () => {
    let resolveFirst!: (value: DatabaseAnalysisReport) => void;
    let resolveSecond!: (value: DatabaseAnalysisReport) => void;
    analyzeDatabaseMock
      .mockReturnValueOnce(
        new Promise<DatabaseAnalysisReport>((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockReturnValueOnce(
        new Promise<DatabaseAnalysisReport>((resolve) => {
          resolveSecond = resolve;
        }),
      );
    render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);

    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
    resolveSecond({ ...report, top_keys_by_memory: [] });
    await screen.findByText("总键数");
    resolveFirst({
      ...report,
      top_keys_by_memory: [
        {
          key: "old:submission",
          key_type: "string",
          length: 1,
          memory_bytes: 1,
          ttl_seconds: -1,
        },
      ],
    });

    await waitFor(() =>
      expect(screen.queryByText("old:submission")).not.toBeInTheDocument(),
    );
  });
});
