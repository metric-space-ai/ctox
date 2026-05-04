"use client";

import { useState } from "react";

type WatchlistManagerProps = {
  competitorUrlLabel: string;
  displayNameLabel: string;
  optionalLabel: string;
  queueNextRunLabel: string;
  rescrapeNowLabel: string;
  searchLabel: string;
  searchPlaceholder: string;
};

export function WatchlistManager({
  competitorUrlLabel,
  displayNameLabel,
  optionalLabel,
  queueNextRunLabel,
  rescrapeNowLabel,
  searchLabel,
  searchPlaceholder
}: WatchlistManagerProps) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<string | null>(null);

  async function addCompany(scrapeMode: "rescrape_now" | "next_standard_scrape") {
    setStatus("Queueing company...");
    const response = await fetch("/api/marketing/competitive-analysis/watchlist", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name, url, scrapeMode })
    });
    const result = await response.json().catch(() => null) as { company?: { name?: string }, error?: string } | null;

    if (!response.ok) {
      setStatus(result?.error ?? "Could not add company.");
      return;
    }

    setStatus(`${result?.company?.name ?? "Company"} added to watchlist.`);
    setName("");
    setUrl("");
  }

  async function searchCompanies() {
    if (!query.trim()) return;
    setStatus("Queueing CTOX web search...");
    const response = await fetch("/api/marketing/competitive-analysis/search", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ query })
    });

    if (!response.ok) {
      setStatus("Could not queue CTOX web search.");
      return;
    }

    setStatus("CTOX web search queued for initial company discovery.");
    setQuery("");
  }

  return (
    <div className="watchlist-manager">
      <label className="drawer-field">
        {competitorUrlLabel}
        <input onChange={(event) => setUrl(event.target.value)} placeholder="https://example.com" type="url" value={url} />
      </label>
      <label className="drawer-field">
        {displayNameLabel}
        <input onChange={(event) => setName(event.target.value)} placeholder={optionalLabel} type="text" value={name} />
      </label>
      <div className="watchlist-actions">
        <button disabled={!url.trim()} onClick={() => addCompany("next_standard_scrape")} type="button">{queueNextRunLabel}</button>
        <button disabled={!url.trim()} onClick={() => addCompany("rescrape_now")} type="button">{rescrapeNowLabel}</button>
      </div>
      <label className="drawer-field">
        {searchLabel}
        <input onChange={(event) => setQuery(event.target.value)} placeholder={searchPlaceholder} type="search" value={query} />
      </label>
      <button className="drawer-primary" disabled={!query.trim()} onClick={searchCompanies} type="button">{searchLabel}</button>
      {status ? <p className="watchlist-status" role="status">{status}</p> : null}
    </div>
  );
}
