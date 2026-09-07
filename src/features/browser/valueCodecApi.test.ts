import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { decodeStringValue, encodeStringValue } from "./valueCodecApi";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
class WorkerStub {
  static instances: WorkerStub[] = [];
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessageerror: (() => void) | null = null;
  terminate = vi.fn();
  postMessage = vi.fn();
  constructor() { WorkerStub.instances.push(this); }
}
const request = () => decodeStringValue({ base64: "kgEC", format: "msgpack", compression: "none" });

describe("协议Worker隔离与超时", () => {
  beforeEach(() => { vi.resetAllMocks(); WorkerStub.instances = []; vi.stubGlobal("Worker", WorkerStub); invoke.mockResolvedValue({ text: "kgEC", byte_length: 3 }); });
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });
  it("先获取本地解压字节再交给Worker，完成后销毁Worker", async () => {
    const result = request(); await Promise.resolve();
    const worker = WorkerStub.instances[0];
    expect(worker.postMessage.mock.calls[0][0]).toEqual({ bytes: Uint8Array.from([0x92, 1, 2]), format: "msgpack" });
    worker.onmessage?.({ data: { ok: true, text: "[1,2]" } });
    expect(await result).toEqual({ text: "[1,2]", byte_length: 3 });
    expect(worker.terminate).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("decode_string_value", { input: { base64: "kgEC", format: "base64", compression: "none" } });
  });
  it("超过2秒终止解码，并且拒绝只读协议编码", async () => {
    vi.useFakeTimers();
    const result = request(); const rejected = expect(result).rejects.toMatchObject({ code: "INVALID_INPUT" });
    await vi.advanceTimersByTimeAsync(2000); await rejected;
    expect(WorkerStub.instances[0].terminate).toHaveBeenCalledTimes(1);
    await expect(encodeStringValue({ text: "{}", format: "protobuf", compression: "none" })).rejects.toMatchObject({ code: "INVALID_INPUT" });
    await expect(encodeStringValue({ text: "{}", format: "php", compression: "none" })).rejects.toMatchObject({ code: "INVALID_INPUT" });
  });
  it("最多同时执行两个Worker，超额输入不创建Worker", async () => {
    const first = request(); const second = request(); await Promise.resolve();
    await expect(request()).rejects.toMatchObject({ code: "INVALID_INPUT" });
    expect(WorkerStub.instances).toHaveLength(2);
    WorkerStub.instances.forEach((worker) => worker.onmessage?.({ data: { ok: true, text: "[]" } }));
    await Promise.all([first, second]);
    invoke.mockResolvedValue({ text: "", byte_length: 4 * 1024 * 1024 + 1 });
    await expect(request()).rejects.toMatchObject({ code: "INVALID_INPUT" });
    expect(WorkerStub.instances).toHaveLength(2);
  });
});
