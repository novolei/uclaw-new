/**
 * ToolsTab — composes ToolSettings + PermissionsSettings + SkillsSettings.
 */
import * as React from 'react'
import { ToolSettings } from './ToolSettings'
import { PermissionsSettings } from './PermissionsSettings'
import { SkillsSettings } from './SkillsSettings'

export function ToolsTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="工具与 MCP">
        <ToolSettings />
      </section>
      <section data-settings-section="工具权限">
        <PermissionsSettings />
      </section>
      <section data-settings-section="已学技能">
        <SkillsSettings />
      </section>
    </div>
  )
}
