# Memory OS — Markdown Sync 用户手册

让 uClaw 的 EntityPage 在磁盘上有一个真实可编辑的镜像 ——
`~/Documents/workground/brain/<subkind>/<slug>.md`。在 Obsidian /
VS Code / Logseq / Foam 里直接改文件,改完点 **Sync**,内容回到
uClaw 的 wiki。或者把 `brain_watcher_enabled` 翻成 `true`,改完自动同步。

---

## 一句话工作流

```
[uClaw Wiki tab] ─Export──▶  ~/Documents/workground/brain/  ◀──Edit── [Obsidian]
                                       │
[uClaw Wiki tab] ◀──Sync─────────────┘
```

- **Export** = 把 uClaw 知道的所有 EntityPage 写到磁盘
- **Edit** = 用任何 markdown 编辑器改 `<subkind>/<slug>.md` 的正文(YAML frontmatter 别动!)
- **Sync** = 把磁盘上的改动读回 uClaw,做成新 version

---

## 文件长什么样

```yaml
---
node_uuid: 7f30e8c1-...
last_synced_version_id: e8c1...
slug: alice
title: Alice
subkind: person
aliases:
  - Allie
  - A. S.
enrichment_tier: 2
last_synthesized_at: "2026-05-15T10:00:00Z"
timeline:
  - date: "2026-05-01"
    text: "joined Acme"
  - date: "2026-05-15"
    text: "promoted to staff"
---

Alice 是 Acme 的高级工程师,五月从 MIT 毕业后加入。最近升任 staff。

她主要参与 RAG 项目和 search 团队的合作。
```

- **`---` 分隔的 YAML 头**:uClaw 用这个块认知 page 的身份(`node_uuid`)和元数据。**不要手动改 node_uuid**,否则 Sync 会把这个文件当成一个全新 page 创建出来。
- **`---` 之下的 markdown 正文**:就是 `compiled_truth`。改这部分最自然 —— Sync 之后 uClaw 会建一个新 version,前一个 version 被标记为 `deprecated`(历史保留)。

如果你只动 frontmatter 字段(比如改了 `aliases` 列表加了一个别名),
Sync 时也会被检测到 ——`enrichment_tier`、`subkind`、`timeline` 等
字段在 sync 时也会被合并回 uClaw,**disk wins**(磁盘永远赢)。

---

## 怎么开始

### 第一次 Export

1. 打开 uClaw → Wiki tab
2. 点击 **Export** 按钮(FolderDown 图标)
3. 等待 Toast 提示 "Exported to brain dir: N written"
4. 在 Finder / `cd ~/Documents/workground/brain/` 查看导出的文件结构
5. 用你喜欢的编辑器(推荐 Obsidian)打开这个文件夹

### Round-trip 一次编辑

1. 在编辑器里改 `alice.md` 正文,加一句话,保存
2. 回到 uClaw Wiki tab,点击 **Sync** 按钮(FolderSync 图标)
3. Toast 显示 "Synced from brain dir: 1 updated"
4. 在 Wiki 里点 alice 这个 page,看到新内容
5. 旧 version 标记为 `deprecated`(用 SQL 查 `memory_versions WHERE node_id=...` 能看到)

### 创建一个新 EntityPage(直接在磁盘上)

如果你想 bypass uClaw 的 QuickCapture,可以直接在 brain 目录里
新建一个 .md 文件:

```yaml
---
slug: charlie
title: Charlie
subkind: person
---

Charlie 是我的好朋友。
```

注意 frontmatter 里**没有 `node_uuid`**。下次 Sync 时 uClaw 会:
- 检查 space 里有没有 `slug=charlie` 的 page —— 没有 → 新建一个
- 把这个文件采纳到 brain_sync_state 里
- 后续 Sync 把这个 page 当作 round-trip 的源(就跟 Export 创建的一样)

---

## 冲突怎么办

冲突 = 你在 disk 上改了 page X,**与此同时** uClaw 的 Agent / 用户
也改了 DB 里的 page X(例如点了 Synthesize 按钮重新合成 compiled_truth)。

Sync 会:

1. **disk 永远赢** —— 磁盘上的内容覆盖 DB
2. 在 Health tab 写一条 `sync_conflict` 错误等级的 finding
3. Finding 的 payload 里包含被覆盖的 DB version_id —— 万一你想找回那一版,SQL 仍能查到(`memory_versions WHERE id = '<overwritten_db_version_id>'`,因为它只是被标成 `deprecated`,没真删)

**recovery 流程:**

