# Frozen source delivery

`business_os::build_delivery::package` consumes an unmodified native source capture,
a validated build grant, a disposable staging root, a unique run ID and the
SHA256 of the actual compiler/toolchain profile. It produces owned upload files,
a source/cache identity and a Linux preparation script. It does not select a
computer, resolve credentials, establish SSH, admit a build or dispatch commands.
Those remain the native adapter's responsibility.

Public GitHub sources upload only the frozen overlay, local-commit bundle and
manifest. The host fetches the previously proved anonymous base directly from
GitHub, without client Git configuration or authentication. Private sources
upload their complete frozen tree, excluding Git metadata and ignored files.
The package outlives the original source capture. Filename lists are NUL framed.

The Linux script needs Bash, GNU timeout, Python3, flock support in Python's
fcntl module, and Git for public sources. It checks the declared lane disk floor,
bounds preparation to 15 minutes and uses a nonblocking lock per source ID
(exit75 on contention). It prepares in a unique private directory, validates
all paths, file hashes, executable bits, literal links and exact tree membership,
then renames the directory and flushes its manifest marker and parent directories.
Cache reuse repeats verification. A failed or interrupted preparation can leave
an unpublished partial directory; the owning adapter must clean it up explicitly.
A published tree with no completed marker is rejected rather than guessed ready.

The service account must exclusively own the canonical lane and its children.
This is source integrity and capacity preparation, not an untrusted-code sandbox.
The adapter uploads within the authorized incoming directory, runs preparation
with a bounded transport operation, and launches the build only after success.
The source ID includes compiler/profile identity; a mutable toolchain name alone
is insufficient. Slot ownership and build completion are separate runner states.

Linux tests execute real archive creation and remote preparation in isolated lane
fixtures, including hostile literal filenames, links, deletion, executable modes,
cache corruption and archive corruption. Public package tests inspect its exact
overlay and bundle; network routing and installed fleet acceptance remain open.
