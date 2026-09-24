export const CREDENTIAL_REVEAL_METHOD = 'ctox.credentials.reveal.v1';

// Never relay a credential response through BroadcastChannel, including a
// follower from an older shell calling the leader's native-request handler.
export function assertNativeRequestPrivacy(method, { relayed = false, isLeader = true } = {}) {
  if (method === CREDENTIAL_REVEAL_METHOD && (relayed || !isLeader)) {
    const error = new Error('Open Credentials in the directly connected Business OS tab.');
    error.code = 'credential_reveal_direct_tab_required';
    throw error;
  }
}
