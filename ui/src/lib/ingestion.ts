import { invoke } from '@tauri-apps/api/core'

export type IngestionStatus =
  | 'queued' | 'parsing' | 'extracting' | 'writing' | 'done' | 'partial' | 'failed'

export interface IngestionProgress { stage: string; done: number; total: number }

export interface IngestionJob {
  id: string
  source_label: string
  status: IngestionStatus
  progress: IngestionProgress
  pages_written: string[]
  error: string | null
}

export function ingestFiles(paths: string[]): Promise<string[]> {
  return invoke<string[]>('ingest_files', { paths })
}

export function ingestUrl(url: string): Promise<string> {
  return invoke<string>('ingest_url', { url })
}

export function ingestListJobs(): Promise<IngestionJob[]> {
  return invoke<IngestionJob[]>('ingest_list_jobs', {})
}

export function ingestJobStatus(id: string): Promise<IngestionJob | null> {
  return invoke<IngestionJob | null>('ingest_job_status', { id })
}
