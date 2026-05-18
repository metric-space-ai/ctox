# Host Observability Commands

These commands are the concrete tool layer for `reliability-ops`. Use them instead of vague "monitoring" language.

## Interactive Resource Views

```sh
htop
btop
top
```

Use these when an interactive snapshot is enough and the environment supports them.

## CPU, Memory, And Process Pressure

```sh
uptime
free -h
vmstat 1 5
ps aux --sort=-%cpu | head -n 20
ps aux --sort=-%mem | head -n 20
```

## Disk And IO

```sh
df -h
du -xh --max-depth=1 <path>
iostat -xz 1 3
findmnt
lsblk -f
```

## Network And Listeners

```sh
ss -s
ss -tulpn
ip -br addr
curl -fsS http://127.0.0.1:<port>/health
```

## Service Health

```sh
systemctl status <unit> --no-pager
journalctl -u <unit> -n 200 --no-pager
docker ps
docker logs --tail 200 <container>
kubectl get pods -A
kubectl logs -n <namespace> <pod> --tail=200
```

Use only the runtime family that actually exists on the host.

## GPU

```sh
nvidia-smi
nvidia-smi dmon -s pucvmet -c 5
```

## Interpreting Patterns

- High load average with low CPU utilization often means IO wait or blocked tasks.
- High RSS alone is not enough; pair it with swap usage, OOM events, and process growth.
- Full filesystems often show up first as log write failures, temp file errors, or database stalls.
- Many sockets in `TIME_WAIT` or `SYN-SENT` can indicate a network or downstream problem, not a CPU problem.
