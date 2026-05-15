import { useAtomValue } from 'jotai'
import { humaneSpecsAtom } from '@/atoms/automation'
import { SpecListItem } from './SpecListItem'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  specs?: HumaneSpecRow[]           // if omitted, reads from humaneSpecsAtom
  selectedSpecId?: string | null
  onSelect?: (specId: string) => void
  onRun?: (specId: string) => void
}

export function SpecList({ specs: propSpecs, selectedSpecId, onSelect, onRun }: Props) {
  const atomSpecs = useAtomValue(humaneSpecsAtom)
  const specs = propSpecs ?? atomSpecs

  if (specs.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-4 text-sm text-muted-foreground">
        没有数字人
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-1 p-2 overflow-y-auto">
      {specs.map((spec) => (
        <SpecListItem
          key={spec.id}
          spec={spec}
          isSelected={spec.id === selectedSpecId}
          onSelect={() => onSelect?.(spec.id)}
          onRun={() => onRun?.(spec.id)}
        />
      ))}
    </div>
  )
}
