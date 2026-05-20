/**
 * bili-get-messages -- browser_run script
 *
 * Runs in the message.bilibili.com/#/reply page context. It fetches the
 * logged-in creator's "reply to me" notifications and returns the reply/add
 * parameters required by bili-reply.
 */
async (params) => {
  const { pages = 1, cursor_id = 0, cursor_time = 0 } = params
  const log = (...args) => console.log('[bili-get-messages]', ...args)
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

  try {
    const navResp = await fetch('https://api.bilibili.com/x/web-interface/nav', {
      credentials: 'include'
    }).then((r) => r.json()).catch(() => null)

    if (!navResp || navResp.code !== 0 || !navResp.data?.isLogin) {
      return { success: false, logged_in: false, error: '未登录 B站，请先在浏览器中登录' }
    }

    const maxPages = Math.min(Math.max(pages, 1), 5)
    const notifications = []
    let cursor = null
    let fetchUrl = 'https://api.bilibili.com/x/msgfeed/reply?platform=web&build=0&mobi_app=web'
    if (cursor_id && cursor_time) {
      fetchUrl += `&id=${cursor_id}&time=${cursor_time}`
    }

    for (let page = 1; page <= maxPages; page += 1) {
      log('fetch page', page, fetchUrl)
      const resp = await fetch(fetchUrl, { credentials: 'include' })
        .then((r) => r.json())
        .catch(() => null)

      if (!resp || resp.code !== 0) {
        if (page === 1) {
          return {
            success: false,
            logged_in: true,
            error: `API 返回错误: code=${resp?.code} msg=${resp?.message}`
          }
        }
        break
      }

      cursor = resp.data?.cursor ?? null
      for (const item of resp.data?.items || []) {
        notifications.push(extractNotification(item))
      }

      if (cursor?.is_end || !(resp.data?.items || []).length || page >= maxPages) break
      fetchUrl = `https://api.bilibili.com/x/msgfeed/reply?platform=web&build=0&mobi_app=web&id=${cursor.id}&time=${cursor.time}`
      await sleep(500)
    }

    return {
      success: true,
      logged_in: true,
      my_mid: navResp.data.mid,
      my_nickname: navResp.data.uname,
      notifications,
      total: notifications.length,
      cursor,
      is_end: cursor?.is_end ?? true
    }
  } catch (err) {
    return { success: false, error: String(err?.message || err) }
  }

  function extractNotification(item) {
    const i = item.item || {}
    const rootParam = i.root_id && i.root_id !== 0 ? i.root_id : i.source_id
    return {
      id: item.id,
      user_nickname: item.user?.nickname || '',
      user_mid: item.user?.mid || 0,
      comment: i.source_content || '',
      replied_to: i.target_reply_content || '',
      root_content: i.root_reply_content || '',
      notification_type: i.type || 'video',
      video_title: i.title || '',
      video_desc: i.desc || '',
      video_url: i.uri || '',
      is_multi: item.is_multi === 1,
      counts: item.counts || 1,
      oid: i.subject_id,
      reply_type: i.business_id,
      root: rootParam,
      parent: i.source_id,
      reply_time: item.reply_time || 0
    }
  }
}
