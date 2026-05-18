# Autotune Records

`tools/new_autotune_record.sh` writes autotune evidence records here.

Use one record per parameter family, such as row-cache scan layout, prefill
Delta stack chunking, attention block shape, split-K strategy, or LM-head vocab
tile. A candidate should not become the accepted profile without a strict
autotune record when the change came from a search.
