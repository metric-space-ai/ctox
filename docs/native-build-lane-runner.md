# Native Linux build lane runner

`business_os::build_lane_runner::plan` consumes a validated `BuildCapability`,
opaque task/run/source identifiers, command arguments and a timeout in seconds.
It returns the script and deterministic source, target and run paths. Transport,
computer authority, source preparation and toolchain selection belong to the
calling adapter. No host table or credential is part of this module.

Install and execute the script with Bash on the authorized Linux lane. It needs
`flock`, GNU `timeout`, `setsid`, `nohup`, `df`, `awk` and standard coreutils.
The service account must exclusively own the lane tree: hostile symlinks or
concurrent writes by another account are outside this trust boundary. Run ids
must be unique; a repeated launch exits 73 without overwriting prior evidence.

The detached session shares the prototype lock namespace (`lane_root/slot-1.lock`
through `slot-SLOTS.lock`) and owns a nonblocking flock lease until the bounded command
and its inherited lock descriptors exit. Admission rejection exits 75, disk
floor rejection exits 74 and GNU timeout returns 124. Inspect the run directory
for `pid`, `started`, `slot`, `log`, atomic `exit` and `finished` files. The parent
must distinguish an absent exit file from completion and retain/reconcile the
run after disconnect. Hard host failure or SIGKILL can leave incomplete metadata;
this planner does not claim power-loss durability or automatic recovery.

Targets are isolated by source identity. The worker cap is declared through
Cargo, Rust test, CMake and Make environment settings. Operator commands can
override these settings or start other processes: the adapter must enforce its
command policy. This runner is not an untrusted-code sandbox. Source ids must
identify compatible source/toolchain state; source upload must finish before
launch. Timeouts signal the command process group and apply a ten-second kill
grace; deliberately escaping descendants require stronger OS isolation.

Linux tests run actual detached commands and contention against isolated lane
directories, exercising quoting, rejection, timeout, persistent logs and exit
status. Installed SSH routing and fleet acceptance remain adapter obligations.
