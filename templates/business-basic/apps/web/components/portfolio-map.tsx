"use client";

import type { CSSProperties } from "react";
import { useState } from "react";

type SupportedLocale = "en" | "de";
type AxisId =
  | "positioning"
  | "overlap"
  | "buyerClarity"
  | "employeeCatalog"
  | "hiringFlow"
  | "providerApi"
  | "pricingClarity"
  | "trust"
  | "seoVelocity";
type Localized = Record<SupportedLocale, string>;

type PortfolioCompetitor = {
  id: string;
  isOwn?: boolean;
  name: string;
  score: number;
  dimensions: Partial<Record<AxisId, number>>;
};

type PortfolioMapProps = {
  addCompetitorLabel: string;
  competitors: PortfolioCompetitor[];
  initialXAxis: AxisId;
  initialYAxis: AxisId;
  labels: {
    horizontalAxis: string;
    mapDescription: string;
    portfolioMap: string;
    verticalAxis: string;
  };
  locale: SupportedLocale;
  query: {
    drawer?: string;
    locale?: string;
    panel?: string;
    recordId?: string;
    theme?: string;
  };
  watchlistHref: string;
};

const axisOptions: Array<{ id: AxisId; label: Localized }> = [
  { id: "positioning", label: { en: "Positioning", de: "Positionierung" } },
  { id: "overlap", label: { en: "Overlap", de: "Überschneidung" } },
  { id: "buyerClarity", label: { en: "Buyer clarity", de: "Käuferklarheit" } },
  { id: "employeeCatalog", label: { en: "AI employee catalog", de: "KI-Mitarbeiter-Katalog" } },
  { id: "hiringFlow", label: { en: "Hiring flow", de: "Hiring Flow" } },
  { id: "providerApi", label: { en: "Provider API", de: "Provider API" } },
  { id: "pricingClarity", label: { en: "Pricing clarity", de: "Preisklarheit" } },
  { id: "trust", label: { en: "Trust", de: "Vertrauen" } },
  { id: "seoVelocity", label: { en: "SEO velocity", de: "SEO-Geschwindigkeit" } }
];

export function PortfolioMap({
  addCompetitorLabel,
  competitors,
  initialXAxis,
  initialYAxis,
  labels,
  locale,
  query,
  watchlistHref
}: PortfolioMapProps) {
  const [xAxis, setXAxis] = useState(initialXAxis);
  const [yAxis, setYAxis] = useState(initialYAxis);
  const [openAxis, setOpenAxis] = useState<"x" | "y" | null>(null);
  const xAxisLabel = axisLabel(xAxis, locale);
  const yAxisLabel = axisLabel(yAxis, locale);

  function setAxis(axis: "x" | "y", axisId: AxisId) {
    const nextXAxis = axis === "x" ? axisId : xAxis;
    const nextYAxis = axis === "y" ? axisId : yAxis;
    setXAxis(nextXAxis);
    setYAxis(nextYAxis);
    setOpenAxis(null);
    replaceAxisUrl(nextXAxis, nextYAxis);
  }

  function replaceAxisUrl(nextXAxis: AxisId, nextYAxis: AxisId) {
    const params = new URLSearchParams(window.location.search);
    params.set("xAxis", nextXAxis);
    params.set("yAxis", nextYAxis);
    window.history.replaceState(null, "", `${window.location.pathname}?${params.toString()}`);
  }

  return (
    <section className="os-pane os-map-pane" aria-label={labels.portfolioMap}>
      <div className="os-pane-head">
        <div>
          <h2>{labels.portfolioMap}</h2>
          <p>{labels.mapDescription.replace("{x}", xAxisLabel).replace("{y}", yAxisLabel)}</p>
        </div>
        <div className="os-pane-actions">
          <span>0-10</span>
          <a
            data-context-action="create"
            data-context-item
            data-context-label={addCompetitorLabel}
            data-context-module="marketing"
            data-context-record-id="new-source"
            data-context-record-type="watchlist"
            data-context-submodule="competitive-analysis"
            href={watchlistHref}
            aria-label={addCompetitorLabel}
          >
            +
          </a>
        </div>
      </div>
      <div className="os-portfolio-map">
        <AxisControl
          activeAxis={xAxis}
          axis="x"
          label={labels.horizontalAxis}
          locale={locale}
          onChange={setAxis}
          onToggle={setOpenAxis}
          open={openAxis === "x"}
        />
        <AxisControl
          activeAxis={yAxis}
          axis="y"
          label={labels.verticalAxis}
          locale={locale}
          onChange={setAxis}
          onToggle={setOpenAxis}
          open={openAxis === "y"}
        />
        {competitors.map((competitor) => {
          const xValue = dimensionValue(competitor, xAxis);
          const yValue = dimensionValue(competitor, yAxis);

          return (
            <a
              className={`matrix-point${competitor.isOwn ? " own-product-point" : ""}`}
              data-context-item
              data-context-module="marketing"
              data-context-submodule="competitive-analysis"
              data-context-record-type="competitor"
              data-context-record-id={competitor.id}
              data-context-label={competitor.name}
              href={panelHref(query, "competitor", competitor.id, "right", xAxis, yAxis)}
              key={competitor.id}
              style={{ "--x": `${xValue}%`, "--y": `${100 - yValue}%` } as CSSProperties}
              title={`${competitor.name}: ${competitor.score}/10`}
            >
              <span>{competitor.name}</span>
            </a>
          );
        })}
      </div>
    </section>
  );
}

