import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ConnectionPage from "./ConnectionPage";
import ConnectionForm from "./ConnectionForm";
import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";

const {
  listConnectionsMock,
  saveConnectionMock,
  openConnectionMock,
  closeConnectionMock,
  deleteConnectionMock,
  testConnectionMock,
  exportConnectionsMock,
  importConnectionsMock,
  onOpenConnectionMock,
  listConnectionTagsMock,
  saveConnectionTagsMock,
} = vi.hoisted(() => ({
  listConnectionsMock: vi.fn(),
  saveConnectionMock: vi.fn(),
  openConnectionMock: vi.fn(),
  closeConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
  exportConnectionsMock: vi.fn(),
  importConnectionsMock: vi.fn(),
  onOpenConnectionMock: vi.fn(),
  listConnectionTagsMock: vi.fn(),
  saveConnectionTagsMock: vi.fn(),
}));

vi.mock("../../lib/localProductsApi", () => ({
  listConnectionTags: listConnectionTagsMock,
  saveConnectionTags: saveConnectionTagsMock,
}));

vi.mock("../../lib/tauri", () => ({
  listConnections: listConnectionsMock,
  saveConnection: saveConnectionMock,
  openConnection: openConnectionMock,
  closeConnection: closeConnectionMock,
  deleteConnection: deleteConnectionMock,
  testConnection: testConnectionMock,
  exportConnections: exportConnectionsMock,
  importConnections: importConnectionsMock,
}));

const localProfile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
  tls: false,
  verify_server_cert: true,
  ca_certificate_name: null,
  client_certificate_name: null,
  has_ca_certificate: false,
  has_client_certificate: false,
};

const testCertificatePem =
  "-----BEGIN CERTIFICATE-----\ncertificate\n-----END CERTIFICATE-----";

function fillStandaloneForm() {
  fireEvent.change(screen.getByLabelText("连接名称"), {
    target: { value: "本地 Redis" },
  });
  fireEvent.change(screen.getByLabelText("主机"), {
    target: { value: "127.0.0.1" },
  });
  fireEvent.change(screen.getByLabelText("端口"), {
    target: { value: "6379" },
  });
}

async function openNewConnectionForm() {
  await screen.findByRole("button", { name: "新增连接" });
  fireEvent.click(screen.getByRole("button", { name: "新增连接" }));
  await screen.findByLabelText("连接名称");
}

