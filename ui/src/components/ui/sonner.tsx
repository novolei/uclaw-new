import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Toaster as Sonner } from 'sonner'
import { CheckCircle2, XCircle, AlertTriangle, Info, Loader2 } from 'lucide-react'
import { resolvedThemeAtom } from '@/atoms/theme'

type ToasterProps = React.ComponentProps<typeof Sonner>

/**
 * Toaster — global toast container.
 *
 * Design goals (Phase 3 polish):
 * - Theme-reactive: follows `resolvedThemeAtom` so dark themes get
 *   dark toasts and CSS variables stay in scope.
 * - Per-type accent: success/error/warning/info each get a tinted left
 *   border + matching icon, but the background uses the neutral
 *   `bg-popover` token so the toast never clashes with any of the 11
 *   themes (warm-paper, qingye, forest-*, …).
 * - Premium feel: soft shadow, subtle backdrop blur, comfortable
 *   padding, max-width so single-word toasts don't look skinny.
 * - Manual dismiss: a close button appears on hover.
 */
const Toaster = (props: ToasterProps) => {
  const theme = useAtomValue(resolvedThemeAtom)

  return (
    <Sonner
      theme={theme}
      position="top-right"
      offset={58}
      closeButton
      duration={3500}
      icons={{
        success: <CheckCircle2 className="size-4 text-emerald-500" />,
        error: <XCircle className="size-4 text-destructive" />,
        warning: <AlertTriangle className="size-4 text-amber-500" />,
        info: <Info className="size-4 text-primary" />,
        loading: <Loader2 className="size-4 animate-spin text-muted-foreground" />,
      }}
      className="toaster group"
      toastOptions={{
        unstyled: false,
        classNames: {
          toast: [
            'group toast pointer-events-auto',
            // Base — neutral popover surface with theme-safe tokens
            'group-[.toaster]:bg-popover/95 group-[.toaster]:backdrop-blur-md',
            'group-[.toaster]:text-popover-foreground group-[.toaster]:border',
            'group-[.toaster]:border-border/60 group-[.toaster]:shadow-xl',
            // Shape — generous padding, rounded, left accent stripe
            'group-[.toaster]:rounded-lg group-[.toaster]:px-4 group-[.toaster]:py-3',
            'group-[.toaster]:gap-3 group-[.toaster]:min-w-[280px]',
            'group-[.toaster]:max-w-[420px]',
            // Type accents — left border stripe via data-type
            '[&[data-type=success]]:border-l-2 [&[data-type=success]]:border-l-emerald-500',
            '[&[data-type=error]]:border-l-2 [&[data-type=error]]:border-l-destructive',
            '[&[data-type=warning]]:border-l-2 [&[data-type=warning]]:border-l-amber-500',
            '[&[data-type=info]]:border-l-2 [&[data-type=info]]:border-l-primary',
            '[&[data-type=loading]]:border-l-2 [&[data-type=loading]]:border-l-muted-foreground',
          ].join(' '),
          title: 'group-[.toast]:text-sm group-[.toast]:font-medium group-[.toast]:leading-snug',
          description: 'group-[.toast]:text-xs group-[.toast]:text-muted-foreground group-[.toast]:leading-snug',
          actionButton: [
            'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground',
            'group-[.toast]:rounded-md group-[.toast]:px-2.5 group-[.toast]:py-1',
            'group-[.toast]:text-xs group-[.toast]:font-medium',
            'group-[.toast]:hover:bg-primary/90 group-[.toast]:transition-colors',
          ].join(' '),
          cancelButton: [
            'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground',
            'group-[.toast]:rounded-md group-[.toast]:px-2.5 group-[.toast]:py-1 group-[.toast]:text-xs',
            'group-[.toast]:hover:bg-muted/80 group-[.toast]:transition-colors',
          ].join(' '),
          closeButton: [
            'group-[.toast]:bg-transparent group-[.toast]:border-0',
            'group-[.toast]:text-muted-foreground/60 group-[.toast]:hover:text-foreground',
            'group-[.toast]:hover:bg-foreground/[0.06] group-[.toast]:rounded-md',
            'group-[.toast]:size-5 group-[.toast]:transition-colors',
          ].join(' '),
          icon: 'group-[.toast]:flex group-[.toast]:items-center group-[.toast]:shrink-0',
        },
      }}
      {...props}
    />
  )
}

export { Toaster }
