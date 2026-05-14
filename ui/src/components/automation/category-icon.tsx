/**
 * CategoryIcon — single source of truth for category / item-icon → lucide mapping.
 *
 * DHP marketplace specs carry an `icon: <keyword>` string (e.g. "news", "social",
 * "content") that is NOT a lucide-react name and NOT an emoji — it's a category
 * keyword from the Halo registry vocabulary. Same vocabulary as the category
 * field. We map both through this one component:
 *
 *   <CategoryIcon name="social" />       // for StoreCard top-left bubble
 *   <CategoryIcon name="productivity" /> // for StoreHeader chip
 *
 * Known categories get a hand-picked lucide icon. Unknown categories (future
 * additions to the DHP registry) get a deterministic fallback — same name always
 * resolves to the same icon, so the UI stays stable across runs and across users.
 */

import * as React from 'react'
import type { LucideIcon } from 'lucide-react'
import {
  MessageCircle,
  Zap,
  FileText,
  Newspaper,
  Database,
  Code2,
  ShoppingBag,
  Package,
  Tag,
  Hash,
  Folder,
  Star,
  Sparkles,
  Bookmark,
  Layers,
  Award,
  Heart,
  Box,
  Globe,
  Briefcase,
} from 'lucide-react'

/** Explicit mappings for known Halo registry categories / icon keywords. */
const KNOWN_ICONS: Record<string, LucideIcon> = {
  social: MessageCircle,
  productivity: Zap,
  content: FileText,
  news: Newspaper,
  data: Database,
  dev: Code2,
  'dev-tools': Code2,
  shopping: ShoppingBag,
  other: Package,
  // common synonyms seen in the wild
  community: Heart,
  market: Briefcase,
  web: Globe,
  bookmark: Bookmark,
  star: Star,
  featured: Sparkles,
}

/** Deterministic fallback pool — unknown categories hash into here. */
const FALLBACK_ICONS: LucideIcon[] = [
  Tag,
  Hash,
  Folder,
  Box,
  Layers,
  Award,
]

function hashString(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) - h + s.charCodeAt(i)) | 0
  }
  return Math.abs(h)
}

export function getCategoryIcon(name: string | null | undefined): LucideIcon {
  if (!name) return Package
  const key = name.toLowerCase().trim()
  return (
    KNOWN_ICONS[key] ??
    FALLBACK_ICONS[hashString(key) % FALLBACK_ICONS.length]
  )
}

interface Props {
  /** Category or icon keyword (e.g. 'social', 'productivity', 'content'). */
  name: string | null | undefined
  size?: number
  className?: string
}

export function CategoryIcon({ name, size = 14, className }: Props): React.ReactElement {
  const Icon = getCategoryIcon(name)
  return <Icon size={size} className={className} />
}
