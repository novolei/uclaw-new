async (params) => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  for (let i = 0; i < 10; i += 1) {
    if (document.body && document.body.innerText && document.body.innerText.trim().length > 20) break;
    await sleep(500);
  }

  const clickFirst = (selector) => {
    const el = document.querySelector(selector);
    if (!el) return false;
    try {
      el.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, cancelable: true }));
      el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      el.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true }));
      el.click();
      return true;
    } catch {
      return false;
    }
  };

  const activationClicks = [
    'video',
    '[class*="player"]',
    '[class*="Player"]',
    '[data-e2e*="player"]',
    '[data-e2e*="live"]',
    'body',
  ].map(clickFirst);

  await sleep(1500);

  const clean = (value) => String(value || '')
    .replace(/\s+/g, ' ')
    .replace(/_ 抖音直播/, '')
    .replace(/ - 抖音直播/, '')
    .replace(/ \| 抖音直播/, '')
    .trim();
  const pickText = (selectors) => {
    for (const selector of selectors) {
      const value = clean(document.querySelector(selector)?.textContent);
      if (value) return value;
    }
    return '';
  };
  const pickMeta = (names) => {
    for (const name of names) {
      const value = clean(document.querySelector(`meta[property="${name}"], meta[name="${name}"]`)?.getAttribute('content'));
      if (value) return value;
    }
    return '';
  };

  const text = document.body?.innerText || '';
  const url = location.href;
  const documentTitle = clean(document.title);
  const hostName = pickText([
    '[data-e2e="live-room-nickname"]',
    '[data-e2e*="anchor"]',
    '[data-e2e*="user"]',
    '[class*="anchor"]',
    '[class*="Anchor"]',
    '[class*="nickname"]',
    '[class*="Nickname"]',
    '[class*="name"]',
  ]);
  const title = pickMeta(['og:title', 'twitter:title'])
    || pickText(['h1', '[data-e2e*="room-title"]', '[class*="room-title"]', '[class*="RoomTitle"]', '[class*="title"]'])
    || documentTitle
    || (hostName ? `${hostName}的抖音直播间` : '')
    || 'Douyin Live Room';
  const commentInput = document.querySelector('[contenteditable="true"], textarea, input:not([type]), input[type="text"]');
  const hasCommentInput = Boolean(commentInput);
  const commentInputDisabled = Boolean(commentInput && (commentInput.disabled || commentInput.getAttribute('aria-disabled') === 'true'));
  const hasVideo = Boolean(document.querySelector('video'));
  const hasLiveSignal = /直播|LIVE|正在直播|关注|粉丝团|礼物|发送/.test(text) || /live\.douyin\.com/.test(url);
  const chatLoginRequired = /需先登录，才能开始聊天|登录后.*聊天|登录可.*发言|登录后.*发言/.test(text);
  const canChat = hasCommentInput && !commentInputDisabled && !chatLoginRequired;
  const loginPromptVisible = /登录后|登录即可|扫码登录|验证码|请登录/.test(text)
    || Boolean(Array.from(document.querySelectorAll('button, [role="button"], a')).find((el) => /登录/.test(el.innerText || el.getAttribute('aria-label') || '')));
  const hardLoginRequired = loginPromptVisible && !hasVideo && (!hasCommentInput || commentInputDisabled) && !hasLiveSignal;
  const blocked = /访问过于频繁|安全验证|请完成验证|环境异常|captcha/i.test(text);
  const roomId = params.configuredRoomId || location.pathname.replace(/[^a-zA-Z0-9_-]/g, '-').replace(/^-+|-+$/g, '') || 'unknown-room';

  return {
    ok: !blocked && !hardLoginRequired,
    status: blocked ? 'blocked' : (hardLoginRequired ? 'login_required' : (hasLiveSignal || hasVideo || hasCommentInput ? 'entered' : 'unknown')),
    roomId,
    roomTitle: title,
    hostName,
    hostId: null,
    url,
    hasCommentInput,
    commentInputDisabled,
    canChat,
    chatLoginRequired,
    hasVideo,
    hasLiveSignal,
    loginPromptVisible,
    loginRequired: hardLoginRequired,
    activationClicks,
  };
}
