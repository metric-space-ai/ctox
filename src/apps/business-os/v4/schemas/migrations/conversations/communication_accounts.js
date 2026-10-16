// Migration conversations/communication_accounts: Basis main@4794af03d (Schema v0) → Schema v1, neue Generation (§16.4).
// Erzeugt von v4/schemas/build-schemas.mjs (Strang SCH). Im App-Paket: migrations/communication_accounts.js. Gründe:
//   - Feld „id“ neu als Pflichtfeld
//   - Schema-Eigenschaft „primaryKey“ geändert
//   - Indizes geändert
export default function migrate(doc) {
  // id ← account_key (App-API 4.0 adressiert über id; account_key bleibt als Pflicht-/Indexfeld). Bereits gesetzte id bleibt.
  if (typeof doc.id === 'string' && doc.id) return doc;
  return { ...doc, id: doc.account_key };
}
