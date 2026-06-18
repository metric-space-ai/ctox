import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  branchenzuschlagForWeeks,
  computeChargeRate,
  isEntgeltgruppe,
  validateWorkerMasterData,
} from './tariff.js';

test('isEntgeltgruppe validates pay-grade keys', () => {
  assert.ok(isEntgeltgruppe('E3'));
  assert.ok(!isEntgeltgruppe('E99'));
});

test('validateWorkerMasterData reports missing payroll fields', () => {
  assert.deepEqual(validateWorkerMasterData({ tax_id: '1', social_security_number: '2', tax_class: 'I', health_insurance: 'AOK', iban: 'DE..' }), {
    complete: true,
    missing: [],
  });
  const partial = validateWorkerMasterData({ tax_id: '1', payload: { iban: 'DE..' } });
  assert.equal(partial.complete, false);
  assert.ok(partial.missing.includes('social_security_number'));
  assert.ok(!partial.missing.includes('iban'), 'reads from payload too');
});

test('branchenzuschlagForWeeks steps up by tenure', () => {
  const schedule = [
    { after_weeks: 6, surcharge_pct: 15 },
    { after_weeks: 9, surcharge_pct: 20 },
    { after_weeks: 15, surcharge_pct: 30 },
  ];
  assert.equal(branchenzuschlagForWeeks(2, schedule), 0);
  assert.equal(branchenzuschlagForWeeks(7, schedule), 15);
  assert.equal(branchenzuschlagForWeeks(40, schedule), 30);
});

test('computeChargeRate applies surcharge then markup', () => {
  const r = computeChargeRate({ baseWage: 2000, markupFactor: 2, branchenzuschlagPct: 15 });
  assert.equal(r.payRate, 2300);
  assert.equal(r.chargeRate, 4600);
  assert.equal(r.equalPayApplies, false);
});

test('computeChargeRate enforces the Equal-Pay floor', () => {
  const r = computeChargeRate({ baseWage: 1800, markupFactor: 2, branchenzuschlagPct: 0, equalPayWage: 2100 });
  assert.equal(r.equalPayApplies, true);
  assert.equal(r.effectivePay, 2100);
  assert.equal(r.chargeRate, 4200);
});
