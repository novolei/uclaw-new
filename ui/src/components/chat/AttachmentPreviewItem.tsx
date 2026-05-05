/**
 * AttachmentPreviewItem - 附件预览卡片
 */

import * as React from 'react'
import { X, Paperclip } from 'lucide-react'
import { cn } from '@/lib/utils'
import { ImageLightbox } from '@/components/ui/image-lightbox'

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
        'group/attachment relative flex items-center gap-2 shrink-0',
        'rounded-lg bg-[#37a5aa]/10 border border-[#37a5aa]/20',
        'pl-2.5 pr-7 py-1.5 text-[13px] text-[#37a5aa]',
        'transition-colors hover:bg-[#37a5aa]/15',
        className
      )}
    >
      <Paperclip className="size-4 shrink-0" />
      <span className="max-w-[160px] truncate">{truncateName(filename)}</span>
      <button
        type="button"
        onClick={onRemove}
        className={cn(
          'absolute top-1/2 right-1.5 -translate-y-1/2 size-[18px] rounded-full',
          'flex items-center justify-center',
          'text-[#37a5aa]/60 hover:text-[#37a5aa] hover:bg-[#37a5aa]/20',
          'opacity-0 group-hover/attachment:opacity-100 transition-all duration-200'
        )}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}
