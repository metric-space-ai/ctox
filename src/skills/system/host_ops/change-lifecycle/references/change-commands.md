# Change Lifecycle Commands

Use these commands to inspect, stage, and verify changes without inventing a separate control plane.

## Repo And Config Diff

```sh
git status --short
git diff -- <path>
git show HEAD -- <path>
systemctl cat <unit>
diff -u <old> <new>
```

## Package And Version Checks

```sh
apt list --upgradable
dnf check-update
rpm -qa | rg <name>
dpkg -l | rg <name>
```

Use only the package manager that exists on the host.

## Dry Runs And Rendering

```sh
rsync -avhn <src> <dst>
docker compose config
docker compose pull
kubectl diff -f <manifest>
nginx -t
apachectl configtest
sshd -t
```

## Service Rollout And Verification

```sh
systemctl daemon-reload
systemctl restart <unit>
systemctl reload <unit>
systemctl status <unit> --no-pager
journalctl -u <unit> -n 200 --no-pager
curl -fsS http://127.0.0.1:<port>/health
ss -tulpn
```

## Container Rollout

```sh
docker compose up -d <service>
docker ps
docker logs --tail 200 <container>
kubectl rollout status deploy/<name> -n <namespace>
kubectl get pods -n <namespace> -o wide
```

## Rollback Preparation

Capture at least one of:

- previous package version
- previous image tag
- previous config file copy
- previous unit file
- backup archive path
- exact restart or reload reversal path

If you cannot name the rollback artifact, the change is not ready.
