---
name: news-aggregation
description: Use when a Business OS News topic asks CTOX to produce a news issue (command news.issue.generate) — collect what is new on a topic from the configured sources (lab sites, X and Reddit through CTOX's own signed-in account, YouTube channels, newsletters, communities), rank it by relevance and discussion, and reply with one ctox.news.issue.v1 JSON object that the News app turns into an X-style feed. Also use for news.source.adapter on an X or Reddit source — connect CTOX's account (sign in, or register with the CTOX mailbox) and reply with ctox.news.source.v1.
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

Use the tools in the listed order and escalate on `403`, `429`, a challenge page
or empty text. A plain `ctox_web_read` never escalates by itself. Record which
path worked in `source_report`.

| kind | order of tools |
|---|---|
| `lab` | 1. Feed first: `ctox_web_read` on `<site>/rss.xml`, `<site>/news/rss.xml`, `<site>/feed` (e.g. `https://openai.com/news/rss.xml` works). 2. `ctox_web_scrape {"target_key": "<target_key>", "mode": "latest"}` when the source has a registered target. 3. `ctox_web_read {"url": "<page>", "query": "<topic>"}`. 4. On 403/400/challenge: `ctox_browser_automation` (stealth browser; reads openai.com/news and x.ai/news from servers). 5. Headlines only: `ctox_web_search {"query": "<lab> announcement", "domains": ["<host>"]}`. |
| `community` | Hacker News: `ctox_web_read` on `https://hn.algolia.com/api/v1/search?tags=front_page` (JSON with points and comment counts), item details via `https://hn.algolia.com/api/v1/items/<id>`. GitHub / Hugging Face trending and aggregator sites such as alphasignal.ai: `ctox_web_read`, then `ctox_browser_automation`. arXiv: `ctox_scholarly_search {"query": "<topic>", "sort": "newest"}`, then `ctox_web_read` on `https://arxiv.org/abs/<id>`. |
| `x`, `reddit` | Only through CTOX's own account — there is no X or Reddit API path, and logged out both block server IPs. Run the source's reader below with `ctox business-os web-stack authenticated-automation`. Login failure or a missing account: report the source `needs_auth`; do not fall back to the logged-out browser. `ctox_web_search {"domains": ["x.com"]}` may only confirm stories found elsewhere. |
| `youtube` | `ctox web youtube-transcript --url <video-url>` (shell CLI; captions only). From data-centre hosts YouTube answers `video not playable (LOGIN_REQUIRED)`; then use the video page via `ctox_web_read` for title and description only, say so in the post body, and report the source `failed` with the reason. |
| `newsletter` | `ctox channel search --channel email --sender <exact address> --query <one word from the topic> --limit 20` (newest first; no date flag, filter on `external_created_at` since the previous issue), open hits with `ctox channel history --thread-key <thread_key>`, then link and read the primary sources they cite with `ctox_web_read`. |
| (always) | `ctox_web_search` / `ctox_web_read` to confirm a story at its primary source and to measure discussion (HN points/comments via the Algolia API). |

`ctox scrape execute` and `ctox_web_scrape` with `mode: execute|test` are not
available to news tasks; only `latest` and `semantic` read stored records.
`web-unlock` repairs the stealth browser when bot detection regresses; it is not
a per-request unblocker and cannot lift IP blocks. When the stealth browser
shows a challenge, record it with `ctox web unlock signals record --source news --url <url>`
and move on.

## Signed-in sources (X, Reddit)

A source with `auth_source` (`x.com`, `reddit.com`) is read in the owner's
CTOX browser profile. The instruction names the command id; pass it as
`--task-id` — the owner is verified from that command, and the credential
(`X_BROWSER_LOGIN`, `REDDIT_BROWSER_LOGIN`) comes from the source profile, so
no `--credential-ref` is needed. Login and reader run in one browser process;
an e-mail code from the provider is fetched from the CTOX mailbox automatically.

```bash
ctox business-os web-stack authenticated-automation --source-id reddit.com \
  --task-id <command id> --timeout-ms 120000 < reader.js
```

The reader is plain JavaScript with `page` and `ctoxBrowser`; its `return`
value is the result. Read every configured item (`items`: subreddits, X lists,
searches, `@accounts`; X `Home` means the timeline) in one script.

Reddit (`items` like `r/LocalLLaMA`):

```js
const out = [];
for (const sub of ["r/LocalLLaMA", "r/MachineLearning"]) {
  await ctoxBrowser.goto(`https://www.reddit.com/${sub}/top/?t=day`, { timeoutMs: 30000 });
  await page.waitForSelector("shreddit-post", { timeout: 15000 }).catch(() => null);
  out.push(...await page.$$eval("shreddit-post", (posts) => posts.slice(0, 15).map((p) => ({
    title: p.getAttribute("post-title"), url: "https://www.reddit.com" + p.getAttribute("permalink"),
    link: p.getAttribute("content-href"), score: Number(p.getAttribute("score")),
    comments: Number(p.getAttribute("comment-count")), created: p.getAttribute("created-timestamp"),
    author: p.getAttribute("author"), sub: p.getAttribute("subreddit-prefixed-name"),
  }))));
}
return { ok: true, posts: out };
```

X (`https://x.com/home`, `https://x.com/i/lists/<id>`,
`https://x.com/search?q=<query>&f=live`, `https://x.com/<handle>`):

