import { useEffect, useRef } from 'react'
import { useAtomValue, useSetAtom, useAtom } from 'jotai'
import {
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  type Vec2,
} from '@/atoms/home-office-atoms'
import { ZONES, STATE_TO_ZONE } from '@/components/home-office/scene/hit-areas'
import { vectorToDirection } from '@/components/home-office/scene/dir-utils'

// Normalized scene units per millisecond. 0.4 units in ~2.5s feels natural.
const WALK_SPEED = 0.4 / 2500
const ARRIVE_EPSILON = 0.005
const TICK_MS = 33 // ~30 Hz logic tick (sprite anim runs separately)

export function useCharacterPath() {
  const state = useAtomValue(homeOfficeStateAtom)
  const [position, setPosition] = useAtom(characterPositionAtom)
  const setDirection = useSetAtom(characterDirectionAtom)
  const setMotion = useSetAtom(characterMotionAtom)

  const targetRef = useRef<Vec2 | null>(null)
  const positionRef = useRef(position)
  positionRef.current = position

  // On state change → choose target zone (or null = stay put)
  useEffect(() => {
    const zoneKey = STATE_TO_ZONE[state]
    if (!zoneKey) {
      targetRef.current = null
      setMotion('pose')
      return
    }
    const zone = ZONES[zoneKey]
    targetRef.current = zone.center
    const dx = zone.center.x - positionRef.current.x
    const dy = zone.center.y - positionRef.current.y
    if (Math.hypot(dx, dy) < ARRIVE_EPSILON) {
      setMotion('pose')
      return
    }
    setDirection(vectorToDirection({ x: dx, y: dy }))
    setMotion('walk')
  }, [state, setDirection, setMotion])

  // Lerp tick — fires while a target is set, stops on arrival.
  useEffect(() => {
    const id = setInterval(() => {
      const target = targetRef.current
      if (!target) return
      const cur = positionRef.current
      const dx = target.x - cur.x
      const dy = target.y - cur.y
      const dist = Math.hypot(dx, dy)
      if (dist < ARRIVE_EPSILON) {
        const arrived: Vec2 = { x: target.x, y: target.y }
        positionRef.current = arrived
        setPosition(arrived)
        setMotion('pose')
        targetRef.current = null
        return
      }
      const step = WALK_SPEED * TICK_MS
      const ratio = Math.min(step / dist, 1)
      const next: Vec2 = { x: cur.x + dx * ratio, y: cur.y + dy * ratio }
      positionRef.current = next
      setPosition(next)
    }, TICK_MS)
    return () => clearInterval(id)
  }, [setPosition, setMotion])
}
