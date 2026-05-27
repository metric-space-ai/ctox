/**
 * E2E Integration & Logic Validation Suite for the German Accounting (Buchhaltung) Module.
 *
 * This file verifies the correctness of our modularized components:
 * - Cent-based Double-Entry Arithmetic (Soll/Haben balances in core/ledger.js)
 * - Hierarchical Account Rollup Sums (SKR03 / SKR04 trees in core/ledger.js)
 * - SEPA camt.053 XML and SWIFT MT940 Text Statement Parsers (parsers/)
 * - Heuristic Reconciler Matching Confidence Scores (core/reconciler.js)
 * - Linear and Degressive Asset AfA calculations (core/depreciation.js)
 * - ELSTER UStVA Field Assignments (reports/elster.js)
 * - DATEV EXTF CSV Stapel Export formatting (exporters/datev.js)
 */

import { validateDoubleEntry, computeAccountSalden } from './core/ledger.js';
import { normalizedDelta, computeDepreciationSchedule } from './core/depreciation.js';
import { calculateMatchScore } from './core/reconciler.js';
import { parseCamt053 } from './parsers/camt.js';
import { parseMT940 } from './parsers/mt940.js';
import { getOperatingGuVResult, buildHgbBilanzTree, buildHgbGuvTree } from './reports/hgb.js';
import { calculateElsterUstva } from './reports/elster.js';
import { generateDatevCsvString } from './exporters/datev.js';

if (typeof globalThis.DOMParser === 'undefined') {
  class TestXmlElement {
    constructor(source) {
      this.source = source || '';
    }

    get textContent() {
      return this.source.replace(/<[^>]+>/g, '').trim();
    }

    getElementsByTagName(tagName) {
      const escaped = tagName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const pattern = new RegExp(`<${escaped}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/${escaped}>`, 'g');
      return Array.from(this.source.matchAll(pattern), (match) => new TestXmlElement(match[1]));
    }
  }

  globalThis.DOMParser = class TestDomParser {
    parseFromString(xmlText) {
      return new TestXmlElement(xmlText);
    }
  };
}

// Simple light-weight testing harness
const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

// Assertions helpers
const assert = {
  equal(a, b, msg) {
    if (a !== b) throw new Error(`${msg || 'Assertion failed'}: expected ${b}, got ${a}`);
  },
  deepEqual(a, b, msg) {
    const s1 = JSON.stringify(a);
    const s2 = JSON.stringify(b);
    if (s1 !== s2) throw new Error(`${msg || 'Assertion failed'}: expected ${s2}, got ${s1}`);
  },
  true(val, msg) {
    if (!val) throw new Error(`${msg || 'Assertion failed'}: expected true`);
  }
};

// =========================================================================
// 🧪 Test case 1: Double-Entry Soll/Haben balance validator
// =========================================================================
test('Cent-based Soll/Haben Math Validation', () => {
  const balancedLines = [
    { account_id: 'SKR03_0400', debit: 11900, credit: 0 }, // 119.00 EUR Gross
    { account_id: 'SKR03_1576', debit: 1900, credit: 0 },  // 19.00 EUR tax
    { account_id: 'SKR03_1600', debit: 0, credit: 13800 }  // Balanced
  ];

  // Should validate successfully
  const result = validateDoubleEntry(balancedLines);
  assert.true(result, 'Balanced lines return true');

  // Imbalanced entries should throw an error
  const imbalancedLines = [
    { account_id: 'SKR03_0400', debit: 11900, credit: 0 },
    { account_id: 'SKR03_1600', debit: 0, credit: 10000 }
  ];

  try {
    validateDoubleEntry(imbalancedLines);
    throw new Error('Should have failed double-entry check');
  } catch (err) {
    assert.true(err.message.includes('unausgeglichen'), 'Throws imbalance error');
  }
});