- Wiki tab → Health pill → 找到 severity=error 的 `Sync conflict (disk won)` 行
- 点开 payload,记下 `overwritten_db_version_id`
- 如果你想恢复 DB 那一版:
  - 用 SQL `SELECT content FROM memory_versions WHERE id = '<id>'`
  - 把那段内容拷回 brain 里的 .md 文件
  - 再 Sync —— disk 又赢一次,但这次写的是你期望的内容

---

## fs Watcher(实时同步)

默认 OFF。打开方法:

```jsonc
// ~/.uclaw/memubot_config.json
{
  "memory_os": {
    "brain_watcher_enabled": true
  }
}
```

重启 uClaw。从此你在编辑器里保存任何 .md 文件,500ms 后 uClaw 会
自动 Sync。Toast 不会主动弹(避免每次保存都打扰),但有冲突时
仍会写 Health finding。

**什么时候关掉 watcher:**

- 网络盘 / iCloud 同步导致大量虚假 modify 事件
- 你的编辑器有 "auto save on focus loss" 行为,导致频繁 fs 事件 + sync 调用
- SQLite lock 在多并发场景下被 sync 占用太久

任何时候关闭都是安全的 —— 你回到 "手动 Sync 按钮" 模式,功能完全保留。

---

## Obsidian 推荐配置

把 `~/Documents/workground/brain/` 作为 vault 根目录,然后:

1. **关闭 "Default new pane location"** —— 避免 Obsidian 在 vault 外创建 attachments
2. **Settings → Files & Links → Default location for new attachments → In the folder specified below**:`_attachments/`(避开 subkind 目录)
3. **Settings → Editor → Properties → Show as raw frontmatter**(可选):让你看到 YAML 而不是 Obsidian 自己的 properties UI(后者有时会修改字段名)
4. **不要**用 Obsidian 的 "Rename file" 重命名 .md 文件 —— 那会改 file path 但不会改 frontmatter 的 `slug`,Sync 时会两边不一致。重命名要改 frontmatter 的 `slug`,然后 uClaw 端 Export 时会自动重新放到 `<新subkind>/<新slug>.md`。

---

## 常见问题

**Q: Sync 之后 Wiki 里看不到改动?**
A: 检查 Toast 内容,如果 "0 updated 1 unchanged",说明 SHA-256 检测到磁盘内容跟上次同步一样 —— 大概率是 frontmatter 被改但正文没改;或者编辑器只 touch 了文件但没真改字节。

**Q: 我改了 frontmatter 的 `node_uuid`,会发生什么?**
A: Sync 会找不到对应的 node(UUID 是 PK),把这个文件当成"新建" —— 在 DB 里会出现一个跟你原本想编辑的 page 不同的全新 page,你原本的 page 不会被改。**别动 node_uuid**。

**Q: 我手动删了 brain 目录里的某个文件,DB 里的 page 会被删吗?**
A: **不会**。Sync 是 "import everything currently on disk",不是双向 mirror。删除 EntityPage 仍然是显式 IPC action(`memory_entity_page_delete` 还没暴露,后续会加)。这是有意为之的安全机制 —— 不让一个 `rm -rf` 把你的记忆抹掉。

**Q: aliases 字段里大小写不一致会怎样?**
A: Sync 会 case-insensitive 去重(`["Alice", "alice"]` → `["Alice"]`)。

**Q: 我多个 space 怎么办?**
A: 当前所有 space 共享同一个 `brain_root`。EntityPage 的 `space_id` 记录在 `brain_sync_state.space_id` 列里,导出文件不冲突(slug 在 space 内唯一)。如果你需要每个 space 一个独立目录,请提 issue。

**Q: 我能不能用 git 跟踪 brain 目录?**
A: 可以,而且推荐 —— 这是把 wiki 当作"我的第二大脑"的核心价值之一。每次 Export 之后 commit 一下,你就有了 wiki 的版本历史 + 跨设备同步(git push/pull)能力。注意 Sync 时 disk wins,所以 git 拉下来的最新版会覆盖 DB —— 用 git 当 source of truth 时务必先 Sync 一次再 Export,避免来回覆盖。

---

## 安全 / 隐私

- 所有数据都在你的本地磁盘,uClaw **不会**把 brain 目录的内容发到任何服务器
- 如果你启用了 Phase 6b/6c 的 Real LLM(`wiki_real_synthesizer_enabled` / `lint_real_analyzer_enabled`),那是另一回事 —— LLM 调用会把 EntityPage 的 compiled_truth 发给你配的 provider。Phase 7 sync 本身完全离线
- Watcher 也是纯本地 —— `notify` crate 监听 OS 级 fs 事件,不上网
