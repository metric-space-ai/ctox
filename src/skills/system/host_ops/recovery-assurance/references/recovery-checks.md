# Recovery Assurance Checks

Use the backup tool that actually exists on the host. Do not invent a backup stack.

## Scheduler And Job Discovery

```sh
systemctl list-timers --all --no-pager
systemctl status <backup-unit> --no-pager
journalctl -u <backup-unit> -n 200 --no-pager
crontab -l
```

## Snapshot And Repository Examples

```sh
restic snapshots
restic check
borg list <repo>
borg check <repo>
rclone ls <remote>:<path>
```

Use only the tool present in the environment.

## Filesystem And Archive Checks

```sh
ls -lh <backup-path>
tar -tf <archive.tar>
sha256sum <artifact>
```

## Database Backup Checks

```sh
pg_dump --version
pg_restore --list <dump>
psql -lqt
mysqldump --version
mysql -e 'show databases;'
```

## Restore Validation

Prefer one of:

- list archive contents
- validate snapshot metadata
- restore to temp directory
- restore to disposable database or namespace
- compare expected critical files after isolated restore

## Evidence To Capture

- last successful backup time
- retention window seen
- off-host destination, if known
- restore test type and scope
- uncovered service or dataset
- unresolved RPO or RTO gap
