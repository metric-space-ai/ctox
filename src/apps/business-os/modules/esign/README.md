# E-Signatur

Generische Signatur-Anfragen: Dokument an Unterzeichner routen und Status verfolgen.

Generic Business OS ATS module. Owns the `signature_requests` collection(s); the engine lives in `core/` (pure, unit-tested) and the mount in `index.js` renders a record list + create form wired to that engine. Part of the ATS build-out (`docs/business-os-ats-plan.md`).
