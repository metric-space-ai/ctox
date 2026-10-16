// news.source.adapter – Zauberstab an einer Quelle (Port aus store_news_commands.rs). Fuellt prompts/source-adapter.md
// (Scraper einrichten, System-Skill universal-scraping) bzw. prompts/source-connect.md (CTOX-Konto bei X/Reddit,
// App-Skill news/news-aggregation) und startet den Agenten (enqueueSourceAdapter, _lib/news.js). Das Ergebnis kommt
// als news.source.adapter.complete / news.source.adapter.failed zurueck (Kern §16.30 Nr. 3).

import { enqueueSourceAdapter } from './_lib/news.js';

export default async function handle(command, ctx) {
  return enqueueSourceAdapter(ctx, command);
}
