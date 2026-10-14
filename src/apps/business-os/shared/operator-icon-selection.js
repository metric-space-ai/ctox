// Canonical shell-facing projection of the frozen operator raster selection.
// This deliberately does not depend on the replicated module catalog: an old
// catalog row or desktop_icons glyph snapshot must never replace an approved
// product icon. Provenance is guarded against manifest.json in the companion
// contract test.
export const OPERATOR_ICON_SELECTION = Object.freeze({
  "ctox": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/ctox.jpg", sha256: "595492dd0ab23f3db59a0ebe9a6ec8062b1b2a80cbc54f566ee32b0501bcf0ce", candidateId: "candidate-02" }),
  "tickets": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/tickets.jpg", sha256: "6d0cbd2138977725b6043fd582b1d2b65b7214c8c9153ad7977942c26ebc4675", candidateId: "candidate-02" }),
  "threads": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/threads.jpg", sha256: "b4cf784881d8c5463837688755795dffbd66b5082333bf1508357df3a10d28ed", candidateId: "candidate-01" }),
  "knowledge": Object.freeze({ asset: "modules/knowledge/assets/icon/knowledge-256.png", sha256: "257526ec6c932c287be864be47e7e7708f32d136d587ab2d17da6cc62629c220", candidateId: "candidate-01" }),
  "browser": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/browser.jpg", sha256: "8fa94a7ae4b6db8885c0088a68cf736a23c0fd717a3711cd9b015c969f799a11", candidateId: "candidate-01" }),
  "credentials": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/credentials.jpg", sha256: "d796dc418c8f64faadc3f43e914a7bfb6ea6e6a4b54a7b6d643e6ff40b1f4e57", candidateId: "candidate-01" }),
  "mail": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/mail.jpg", sha256: "dd943203a5729623a71df00b3c994f5a9085cf8d1373c6c7f440494409f3bcc4", candidateId: "candidate-01" }),
  "app-store": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/app-store.jpg", sha256: "b6e9bc5bfa8bb42b86748efc00e9b5e736c4a0b6809b6c763aa6e8e7164de108", candidateId: "candidate-01" }),
  "importer": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/importer.jpg", sha256: "13c493fc01ebc97db2fd6d098281ffde490761deadfbfc9963db7b8ecda60e61", candidateId: "candidate-01" }),
  "reports": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/reports.jpg", sha256: "f83ec81f7954a899490a63d02af84fc4d145f49184b7884b97a749a8bd373275", candidateId: "candidate-01" }),
  "appsec-pentest": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/appsec-pentest.jpg", sha256: "85fd7ee28a9d4f8a047569cbc376b63e5f26d16584272ccd0785fae638fb1a53", candidateId: "candidate-01" }),
  "coding-agents": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/coding-agents.jpg", sha256: "1cf4cc4fd8e785f1b557be6dbbc3d0d8f02ab7462f9c16b17b163fc023c0bae3", candidateId: "candidate-09" }),
  "conversations": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/conversations.jpg", sha256: "1656adf1e387e7db50c4b7f6ea99998de43ecd6373fc1bedeb760e3cd109941b", candidateId: "candidate-01" }),
  "calendar": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/calendar.jpg", sha256: "c3de8ab4a29322bb08256ac953058904ac73d8d945df22f20df72ff988067f67", candidateId: "candidate-05" }),
  "news": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/news.jpg", sha256: "08066aefae5b7b58ae2e685ba24390efd5e214ec83c65e2be149b62fc1735759", candidateId: "candidate-01" }),
  "notes": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/notes.jpg", sha256: "aa1aafef932cb21d16611927780be82d4a5d68c9b474b70308aaeda83be5cf8b", candidateId: "candidate-06" }),
  "support": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/support.jpg", sha256: "d5058006ec868a0ea19378ea477e16f8ddb51bb92c7817c1c39c390291e26235", candidateId: "candidate-02" }),
  "customers": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/customers.jpg", sha256: "3a03210ceaa9ce7850ebd0409bff33e09e20e0c1850d9f84805682c307c4a5f9", candidateId: "candidate-16" }),
  "outbound": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/outbound.jpg", sha256: "a47a0a7e7e42acf918ae7e8a64304566b62ce94d31513a33894e163fda735c5a", candidateId: "candidate-05" }),
  "invoices": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/invoices.jpg", sha256: "1a0828f86f17d972434a30390e81f7d58fb8bff6e4fd4f9f5a7476088c36c52e", candidateId: "candidate-04" }),
  "buchhaltung": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/buchhaltung.jpg", sha256: "56b558dd0dbc5210bf6b0472ccef3df5602ce582b987ae04ee4c5f762d14eb0a", candidateId: "candidate-14" }),
  "consent": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/consent.jpg", sha256: "a886247228318a5bf11d85a7e64ffd4f9e914454327718be8e1f451f9b032596", candidateId: "candidate-13" }),
  "esign": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/esign.jpg", sha256: "766c1175e036a870e4a28465ddc9e294bd0df22a9376e3acb8e125683026089f", candidateId: "candidate-04" }),
  "intake": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/intake.jpg", sha256: "0a6d81e7d9c14b8526ab8d19f8344164d054db52fdc9b3856108f12729e532e7", candidateId: "candidate-14" }),
  "interviews": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/interviews.jpg", sha256: "642cda9e3cb5c56c9ac63b46d731696087f980b48423501f2d8428bf5554f9d7", candidateId: "candidate-01" }),
  "matching": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/matching.jpg", sha256: "3ececa42f99f1d63104468b920b6fea37fcc398186cae1f3b409de1647e2c3b7", candidateId: "candidate-02" }),
  "nachweise": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/nachweise.jpg", sha256: "cd43c8c297a229317fab2d9cd399e889e7377a6dcf16025bf52a1ce325f6d76a", candidateId: "candidate-11" }),
  "placements": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/placements.jpg", sha256: "f5818b645ddd2f8c4ea3f54a90b34003c1fb3fa86228b2f44ac5c4c90703079c", candidateId: "candidate-13" }),
  "shiftflow": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/shiftflow.jpg", sha256: "19b53504a0ce1129bc049039aa24c1e5f2baa64f70afe6cc23011420e5a81ef0", candidateId: "candidate-14" }),
  "submissions": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/submissions.jpg", sha256: "8cbfd230fafe8eacc84752410f1e1321024378125bb17189b9fcb7b7b39bfc13", candidateId: "candidate-08" }),
  "cv-print-builder": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/cv-print-builder.jpg", sha256: "bfda9b1a621037eb1d508614ddc3a9d4221abfe941976ddf326764d6b02c2b87", candidateId: "candidate-01" }),
  "documents": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/documents.jpg", sha256: "31123bf173860e01f05eb6be2431f71925bab9f355241b2582d30a2d06aef216", candidateId: "candidate-05" }),
  "iot": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/iot.jpg", sha256: "47e1a59d94e34baa238268a4eb5ec8190b8211e885ac418e106bb0de25bd2e43", candidateId: "candidate-14" }),
  "research": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/research.jpg", sha256: "a888d0b6ca2053cd15030286577951004fb449a97e79980df76ffaf643e796fb", candidateId: "candidate-14" }),
  "spreadsheets": Object.freeze({ asset: "shared/assets/workjet-icons/operator-selection-v1/spreadsheets.jpg", sha256: "664b5a31fa43c3d1135b15979c8348e0397dc648c37dbd823e681f4da35db284", candidateId: "candidate-05" }),
});

export function operatorIconFor(moduleId) {
  const normalized = String(moduleId || '').replace(/^module:|^desktop-app:/, '');
  return OPERATOR_ICON_SELECTION[normalized] || null;
}
