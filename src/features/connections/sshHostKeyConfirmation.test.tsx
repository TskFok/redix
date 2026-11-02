import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import ConnectionPage from "./ConnectionPage";
import ConnectionForm from "./ConnectionForm";
import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const challengeId = "11c630fb-3c1b-431c-8ca7-911260dc5925";
const otherChallengeId = "e849f894-216f-47b9-88d0-74de4fec6e54";
const connectionToken = "070a4dd4-56a5-4bcb-a3f6-76565763a079";
const challenge = {
  challenge_id: challengeId, host: "bastion.example", port: 2222,
  algorithm: "ssh-ed25519", fingerprint: "SHA256:Uyb44yHMR4oC+sHn3bRYb2jboKfiAmeOxCslJ0ghIIU",
};
const unknownHost = { code: "SSH_HOST_KEY_UNKNOWN", message: "SSH 主机尚未信任", challenge_id: challengeId };
const profile: ConnectionProfile = {
  id: "remote", name: "远程 Redis", host: "127.0.0.1", port: 6379, username: null,
  database: 0, has_password: false, tls: false, verify_server_cert: true,
  ca_certificate_name: null, client_certificate_name: null, has_ca_certificate: false, has_client_certificate: false,
  ssh: { host: "bastion.example", port: 2222, username: "operator", auth_method: "agent",
    has_password: false, has_private_key: false, has_passphrase: false, has_identity_file: false, has_known_hosts_file: true },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function calls(command: string) { return invokeMock.mock.calls.filter(([name]) => name === command); }

function overrideCommand(command: string, replacement: () => unknown | Promise<unknown>) {
  const previous = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (name, args, options) => {
    if (name === command) return await replacement();
    return previous(name, args, options);
  });
}

async function acceptHost() {
  const dialog = await screen.findByRole("alertdialog", { name: "确认 SSH 主机指纹" });
  fireEvent.click(within(dialog).getByRole("button", { name: "信任并连接" }));
}

