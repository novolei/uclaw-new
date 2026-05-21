/**
 * ProviderAvatar — shared assistant avatar.
 *
 * Resolves a provider logo via `getModelLogo` and falls back to a Bot icon
 * when the model id doesn't match a known provider (so we never render
 * `<img src="">` and get the broken-image glyph).
 *
 * Used by both Chat and Agent message headers so the avatar stays visually
 * identical across modes.
 */

import * as React from 'react'
import { Bot } from 'lucide-react'
import { getModelLogo } from '@/lib/model-logo'
import { cn } from '@/lib/utils'

export interface ProviderAvatarProps {
  /** Raw model id — e.g. "claude-sonnet-4-6", "deepseek-v4-pro". */
  model?: string | null
  /** Optional provider hint — overrides modelId-based inference. */
  provider?: string
  /** Square edge length in px. Default 35 (matches the message header gutter). */
  size?: number
  /** Optional class overrides for the outer element. */
  className?: string
}

export function ProviderAvatar({
  model,
  provider,
  size = 35,
  className,
}: ProviderAvatarProps): React.ReactElement {
  const logoUrl = model ? getModelLogo(model, provider) : ''
  const dimensionStyle = { width: size, height: size }

  if (logoUrl) {
    return (
      <img
        src={logoUrl}
        alt={model ?? 'AI'}
        style={dimensionStyle}
        className={cn('rounded-[25%] object-cover bg-muted/30 shrink-0', className)}
      />
    )
  }

  return (
    <div
      style={dimensionStyle}
      className={cn(
        'rounded-[25%] bg-primary/10 flex items-center justify-center shrink-0',
        className,
      )}
    >
      <Bot size={Math.round(size * 0.51)} className="text-primary" />
    </div>
  )
}
