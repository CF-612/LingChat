# PR #791 复盘与评审反馈（issue #784）

> 分析对象：`SlimeBoyOwO/LingChat#791` — 分支 `fix/persistent-memory-phase1-main`，base `main`，7 个提交，**尚未合并**（无 review，1 条 maintainer 评论）。
> 分析基准：PR 头部 `58fa42b0`，merge-base `ef8f7914`（2026-09-01）；`main` 最新为 2026-09-13。
> 下文行号除标注「旧」外，均指 **PR 分支版本**；标「旧」的指 `origin/main`。源码树已解包到临时目录逐文件核对。

---

## 0. 结论先行

1. **这个 PR 不是"改一个 bug"**：它把 issue #784 的三个 bug 修复、永久记忆模块重写、剧本试玩/存档隔离、以及一整套离线验证基建（HTTP 验证服务 + 测试脚手架 + 2 个 node 脚本）打包在了一起。**测试代码占了 58%**，这是它 +8K 行的主因。
2. **三个 bug 的修复本身方向是对的**，而且 `HistoryChange` 那层语义显式化确实是治本的做法（把"追加/重写/替换/试玩/读档"分开是旧代码最缺的东西）。
3. **但它夹带了 4 处与 bug 无关的对外契约/行为变更**（含一处前端序列化契约变更、一处用户可感知的 UX 回退、一处工具链路性能回退），这些必须拆出来单独评审或回退。
4. **分支未 rebase，落后 `main` 38 个提交**，且改动集中在 `Cargo.toml`/`lib.rs`/`init/mod.rs`/`game_status.rs`/`role_manager.rs`/`api/{chat,game,save}.rs` 等热点文件，合并风险高。
5. **PR 的 7 条提交信息里只有 1 条写了正文**，且没有任何地方写清"旧代码哪里错了"。所以 review 看不过来不是审阅者的问题，是 PR 本身缺少根因说明。第 7 节给了可直接回帖的 comment 草稿。

### 规模构成（三项 diff：`git diff --numstat origin/main...origin/pr791`）

| 部分                                         | 文件数 | 新增     | 删除     | 占比         |
| -------------------------------------------- | ------ | -------- | -------- | ------------ |
| 产品代码（`src-tauri/src/**`、`src/**`）     | 41     | 3599     | 1946     | 42% 新增     |
| `test/` 测试树                               | 22     | 4729     | 0        | **56% 新增** |
| ├─ `test/memory/tests/*`（用例）             | 6      | 2964     | 0        |              |
| ├─ `test/memory/*`（脚手架 + HTTP 验证服务） | 7      | 1373     | 0        |              |
| └─ `test/scripts/*.mjs` + fixtures + README  | 9      | 392      | 0        |              |
| 内联单测 `message_system/ordering_tests.rs`  | 1      | 146      | 0        | 2%           |
| **合计**                                     | **64** | **8474** | **1946** |              |

> ⚠️ 用**两点 diff**（`git diff origin/main origin/pr791`）会得到 `+11793/−5818` 并显示"删除了 `llm/vision.rs`、`utils/image.rs`、`.husky/pre-commit`、`docs/tips.md`，版本号 0.5.2→0.5.1"等一大堆东西。**这些都是 main 在 9-01 之后自己加的、本分支没跟上的内容**，不是本 PR 的改动（已用 `git cat-file -e origin/pr791:<path>` 逐个确认三方 diff 为空）。评审时务必用三点 diff，否则会误判 PR 在删 main 的功能。

---

## 1. 根因一：长期记忆压缩后人设（system_prompt）丢失

### 1.1 机制

人设不是独立字段，而是 **`line_list` 里的一条普通台词**（`attribute = System`、`sender_role_id = 角色 id`），由 `api/game.rs` 在角色上场时插入，且用"表里已有本角色 System 行就跳过"来防重（旧 `api/game.rs:641-660`）。

发给 LLM 的上下文重建路径（旧 `role_manager.rs:335-346`）：

```
slice_start = bank.meta.last_processed_global_idx 往回数 recent_window 条可见台词
sliced      = source_lines[slice_start..]            ← 人设在 index 0，压缩一次就被切掉
has_prompt  = find_first_system_prompt(&sliced)
if !has_prompt { 回退到 find_first_system_prompt(source_lines) 重新插到 0 }
else           { tracing::warn!("没有找到 SYSTEM 属性的台词，可能人设丢失") }
```

关键缺陷有两层：

1. **`find_first_system_prompt` 只认 `System + sender_role_id == Some(role_id)`**（旧 `role_manager.rs:664-668`），所以一旦人设行**不在传入的列表里**，回退也找不到——而人设行的存在完全依赖 `line_list` 当前的内容。
2. **`line_list` 会被整体替换**：读档（`api/save.rs`）、回滚截断、剧本试玩（preview）、章节切换都会重写历史。旧代码只有一个 `sync_memories(db, lines, recent_n)` 入口，**不区分"追加"还是"重写"**（旧 `game_status.rs:111-114` 恒传 `None`；全仓 `sync_memories` 只有这一个调用点）。

**人设彻底缺席的充要条件**是：重建时 `final_sliced` 里没有该角色的 System 行，且 `find_first_system_prompt(source_lines, rid)` 也找不到（否则会被补插回 index 0）。此时 `merge_memory_bank_into_context`（`role_manager.rs:608-660`）会把记忆库插到 system 位——注意 `render_system_memory` **恒返回非空**（空 bank 也带 `====== 记忆库 ======` 头尾），所以 `use_mb` 恒为真、且没有"空 bank 不注入"的闸门——prompt 于是就**只剩长期记忆内容**，正是反馈里的描述。旧代码在这条路径上只打一条 `tracing::warn!("role_id={} 没有找到 SYSTEM 属性的台词，可能人设丢失")` 就继续跑。

两条可达路径（**推断，需作者确认实际复现的是哪条**）：

1. **角色从未写入过 System 行**：只有被 `perceived_role_ids` 牵扯进来、却从未"入场上台/写人设"的角色（多 NPC 自由对话的配角、剧本 NPC）。这类角色**每一条**上下文都无人设，记忆库一进来就占据 system 位。
2. **窗口裁剪 + 历史替换叠加**：压缩把指针推过 index 0，切片里没有人设；若此时历史被整体替换（读档/试玩/回滚），连兜底的查找源里也没有人设行。

需要说明的是：在 main 上 `refresh_memories` 恒传 `None`（`game_status.rs:113`），兜底查找源就是全量历史，**只有历史本身不含人设行时才会失效**。我没能在 main 上构造出"纯追加聊天 + 压缩"就丢人设的路径；如果作者手上有复现步骤，请贴出来。

### 1.2 修复

