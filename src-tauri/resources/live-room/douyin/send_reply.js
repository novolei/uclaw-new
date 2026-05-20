async (params) => {
  const text = String(params.text || '').trim();
  if (!text) return { ok: false, action: 'send_reply', error: 'empty_text' };
  const box = document.querySelector('[contenteditable="true"], textarea, input[type="text"]');
  if (!box) return { ok: false, action: 'send_reply', needsBrowserTask: true, error: 'input_not_found' };
  box.focus();
  if ('value' in box) box.value = text;
  else box.textContent = text;
  box.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
  const send = Array.from(document.querySelectorAll('button, [role="button"]'))
    .find((el) => /发送|send/i.test(el.innerText || el.getAttribute('aria-label') || ''));
  if (!send) return { ok: false, action: 'send_reply', needsBrowserTask: true, error: 'send_button_not_found' };
  send.click();
  return { ok: true, action: 'send_reply', text };
}
