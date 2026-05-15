/**
 * SkillSuggestionBar — 聊天输入框下方的技能建议 chip 条。
 *
 * 监听输入文本变化，debounce 500ms 后在本地搜索已有技能
 * (listSkills + listLearnedSkills)，按名称/描述/场景模糊匹配，
 * 显示 top-3 命中结果。点击 chip 触发 onSkillSelect('/<name>')。
 *
 * Phase 4 (G9): 让用户在打字时发现可用技能。
 */
import * as React from 'react'
import { Sparkles } from 'lucide-react'
import { listSkills, listLearnedSkills } from '@/lib/tauri-bridge'
import { cn } from '@/lib/utils'

interface SkillSuggestionBarProps {
  inputText: string
  onSkillSelect: (skillName: string) => void
}

interface SkillCandidate {
  name: string
  description: string
  provenance: 'learned' | 'builtin'
}

export function SkillSuggestionBar({ inputText, onSkillSelect }: SkillSuggestionBarProps): React.ReactElement | null {
  const [suggestions, setSuggestions] = React.useState<SkillCandidate[]>([])
  const cacheRef = React.useRef<SkillCandidate[] | null>(null)

  // Load skills once (lazy, cached)
  const loadSkills = React.useCallback(async (): Promise<SkillCandidate[]> => {
    if (cacheRef.current) return cacheRef.current

    const [builtinResult, learnedResult] = await Promise.allSettled([
      listSkills(),
      listLearnedSkills(),
    ])

    const candidates: SkillCandidate[] = []

    if (builtinResult.status === 'fulfilled') {
      for (const s of builtinResult.value) {
        if (s.enabled) {
          candidates.push({
            name: s.name,
            description: s.description || s.category || '',
            provenance: 'builtin',
          })
        }
      }
    }

    if (learnedResult.status === 'fulfilled') {
      for (const s of learnedResult.value) {
        if (s.enabled) {
          candidates.push({
            name: s.name,
            description: s.context?.split('\n')[0]?.slice(0, 80) || '',
            provenance: 'learned',
          })
        }
      }
    }

    cacheRef.current = candidates
    return candidates
  }, [])

  // Debounced search
  React.useEffect(() => {
    const q = inputText.trim().toLowerCase()
    if (q.length < 5) {
      setSuggestions([])
      return
    }

    const timer = setTimeout(async () => {
      try {
        const all = await loadSkills()
        const matched = all
          .filter((s) => {
            return (
              s.name.toLowerCase().includes(q) ||
              s.description.toLowerCase().includes(q)
            )
          })
          .slice(0, 3)
        setSuggestions(matched)
      } catch {
        setSuggestions([])
      }
    }, 500)

    return () => clearTimeout(timer)
  }, [inputText, loadSkills])

  if (suggestions.length === 0) return null

  return (
    <div className="flex items-center gap-1.5 px-4 pb-1.5">
      <Sparkles className="size-3 text-muted-foreground shrink-0" />
      {suggestions.map((s) => (
        <button
          key={`${s.provenance}-${s.name}`}
          type="button"
          onClick={() => onSkillSelect(`/${s.name}`)}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
            'text-[10.5px] leading-tight',
            'bg-accent/10 text-accent-foreground border border-accent/25',
            'hover:bg-accent/20 hover:border-accent/40',
            'transition-colors truncate max-w-[200px]',
          )}
        >
          <span className="truncate">{s.name}</span>
          {s.description && (
            <span className="text-muted-foreground truncate max-w-[100px]">
              · {s.description}
            </span>
          )}
        </button>
      ))}
    </div>
  )
}
