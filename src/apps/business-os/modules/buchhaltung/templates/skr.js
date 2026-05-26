export const SKR03_TEMPLATE = [
  // AKTIVA (Assets)
  { code: '0410', name: 'Geschäftsausstattung (Büromöbel, IT)', root_type: 'asset', account_type: 'fixed_asset', parent_id: '0400', is_group: false, tax_rate_id: '' },
  { code: '0400', name: 'Sachanlagen', root_type: 'asset', account_type: 'group', parent_id: '0100', is_group: true, tax_rate_id: '' },
  { code: '0100', name: 'Anlagevermögen', root_type: 'asset', account_type: 'group', parent_id: 'AKTIVA', is_group: true, tax_rate_id: '' },

  { code: '1000', name: 'Kasse', root_type: 'asset', account_type: 'cash', parent_id: '1200', is_group: false, tax_rate_id: '' },
  { code: '1200', name: 'Bank (Girokonto)', root_type: 'asset', account_type: 'bank', parent_id: '1100', is_group: false, tax_rate_id: '' },
  { code: '1400', name: 'Forderungen aus Lieferungen und Leistungen (Debitoren)', root_type: 'asset', account_type: 'receivable', parent_id: '1100', is_group: false, tax_rate_id: '' },
  { code: '1571', name: 'Abziehbare Vorsteuer 7%', root_type: 'asset', account_type: 'tax', parent_id: '1570', is_group: false, tax_rate_id: 'DE_7' },
  { code: '1576', name: 'Abziehbare Vorsteuer 19%', root_type: 'asset', account_type: 'tax', parent_id: '1570', is_group: false, tax_rate_id: 'DE_19' },
  { code: '1570', name: 'Vorsteuerkonten', root_type: 'asset', account_type: 'group', parent_id: '1100', is_group: true, tax_rate_id: '' },
  { code: '1100', name: 'Umlaufvermögen', root_type: 'asset', account_type: 'group', parent_id: 'AKTIVA', is_group: true, tax_rate_id: '' },

  // PASSIVA (Liabilities & Equity)
  { code: '0800', name: 'Gezeichnetes Kapital / Eigenkapital', root_type: 'liability', account_type: 'equity', parent_id: '0800_G', is_group: false, tax_rate_id: '' },
  { code: '0800_G', name: 'Eigenkapital', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  { code: '0950', name: 'Rückstellungen für Pensionen', root_type: 'liability', account_type: 'provisions', parent_id: '0900', is_group: false, tax_rate_id: '' },
  { code: '0900', name: 'Rückstellungen', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  { code: '1600', name: 'Verbindlichkeiten aus Lieferungen und Leistungen (Kreditoren)', root_type: 'liability', account_type: 'payable', parent_id: '1700', is_group: false, tax_rate_id: '' },
  { code: '1771', name: 'Umsatzsteuer 7%', root_type: 'liability', account_type: 'tax', parent_id: '1770', is_group: false, tax_rate_id: 'DE_7' },
  { code: '1776', name: 'Umsatzsteuer 19%', root_type: 'liability', account_type: 'tax', parent_id: '1770', is_group: false, tax_rate_id: 'DE_19' },
  { code: '1780', name: 'Umsatzsteuer-Vorauszahlungen', root_type: 'liability', account_type: 'tax', parent_id: '1770', is_group: false, tax_rate_id: '' },
  { code: '1770', name: 'Umsatzsteuerkonten', root_type: 'liability', account_type: 'group', parent_id: '1700', is_group: true, tax_rate_id: '' },
  { code: '1700', name: 'Verbindlichkeiten', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  // ERLÖSE (Revenue - SKR03 Class 8)
  { code: '8100', name: 'Steuerfreie Umsätze', root_type: 'revenue', account_type: 'revenue', parent_id: '8000', is_group: false, tax_rate_id: '' },
  { code: '8300', name: 'Erlöse 7% USt', root_type: 'revenue', account_type: 'revenue', parent_id: '8000', is_group: false, tax_rate_id: 'DE_7' },
  { code: '8400', name: 'Erlöse 19% USt', root_type: 'revenue', account_type: 'revenue', parent_id: '8000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '8000', name: 'Umsatzerlöse', root_type: 'revenue', account_type: 'group', parent_id: 'REVENUE', is_group: true, tax_rate_id: '' },

  // AUFWAND (Expenses - SKR03 Class 3 & 4)
  { code: '3400', name: 'Wareneinkauf 19% Vorsteuer', root_type: 'expense', account_type: 'expense', parent_id: '3000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '3000', name: 'Wareneingang / Materialaufwand', root_type: 'expense', account_type: 'group', parent_id: 'EXPENSE', is_group: true, tax_rate_id: '' },

  { code: '4210', name: 'Miete (Büroräume)', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false, tax_rate_id: '' },
  { code: '4600', name: 'Werbekosten', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '4830', name: 'Abschreibungen auf Sachanlagen (AfA)', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false, tax_rate_id: '' },
  { code: '4930', name: 'Bürobedarf / IT-Kosten / Software', root_type: 'expense', account_type: 'expense', parent_id: '4000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '4000', name: 'Betriebliche Aufwendungen', root_type: 'expense', account_type: 'group', parent_id: 'EXPENSE', is_group: true, tax_rate_id: '' }
];

export const SKR04_TEMPLATE = [
  // AKTIVA (Assets)
  { code: '0650', name: 'Geschäftsausstattung (Büromöbel, IT)', root_type: 'asset', account_type: 'fixed_asset', parent_id: '0600', is_group: false, tax_rate_id: '' },
  { code: '0600', name: 'Sachanlagen', root_type: 'asset', account_type: 'group', parent_id: '0100', is_group: true, tax_rate_id: '' },
  { code: '0100', name: 'Anlagevermögen', root_type: 'asset', account_type: 'group', parent_id: 'AKTIVA', is_group: true, tax_rate_id: '' },

  { code: '1600', name: 'Kasse', root_type: 'asset', account_type: 'cash', parent_id: '1300', is_group: false, tax_rate_id: '' },
  { code: '1800', name: 'Bank (Girokonto)', root_type: 'asset', account_type: 'bank', parent_id: '1300', is_group: false, tax_rate_id: '' },
  { code: '1200', name: 'Forderungen aus Lieferungen und Leistungen (Debitoren)', root_type: 'asset', account_type: 'receivable', parent_id: '1300', is_group: false, tax_rate_id: '' },
  { code: '1401', name: 'Abziehbare Vorsteuer 7%', root_type: 'asset', account_type: 'tax', parent_id: '1400', is_group: false, tax_rate_id: 'DE_7' },
  { code: '1406', name: 'Abziehbare Vorsteuer 19%', root_type: 'asset', account_type: 'tax', parent_id: '1400', is_group: false, tax_rate_id: 'DE_19' },
  { code: '1400', name: 'Vorsteuerkonten', root_type: 'asset', account_type: 'group', parent_id: '1300', is_group: true, tax_rate_id: '' },
  { code: '1300', name: 'Umlaufvermögen', root_type: 'asset', account_type: 'group', parent_id: 'AKTIVA', is_group: true, tax_rate_id: '' },

  // PASSIVA (Liabilities & Equity)
  { code: '2900', name: 'Gezeichnetes Kapital / Eigenkapital', root_type: 'liability', account_type: 'equity', parent_id: '2000', is_group: false, tax_rate_id: '' },
  { code: '2000', name: 'Eigenkapital', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  { code: '3000', name: 'Rückstellungen für Pensionen', root_type: 'liability', account_type: 'provisions', parent_id: '3000_G', is_group: false, tax_rate_id: '' },
  { code: '3000_G', name: 'Rückstellungen', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  { code: '3300', name: 'Verbindlichkeiten aus Lieferungen und Leistungen (Kreditoren)', root_type: 'liability', account_type: 'payable', parent_id: '3500', is_group: false, tax_rate_id: '' },
  { code: '3801', name: 'Umsatzsteuer 7%', root_type: 'liability', account_type: 'tax', parent_id: '3800', is_group: false, tax_rate_id: 'DE_7' },
  { code: '3806', name: 'Umsatzsteuer 19%', root_type: 'liability', account_type: 'tax', parent_id: '3800', is_group: false, tax_rate_id: 'DE_19' },
  { code: '3820', name: 'Umsatzsteuer-Vorauszahlungen', root_type: 'liability', account_type: 'tax', parent_id: '3800', is_group: false, tax_rate_id: '' },
  { code: '3800', name: 'Umsatzsteuerkonten', root_type: 'liability', account_type: 'group', parent_id: '3500', is_group: true, tax_rate_id: '' },
  { code: '3500', name: 'Verbindlichkeiten', root_type: 'liability', account_type: 'group', parent_id: 'PASSIVA', is_group: true, tax_rate_id: '' },

  // ERLÖSE (Revenue - SKR04 Class 4)
  { code: '4100', name: 'Steuerfreie Umsätze', root_type: 'revenue', account_type: 'revenue', parent_id: '4000', is_group: false, tax_rate_id: '' },
  { code: '4300', name: 'Erlöse 7% USt', root_type: 'revenue', account_type: 'revenue', parent_id: '4000', is_group: false, tax_rate_id: 'DE_7' },
  { code: '4400', name: 'Erlöse 19% USt', root_type: 'revenue', account_type: 'revenue', parent_id: '4000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '4000', name: 'Umsatzerlöse', root_type: 'revenue', account_type: 'group', parent_id: 'REVENUE', is_group: true, tax_rate_id: '' },

  // AUFWAND (Expenses - SKR04 Class 5 & 6)
  { code: '5400', name: 'Wareneinkauf 19% Vorsteuer', root_type: 'expense', account_type: 'expense', parent_id: '5000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '5000', name: 'Wareneingang / Materialaufwand', root_type: 'expense', account_type: 'group', parent_id: 'EXPENSE', is_group: true, tax_rate_id: '' },

  { code: '6310', name: 'Miete (Büroräume)', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false, tax_rate_id: '' },
  { code: '6600', name: 'Werbekosten', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '6220', name: 'Abschreibungen auf Sachanlagen (AfA)', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false, tax_rate_id: '' },
  { code: '6815', name: 'Bürobedarf / IT-Kosten / Software', root_type: 'expense', account_type: 'expense', parent_id: '6000', is_group: false, tax_rate_id: 'DE_19' },
  { code: '6000', name: 'Betriebliche Aufwendungen', root_type: 'expense', account_type: 'group', parent_id: 'EXPENSE', is_group: true, tax_rate_id: '' }
];

export async function importTemplateToDb(db, skrName) {
  const collection = db?.accounting_accounts;
  if (!collection) return;

  const template = skrName === 'SKR04' ? SKR04_TEMPLATE : SKR03_TEMPLATE;
  const now = Date.now();

  for (const acct of template) {
    const id = `${skrName}_${acct.code}`;
    const existing = await collection.findOne({ selector: { id } }).exec();
    if (!existing) {
      await collection.insert({
        id,
        code: acct.code,
        name: acct.name,
        root_type: acct.root_type,
        account_type: acct.account_type,
        parent_id: acct.parent_id ? `${skrName}_${acct.parent_id}` : '',
        is_group: acct.is_group,
        tax_rate_id: acct.tax_rate_id,
        skr: skrName,
        updated_at_ms: now
      });
    }
  }
}