- 回退源从"可能被截断/替换的 `source_lines`"改为 **canonical 全量历史 `lines`**（`role_manager.rs:391-397`）。
- 新增 `HistoryChange`，把"追加/重写/替换/试玩/读档"显式区分（见第 3 节），历史被替换时**先重置该角色的记忆指针**（`rewrite_memory_history(from_idx)`，`role_manager.rs:305-307`），而不是拿着旧指针去切新历史。
- 新增回归用例 `test/memory/tests/compression.rs:134` `production_append_preserves_persona_across_two_compression_windows`，断言压缩两轮后 `memory[0]` 仍是含人设的 system 消息（`:244-249`）。
- **但没有给人设行本身不存在的场景补兜底**：仍是"找不到就 warn 一句继续"，测试断言的也只是"人设存在时必须存活"（`compression.rs:250-254/316-320`）。第 1 条路径（从未写入人设的配角）在修复后依然无人设——**这是修复范围内的遗漏，应明确是否需要一并处理**。

---

## 2. 根因二：阈值设为 250 时压缩从不触发

### 2.1 先排除一个误判

**触发判定与计数逻辑新旧完全一致**，不是这里的锅：

- 计数：旧 `persistent_memory_system.rs:634-639` vs 新 `memory/runtime.rs:416-423`，逐行等价（都是 `line_visible_to_role` 过滤后 `.count()`）。
- 阈值比较：旧 `persistent_memory_system.rs:399` 与新 `memory/runtime.rs:269` 都是 `if visible_count < self.update_interval { return; }`。
- 可见性规则：`System` 行不算、空内容不算、必须 `sender_role_id == role_id` 或 `perceived_role_ids` 含该角色（旧 `persistent_memory_system.rs:675`，新 `memory/context.rs:15-19`）。

**已排除的机制**：我逐条核对了 `invalidate_memory_history()` 在 main 上的全部调用点——`generator.rs:297`（就地改写 user 行）、`generator.rs:575`（工具回填的中间插入）、`service.rs:180`（清空状态）、`service.rs:201`（读档替换历史）、`api/chat.rs:326`（截断历史）、`api/script_editor/commands.rs:1568/1627`（试玩）——**全部是真正的"历史重写/替换"，没有一个是普通追加**。旧 `GameStatus::add_line` 本身不调用它。所以"追加台词把在跑的压缩判为过期、导致指针永不前进"这条看着很像的机制**不成立**，不必再往这个方向找。（同理，`sync_memories` 的 `recent_n` 截断在 main 上恒传 `None`，是死参数，也不是 live 根因。）

### 2.2 单位口径是第一个坑

设置项文案是「**触发摘要的可见台词数**（1–10000，默认 250）」和「压缩后保留的**角色可见台词数**（0–10000，默认 30）」（`src/locales/zh-CN/settings.ts:228-231`）。也就是说：

- 阈值单位是**该角色可见的非 system 台词**，不是"消息条数"；
- 多角色剧本里，别的角色的台词、`System` 行、该角色感知不到的台词**都不计数**。

用户按"250 条消息"的直觉去理解，实际要攒到 250 条**本角色可见台词**，体感上就是"迟迟不触发"。这一点至少需要在 UI 文案 / 文档上说明。

### 2.3 候选根因：运行时不存在的静默路径

旧 `ensure_memory_bank_system` 在 LLM 槽位为空时**直接 return，连运行时都不插入**（旧 `role_manager.rs:394-404`）：

```rust
if self.llm.try_read().map(|g| g.is_none()).unwrap_or(true) {
    if enabled { tracing::warn!("...永久记忆已开启但 LLM 槽位为空"); }
    return;                       // ← 不 insert
}
```

而 `sync_memories` 的后续逻辑是 `self.memory_bank_systems.get(&rid)` → `None` → `mb_exists = false` → **既不触发压缩、也不合并记忆文本**。配合旧代码里 `enabled` 在构造时就固定（后续不会重算），只要运行时是在"配置还没就绪 / 开关还没打开"的时刻被创建的，它就会一直停在 disabled 上。

新代码把这条路堵住了：`MemoryCoordinator::ensure` **无论如何都插入运行时**，并在每次调用时重算开关（`coordinator.rs:65-96`，含 `set_enabled(self.config.enabled && llm_ready)` 和一条明确的 warn）。

> **需要作者确认的部分**：我没有在真机上复现 issue #784，上面是读代码得到的**候选机制**（2.2 的计数口径 + 2.3 的静默 disabled）。这两条都能独立造成"压根没触发过"，但**哪一条是实际发生的，PR 里没有任何说明** —— 这正是第 7 节要问作者的第一件事。新代码两侧都收紧了，所以现象会消失，但根因结论仍需作者给出。

---

## 3. 根因三：工具调用流式输出顺序错乱

### 3.1 根因

**这是本次唯一"两端都有份"的 bug**，旧实现有三层叠加：

1. **Rust 侧两条 emit 通道之间没有任何顺序关系（主因）**。回复走的是三段异步流水线：`producer → consumer 池（并发，做解析/翻译/TTS）→ publisher(tokio::spawn) → app.emit("ai:reply")`；而工具事件是在 `tool_loop` 的 async_stream 里**裸调 `app.emit`**（旧 `tool_loop.rs:221`）。前导台词此刻还卡在 consumer 里做**翻译 LLM 往返 + TTS 合成**（新 `generator.rs:960-977` 可看到该段），工具卡片必然抢跑。
2. **前导文本被情绪 tag 切分扣住**。`StreamProducer` 只在出现完整 `【…】` 情绪 tag 时才切句，流里另外的 flush 点只有 EOF（`producer.rs:132-209`）。模型前导文本若不带情绪 tag，就一直躺在 buffer 里，直到工具后的第二轮文本到达才被一起切出去 → 表现为"工具调用在前、前缀在后"。
3. **前端 `ai:tool_activity` 绕过 EventQueue 直接调用**（`src/api/tauri-events.ts`，与 `ai:reply` 走 `eventQueue.addEvent` 不同）。`EventQueue` 是纯 FIFO 串行、**没有任何优先级机制**（已通读 `src/core/events/event-queue.ts` 确认），所以即便 Rust 侧顺序对了，工具 UI 仍可能先落地。

### 3.2 修复策略：把"呈现顺序"变成显式契约

