#!/bin/sh
# Origin: CTOX; License: AGPL-3.0-only
# Image customization payload; never execute on a host or live guest.
# Run only inside the stopped private image through the reviewed virt-customize
# recipe after actual image/build/host approval. The marker identifies the copy;
# it is NOT an approval token or an execution/controller permit.
set -eu
[ "$(uname -s)" = Linux ]
[ "$(uname -m)" = x86_64 ]
[ -x /usr/local/bin/ctox ]
. /etc/os-release
[ "$ID" = ubuntu ] && [ "$VERSION_ID" = 24.04 ]

configure_boot() {
    # TCG cold boots can discover the mandatory boot devices after systemd's
    # default device deadline. Keep both mounts required, with a finite bound.
    python3 - <<'PY'
import os
import pathlib
import tempfile

path = pathlib.Path('/etc/fstab')
lines = path.read_text().splitlines(keepends=True)
mounts = {'/boot': 0, '/boot/efi': 0}
for i, line in enumerate(lines):
    if not line.strip() or line.lstrip().startswith('#'):
        continue
    fields = line.split()
    if len(fields) < 4 or fields[1] not in mounts:
        continue
    mounts[fields[1]] += 1
    options = fields[3].split(',')
    if 'nofail' in options or 'noauto' in options:
        raise SystemExit('boot mounts must remain mandatory')
    options = [option for option in options
               if not option.startswith('x-systemd.device-timeout=')]
    fields[3] = ','.join(options + ['x-systemd.device-timeout=300s'])
    lines[i] = '\t'.join(fields) + '\n'
if any(count != 1 for count in mounts.values()):
    raise SystemExit('exactly one /boot and /boot/efi mount required')
mode = path.stat().st_mode & 0o777
fd, temporary = tempfile.mkstemp(prefix='.ctox-fstab-', dir=path.parent)
try:
    with os.fdopen(fd, 'w') as stream:
        stream.writelines(lines)
        stream.flush()
        os.fchmod(stream.fileno(), mode)
        os.fsync(stream.fileno())
    os.replace(temporary, path)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
# The fixed virtio-vga guest has one framebuffer. Keep fbcon from taking over
# its text consoles: emulated framebuffer damage work otherwise dominates
# cold boot. Xorg still uses the real DRM device and graphical VT.
grub = pathlib.Path('/etc/default/grub.d/99-ctox-serial-console.cfg')
grub.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
grub.write_text('''ctox_without_guest_overrides() {
    set -f
    for ctox_argument in $1; do
        case "$ctox_argument" in
            console=*|clocksource=*|fbcon=*) : ;;
            *) printf '%s ' "$ctox_argument" ;;
        esac
    done
}
# A stopped KVM guest can resume under TCG. Its source TSC may become
# unstable and HPET fallback is costly to emulate. Use the common ACPI PM
# timer from boot; retain clocksource verification and all timeout guards.
# ref: Linux v6.8 admin-guide/kernel-parameters.html, clocksource=
GRUB_CMDLINE_LINUX_DEFAULT=$(ctox_without_guest_overrides "$GRUB_CMDLINE_LINUX_DEFAULT")
ctox_linux=$(ctox_without_guest_overrides "$GRUB_CMDLINE_LINUX")
GRUB_CMDLINE_LINUX="$ctox_linux console=ttyS0,115200n8 fbcon=map:1 clocksource=acpi_pm"
unset ctox_linux
unset -f ctox_without_guest_overrides
''')
grub.chmod(0o644)
PY
    update-grub
}

configure_desktop() {
cat > /usr/local/libexec/ctox-wait-x11 <<'WAIT'
#!/bin/sh
set -eu
# Bounded startup only, no reconnect/watch/restart loop. systemd owns the unit.
read -r started ignored < /proc/uptime
deadline=$((${started%%.*} + 180))
while :; do
    if /usr/bin/timeout --signal=TERM --kill-after=1s 3s /usr/bin/xdpyinfo -display :0 >/dev/null 2>&1; then
        printf '%s\n' 'CTOX X11 readiness confirmed'
        exit 0
    fi
    read -r now ignored < /proc/uptime
    [ "${now%%.*}" -lt "$deadline" ] || break
    sleep 1
done
printf '%s\n' 'CTOX X11 readiness deadline exceeded' >&2
exit 1
WAIT
chmod 0755 /usr/local/libexec/ctox-wait-x11
cat > /etc/systemd/system/ctox-desktop.service <<'UNIT'
[Unit]
Description=CTOX isolated XFCE worker desktop
Requires=ctox-xorg.service
BindsTo=ctox-xorg.service
After=ctox-xorg.service
[Service]
Type=simple
User=ctox-desktop
Group=ctox-desktop
RuntimeDirectory=ctox-user
RuntimeDirectoryMode=0700
Environment=DISPLAY=:0
Environment=XAUTHORITY=/run/ctox-desktop/Xauthority
Environment=XDG_RUNTIME_DIR=/run/ctox-user
Environment=HOME=/home/ctox-desktop
ExecStartPre=/usr/local/libexec/ctox-wait-x11
ExecStart=/usr/bin/dbus-run-session -- /usr/bin/xfce4-session
StandardOutput=journal+console
StandardError=journal+console
Restart=no
TimeoutStartSec=190
TimeoutStopSec=10
KillMode=control-group
[Install]
WantedBy=multi-user.target
UNIT
}

