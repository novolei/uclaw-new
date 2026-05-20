async (params) => {
  return {
    ok: true,
    status: 'entered',
    roomId: params.configuredRoomId || location.pathname.replace(/[^a-zA-Z0-9_-]/g, '-').replace(/^-+|-+$/g, '') || 'unknown-room',
    roomTitle: document.title || 'Douyin Live Room',
    hostId: null,
    url: location.href
  };
}
