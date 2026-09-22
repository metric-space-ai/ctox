// Ordered RPC execution is separate from ordered transport-frame ingestion.
// Budgets include running handlers, even after cancellation until they settle:
// an adapter that ignores AbortSignal cannot create unbounded detached work.
export class InboundRequestQueue {
  constructor({ maxCount, maxBytes, onError }) {
    this.maxCount = maxCount;
    this.maxBytes = maxBytes;
    this.onError = onError;
    this.count = 0;
    this.bytes = 0;
    this.owners = new Map();
  }

  enqueue(owner, bytes, run) {
    if (!owner || !Number.isFinite(bytes) || bytes < 0
      || this.count >= this.maxCount || this.bytes + bytes > this.maxBytes) return false;
    let queue = this.owners.get(owner);
    if (!queue) {
      queue = { owner, pending: [], active: false, cancelled: false, abort: new AbortController() };
      this.owners.set(owner, queue);
    }
    if (queue.cancelled) return false;
    queue.pending.push({ bytes, run });
    this.count++;
    this.bytes += bytes;
    if (!queue.active) {
      queue.active = true;
      // drain catches handler failures and always releases accounting.
      void this.drain(queue);
    }
    return true;
  }

  async drain(queue) {
    while (!queue.cancelled && queue.pending.length) {
      const entry = queue.pending.shift();
      try {
        await entry.run(queue.abort.signal);
      } catch (error) {
        if (!queue.cancelled) {
          try { this.onError?.(error); } catch { /* An observer cannot poison cleanup. */ }
        }
      } finally {
        this.count--;
        this.bytes -= entry.bytes;
      }
    }
    queue.active = false;
    if (this.owners.get(queue.owner) === queue) this.owners.delete(queue.owner);
  }

  cancel(owner) {
    const queue = this.owners.get(owner);
    if (!queue) return;
    queue.cancelled = true;
    for (const entry of queue.pending.splice(0)) {
      this.count--;
      this.bytes -= entry.bytes;
    }
    queue.abort.abort();
    // The running handler retains its reservation until it actually settles.
  }
}
