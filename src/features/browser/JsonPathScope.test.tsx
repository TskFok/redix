import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import KeyDetails from "./KeyDetails";
import type { KeyValue } from "../../lib/types";
import type { ModuleProbeState } from "./browserState";

const { setJsonPath, deleteJsonPath, getBrowserKey } = vi.hoisted(() => ({
  setJsonPath: vi.fn(),
  deleteJsonPath: vi.fn(),
  getBrowserKey: vi.fn(),
}));
vi.mock("../../lib/tauri", () => ({ setJsonPath, deleteJsonPath, getBrowserKey }));

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
    vi.restoreAllMocks();
  });

  it.each(["$", "."])("根路径 %s 使用应用内确认，取消不请求，确认只删除一次", async (path) => {
    const nativeConfirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    deleteJsonPath.mockResolvedValue({ key: "profile:1", path, affected: 1, new_length: null, ttl_ms: -2 });
    render(<KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />);
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: path } });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("确定删除整个 JSON 键“profile:1”吗？");
    expect(deleteJsonPath).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
    await act(async () => {});
    expect(deleteJsonPath).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(callbacks.onDeleted).toHaveBeenCalledWith("profile:1"));
    expect(deleteJsonPath).toHaveBeenCalledExactlyOnceWith({ connection_id: "local", key: "profile:1", path });
    expect(nativeConfirm).not.toHaveBeenCalled();
  });

  it.each(["连接", "键", "路径", "数据", "加载", "卸载"])("根删除确认期间切换%s会取消旧操作", async (change) => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    const props = { connectionId: "local", detail, loading: false, moduleProbe: probe(), ...callbacks };
    const view = render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    if (change === "连接") view.rerender(<KeyDetails {...props} connectionId="remote" />);
    else if (change === "键") view.rerender(<KeyDetails {...props} detail={{ ...detail, key: "profile:2" }} />);
    else if (change === "路径") fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: "$.name" } });
    else if (change === "数据") view.rerender(<KeyDetails {...props} detail={{ ...detail, value: { Json: { value: { name: "Bob" } } } }} />);
    else if (change === "加载") view.rerender(<KeyDetails {...props} loading />);
    else view.unmount();
    fireEvent.click(accept);
    await act(async () => {});
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(deleteJsonPath).not.toHaveBeenCalled();
  });

  it("确认接受后同批次切换路径也不发送过期根删除", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />);
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    await act(async () => {
      fireEvent.click(accept);
      fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: "$.name" } });
    });
    expect(deleteJsonPath).not.toHaveBeenCalled();
  });

  it("子路径删除保留原有直接提交行为", async () => {
    deleteJsonPath.mockResolvedValue({ key: "profile:1", path: "$.name", affected: 1, new_length: null, ttl_ms: 5000 });
    getBrowserKey.mockResolvedValue(detail);
    render(<KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />);
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: "$.name" } });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    await waitFor(() => expect(callbacks.onDetailChange).toHaveBeenCalledWith(detail));
    expect(deleteJsonPath).toHaveBeenCalledExactlyOnceWith({ connection_id: "local", key: "profile:1", path: "$.name" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(callbacks.onDeleted).not.toHaveBeenCalled();
  });

  it("同键TTL或能力版本变化保留编辑器实例和草稿", async () => {
    setJsonPath.mockResolvedValue({ key: "profile:1", path: "$.name", affected: 1, new_length: null, ttl_ms: 4000 });
    getBrowserKey.mockResolvedValue(detail);
    const { rerender } = render(
      <KeyDetails connectionId="local" detail={detail} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    const pathInput = screen.getByRole("textbox", { name: "JSON Path" });
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
    expect(screen.getByRole("textbox", { name: "JSON Path" })).toBe(pathInput);
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
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: "$.name" } });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), { target: { value: '"Bob"' } });
    rerender(
      <KeyDetails connectionId="local" detail={{ ...detail, key: "profile:2" }} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    expect(screen.getByRole("textbox", { name: "JSON Path" })).toHaveValue("$");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue(JSON.stringify(value, null, 2));
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), { target: { value: "$.tags" } });
    rerender(
      <KeyDetails connectionId="remote" detail={{ ...detail, key: "profile:2" }} loading={false} moduleProbe={probe()} {...callbacks} />,
    );
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    expect(screen.getByRole("textbox", { name: "JSON Path" })).toHaveValue("$");
  });
});
