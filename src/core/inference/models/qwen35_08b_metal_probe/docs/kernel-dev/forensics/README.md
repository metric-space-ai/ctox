# Cache Forensics Records

`tools/new_cache_forensics_record.sh` writes cache and memory forensics records
here.

Use one record per hot operation and candidate layout/tile strategy. The record
must label evidence as `inferred-only` or `hardware-counter-backed`; do not claim
hardware cache misses unless the counter source is named.
