import {
  applyProjectionJournalEntries,
  buildRuntimeProjection,
  emptyRuntimeProjection,
  type BuildRuntimeProjectionOptions,
  type ProjectionJournalEntry,
  type RuntimeProjection,
  type SessionProjectionStub,
} from './projection-reducer'
import {
  buildWorldProjection,
  type WorldProjection,
  type WorldProjectionOptions,
} from './world-projection'

export interface ProjectionHydrationPayload {
  sessionId?: string
  stub?: SessionProjectionStub | null
  journalEntries?: ProjectionJournalEntry[] | null
}

export interface ProjectionHydrationOptions {
  runtime?: BuildRuntimeProjectionOptions
  world?: WorldProjectionOptions
}

export interface ProjectionHydrationResult {
  runtime: RuntimeProjection
  world: WorldProjection
  source: {
    hasStub: boolean
    journalEntryCount: number
  }
}

export function hydrateProjection(
  payload: ProjectionHydrationPayload,
  options: ProjectionHydrationOptions = {},
): ProjectionHydrationResult {
  const runtime = buildRuntimeFromPayload(payload, options)
  return buildHydrationResult(runtime, payload, options)
}

export function applyProjectionHydration(
  previous: ProjectionHydrationResult,
  payload: ProjectionHydrationPayload,
  options: ProjectionHydrationOptions = {},
): ProjectionHydrationResult {
  const requestedSessionId = payload.sessionId ?? options.runtime?.sessionId
  const shouldResetSession =
    requestedSessionId !== undefined &&
    requestedSessionId !== previous.runtime.session.sessionId

  const runtime =
    payload.stub || shouldResetSession
      ? buildRuntimeFromPayload(payload, options)
      : applyProjectionJournalEntries(
          previous.runtime,
          payloadJournalEntries(payload),
        )

  return buildHydrationResult(runtime, payload, options)
}

function buildRuntimeFromPayload(
  payload: ProjectionHydrationPayload,
  options: ProjectionHydrationOptions,
): RuntimeProjection {
  const sessionId = payload.sessionId ?? options.runtime?.sessionId
  const initialRuntime = payload.stub
    ? buildRuntimeProjection(payload.stub, {
        ...options.runtime,
        sessionId,
      })
    : emptyRuntimeProjection(sessionId)

  return applyProjectionJournalEntries(
    initialRuntime,
    payloadJournalEntries(payload),
  )
}

function buildHydrationResult(
  runtime: RuntimeProjection,
  payload: ProjectionHydrationPayload,
  options: ProjectionHydrationOptions,
): ProjectionHydrationResult {
  return {
    runtime,
    world: buildWorldProjection(runtime, options.world),
    source: {
      hasStub: Boolean(payload.stub),
      journalEntryCount: payloadJournalEntries(payload).length,
    },
  }
}

function payloadJournalEntries(
  payload: ProjectionHydrationPayload,
): ProjectionJournalEntry[] {
  return payload.journalEntries ?? []
}
