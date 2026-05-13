/** Returns onMouseEnter / onMouseLeave handlers that drive petHoverActiveAtom. */
import { useSetAtom } from 'jotai'
import { useCallback, useMemo } from 'react'
import { petHoverActiveAtom } from '@/atoms/pet-atoms'

export function usePetHover() {
  const setHover = useSetAtom(petHoverActiveAtom)
  const onMouseEnter = useCallback(() => setHover(true), [setHover])
  const onMouseLeave = useCallback(() => setHover(false), [setHover])
  return useMemo(() => ({ onMouseEnter, onMouseLeave }), [onMouseEnter, onMouseLeave])
}
