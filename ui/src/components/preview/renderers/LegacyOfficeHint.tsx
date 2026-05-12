import * as React from 'react'
import { FileWarning } from 'lucide-react'

interface LegacyOfficeHintProps {
  name: string
  ext: string
}

const FORMAT_LABEL: Record<string, string> = {
  doc: 'Word 97-2003 文档 (.doc)',
  xls: 'Excel 97-2003 工作簿 (.xls)',
  ppt: 'PowerPoint 97-2003 演示文稿 (.ppt)',
}

const NEW_FORMAT: Record<string, string> = {
  doc: '.docx',
  xls: '.xlsx',
  ppt: '.pptx',
}

export function LegacyOfficeHint({ name, ext }: LegacyOfficeHintProps): React.ReactElement {
  const label = FORMAT_LABEL[ext] ?? `Legacy ${ext.toUpperCase()}`
  const newFmt = NEW_FORMAT[ext] ?? '.docx / .xlsx / .pptx'
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center select-none bg-popover">
      <div className="size-14 rounded-full bg-amber-500/10 flex items-center justify-center mb-4">
        <FileWarning className="size-7 text-amber-700 dark:text-amber-300" aria-hidden />
      </div>
      <div className="text-[13px] font-medium text-foreground/85 mb-1">
        暂不支持预览此格式
      </div>
      <div className="text-[11.5px] text-muted-foreground max-w-[320px] leading-relaxed mb-3">
        {label}
      </div>
      <div className="text-[11.5px] text-muted-foreground/80 max-w-[320px] leading-relaxed">
        请使用 Microsoft Office、Pages/Numbers/Keynote 或 LibreOffice 将文件
        另存为 <span className="font-mono text-foreground/75">{newFmt}</span> 格式
        后再次预览。
      </div>
      <div
        className="mt-4 text-[10.5px] text-muted-foreground/60 font-mono tabular-nums max-w-[320px] break-words"
        title={name}
      >
        {name}
      </div>
    </div>
  )
}
