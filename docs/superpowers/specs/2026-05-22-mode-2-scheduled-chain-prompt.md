# Mode 2: Self-perpetuating Scheduled Chain — Prompt Template

> Reference template for kicking off an unattended overnight execution
> chain via `mcp__scheduled-tasks__create_scheduled_task`. The first
> task is launched manually by the user; each subsequent task is
> scheduled by the previous task's agent as its last act before exiting.

---

## When to use this

- Many sequential PRs in a single phase (M3 wire-up ≈ 6-12 PRs)
- Each PR is reasonably mechanical (spec'd in advance)
- User wants to wake up to N completed prep branches, batch-review and merge

**Don't use when**:

- PRs need design discussion mid-flight (Mode 1 instead)
- The phase is brand-new with no spec yet (write spec sessions first)
- You don't trust circuit breakers — overnight unsupervised drift can stack 4 broken PRs

---

## The kick-off message (user → agent)

Paste this in chat to start the chain. Adjust `MAX_TASKS` and `SLOT_MINUTES`.

```
开始 Mode 2 自我繁衍链。参数:
- 起点队列: docs/superpowers/queue/C1-execution-queue.md (从第一个 [ ] 开始)
- 每个 slot 间隔: 90 分钟
- MAX_TASKS: 6
- 熔断阈值: cargo build 失败 / drift-check RED / 队列空 / 已达 MAX

每个 task 必做完整 closed-loop 流程:
1. 读 SSoT (MILESTONE_STATUS.md)
2. 跑 drift-check
3. 读 spec / 写 spec
4. 实施 (push 到 prep/ 分支, 不开 PR — 留给我审)
5. 改 queue 文件标 [x]
6. 调 mcp__scheduled-tasks__create_scheduled_task 排下一个 90 min 后跑,prompt 用 mode-2 模板自我复制

完成所有 task 或熔断时,发邮件给 ryanclaudemax@gmail.com (subject: "uclaw Mode 2 chain stopped — X/Y tasks completed").

现在创建第一个 scheduled task,fireAt = $(date +%s) + 60s (1 分钟后启动)。
```

---

## The scheduled task prompt template (agent uses to perpetuate)

This is what each chained task's prompt looks like. Substitute
`{{N}}` (task index), `{{MAX}}` (cap), `{{QUEUE}}` (queue file path),
`{{SLOT_MINUTES}}` (interval), `{{NEXT_FIRE_AT}}` (computed).

```
Mode 2 chain task {{N}}/{{MAX}}.

# 强制 pre-flight
1. cd ~/Documents/uclaw && git status -sb  (must be clean modulo untracked)
2. cat docs/superpowers/MILESTONE_STATUS.md | head -40
3. ./scripts/milestone-drift-check.sh --since "1 week ago"
   - 如果 RED 且当前是 wire-up/foundation 类工作: 继续 (wire-up 是 anti-drift 行为)
   - 如果 RED 且是 Bundle: STOP + 邮件
4. Load skill: uclaw-milestone-closed-loop

# 找下一项
5. cat {{QUEUE}} 找第一个 [ ]
6. 如果队列空: STOP + 邮件 (subject: "Mode 2 chain — all tasks completed")
7. 该项的 Spec 链接必须存在: 如果是 _(needs to be written)_,先写 spec 提交 commit 推送,然后本 task 退出 + 排下一个 task 同 slot 接力 (写 spec 不算消耗 task quota)

# 实施
8. 切 prep 分支 (queue 项里写了 Branch: prep/xxx)
9. 按 spec §4 commit plan 逐 commit
10. 每 commit 前: cargo build (errors only)。失败 → STOP + 邮件 (subject: "Mode 2 chain BROKEN — task {{N}} cargo build failed on commit X")
11. 跑相关单测 (spec §X.X 列出来的)
12. 推送 prep 分支 (不开 PR — 用户审)

# 收尾 (本 task 完工)
13. 编辑 {{QUEUE}} 标 [x] + 填 Actual PR: prep/xxx pushed waiting review
14. 编辑 docs/superpowers/MILESTONE_STATUS.md 反映新进度
15. 提交 + 推送本 task 的 doc 改动

# 自我繁衍
16. 如果 N < MAX:
    fire_at = now + {{SLOT_MINUTES}} 分钟
    mcp__scheduled-tasks__create_scheduled_task({
      prompt: <本模板 with N=N+1>,
      fireAt: fire_at
    })
17. 如果 N >= MAX: 发邮件 subject "Mode 2 chain — hit MAX_TASKS={{MAX}}"

退出.
```

---

## Circuit breakers (forcing functions inside each task)

| 条件 | 处理 |
|---|---|
| `cargo build` fail | STOP chain, 邮件 with last cargo output |
| `cargo test` fail | STOP chain, 邮件 with failing test |
| drift check RED + 当前是 Bundle | STOP chain, 邮件警告 |
| Queue 空 | 优雅退出,邮件 "all done" |
| Hit MAX_TASKS | 优雅退出,邮件 "hit max" |
| Spec 不存在且 queue 没说 _(needs to be written)_ | 写 spec 后让步,排下一 task 实施(spec 写作不算 task 配额) |
| git push 冲突 (main 有新 commit) | STOP chain, 邮件 "needs human merge" |
| Working tree dirty | STOP chain (shouldn't happen since cleanup is per-task) |

---

## Email notification setup (one-time)

This template uses Cowork's email-as-output flow. Either:

**Option A** — call `send_email` MCP tool if available in scheduled task session.

**Option B** — Pin a stdout output that Cowork's daily digest will capture
+ check digest manually. Less reliable.

**Option C** — Use a Webhook MCP that hits a Slack channel webhook URL.
Most reliable but requires Slack workspace.

If none available, fallback: chain agent writes the final status to
`/tmp/mode-2-chain-status.json` + user checks that file at start of day.

---

## Manual Mode 2 abort

If chain misbehaves overnight, kill it via:

```bash
# List scheduled tasks
gh ... # or via Cowork UI: Settings → Scheduled Tasks

# In a fresh agent session:
mcp__scheduled-tasks__list_scheduled_tasks → find Mode-2-* IDs
mcp__scheduled-tasks__update_scheduled_task with disabled=true
```

Or simpler: comment out all `[ ]` items in the active queue. Next chain
task will see "queue empty" and gracefully stop.

---

## When to graduate from Mode 2

Mode 2 is for **execution scaling**, not **complexity scaling**. If
you find yourself wanting to chain "exploratory" tasks (e.g. "investigate
this bug") via Mode 2, you've outgrown it — those need attended sessions
(Mode 1).

Mode 2 works when each queue item could fit in a 1-paragraph Jira ticket
plus a spec link. If items need ≥ 1 page of context per turn, downgrade
to Mode 1.

---

## Reference

- Queue pattern: [`../queue/README.md`](../queue/README.md)
- Closed-loop discipline: [`../plans/2026-05-22-pr-integration-strategy.md`](../plans/2026-05-22-pr-integration-strategy.md)
- Forcing function rules: [skill `uclaw-milestone-closed-loop`](../../.claude/skills/uclaw-milestone-closed-loop/)