describe("Redis 连接管理页面", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("crypto", { randomUUID: vi.fn(() => "local") });
    listConnectionsMock.mockResolvedValue([]);
    listConnectionTagsMock.mockResolvedValue({});
    saveConnectionMock.mockResolvedValue(localProfile);
    openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
    closeConnectionMock.mockResolvedValue(undefined);
    deleteConnectionMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("连接标签可在列表中管理、筛选，清空筛选恢复连接", async () => {
    listConnectionsMock.mockResolvedValue([localProfile, { ...localProfile, id: "two", name: "另一个 Redis" }]);
    listConnectionTagsMock.mockResolvedValue({ local: [{ key: "env", value: "prod" }] });
    saveConnectionTagsMock.mockResolvedValue([{ key: "env", value: "dev" }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("env=prod");
    fireEvent.change(screen.getByLabelText("筛选连接标签"), {target:{value:"prod"}});
    expect(screen.queryByText("另一个 Redis")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", {name:"管理标签 本地 Redis"}));
    fireEvent.change(screen.getByLabelText("标签值 1"), {target:{value:"dev"}});
    fireEvent.click(screen.getByRole("button", {name:"保存标签"}));
    expect(await screen.findByText("没有匹配标签的连接。")).toBeInTheDocument();
    expect(saveConnectionTagsMock).toHaveBeenCalledWith("local", [{key:"env",value:"dev"}]);
    fireEvent.change(screen.getByLabelText("筛选连接标签"), {target:{value:""}});
    expect(screen.getByText("env=dev")).toBeInTheDocument();
    expect(screen.getByText("另一个 Redis")).toBeInTheDocument();
  });

  it("特殊连接ID缺少标签时仍可打开标签编辑器", async () => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, id: "__proto__" }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "管理标签 本地 Redis" }));
    expect(screen.getByRole("button", { name: "添加标签" })).toBeInTheDocument();
    expect(screen.queryByLabelText("标签键 1")).not.toBeInTheDocument();
  });

  it("无匹配连接时清除全部筛选并恢复列表和数量", async () => {
    listConnectionsMock.mockResolvedValue([localProfile, { ...localProfile, id: "two", name: "另一个 Redis" }]);
    listConnectionTagsMock.mockResolvedValue({ local: [{ key: "env", value: "prod" }] });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("env=prod");
    fireEvent.change(screen.getByLabelText("筛选连接标签"), { target: { value: "prod" } });
    fireEvent.click(screen.getByLabelText("仅显示无标签连接"));
    expect(screen.getByText("没有匹配标签的连接。")).toBeInTheDocument();
    expect(screen.getByText("显示 0 / 2 个连接")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));
    expect(screen.getByLabelText("筛选连接标签")).toHaveValue("");
    expect(screen.getByLabelText("仅显示无标签连接")).not.toBeChecked();
    expect(screen.getByText("本地 Redis")).toBeInTheDocument();
    expect(screen.getByText("另一个 Redis")).toBeInTheDocument();
    expect(screen.getByText("共 2 个连接")).toBeInTheDocument();
  });

  it("无连接时显示新增提示并能打开连接表单", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    expect(await screen.findByText("还没有 Redis 连接")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "新增连接" }));

    expect(screen.getByLabelText("连接名称")).toBeInTheDocument();
    expect(screen.getByLabelText("主机")).toBeInTheDocument();
    expect(screen.getByLabelText("端口")).toBeInTheDocument();
    expect(screen.getByLabelText("密码")).toHaveAttribute("type", "password");
  });

  it("保存 Sentinel 种子与独立凭据并说明重新连接语义", async () => {
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => input.profile);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("连接拓扑"), { target: { value: "sentinel" } });
    fireEvent.change(screen.getByLabelText("Sentinel 主节点名称"), { target: { value: "primary" } });
    fireEvent.change(screen.getByLabelText("Sentinel 种子节点"), { target: { value: "sentinel-a:26379\n[::1]:26380" } });
    fireEvent.change(screen.getByLabelText("Sentinel 用户名"), { target: { value: "watcher" } });
    fireEvent.change(screen.getByLabelText("Sentinel 密码"), { target: { value: "sentinel-secret" } });
    fireEvent.change(screen.getByLabelText("密码"), { target: { value: "redis-secret" } });
    fireEvent.click(screen.getByLabelText("Sentinel 启用 TLS"));
    expect(screen.getByLabelText("启用 TLS")).not.toBeChecked();
    expect(screen.getByLabelText("验证服务端证书")).toBeInTheDocument();
    expect(screen.getByLabelText("CA 名称")).toBeInTheDocument();
    expect(screen.getByText(/重新连接时发现当前主节点/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
      profile: { sentinel: { master_name: "primary", nodes: [{ host: "sentinel-a", port: 26379 }, { host: "::1", port: 26380 }], username: "watcher", has_password: true } },
      password: "redis-secret", sentinel_password: "sentinel-secret",
    });
    expect(await screen.findByText("Sentinel · primary")).toBeInTheDocument();
  });

  it("SSH 私钥路径仅进入 secret input 并允许 TLS", async () => {
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => input.profile);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByLabelText("启用 SSH 隧道"));
    fireEvent.change(screen.getByLabelText("SSH 主机"), { target: { value: "bastion.example" } });
    fireEvent.change(screen.getByLabelText("SSH 用户名"), { target: { value: "operator" } });
    fireEvent.change(screen.getByLabelText("SSH 认证方式"), { target: { value: "private_key" } });
    fireEvent.change(screen.getByLabelText("SSH 私钥文件路径"), { target: { value: "/Users/operator/.ssh/id_ed25519" } });
    expect(screen.getByText(/known_hosts/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("启用 TLS"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).toMatchObject({ host: "bastion.example", port: 22, username: "operator", auth_method: "private_key", has_identity_file: true });
    expect(saveConnectionMock.mock.calls[0][0].ssh_identity_file).toBe("/Users/operator/.ssh/id_ed25519");
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).not.toHaveProperty("identity_file");
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).not.toHaveProperty("known_hosts_file");
    expect(await screen.findByText("SSH · bastion.example:22")).toBeInTheDocument();
  });

  it("Cluster 校验唯一种子并固定 DB0，测试和保存使用相同材料", async () => {
    let generated = 0;
    vi.mocked(crypto.randomUUID).mockImplementation(() => `connection-${++generated}` as `${string}-${string}-${string}-${string}-${string}`);
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => input.profile);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm(); fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("数据库"), { target: { value: "9" } });
    fireEvent.change(screen.getByLabelText("连接拓扑"), { target: { value: "cluster" } });
    expect(screen.getByLabelText("数据库")).toHaveValue(0);
    expect(screen.getByLabelText("启用 SSH 隧道")).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Cluster 种子节点"), { target: { value: "[::1]:7000\n[::1]:7000" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("唯一");
    fireEvent.change(screen.getByLabelText("Cluster 种子节点"), { target: { value: "[::1]:7000\nredis-b:7001" } });
    fireEvent.click(screen.getByLabelText("允许从副本读取"));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await screen.findByText(/连接成功/);
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock.mock.calls[0][0]).toEqual(testConnectionMock.mock.calls[0][0]);
    expect(saveConnectionMock.mock.calls[0][0].profile).toMatchObject({ host: "::1", port: 7000, database: 0, cluster: { nodes: [{ host: "::1", port: 7000 }, { host: "redis-b", port: 7001 }], read_from_replicas: true } });
    expect(screen.getByText(/Cluster · 2 个种子/)).toHaveTextContent("redis-b:7001");
  });

  it("Sentinel SSH 密码与 Redis/Sentinel 凭据独立，支持双 TLS", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm(); fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("连接拓扑"), { target: { value: "sentinel" } });
    fireEvent.change(screen.getByLabelText("Sentinel 主节点名称"), { target: { value: "primary" } });
    fireEvent.change(screen.getByLabelText("Sentinel 密码"), { target: { value: "sentinel-secret" } });
    fireEvent.change(screen.getByLabelText("密码"), { target: { value: "redis-secret" } });
    fireEvent.click(screen.getByLabelText("启用 SSH 隧道"));
    fireEvent.change(screen.getByLabelText("SSH 主机"), { target: { value: "jump" } });
    fireEvent.change(screen.getByLabelText("SSH 用户名"), { target: { value: "user" } });
    fireEvent.change(screen.getByLabelText("SSH 认证方式"), { target: { value: "password" } });
    fireEvent.change(screen.getByLabelText("SSH 密码"), { target: { value: "ssh-secret" } });
    fireEvent.click(screen.getByLabelText("启用 TLS")); fireEvent.click(screen.getByLabelText("Sentinel 启用 TLS"));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await screen.findByText(/连接成功/);
    expect(testConnectionMock.mock.calls[0][0]).toMatchObject({ password: "redis-secret", sentinel_password: "sentinel-secret", ssh_password: "ssh-secret", profile: { tls: true, sentinel: { tls: true }, ssh: { auth_method: "password" } } });
  });

  it("Private Key 清除旧路径后必须补新私钥，PEM 与口令不进入 profile", async () => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, ssh: { host: "jump", port: 22, username: "user", auth_method: "private_key", has_password: false, has_private_key: false, has_identity_file: true, has_passphrase: true, has_known_hosts_file: false } }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 本地 Redis" }));
    expect(screen.getByLabelText("SSH 私钥文件路径")).toHaveValue("");
    expect(screen.getByLabelText("SSH 私钥口令")).toHaveValue("");
    fireEvent.click(screen.getByLabelText("清除已保存的 SSH 凭据和路径"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请输入 SSH 私钥");
    const pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nsecret-key\n-----END OPENSSH PRIVATE KEY-----";
    fireEvent.change(screen.getByLabelText("SSH 私钥内容"), { target: { value: pem } });
    fireEvent.change(screen.getByLabelText("SSH 私钥口令"), { target: { value: " passphrase " } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({ clear_ssh_secrets: true, ssh_private_key: pem, ssh_passphrase: " passphrase ", profile: { ssh: { has_private_key: true, has_identity_file: false, has_passphrase: true } } });
    expect(JSON.stringify(saveConnectionMock.mock.calls[0][0].profile)).not.toContain("secret-key");
  });

  it.each([
    { storedPem: true, label: "SSH 私钥文件路径", value: "/private/new-key", field: "ssh_identity_file", otherField: "ssh_private_key" },
    { storedPem: false, label: "SSH 私钥内容", value: "-----BEGIN OPENSSH PRIVATE KEY-----\nreplacement\n-----END OPENSSH PRIVATE KEY-----", field: "ssh_private_key", otherField: "ssh_identity_file" },
  ])("切换已保存私钥来源到 $label 前要求显式清除，保存和测试一致", async ({ storedPem, label, value, field, otherField }) => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, ssh: { host: "jump", port: 22, username: "user", auth_method: "private_key", has_password: false, has_private_key: storedPem, has_identity_file: !storedPem, has_passphrase: true, has_known_hosts_file: true } }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 本地 Redis" }));
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("切换私钥来源前，请勾选清除已保存的 SSH 凭据和路径");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(testConnectionMock).not.toHaveBeenCalled(); expect(saveConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent("known_hosts");
    fireEvent.click(screen.getByLabelText("清除已保存的 SSH 凭据和路径"));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await screen.findByText(/连接成功/);
    expect(testConnectionMock.mock.calls[0][0]).toMatchObject({ clear_ssh_secrets: true, [field]: value, [otherField]: null, profile: { ssh: { has_passphrase: false, has_known_hosts_file: false } } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledOnce());
    expect(saveConnectionMock.mock.calls[0][0]).toEqual(testConnectionMock.mock.calls[0][0]);
  });

  it("新 PEM 与新私钥路径不能同时提交，即使已选择清除", async () => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, ssh: { host: "jump", port: 22, username: "user", auth_method: "private_key", has_password: false, has_private_key: true, has_identity_file: false, has_passphrase: false, has_known_hosts_file: false } }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 本地 Redis" }));
    fireEvent.click(screen.getByLabelText("清除已保存的 SSH 凭据和路径"));
    fireEvent.change(screen.getByLabelText("SSH 私钥内容"), { target: { value: "pem" } });
    fireEvent.change(screen.getByLabelText("SSH 私钥文件路径"), { target: { value: "/private/key" } });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH 私钥内容和私钥文件路径只能填写一项");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(testConnectionMock).not.toHaveBeenCalled(); expect(saveConnectionMock).not.toHaveBeenCalled();
  });

  it.each([true, false])("同来源私钥可留空保留或直接替换（PEM=%s）", async (storedPem) => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, ssh: { host: "jump", port: 22, username: "user", auth_method: "private_key", has_password: false, has_private_key: storedPem, has_identity_file: !storedPem, has_passphrase: true, has_known_hosts_file: true } }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 本地 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await screen.findByText(/连接成功/);
    expect(testConnectionMock.mock.calls[0][0]).toMatchObject({ clear_ssh_secrets: false, ssh_private_key: null, ssh_identity_file: null, profile: { ssh: { has_private_key: storedPem, has_identity_file: !storedPem } } });
    fireEvent.change(screen.getByLabelText(storedPem ? "SSH 私钥内容" : "SSH 私钥文件路径"), { target: { value: storedPem ? "replacement-pem" : "/private/replacement" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledOnce());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({ clear_ssh_secrets: false, [storedPem ? "ssh_private_key" : "ssh_identity_file"]: storedPem ? "replacement-pem" : "/private/replacement", profile: { ssh: { has_passphrase: true, has_known_hosts_file: true } } });
  });

  it("清除 SSH 保存材料后旧标记不满足密码，允许输入替换并清理模式材料", async () => {
    listConnectionsMock.mockResolvedValue([{ ...localProfile, ssh: { host: "jump", port: 22, username: "user", auth_method: "password", has_password: true, has_private_key: false, has_identity_file: false, has_passphrase: false, has_known_hosts_file: true } }]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 本地 Redis" }));
    expect(screen.getByLabelText("SSH 密码")).toHaveValue("");
    expect(screen.getByLabelText("SSH 已知主机文件路径")).toHaveValue("");
    fireEvent.click(screen.getByLabelText("清除已保存的 SSH 凭据和路径"));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH 密码");
    fireEvent.change(screen.getByLabelText("SSH 密码"), { target: { value: "replacement" } });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await screen.findByText(/连接成功/);
    expect(testConnectionMock.mock.calls[0][0]).toMatchObject({ clear_ssh_secrets: true, ssh_password: "replacement", profile: { ssh: { has_known_hosts_file: false } } });
    fireEvent.change(screen.getByLabelText("SSH 认证方式"), { target: { value: "agent" } });
    expect(screen.queryByLabelText("SSH 私钥文件路径")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    await waitFor(() => expect(testConnectionMock).toHaveBeenCalledTimes(2));
    expect(testConnectionMock.mock.calls[1][0]).toMatchObject({ ssh_password: null, profile: { ssh: { auth_method: "agent", has_password: false } } });
  });

  it("导出连接时调用 typed IPC 并显示成功反馈", async () => {
    exportConnectionsMock.mockResolvedValue({ version: 2, connections: [] });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("还没有 Redis 连接");
    vi.useFakeTimers();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "导出连接" }));
    });

    expect(exportConnectionsMock).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status")).toHaveTextContent("连接已导出");
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.queryByText(/连接已导出/)).not.toBeInTheDocument();
  });

  it("导入 JSON 后追加连接并显示部分成功及敏感字段忽略提示", async () => {
    importConnectionsMock.mockResolvedValue({
      imported: [localProfile],
      failed: [
        {
          index: 1,
          name: "Broken",
          code: "INVALID_CONNECTION",
          message: "连接配置无效",
        },
      ],
      ignored_secret_fields: 1,
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    const file = new File(
      ['{"connections":[]}'],
      "connections.json",
      { type: "application/json" },
    );
    fireEvent.change(screen.getByLabelText("导入连接文件"), {
      target: { files: [file] },
    });

    await waitFor(() => expect(importConnectionsMock).toHaveBeenCalledTimes(1));
    expect(importConnectionsMock).toHaveBeenCalledWith({
      content: '{"connections":[]}',
    });
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("部分导入");
    expect(status).toHaveTextContent("已忽略 1 个敏感字段");
    expect(status).toHaveTextContent("Broken");
  });

  it("导入时忽略敏感字段的提示持续显示", async () => {
    importConnectionsMock.mockResolvedValue({
      imported: [localProfile],
      failed: [],
      ignored_secret_fields: 1,
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("还没有 Redis 连接");
    vi.useFakeTimers();

    await act(async () => {
      fireEvent.change(screen.getByLabelText("导入连接文件"), {
        target: { files: [new File(['{"connections":[]}'], "connections.json", { type: "application/json" })] },
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("已忽略 1 个敏感字段");
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.getByRole("status")).toHaveTextContent("已忽略 1 个敏感字段");
  });

  it("测试连接成功提示会自动消失", async () => {
    render(<ConnectionForm initial={localProfile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    vi.useFakeTimers();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    });

    expect(screen.getByText(/连接成功/)).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.queryByText(/连接成功/)).not.toBeInTheDocument();
  });

  it("拒绝超过 10 MiB 的连接导入文件", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    const file = new File(["{}"], "too-large.json", { type: "application/json" });
    Object.defineProperty(file, "size", { value: 10 * 1024 * 1024 + 1 });
    fireEvent.change(screen.getByLabelText("导入连接文件"), {
      target: { files: [file] },
    });

    expect(await screen.findByRole("alert")).toHaveTextContent("不能超过 10 MB");
    expect(importConnectionsMock).not.toHaveBeenCalled();
  });

  it("连接卡片显示 TLS 与证书重新录入状态", async () => {
    const tlsProfile: ConnectionProfile = {
      ...localProfile,
      id: "tls-profile",
      name: "TLS Redis",
      tls: true,
      ca_certificate_name: "Root CA",
      has_ca_certificate: false,
    };
    listConnectionsMock.mockResolvedValue([tlsProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("TLS Redis");
    expect(screen.getByText("已启用")).toBeInTheDocument();
    expect(screen.getByText("需重新录入")).toBeInTheDocument();
  });

  it("保存成功后刷新连接列表并按顺序打开对应连接", async () => {
    const calls: string[] = [];
    const saveInput: SaveConnectionInput = {
      clear_ssh_secrets: false,
      profile: localProfile,
      password: null,
      ca_certificate: null,
      client_certificate: null,
      client_key: null,
      clear_ca_certificate: false,
      clear_client_certificate: false,
    };
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => {
      calls.push("save");
      expect(input).toEqual(saveInput);
      return localProfile;
    });
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push("open");
      expect(connectionId).toBe(localProfile.id);
      return { server_version: "8.4.0" };
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    await waitFor(() => {
      expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile);
    });
    expect(calls).toEqual(["save", "open"]);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);
    expect(openConnectionMock).toHaveBeenCalledWith(localProfile.id);
    expect(crypto.randomUUID).toHaveBeenCalled();
  });

  it("测试连接只调用 testConnection，不保存或打开连接", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "test-only-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(testConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock).not.toHaveBeenCalled();
    expect(openConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Redis 8.4.0");
  });

  it("提交前校验必填字段并在失败时显示可访问错误", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("请输入连接名称");
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });

  it("编辑已有连接时空密码会保留旧 secret 标记", async () => {
    const securedProfile: ConnectionProfile = {
      ...localProfile,
      id: "secured",
      name: "受保护 Redis",
      has_password: true,
    };
    listConnectionsMock.mockResolvedValue([securedProfile]);
    saveConnectionMock.mockResolvedValue(securedProfile);

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("受保护 Redis");
    fireEvent.click(screen.getByRole("button", { name: "编辑 受保护 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock).toHaveBeenCalledWith({
      profile: securedProfile,
      clear_ssh_secrets: false,
      password: null,
      ca_certificate: null,
      client_certificate: null,
      client_key: null,
      clear_ca_certificate: false,
      clear_client_certificate: false,
    });
  });

  it("保存 TLS 表单时发送校验开关和 CA 证书材料", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByLabelText("启用 TLS"));
    fireEvent.change(screen.getByLabelText("CA 证书"), {
      target: { value: testCertificatePem },
    });
    fireEvent.change(screen.getByLabelText("CA 名称"), {
      target: { value: "Root CA" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
      profile: expect.objectContaining({
        tls: true,
        verify_server_cert: true,
        ca_certificate_name: "Root CA",
      }),
      ca_certificate: testCertificatePem,
      clear_ca_certificate: false,
    });
  });

  it("删除前确认，确认后删除并从列表移除连接", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("本地 Redis");

    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));
    expect(deleteConnectionMock).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(deleteConnectionMock).toHaveBeenCalledWith("local"));
    expect(screen.queryByText("本地 Redis")).not.toBeInTheDocument();
  });

  it("删除当前活动连接后通知父级清理活动 profile", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("本地 Redis");

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() =>
      expect(onOpenConnectionMock).toHaveBeenNthCalledWith(2, null),
    );
  });

  it("返回后重新挂载仍识别活动连接，删除时通知父级清理", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    const initial = render(<ConnectionPage activeConnectionId={null} onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));
    initial.unmount();

    render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);
    expect(await screen.findByRole("button", { name: "重新连接" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenLastCalledWith(null));
    expect(screen.queryByText("本地 Redis")).not.toBeInTheDocument();
  });

  it("取消删除连接保留连接且不发送请求", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "删除 本地 Redis" }));
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("本地 Redis");
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "取消" })));
    expect(deleteConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByText("本地 Redis")).toBeInTheDocument();
  });

  it.each(["活动连接", "编辑连接", "筛选连接"])("更改%s取消待确认删除", async (context) => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    const view = render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "删除 本地 Redis" }));
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    if (context === "活动连接") view.rerender(<ConnectionPage activeConnectionId="other" onOpenConnection={onOpenConnectionMock} />);
    else if (context === "编辑连接") fireEvent.click(screen.getByRole("button", { name: "编辑 本地 Redis" }));
    else fireEvent.change(screen.getByLabelText("筛选连接标签"), { target: { value: "missing" } });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await act(async () => fireEvent.click(accept));
    expect(deleteConnectionMock).not.toHaveBeenCalled();
  });

  it("连接删除请求未完成时不重复请求", async () => {
    let finish!: () => void;
    deleteConnectionMock.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    const remove = await screen.findByRole("button", { name: "删除 本地 Redis" });
    fireEvent.click(remove);
    fireEvent.click(remove);
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    await act(async () => fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" })));
    fireEvent.click(remove);
    expect(deleteConnectionMock).toHaveBeenCalledTimes(1);
    await act(async () => finish());
    expect(screen.queryByText("本地 Redis")).not.toBeInTheDocument();
  });

  it("确认接受后同批切换活动连接仍阻止旧删除请求", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    const view = render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "删除 本地 Redis" }));
    act(() => {
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
      view.rerender(<ConnectionPage activeConnectionId="other" onOpenConnection={onOpenConnectionMock} />);
    });
    await act(async () => {});
    expect(deleteConnectionMock).not.toHaveBeenCalled();
  });

  it("外部活动连接置空时不再沿用内部已连接状态", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    const view = render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "重新连接" });

    view.rerender(<ConnectionPage activeConnectionId={null} onOpenConnection={onOpenConnectionMock} />);
    expect(screen.getByRole("button", { name: "连接" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "重新连接" })).not.toBeInTheDocument();
  });

  it("重挂载后的切换先打开新连接，再释放外部活动连接", async () => {
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis" };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    openConnectionMock.mockImplementation(async (id: string) => { calls.push(`open:${id}`); return { server_version: "8.4.0" }; });
    closeConnectionMock.mockImplementation(async (id: string) => { calls.push(`close:${id}`); });
    render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("远程 Redis");
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(remoteProfile));
    expect(calls).toEqual(["open:remote", "close:local"]);
  });

  it("保存并连接也按顺序切换并释放旧连接", async () => {
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis" };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    saveConnectionMock.mockImplementation(async () => { calls.push("save:remote"); return remoteProfile; });
    openConnectionMock.mockImplementation(async (id: string) => { calls.push(`open:${id}`); return { server_version: "8.4.0" }; });
    closeConnectionMock.mockImplementation(async (id: string) => { calls.push(`close:${id}`); });
    render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);

    fireEvent.click(await screen.findByRole("button", { name: "编辑 远程 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(remoteProfile));
    expect(calls).toEqual(["save:remote", "open:remote", "close:local"]);
  });

  it.each([
    { failure: "open", message: "已保存连接，但打开失败", closeIds: [] },
    { failure: "close", message: "切换连接失败，已保留原连接", closeIds: ["local", "remote"] },
    { failure: "cleanup", message: "切换连接失败，清理新连接失败", closeIds: ["local", "remote"] },
  ])("保存并连接在 $failure 失败后保留原连接并显示反馈", async ({ failure, message, closeIds }) => {
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis" };
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    saveConnectionMock.mockResolvedValue(remoteProfile);
    openConnectionMock.mockImplementation(async () => {
      if (failure === "open") throw { code: "CONNECTION_FAILED" };
      return { server_version: "8.4.0" };
    });
    closeConnectionMock.mockImplementation(async (id: string) => {
      if (id === "local" || failure === "cleanup") throw { code: "CONNECTION_FAILED" };
    });
    render(<ConnectionPage activeConnectionId="local" onOpenConnection={onOpenConnectionMock} />);

    fireEvent.click(await screen.findByRole("button", { name: "编辑 远程 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    expect(screen.getByRole("button", { name: "重新连接" }).closest("article")).toHaveTextContent("本地 Redis");
    expect(screen.getByText("远程 Redis")).toBeInTheDocument();
    expect(onOpenConnectionMock).not.toHaveBeenCalled();
    expect(closeConnectionMock.mock.calls.map(([id]) => id)).toEqual(closeIds);
  });

  it("独立连接表单仍支持保存并连接及成功通知", async () => {
    const onSaved = vi.fn();
    render(<ConnectionForm initial={localProfile} onSaved={onSaved} onCancel={vi.fn()} onOpened={onOpenConnectionMock} />);
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));
    expect(onSaved).toHaveBeenCalledWith(localProfile);
    expect(openConnectionMock).toHaveBeenCalledWith("local");
  });

  it("切换连接时先打开新连接，再关闭旧连接并更新活动连接", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote",
      name: "远程 Redis",
      host: "192.0.2.10",
    };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`open:${connectionId}`);
      return { server_version: "8.4.0" };
    });
    closeConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`close:${connectionId}`);
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("远程 Redis");

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() =>
      expect(onOpenConnectionMock).toHaveBeenLastCalledWith(secondProfile),
    );

    expect(calls).toEqual(["open:local", "open:remote", "close:local"]);
    expect(closeConnectionMock).toHaveBeenCalledWith("local");
  });

  it("打开新连接失败时保留旧连接且不关闭旧连接", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote-failed",
      name: "失败 Redis",
    };
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      if (connectionId === secondProfile.id) {
        throw { code: "CONNECTION_FAILED", message: "连接失败" };
      }
      return { server_version: "8.4.0" };
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("失败 Redis");
    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("无法连接到 Redis 服务器"),
    );

    expect(closeConnectionMock).not.toHaveBeenCalled();
    expect(onOpenConnectionMock).toHaveBeenLastCalledWith(localProfile);
    expect(screen.getByRole("button", { name: "重新连接" })).toBeInTheDocument();
  });

  it("旧连接关闭失败时清理新连接并保留旧连接状态", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote-close-failed",
      name: "关闭失败 Redis",
    };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`open:${connectionId}`);
      return { server_version: "8.4.0" };
    });
    closeConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`close:${connectionId}`);
      if (connectionId === localProfile.id) {
        throw { code: "CONNECTION_FAILED", message: "close failed" };
      }
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("关闭失败 Redis");
    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("切换连接失败，已保留原连接。"),
    );

    expect(calls).toEqual([
      "open:local",
      "open:remote-close-failed",
      "close:local",
      "close:remote-close-failed",
    ]);
    expect(onOpenConnectionMock).toHaveBeenLastCalledWith(localProfile);
    expect(screen.getByRole("button", { name: "重新连接" })).toBeInTheDocument();
  });

  it("保存成功但打开失败时保留 profile 并显示稳定提示", async () => {
    const calls: string[] = [];
    saveConnectionMock.mockImplementation(async () => {
      calls.push("save");
      return localProfile;
    });
    openConnectionMock.mockImplementation(async () => {
      calls.push("open");
      throw { code: "CONNECTION_FAILED", message: "无法连接到 Redis 服务器" };
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    await waitFor(() => expect(screen.getByText("本地 Redis")).toBeInTheDocument());
    expect(calls).toEqual(["save", "open"]);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);
    expect(openConnectionMock).toHaveBeenCalledWith(localProfile.id);
    expect(onOpenConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "已保存连接，但打开失败，请重试",
    );
    expect(screen.queryByRole("heading", { name: "新增连接" })).not.toBeInTheDocument();
  });

  it("编辑已有认证连接空密码测试时保留已保存凭据", async () => {
    const securedProfile: ConnectionProfile = {
      ...localProfile,
      id: "secured-test",
      name: "受保护测试 Redis",
      has_password: true,
    };
    listConnectionsMock.mockResolvedValue([securedProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("受保护测试 Redis");
    fireEvent.click(screen.getByRole("button", { name: "编辑 受保护测试 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    expect(await screen.findByText(/连接成功/)).toBeInTheDocument();
    expect(testConnectionMock).toHaveBeenCalledWith(expect.objectContaining({ password: null, profile: expect.objectContaining({ has_password: true }) }));
  });

  it("连接错误不会渲染密码、原始命令或 URI", async () => {
    saveConnectionMock.mockRejectedValue({
      code: "CONNECTION_FAILED",
      message: "redis://:secret-value@127.0.0.1:6379/0 执行 GET secret-key 失败",
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "secret-value" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法连接到 Redis 服务器");
    expect(alert).not.toHaveTextContent("secret-value");
    expect(alert).not.toHaveTextContent("redis://");
    expect(alert).not.toHaveTextContent("GET secret-key");
  });

  it("提交期间禁用重复保存操作", async () => {
    let resolveSave: ((profile: ConnectionProfile) => void) | undefined;
    saveConnectionMock.mockImplementation(
      () => new Promise<ConnectionProfile>((resolve) => {
        resolveSave = resolve;
      }),
    );
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    const saveButton = screen.getByRole("button", { name: "保存" });

    fireEvent.click(saveButton);
    expect(saveButton).toBeDisabled();
    fireEvent.click(saveButton);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);

    resolveSave?.(localProfile);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument(),
    );
  });
});
