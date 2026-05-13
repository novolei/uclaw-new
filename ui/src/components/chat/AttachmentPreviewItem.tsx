/**
 * AttachmentPreviewItem - 附件预览卡片
 */

import * as React from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { ImageLightbox } from '@/components/ui/image-lightbox'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'

interface AttachmentPreviewItemProps {
  filename: string
  mediaType: string
  previewUrl?: string
  onRemove: () => void
  className?: string
}

function isImage(mediaType: string): boolean {
  return mediaType.startsWith('image/')
}

function truncateName(name: string, max: number = 20): string {
  return name.length > max ? name.slice(0, max - 3) + '...' : name
}

export function AttachmentPreviewItem({
  filename,
  mediaType,
  previewUrl,
  onRemove,
  className,
}: AttachmentPreviewItemProps): React.ReactElement {
  const [lightboxOpen, setLightboxOpen] = React.useState(false)

  if (isImage(mediaType) && previewUrl) {
    return (
      <div
        className={cn(
          'group/attachment relative size-[72px] shrink-0 rounded-lg overflow-hidden',
          className
        )}
      >
        <img
          src={previewUrl}
          alt={filename}
          className="size-full object-cover cursor-pointer"
          onClick={() => setLightboxOpen(true)}
        />
        <button
          type="button"
          onClick={onRemove}
          className={cn(
            'absolute top-1 right-1 size-[18px] rounded-full',
            'bg-black/50 text-white backdrop-blur-sm',
            'flex items-center justify-center',
            'opacity-0 group-hover/attachment:opacity-100 transition-opacity duration-200',
            'hover:bg-black/70'
          )}
        >
          <X className="size-3" />
        </button>
        <ImageLightbox
          src={previewUrl}
          alt={filename}
          open={lightboxOpen}
          onOpenChange={setLightboxOpen}
        />
      </div>
    )
  }

  return (
    <div
      className={cn(
        'group/attachment relative flex items-center gap-1.5 shrink-0',
        'rounded-md bg-foreground/[0.04] border border-border/60',
        'pl-1.5 pr-6 py-1 text-[12px] text-foreground/85',
        'transition-colors hover:bg-foreground/[0.07] hover:border-border',
        className
      )}
      title={filename}
    >
      <FileTypeIcon name={filename} isDirectory={false} size={14} className="shrink-0" />
      <span className="max-w-[160px] truncate leading-tight">{truncateName(filename)}</span>
      <button
        type="button"
        onClick={onRemove}
        aria-label="移除附件"
        className={cn(
          'absolute top-1/2 right-1 -translate-y-1/2 size-[16px] rounded-full',
          'flex items-center justify-center',
          'text-foreground/45 hover:text-foreground hover:bg-foreground/10',
          'opacity-0 group-hover/attachment:opacity-100 transition-all duration-150',
          'focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
        )}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}
