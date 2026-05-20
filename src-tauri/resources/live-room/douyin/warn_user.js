async (params) => {
  const name = params.authorName || params.authorId || '这位朋友';
  const reason = params.reason || '直播间规则';
  const text = `@${name} 请注意${reason}，请不要继续刷屏或发布不当内容。`;
  const box = document.querySelector('[contenteditable="true"], textarea, input[type="text"]');
  if (!box) return { ok: false, action: 'warn_user', needsBrowserTask: true, error: 'input_not_found', text };
  box.focus();
  if ('value' in box) box.value = text;
  else box.textContent = text;
  box.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
  const send = Array.from(document.querySelectorAll('button, [role="button"]'))
    .find((el) => /发送|send/i.test(el.innerText || el.getAttribute('aria-label') || ''));
  if (!send) return { ok: false, action: 'warn_user', needsBrowserTask: true, error: 'send_button_not_found', text };
  send.click();
  return { ok: true, action: 'warn_user', text };
}