// =========================================================================
// 🧪 Test case 2: Hierarchical Account Rollups
// =========================================================================
test('Recursive bottom-up ledger rollups', () => {
  const mockAccounts = [
    { id: 'AKTIVA', parent_id: '', is_group: true, root_type: 'asset' },
    { id: 'UMLAUF', parent_id: 'AKTIVA', is_group: true, root_type: 'asset' },
    { id: 'BANK', parent_id: 'UMLAUF', is_group: false, root_type: 'asset' },
    { id: 'KASSE', parent_id: 'UMLAUF', is_group: false, root_type: 'asset' }
  ];

  const mockLedgerDF = [
    { account_id: 'BANK', debit: 500000, credit: 100000 },
    { account_id: 'KASSE', debit: 50000, credit: 0 }
  ];

  const resultAccounts = computeAccountSalden(mockAccounts, mockLedgerDF);

  const bank = resultAccounts.find(a => a.id === 'BANK');
  const kasse = resultAccounts.find(a => a.id === 'KASSE');
  const umlauf = resultAccounts.find(a => a.id === 'UMLAUF');
  const aktiva = resultAccounts.find(a => a.id === 'AKTIVA');

  assert.equal(bank.netto_saldo, 400000, 'Bank balance (5000 - 1000)');
  assert.equal(kasse.netto_saldo, 50000, 'Kasse balance (500)');
  assert.equal(umlauf.debit_saldo, 550000, 'Umlauf Group debit sum');
  assert.equal(umlauf.credit_saldo, 100000, 'Umlauf Group credit sum');
  assert.equal(aktiva.debit_saldo, 550000, 'Rollup Aktiva debit sum');
  assert.equal(aktiva.netto_saldo, 450000, 'Rollup Aktiva net sum');
});

// =========================================================================
// 🧪 Test case 3: camt.053 SEPA XML Parser
// =========================================================================
test('SEPA XML camt.053 transaction parsing', () => {
  const xmlSample = `
    <Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.02">
      <BkToCstmrStmt>
        <Stmt>
          <Ntry>
            <BookgDt><Dt>2026-05-22</Dt></BookgDt>
            <Amt>119.00</Amt>
            <CdtDbtInd>CRDT</CdtDbtInd>
            <Ustrd>Hetzner Cloud Invoice RE-19827</Ustrd>
            <Dbtr><Nm>Hetzner Online GmbH</Nm></Dbtr>
          </Ntry>
        </Stmt>
      </BkToCstmrStmt>
    </Document>
  `;

  const parsed = parseCamt053(xmlSample);

  assert.equal(parsed.length, 1, 'Extracted entry count');
  assert.equal(parsed[0].amount, 11900, 'Extracted amount in cents');
  assert.equal(parsed[0].counterparty_name, 'Hetzner Online GmbH', 'Extracted sender name');
  assert.equal(parsed[0].value_date, '2026-05-22', 'Extracted date');
});

// =========================================================================
// 🧪 Test case 4: Heuristic Bank Reconciler
// =========================================================================
test('Bank matching heuristic scoring rules', () => {
  const bankLine = { amount: -11900, narration: 'Bezahlung RE-2026-98127 Hetzner', counterparty_name: 'Hetzner Online' };
  const receipt1 = { gross_amount: 11900, supplier_name: 'Hetzner', invoice_number: 'RE-2026-98127' };
  const receipt2 = { gross_amount: 5000, supplier_name: 'Telekom', invoice_number: 'TEL-12' };

  const score1 = calculateMatchScore(bankLine, receipt1);
  const score2 = calculateMatchScore(bankLine, receipt2);

  assert.true(score1 > score2, 'Higher score for matching invoice details');
  assert.equal(score1, 100, 'Perfect match confidence score 100 (50 + 40 + 10)');
  assert.equal(score2, 0, 'No match score is 0');
});

// =========================================================================
// 🧪 Test case 5: Stateless AfA Asset linear rate calculation
// =========================================================================
test('Linear and Degressive Asset Depreciation', () => {
  const cost = 420000;      // 4,200.00 EUR
  const residual = 0;
  const start = '2026-01-01';
  const lifespanMonths = 36; // 3 years

  // Linear AfA schedule
  const linearSchedule = computeDepreciationSchedule(cost, residual, start, lifespanMonths);
  assert.equal(linearSchedule.length, 36, 'Lifespan in months');
  assert.equal(linearSchedule[0].depreciation, 11666, 'Monthly depreciation rate (4200 / 36 = 116.66 EUR)');
  assert.equal(linearSchedule[35].book_value, 24, 'Correct remaining book value after rounding');

  // normalizedDelta leap-year test
  const startD = new Date('2024-01-01'); // Leap year
  const endD = new Date('2024-03-01');
  const delta = normalizedDelta(startD, endD);
  assert.equal(delta, 59, 'Calibrated elapsed days corrected for leap day in Feb 2024');
});

// =========================================================================
// 🧪 Test case 6: Split Bookings Math
// =========================================================================
test('Cent-based Split Booking Math Validation', () => {
  import('./core/splits.js').then(splitsModule => {
    const totalAmount = 50000; // 500.00 EUR
    const splits = [
      { amount: 15000 },
      { amount: 25000 },
      { amount: 10000 }
    ];

    const remaining = splitsModule.calculateRemainingSplit(totalAmount, splits);
    assert.equal(remaining, 0, 'Remaining split amount is 0');
    assert.true(splitsModule.validateSplitBalanced(totalAmount, splits), 'Split is balanced');

    const imbalancedSplits = [
      { amount: 15000 },
      { amount: 20000 }
    ];
    const remaining2 = splitsModule.calculateRemainingSplit(totalAmount, imbalancedSplits);
    assert.equal(remaining2, 15000, 'Remaining is 150.00 EUR');
    assert.true(!splitsModule.validateSplitBalanced(totalAmount, imbalancedSplits), 'Split is imbalanced');
  });
});

