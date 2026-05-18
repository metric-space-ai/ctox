# Discovery Graph Command Palette

Use these commands to build a concrete inventory. Prefer the smallest command that proves the fact.

## Host Identity

```sh
hostnamectl
uname -a
uptime
whoami
pwd
```

## Network And Listeners

```sh
ip -json address show
ip route
ss -tulpn
ss -s
```

## Services, Timers, And Logs

```sh
systemctl list-units --type=service --all --no-pager
systemctl show --type=service --all --property Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description
systemctl list-timers --all --no-pager
systemctl show --type=timer --all --property Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description
systemctl --failed --no-pager
systemctl cat <unit>
journalctl -p warning -n 200 --no-pager
journalctl -u <unit> -n 100 --no-pager
```

## Storage

```sh
lsblk -f
findmnt
df -h
du -xh --max-depth=1 <path>
```

## Processes And Containers

```sh
ps aux --sort=-%cpu | head
docker ps --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}'
docker inspect <container>
podman ps
kubectl get nodes -o wide
kubectl get pods -A -o wide
kubectl get svc -A
```

Use only the container or cluster commands that exist on the host.

## Repo Discovery

```sh
rg --files
rg -n "listen|port|host|database|redis|postgres|mysql|systemd|cron|timer|backup"
```

Search for:

- compose files
- unit files
- deployment manifests
- backup scripts
- environment files
- reverse proxy configs
- health checks

## Normalization Hints

Default full-sweep raw pipeline:

```sh
python3 <skill-path>/scripts/capture_run.py --db runtime/discovery_graph.db --repo-root <repo>
python3 <skill-path>/scripts/normalize_minimum.py --db runtime/discovery_graph.db --run-id <run-id> > graph.json
python3 <skill-path>/scripts/discovery_store.py store-graph --db runtime/discovery_graph.db --input graph.json
```

`capture_run.py` does not normalize anything. It only guarantees that raw collector output is both returned to the agent and persisted into SQLite before normalization starts.

Normalize findings into these buckets:

- host
- network interface
- service
- listener
- container
- kubernetes workload
- database
- queue
- timer
- storage volume
- ownership hint
- dependency edge
- coverage gap

If a fact cannot be proven from the repo or the host, keep it in `coverage gap`.
