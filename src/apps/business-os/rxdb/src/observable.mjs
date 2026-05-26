export class CtoxSubject {
  constructor(initialValue) {
    this.value = initialValue;
    this.listeners = new Set();
  }

  next(value) {
    this.value = value;
    for (const listener of [...this.listeners]) {
      listener(value);
    }
  }

  subscribe(listener) {
    this.listeners.add(listener);
    if (this.value !== undefined) {
      listener(this.value);
    }
    return {
      unsubscribe: () => this.listeners.delete(listener),
    };
  }

  getValue() {
    return this.value;
  }
}