- **Rust 侧加发布栅栏**：新增 `PresentationChunk::BeforeTools(oneshot::Sender<bool>)`（`producer.rs:29-35`），由 `tool_loop` 在**执行任何工具之前**插入并等待 ack（`tool_loop.rs:274-283`）。栅栏在 `producer` 里也占一个序号（`SentenceItem::BeforeTools{index, ack}`，`producer.rs:40-46`），consumer 原样透传，`publish_ordered` 按 index 重排，**遇到栅栏时先等所有更小 index 处理完再回传 ack**（`generator.rs:712-760`）。于是"前端已收到全部前导台词"与"工具开始执行"被线性化。
- **producer 侧栅栏到达时强制 flush**（`producer.rs:239-260`）：把 pending 句和前导文本作为**非 final** 前导句推出，这一步同时修掉了 3.1 的第 2 层。
- **前端加"呈现前沿"**：`ai:tool_activity` 的 started 事件带上 `wait_for_reply`，`EventQueue` 新增按事件独立的 promise，在 `processEvent` 返回后立即 resolve（不等点击、不等音频），工具三连事件挂到同一前沿之后（`src/core/events/event-queue.ts:14-38,120-128`；`src/api/services/tool-settings.ts:219-243`）。
- **测试**：`ordering_tests.rs`（5 例，`#[cfg(test)]` 门控正确，生产不编译）+ `test/scripts/verify-tool-order.mjs`（7 例）。

---

## 4. 修复策略总览（记忆侧）

| 旧                                                                                                                                                                             | 新                                                                                                                                  | 解决的问题                                                         |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `game_system/persistent_memory_system.rs`（859 行，混在游戏系统里）                                                                                                            | `ai_service/memory/{mod,config,context,compactor,coordinator,runtime}.rs`                                                           | 记忆不再是 `GameRoleManager` 的一部分                              |
| `GameRoleManager.memory_bank_systems: HashMap<i32, PersistentMemorySystem>`（旧 `role_manager.rs:30`）                                                                         | `MemoryCoordinator` 独占运行时（`coordinator.rs:20-24`）                                                                            | 单一所有权；`GameRoleManager` 无法再变成 MemoryBank 状态机的持有者 |
| `GameRole.memory_bank: GameMemoryBank` 字段（内存里第二份副本）                                                                                                                | **删除该字段**（`ai_service/types.rs`）                                                                                             | 消除"运行时 bank"与"角色内 bank"两份数据                           |
| `sync_memories(db, lines, recent_n: Option<usize>)`，全仓只有 1 个调用点且恒传 `None`（旧 `game_status.rs:113`）                                                               | `sync_memories(db, lines, change: HistoryChange)`（`game_status.rs:22-38`）                                                         | 追加/重写/替换/试玩/读档语义显式化                                 |
| 试玩与正式共用一条历史刷新路径                                                                                                                                                 | `refresh_memories_for_change` 作为**唯一**预览边界，预览态下所有变更统一翻译为 `HistoryChange::Preview`（`game_status.rs:349-365`） | 试玩不再重置/提交正式会话的 MemoryBank                             |
| 压缩触发只看得到调用方给的那份列表                                                                                                                                             | 触发与切片**恒看 canonical 全量历史**（`role_manager.rs:365-372` 有明确注释）                                                       | 避免"窗口切片被当成全量历史"                                       |
| `ensure_memory_bank_system` LLM 未就绪时不插入                                                                                                                                 | `MemoryCoordinator::ensure` 总是插入并重算 enabled（`coordinator.rs:65-96`）                                                        | 消除静默 disabled                                                  |
| 空压缩结果按成功处理                                                                                                                                                           | 空响应视为失败，不推进指针（`compactor.rs:110-114`）                                                                                | 避免静默丢弃一批对话                                               |
| 记忆段超长直接截断喂给 LLM 且无提示                                                                                                                                            | 截断时打 warn 说明超限尾部会被丢弃（`compactor.rs:87-99`）                                                                          | 可观测性                                                           |
| 压缩 prompt 里**没有角色名**：`update_section` 的形参是 `_ai_name`，从未被使用（旧 `persistent_memory_system.rs:591`），但提示词的通用规则却要求"用（本AI角色的名字）第三人称" | 加入 `【角色名称】：{ai_name}`（`compactor.rs:101-104`）                                                                            | 摘要张冠李戴，是"越推越错"的合理贡献因素                           |

数据层：`db/managers/memory_repo.rs`（typed 化，−23 净行）、`save_repo.rs`（−43 净行，收敛会话快照）——**没有新增 migration**，老存档沿用 `memory_bank` 表结构。

---

## 5. 风险清单（按严重度，评审必须逐条过）

### 🔴 高：前端序列化契约变更，且是被新测试"倒逼"出来的

`db/entities/line.rs` 给 `LineAttribute` 加了：

```rust
#[serde(rename_all = "lowercase")]
pub enum LineAttribute { User, Assistant, System, Tool }
```

- 旧行为：序列化成 `"User"` / `"Assistant"` / `"System"` / `"Tool"`（serde 默认用 variant 名）。
- 新行为：`"user"` / `"assistant"` / `"system"` / `"tool"`。
- **改动动机不是 bug 修复**：新写的 fixture `test/fixtures/memory/multilingual.json` 里写的是 `"attribute":"user"`，而 `test/memory/tests/persistence.rs:38-46` 用 `serde_json::from_value::<GameLine>` 去解析它 —— **不加 rename 这个测试就 parse 失败**。即"测试数据写法决定了产品序列化格式"。
- **消费点目前是混用的**：`src/stores/modules/game/actions.ts:258-273` 全用**小写**比较；`src/utils/function.ts:8-31` 大小写**都判**（防御式写法）；`src/api/services/history.ts:9` 的注释还写着 `"NORMAL", "SYSTEM"`。这条链路历史上就是乱的。
- **最严重的后果是 LAN 同步（已核实）**：`lan_sync/db_sync.rs:90` 用 `export_table::<line::Entity, line::Model>(&db, "line")` 把**整个 `line` 表通过 serde 导出成 JSON**，并暂存为 `.lan_sync_staging/db_records.json`，**下次启动才导入**（`lan_sync/staging.rs:57-79`）。跨版本的 LAN 同步会在导入侧直接解析失败（`unknown variant 'system'`），而且"旧版本写入的暂存文件 + 升级后首次启动"这条最常见的升级路径正好命中。这是**数据级的不兼容**，不是 UI 显示问题。
- **要求**：作者必须说明 (a) 这个 rename 是不是本 bug 修复必需；(b) 逐个列出 `attribute` 的全部消费点及结论（IPC 前端、插件、LAN 同步、任何 JSON 快照）；(c) 若旧值曾随存档/同步落盘，给出兼容处理。**若只说"顺手改的"，建议拆出本 PR 单独评审。**

### 🔴 高：工具执行现在要等前导台词的翻译 + TTS

栅栏 ack 由 publisher 在 `consume_sentence` 完整跑完后才回传，而 `consume_sentence`（`generator.rs:829`）内部含 `translate_segments_to`（`:963`）和 `generate_voice_files`（`:974`）的 TTS 合成。**结果是每次工具调用都要等前导台词的翻译与 TTS 完成才开始执行**，原来是可并行的。开翻译或本地 TTS 时工具链路端到端延迟会明显上升；且 `fence_rx.await` **没有超时**，翻译 provider 卡住则工具永远不执行。

