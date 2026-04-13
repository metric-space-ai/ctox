# Incident Triage Commands

Use these commands to turn an alert into a grounded incident picture.

## Fast Health Checks

```sh
uptime
free -h
df -h
ss -tulpn
curl -fsS http://127.0.0.1:<port>/health
```

## Service And Log Checks

```sh
systemctl status <unit> --no-pager
journalctl -u <unit> --since '30 min ago' --no-pager
docker ps
docker logs --since 30m <container>
kubectl get pods -A
kubectl logs -n <namespace> <pod> --since=30m
```

## Recent Change Evidence

```sh
git log --oneline -n 10
systemctl list-timers --all --no-pager
ls -lt <config-or-release-dir> | head
```

## Network And Dependency Checks

```sh
ping -c 3 <host>
dig <name>
curl -I <url>
ss -s
```

## Common Immediate Mitigations

Only use when justified by the evidence:

- restart a single failed unit
- roll back a single deploy slice
- stop one runaway service or timer
- free disk space from an obviously safe temp or log location
- fail over traffic only when the environment already supports it

Record before and after evidence for every mitigation.
