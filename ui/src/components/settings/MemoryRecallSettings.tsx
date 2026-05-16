/**
 * MemoryRecallSettings — 记忆召回参数配置表单
 *
 * 将 MemoryRecallConfig 的 11 个字段暴露为可编辑的设置项，
 * 支持输入验证、范围限制、一键恢复默认值和批量保存。
 */
import { useState, useEffect, useCallback } from 'react'
import { RotateCcw, Save, ChevronDown } from 'lucide-react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsSelect } from './primitives/SettingsSelect'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '@/components/ui/collapsible'
import {
  getMemoryRecallConfig,
  patchMemoryRecallConfig,
  type MemoryRecallConfigDto,
} from '@/lib/tauri-bridge'

// ─── 默认值（与 Rust MemoryRecallConfig::default() 保持同步）──────────

const DEFAULTS: Required<MemoryRecallConfigDto> = {
  bootLimit: 8,
  triggerLimit: 6,
  seedLimit: 8,
  expansionLimit: 6,
  recentLimit: 3,
  fusionStrategy: 'rrf',
  rrfK: 60,
  ftsWeight: 0.5,
  vectorWeight: 0.5,
  bootLearnedSkillsLimit: 3,
  tokenBudget: 5000,
  layerExpandedSeedTake: 5,
  layerExpandedMaxDepth: 2,
  timeDecayHalfLifeDays: 7.0,
  ftsFallbackLimitMultiplier: 2.0,
  bootUserProfileLimit: 5,
}

// ─── 验证范围（与 Rust patch_memory_recall_config 保持同步）────────────

const RANGES = {
  bootLimit: { min: 0, max: 50, label: '启动层召回数' },
  triggerLimit: { min: 0, max: 50, label: '触发层召回数' },
  seedLimit: { min: 0, max: 50, label: '种子层召回数' },
  expansionLimit: { min: 0, max: 50, label: '扩展层召回数' },
  recentLimit: { min: 0, max: 30, label: '近期层召回数' },
  rrfK: { min: 1, max: 200, label: 'RRF 融合参数 k' },
  ftsWeight: { min: 0, max: 1, label: '全文搜索权重' },
  vectorWeight: { min: 0, max: 1, label: '向量搜索权重' },
  bootLearnedSkillsLimit: { min: 0, max: 20, label: '自动挂载技能数' },
  tokenBudget: { min: 100, max: 20000, label: 'Token 预算' },
  layerExpandedSeedTake: { min: 1, max: 20, label: '图扩展种子数' },
  layerExpandedMaxDepth: { min: 1, max: 5, label: '图扩展深度' },
  timeDecayHalfLifeDays: { min: 0.5, max: 90, label: '时间衰减半衰期 (天)' },
  ftsFallbackLimitMultiplier: { min: 1.0, max: 5.0, label: 'FTS 降级倍率' },
  bootUserProfileLimit: { min: 0, max: 20, label: '用户档案挂载数' },
} as const

const FUSION_OPTIONS = [
  { value: 'rrf', label: 'RRF（倒数排名融合）' },
  { value: 'weighted', label: 'Weighted（加权融合）' },
]

// ─── 辅助 ────────────────────────────────────────────────────────────────

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

// ─── 组件 ────────────────────────────────────────────────────────────────

