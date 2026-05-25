import { Badge } from '@/components/ui/badge'
import { SettingsCard, SettingsRow, SettingsSection } from '@/components/settings/primitives'

export function PlaywrightMcpBuiltinDetail() {
  return (
    <div className="mt-4">
      <SettingsSection title="Playwright MCP" description="App-managed provider">
        <SettingsCard>
          <SettingsRow
            label="Status"
            description="Advanced provider, configured through Browser Runtime Control Center"
          >
            <Badge variant="secondary">Built-in integration</Badge>
          </SettingsRow>
          <SettingsRow label="Raw MCP exposure" description="Raw MCP tools locked off" />
          <SettingsRow label="Action boundary" description="Wrapped browser actions only" />
          <SettingsRow label="Runtime pack source" description="uClaw-managed Browser Runtime Pack" />
          <SettingsRow label="Sidecar startup" description="App-managed" />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
