"use client";

import { useMemo, useState } from "react";

type ScoreCriterion = {
  id: string;
  name: string;
  weight: number;
};

type ScoreModelEditorProps = {
  addLabel: string;
  initialCriteria: ScoreCriterion[];
  nameLabel: string;
  newCriterionLabel: string;
  nextScrapeLabel: string;
  removeLabel: string;
  rescrapeNoticeLabel: string;
  rescrapeNowLabel: string;
  scrapeDecisionLabel: string;
  weightLabel: string;
};

export function ScoreModelEditor({
  addLabel,
  initialCriteria,
  nameLabel,
  newCriterionLabel,
  nextScrapeLabel,
  removeLabel,
  rescrapeNoticeLabel,
  rescrapeNowLabel,
  scrapeDecisionLabel,
  weightLabel
}: ScoreModelEditorProps) {
  const [criteria, setCriteria] = useState(initialCriteria);
  const [draftName, setDraftName] = useState("");
  const [draftWeight, setDraftWeight] = useState(1);
  const [newCriterionName, setNewCriterionName] = useState<string | null>(null);
  const [scrapeDecision, setScrapeDecision] = useState<string | null>(null);
  const [scrapeStatus, setScrapeStatus] = useState<"idle" | "queueing" | "queued" | "failed">("idle");

  const totalWeight = useMemo(
    () => criteria.reduce((sum, item) => sum + item.weight, 0),
    [criteria]
  );

  function updateCriterion(id: string, patch: Partial<ScoreCriterion>) {
    setCriteria((items) => items.map((item) => item.id === id ? { ...item, ...patch } : item));
  }

  function removeCriterion(id: string) {
    setCriteria((items) => items.filter((item) => item.id !== id));
  }

  function addCriterion() {
    const name = draftName.trim();
    if (!name) return;
    setCriteria((items) => [
      ...items,
      {
        id: `criterion-${Date.now()}`,
        name,
        weight: draftWeight
      }
    ]);
    setNewCriterionName(name);
    setScrapeDecision(null);
    setDraftName("");
    setDraftWeight(1);
  }

  async function chooseScrape(mode: "rescrape_now" | "next_standard_scrape", label: string) {
    if (!newCriterionName) return;
    setScrapeStatus("queueing");
    const response = await fetch("/api/marketing/competitive-analysis/scrape", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        criterion: newCriterionName,
        mode,
        triggerKind: "criterion_added"
      })
    });

    if (!response.ok) {
      setScrapeStatus("failed");
      return;
    }

    setScrapeDecision(label);
    setScrapeStatus("queued");
  }

  return (
    <div className="score-model-editor">
      <div className="score-model-summary">
        <span>{weightLabel}</span>
        <strong>{totalWeight.toFixed(2)}</strong>
      </div>
      <div className="score-model-rows">
        {criteria.map((criterion) => (
          <div className="score-model-row" key={criterion.id}>
            <input
              aria-label={`${nameLabel}: ${criterion.name}`}
              className="score-model-name"
              onChange={(event) => updateCriterion(criterion.id, { name: event.target.value })}
              value={criterion.name}
            />
            <input
              aria-label={`${weightLabel}: ${criterion.name}`}
              className="score-model-slider"
              max="2"
              min="0"
              onChange={(event) => updateCriterion(criterion.id, { weight: Number(event.target.value) })}
              step="0.05"
              type="range"
              value={criterion.weight}
            />
            <input
              aria-label={`${weightLabel} value: ${criterion.name}`}
              className="score-model-weight"
              max="2"
              min="0"
              onChange={(event) => updateCriterion(criterion.id, { weight: Number(event.target.value) })}
              step="0.05"
              type="number"
              value={criterion.weight.toFixed(2)}
            />
            <button
              aria-label={`${removeLabel}: ${criterion.name}`}
              className="score-model-icon"
              onClick={() => removeCriterion(criterion.id)}
              type="button"
            >
              x
            </button>
          </div>
        ))}
      </div>
      <div className="score-model-row score-model-new">
        <input
          aria-label={newCriterionLabel}
          className="score-model-name"
          onChange={(event) => setDraftName(event.target.value)}
          placeholder={newCriterionLabel}
          value={draftName}
        />
        <input
          aria-label={weightLabel}
          className="score-model-slider"
          max="2"
          min="0"
          onChange={(event) => setDraftWeight(Number(event.target.value))}
          step="0.05"
          type="range"
          value={draftWeight}
        />
        <input
          aria-label={`${weightLabel} value`}
          className="score-model-weight"
          max="2"
          min="0"
          onChange={(event) => setDraftWeight(Number(event.target.value))}
          step="0.05"
          type="number"
          value={draftWeight.toFixed(2)}
        />
        <button
          aria-label={addLabel}
          className="score-model-icon score-model-add"
          disabled={!draftName.trim()}
          onClick={addCriterion}
          type="button"
        >
          +
        </button>
      </div>
      {newCriterionName ? (
        <div className="score-model-rescrape-notice" role="status">
          <p>{rescrapeNoticeLabel.replace("{criterion}", newCriterionName)}</p>
          <div>
            <button disabled={scrapeStatus === "queueing"} onClick={() => chooseScrape("rescrape_now", rescrapeNowLabel)} type="button">{rescrapeNowLabel}</button>
            <button disabled={scrapeStatus === "queueing"} onClick={() => chooseScrape("next_standard_scrape", nextScrapeLabel)} type="button">{nextScrapeLabel}</button>
          </div>
          {scrapeDecision ? <strong>{scrapeDecisionLabel.replace("{decision}", scrapeDecision)}</strong> : null}
          {scrapeStatus === "failed" ? <strong>Scrape queue failed.</strong> : null}
        </div>
      ) : null}
    </div>
  );
}
