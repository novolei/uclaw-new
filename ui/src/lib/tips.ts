/**
 * tips — UI 提示文案
 *
 * 提供随机/轮转的 UI 提示文案，用于空状态、加载页等。
 * 从 Proma 迁移。
 */

/** 通用加载提示 */
export const LOADING_TIPS: string[] = [
  '正在连接 AI 引擎...',
  '准备就绪，随时为你服务',
  '每一个想法都值得被认真对待',
  '让 AI 帮你把想法变成现实',
  '好的问题是成功的一半',
]

/** 空对话提示 */
export const EMPTY_CHAT_TIPS: string[] = [
  '开始一段新对话吧！',
  '试着问一个你感兴趣的问题',
  '可以让 AI 帮你写代码、翻译文本、分析数据…',
  '快捷键 Cmd+N / Ctrl+N 创建新对话',
]

/** Agent 空状态提示 */
export const EMPTY_AGENT_TIPS: string[] = [
  '告诉 Agent 你想完成什么任务',
  'Agent 可以帮你执行复杂的多步骤任务',
  '描述你的目标，Agent 会为你拆解和执行',
  '你可以让 Agent 浏览文件、修改代码、运行命令',
]

/** 输入框占位提示 */
export const INPUT_PLACEHOLDER_TIPS: string[] = [
  '输入你的问题或指令...',
  '描述你想完成的任务...',
  '有什么我可以帮你的？',
  '试试看：帮我重构这段代码',
]

/**
 * 随机获取一条提示
 */
export function getRandomTip(tips: string[]): string {
  if (tips.length === 0) return ''
  return tips[Math.floor(Math.random() * tips.length)]!
}

/**
 * 按索引轮转获取提示
 */
export function getTipByIndex(tips: string[], index: number): string {
  if (tips.length === 0) return ''
  return tips[index % tips.length]!
}

/**
 * 获取 Agent 欢迎语
 */
export function getAgentWelcomeMessage(userName?: string): string {
  const name = userName || '你'
  const hour = new Date().getHours()
  let greeting: string

  if (hour < 6) greeting = '夜深了'
  else if (hour < 12) greeting = '早上好'
  else if (hour < 18) greeting = '下午好'
  else greeting = '晚上好'

  return `${greeting}，${name}！有什么我可以帮你完成的吗？`
}
