# PDF Fixture Layout

This directory holds CTOX page-level regression fixtures for `pdf_parse`.

Expected structure:

```text
tests/fixtures/
  corpus.json
  pages/
    example_page.json
  samples/
    ...
```

`corpus.json` lists the page fixture files.

Each page fixture describes one PDF page and the linearization contract that
must hold after parsing:

- exact expected lines
- required patterns
- ordered phrases
- phrases that must remain on the same line
- phrases that must not end up on the same line
- forbidden output patterns

Use the `eval_fixture` example to run one fixture or a whole corpus:

```bash
cargo run --manifest-path tools/pdf-parse/Cargo.toml --example eval_fixture \
  tools/pdf-parse/tests/fixtures/corpus.json \
  --pdf-root /path/to/pdf/root
```

Add `--show-text` when you need the exact parsed page text alongside the
evaluation result while adjusting fixtures or diagnosing a regression.

Corpus manifests can also carry an optional `pdf_root`. This is useful for
public benchmark smoke corpora whose sample PDFs live under a fixed repo-local
directory, for example `corpus.public-opendataloader.json` or
`corpus.public-pdfjs.json`.

To bootstrap a new page fixture from an existing PDF page, use the
`suggest_fixture` example:

```bash
cargo run --manifest-path tools/pdf-parse/Cargo.toml --example suggest_fixture \
  /path/to/file.pdf 1 --pdf-root /path/to/pdf/root
```

It emits a JSON skeleton with the first non-empty page lines prefilled as
`expected_lines`.

For public benchmark samples that should not be checked into the repo as binary
assets, use the `fetch_public_samples` example together with a sample manifest:

```bash
cargo run --manifest-path tools/pdf-parse/Cargo.toml --example fetch_public_samples \
  tools/pdf-parse/tests/fixtures/samples/public/opendataloader/samples.json
```

That command downloads the PDFs into the manifest directory, verifies their
SHA-256 checksums when available, and makes them ready for:

```bash
cargo run --manifest-path tools/pdf-parse/Cargo.toml --example eval_fixture \
  tools/pdf-parse/tests/fixtures/corpus.public-opendataloader.json
```

The same flow applies to the pinned `pdf.js` smoke corpus:

```bash
cargo run --manifest-path tools/pdf-parse/Cargo.toml --example fetch_public_samples \
  tools/pdf-parse/tests/fixtures/samples/public/pdfjs/samples.json

cargo run --manifest-path tools/pdf-parse/Cargo.toml --example eval_fixture \
  tools/pdf-parse/tests/fixtures/corpus.public-pdfjs.json
```

Current `pdf.js` coverage includes:

- text widgets
- button and choice widgets
- link/popup and highlight annotations
- basic forms with footer URLs
- rotated text
