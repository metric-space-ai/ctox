export class CtoxEventEmitter {
  constructor() {
    this.target = new EventTarget();
  }

  on(type, listener) {
    this.target.addEventListener(type, listener);
    return () => this.target.removeEventListener(type, listener);
  }

  once(type, listener) {
    const unsubscribe = this.on(type, (event) => {
      unsubscribe();
      listener(event);
    });
    return unsubscribe;
  }

  emit(type, detail = {}) {
    this.target.dispatchEvent(new CustomEvent(type, { detail }));
  }
}

export function waitForEvent(emitter, type, timeoutMs = 10000) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timed out waiting for ${type}`));
    }, timeoutMs);
    const unsubscribe = emitter.once(type, (event) => {
      clearTimeout(timeout);
      resolve(event.detail);
    });
  });
}
