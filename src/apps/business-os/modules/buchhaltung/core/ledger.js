/**
 * core/ledger.js
 *
 * Core Double-Entry & Ledger Engine.
 * Handles Soll/Haben balances validation and hierarchical account salden rollups.
 */

/**
 * Validates that the sum of Debits equals the sum of Credits in a journal entry.
 * All amounts must be Cent-based integers.
 *
 * @param {Array} lines - The entry lines to validate
 * @returns {boolean} - True if balanced
 * @throws {Error} - If unbalanced or invalid amounts
 */
export function validateDoubleEntry(lines) {
  if (!lines || lines.length < 2) {
    throw new Error("Eine Buchung muss mindestens zwei Zeilen (Soll und Haben) enthalten.");
  }

  let debitSum = 0;
  let creditSum = 0;

  for (const line of lines) {
    const debit = line.debit || 0;
    const credit = line.credit || 0;

    if (!Number.isInteger(debit) || !Number.isInteger(credit)) {
      throw new Error("Buchungsbeträge müssen Cent-basierte Ganzzahlen (Integer) sein.");
    }
    if (debit < 0 || credit < 0) {
      throw new Error("Buchungsbeträge dürfen nicht negativ sein. Nutzen Sie Soll/Haben-Invertierung.");
    }

    debitSum += debit;
    creditSum += credit;
  }

  if (debitSum !== creditSum) {
    throw new Error(`Buchung unausgeglichen! Soll-Summe: ${debitSum} Cent, Haben-Summe: ${creditSum} Cent. Differenz: ${debitSum - creditSum} Cent.`);
  }

  return true;
}

/**
 * Computes rolled up debit, credit, and net balances for all accounts recursively.
 *
 * @param {Array} accounts - List of all accounts (groups and leaf nodes)
 * @param {Array} ledgerDF - Denormalized ledger lines representing posted entries
 * @returns {Array} - The accounts list updated with computed balances
 */
export function computeAccountSalden(accounts, ledgerDF) {
  // Clear previous salden and initialize
  const updatedAccounts = accounts.map(acct => ({
    ...acct,
    debit_saldo: 0,
    credit_saldo: 0,
    netto_saldo: 0
  }));

  // 1. Calculate direct balances from Ledger lines
  ledgerDF.forEach(line => {
    const acct = updatedAccounts.find(a => a.id === line.account_id);
    if (acct) {
      acct.debit_saldo += line.debit || 0;
      acct.credit_saldo += line.credit || 0;
    }
  });

  // 2. Perform recursive bottom-up hierarchical rollup
  const maxDepth = 6;
  for (let round = 0; round < maxDepth; round++) {
    updatedAccounts.forEach(acct => {
      if (acct.is_group) {
        const children = updatedAccounts.filter(a => a.parent_id === acct.id);
        let dSum = 0;
        let cSum = 0;
        children.forEach(child => {
          dSum += child.debit_saldo || 0;
          cSum += child.credit_saldo || 0;
        });
        acct.debit_saldo = dSum;
        acct.credit_saldo = cSum;
      }
    });
  }

  // 3. Compute net balance based on root account type (Assets/Expenses are Debit-heavy, others Credit-heavy)
  updatedAccounts.forEach(acct => {
    if (acct.root_type === 'asset' || acct.root_type === 'expense') {
      acct.netto_saldo = acct.debit_saldo - acct.credit_saldo;
    } else {
      acct.netto_saldo = acct.credit_saldo - acct.debit_saldo;
    }
  });

  return updatedAccounts;
}
