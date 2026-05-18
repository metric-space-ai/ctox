# Sudo Rules

- Preferred secret reference:
  - `runtime/secrets/ctox-sudo.env`
- Expected key:
  - `CTOX_SUDO_PASSWORD`

Use this only for local host operations that truly require privilege, such as:

- package installation
- service enable/start via systemd
- privileged container runtime control
- writing protected config files

If `sudo -n` already works, the helper is optional.
If `sudo -n` fails and no secret reference exists, the task must stay blocked.