export function MemoryRecallSettings(): React.ReactElement {
  const [config, setConfig] = useState<MemoryRecallConfigDto>({ ...DEFAULTS })
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)

  // 加载当前配置
  useEffect(() => {
    getMemoryRecallConfig()
      .then((cfg) => {
        // 合并：未设置的字段回退到默认值
        setConfig({ ...DEFAULTS, ...cfg })
      })
      .catch((err) => console.error('加载记忆召回配置失败:', err))
      .finally(() => setLoading(false))
  }, [])

  // 更新单个字段
  const updateField = useCallback(
    <K extends keyof MemoryRecallConfigDto>(
      key: K,
      value: MemoryRecallConfigDto[K],
    ) => {
      setConfig((prev) => ({ ...prev, [key]: value }))
      setDirty(true)
    },
    [],
  )

  // 保存
  const handleSave = useCallback(async () => {
    setSaving(true)
    try {
      const saved = await patchMemoryRecallConfig(config)
      setConfig({ ...DEFAULTS, ...saved })
      setDirty(false)
    } catch (err) {
      console.error('保存记忆召回配置失败:', err)
    } finally {
      setSaving(false)
    }
  }, [config])

  // 恢复默认值
  const handleReset = useCallback(() => {
    setConfig({ ...DEFAULTS })
    setDirty(true)
  }, [])

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-6 bg-muted rounded w-32" />
        <div className="h-40 bg-muted rounded" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 操作栏 */}
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">
          修改后点击「保存」生效，每次 Agent 对话自动热加载最新配置
        </p>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleReset}
            disabled={saving}
            className="h-7 text-xs gap-1"
          >
            <RotateCcw size={12} />
            恢复默认
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!dirty || saving}
            className="h-7 text-xs gap-1"
          >
            <Save size={12} />
            {saving ? '保存中…' : '保存'}
          </Button>
        </div>
      </div>

      {/* Token 预算 */}
      <SettingsSection
        title="Token 预算"
        description="控制每轮 Agent 对话中记忆上下文占用的最大 token 数。设为 0 可禁用限制。"
      >
        <SettingsCard>
          <SettingsRow
            label="token_budget"
            description={`范围: ${RANGES.tokenBudget.min} – ${RANGES.tokenBudget.max} tokens · 默认: ${DEFAULTS.tokenBudget}`}
          >
            <NumberInput
              value={config.tokenBudget ?? DEFAULTS.tokenBudget}
              min={RANGES.tokenBudget.min}
              max={RANGES.tokenBudget.max}
              onChange={(v) => updateField('tokenBudget', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 召回数量限制 */}
      <SettingsSection
        title="召回数量限制"
        description="控制各记忆层的候选召回数量。减少可降低 token 消耗但可能遗漏关键记忆。"
      >
        <SettingsCard>
          <SettingsRow
            label="boot_limit"
            description={`启动层（始终注入）· ${RANGES.bootLimit.min}–${RANGES.bootLimit.max} · 默认 ${DEFAULTS.bootLimit}`}
          >
            <NumberInput
              value={config.bootLimit ?? DEFAULTS.bootLimit}
              min={RANGES.bootLimit.min}
              max={RANGES.bootLimit.max}
              onChange={(v) => updateField('bootLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="trigger_limit"
            description={`触发层（直接匹配）· ${RANGES.triggerLimit.min}–${RANGES.triggerLimit.max} · 默认 ${DEFAULTS.triggerLimit}`}
          >
            <NumberInput
              value={config.triggerLimit ?? DEFAULTS.triggerLimit}
              min={RANGES.triggerLimit.min}
              max={RANGES.triggerLimit.max}
              onChange={(v) => updateField('triggerLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="seed_limit"
            description={`种子层（触发邻居）· ${RANGES.seedLimit.min}–${RANGES.seedLimit.max} · 默认 ${DEFAULTS.seedLimit}`}
          >
            <NumberInput
              value={config.seedLimit ?? DEFAULTS.seedLimit}
              min={RANGES.seedLimit.min}
              max={RANGES.seedLimit.max}
              onChange={(v) => updateField('seedLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="expansion_limit"
            description={`扩展层（种子邻居）· ${RANGES.expansionLimit.min}–${RANGES.expansionLimit.max} · 默认 ${DEFAULTS.expansionLimit}`}
          >
            <NumberInput
              value={config.expansionLimit ?? DEFAULTS.expansionLimit}
              min={RANGES.expansionLimit.min}
              max={RANGES.expansionLimit.max}
              onChange={(v) => updateField('expansionLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="recent_limit"
            description={`近期层（最近使用）· ${RANGES.recentLimit.min}–${RANGES.recentLimit.max} · 默认 ${DEFAULTS.recentLimit}`}
          >
            <NumberInput
              value={config.recentLimit ?? DEFAULTS.recentLimit}
              min={RANGES.recentLimit.min}
              max={RANGES.recentLimit.max}
              onChange={(v) => updateField('recentLimit', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 融合策略 */}
      <SettingsSection
        title="融合策略"
        description="控制全文搜索和向量搜索结果的融合方式。RRF 使用倒数排名融合，Weighted 使用加权分数。"
      >
        <SettingsCard>
          <SettingsRow
            label="fusion_strategy"
            description="融合算法选择"
          >
            <SettingsSelect
              value={config.fusionStrategy ?? DEFAULTS.fusionStrategy}
              onValueChange={(v) =>
                updateField('fusionStrategy', v as 'rrf' | 'weighted')
              }
              options={FUSION_OPTIONS}
            />
          </SettingsRow>
          <SettingsRow
            label="rrf_k"
            description={`RRF 平滑参数 · ${RANGES.rrfK.min}–${RANGES.rrfK.max} · 默认 ${DEFAULTS.rrfK}（仅 RRF 模式生效）`}
          >
            <NumberInput
              value={config.rrfK ?? DEFAULTS.rrfK}
              min={RANGES.rrfK.min}
              max={RANGES.rrfK.max}
              onChange={(v) => updateField('rrfK', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="fts_weight"
            description={`全文搜索权重 · ${RANGES.ftsWeight.min}–${RANGES.ftsWeight.max} · 默认 ${DEFAULTS.ftsWeight}（仅 Weighted 模式生效）`}
          >
            <NumberInput
              value={config.ftsWeight ?? DEFAULTS.ftsWeight}
              min={RANGES.ftsWeight.min}
              max={RANGES.ftsWeight.max}
              step={0.1}
              onChange={(v) => updateField('ftsWeight', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="vector_weight"
            description={`向量搜索权重 · ${RANGES.vectorWeight.min}–${RANGES.vectorWeight.max} · 默认 ${DEFAULTS.vectorWeight}（仅 Weighted 模式生效）`}
          >
            <NumberInput
              value={config.vectorWeight ?? DEFAULTS.vectorWeight}
              min={RANGES.vectorWeight.min}
              max={RANGES.vectorWeight.max}
              step={0.1}
              onChange={(v) => updateField('vectorWeight', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 技能挂载 */}
      <SettingsSection
        title="技能挂载"
        description="控制每轮 Agent 对话中自动注入的已学技能数量。技能按使用次数排名。"
      >
        <SettingsCard>
          <SettingsRow
            label="boot_learned_skills_limit"
            description={`自动挂载技能数 · ${RANGES.bootLearnedSkillsLimit.min}–${RANGES.bootLearnedSkillsLimit.max} · 默认 ${DEFAULTS.bootLearnedSkillsLimit}（0=禁用）`}
          >
            <NumberInput
              value={
                config.bootLearnedSkillsLimit ?? DEFAULTS.bootLearnedSkillsLimit
              }
              min={RANGES.bootLearnedSkillsLimit.min}
              max={RANGES.bootLearnedSkillsLimit.max}
              onChange={(v) => updateField('bootLearnedSkillsLimit', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 高级设置 */}
      <Collapsible>
        <CollapsibleTrigger className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground hover:text-foreground cursor-pointer transition-colors duration-150 py-1">
          <ChevronDown className="size-3.5 transition-transform duration-200 [[data-state=open]>&]:rotate-180" />
          高级设置
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="mt-3 space-y-6">
            <SettingsSection
              title="图扩展参数"
              description="控制 L4 BFS 图扩展阶段的种子数和搜索深度。"
            >
              <SettingsCard>
                <SettingsRow
                  label="layer_expanded_seed_take"
                  description={`图扩展种子数 · ${RANGES.layerExpandedSeedTake.min}–${RANGES.layerExpandedSeedTake.max} · 默认 ${DEFAULTS.layerExpandedSeedTake}`}
                >
                  <NumberInput
                    value={config.layerExpandedSeedTake ?? DEFAULTS.layerExpandedSeedTake}
                    min={RANGES.layerExpandedSeedTake.min}
                    max={RANGES.layerExpandedSeedTake.max}
                    onChange={(v) => updateField('layerExpandedSeedTake', v)}
                  />
                </SettingsRow>
                <SettingsRow
                  label="layer_expanded_max_depth"
                  description={`BFS 最大搜索深度 · ${RANGES.layerExpandedMaxDepth.min}–${RANGES.layerExpandedMaxDepth.max} · 默认 ${DEFAULTS.layerExpandedMaxDepth}`}
                >
                  <SettingsSelect
                    value={String(config.layerExpandedMaxDepth ?? DEFAULTS.layerExpandedMaxDepth)}
                    onValueChange={(v) => updateField('layerExpandedMaxDepth', Number(v))}
                    options={[1, 2, 3, 4, 5].map((n) => ({ value: String(n), label: String(n) }))}
                  />
                </SettingsRow>
              </SettingsCard>
            </SettingsSection>

            <SettingsSection
              title="时间衰减"
              description="记忆相关性随时间衰减的半衰期。较短的半衰期会更偏向近期记忆。"
            >
              <SettingsCard>
                <SettingsRow
                  label="time_decay_half_life_days"
                  description={`半衰期天数 · ${RANGES.timeDecayHalfLifeDays.min}–${RANGES.timeDecayHalfLifeDays.max} · 默认 ${DEFAULTS.timeDecayHalfLifeDays}`}
                >
                  <NumberInput
                    value={config.timeDecayHalfLifeDays ?? DEFAULTS.timeDecayHalfLifeDays}
                    min={RANGES.timeDecayHalfLifeDays.min}
                    max={RANGES.timeDecayHalfLifeDays.max}
                    step={0.5}
                    onChange={(v) => updateField('timeDecayHalfLifeDays', v)}
                  />
                </SettingsRow>
              </SettingsCard>
            </SettingsSection>

            <SettingsSection
              title="FTS 降级"
              description="当 memU 向量引擎不可用时，全文搜索候选数量的倍增系数。"
            >
              <SettingsCard>
                <SettingsRow
                  label="fts_fallback_limit_multiplier"
                  description={`倍增系数 · ${RANGES.ftsFallbackLimitMultiplier.min}–${RANGES.ftsFallbackLimitMultiplier.max} · 默认 ${DEFAULTS.ftsFallbackLimitMultiplier}`}
                >
                  <NumberInput
                    value={config.ftsFallbackLimitMultiplier ?? DEFAULTS.ftsFallbackLimitMultiplier}
                    min={RANGES.ftsFallbackLimitMultiplier.min}
                    max={RANGES.ftsFallbackLimitMultiplier.max}
                    step={0.1}
                    onChange={(v) => updateField('ftsFallbackLimitMultiplier', v)}
                  />
                </SettingsRow>
              </SettingsCard>
            </SettingsSection>

            <SettingsSection
              title="用户档案"
              description="控制自动挂载的 UserProfile 节点数量。0 为禁用。"
            >
              <SettingsCard>
                <SettingsRow
                  label="boot_user_profile_limit"
                  description={`挂载数 · ${RANGES.bootUserProfileLimit.min}–${RANGES.bootUserProfileLimit.max} · 默认 ${DEFAULTS.bootUserProfileLimit}`}
                >
                  <NumberInput
                    value={config.bootUserProfileLimit ?? DEFAULTS.bootUserProfileLimit}
                    min={RANGES.bootUserProfileLimit.min}
                    max={RANGES.bootUserProfileLimit.max}
                    onChange={(v) => updateField('bootUserProfileLimit', v)}
                  />
                </SettingsRow>
              </SettingsCard>
            </SettingsSection>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

// ─── NumberInput 内联组件 ────────────────────────────────────────────────

function NumberInput({
  value,
  min,
  max,
  step = 1,
  onChange,
}: {
  value: number
  min: number
  max: number
  step?: number
  onChange: (v: number) => void
}): React.ReactElement {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(e) => {
        const raw = e.target.value
        if (raw === '' || raw === '-') return // allow clearing, treat as unchanged
        const parsed = Number(raw)
        if (!isNaN(parsed)) {
          onChange(clamp(parsed, min, max))
        }
      }}
      onBlur={(e) => {
        // Re-clamp on blur to catch edge cases
        const parsed = Number(e.target.value)
        if (!isNaN(parsed)) {
          const clamped = clamp(parsed, min, max)
          if (clamped !== parsed) onChange(clamped)
        } else {
          onChange(min) // fallback
        }
      }}
      className="w-24 h-7 text-xs text-right rounded-md border border-border bg-muted/40 px-2 focus:outline-none focus:ring-1 focus:ring-ring focus:border-ring"
    />
  )
}
