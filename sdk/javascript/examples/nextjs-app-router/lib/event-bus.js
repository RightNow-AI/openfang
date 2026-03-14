/**
 * lib/event-bus.js
 *
 * In-memory pub/sub for run events.
 *
 * Subscribers register by runId and receive normalized RunEvent objects.
 * Events are also stored on the run record (via run-store) so latecomers
 * can replay any events they missed before subscribing.
 *
 * Thread safety: single Node.js process, synchronous subscriber dispatch.
 * Events are never held; if no subscriber is listening they are silently dropped
 * (the SSE client can use the replay buffer in run-store to catch up).
 *
 * @typedef {import('./alive-service').RunEvent} RunEvent
 */

'use strict';

/** @type {Map<string, Set<(event: RunEvent) => void>>} */
const subscribers = new Map();

const eventBus = {
  /**
   * Subscribe to events for a specific run.
   *
   * Returns an unsubscribe function.
   *
   * @param {string} runId
   * @param {(event: RunEvent) => void} callback
   * @returns {() => void}
   */
  subscribe(runId, callback) {
    if (!subscribers.has(runId)) {
      subscribers.set(runId, new Set());
    }
    subscribers.get(runId).add(callback);

    return function unsubscribe() {
      const set = subscribers.get(runId);
      if (!set) return;
      set.delete(callback);
      if (set.size === 0) subscribers.delete(runId);
    };
  },

  /**
   * Emit an event to all current subscribers of a run.
   * Never throws — subscriber errors are swallowed.
   *
   * @param {string} runId
   * @param {RunEvent} event
   */
  emit(runId, event) {
    const set = subscribers.get(runId);
    if (!set || set.size === 0) return;
    for (const cb of set) {
      try {
        cb(event);
      } catch {
        // subscriber errors must never crash the emitter
      }
    }
  },

  /** @param {string} runId */
  hasSubscribers(runId) {
    return (subscribers.get(runId)?.size ?? 0) > 0;
  },

  /** Active subscriber count (useful for debugging) */
  get size() {
    let total = 0;
    for (const set of subscribers.values()) total += set.size;
    return total;
  },
};

module.exports = { eventBus };