### 🟠 中：缺失 index 会挂死整轮（推断，机制明确）

consumer 池并发执行，`publish_ordered` 按 index 严格递增推进。若某个 consumer **panic** 且没有把该 index 以 `response: None` 交回 publisher，栅栏的 `oneshot::Sender` 会卡在 `pending` map 里既不发也不 drop → `tool_loop` 的 `fence_rx.await` 永久 pending。旧代码丢一句只是少一句话，新代码是整轮挂住。建议给 `fence_rx.await` 加超时并在超时后 fail-open/fail-closed 二者中选定一个明确语义。

### 🟠 中：用户可感知的 UX 回退

模型在工具后**原样复读前导台词**是本项目自己文档化过的常见现象（`producer.rs:104-106` 有注释）。旧代码会把前导再当 final 发一次让本轮收尾；新代码在此判定 `sent_final=false` → 走 `emit_error("模型没有返回完整的最终内容，请再试一次")` 并重置输入态（`generator.rs:695-705`）。用户体感可能是"台词明明显示了，却弹错误提示"。这个行为变更被测试固化了，但**属于产品决策，不该由 bugfix PR 单方面决定**。

### 🟠 中：一条坏记忆行会让整个存档打不开

`load_memory_banks_from_db` 新增的 typed 读取对"最新行 JSON 损坏"**直接报错**（旧实现是 `serde_json::from_value(..).unwrap_or_default()` 静默降级为空 bank），而 `api/save.rs:268-275` 把 `restore_memory_banks` 的错误从 `let _ = ... eprintln!("[SAVE_WARN] ...")`（旧 `api/save.rs:245-248`）改成了 `.map_err(...)?`。于是：**只要有一条损坏的记忆行，用户就从"记忆退化为空"变成"存档载入失败"**；而且该函数是在 `for` 循环里 `?`，一个角色报错会**中断其余角色的记忆恢复**。容错方向从"降级"变成"致命"，需要明确这是有意为之。

### 🟠 中：任何触及已处理前缀的重写都会整库清零

新增的 `rewrite_from`（`runtime.rs:190-198`）在 `from_idx < last_processed_global_idx` 时执行 `state.bank = GameMemoryBank::default()`——**把整个记忆库清零，而不是像旧代码那样只作废在跑的任务**。触发条件是任何"回退到较早台词再编辑"（剧本编辑器回滚、`truncate_lines`、工具回填）。而下一次自动存档会用 `memory_bank_snapshots()` 把这个空 bank **写回数据库覆盖原行**，即不可恢复。这是**语义变更而非修复**，建议至少保留 `long_term`/`user_info` 或给用户提示。

### 🟠 中：分支未 rebase（落后 main 38 个提交），会静默吃掉 main 的成果

改动集中在最热的文件上（与 main 侧 21 个文件重叠），且 PR 顺带改写了 `GameStatus::add_line` 的语义。已确认的实例：main 在分支点之后给 `MemoryBuilder` 加了 `with_continue_user()` 并在 role_manager 传参（main `memory_builder.rs:29`、`role_manager.rs:349`，即 `memory_inject_continue_user` 那条 Gemini "首条必须 user" 的兼容），而 **PR 树里完全没有这两个东西**（`git grep` 在 `origin/pr791` 上零命中），PR 的 `role_manager.rs:397` 是 `MemoryBuilder::new(rid).build(...)` 不传参。合并时若按 PR 侧消解冲突，**配置项还在、但没人读它**，功能静默失效。同理还有 #774（`sanitize_tool_pairing`）、#803（自动存档只在真实对话变化时触发）、原生多模态识图整条线。**建议先 rebase 再评审**，否则 review 的是两周前的代码。

### 🟡 低：测试专用 API 泄漏到产品路径

- `role_manager.rs:449 / 629 / 636 / 732`（`wait_memory_updates`、`memory_system_text`、`memory_short_term_text`、`memory_as_json`）**无条件 `pub`，且 `src/` 下零调用者**，唯一调用方全在 `test/`。应改为 `#[cfg(feature = "memory-test-api")] pub(crate)` 或 `#[cfg(test)]`。
- `auto_save.rs:242-261` 的 `for_test` / `perform_test_save` / `test_saved_revision` 已用 feature 门控（可接受），但 `for_test` 的存在只是为了绕过字段私有，属于"为测试开 pub"。
- 正面例子：`coordinator.rs:59-62 insert_for_test`、`runtime.rs:138 history_revision_for_test`、`ordering_tests.rs` 都是正确的 `#[cfg(test)]` 写法。**生产构建不会被编进测试代码**（已验证 `[[bin]] required-features` + `lib.rs:16-18` 的 `#[cfg(feature)]` 双重隔离）。

---

## 6. test 模块：冗余审计与"必要集合"

### 6.1 现状

- 47 个测试函数集中在 `test/memory/tests/`（**其中 0 个是零断言空壳**——问题不是造假，是同一路径被 3~4 层重复断言）＋ 生产源码内联的 15 个。
- **不在 CI 里**：全仓 `.github/workflows/*` 搜不到 `cargo test`、`memory-test-api`、`verify-*.mjs` 任何一项；最后一个提交还带 `[skip ci]`。也就是说这 4729 行**目前只有作者本机跑过**，本次提交连 `cargo build` 门禁都跳过了。
- 引入成本：`[[bin]] path = "../test/memory/main.rs"` + `lib.rs:16-18` 的 `#[path = "../../test/memory/mod.rs"] pub mod memory_test_api` + 生产源文件 `auto_save.rs:90-92` 用**四层 `../`** 反向挂载测试文件。这套结构本身是 review 负担。
- 运行成本：`cargo test --lib --features memory-test-api` 是独立 feature 指纹，`target/` 缓存与常规构建不共享，Windows 冷编译数分钟级；用例本身不慢（`compression.rs:134` 跑 500 次 `append_line` 是最重的一个）。

### 6.2 建议删除（保守口径，约 −400 行）

