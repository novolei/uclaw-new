import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsInput } from './primitives/SettingsInput'
import { SettingsSecretInput } from './primitives/SettingsSecretInput'
import { Button } from '@/components/ui/button'
import { configureProvider, getProviderConfig } from '@/lib/tauri-bridge'

interface ChannelFormProps {
  providerId: string | null
  onClose: () => void
  onSaved: () => void
}

export function ChannelForm({ providerId, onClose, onSaved }: ChannelFormProps) {
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (providerId) {
      getProviderConfig(providerId).then((config) => {
        if (config) {
          setBaseUrl(config.baseUrl || '')
          // API key is masked, don't pre-fill
        }
      })
    }
  }, [providerId])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!providerId) return

    setSubmitting(true)
    try {
      await configureProvider({
        providerId,
        displayName: providerId,
        apiKey,
        baseUrl: baseUrl || undefined,
      })
      onSaved()
    } catch (err) {
      console.error('Failed to configure provider:', err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-xl p-6 w-[480px] max-w-[90vw] space-y-4">
        <h3 className="text-base font-semibold">
          配置 Provider: {providerId}
        </h3>

        <form onSubmit={handleSubmit} className="space-y-4">
          <SettingsSection>
            <SettingsSecretInput
              label="API Key"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              required
            />
            <SettingsInput
              label="Base URL（可选）"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
            />
          </SettingsSection>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={submitting}>
              {submitting ? '保存中...' : '保存'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}
