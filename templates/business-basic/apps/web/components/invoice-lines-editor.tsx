"use client";

import { useMemo, useState } from "react";

type SupportedLocale = "en" | "de";

export type InvoiceLineDraft = {
  currency: "EUR" | "USD";
  description: string;
  id: string;
  quantity: number;
  taxRate: number;
  title: string;
  unit: string;
  unitPrice: number;
};

type InvoiceLineCopy = {
  addLineItem: string;
  amount: string;
  deleteLine: string;
  description: string;
  discount: string;
  discountIn: string;
  duplicateLine: string;
  freeText: string;
  hour: string;
  lineItem: string;
  manageUnits: string;
  moveDown: string;
  moveUp: string;
  percent: string;
  piece: string;
  quantity: string;
  subtotalNet: string;
  taxRate: string;
  totalDiscount: string;
  totalGross: string;
  unit: string;
  unitNet: string;
  value: string;
  vat: string;
};

export function InvoiceLinesEditor({
  copy,
  initialLines,
  locale
}: {
  copy: InvoiceLineCopy;
  initialLines: InvoiceLineDraft[];
  locale: SupportedLocale;
}) {
  const [lines, setLines] = useState(initialLines);
  const [hasTotalDiscount, setHasTotalDiscount] = useState(false);
  const [totalDiscountMode, setTotalDiscountMode] = useState<"percent" | "amount">("percent");
  const [totalDiscountValue, setTotalDiscountValue] = useState("");
  const currency = lines[0]?.currency ?? "EUR";
  const totals = useMemo(() => {
    const subtotal = lines.reduce((sum, line) => {
      const net = line.quantity * line.unitPrice;
      return {
        gross: sum.gross + net * (1 + line.taxRate / 100),
        net: sum.net + net
      };
    }, { gross: 0, net: 0 });
    const parsedDiscount = Number(totalDiscountValue.replace(",", ".")) || 0;
    const discountNet = hasTotalDiscount
      ? totalDiscountMode === "percent"
        ? subtotal.net * (parsedDiscount / 100)
        : Math.min(parsedDiscount, subtotal.net)
      : 0;
    const discountRatio = subtotal.net > 0 ? discountNet / subtotal.net : 0;
    return {
      discountNet,
      gross: Math.max(0, subtotal.gross * (1 - discountRatio)),
      net: Math.max(0, subtotal.net - discountNet)
    };
  }, [hasTotalDiscount, lines, totalDiscountMode, totalDiscountValue]);

  const moveLine = (index: number, direction: -1 | 1) => {
    setLines((current) => {
      const nextIndex = index + direction;
      if (nextIndex < 0 || nextIndex >= current.length) return current;
      const next = [...current];
      const [line] = next.splice(index, 1);
      next.splice(nextIndex, 0, line);
      return next;
    });
  };

  const duplicateLine = (index: number) => {
    setLines((current) => {
      const source = current[index];
      if (!source) return current;
      const duplicate = { ...source, id: `${source.id}-copy-${Date.now()}` };
      const next = [...current];
      next.splice(index + 1, 0, duplicate);
      return next;
    });
  };

  const deleteLine = (index: number) => {
    setLines((current) => current.filter((_, lineIndex) => lineIndex !== index));
  };

  const setTaxRate = (index: number, taxRate: number) => {
    setLines((current) => current.map((line, lineIndex) => lineIndex === index ? { ...line, taxRate } : line));
  };

  const setUnit = (index: number, unit: string) => {
    setLines((current) => current.map((line, lineIndex) => lineIndex === index ? { ...line, unit } : line));
  };

  return (
    <section className="invoice-editor-card invoice-lines-card">
      <div className="invoice-lines-head">
        <span>{copy.lineItem}</span>
        <span>{copy.quantity}</span>
        <span>{copy.unit}</span>
        <span>{copy.unitNet}</span>
        <span>{copy.discount}</span>
        <span>{copy.amount}</span>
        <span />
      </div>
      {lines.map((line, index) => {
        const lineNet = line.quantity * line.unitPrice;

        return (
          <div className="invoice-line-editor" key={line.id}>
            <span className="invoice-line-index">{index + 1}</span>
            <div className="invoice-line-tools" aria-label={`${copy.lineItem} ${index + 1}`}>
              <button aria-label={copy.moveUp} disabled={index === 0} onClick={() => moveLine(index, -1)} type="button">
                <span className="invoice-icon-up" aria-hidden="true" />
              </button>
              <button aria-label={copy.moveDown} disabled={index === lines.length - 1} onClick={() => moveLine(index, 1)} type="button">
                <span className="invoice-icon-down" aria-hidden="true" />
              </button>
              <button aria-label={copy.duplicateLine} onClick={() => duplicateLine(index)} type="button">
                <span className="invoice-icon-duplicate" aria-hidden="true" />
              </button>
            </div>
            <InvoiceStaticField label={copy.lineItem} value={line.title} />
            <InvoiceStaticField label={copy.quantity} value={line.quantity.toLocaleString(locale === "de" ? "de-DE" : "en-US", { minimumFractionDigits: 2 })} />
            <label className="invoice-field invoice-select-field invoice-line-unit">
              <span>{copy.unit}</span>
              <select aria-label={`${copy.unit} ${index + 1}`} onChange={(event) => setUnit(index, event.target.value)} value={line.unit}>
                <option value={locale === "de" ? "Tagessatz" : "Day rate"}>{locale === "de" ? "Tagessatz" : "Day rate"}</option>
                <option value={copy.hour}>{copy.hour}</option>
                <option value={copy.piece}>{copy.piece}</option>
              </select>
              <button aria-label={copy.manageUnits} type="button">
                <span className="invoice-template-icon-lines" aria-hidden="true"><i /><i /><i /></span>
              </button>
            </label>
            <InvoiceStaticField label={copy.unitNet} value={formatCurrency(line.unitPrice, line.currency, locale)} />
            <InvoiceStaticField label={copy.discount} value="0,00 %" />
            <div className="invoice-line-total">
              <strong>{formatCurrency(lineNet, line.currency, locale)}</strong>
              <label className="invoice-tax-control">
                <span>{copy.taxRate}</span>
                <select aria-label={`${copy.taxRate} ${index + 1}`} onChange={(event) => setTaxRate(index, Number(event.target.value))} value={line.taxRate}>
                  <option value={19}>{copy.vat} 19 %</option>
                  <option value={7}>{copy.vat} 7 %</option>
                  <option value={0}>{copy.vat} 0 %</option>
                </select>
              </label>
            </div>
            <button aria-label={copy.deleteLine} className="invoice-line-delete" onClick={() => deleteLine(index)} type="button">
              <span className="invoice-trash-icon" aria-hidden="true" />
            </button>
            <InvoiceStaticField className="invoice-line-description" label={copy.description} muted value={line.description || copy.description} />
          </div>
        );
      })}
      <div className="invoice-line-actions">
        <button type="button">+ {copy.addLineItem}</button>
        <button type="button">{copy.freeText}</button>
        <button onClick={() => setHasTotalDiscount(true)} type="button">{copy.totalDiscount}</button>
      </div>
      {hasTotalDiscount ? (
        <div className="invoice-total-discount-row">
          <span className="invoice-total-discount-icon">%</span>
          <InvoiceStaticField label={copy.totalDiscount} muted value={copy.totalDiscount} />
          <label className="invoice-field invoice-select-field">
            <span>{copy.discountIn}</span>
            <select onChange={(event) => setTotalDiscountMode(event.target.value as "percent" | "amount")} value={totalDiscountMode}>
              <option value="percent">{copy.percent}</option>
              <option value="amount">{copy.amount}</option>
            </select>
          </label>
          <label className="invoice-field invoice-discount-value-field">
            <span>{copy.value}</span>
            <input
              inputMode="decimal"
              onChange={(event) => setTotalDiscountValue(event.target.value)}
              placeholder="0"
              value={totalDiscountValue}
            />
            <em>{totalDiscountMode === "percent" ? "%" : currency}</em>
          </label>
          <strong>{formatCurrency(totals.discountNet, currency, locale)}</strong>
          <button aria-label={copy.deleteLine} className="invoice-line-delete" onClick={() => {
            setHasTotalDiscount(false);
            setTotalDiscountValue("");
          }} type="button">
            <span className="invoice-trash-icon" aria-hidden="true" />
          </button>
        </div>
      ) : null}
      <div className="invoice-total-bar">
        <div>
          <span>{copy.subtotalNet}</span>
          <strong>{formatCurrency(totals.net, currency, locale)}</strong>
        </div>
        <div>
          <span>{copy.totalGross}</span>
          <strong>{formatCurrency(totals.gross, currency, locale)}</strong>
        </div>
      </div>
    </section>
  );
}

function InvoiceStaticField({
  className,
  label,
  muted,
  value
}: {
  className?: string;
  label: string;
  muted?: boolean;
  value: string;
}) {
  return (
    <div className={`invoice-field ${className ?? ""} ${muted ? "is-muted" : ""}`.trim()}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function formatCurrency(amount: number, currency: "EUR" | "USD", locale: SupportedLocale) {
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", {
    currency,
    maximumFractionDigits: 2,
    minimumFractionDigits: 2,
    style: "currency"
  }).format(amount);
}