if [ "${1-}" = --desktop-only ]; then
    [ "$#" -eq 1 ]
    [ "$(cat /etc/ctox-image-build.marker)" = workjet-noble-amd64-20260926-ctox-555140a08-v9 ]
    [ "$(id -u ctox-desktop)" = 1500 ]
    [ ! -e /etc/ctox/guest-startup.json ]
    [ -f /etc/systemd/system/ctox-xorg.service ]
    [ -f /etc/systemd/system/ctox-guest-desktop.service ]
    configure_desktop
    rm /etc/ctox-image-build.marker
    exit 0
fi


if [ "${1-}" = --clock-only ]; then
    [ "$#" -eq 1 ]
    [ "$(cat /etc/ctox-image-build.marker)" = workjet-noble-amd64-20260926-ctox-555140a08-v10 ]
    [ "$(id -u ctox-desktop)" = 1500 ]
    [ ! -e /etc/ctox/guest-startup.json ]
    configure_boot
    rm /etc/ctox-image-build.marker
    exit 0
fi

if [ "${1-}" = --boot-only ]; then
    [ "$#" -eq 1 ]
    [ "$(cat /etc/ctox-image-build.marker)" = workjet-noble-amd64-20260926-ctox-555140a08-v8 ]
    [ "$(id -u ctox-desktop)" = 1500 ]
    [ ! -e /etc/ctox/guest-startup.json ]
    configure_boot
    rm /etc/ctox-image-build.marker
    exit 0
fi
[ "$#" -eq 0 ]
[ "$(cat /etc/ctox-image-build.marker)" = workjet-noble-amd64-20260926-ctox-b3745d911-v5 ]
export DEBIAN_FRONTEND=noninteractive
cat > /etc/apt/sources.list.d/ubuntu.sources <<'APT'
Types: deb
URIs: https://archive.ubuntu.com/ubuntu
Suites: noble noble-updates
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
Snapshot: 20261001T000000Z

Types: deb
URIs: https://security.ubuntu.com/ubuntu
Suites: noble-security
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
Snapshot: 20261001T000000Z
APT
# Never turn off repository signatures or substitute a latest/current archive.
apt-get --snapshot 20261001T000000Z update
apt-get --snapshot 20261001T000000Z -y --no-install-recommends install \
  xserver-xorg-core xserver-xorg-video-all xserver-xorg-input-libinput \
  xauth x11-utils x11-xserver-utils xfce4-session xfce4-panel xfwm4 \
  xfdesktop4 xfce4-settings xfce4-terminal thunar mousepad dbus-x11 \
  fonts-dejavu-core maim xdotool kmod linux-modules-extra-6.8.0-142-generic
# The fixed cloud image omits the fw_cfg module used for fresh assignments.
# Validate against the GUEST kernel, never the build appliance's uname release.
modinfo -k 6.8.0-142-generic qemu_fw_cfg >/dev/null
# Refuse an existing UID/GID/name instead of reassigning another image user.
if getent passwd ctox-desktop || getent passwd 1500 \
    || getent group ctox-desktop || getent group 1500; then
    printf '%s\n' 'native desktop UID/GID/name is already assigned' >&2
    exit 1
fi

