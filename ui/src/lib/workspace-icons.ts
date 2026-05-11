/**
 * Workspace icon catalog — single source of truth for picker grid +
 * lookup. `workspace.icon` is a string that historically held an emoji;
 * Phase 4b switches to lucide icon names. Legacy emoji values are
 * preserved by the lookup so existing workspaces continue to render
 * correctly without backend migration.
 */

import {
  Star, Bookmark, Heart, Flag, Zap, Triangle, Asterisk, Bell,
  Lightbulb, Droplet, Grid3x3, LayoutGrid, Layers, Database, Inbox, Files,
  Folder, Mail, Calendar, Check, FileText, BookOpen, MessageSquare, MessageCircle,
  Users, Terminal, Wrench, Square, Sun, Moon, Cloud, Circle,
  Globe, Leaf, CloudRain, PawPrint, ShoppingBasket, Gift, Bed, Utensils,
  Dumbbell, Plane, Music, Palette, Video, Bandage, Code, Trophy,
  CloudSun, Map, Flame, Pizza, Skull, NotebookPen, ThumbsUp, Train,
  Briefcase, Rocket, Microscope, PenTool, Target, Home,
  type LucideIcon,
} from 'lucide-react'

/** Ordered icon catalog for the picker grid. ~56 icons in 8-wide rows. */
export const WORKSPACE_ICON_CATALOG: ReadonlyArray<{ name: string; component: LucideIcon }> = [
  // Row 1: symbols
  { name: 'Star', component: Star },
  { name: 'Bookmark', component: Bookmark },
  { name: 'Heart', component: Heart },
  { name: 'Flag', component: Flag },
  { name: 'Zap', component: Zap },
  { name: 'Triangle', component: Triangle },
  { name: 'Asterisk', component: Asterisk },
  { name: 'Bell', component: Bell },
  // Row 2: ideas & containers
  { name: 'Lightbulb', component: Lightbulb },
  { name: 'Droplet', component: Droplet },
  { name: 'Grid3x3', component: Grid3x3 },
  { name: 'LayoutGrid', component: LayoutGrid },
  { name: 'Layers', component: Layers },
  { name: 'Database', component: Database },
  { name: 'Inbox', component: Inbox },
  { name: 'Files', component: Files },
  // Row 3: documents
  { name: 'Folder', component: Folder },
  { name: 'Mail', component: Mail },
  { name: 'Calendar', component: Calendar },
  { name: 'Check', component: Check },
  { name: 'FileText', component: FileText },
  { name: 'BookOpen', component: BookOpen },
  { name: 'MessageSquare', component: MessageSquare },
  { name: 'MessageCircle', component: MessageCircle },
  // Row 4: people / tools
  { name: 'Users', component: Users },
  { name: 'Terminal', component: Terminal },
  { name: 'Wrench', component: Wrench },
  { name: 'Square', component: Square },
  { name: 'Sun', component: Sun },
  { name: 'Moon', component: Moon },
  { name: 'Cloud', component: Cloud },
  { name: 'Circle', component: Circle },
  // Row 5: nature & life
  { name: 'Globe', component: Globe },
  { name: 'Leaf', component: Leaf },
  { name: 'CloudRain', component: CloudRain },
  { name: 'PawPrint', component: PawPrint },
  { name: 'ShoppingBasket', component: ShoppingBasket },
  { name: 'Gift', component: Gift },
  { name: 'Bed', component: Bed },
  { name: 'Utensils', component: Utensils },
  // Row 6: activities
  { name: 'Dumbbell', component: Dumbbell },
  { name: 'Plane', component: Plane },
  { name: 'Music', component: Music },
  { name: 'Palette', component: Palette },
  { name: 'Video', component: Video },
  { name: 'Bandage', component: Bandage },
  { name: 'Code', component: Code },
  { name: 'Trophy', component: Trophy },
  // Row 7: travel / misc
  { name: 'CloudSun', component: CloudSun },
  { name: 'Map', component: Map },
  { name: 'Flame', component: Flame },
  { name: 'Pizza', component: Pizza },
  { name: 'Skull', component: Skull },
  { name: 'NotebookPen', component: NotebookPen },
  { name: 'ThumbsUp', component: ThumbsUp },
  { name: 'Train', component: Train },
]

/** Quick-lookup map for getWorkspaceIcon. */
const WORKSPACE_ICON_BY_NAME: Record<string, LucideIcon> = Object.fromEntries(
  WORKSPACE_ICON_CATALOG.map((entry) => [entry.name, entry.component])
)

/** Legacy emoji fallbacks — covers the EMOJI_CHOICES set from the
 *  pre-Phase-4b WorkspaceCreateDialog. Existing workspaces with these
 *  values keep rendering as a sensible icon without a backend migration. */
const LEGACY_EMOJI_MAP: Record<string, LucideIcon> = {
  '📁': Folder,
  '💼': Briefcase,
  '🚀': Rocket,
  '🔬': Microscope,
  '✍️': PenTool,
  '🎯': Target,
  '🏠': Home,
  '⚙️': Wrench,
}

/** Default icon name for newly-created workspaces. */
export const DEFAULT_WORKSPACE_ICON = 'Folder'

/**
 * Resolve a workspace.icon value to a lucide icon component.
 *
 * - New workspaces: stored as a lucide icon name (e.g. "Folder", "Star")
 * - Legacy workspaces: stored as an emoji (e.g. "📁")
 * - Anything else: falls back to Folder.
 */
export function getWorkspaceIcon(value: string | null | undefined): LucideIcon {
  if (!value) return Folder
  return WORKSPACE_ICON_BY_NAME[value] ?? LEGACY_EMOJI_MAP[value] ?? Folder
}