describe("SSH 主机指纹确认", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    let trusted = false;
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_connections") return [profile];
      if (command === "save_connection") return (args as { input: SaveConnectionInput }).input.profile;
      if (command === "get_ssh_host_key_challenge") return challenge;
      if (command === "trust_ssh_host_key") { trusted = true; return; }
      if (command === "open_connection" || command === "test_connection") {
        if (!trusted) throw unknownHost;
        return { server_version: "8.4.0" };
      }
      return undefined;
    });
  });
  afterEach(cleanup);

  it("已保存连接核对主机与指纹，确认并重试成功后才关闭旧连接", async () => {
    const onOpened = vi.fn();
    render(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认 SSH 主机指纹" });
    expect(dialog).toHaveTextContent("bastion.example:2222");
    expect(dialog).toHaveTextContent(challenge.algorithm);
    expect(dialog).toHaveTextContent(challenge.fingerprint);
    expect(within(dialog).getByRole("button", { name: "取消" })).toHaveFocus();
    expect(calls("trust_ssh_host_key")).toHaveLength(0);
    expect(calls("close_connection")).toHaveLength(0);
    await acceptHost();
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(profile));
    expect(invokeMock.mock.calls.map(([name]) => name)).toEqual([
      "list_connections", "open_connection", "get_ssh_host_key_challenge", "trust_ssh_host_key", "open_connection", "close_connection",
    ]);
    expect(calls("trust_ssh_host_key")[0][1]).toEqual({ challenge_id: challengeId });
    expect(calls("open_connection")[1][1]).toEqual({ connection_id: "remote" });
  });

  it.each(["取消", "Escape"])("%s 不信任、不重试并清理待确认记录", async (method) => {
    const onOpened = vi.fn();
    render(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    const dialog = await screen.findByRole("alertdialog");
    if (method === "Escape") fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    else fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    await waitFor(() => expect(calls("discard_ssh_host_key")).toHaveLength(1));
    expect(calls("discard_ssh_host_key")[0][1]).toEqual({ challenge_id: challengeId });
    expect(calls("trust_ssh_host_key")).toHaveLength(0);
    expect(calls("open_connection")).toHaveLength(1);
    expect(calls("close_connection")).toHaveLength(0);
    expect(onOpened).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("测试连接确认后重用原始输入，不保存连接或发送历史 known_hosts 路径", async () => {
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 远程 Redis" }));
    expect(screen.queryByLabelText("SSH 已知主机文件路径")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await acceptHost();
    expect(await screen.findByRole("status")).toHaveTextContent("Redis 8.4.0");
    const attempts = calls("test_connection");
    expect(attempts).toHaveLength(2);
    expect(attempts[1][1]).toEqual(attempts[0][1]);
    expect((attempts[0][1] as { input: SaveConnectionInput }).input).not.toHaveProperty("ssh_known_hosts_file");
    expect(calls("save_connection")).toHaveLength(0);
    expect(calls("open_connection")).toHaveLength(0);
  });

  it("保存并连接在表单卸载后仍可确认，不重复保存", async () => {
    const onOpened = vi.fn();
    render(<ConnectionPage onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 远程 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));
    await screen.findByRole("alertdialog");
    expect(screen.queryByLabelText("SSH 主机")).not.toBeInTheDocument();
    await acceptHost();
    await waitFor(() => expect(onOpened).toHaveBeenCalled());
    expect(calls("save_connection")).toHaveLength(1);
    expect(calls("open_connection")).toHaveLength(2);
  });

  it("独立表单的保存并连接也确认后重试", async () => {
    const onOpened = vi.fn();
    render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} onOpened={onOpened} />);
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));
    await acceptHost();
    await waitFor(() => expect(onOpened).toHaveBeenCalled());
    expect(calls("open_connection")).toHaveLength(2);
  });

  it.each(["SSH_HOST_KEY_CHANGED", "SSH_HOST_KEY_STORE_FAILED", "SSH_HOST_KEY_CHALLENGE_EXPIRED"])("%s 显示明确错误且不能首次信任覆盖", async (code) => {
    overrideCommand("open_connection", () => { throw { code, message: "SSH 失败" }; });
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(code);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(calls("trust_ssh_host_key")).toHaveLength(0);
    expect(calls("open_connection")).toHaveLength(1);
  });

  it.each(["get_ssh_host_key_challenge", "trust_ssh_host_key"])("%s 失败时清理待确认记录且不重试", async (command) => {
    overrideCommand(command, () => { throw { code: "SSH_HOST_KEY_STORE_FAILED", message: "无法保存" }; });
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    if (command === "trust_ssh_host_key") await acceptHost();
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH_HOST_KEY_STORE_FAILED");
    expect(calls("open_connection")).toHaveLength(1);
    expect(calls("discard_ssh_host_key")).toHaveLength(1);
  });

  it("确认后至多重试一次，再次未知时清理新记录并停止", async () => {
    let attempts = 0;
    overrideCommand("open_connection", () => {
      attempts += 1;
      throw { ...unknownHost, challenge_id: attempts === 1 ? challengeId : otherChallengeId };
    });
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await acceptHost();
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH_HOST_KEY_UNKNOWN");
    expect(calls("open_connection")).toHaveLength(2);
    expect(calls("get_ssh_host_key_challenge")).toHaveLength(1);
    expect(calls("discard_ssh_host_key").map(([, args]) => args)).toContainEqual({ challenge_id: otherChallengeId });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it.each(["读取指纹", "等待确认", "保存信任"])("%s期间卸载页面清理记录且不再连接", async (stage) => {
    const pending = deferred<unknown>();
    if (stage === "读取指纹") overrideCommand("get_ssh_host_key_challenge", () => pending.promise);
    if (stage === "保存信任") overrideCommand("trust_ssh_host_key", () => pending.promise);
    const onOpened = vi.fn();
    const view = render(<ConnectionPage onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    if (stage === "读取指纹") await waitFor(() => expect(calls("get_ssh_host_key_challenge")).toHaveLength(1));
    else await screen.findByRole("alertdialog");
    if (stage === "保存信任") {
      await acceptHost();
      await waitFor(() => expect(calls("trust_ssh_host_key")).toHaveLength(1));
    }
    view.unmount();
    await waitFor(() => expect(calls("discard_ssh_host_key")).toHaveLength(1));
    await act(async () => { pending.resolve(stage === "读取指纹" ? challenge : undefined); });
    expect(calls("open_connection")).toHaveLength(1);
    expect(onOpened).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    if (stage !== "保存信任") expect(calls("trust_ssh_host_key")).toHaveLength(0);
  });

  it("同一批次重复连接不会并行读取或混淆不同主机的指纹", async () => {
    overrideCommand("list_connections", () => [profile, { ...profile, id: "second", name: "第二个连接" }]);
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    await screen.findByText("第二个连接");
    const buttons = screen.getAllByRole("button", { name: "连接" });
    act(() => { buttons[0].click(); buttons[1].click(); });
    expect(await screen.findByRole("alertdialog")).toHaveTextContent(challenge.fingerprint);
    expect(calls("open_connection")).toHaveLength(1);
    expect(calls("get_ssh_host_key_challenge")).toHaveLength(1);
  });

  it.each(["新增连接", "编辑 第二个连接"])("读取指纹时阻止%s启动另一表单", async (action) => {
    const pending = deferred<typeof challenge>();
    overrideCommand("list_connections", () => [profile, { ...profile, id: "second", name: "第二个连接" }]);
    overrideCommand("get_ssh_host_key_challenge", () => pending.promise);
    render(<ConnectionPage onOpenConnection={vi.fn()} />);
    fireEvent.click((await screen.findAllByRole("button", { name: "连接" }))[0]);
    await waitFor(() => expect(calls("get_ssh_host_key_challenge")).toHaveLength(1));
    fireEvent.click(screen.getByRole("button", { name: action }));
    expect(screen.queryByLabelText("SSH 主机")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("连接名称")).not.toBeInTheDocument();
    await act(async () => { pending.resolve(challenge); });
    expect(await screen.findByRole("alertdialog")).toHaveTextContent(challenge.fingerprint);
  });

  it("活动连接变化会取消旧指纹确认", async () => {
    const onOpened = vi.fn();
    const view = render(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("alertdialog");
    view.rerender(<ConnectionPage activeConnectionId="other" onOpenConnection={onOpened} />);
    await waitFor(() => expect(calls("discard_ssh_host_key")).toHaveLength(1));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(calls("trust_ssh_host_key")).toHaveLength(0);
    expect(calls("open_connection")).toHaveLength(1);
    expect(onOpened).not.toHaveBeenCalled();
  });

  it.each([
    { standalone: false, retry: false },
    { standalone: false, retry: true },
    { standalone: true, retry: false },
    { standalone: true, retry: true },
  ])("已发出的连接在卸载后成功时按本次令牌清理（独立表单=$standalone，重试=$retry）", async ({ standalone, retry }) => {
    const pending = deferred<{ server_version: string; connection_token: string }>();
    let attempts = 0;
    overrideCommand("open_connection", () => {
      attempts += 1;
      if (retry && attempts === 1) throw unknownHost;
      return pending.promise;
    });
    const onOpened = vi.fn();
    const view = standalone
      ? render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} onOpened={onOpened} />)
      : render(<ConnectionPage onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: standalone ? "保存并连接" : "连接" }));
    if (retry) await acceptHost();
    await waitFor(() => expect(calls("open_connection")).toHaveLength(retry ? 2 : 1));
    view.unmount();
    await act(async () => { pending.resolve({ server_version: "8.4.0", connection_token: connectionToken }); });
    expect(calls("close_connection_if_current").map(([, args]) => args)).toEqual([
      { connection_id: "remote", connection_token: connectionToken },
    ]);
    expect(calls("close_connection")).toHaveLength(0);
    expect(onOpened).not.toHaveBeenCalled();
  });

  it.each(["卸载", "切换作用域", "切换后返回原作用域"])("等待旧连接关闭期间%s只清理本次新连接，不触发迟到成功通知", async (context) => {
    const closing = deferred<void>();
    overrideCommand("open_connection", () => ({ server_version: "8.4.0", connection_token: connectionToken }));
    overrideCommand("close_connection", () => closing.promise);
    const onOpened = vi.fn();
    const view = render(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(calls("close_connection")).toHaveLength(1));
    if (context === "卸载") view.unmount();
    else {
      view.rerender(<ConnectionPage activeConnectionId="other" onOpenConnection={onOpened} />);
      if (context === "切换后返回原作用域") view.rerender(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    }
    await act(async () => { closing.resolve(); });
    expect(calls("close_connection_if_current").map(([, args]) => args)).toEqual([
      { connection_id: "remote", connection_token: connectionToken },
    ]);
    expect(onOpened).not.toHaveBeenCalled();
  });

  it("卸载后旧连接关闭失败也只按令牌清理新连接", async () => {
    const closing = deferred<void>();
    overrideCommand("open_connection", () => ({ server_version: "8.4.0", connection_token: connectionToken }));
    overrideCommand("close_connection", () => closing.promise);
    const onOpened = vi.fn();
    const view = render(<ConnectionPage activeConnectionId="old" onOpenConnection={onOpened} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(calls("close_connection")).toHaveLength(1));
    view.unmount();
    await act(async () => { closing.reject({ code: "CONNECTION_FAILED", message: "关闭失败" }); });
    expect(calls("close_connection_if_current").map(([, args]) => args)).toEqual([
      { connection_id: "remote", connection_token: connectionToken },
    ]);
    expect(calls("close_connection")).toHaveLength(1);
    expect(onOpened).not.toHaveBeenCalled();
  });
});