groupadd --gid 1500 ctox-desktop
useradd --uid 1500 --gid 1500 --create-home --shell /bin/bash ctox-desktop
passwd --lock ctox-desktop
install -d -m 0750 -o root -g ctox-desktop /etc/ctox
install -d -m 0755 /usr/local/libexec /etc/X11/xorg.conf.d
# No guest ID in the golden image. Native QEMU supplies only its actual enrolled
# ID and fixed display paths through fw_cfg. Before the endpoint starts, the
# root unit copies that bounded blob into THIS guest's writable overlay.
[ ! -e /etc/ctox/guest-startup.json ]
cat > /usr/local/libexec/ctox-install-guest-startup <<'STARTUP'
#!/bin/sh
set -eu
umask 077
source=/sys/firmware/qemu_fw_cfg/by_name/opt/org.ctox/guest-startup/raw
[ -r "$source" ]
temporary=$(mktemp /etc/ctox/.guest-startup.XXXXXX)
trap 'rm -f "$temporary"' EXIT
trap 'exit 1' HUP INT TERM
# One bounded read; a missing/empty/oversized blob cannot reuse a restored ID.
# Native startup parses the exact JSON fields before opening the fixed device.
dd if="$source" of="$temporary" bs=4097 count=1 iflag=fullblock status=none
[ -s "$temporary" ] && [ "$(wc -c < "$temporary")" -le 4096 ]
chown 1500:1500 "$temporary"
chmod 0600 "$temporary"
mv -T "$temporary" /etc/ctox/guest-startup.json
sync -f /etc/ctox/guest-startup.json
STARTUP
chmod 0755 /usr/local/libexec/ctox-install-guest-startup
cat > /etc/systemd/system/ctox-guest-startup.service <<'UNIT'
[Unit]
Description=CTOX native assignment configuration for this guest
Before=ctox-guest-desktop.service
[Service]
Type=oneshot
User=root
StandardOutput=journal+console
StandardError=journal+console
ExecStartPre=/usr/sbin/modprobe qemu_fw_cfg
ExecStart=/usr/local/libexec/ctox-install-guest-startup
RemainAfterExit=yes
TimeoutStartSec=10
TimeoutStopSec=10
UNIT
cat > /etc/udev/rules.d/99-ctox-guest-desktop.rules <<'UDEV'
SUBSYSTEM=="virtio-ports", ATTR{name}=="org.ctox.guest.desktop", GROUP="ctox-desktop", MODE="0660"
UDEV
cat > /etc/X11/xorg.conf.d/20-ctox-virtio.conf <<'XORG'
Section "Device"
    Identifier "ctox-virtio"
    Driver "modesetting"
    Option "AccelMethod" "none"
EndSection
Section "Screen"
    Identifier "ctox-screen"
    Device "ctox-virtio"
    DefaultDepth 24
    SubSection "Display"
        Depth 24
        Virtual 1280 800
    EndSubSection
EndSection
XORG
cat > /usr/local/libexec/ctox-prepare-xauthority <<'AUTH'
#!/bin/sh
set -eu
umask 077
install -m 0600 -o 1500 -g 1500 /dev/null /run/ctox-desktop/Xauthority
cookie=$(/usr/bin/mcookie)
/usr/bin/xauth -f /run/ctox-desktop/Xauthority add :0 MIT-MAGIC-COOKIE-1 "$cookie"
# xauth can replace its file atomically; restore the desktop user's access.
chown 1500:1500 /run/ctox-desktop/Xauthority
chmod 0600 /run/ctox-desktop/Xauthority

unset cookie
AUTH
configure_desktop
chmod 0755 /usr/local/libexec/ctox-prepare-xauthority /usr/local/libexec/ctox-wait-x11
cat > /etc/systemd/system/ctox-xorg.service <<'UNIT'
[Unit]
Description=CTOX isolated guest real Xorg display
After=systemd-user-sessions.service
[Service]
Type=simple
User=root
Group=ctox-desktop
RuntimeDirectory=ctox-desktop
RuntimeDirectoryMode=0750
ExecStartPre=/usr/local/libexec/ctox-prepare-xauthority
ExecStart=/usr/bin/Xorg :0 -nolisten tcp -auth /run/ctox-desktop/Xauthority -noreset vt1
Restart=no
TimeoutStartSec=40
TimeoutStopSec=10
KillMode=control-group
[Install]
WantedBy=multi-user.target
UNIT

cat > /etc/systemd/system/ctox-guest-desktop.service <<'UNIT'
[Unit]
Description=CTOX bounded native guest desktop endpoint
Requires=ctox-desktop.service ctox-guest-startup.service
BindsTo=ctox-desktop.service
After=ctox-desktop.service ctox-guest-startup.service
[Service]
Type=simple
User=ctox-desktop
Group=ctox-desktop
ExecStart=/usr/local/bin/ctox __native-guest-desktop --config /etc/ctox/guest-startup.json
Restart=no
TimeoutStartSec=40
TimeoutStopSec=10
KillMode=control-group
[Install]
WantedBy=multi-user.target
UNIT
# A native fixed virtio endpoint is the only guest control channel. Production
# QEMU still has no NIC and no host filesystem sharing. Do not add SSH keys.
touch /etc/cloud/cloud-init.disabled
systemctl mask ssh.service ssh.socket getty@tty1.service systemd-networkd-wait-online.service
systemctl enable ctox-xorg.service ctox-desktop.service ctox-guest-desktop.service
# virt-resize can renumber GPT partitions. Reinstall the BIOS loader against
# this stopped appliance disk; retain UUID-based Linux boot configuration.
test -d /usr/lib/grub/i386-pc
grub-install --target=i386-pc --recheck /dev/sda
configure_boot
install -d -m 0755 /usr/local/share/ctox-image
dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' > /usr/local/share/ctox-image/packages.tsv
sha256sum /usr/local/bin/ctox > /usr/local/share/ctox-image/native-binary.sha256
printf '%s\n' b3745d911e2fa054127f04b83f28106b5c28bb3a > /usr/local/share/ctox-image/native-source.commit
apt-get clean
rm /etc/ctox-image-build.marker