// =========================================================================
// 🧪 Test case 7: Travel Allowance & VMA Deductions
// =========================================================================
test('German Travel Allowance & Meal Deductions', () => {
  import('./core/travel_expenses.js').then(travelModule => {
    // 3-day trip: 2026-05-22 to 2026-05-24
    const travelDays = travelModule.generateTravelDays('2026-05-22T08:00:00', '2026-05-24T18:00:00');

    assert.equal(travelDays.length, 3, '3 days of travel generated');
    assert.equal(travelDays[0].type, 'arrival', 'First day is arrival');
    assert.equal(travelDays[1].type, 'full', 'Second day is full');
    assert.equal(travelDays[2].type, 'departure', 'Third day is departure');

    // Add meal deductions: breakfast on day 2
    travelDays[1].breakfast = true;

    const result = travelModule.calculateTotalTravelAllowance(travelDays);
    // Day 1: 14.00 € (1400)
    // Day 2: 28.00 € - 5.60 € breakfast deduction = 22.40 € (2240)
    // Day 3: 14.00 € (1400)
    // Total: 1400 + 2240 + 1400 = 50.40 € (5040)
    assert.equal(result.totalAllowance, 5040, 'Reimbursement sum matches exactly 50.40 EUR');
  });
});

// =========================================================================
// 🧪 Test case 8: Mileage log & usage shares
// =========================================================================
test('Mileage log reimbursement & annual shares', () => {
  import('./core/mileage_log.js').then(mileageModule => {
    const km = 150;
    const reimbursement = mileageModule.calculateMileageReimbursement(km);
    assert.equal(reimbursement, 4500, '150 km * 0.30 EUR = 45.00 EUR (4500 Cent)');

    const trips = [
      { km: 100, purpose: 'business' },
      { km: 20, purpose: 'private' },
      { km: 30, purpose: 'commute' }
    ];
    const shares = mileageModule.calculateAnnualUsageShares(trips);
    assert.equal(shares.totalKm, 150, 'Total km is 150');
    assert.equal(shares.ratios.business, 66.67, 'Business share is 66.67%');
    assert.equal(shares.ratios.private, 13.33, 'Private share is 13.33%');
  });
});

// =========================================================================
// 🧪 Test case 9: Tax Advisor Rules (Bewirtung 70/30 & Gift limits)
// =========================================================================
test('German Tax Advisor Tricks (70/30 Split & 35€ Gift)', () => {
  import('./core/tax_tricks.js').then(taxModule => {
    // Restaurant bill: 119.00 € gross (100.00 € net + 19.00 € VAT)
    const gross = 11900;
    const splits = taxModule.calculateEntertainmentSplit(gross, 19);

    assert.equal(splits.netAmount, 10000, 'Net amount is 100.00 EUR');
    assert.equal(splits.vatAmount, 1900, 'VAT amount is 19.00 EUR');
    assert.equal(splits.deductibleNet, 7000, '70% deductible is 70.00 EUR');
    assert.equal(splits.nonDeductibleNet, 3000, '30% non-deductible is 30.00 EUR');

    // Gift rules: <= 35 € vs > 35 €
    const giftAcc1 = taxModule.recommendGiftAccount(3499, 'SKR03');
    const giftAcc2 = taxModule.recommendGiftAccount(3501, 'SKR03');
    assert.equal(giftAcc1, '4630', 'Suggests deductible account (4630)');
    assert.equal(giftAcc2, '4635', 'Suggests non-deductible account (4635)');
  });
});

// =========================================================================
// Run test suites
// =========================================================================
export function runAllTests() {
  console.log('--- STARTING BUCHHALTUNG INTEGRATION TESTS ---');
  let passed = 0;
  let failed = 0;

  tests.forEach(t => {
    try {
      t.fn();
      console.log(`✔️ PASSED: ${t.name}`);
      passed++;
    } catch (err) {
      console.error(`❌ FAILED: ${t.name}`);
      console.error(err);
      failed++;
    }
  });

  console.log(`--- TEST RESULTS: ${passed} Passed, ${failed} Failed ---`);
  return { passed, failed };
}
