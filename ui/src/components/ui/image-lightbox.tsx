// [PLACEHOLDER] ui/image-lightbox — 图片灯箱组件
import * as React from 'react'

interface ImageLightboxProps {
  src: string
  alt: string
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ImageLightbox({ src, alt, open, onOpenChange }: ImageLightboxProps): React.ReactElement | null {
  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm"
      onClick={() => onOpenChange(false)}
    >
      <img
        src={src}
        alt={alt}
        className="max-w-[90vw] max-h-[90vh] object-contain rounded-lg"
        onClick={(e) => e.stopPropagation()}
      />
    </div>
  )
}
