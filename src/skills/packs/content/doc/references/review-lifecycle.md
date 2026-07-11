# Review lifecycle: tracked changes and comments

How redlining and commenting work at the package level, and how to finalize a
reviewed document with the native engine ops.

## Semantics: when to redline, when to comment

- A tracked change proposes a concrete text or formatting edit that the
  document owner will accept or reject. Use it whenever you would otherwise
  silently change someone's text.
- A comment carries feedback, questions, or rationale without touching the
  text. Use it to explain a redline, flag a decision, or ask for input.
- Pair them: a non-obvious redline gets a comment explaining why, anchored at
  the same location.
- Never collect feedback in a block at the end of the document; anchor every
  note at the exact place it concerns.

## How the package represents them (OOXML essentials)

- Insertions live in `w:ins` wrappers; deletions move the affected runs into
  `w:del` wrappers whose text becomes `w:delText`. Formatting-change history
  is kept in `*PrChange` elements (`w:rPrChange`, `w:pPrChange`, table
  variants). Moves are `w:moveFrom`/`w:moveTo` pairs with range markers.
- Comment text lives in its own part (`word/comments.xml`); the story text
  carries anchors (`w:commentRangeStart`/`End` plus a reference run).
  Resolution state ("done") is stored separately in
  `word/commentsExtended.xml`, keyed by the paragraph id of the comment's
  last paragraph.
- Because comments span multiple parts, verifying them needs a structural
  check (parts, anchors, relationships consistent) — page renders often do
  not show comments at all.

## Finalization with engine ops

Accepting all revisions means: insertion wrappers unwrap into plain content,
deletions and move sources disappear, property-change history is discarded in
favor of the current formatting.

```
# Overview of open review items
ctox-office-engine comments-extract reviewed.docx

# Produce the clean copy
ctox-office-engine tracked-changes-accept reviewed.docx accepted.docx

# Optional: freeze the result for circulation
ctox-office-engine protection-set accepted.docx final.docx readonly
```

Comment removal for final delivery is part of the comments op family; check
`references/execution-surfaces.md` for what has shipped. After finalization,
re-render and re-inspect: acceptance changes layout (deleted content
disappears, inserted content reflows).

## Checklist before delivering a reviewed document

- Every tracked change either accepted or rejected — none left silently.
- Every comment resolved, answered, or deliberately retained (say which).
- `comments-extract` output matches expectations (no orphaned or forgotten
  threads).
- Clean copy re-rendered and visually verified; render evidence persisted.
- If the document leaves the organization: `privacy-scrub` for author
  metadata and revision ids, and check the a11y audit findings.
