---
name: news-aggregation
description: Use when a Business OS News topic asks CTOX to produce a news issue (command news.issue.generate) — collect what is new on a topic from the configured sources (lab sites, the owner's X account, YouTube channels, newsletters, communities), rank it by relevance and discussion, and reply with one ctox.news.issue.v1 JSON object that the News app turns into an X-style feed.
cluster: research
---

# News Aggregation

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven internal work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or internal work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Before adding follow-up work, check for existing matching internal work, queue, plan, or ticket state and consolidate rather than duplicating.

## What the command gives you

`payload.record_snapshot.topic` — title, prompt (the editorial focus), language,
categories, `max_posts`. `payload.record_snapshot.sources` — the enabled
sources of this topic. `payload.issue_id`, `payload.issue_date`,
`payload.previous_issue_id`. Only these sources plus open web search are in
scope; do not invent sources the topic did not configure, except to confirm or
complete a story found through them.

## Collect, per source kind

| kind | adapter | how to read it |
| --- | --- | --- |
| `lab`, `community` | `scrape_target` | `ctox scrape execute --target-key <target_key> --allow-heal`, then `ctox scrape query-records --target-key <target_key>`. Without a registered target, read the page with `ctox_web_read` and say so in `summary`. |
| `x` | `api_browser` | `ctox_browser_automation` in the owner's CTOX browser profile (the user signed in through the News app). Read the home timeline and each configured list/search in `items`. If the page is the login wall, stop using X for this run and mention it in `summary` — never ask for or type credentials. |
| `youtube` | `youtube_transcript` | For each channel in `items`, find videos published since the previous issue, then `ctox web youtube-transcript --url <video-url>` and summarize from the transcript, not from the title. |
| `newsletter` | `mail` | `ctox channel search --channel email --sender <sender> --query <topic words>` for each sender in `items`; open the hits with `ctox channel history`. Treat newsletters as pointers: link the primary source they cite, not the newsletter, whenever it exists. |
| (always) | web | `ctox_web_search` / `ctox_web_read` to confirm a story at its primary source and to measure discussion (Hacker News points and comments, Reddit score and comments, repost counts on X). |

Time window: since the previous issue (`previous_issue_id`), otherwise the last
24 hours for daily topics and 7 days for weekly ones.

## Editorial rules

1. One post per story. Merge duplicate coverage into the strongest primary
   source; list the others as additional `sources`.
2. Primary source first: the lab's own post, model card, paper, repo or
   release note. A newsletter or aggregator is never the only source when the
   primary exists.
3. `intro`: one or two sentences, what happened and why it matters — written
   like a good X post, no hashtags, no emoji, in the topic language.
4. `body`: the long form — concrete facts (numbers, availability, pricing,
   licence, benchmarks with their caveats), what is new versus the previous
   version, and what the discussion says. Separate paragraphs with a blank
   line. No speculation presented as fact.
5. `quote`: optional, one telling comment from the discussion (HN, Reddit, X)
   with its link.
6. `hype_score` 0–100 reflects measured attention (points, comments, reposts,
   coverage breadth), not your opinion. `discussion_count` is the summed
   comment/reply count you actually observed.
7. Skip anything already covered in the previous issue unless there is a real
   development; then say what changed.
8. Order `posts` by relevance to the topic prompt; the app numbers them.
9. Respect `max_posts` and `categories` (`model`, `tool`, `research`,
   `debate`; anything else becomes `other`).

## Reply contract — `ctox.news.issue.v1`

The final reply is exactly one JSON object (a fenced ```json block is fine).
Posts without `intro` or without at least one `http(s)` source URL are
dropped by the writeback; a reply with no valid post fails the issue.

```json
{
  "schema": "ctox.news.issue.v1",
  "summary": "Two frontier releases, an agent SDK, and a licence debate.",
  "posts": [
    {
      "category": "model",
      "author_name": "OpenAI",
      "author_handle": "openai",
      "author_kind": "lab",
      "intro": "…",
      "body": "…\n\n…",
      "published_at": "2026-10-13T05:00:00Z",
      "hype_score": 94,
      "discussion_count": 2400,
      "link_card": { "url": "https://…", "title": "…", "image_url": "https://…" },
      "quote": { "source": "Hacker News", "meta": "860 Punkte", "text": "…", "url": "https://…" },
      "sources": [
        { "label": "openai.com · Ankündigung", "url": "https://…", "kind": "primary" },
        { "label": "Hacker News", "url": "https://news.ycombinator.com/item?id=…", "kind": "discussion" }
      ]
    }
  ],
  "trends": [
    { "title": "Agent-Sandboxing", "category": "tool", "discussion_count": 860, "post_index": 1 }
  ]
}
```

`author_kind` is `lab`, `community`, `media` or `person`. `trends` lists the
three to five most discussed themes of the issue; `post_index` is the 0-based
index of the post that covers it.

## Done when

- Every configured source was read or its failure is named in `summary`.
- Every post links its primary source.
- The reply is the JSON object above and nothing the writeback would drop
  silently.
