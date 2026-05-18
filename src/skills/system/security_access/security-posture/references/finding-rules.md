# Security Posture Finding Rules

Keep this skill narrow:

- use `security_posture` for exposure, privilege, certificate, secret, firewall, and hardening questions
- use `reliability_ops` for health and saturation questions
- use `change_lifecycle` for the actual remediation when it mutates live state

Conservative interpretation:

- a public listener, weak file permissions, or certificate metadata is enough for a posture finding
- do not infer exploitability from posture alone
- do not rotate secrets, revoke privileges, or rewrite firewall rules unless the user asked for execution or another approved path exists
