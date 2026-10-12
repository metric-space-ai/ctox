# Installed guest image readiness measurement

`tests/native_guest_readiness.py` measures the existing Linux guest image,
with its installed CTOX desktop endpoint, under the versioned native QEMU
profile. It cold-boots a fresh disposable overlay; the immutable RAW master
is verified by full SHA256 and never written. This is an acceptance component,
not a production VM provisioner or an execution-authority path.

Run only through the shared GPU admission gate, with two workers. Supply an
existing native-owned0700 runtime parent and a fresh empty0700 output directory:

```sh
greppy bash-smart -- ~/.codex/bin/gpu-build-run.sh \
  --owner <thread-id> --task o04-guest-readiness-pr --host gpu3 --jobs 2 \
  --src <worktree> -- timeout 360 python3 tests/native_guest_readiness.py \
  --base <canonical-private-immutable-raw> --base-sha256 <full-sha256> \
  --embedded-native-source <actual-image-native-revision> \
  --owner <thread-id> --work-parent <private-runtime-parent> \
  --output <fresh-private-output> --acceleration kvm --timeout 180
```

The selected image must already be independently provisioned on that host.
Do not use SSH to transport the image, a checkpoint or source credentials.
The test never starts a native host, creates an account/grant, sends a model
request, runs a Core turn or sends desktop input. TCG is an explicit operator
choice, never an automatic fallback for unavailable KVM.

A successful receipt requires:

- The exact owned QEMU child connects to both private Unix sockets, verified
  by Linux peer PID/UID. QMP reports initially paused before the explicit boot.
- The installed guest endpoint returns the assigned guest ID and a canonical
  service-session UUID through the existing length-prefixed virtio protocol.
- A session-bound observation returns a complete CRC-checked PNG whose bounded
  dimensions match the reply. `guest.png` retains that actual X11 observation.
- An observation carrying a different service-session UUID is explicitly
  rejected. A subsequent probe still returns the original guest/session.
- QMP quit succeeds, the exact child exits0 and is reaped, the temporary overlay
  and socket directory are removed, and the immutable base metadata is unchanged.

The absolute deadline covers hashing, child preparation, boot, framed replies
and QMP commands. There is no automatic request replay. On error, the test stops
and reaps only its own child, retaining `receipt.json`, `qemu.log` and the initial
`process-owner.json`. The child stays in the admission runner's process group
so its outer timeout also owns it.

`pass:true` in this component receipt is **not** `goal19_pass:true`. The latter
stays false: service-session rejection is not native controller-right revocation.
Production goal19 additionally requires current real account/group/controller
admission, connector tests, protected staged import, original-service restore,
and the actual two-host continuation/stale-source/reconnect/abort measurements.
The same limitation applies to goals16/18. No synthetic voter, caller clean flag,
restored fixture or generic Transfer receipt can satisfy that authority.

Protocol validation tests:

```sh
python3 -m unittest discover -s tests -p test_native_guest_readiness.py -v
```

They check untrusted framing bounds, EOF, response correlation, PNG integrity
and immutable-image alias/write rejection. They do not substitute for the
installed image measurement or the native connector tests.
