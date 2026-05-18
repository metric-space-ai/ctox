# Security Posture Checks

Use these commands to inspect concrete host-level posture.

## Accounts, Groups, And Privilege

```sh
getent passwd
getent group
id <user>
sudo -l -U <user>
last -a | head
```

## Listener Exposure

```sh
ss -tulpn
ip -br addr
systemctl list-units --type=socket --all --no-pager
```

## Firewall And Network Policy

```sh
ufw status verbose
nft list ruleset
iptables -S
```

Use the firewall stack that exists on the host.

## Certificates

```sh
openssl x509 -in <cert.pem> -noout -subject -issuer -dates
openssl s_client -connect <host>:<port> -servername <host> </dev/null
```

## File Permissions And Secret Exposure

```sh
find /etc -xdev -type f -perm -0002
find <path> -xdev -type f \\( -name '*.env' -o -name '*.pem' -o -name '*.key' \\) -ls
stat <path>
```

## Package Posture

```sh
apt list --installed
dnf list installed
rpm -qa
dpkg -l
```

Use package evidence only for what is installed or pending. Do not claim vulnerability state without an actual source.

## Service Hardening

```sh
systemctl cat <unit>
systemd-analyze security <unit>
sshd -T
nginx -T
```

## Common Finding Shapes

- unexpected public listener
- overly broad sudo or group membership
- expiring or expired certificate
- secret material in readable or world-writable locations
- service config lacking obvious hardening or binding too widely