| 位置                                                                                    | 内容                                                                  | 理由                                                                                                                                                                                                                                                                        |
| --------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test/memory/tests/auto_save.rs:39-60`                                                  | `failed_persistence_does_not_advance_success_fingerprint_and_retries` | **已核实**：函数名承诺"持久化失败"，函数体只调纯函数 `fingerprint_requires_save`，一次 DB 都没碰；与 `:19-37` 同构，真正的失败注入在 `:62-176`                                                                                                                              |
| `test/memory/tests/history.rs:37-51`                                                    | 两个 `HistoryChange` enum 匹配臂用例                                  | 只断言 `rewrite_from()` 的 match 结果，保留 `:30-35` 一个即可                                                                                                                                                                                                               |
| `test/memory/tests/history.rs:172-262`                                                  | `entry_greeting_session_*` 与 `ai_dialogue_session_*`                 | 与 `:101-170` 逐行同构（同一个 generation 计数器），其中 permit 部分已被 `:264-377` 更强地覆盖                                                                                                                                                                              |
| `test/memory/tests/compression.rs:17-24` + `harness.rs:327-355`                         | `successful_scripted_provider_*` + `validate_scripted`                | 自证式断言（`scripted_provider.rs:42` 返回 `[scripted:{section}]`，四段必然不同）；`validate_real` 已覆盖真实路径                                                                                                                                                           |
| `test/memory/tests/persistence.rs:22-34`                                                | `memory_bank_round_trips_through_migrated_sqlite`                     | 测的是脚手架自己的 round_trip，与 `:949-974` 重复                                                                                                                                                                                                                           |
| `test/memory/tests/persistence.rs:217-223`                                              | `malformed_memory_json_is_reported`                                   | 只断言 helper 自实现的 `is_err()`，未触达生产路径                                                                                                                                                                                                                           |
| `test/memory/tests/persistence.rs:343-441`                                              | `preview_gate_rejects_load_*`                                         | 与 `history.rs:591-701` 同一条门控，二选一                                                                                                                                                                                                                                  |
| `test/memory/tests/compression.rs:348-374`                                              | `append_during_update_*`                                              | 与 `tests/api.rs`、`verify-memory.mjs`、`api.rs:511-518` 四重重叠                                                                                                                                                                                                           |
| `test/fixtures/memory/{basic,append_during_update,partial_failure,panic,rollback}.json` | 5 个 fixture                                                          | **已核实**：`api.rs:239-328` 对每个内置 scenario 强制覆写 `initial_bank`/`line_count`/`update_interval`/各类失败标志（注释原文 "Built-in scenarios own their inputs"），这 5 个文件的字段实际一个都没被使用（`multilingual.json` 例外，它被 `persistence.rs` 真正反序列化） |

### 6.3 建议整体砍掉：测试专用 HTTP 层（约 −1200 行，test 树瘦身 26%）

`test/memory/api.rs`(670) + `test/memory/tests/api.rs`(213) + `test/scripts/verify-memory.mjs`(160) 三层断言的是同一批数字：`api.rs` 是一个**测试专用的 axum 服务**（第二实现），`tests/api.rs` 的 8 个用例全在测这个服务自身的鉴权 401 / 体积 413 / 单飞 429 / 关停，**不触达任何产品代码**。

`verify-memory.mjs` 里还有两处自欺欺人：

- `:140` 的 `assertions: { utf8: true, ... }` —— **`utf8: true` 是硬编码字面量，不是断言**，该行唯一真实检查是 HTTP 200；
- `:136-138` 把 `multilingual.json` 发到 `/v1/memory/validate`，但 payload 里的 `scenario` 是 `"basic-compression"`，于是内置分支把 `line_count`/`update_interval`/`initial_bank` 全部覆写 —— **这个"多语言校验"实际什么都没验**（真正验它的是 `persistence.rs:38-46`）；
- `:62-66` 硬编码了作者本机的 rustc 绝对路径。

**建议**：保留 `verify-memory.mjs:84-108` 那 30 行做 smoke（能证明"二进制编得出来、能 bind `127.0.0.1:0`、token 握手、能优雅关停"——这四点是 `tests/api.rs` 用 `oneshot()` 覆盖不到的），删掉场景循环、fixture 读取与 multilingual 段落；随之 `api.rs`、`tests/api.rs`、`scenarios.rs`、`test/README.md` 一起去掉。压缩后的**保留核心**是：`temp_db.rs` + `harness.rs:53-197`(`validate_real`) + `scripted_provider.rs` 骨架 + `tests/{auto_save(62-176,178-330), compression(26-346), history(101-170,264-947), persistence(79-974), tool_backfill}`。

### 6.4 `verify-tool-order.mjs`：真校验，但**必须挂入口**

它用 `ts.transpileModule` 编译**仓库里真实的** `event-queue.ts` 与 `tool-settings.ts`，在 `vm` 沙箱里跑 7 个 case（stub 掉 store / tauri invoke / vue / i18n），全部是 `assert.equal` 真断言 —— 不是静态检查、也不是重写一遍逻辑。

**但全仓零引用**：`package.json` 只挂了 `test:memory:prepare` / `test:memory` 两个脚本（都指向 `verify-memory.mjs`），CI 里也搜不到它。**只保留不挂入口 = 等于删除**。建议 `package.json` 加 `"test:tool-order": "node test/scripts/verify-tool-order.mjs"`，并在 `dev-build.yml` 的 PR job 上加一步（纯 node + typescript，不需要 Rust 编译，几秒成本）。

### 6.5 覆盖率真实边界（别被"145 项测试"误导）

- `ordering_tests.rs` 用手写的假流 + 假 consumer 驱动真实的 `StreamProducer` 与 `publish_ordered`，证明的是**两个中间层组件的契约**。若有人删掉 `tool_loop.rs:279` 的 `yield PresentationChunk::BeforeTools`，这 5 个测试**全绿**。
- `verify-tool-order.mjs` 的 `processEvent` 是 stub，验证的是队列的前沿记账，不是真实渲染。
- 作者在提交信息里已如实说明：「真实桌面端与真实模型/TTS 冒烟尚未执行」。**这一点应在 PR 描述里明说，并作为合并前的必做项**（尤其考虑到第 5 节的高危项都只在真实链路上暴露）。

---

## 7. 建议 PR 描述 / 回帖草稿

> 以下可直接贴到 PR #791。目标是把 review 规模从 8.5K 行压到"能看的关键路径"。

---

这个 PR 的修复方向我认可（尤其 `HistoryChange` 把"追加/重写/试玩/读档"分开，是治本的做法），但现在有 8474 行新增、64 个文件，其中**测试代码占 58%（4729 行）**，而且提交信息里 7 条只有 1 条写了正文，我无法从 PR 本身看出"旧代码到底哪里错了"。请先补齐下面三件事，再进入代码评审。

**1）补一段根因说明（对应 issue #784 的三个现象）**，需要落到具体代码路径：

- **人设丢失**：请给出实测复现路径。我读代码的结论是：main 上 `refresh_memories` 恒传 `None`（`game_status.rs:113`），`find_first_system_prompt(source_lines)` 的兜底源就是**全量历史**，所以"纯追加聊天 + 压缩"本身不会丢人设——会丢的前提是**历史里根本没有该角色的 System 行**。请确认你复现的是：(a) 被 `perceived_role_ids` 牵扯进来、却从未写入人设行的角色（多 NPC 自由对话 / 剧本 NPC），还是 (b) 窗口裁剪叠加了历史替换（读档/试玩/回滚）。另外本次修复**没有给"人设行不存在"补兜底**（仍是 warn 一句继续），路径 (a) 修完后依然无人设，请说明这是否在范围内。
- **250 条阈值不触发**：我读代码的结论是**触发判定与计数逻辑新旧完全一致**（旧 `persistent_memory_system.rs:634-639/399` vs 新 `runtime.rs:416-423/269`），所以根因不在这里。请给出实测的触发链断点。我这边有两个候选，需要你确认是哪一个（或都不是）：
  (a) 阈值单位是"该角色可见的非 system 台词"，250 条这个口径与用户"250 条消息"的直觉差很远；
  (b) 旧 `ensure_memory_bank_system` 在 LLM 槽位为空时直接 `return`、不插入运行时（旧 `role_manager.rs:394-404`），导致该角色的记忆系统整个会话都不存在。
- **工具顺序错乱**：请确认是"两条 emit 通道无同步"+"前导文本被情绪 tag 扣住"+"前端绕过 EventQueue"这三层，还是只修了其中某一层。

**2）把与 bug 无关的改动拆出去**，以下四条我认为不属于本 issue 的修复范围，请说明理由或拆成独立 PR：

- `db/entities/line.rs` 给 `LineAttribute` 加 `#[serde(rename_all = "lowercase")]` —— 这会改变所有发给前端的 `attribute` 值（`"User"` → `"user"`）。我注意到是新 fixture `multilingual.json` 写的 `"attribute":"user"` 倒逼的（`tests/persistence.rs:38-46` 会 parse 失败）。前端消费点目前大小写混用（`game/actions.ts` 全小写、`utils/function.ts` 都判、`history.ts` 注释写 `"SYSTEM"`）。**更要紧的是 `lan_sync/db_sync.rs:90` 会把整个 `line` 表用 serde 导出成 JSON 并暂存为 `db_records.json`、下次启动才导入**——跨版本 LAN 同步会直接解析失败（`unknown variant 'system'`）。请说明这个 rename 是不是本 bug 修复必需，并列出全部消费点（含 LAN 同步与暂存文件）的兼容结论。
- 工具执行现在要等前导台词的**翻译 + TTS** 完成（栅栏 ack 在 `consume_sentence` 之后才回传，`generator.rs:829/963/974`），且 `fence_rx.await` **没有超时**。这是性能回退，请给出取舍说明或加超时。
- 工具后"模型复读前导"从"再发一次 final 收尾"改成 `emit_error("模型没有返回完整的最终内容…")`（`generator.rs:695-705`）。这是产品行为变更，需要单独决策。
- `GameStatus::add_line` 语义被改写（不再是 `append_line` 的别名、不再触发 `invalidate_memory_history`）。请说明调用方影响面。
- 记忆读取的容错从"降级"改成"致命"：`load_memory_banks_from_db` 遇到损坏 JSON 直接报错，`api/save.rs:268-275` 又把 `restore_memory_banks` 的错误上抛（旧代码是 `[SAVE_WARN]` 打印后继续）。**一条坏行 = 整个存档打不开**，且循环里的 `?` 会中断其余角色的恢复。请确认是否有意为之。
- `runtime.rs:190-198` 的 `rewrite_from` 在 `from_idx < last_processed_global_idx` 时**把整个 bank 清零**（旧语义只作废在跑的任务），而下一次自动存档会把这个空 bank 写回 DB 覆盖原行。请说明为什么不是"只作废任务"，以及是否会丢 `long_term`/`user_info`。

