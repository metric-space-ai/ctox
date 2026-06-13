// tests/invoice-xrechnung.test.mjs — exercises the XRechnung builder.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { buildXRechnungXml } from '../core/invoice-xrechnung.js';

const baseInvoice = () => ({
  id: 'inv_xr',
  invoice_number: 'RE-2026-0001',
  invoice_type: 'sale_out',
  party_id: 'cust_1',
  invoice_date_ms: Date.UTC(2026, 5, 1),
  payment_terms: { net_days: 14 },
  currency: 'EUR',
  lines: [
    {
      id: 'l1',
      position: 1,
      description: 'Beratung',
      quantity: 1000,
      unit: 'HUR',
      unit_price_cents: 12000,
      tax_rate: 0.19,
      account_code: '8400',
    },
  ],
});

const baseParty = () => ({ name: 'Acme GmbH' });
const baseSupplier = () => ({ name: 'CTOX Demo', vat_id: 'DE123456789' });

test('buildXRechnungXml returns a valid envelope with the right root element', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  assert.ok(xml.startsWith('<?xml'));
  assert.ok(xml.includes('<CrossIndustryInvoice'));
  assert.ok(xml.includes('</CrossIndustryInvoice>'));
});

test('buildXRechnungXml includes the seller name and VAT ID', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  assert.ok(xml.includes('CTOX Demo'));
  assert.ok(xml.includes('DE123456789'));
});

test('buildXRechnungXml includes the buyer name', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  assert.ok(xml.includes('Acme GmbH'));
});

test('buildXRechnungXml includes the due date as ISO 102', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  // Date format YYYYMMDD — invoice_date + 14d = 2026-06-15
  assert.ok(xml.includes('20260615'));
});

test('buildXRechnungXml includes the line description and quantity', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  assert.ok(xml.includes('Beratung'));
  assert.ok(xml.includes('HUR'));
  // 1000 / 1000 = 1.000
  assert.ok(xml.includes('1.000'));
});

test('buildXRechnungXml includes the grand total in EUR', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  // quantity=1000 (thousandths) * unit_price_cents=12_000 = 12_000 cent net
  // = 120.00 EUR. tax 19% = 2_280 cent = 22.80 EUR. total 14_280 cent = 142.80 EUR.
  assert.ok(xml.includes('142.80'), 'grand total 142.80 not found; XML head: ' + xml.slice(0, 2000));
  assert.ok(xml.includes('22.80'));
  assert.ok(xml.includes('120.00'));
});

test('buildXRechnungXml includes a tax breakdown entry', () => {
  const xml = buildXRechnungXml(baseInvoice(), baseParty(), baseSupplier());
  assert.ok(xml.includes('ApplicableTradeTax'));
  assert.ok(xml.includes('19.00'));
});

test('buildXRechnungXml escapes special characters in description', () => {
  const inv = baseInvoice();
  inv.lines[0].description = 'Buchführung <Test> & "Quote"';
  const xml = buildXRechnungXml(inv, baseParty(), baseSupplier());
  assert.ok(xml.includes('Buchführung &lt;Test&gt; &amp; &quot;Quote&quot;'));
});