```js
await ctoxBrowser.goto("https://x.com/home", { timeoutMs: 30000 });
await page.waitForSelector('article[data-testid="tweet"]', { timeout: 20000 }).catch(() => null);
for (let i = 0; i < 4; i += 1) { await page.mouse.wheel(0, 2400); await page.waitForTimeout(1200); }
const posts = await page.$$eval('article[data-testid="tweet"]', (items) => items.map((a) => ({
  author: a.querySelector('[data-testid="User-Name"]')?.innerText.replace(/\s+/g, " "),
  text: a.querySelector('[data-testid="tweetText"]')?.innerText,
  url: a.querySelector('a[href*="/status/"] time')?.closest("a")?.href,
  time: a.querySelector("time")?.getAttribute("datetime"),
  stats: ["reply", "retweet", "like"].map((k) => a.querySelector(`[data-testid="${k}"]`)?.getAttribute("aria-label")),
})));
return { ok: true, posts };
```

Result `ok: false` with `authenticated_login.status` other than `completed`
means the account is not usable: report `needs_auth`. A challenge page inside
the reader: record it with `ctox web unlock signals record --source news --url <url>`.

## Connect an account (`news.source.adapter` with `auth_source`)

The owner asked CTOX to set up its own account for this source.

1. Probe the login: `authenticated-automation --source-id <auth_source> --task-id <command id>`
   with the reader `return { ok: true, url: page.url() };`. Success → reply `connected`.
2. Error `failed to resolve credential_ref` means CTOX has no account yet.
   Register with the CTOX mailbox:
   `ctox business-os web-stack auth-assist-signup --source-id <auth_source> --generate-credential --confirm-provisioning --accept-terms --task-id <command id> --timeout-ms 240000`.
   `--generate-credential` stores `{email, password}` in the secret store; you
   never see or print the password.
3. Signup `completed`: repeat step 1. `verification_required`, a CAPTCHA, a
   phone check or any other human step: do not work around it. Reply
   `needs_owner`; the signup session stays open in the owner's Browser app
   (`session_id` from the result) where the owner finishes it.
4. Reply with exactly one object:

```json
{"schema": "ctox.news.source.v1", "source_id": "<id>", "status": "connected|needs_owner|failed",
 "account": "<username or e-mail, no password>", "note": "<one sentence for the owner>"}
```

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
  ],
  "source_report": [
    { "source_id": "source_…", "status": "ok" },
    { "source_id": "source_…", "status": "needs_auth", "note": "X shows the login wall" }
  ]
}
```

`author_kind` is `lab`, `community`, `media` or `person`. `trends` lists the
three to five most discussed themes of the issue; `post_index` is the 0-based
index of the post that covers it. `source_report` gives every configured
source (`record_snapshot.sources[].id`) one status — `ok`, `needs_auth` or
`failed` with a short `note`; the News app shows it on the source row.

## Done when

- Every configured source was read or its failure is named in `source_report`.
- Every post links its primary source.
- The reply is the JSON object above and nothing the writeback would drop
  silently.