**3）test 模块按"必要"收缩**（我按下面口径砍到约 3500 行，即当前 test 树的 74%）：

- **整体删除测试专用 HTTP 层**：`test/memory/api.rs`(670) + `test/memory/tests/api.rs`(213) + `verify-memory.mjs` 的场景循环部分（保留 `:84-108` 的 30 行 smoke）+ `scenarios.rs` + `test/README.md`。这三层断言的是同一个自制服务的自身分支，不触达产品代码；`verify-memory.mjs:140` 的 `utf8: true` 是硬编码字面量不是断言，`:136-138` 的"多语言校验"因为 `scenario` 被内置分支覆写而实际什么都没验。
- **删除重复覆盖的用例**：`history.rs:37-51`（enum 匹配臂）、`history.rs:172-262`（与 `:101-170` 同构，permit 部分已被 `:264-377` 覆盖）、`persistence.rs:22-34`、`persistence.rs:217-223`、`persistence.rs:343-441`（与 `history.rs:591-701` 二选一）、`compression.rs:17-24` + `harness.rs:327-355`（自证式）、`compression.rs:348-374`、`auto_save.rs:39-60`（名字说"持久化失败"但一次 DB 都没碰）。
- **删除 5 个惰性 fixture**：`api.rs:239-328` 对内置 scenario 强制覆写 `initial_bank`/`line_count`/`update_interval`/失败标志，`basic/append_during_update/partial_failure/panic/rollback.json` 的字段实际全未被使用（只有 `multilingual.json` 被真正反序列化，保留）。
- **保留**：`tests/tool_backfill.rs`（唯一覆盖 tool 配对顺序）、`auto_save.rs:62-176`（真 SQLite + 真失败注入）、`compression.rs:134`（250 阈值 + 人设存活，唯一的回归用例）、`persistence.rs` 的 typed repo 往返、`history.rs` 的 preview/session 语义、`runtime.rs` 的内联单测。
- **`verify-tool-order.mjs` 建议保留，但要挂入口**：`package.json` 加 `"test:tool-order": "node test/scripts/verify-tool-order.mjs"`，并在 `dev-build.yml` 的 PR job 里跑一步。现在它全仓零引用，等于死代码。

**4）另外两点流程性要求**：

- 分支落后 `main` 38 个提交（merge-base 是 9-01），与 main 侧 21 个文件重叠，请先 rebase 再评审。已经能确定会静默失效的一例：main 的 `MemoryBuilder::with_continue_user()` / `memory_inject_continue_user`（`memory_builder.rs:29`、`role_manager.rs:349`，Gemini "首条必须 user" 的兼容）在 PR 树里**完全不存在**，按 PR 侧消解冲突会让配置项留着但没人读。请一并确认 #774、#803 与原生识图那条线的去留。
- `test/memory` 现在既不在 CI 里（`.github/` 搜不到任何相关入口），最后一个提交又带 `[skip ci]`——等于 4729 行测试和产品改动都没有过一次编译门禁。请在 rebase 后跑一次完整 `cargo check` + `pnpm build`，并把真实桌面端与真实模型/TTS 的冒烟结果贴上来。

---

## 8. 本文的验证边界

