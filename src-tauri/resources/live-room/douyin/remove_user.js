async (params) => {
  const authorId = params.authorId || null;
  const authorName = params.authorName || '';
  const candidates = Array.from(document.querySelectorAll('[data-user-id], [data-id], [class*="comment"], [class*="chat"]'));
  const target = candidates.find((node) => {
    const uid = node.getAttribute('data-user-id') || node.querySelector('[data-user-id]')?.getAttribute('data-user-id');
    const text = node.innerText || '';
    return (authorId && uid === authorId) || (authorName && text.includes(authorName));
  });
  if (!target) return { ok: false, action: 'remove_user', authorId, needsBrowserTask: true, error: 'target_not_found' };
  target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
  target.click();
  const action = Array.from(document.querySelectorAll('button, [role="button"], [role="menuitem"]'))
    .find((el) => /踢出|移出|remove|kick/i.test(el.innerText || el.getAttribute('aria-label') || ''));
  if (!action) return { ok: false, action: 'remove_user', authorId, needsBrowserTask: true, error: 'remove_action_not_found' };
  action.click();
  return {
    ok: true,
    action: 'remove_user',
    authorId,
    reason: params.reason || '',
    verifiedAuthorId: authorId
  };
}
