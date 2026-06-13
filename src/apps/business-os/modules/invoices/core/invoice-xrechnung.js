// core/invoice-xrechnung.js — pure XRechnung XML builder.
// Ported from XRechnung 2.x specification (https://xeinkauf.de/xrechnung/).
// XRechnung is the German standard for electronic invoices in public
// procurement (B2G). The browser keeps the canonical XML so the customer
// can hand it to procurement portals; PDF rendering goes through the
// CTOX print pipeline in a later phase.

import { aggregateTaxBreakdown, computeLineTotals, computeDueDateMs } from './invoice-tax.js';

/**
 * @typedef XRechnungOptions
 * @property {string} [leitwegId] Buyer-side routing ID (B2G)
 * @property {string} [bestellnummer] Purchase order reference
 */

/**
 * @param {object} invoice
 * @param {object} party
 * @param {object} supplier
 * @param {XRechnungOptions} [opts]
 * @returns {string} XML string
 */
export function buildXRechnungXml(invoice, party, supplier, opts = {}) {
  const ns = 'urn:xeinkauf:standard:vertrag';
  const lines = (invoice.lines || []).map((line) => ({
    ...line,
    ...computeLineTotals(line),
  }));
  const aggregate = aggregateTaxBreakdown(lines);
  const lineNo = (idx) => idx + 1;
  const esc = (s) => xmlEscape(String(s ?? ''));

  const due_date_ms = invoice.due_date_ms
    ? invoice.due_date_ms
    : invoice.invoice_date_ms && invoice.payment_terms
    ? computeDueDateMs(invoice.invoice_date_ms, invoice.payment_terms.net_days)
    : null;

  const header = `<?xml version="1.0" encoding="UTF-8"?>
<CrossIndustryInvoice xmlns="${ns}" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100" xmlns:qdt="urn:un:unece:uncefact:data:standard:QualifiedDataType:100">
  <Context>
    <DocumentContext>
      <ram:GuidelineSpecifiedDocumentContextParameter>
        <ram:ID>urn:xeinkauf:standard:vertrag:xrechnung:2.0</ram:ID>
      </ram:GuidelineSpecifiedDocumentContextParameter>
    </DocumentContext>
  </Context>
  <Header>
    ${invoice.invoice_number ? `<ram:DocumentInformation>\n      <ram:ID>${esc(invoice.invoice_number)}</ram:ID>\n    </ram:DocumentInformation>` : ''}
  </Header>
  <SupplyChainTradeTransaction>
    <ram:IncludedSupplyChainTradeLineItem>`;

  const lineXml = lines
    .map(
      (line, idx) => `
      <ram:AssociatedDocumentLineDocument>
        <ram:LineID>${lineNo(idx)}</ram:LineID>
      </ram:AssociatedDocumentLineDocument>
      <ram:SpecifiedTradeProduct>
        <ram:Name>${esc(line.description)}</ram:Name>
      </ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement>
        <ram:NetPriceProductTradePrice>
          <ram:ChargeAmount currencyID="${esc(invoice.currency || 'EUR')}">${(line.unit_price_cents / 100).toFixed(2)}</ram:ChargeAmount>
        </ram:NetPriceProductTradePrice>
      </ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery>
        <ram:BilledQuantity unitCode="${esc(line.unit || 'C62')}">${(line.quantity / 1000).toFixed(3)}</ram:BilledQuantity>
      </ram:SpecifiedLineTradeDelivery>
      <ram:SpecifiedLineTradeSettlement>
        <ram:ApplicableTradeTax>
          <ram:TypeCode>VAT</ram:TypeCode>
          <ram:CategoryCode>S</ram:CategoryCode>
          <ram:RateApplicablePercent>${(line.tax_rate * 100).toFixed(2)}</ram:RateApplicablePercent>
        </ram:ApplicableTradeTax>
        <ram:SpecifiedTradeSettlementLineMonetarySummation>
          <ram:LineTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">${(line.net_cents / 100).toFixed(2)}</ram:LineTotalAmount>
        </ram:SpecifiedTradeSettlementLineMonetarySummation>
      </ram:SpecifiedLineTradeSettlement>`
    )
    .join('');

  const lineItemClose = `
    </ram:IncludedSupplyChainTradeLineItem>`;

  const agreementXml = `
    <ram:ApplicableHeaderTradeAgreement>
      <ram:SellerTradeParty>
        <ram:Name>${esc(supplier?.name || 'CTOX')}</ram:Name>
        ${supplier?.vat_id ? `<ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">${esc(supplier.vat_id)}</ram:ID></ram:SpecifiedTaxRegistration>` : ''}
      </ram:SellerTradeParty>
      <ram:BuyerTradeParty>
        <ram:Name>${esc(party?.name || '')}</ram:Name>
      </ram:BuyerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>`;

  const taxXml = aggregate.tax_breakdown
    .map(
      (bucket) => `
    <ram:ApplicableTradeTax>
      <ram:TypeCode>VAT</ram:TypeCode>
      <ram:CategoryCode>S</ram:CategoryCode>
      <ram:BasisAmount currencyID="${esc(invoice.currency || 'EUR')}">${(bucket.net_cents / 100).toFixed(2)}</ram:BasisAmount>
      <ram:CalculatedAmount currencyID="${esc(invoice.currency || 'EUR')}">${(bucket.tax_cents / 100).toFixed(2)}</ram:CalculatedAmount>
      <ram:RateApplicablePercent>${(bucket.tax_rate * 100).toFixed(2)}</ram:RateApplicablePercent>
    </ram:ApplicableTradeTax>`
    )
    .join('');

  const footer = `
    <ram:SpecifiedTradeSettlement>
      ${opts.leitwegId ? `<ram:PayeePartyCreditorFinancialAccount><ram:ProprietaryID>${esc(opts.leitwegId)}</ram:ProprietaryID></ram:PayeePartyCreditorFinancialAccount>` : ''}
      ${invoice.invoice_number ? `<ram:PaymentReference>${esc(invoice.invoice_number)}</ram:PaymentReference>` : ''}
      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>
        <ram:LineTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">${(aggregate.subtotal_cents / 100).toFixed(2)}</ram:LineTotalAmount>
        <ram:ChargeTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">0.00</ram:ChargeTotalAmount>
        <ram:AllowanceTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">0.00</ram:AllowanceTotalAmount>
        <ram:TaxBasisTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">${(aggregate.subtotal_cents / 100).toFixed(2)}</ram:TaxBasisTotalAmount>
        <ram:TaxTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">${(aggregate.tax_cents / 100).toFixed(2)}</ram:TaxTotalAmount>
        <ram:GrandTotalAmount currencyID="${esc(invoice.currency || 'EUR')}">${(aggregate.total_cents / 100).toFixed(2)}</ram:GrandTotalAmount>
        <ram:DuePayableAmount currencyID="${esc(invoice.currency || 'EUR')}">${(aggregate.total_cents / 100).toFixed(2)}</ram:DuePayableAmount>
      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>
    </ram:SpecifiedTradeSettlement>
    <ram:SpecifiedSupplyChainTradeDelivery>
      ${due_date_ms ? `<ram:DueDateDateTime><udt:DateTimeString format="102">${isoDate(due_date_ms)}</udt:DateTimeString></ram:DueDateDateTime>` : ''}
    </ram:SpecifiedSupplyChainTradeDelivery>
  </SupplyChainTradeTransaction>
</CrossIndustryInvoice>
`;

  return header + lineXml + lineItemClose + agreementXml + taxXml + footer;
}

function xmlEscape(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

function isoDate(ms) {
  const d = new Date(ms);
  return `${d.getUTCFullYear()}${String(d.getUTCMonth() + 1).padStart(2, '0')}${String(d.getUTCDate()).padStart(2, '0')}`;
}

export default { buildXRechnungXml };
