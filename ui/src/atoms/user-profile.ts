/**
 * User Profile Atom - 用户档案状态
 *
 * 管理用户名和头像。
 * 从 Proma 迁移，类型引用本地化。
 */

import { atom } from 'jotai'
import { DEFAULT_USER_AVATAR, DEFAULT_USER_NAME } from '@/lib/chat-types'
import type { UserProfile } from '@/lib/chat-types'

/** 用户档案 */
export const userProfileAtom = atom<UserProfile>({
  userName: DEFAULT_USER_NAME,
  avatar: DEFAULT_USER_AVATAR,
})
