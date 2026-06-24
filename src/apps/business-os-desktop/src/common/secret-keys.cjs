"use strict";

// Single source of truth for "this key name looks like it holds a secret",
// shared by the registry persistence backstop (instance-model.cjs) and the
// crash-report / support-snapshot redactor (redaction.cjs) so the two can never
// drift apart and miss a credential key one of them knows about.
//
// Deliberately omits a bare `key` term so legitimately-public fields such as
// hostKeyFingerprint / hostKeyAlgorithm are not treated as secrets.
const SECRET_KEY_TERMS = [
  "password",
  "passphrase",
  "secret",
  "token",
  "credential",
  "private[_-]?key",
  "api[_-]?key",
  "access[_-]?key",
  "authorization",
  "bearer",
  "session[_-]?cookie",
  "room[_-]?password",
  "ctox[_-]?config",
];

// Substring match: aggressive, used by the registry backstop so e.g. `roomSecret`
// or `accessKeyHash` are rejected before they can be persisted in cleartext.
const SECRET_KEY_SUBSTRING_RE = new RegExp(`(${SECRET_KEY_TERMS.join("|")})`, "i");

// Segment match: anchored on word boundaries, used by the redactor so a key only
// triggers redaction when one of the terms is a whole segment (apiKey, auth_token,
// room_password) rather than an incidental substring.
const SECRET_KEY_SEGMENT_RE = new RegExp(`(^|[_-])(${SECRET_KEY_TERMS.join("|")})([_-]|$)`, "i");

module.exports = {
  SECRET_KEY_TERMS,
  SECRET_KEY_SUBSTRING_RE,
  SECRET_KEY_SEGMENT_RE,
};
