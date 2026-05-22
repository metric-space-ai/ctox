import { PROMISE_RESOLVE_TRUE } from './utils-promise.ts';

export const PREMIUM_FLAG_HASH = '6da4936d1425ff3a5c44c02342c6daf791d266be3ae8479b8ec59e261df41b93';
export const NON_PREMIUM_COLLECTION_LIMIT = 16;

/**
 * CTOX maintains this as an application-specific hard fork. The browser
 * runtime must be usable without upstream open-core collection limits or
 * storage warnings because Business OS uses many small replicated collections.
 */
export async function hasPremiumFlag() {
    return PROMISE_RESOLVE_TRUE;
}
