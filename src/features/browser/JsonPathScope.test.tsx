import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import KeyDetails from "./KeyDetails";
import type { KeyValue } from "../../lib/types";
import type { ModuleProbeState } from "./browserState";

const { setJsonPath, getBrowserKey } = vi.hoisted(() => ({
  setJsonPath: vi.fn(),
  getBrowserKey: vi.fn(),
}));
vi.mock("../../lib/tauri", () => ({ setJsonPath, getBrowserKey }));

const value = { name: "Alice", tags: ["redis"] };
const detail: KeyValue = {
  key: "profile:1",
  key_type: "ReJSON-RL",
  ttl_ms: 5000,
  value: { Json: { value } },
};
const probe = (version = "20611"): ModuleProbeState => ({
  status: "ready",
  capabilities: {
    modules: [],
    json_supported: true,
    json_version: version,
    search_supported: false,
    search_version: null,
    array_supported: false,
    vector_set_supported: false,
  },
});
const callbacks = { onDetailChange: vi.fn(), onDeleted: vi.fn() };

describe("JSON路径编辑器生命周期", () => {
  afterEach(() => {
    cleanup();
    vi.resetAllMocks();
  });

  it("同键TTL或能力版本变化保留编辑器实例和草稿", async () => {
    setJsonPath.mockResolvedValue({ key: "profile:1", path: "$.name", affected: 1, new_length: null, ttl_ms: 4000 });
    getBrowserKey.mockResolvedValue(detail);
    const { rerender } = render(
      <KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    const pathInput = screen.getByLabelText("JSON Path");
    fireEvent.change(pathInput, { target: { value: "$.name" } });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), { target: { value: '"Bob"' } });
    const refreshedDetail: KeyValue = {
      ...detail,
      ttl_ms: 4000,
      value: { Json: { value: { tags: ["redis"], name: "Alice" } } },
    };
    rerender(
      <KeyDetails connectionId="local" detail={refreshedDetail} loading={false} moduleProbe={probe("20612")} {...callbacks} />,
    );
    expect(screen.getByLabelText("JSON Path")).toBe(pathInput);
    expect(pathInput).toHaveValue("$.name");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue('"Bob"');
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));
    await waitFor(() => expect(setJsonPath).toHaveBeenCalledWith({
      connection_id: "local", key: "profile:1", path: "$.name", value: "Bob",
    }));
  });

  it("切换键或连接时即使JSON值等价，也清除旧路径草稿", () => {
    const { rerender } = render(
      <KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    fireEvent.change(screen.getByLabelText("JSON Path"), { target: { value: "$.name" } });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), { target: { value: '"Bob"' } });
    rerender(
      <KeyDetails connectionId="local" detail={{ ...detail, key: "profile:2" }} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    expect(screen.getByLabelText("JSON Path")).toHaveValue("$");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue(JSON.stringify(value, null, 2));
    fireEvent.change(screen.getByLabelText("JSON Path"), { target: { value: "$.tags" } });
    rerender(
      <KeyDetails connectionId="remote" detail={{ ...detail, key: "profile:2" }} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    expect(screen.getByLabelText("JSON Path")).toHaveValue("$");
  });
});
