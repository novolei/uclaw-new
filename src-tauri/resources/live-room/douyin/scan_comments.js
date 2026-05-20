async (params) => {
  const cursor = params.cursor || null;
  const nodes = Array.from(document.querySelectorAll('[data-e2e*="chat"], [class*="comment"], [class*="chat"]'));
  const comments = nodes
    .map((node, index) => {
      const text = (node.innerText || '').trim();
      if (!text) return null;
      return {
        id: node.getAttribute('data-id') || `${Date.now()}-${index}`,
        userId: node.getAttribute('data-user-id') || node.querySelector('[data-user-id]')?.getAttribute('data-user-id') || `unknown-${index}`,
        nickname: node.querySelector('[class*="name"], [data-e2e*="name"]')?.textContent?.trim() || 'unknown',
        text,
        ts: Date.now()
      };
    })
    .filter(Boolean);
  return {
    nextCursor: String(comments.length ? comments[comments.length - 1].id : cursor || ''),
    comments
  };
}
