"use client";

import { useMemo, useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type AccountingStoryWorkflow = {
  acceptance: { de: string; en: string };
  ctoxPrompt: { de: string; en: string };
  id: string;
  manual: { de: string; en: string };
  primaryAction: { de: string; en: string };
  submoduleId: string;
  title: { de: string; en: string };
};

type AccountingStoryWorkflowPanelProps = {
  contextPrompt?: string;
  locale: "de" | "en";
  recommendedStoryId?: string;
  stories: AccountingStoryWorkflow[];
  submoduleId: string;
};

type RunState = {
  id?: string;
  message: string;
  status: "error" | "idle" | "running" | "sent";
};

export function AccountingStoryWorkflowPanel({ contextPrompt, locale, recommendedStoryId, stories, submoduleId }: AccountingStoryWorkflowPanelProps) {
  const initialStoryId = stories.find((story) => story.id === recommendedStoryId)?.id ?? stories[0]?.id ?? "";
  const [selectedId, setSelectedId] = useState(initialStoryId);
  const [runState, setRunState] = useState<RunState>({ message: "", status: "idle" });
  const selected = useMemo(
    () => stories.find((story) => story.id === selectedId) ?? stories[0],
    [selectedId, stories]
  );
  const de = locale === "de";
  const otherStories = stories.filter((story) => story.id !== selected?.id);
  const visiblePrompt = contextPrompt ?? selected?.ctoxPrompt[locale] ?? "";

  async function prepareStory(story: AccountingStoryWorkflow) {
    setRunState({ id: story.id, message: de ? "CTOX-Vorschlag wird vorbereitet." : "Preparing CTOX proposal.", status: "running" });
    const response = await fetch("/api/business/accounting/story-workflows", {
      body: JSON.stringify({ locale, storyId: story.id }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const payload = await response.json().catch(() => ({ error: "invalid_response" })) as {
      error?: string;
      persisted?: boolean;
      workflow?: unknown;
    };

    if (!response.ok || payload.error) {
      setRunState({ id: story.id, message: payload.error ?? "Story workflow failed.", status: "error" });
      return;
    }

    notifyAccountingWorkflowUpdated({
      persisted: payload.persisted,
      workflow: payload.workflow
    });
    setRunState({
      id: story.id,
      message: payload.persisted
        ? (de ? "Vorschlag gespeichert." : "Proposal saved.")
        : (de ? "Demo-Vorschlag im Review." : "Demo proposal added."),
      status: "sent"
    });
  }

  if (!stories.length || !selected) return null;

  return (
    <section className="accounting-story-panel" data-submodule={submoduleId} aria-label={de ? "CTOX Assistenz" : "CTOX assist"}>
      <header>
        <div>
          <p>CTOX</p>
          <h2>{de ? "Vorschlag vorbereiten" : "Prepare proposal"}</h2>
        </div>
        <span>{de ? "Freigabe bleibt bei dir" : "You approve"}</span>
      </header>

      <article className="accounting-story-focus">
        <div className="accounting-story-kicker">
          <span>{de ? "Empfohlen" : "Recommended"}</span>
          <span>{selected.primaryAction[locale]}</span>
        </div>
        <h3>{de ? "Sage CTOX" : "Tell CTOX"}</h3>
        <blockquote>{visiblePrompt}</blockquote>
        <button disabled={runState.status === "running" && runState.id === selected.id} onClick={() => void prepareStory(selected)} type="button">
          {runState.status === "running" && runState.id === selected.id ? (de ? "Wird vorbereitet..." : "Preparing...") : (de ? "Vorschlag prüfen" : "Review proposal")}
        </button>
        {runState.id === selected.id && runState.message ? <small className={`is-${runState.status}`}>{runState.message}</small> : null}
        <details className="accounting-story-context">
          <summary>{de ? "Manueller Weg und Ergebnis" : "Manual path and result"}</summary>
          <p>{selected.manual[locale]}</p>
          <p>{selected.acceptance[locale]}</p>
        </details>
      </article>

      <details className="accounting-story-queue">
        <summary>{de ? "Weitere Befehle fuer diese Maske" : "More commands for this view"}</summary>
        <div role="list" aria-label={de ? "Weitere Abläufe" : "Other flows"}>
          {otherStories.map((story) => (
            <button
              aria-current={story.id === selected.id ? "true" : undefined}
              key={story.id}
              onClick={() => setSelectedId(story.id)}
              role="listitem"
              type="button"
            >
              <span>{story.id.replace("story-", "#")}</span>
              <strong>{story.title[locale]}</strong>
            </button>
          ))}
        </div>
      </details>
    </section>
  );
}
