import { describe, it, expect } from 'vitest'
import { parsePlanMarkdown } from './PlanViewer'

// The backend (src-tauri/src/agent/tools/builtin/plan.rs) writes
// `task: "{title}"` in the YAML frontmatter — NOT `title:`. The parser must
// accept both, so historical plan files keep rendering and any future
// rename to `title:` works without breaking the field name contract.
//
// Regression: 2026-05-18 gomoku session. Plan file had `task: "网页五子棋小游戏开发计划"`,
// UI rendered "Unnamed Plan", confusing the user.

const BACKEND_WRITTEN_PLAN = `---
task: "网页五子棋小游戏开发计划"
status: in_progress
created_at: 2026-05-17T19:14:24+00:00
---

## Goal
网页五子棋小游戏开发计划

## Steps
- [ ] 1. 初始化项目结构
- [ ] 2. 构建棋盘组件
- [x] 3. 实现落子逻辑

## Notes
单文件 HTML 实现
`

describe('parsePlanMarkdown', () => {
  it('reads the task field that plan_write actually emits', () => {
    const parsed = parsePlanMarkdown(BACKEND_WRITTEN_PLAN)
    expect(parsed.title).toBe('网页五子棋小游戏开发计划')
    expect(parsed.title).not.toBe('Unnamed Plan')
  })

  it('also accepts the title field for forward compatibility', () => {
    const withTitle = BACKEND_WRITTEN_PLAN.replace(
      'task: "网页五子棋小游戏开发计划"',
      'title: "网页五子棋小游戏开发计划"',
    )
    const parsed = parsePlanMarkdown(withTitle)
    expect(parsed.title).toBe('网页五子棋小游戏开发计划')
  })

  it('falls back to Unnamed Plan when neither field is present', () => {
    const noTitle = `---
status: in_progress
---

## Steps
- [ ] 1. step
`
    const parsed = parsePlanMarkdown(noTitle)
    expect(parsed.title).toBe('Unnamed Plan')
  })

  it('parses steps and tracks done state correctly', () => {
    const parsed = parsePlanMarkdown(BACKEND_WRITTEN_PLAN)
    expect(parsed.steps).toHaveLength(3)
    expect(parsed.steps[0].done).toBe(false)
    expect(parsed.steps[2].done).toBe(true)
  })
})