function AxisControl({
  activeAxis,
  axis,
  label,
  locale,
  onChange,
  onToggle,
  open
}: {
  activeAxis: AxisId;
  axis: "x" | "y";
  label: string;
  locale: SupportedLocale;
  onChange: (axis: "x" | "y", axisId: AxisId) => void;
  onToggle: (axis: "x" | "y" | null) => void;
  open: boolean;
}) {
  return (
    <div className={`matrix-axis ${axis}-axis`} aria-label={label}>
      <button
        aria-expanded={open}
        className="matrix-axis-toggle"
        onClick={() => onToggle(open ? null : axis)}
        type="button"
      >
        <span>{label}</span>
        <strong>{axisLabel(activeAxis, locale)}</strong>
      </button>
      {open ? (
        <div className="axis-options">
          {axisOptions.map((option) => (
            <button
              aria-pressed={option.id === activeAxis}
              data-active={option.id === activeAxis ? "true" : "false"}
              key={option.id}
              onClick={() => onChange(axis, option.id)}
              type="button"
            >
              {text(option.label, locale)}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function panelHref(
  query: PortfolioMapProps["query"],
  panel: string,
  recordId: string,
  drawer: "left-bottom" | "bottom" | "right",
  xAxis: AxisId,
  yAxis: AxisId
) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("xAxis", xAxis);
  params.set("yAxis", yAxis);

  if (!(query.panel === panel && query.recordId === recordId)) {
    params.set("panel", panel);
    params.set("recordId", recordId);
    params.set("drawer", drawer);
  }

  return `/app/marketing/competitive-analysis?${params.toString()}`;
}

function axisLabel(axis: AxisId, locale: SupportedLocale) {
  return text(axisOptions.find((option) => option.id === axis)?.label ?? axisOptions[0].label, locale);
}

function dimensionValue(competitor: PortfolioCompetitor, axis: AxisId) {
  const directValue = competitor.dimensions[axis];
  if (typeof directValue === "number") return directValue;

  const score = competitor.score * 10;
  const overlap = competitor.dimensions.overlap ?? score;
  const buyerClarity = competitor.dimensions.buyerClarity ?? score;
  const trust = competitor.dimensions.trust ?? score;
  const seoVelocity = competitor.dimensions.seoVelocity ?? score;

  const derived: Record<AxisId, number> = {
    positioning: (overlap * 0.55) + (buyerClarity * 0.45),
    overlap,
    buyerClarity,
    employeeCatalog: (overlap * 0.45) + (score * 0.35) + (seoVelocity * 0.2),
    hiringFlow: (buyerClarity * 0.5) + (overlap * 0.3) + (score * 0.2),
    providerApi: (trust * 0.55) + (score * 0.25) + (buyerClarity * 0.2),
    pricingClarity: (buyerClarity * 0.65) + (trust * 0.2) + (score * 0.15),
    trust,
    seoVelocity
  };

  return Math.max(6, Math.min(94, Math.round(derived[axis])));
}

function text(value: Localized, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}
