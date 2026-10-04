# Update backup credential recovery

Update backups contain consistent SQLite snapshots and selected state files.
The secret store's `ctox-secrets.key` is copied explicitly alongside its encrypted
SQLite database. On Unix, the backup key is created with mode `0600`; restoring
it preserves that mode. Symlink keys and key-copy failures abort backup creation
before the completion manifest is written. Unrelated `.key` files remain excluded.

The key is copied after database snapshots because migration writes the protected
key file before removing the legacy embedded key. Older stores without a key file
remain supported through their embedded database key.

Snapshots made by versions that excluded `ctox-secrets.key` may contain encrypted
credentials with no decryption key. A newer binary cannot reconstruct that missing
key. Preserve the original protected key and recovery copies until a corrected
backup has been created and validated. Do not publish keys or real secret data in
logs, test fixtures, or recovery evidence.

The regression test `state_backup_restores_encrypted_credentials_without_original_store`
creates synthetic credentials, moves the backup outside the original store, deletes
the original, restores to a fresh root, and decrypts the fixture using only that
backup. It checks Unix permissions before the secrets API can repair them.

This validates credential recovery through the update backup path. It does not
establish off-host backup availability or a complete service disaster-recovery test.
