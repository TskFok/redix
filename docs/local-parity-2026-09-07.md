# 2026-09-07 本地功能补齐记录

对比基线：Redix `67318ae`，只读参考 RedisInsight `48ee19fab`。沿当前 `main` 修改，未创建分支、提交或推送。Redis Cloud 与既有设计永久排除项不进入产品。

## 已实现

- Browser：原始字节 String 读取/编辑与截断保护；UTF-8/JSON/ASCII/Hex/Binary/Base64，Gzip/Zlib/Deflate；只读 MessagePack/CSharp LZ4、无 schema Protobuf 和 PHP serialized。结构化值在有并发与时间限制的 Worker 内解析，保留64位整数、BOM及二进制 PHP 键。PHP类/引用只展示数据，不实例化。键树分隔符、暂停编辑时的自动刷新、后台批量删除、JSON草稿生命周期修复。
- Stream：SETID、按范围/消费者分页 Pending、高级 Claim（IDLE/TIME/RETRYCOUNT/FORCE/JUSTID）。
- Search：完整 FLAT/HNSW 创建参数；Text/Tag/Numeric/Geo可视过滤；typed二进制PARAMS/KNN、服务器schema验证、FLOAT32/64、K≤200；字段显式AS别名贯通JSON VECTOR创建与查询；修复索引详情初始化废弃查询的竞态。
- 本地产品：连接key/value标签和过滤、版本化查询包导入导出（原子追加和并发写保护）、快捷键搜索面板与导航/聚焦、IME及焦点保护。
- 分析与概览：可取消/总超时/跨页面恢复的后台分析、Top Keys阈值建议、相同参数/节点范围的历史趋势、当前页面120次实例指标趋势。Cluster节点增加版本/模式/键总数/maxmemory；扫描配额按稳定node ID分配，避免节点返回顺序改变抽样范围。
- Workbench：绑定已执行命令的 TimeSeries/Geo本地可视化、序列切换和分页数据表。仅白名单命令按已知返回形状展示，最多2000点、20条序列，名称展示最多256字符；异常数据保留原始结果并解释原因。

## 关键边界

后台删除最多10000个去重键、4个并发任务、20条会话记录。取消不能撤回在途命令；失败/超时项可能已经生效。Cluster按原始槽位表向直连主节点发送一次UNLINK，不通过会自动重试写入的Cluster路由连接重发。

后台分析最多2个并发任务、16条记录，每个连接/数据库同时1个；默认总超时300秒，最多900秒；报告内存上限1MiB。取消、断连与切库停止后续读取，已发出的命令可能在服务端完成。Standalone沿原算法处理最后一页，因此处理数可能略超过配置上限；Cluster遵守各节点配额。任务与结果默认不落盘，显式保存历史才持久化。

String读取4MiB并检测额外哨兵字节，截断值禁止整体保存；普通读取不需要EVAL权限，写入用原子校验保留TTL。结构化Worker最多2个并发、2秒、输出8MiB；MessagePack库内部解析前的分配仍不能由解析后的节点限制完全约束。Redis客户端原始RESP读取也未实现传输层分配限额。

Geo图表是经纬度散点，不提供地图底图；时间序列暂支持标准RESP数组形状，其他形状保留原始结果。可视化不执行插件代码或请求远程服务。

## 验证

| 检查 | 本轮结果 |
|---|---|
| 全量前端 | 54文件、420项通过 |
| TypeScript/Vite生产构建 | 120模块通过；主包525.74kB触发Vite体积提示，未通过提高阈值隐藏 |
| 全量Rust | 420项通过、37项默认ignored；ignored未记作通过 |
| 隔离普通Redis | 11个真实流程通过；另3个Stack分支因无环境early-skip |
| 隔离Cluster | 三主节点4个真实流程通过：路由/扫描/拓扑/分析、socket状态命令门控、后台删除、后台分析与同步报告一致 |
| 测试launcher单测 | 17项通过（Stack 11、Cluster 6） |
| macOS原生release | `CARGO_NET_OFFLINE=true npm run tauri:build -- --no-bundle`通过，Rust release编译3分46秒；未打包、签名、安装或发布 |
| 代码格式与范围 | non-cloud、cargo fmt --check、git diff --check通过 |
| 界面检查 | 本地Vite预览检查快捷键面板布局、搜索、Enter导航与焦点恢复；无Tauri IPC，不能当作真实Redis桌面端到端 |

真实Cluster测试先发现后台/同步配额不一致，再按节点ID固定排序，保持完整报告断言后通过。其他审查修复包括只读ACL、LZ4非法回溯、Protobuf整数精度、PHP BOM/二进制键、标签原型属性、图表极值/长数字/长名称，以及连接切换期间遗留switching状态。

Redis Search模块实测未执行：本机无现成镜像，Docker拉取官方Redis 8.4.5时注册中心报认证失败，临时空配置匿名拉取同样失败。未创建容器、修改用户Docker凭据或使用用户现有Redis。新增 `npm run test:redis:stack` 脚本保留官方镜像默认启动、loopback随机端口、tmpfs数据、资源上限、nonce/ID/run_id身份核验和清理；单测通过不等同模块实测通过。真实Search用例覆盖HASH/JSON、FLAT/HNSW、FLOAT32/FLOAT64和AS别名。

未运行真实sshd、Windows/Linux原生或完整桌面端到端；已有CI配置不代表本次已经执行。

## 尚未完成的对齐项

完整状态见 [功能矩阵](redisinsight-feature-matrix.md)。仍包括：按模式后台全库操作与后台导入导出/重启恢复，Java/Pickle/schema Protobuf及更多值格式/压缩，完整推荐策略，长期监控，按索引查询库分区，通知/自动更新，完整本地插件能力；Cluster+SSH及带节点作用域的运维观察仍有边界。此次功能实现不能称为与RedisInsight全量等价。