**已核实（读代码 + git 命令逐条确认）**：三点 diff 的规模构成；分支落后 38 个提交及两点 diff 的假象来源；`test/memory` 的三种挂载方式与 CI 零接入；`LineAttribute` serde 改名的动机（fixture 倒逼）与 LAN 同步导出路径（`lan_sync/db_sync.rs:90` + `staging.rs:57-79`）；新旧计数/阈值逻辑逐行等价；main 上 `invalidate_memory_history()` 的全部调用点均为真正的历史重写（即 §2.1 排除的机制）；旧 `ensure_memory_bank_system` 的静默 return；旧 `update_section` 的 `_ai_name` 未被使用；`api/save.rs` 记忆恢复错误从 warn 改为上抛；`rewrite_from` 的整库清零；`with_continue_user` 在 PR 树零命中；栅栏 ack 位于 `consume_sentence` 之后（翻译/TTS 之后）；`ordering_tests.rs` 与 `verify-tool-order.mjs` 的实际覆盖边界；`verify-memory.mjs` 的非断言字面量与 fixture 覆写；全部改动过的既有 `src-tauri/*.rs` 通过 `rustfmt --check`。

**未验证**：没有编译、没有运行任何测试、没有在真机复现 issue #784（Windows + Tauri 全量构建成本过高）。因此：(a) 第 1、2 节的根因链是**读代码得出的机制推断**，`2.3` 节列为候选机制，需作者确认；(b) 第 6.2/6.3 节的删减清单来自逐文件审计（引用行号为 PR 分支版本），**删除前请先本地跑一遍 `cargo test --lib --features memory-test-api` 确认**；(c) PR 是否真的编译通过、145 项测试是否真的全绿，本文没有验证，需要作者提供或本地复跑。

---

## 9. 有必要改这么多吗？—— 最小化破坏评估

### 9.1 结论

**不必要。** 把 issue #784 的三个 bug 修到"行为正确且可回归"的状态，大约需要 **700~900 行（含测试）**，是现在 8474 行的 **~10%**。

需要区分一件事：**bug 3（工具顺序）确实无法小改**——顺序问题的根源是"两条 emit 通道之间没有同步点"，不存在一行式的修法，必须引入栅栏/ack 这类机制。这一点作者的判断是对的。但 **bug 1 和 bug 2 都不需要重写**：

| Bug              | 最小修复                                                    | 实际做法                                                                                  |
| ---------------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| bug 1 人设丢失   | 兜底源用全量历史（1 行）+ 给"人设行不存在"补兜底（~20 行）  | 做了前半（`source_lines` → `lines`），**后半漏了**（§1.2）                                |
| bug 2 阈值不触发 | 运行时创建/启用路径的修正（几十行，取决于真实根因）         | 删掉 859 行旧实现、重写 784 行 runtime + coordinator/config/context/compactor（~1200 行） |
| bug 3 工具顺序   | 栅栏 + producer flush + 前端呈现前沿（~300~400 行产品代码） | 做了，约 785 行 churn（含 UX 变更与 clear() 竞态顺带修）                                  |

### 9.2 逐块判定

**✅ 属于修复必需**

- bug 3 的栅栏机制（`PresentationChunk::BeforeTools` + `publish_ordered` + 前端 `wait_for_reply` 前沿）
- bug 1 的兜底源修正、bug 2 的运行时启用路径修正
- 针对这三个行为的回归测试（~300 行足够，即 `compression.rs:134` 那条 + `ordering_tests.rs` 的 5 例 + 少量边界）

**🔀 应该拆成独立 PR（内容有价值，但不属于本 issue，且让回滚粒度归零）**

- `HistoryChange` + preview/session 身份机 + `preview_session_gate`（`game_status.rs` ~200 行）：**这是整个 PR 里设计最好的一块**，值得单独评审、单独合入。它修的是"试玩/读档污染正式记忆"，与 #784 无关。
- typed `MemoryRepo` + `SessionSnapshot` + 指纹式自动存档（`auto_save.rs` 244 行 + `memory_repo.rs`/`save_repo.rs`）：修的是"压缩在台词落盘之后才提交 ⇒ 该次记忆永不入库"，是另一个 bug。
- memory 模块搬迁到 `ai_service/memory/`（消除双份 bank、收敛可见性规则）：**纯重构可以做，但要在 bug 修好之后**，用现有回归测试当安全网。
- `test/memory` 验证基建（4729 行）：更该是独立 PR——而且要先决定它是否进 CI，否则就是死重（§6.1）。

**❌ 应该删掉或回退（不是"修复"，是顺带的决策）**

- `LineAttribute` 的 serde rename（LAN 同步不兼容 + 前端契约变更）
- 工具后复读前导改成 `emit_error`（UX 回退）
- 坏记忆行从"降级"变"致命"（可用性回退）
- `rewrite_from` 整库清零（数据风险）
- 栅栏 `await` 无超时 + 工具执行等前导翻译/TTS（性能回退）

### 9.3 "重写才能治本"这个理由站不住

作者在 PR 描述里写的是"历史债比较重……继续改就会扩大成数据库和会话模型的全部整体重构，目前只能先把问题修好"。但实际交付的恰恰是**重构 + 修复捆在一起**，而且这个组合最大的代价不是行数，是**回滚粒度**：

- 现在想只回退"整库清零"或"容错致命化"——做不到，只能整体接受或整体拒绝；
- 想只评审 `HistoryChange` 这个好设计——做不到，它和 8474 行混在一起；
- 想在下个版本先修 bug、重构下个版本再说——做不到，旧模块已经被删了。

历史债确实存在（旧的 `AtomicU64 history_revision` + 独立的 `std Mutex commit_gate` + `AtomicBool has_pending` + 双份 bank，这套确实是"纯 vibe 代码"），**该还**。但正确顺序是：**先让 bug 修复可独立评审、可独立回滚 → 再发纯搬运/纯重构 PR（行为不变，用回归测试对照）→ 最后删旧代码**。反过来的顺序会让 maintainer 在"接受全部"和"拒绝全部"之间二选一，这正是不该出现在 bugfix PR 里的东西。

### 9.4 如果我来切

| PR                | 内容                                                            | 预估                             |
| ----------------- | --------------------------------------------------------------- | -------------------------------- |
| PR 1（可立即修）  | 三个 bug 的最小修复 + 针对性回归测试                            | ~800 行                          |
| PR 2（纯重构）    | memory 模块搬迁、双份 bank 合一、typed MemoryRepo，行为不变     | ~1200 行，用 PR 1 的测试当安全网 |
| PR 3（功能/基建） | preview 隔离 + `HistoryChange`；验证 harness（先决定进不进 CI） | ~1500 行 + harness               |

---

## 10. 值得肯定（这块必须说清楚，否则作者会以为全盘否定）

