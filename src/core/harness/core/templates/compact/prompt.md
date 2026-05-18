Optional compaction task hint:
- If the current task changed, state the exact next task that should continue after compaction.
- Otherwise leave this unchanged. CTOX will preserve compact durable context for the same parent task and will not turn compaction into a separate work loop.
