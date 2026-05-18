/**
 * Visual mapping for IM-origin agent sessions.
 *
 * When `imChannelType` is present on a session (sourced from the
 * `im_sessions` JOIN in list_agent_sessions), the sidebar item and tab
 * surface a channel-specific marker so users can tell at a glance which
 * sessions are bridged from external IM channels.
 *
 * Channel keys mirror `crate::channels::types::ImChannelType::as_str` on
 * the backend. Channels with a real logo asset render an `<img>`; the
 * rest fall back to an emoji glyph.
 */

import wechatLogo from '@/assets/channel-logos/channel_logo_wechat.png'
import feishuLogo from '@/assets/channel-logos/channel_logo_feishu.png'

export interface ImChannelDisplay {
  /** Imported logo URL when a real asset exists. Components prefer this
   *  over `emoji` whenever present. */
  logoSrc?: string
  /** Emoji fallback for channels without a logo. Always present. */
  emoji: string
  /** Short Chinese label for tooltips and accessibility text. */
  label: string
}

const CHANNEL_TABLE: Record<string, ImChannelDisplay> = {
  // WeChat personal (iLink) — uses the WeChat logo.
  wechat_ilink: { logoSrc: wechatLogo, emoji: '💬', label: '微信' },
  // Enterprise WeChat (WeCom) — no dedicated logo yet, reuse WeChat asset
  // (same brand family) with the distinct Chinese label disambiguating it
  // in tooltips. Swap to a dedicated wecom logo when available.
  wecom_bot:    { logoSrc: wechatLogo, emoji: '🏢', label: '企业微信' },
  feishu:       { logoSrc: feishuLogo, emoji: '🪶', label: '飞书' },
  // No logo assets yet — fall back to emoji.
  dingtalk:     { emoji: '🔔', label: '钉钉' },
  email:        { emoji: '✉️', label: '邮件' },
  webhook:      { emoji: '🪝', label: 'Webhook' },
}

const UNKNOWN_CHANNEL: ImChannelDisplay = { emoji: '📩', label: 'IM' }

export function imChannelDisplay(channelType: string | undefined | null): ImChannelDisplay | null {
  if (!channelType) return null
  return CHANNEL_TABLE[channelType] ?? UNKNOWN_CHANNEL
}
