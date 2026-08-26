# LumenLP 历史补数方案对比

日期：2026-07-26

## 目标

为 `lumenlp` 的 pools analytics 建立可持续的数据历史。

当前诉求不是“立刻拥有全历史”，而是：

- 先把 indexer 常驻跑起来
- 接受当前只能回补最近一段 Soroban RPC 保留窗口
- 从今天开始持续积累自己的事件、swap、rollup 历史
- 后续如有必要，再补更早历史

## 当前事实

截至 2026-07-26，公共主网 Soroban RPC `https://mainnet.sorobanrpc.com` 的 `getHealth`
返回了有限保留窗口：

- `latestLedger = 63651214`
- `oldestLedger = 63530255`
- `ledgerRetentionWindow = 120960`

对应时间大致是：

- 最新 ledger：`2026-07-26T03:56:52Z`
- 最老可查：`2026-07-18T07:05:25Z`

这意味着：

- `getEvents` / `getTransactions` 只能回补最近窗口
- 公共 RPC 不能作为全历史补数源
- 自建 RPC 若没有额外历史保留能力，也通常只适合作为近历史与实时增量摄取源

## 推荐结论

当前推荐路线：

1. 立刻在 `88.198.16.144` 上常驻运行 `pool-indexer`
2. 接受当前只能回补最近保留窗口
3. 从 2026-07-26 开始长期积累自己的历史库
4. 等产品稳定后，再决定是否补 2026-07-18 之前的历史

这是当前阶段性价比最高的方案。

原因：

- 工程复杂度最低
- 运维成本最低
- 能最快把产品数据面跑起来
- 不会因为追求“全历史”而阻塞当前开发

## 方案对比

### 方案 A：现在就跑，自然积累

方案描述：

- `snapshotter` 继续写 `lumenlp.db`
- `pool-indexer` 常驻写 `pool-indexer.db`
- `api-server` 读取两份库，优先使用 event-driven rollup

优点：

- 最快上线
- 不依赖新外部服务
- 数据质量会随着运行时间自动提升
- 7 天后拥有完整 7d 历史，30 天后拥有完整 30d 历史

缺点：

- 2026-07-18 之前的历史拿不到
- 最初几天 `5m / 1h / 6h / 24h` 指标覆盖仍然偏薄

适用性：

- 最适合当前阶段

### 方案 B：第三方历史数据源补数

方案描述：

- 找可提供更长 Soroban / Aquarius 历史的 API、导出或索引服务
- 将其转换后写入 `pool_events` / `pool_swaps` / `pool_rollups`

优点：

- 能较快补齐早期历史
- 比自建完整 archive 轻很多

缺点：

- 依赖第三方稳定性和字段质量
- 可能有费用、限流、许可问题
- 仍需做字段映射和一致性校验

适用性：

- 适合作为中期补历史方案

### 方案 C：Galexie / Ingest SDK / 自建历史数据管道

方案描述：

- 使用 Stellar 官方推荐的数据获取路径
- 从 ledger metadata / archive 数据构建自己的 analytics pipeline

优点：

- 路线正确，适合长期演进
- 比直接维护 validator archive 更贴近 analytics 需求
- 更适合未来做多协议、多维度数据层

缺点：

- 工程复杂度明显上升
- 需要单独的数据存储、加工、回放流程
- 不适合当前产品阶段立刻投入

适用性：

- 适合作为中长期基础设施路线

### 方案 D：自建 archive node / 发布完整 history archive

方案描述：

- 自己运行 Stellar Core
- 自己发布完整 history archive
- 再基于 archive 或 replay 构建历史补数

优点：

- 自主性最强
- 理论上可获得完整历史控制权

缺点：

- 最重的方案
- 维护的是“网络历史基础设施”，不是直接的产品能力
- history archive 最终会是 TB 级长期增长
- 更适合 validator / infra 团队，不适合当前 `lumenlp` 阶段

适用性：

- 当前不推荐

## 磁盘与成本判断

### 仅运行 Core / RPC 实时节点

官方当前对本地状态的量级描述大致是：

- buckets：`20–40 GB`
- metadata SQL：几 GB

实际部署建议预留：

- `100–200 GB SSD`

这适合实时 RPC，但不等于“拥有完整历史补数能力”。

### 运行完整 history archive

archive 的重点不在本机 SSD，而在外部对象存储：

- S3
- Cloudflare R2
- Backblaze B2
- 其他兼容对象存储

长期容量会增长到 TB 级。

这对当前 `lumenlp` 来说过重。

## 当前推荐实施路径

### 第一阶段：现在就跑

- 在 `88.198.16.144` 上常驻运行 `pool-indexer`
- 让 `api-server` 读取 `pool-indexer.db`
- 接受仅有最近保留窗口的回补能力
- 从今天开始沉淀长期历史

### 第二阶段：观察两周

- 观察数据完整性
- 观察 `/pools` 的排序与指标体验
- 观察 indexer 扫描成本与 RPC 压力

### 第三阶段：如确有必要，再补早期历史

优先级建议：

1. 第三方历史数据源
2. Galexie / Ingest SDK 数据管道
3. 自建 archive

## 决策

当前确定采用：

- 方案 A：现在就跑，自然积累

当前明确不做：

- 为了 `lumenlp` 当前阶段而先上 archive node

## 后续工作

- [ ] 在 `88.198.16.144` 上部署并常驻运行 `pool-indexer`
- [ ] 验证 `api-server` 读取 `pool-indexer.db`
- [ ] 增加 indexer 状态监控：cursor、event_count、rollup 更新时间
- [ ] 两周后再评估是否需要补 2026-07-18 之前历史
