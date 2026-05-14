import * as React from 'react'
import { PackageOpen } from 'lucide-react'

export function AppsTab(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center h-full text-muted-foreground p-8">
      <PackageOpen size={32} className="text-muted-foreground/30 mb-3" />
      <p className="text-[14px] font-medium mb-1">我的应用</p>
      <p className="text-[12px] text-muted-foreground max-w-md text-center">
        MCP 服务 / 复用技能 / 扩展程序的管理界面将在 Phase 3b 开放，配合多注册表支持一起发布。
      </p>
    </div>
  )
}
