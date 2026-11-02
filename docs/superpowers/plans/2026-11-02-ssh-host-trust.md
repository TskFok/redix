# SSH 主机指纹确认 Implementation Plan

> **For agentic workers:** 使用 superpowers:subagent-driven-development 实施独立任务；主代理负责 transport 集成和最终验证。

**Goal:** 未知 SSH 主机在应用中确认并持久化后继续连接，完全独立于系统 known_hosts。

**Architecture:** 后端握手产生绑定真实密钥的短期 challenge，前端确认后调用持久化命令并重试一次。应用拥有共享信任库，连接和重连统一使用它。

**Tech Stack:** Rust、ssh2、ring、uuid、Tauri、React、Vitest。

**Spec:** `docs/superpowers/specs/2026-11-02-ssh-host-trust-design.md`

## Global Constraints

- 简体中文交互；当前分支实施；不引入新依赖；不读写系统 known_hosts；不执行 SQL。
- 认证前校验完整主机密钥，变更拒绝；待确认记录 5 分钟过期且容量有界。
- 文件锁与现有私有文件原子写复用；不接受前端指定公钥或文件路径。

## Review Focus

- 确认和重试间密钥变化必须拒绝。
- 保存后表单卸载仍能完成确认；页面卸载取消。
- 并发确认不覆盖其他已保存主机或不同公钥。
- 系统/历史路径存在也不得绕过应用首次确认。
- 文件损坏、写失败、凭据过期、拒绝确认不继续连接。

### Task 1：后端信任库和 IPC

Files: `src-tauri/src/persistence/ssh_host_keys.rs`、`persistence/mod.rs`、`private_file.rs`、`error.rs`、`commands/connections.rs`、`lib.rs`。

Interfaces: `SshHostKeyStore::new(PathBuf)`；`verify(host, port, key, HostKeyType)`；`get_challenge(Uuid)`；`trust(Uuid)`；`discard(Uuid)`。`AppState.ssh_host_keys` 与 RedisService 共享同一 Arc。

- [x] 先补未知→确认→重启读取、取消/过期、变更、公钥算法/端口、并发确认、损坏/写失败测试，并确认缺少实现时失败。
- [x] 实现 store、错误和上述三个 Tauri 命令。
- [x] 定向 Rust 测试通过并审查信任边界。

### Task 2：SSH 建连和恢复

Files: `src-tauri/src/redis/ssh.rs`、`connection_manager.rs`、`tests/ssh.rs`。

Interfaces: `RedisService::with_ssh_host_keys(profiles, secrets, Arc<SshHostKeyStore>)`；SshTransport 在首次连接和 reconnect 携带同一 Arc。

- [x] 补认证前拒绝未知/变化、公钥确认后恢复、旧路径不再参与、重连持有相同信任库测试。
- [x] 删除系统 HOME 回退，接入 store，迁移已有 SSH fixture。
- [x] 运行 SSH/连接测试及 Rust 全量回归。

### Task 3：前端确认与连接流程

Files: `src/lib/types.ts`、`tauri.ts`、连接功能模块及其测试。

- [x] 先写未知主机确认/取消/失败/变更、测试连接、保存后连接和页面卸载回归。
- [x] 保留 challenge_id，接入三个 IPC wrappers 和一次重试，复用确认框并移除旧 known_hosts 路径输入及提示。
- [x] 定向前端测试、全量测试和生产构建通过。

### Task 4：验收与文档

- [x] 独立审查所有改动并修复有效问题。
- [x] 更新 README 和范围说明，记录测试证据及真实 SSH 验证限制。
- [x] 格式、差异和必要原生构建检查；所有改动保留当前工作区。

## 审查后的连接清理

新增 `ConnectionInfo.connection_token` 和 `close_connection_if_current`，前端丢弃迟到 open 成功结果时按实际发布身份清理；后端将已发布身份与 pending attempt 分开，清理旧连接不影响新尝试，同时使旧缓存和后台工作失效。已覆盖新尝试成功、失败、仍在等待以及页面卸载/作用域变化。

## 最终验证

- 前端：74 个文件、878 项测试通过；生产构建通过。
- Rust：全量 545 项通过、52 项按 fixture 要求默认忽略；其中另行实测 6 项隔离 SSH 流程全部通过。
- macOS：`npm run tauri -- build --debug --no-bundle --ci -- --locked` 通过，产物 `src-tauri/target/debug/redix`。
- `cargo fmt --check`、`git diff --check`、非 Cloud 范围检查通过；独立审查未发现遗留 P1/P2。
- 保留既有 Rust 未使用代码、测试 support helper、前端大 chunk 与 JSDOM 导航提示。Windows/Linux、SSH 密码与 Agent 的真实认证、本机桌面完整交互 E2E 本次未实测。
- 全部改动保留在当前 main 工作区；未提交、推送或发布。测试临时 SSH/Redis 服务与密钥已清理。
