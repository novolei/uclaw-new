async () => {
  const text = document.body?.innerText || '';
  const hasEndedText = /直播已结束|主播已结束直播|live has ended|replay/i.test(text);
  const input = document.querySelector('[contenteditable="true"], textarea, input[type="text"]');
  const url = location.href;
  const signals = [];
  if (hasEndedText) signals.push('ended_text');
  if (!input || input.disabled || input.getAttribute('aria-disabled') === 'true') signals.push('no_comment_input');
  if (/replay|record|profile|user/.test(url) && !/live/.test(url)) signals.push('non_live_url');
  const endedScore = signals.filter((signal) => ['ended_text', 'no_comment_input', 'non_live_url'].includes(signal)).length;
  return {
    status: endedScore >= 2 ? 'ended' : 'live',
    signals,
    reason: hasEndedText ? 'room ended text detected' : ''
  };
}
