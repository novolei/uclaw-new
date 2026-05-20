/**
 * bili-reply -- browser_run script
 *
 * Runs on a bilibili.com page and sends a reply through Bilibili's reply/add
 * API using the logged-in browser session cookies.
 */
async (params) => {
  const { oid, reply_type, root, parent, message } = params
  const log = (...args) => console.log('[bili-reply]', ...args)

  if (!oid) return { success: false, error: '缺少参数: oid' }
  if (!reply_type) return { success: false, error: '缺少参数: reply_type' }
  if (!root) return { success: false, error: '缺少参数: root' }
  if (!parent) return { success: false, error: '缺少参数: parent' }
  if (!message?.trim()) return { success: false, error: '回复内容不能为空' }
  if (message.trim().length > 500) {
    return { success: false, error: `回复内容过长: ${message.trim().length} 字（上限 500）` }
  }

  const csrf = document.cookie
    .split(';')
    .map((c) => c.trim())
    .find((c) => c.startsWith('bili_jct='))
    ?.split('=')[1]

  if (!csrf) {
    return { success: false, error: '未找到 bili_jct cookie，请确认已登录 B 站' }
  }

  try {
    const formData = new URLSearchParams()
    formData.append('oid', String(oid))
    formData.append('type', String(reply_type))
    formData.append('message', message.trim())
    formData.append('scene', 'msg')
    formData.append('plat', '1')
    formData.append('from', 'im-reply')
    formData.append('build', '0')
    formData.append('mobi_app', 'web')
    formData.append('root', String(root))
    formData.append('parent', String(parent))
    formData.append('csrf', csrf)

    log('sending reply', { oid, reply_type, root, parent, msgLen: message.length })
    const fetchResp = await fetch('https://api.bilibili.com/x/v2/reply/add', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      credentials: 'include',
      body: formData.toString()
    })
    const respData = await fetchResp.json().catch(() => null)

    if (!respData) {
      return { success: false, error: '回复 API 返回无法解析的响应' }
    }
    if (respData.code !== 0) {
      const hints = {
        '-101': '账号未登录',
        '-111': 'CSRF 校验失败',
        '-400': '请求参数错误',
        '12002': '评论区已关闭',
        '12015': '回复过于频繁，请稍后再试',
        '12035': '该评论已不存在'
      }
      const hint = hints[String(respData.code)]
      return {
        success: false,
        error: `API 错误 code=${respData.code}: ${respData.message}${hint ? ' (' + hint + ')' : ''}`,
        code: respData.code
      }
    }

    return {
      success: true,
      comment_id: respData.data?.rpid || respData.data?.reply?.rpid,
      message: message.trim()
    }
  } catch (err) {
    return { success: false, error: String(err?.message || err) }
  }
}
