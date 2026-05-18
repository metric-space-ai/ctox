# CTOX Web Paths

CTOX separates web work into four distinct capability paths.

This is intentional. Different web tasks have different cost, evidence quality, runtime requirements, and operational consequences.

## 1. WebSearch

Use `WebSearch` when the task is:

- find out what is true now
- discover current sources
- check recent changes

This is the lowest-friction path for current discovery.

## 2. WebRead

Use `WebRead` when the task is:

- read a concrete source carefully
- inspect a page, document, or PDF
- pull specific facts out of a known source

This is the source-reading path.

## 3. interactive-browser

Use `interactive-browser` when the task is:

- the live page behavior itself matters
- JavaScript execution is required
- auth or session state is required
- screenshots or rendered evidence are the source of truth

This is the reviewed real-browser path. It is not the default for routine web work.

## 4. WebScrape

Use `WebScrape` when the task is:

- make extraction repeatable
- maintain a target over time
- persist canonical records
- query the result later through a stable local API

This is the operational web path.

Each scrape target can expose four stable HTTP read paths:

- `/ctox/scrape/targets/{target_key}/api`
- `/ctox/scrape/targets/{target_key}/records`
- `/ctox/scrape/targets/{target_key}/semantic`
- `/ctox/scrape/targets/{target_key}/latest`

## Routing Rule

Use this rule:

- use `WebSearch` for current discovery
- use `WebRead` for reading specific sources
- use `interactive-browser` when runtime page behavior matters
- use `WebScrape` when the work should become durable and operational
