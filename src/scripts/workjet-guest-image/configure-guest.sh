#!/bin/sh
# Origin: CTOX; License: AGPL-3.0-only
# PROPOSED image customization payload, not executed on a host or live guest.
# Run only inside the stopped private image through the reviewed virt-customize
# recipe after actual image/build/host approval. The marker identifies the copy;
# it is NOT an approval token or an execution/controller permit.
set -eu
[ "$(uname -s)" = Linux ]
[ "$(uname -m)" = x86_64 ]
[ "$(cat /etc/ctox-image-build.marker)" = workjet-noble-amd64-20260926-ctox-318890469-v1 ]
[ -x /usr/local/bin/ctox ]
. /etc/os-release
[ "$ID" = ubuntu ] && [ "$VERSION_ID" = 24.04 ]
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
  fonts-dejavu-core maim xdotool
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
# No guest ID in the golden image. The actual native enrolled owner injects
# /etc/ctox/guest-startup.json into its OWN writable overlay before first boot:
# {"guest_id":"<actual enrolled ID>","display":":0",
#  "xauthority":"/run/ctox-desktop/Xauthority"}, owned1500:1500/mode0600.
[ ! -e /etc/ctox/guest-startup.json ]
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
cat > /usr/local/libexec/ctox-wait-x11 <<'WAIT'
#!/bin/sh
set -eu
# Bounded startup only, no reconnect/watch/restart loop. systemd owns the unit.
attempt=0
while [ "$attempt" -lt 30 ]; do
    if /usr/bin/xdpyinfo -display :0 >/dev/null 2>&1; then exit 0; fi
    attempt=$((attempt + 1))
    sleep 1
done
exit 1
WAIT
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
Requires=ctox-desktop.service
BindsTo=ctox-desktop.service
After=ctox-desktop.service
ConditionPathExists=/etc/ctox/guest-startup.json
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
systemctl mask ssh.service ssh.socket getty@tty1.service
systemctl enable ctox-xorg.service ctox-desktop.service ctox-guest-desktop.service
install -d -m 0755 /usr/local/share/ctox-image
dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' > /usr/local/share/ctox-image/packages.tsv
sha256sum /usr/local/bin/ctox > /usr/local/share/ctox-image/native-binary.sha256
printf '%s\n' 318890469b60e11852f14df8b996e422616af269 > /usr/local/share/ctox-image/native-source.commit
apt-get clean
rm /etc/ctox-image-build.marker