1. **栅栏的抽象层级选对了**。没有在前端用延时/`setTimeout` 补锅，而是在事件流里插入一个带 ack 的显式栅栏，让"前端已收到全部前导台词"与"工具开始执行"线性化——这是这个问题唯一正确的解法层。
2. **`publish_ordered` 生产与测试共用同一个函数**（`generator.rs:712`），测试测的是真实实现而不是副本。这是很讲究的做法。
3. **栅栏 fail-closed**：emit 失败时关闭 pending 栅栏、不放行工具（`generator.rs:730-733` + `ordering_tests.rs` 有对应用例），安全侧偏向正确。
4. **`HistoryChange` 把五种历史语义显式化**（`Append`/`Rewrite`/`ReplaceAll`/`Preview`/`Restore`）。旧代码用一个 `Option<usize>` 和一个布尔承载全部语义，这才是"越改越乱"的根源。这是整个 PR 最有价值的产出。
5. **收敛到单一边界**：`refresh_memories_for_change` 是唯一的预览边界，所有变更在 `game_status` 层统一降级为 `Preview`，而不是让每个调用点各自判断（`game_status.rs:349-365`）。
6. **`GameRole.memory_bank` 字段删除**，bank 只留运行时一份，消掉了 `sync_to_role()` 的 `try_lock` 非阻塞同步竞态——双份状态合一，方向完全正确。
7. **把不变量写进注释**。"Compression always sees canonical global history; an Append suffix is only an optimization…"（`role_manager.rs:365-369`）这类注释解释了"为什么"，而不只是"做了什么"。这在同类代码里很少见。
8. **两个真实的静默失败被修掉**：压缩 LLM 返回空串原本按成功处理、会静默丢弃一批对话并推进指针（`compactor.rs:110-114`）；旧压缩 prompt 要求"用（本 AI 角色的名字）第三人称"却**从不告诉模型名字**（旧 `update_section` 的形参是 `_ai_name`），现在加了 `【角色名称】`（`compactor.rs:101-104`）——这条对"越推越错"有实质贡献。
9. **可观测性有改善**：记忆段超长截断时明确 warn"超限尾部将被本次压缩丢弃"（`compactor.rs:87-99`），而不是默默丢。
10. **测试门控干净**：`ordering_tests.rs` 是 `#[cfg(test)]`，`test/memory` 走 `required-features` 的 `[[bin]]`，**生产构建确实不会被编进测试代码**（已逐处核实）。`verify-tool-order.mjs` 敢用 `ts.transpileModule` 编译**仓库里真实的** TS 源码跑断言，而不是重写一遍逻辑——测试写法上是有追求的。
11. **代码格式与注释密度**：所有改动过的既有 `src-tauri/*.rs` 通过 `rustfmt --check`，注释都是指向不变量的说明性文字。工程习惯本身是好的。

---

## 11. 批评

### 11.1 方法层面

1. **范围失控是本 PR 的根本问题**。一个 issue 三个 bug，外加重写一个模块、修两类别的 bug（试玩隔离、快照入库）、重建一套验证基建。后果全部由评审者承担：8474 行里真正的修复不到 10%，review 时必须自己从 90% 的噪声里把它挑出来。**作者应该主动做这个切分，而不是等 maintainer 说"看不过来"。**
2. **根因没写**。7 条提交信息里只有 1 条有正文，且全文没有一处说明"旧代码哪里错了"。这不是格式问题：**没有根因说明，就无法判断修复是否对症**——本文 §2.1 排除了一个看起来很像但实际不成立的机制，如果作者写了根因，这条根本不用排查。**要求作者在 PR 描述里逐条写出"旧代码 → 为什么错 → 新代码为什么对"**，这是评审的入场券，不是可选项。
3. **搭便车改产品契约**。`LineAttribute` 的 serde rename（而且是被新写的测试 fixture 倒逼出来的——测试数据决定产品格式，这个因果方向是反的）、`add_line` 语义、`render_system_memory` 恒非空导致 `use_mb` 恒真、复读前导改为报错。这些不是修复，是决策，必须单独提出并单独评审。
4. **容错方向退化**。bugfix 应当收紧正确性，但这个 PR 同时把可用性和数据安全放松了：一条坏记忆行从"记忆退化为空"变成"存档打不开"；`rewrite_from` 把整库清零且下一次自动存档会把空 bank 写回覆盖。**修 bug 不应该让用户丢数据。**
5. **未 rebase 就提交大改**。落后 38 个提交、与 main 侧 21 个文件重叠，已经能确认会静默吃掉 main 的 `memory_inject_continue_user`（配置项留着、没人读）。大改动前先 rebase 是基本要求。
6. **没有真机验证就声明完成**（作者自己在提交信息里承认"真实桌面端与真实模型/TTS 冒烟尚未执行"）。这一点尤其要紧：本文标为高危的几条（工具执行等翻译/TTS、LAN 同步跨版本、坏行致命化）**全部只在真实链路上暴露**，单元测试覆盖不到。

### 11.2 测试层面

7. **测试量（4729 行）是产品改动量（3599 行）的 1.3 倍，但证据强度并不成比例**。测试规模和可信度是两件事：这里 47 个用例里有一批测的是自己刚写的脚手架（`test/memory/api.rs` 那个 HTTP 服务本身的 401/413/429，不触达任何产品代码），有的是自证式断言（`validate_scripted` 断言 `scripted_provider` 自己生成的四个字符串互不相同），有的是同一路径的第四份重复。
8. **"145 项测试通过"这个说法需要打折看**。`ordering_tests.rs` 用手写的假流 + 假 consumer 驱动真实的 `StreamProducer` 与 `publish_ordered`——它证明的是**两个中间层组件的契约**；删掉 `tool_loop.rs:279` 那行 `yield PresentationChunk::BeforeTools`，5 个用例全绿。真正的端到端证据只有 `verify-tool-order.mjs`（另一端）+ 人工冒烟，而前者全仓零引用、后者未做。
9. **验证基建没进 CI，还带 `[skip ci]`**。4729 行既不在任何 workflow 里，最后一个提交又跳过了 CI——等于这批代码和产品改动**连一次编译门禁都没过**。写了不在 CI 里跑的测试，边际价值远低于它的评审成本。
10. **测试代码污染生产命名空间**：`role_manager.rs` 里 4 个无条件 `pub` 且 `src/` 零调用者的方法（`wait_memory_updates`/`memory_system_text`/`memory_short_term_text`/`memory_as_json`）；生产源文件 `auto_save.rs:90-92` 用**四层 `../`** 反向挂载 `test/` 下的文件；`[[bin]]` 与 `#[path]` 都指向 crate 目录之外（对 Android/iOS/`cargo package` 的影响未验证）。这些都在增加"测试与产品的耦合"，和"最小化破坏"的目标相反。

### 11.3 一句话

这个 PR 里**最有价值的部分（`HistoryChange`、栅栏、不变量注释）**和**最危险的部分（整库清零、容错致命化、契约变更）**都在同一个不可分割的提交里，而真正要修的那个 bug 占不到 10%。**这不是能力问题，是交付纪律问题**：把"我想顺手做的好事"和"我必须做的那件事"分开提交，是这个项目现在最需要的一条规矩。
