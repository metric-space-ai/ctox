export function createEventBus() {
  const listeners = new Map();

  function subscribe(event, callback) {
    if (!listeners.has(event)) listeners.set(event, []);
    const id = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
    const token = { id, cb: callback };
    listeners.get(event).push(token);
    return token;
  }

  function once(event, callback) {
    const token = subscribe(event, (data) => {
      unsubscribe(event, token);
      callback(data);
    });
    return token;
  }

  function unsubscribe(event, tokenOrId) {
    const handlers = listeners.get(event);
    if (!handlers) return;
    const id = typeof tokenOrId === 'string' ? tokenOrId : tokenOrId?.id;
    const next = handlers.filter((t) => t.id !== id);
    if (next.length) listeners.set(event, next);
    else listeners.delete(event);
  }

  function clear(event) {
    if (event === undefined) listeners.clear();
    else listeners.delete(event);
  }

  function publish(event, data) {
    const handlers = listeners.get(event);
    if (!handlers) return;
    for (const t of [...handlers]) {
      try {
        t.cb(data);
      } catch (error) {
        console.error(`[desktop] event "${event}" handler threw:`, error);
      }
    }
  }

  function publishAsync(event, data) {
    setTimeout(() => publish(event, data), 0);
  }

  return {
    subscribe,
    unsubscribe,
    once,
    clear,
    publish,
    publishAsync,
    on: (event, cb) => subscribe(event, cb),
    off: (event, token) => unsubscribe(event, token),
    emit: (event, data) => publish(event, data),
    emitAsync: (event, data) => publishAsync(event, data),
  };
}
