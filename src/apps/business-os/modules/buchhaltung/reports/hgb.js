/**
 * reports/hgb.js
 *
 * HGB Financial Reports Tree Generator.
 * Implements structural trees for HGB § 266 Bilanz (Aktiva/Passiva) and § 275 GuV (Gesamtkostenverfahren).
 */

/**
 * Calculates net profit or loss (Jahresüberschuss/-fehlbetrag) from revenues and expenses.
 *
 * @param {Array} accounts - Accounts list with computed salden
 * @returns {number} Operating result in Cents (+ for profit, - for loss)
 */
export function getOperatingGuVResult(accounts) {
  const isSkr04 = accounts.some(a => a.skr === 'SKR04');
  const revenueCode = isSkr04 ? '4000' : '8000';
  const expenseCode = isSkr04 ? '6000' : '4000';

  const revenueGroup = accounts.find(a => a.code === revenueCode);
  const expenseGroup = accounts.find(a => a.code === expenseCode);

  const revenueVal = revenueGroup ? (revenueGroup.credit_saldo - revenueGroup.debit_saldo) : 0;
  const expenseVal = expenseGroup ? (expenseGroup.debit_saldo - expenseGroup.credit_saldo) : 0;

  return revenueVal - expenseVal;
}

/**
 * Builds the structural trees for Aktiva and Passiva according to HGB § 266.
 *
 * @param {Array} accounts - Accounts list with computed salden
 * @param {number} operatingResult - Net profit/loss to inject into Eigenkapital
 * @returns {Object} Tree structure with Aktiva, Passiva, and Totals
 */
export function buildHgbBilanzTree(accounts, operatingResult) {
  // 1. GATHER AKTIVA GROUPS
  // Anlagevermögen: code Group 0100 (MacBook, Software, etc.)
  const anlagevermoegenGroup = accounts.find(a => a.code === '0100');
  const anlageBalance = anlagevermoegenGroup ? ((anlagevermoegenGroup.debit_saldo || 0) - (anlagevermoegenGroup.credit_saldo || 0)) : 0;
  const anlageSubs = accounts.filter(a => a.parent_id === anlagevermoegenGroup?.id)
    .map(sub => ({
      code: sub.code,
      name: sub.name,
      balance: sub.netto_saldo
    }));

  // Umlaufvermögen: Group 1100 / 1300
  const umlaufvermoegenGroup = accounts.find(a => a.code === '1100' || a.code === '1300');
  const umlaufBalance = umlaufvermoegenGroup ? ((umlaufvermoegenGroup.debit_saldo || 0) - (umlaufvermoegenGroup.credit_saldo || 0)) : 0;
  const umlaufSubs = accounts.filter(a => a.parent_id === umlaufvermoegenGroup?.id)
    .map(sub => {
      const leafs = accounts.filter(a => a.parent_id === sub.id)
        .map(leaf => ({
          code: leaf.code,
          name: leaf.name,
          balance: leaf.netto_saldo
        }));
      return {
        code: sub.code,
        name: sub.name,
        balance: sub.debit_saldo - sub.credit_saldo,
        children: leafs
      };
    });

  const totalAktiva = anlageBalance + umlaufBalance;

  // 2. GATHER PASSIVA GROUPS
  // Eigenkapital (Eigenkapital + operatingResult)
  const eigenkapitalGroup = accounts.find(a => a.code === '0800_G' || a.code === '2000');
  const baseEk = eigenkapitalGroup ? ((eigenkapitalGroup.credit_saldo || 0) - (eigenkapitalGroup.debit_saldo || 0)) : 0;
  const totalEk = baseEk + operatingResult;
  const ekSubs = accounts.filter(a => a.parent_id === eigenkapitalGroup?.id)
    .map(sub => ({
      code: sub.code,
      name: sub.name,
      balance: sub.credit_saldo - sub.debit_saldo
    }));

  // Rückstellungen: Group 0900 / 3000_G
  const rueckstellungenGroup = accounts.find(a => a.code === '0900' || a.code === '3000_G');
  const rueckBalance = rueckstellungenGroup ? ((rueckstellungenGroup.credit_saldo || 0) - (rueckstellungenGroup.debit_saldo || 0)) : 0;

  // Verbindlichkeiten: Group 1700 / 3500
  const verbindlichkeitenGroup = accounts.find(a => a.code === '1700' || a.code === '3500');
  const verbeBalance = verbindlichkeitenGroup ? ((verbindlichkeitenGroup.credit_saldo || 0) - (verbindlichkeitenGroup.debit_saldo || 0)) : 0;

  const totalPassiva = totalEk + rueckBalance + verbeBalance;

  return {
    aktiva: {
      anlagevermoegen: {
        name: "A. Anlagevermögen",
        total: anlageBalance,
        items: anlageSubs
      },
      umlaufvermoegen: {
        name: "B. Umlaufvermögen",
        total: umlaufBalance,
        items: umlaufSubs
      },
      total: totalAktiva
    },
    passiva: {
      eigenkapital: {
        name: "A. Eigenkapital",
        total: totalEk,
        items: ekSubs,
        operatingResult: operatingResult
      },
      rueckstellungen: {
        name: "B. Rückstellungen",
        total: rueckBalance
      },
      verbindlichkeiten: {
        name: "C. Verbindlichkeiten",
        total: verbeBalance
      },
      total: totalPassiva
    }
  };
}

/**
 * Builds the structural trees for HGB Income Statement (§ 275 Gesamtkostenverfahren).
 *
 * @param {Array} accounts - Accounts list with computed salden
 * @returns {Object} Structured GuV tree categories
 */
export function buildHgbGuvTree(accounts) {
  const isSkr04 = accounts.some(a => a.skr === 'SKR04');
  const revenueCode = isSkr04 ? '4000' : '8000';
  const expenseCode = isSkr04 ? '6000' : '4000';

  // 1. Umsatzerlöse
  const revenueGroup = accounts.find(a => a.code === revenueCode);
  const revenues = revenueGroup ? (revenueGroup.credit_saldo - revenueGroup.debit_saldo) : 0;
  const revSubs = accounts.filter(a => a.parent_id === revenueGroup?.id)
    .map(sub => ({
      code: sub.code,
      name: sub.name,
      balance: sub.credit_saldo - sub.debit_saldo
    }));

  // 2. Personalaufwand / Materialaufwand (Expenses)
  const expenseGroup = accounts.find(a => a.code === expenseCode);
  const expenses = expenseGroup ? (expenseGroup.debit_saldo - expenseGroup.credit_saldo) : 0;
  const expSubs = accounts.filter(a => a.parent_id === expenseGroup?.id)
    .map(sub => ({
      code: sub.code,
      name: sub.name,
      balance: sub.debit_saldo - sub.credit_saldo
    }));

  const jahresergebnis = revenues - expenses;

  return {
    umsatzerloese: {
      name: "1. Umsatzerlöse",
      total: revenues,
      items: revSubs
    },
    aufwendungen: {
      name: "2. Personal- und sonstige betriebliche Aufwendungen",
      total: expenses,
      items: expSubs
    },
    result: {
      name: jahresergebnis >= 0 ? "JAHRESÜBERSCHUSS" : "JAHRESFEHLBETRAG",
      total: jahresergebnis
    }
  };
}
