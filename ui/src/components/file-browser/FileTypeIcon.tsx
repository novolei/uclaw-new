/**
 * FileTypeIcon — 文件类型图标
 *
 * 根据文件扩展名或目录类型渲染对应图标。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import {
  File,
  FileText,
  FileCode,
  FileImage,
  FileVideo,
  FileAudio,
  FileJson,
  Folder,
  FolderOpen,
  FileSpreadsheet,
  FileArchive,
  Settings,
  Terminal,
  Database,
  FileType2,
  LucideIcon,
} from 'lucide-react'
import { cn } from '@/lib/utils'

/** 文件扩展名 → 图标映射 */
const EXTENSION_ICON_MAP: Record<string, LucideIcon> = {
  // 代码文件
  ts: FileCode,
  tsx: FileCode,
  js: FileCode,
  jsx: FileCode,
  py: FileCode,
  rs: FileCode,
  go: FileCode,
  java: FileCode,
  c: FileCode,
  cpp: FileCode,
  h: FileCode,
  hpp: FileCode,
  cs: FileCode,
  rb: FileCode,
  php: FileCode,
  swift: FileCode,
  kt: FileCode,
  dart: FileCode,
  vue: FileCode,
  svelte: FileCode,
  // 标记文件
  html: FileCode,
  css: FileCode,
  scss: FileCode,
  less: FileCode,
  xml: FileCode,
  svg: FileCode,
  // 数据文件
  json: FileJson,
  yaml: FileJson,
  yml: FileJson,
  toml: FileJson,
  csv: FileSpreadsheet,
  xls: FileSpreadsheet,
  xlsx: FileSpreadsheet,
  // 文本文件
  md: FileText,
  txt: FileText,
  log: FileText,
  rtf: FileText,
  doc: FileText,
  docx: FileText,
  pdf: FileText,
  // 图片文件
  png: FileImage,
  jpg: FileImage,
  jpeg: FileImage,
  gif: FileImage,
  webp: FileImage,
  bmp: FileImage,
  ico: FileImage,
  // 视频文件
  mp4: FileVideo,
  webm: FileVideo,
  avi: FileVideo,
  mov: FileVideo,
  mkv: FileVideo,
  // 音频文件
  mp3: FileAudio,
  wav: FileAudio,
  ogg: FileAudio,
  flac: FileAudio,
  aac: FileAudio,
  // 压缩文件
  zip: FileArchive,
  tar: FileArchive,
  gz: FileArchive,
  rar: FileArchive,
  '7z': FileArchive,
  // 配置文件
  ini: Settings,
  conf: Settings,
  env: Settings,
  // 终端/脚本
  sh: Terminal,
  bash: Terminal,
  zsh: Terminal,
  fish: Terminal,
  bat: Terminal,
  cmd: Terminal,
  ps1: Terminal,
  // 数据库
  sql: Database,
  db: Database,
  sqlite: Database,
}

/** 文件名 → 图标映射 */
const FILENAME_ICON_MAP: Record<string, LucideIcon> = {
  Dockerfile: Terminal,
  Makefile: Terminal,
  Cargo: Settings,
  'package.json': FileJson,
  'tsconfig.json': FileJson,
}

/** 颜色映射 */
const EXTENSION_COLOR_MAP: Record<string, string> = {
  ts: 'text-blue-400',
  tsx: 'text-blue-400',
  js: 'text-yellow-400',
  jsx: 'text-yellow-400',
  py: 'text-green-400',
  rs: 'text-orange-400',
  go: 'text-cyan-400',
  json: 'text-yellow-300',
  md: 'text-gray-400',
  css: 'text-purple-400',
  html: 'text-orange-300',
  svg: 'text-pink-400',
}

interface FileTypeIconProps {
  name: string
  isDirectory: boolean
  isOpen?: boolean
  className?: string
  size?: number
}

export function FileTypeIcon({
  name,
  isDirectory,
  isOpen = false,
  className,
  size = 16,
}: FileTypeIconProps): React.ReactElement {
  if (isDirectory) {
    const Icon = isOpen ? FolderOpen : Folder
    return <Icon className={cn('text-yellow-500/80', className)} size={size} />
  }

  const ext = name.includes('.') ? name.split('.').pop()?.toLowerCase() ?? '' : ''
  const nameKey = Object.keys(FILENAME_ICON_MAP).find((k) => name.startsWith(k))
  const Icon = nameKey ? FILENAME_ICON_MAP[nameKey]! : EXTENSION_ICON_MAP[ext] ?? File
  const colorClass = EXTENSION_COLOR_MAP[ext] ?? 'text-muted-foreground/60'

  return <Icon className={cn(colorClass, className)} size={size} />
}
