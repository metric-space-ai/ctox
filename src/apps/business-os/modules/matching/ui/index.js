// requirement matching view
// ---------------------------
// Datenquellen & Zustand
// ---------------------------

import { CtoxResizer } from '../../../shared/resizer.js';
import {
  computeRequirementMatch,
  computeTotalMatchScoreFromItems,
  recomputeAllMatchScoresOnce,
  shortlistObjectsForRequirement,
  shortlistRequirementsForObject
} from './matchingTools.js';
import { readViewState, patchViewState } from './viewState.js';
window.recomputeAllMatchScoresOnce = recomputeAllMatchScoresOnce;

import { CtoxQueuedCommandError, llmChat, queueObjectParseTask, queueRequirementParseTask } from './ctoxCommandAdapter.js';
import { getContactsCollection } from './businessOsDataSource.js';
import { createSyncFeedback } from './syncFeedback.js';
import { getActiveMatchingDefinition, matchingText, setActiveMatchingDefinition } from './matchingDefinition.js';
import { showBusinessAlert, showBusinessConfirm, showBusinessPrompt } from '../../../shared/dialogs.js';

const EMBEDDING_REQUEST_TIMEOUT_MS = 12_000;
const MATCH_SCORE_FORMULA_VERSION = 3;
const MATCH_SCORE_FORMULA_VERSION_KEY = 'requirementMatching.scoreFormulaVersion';
let activeObjectId = null;
function defText(path, fallback = '') {
  return matchingText(path, fallback);
}

function activeDefinitionId() {
  return getActiveMatchingDefinition()?.id || 'generic_matching.v1';
}

function activeSchemaVersion() {
  return getActiveMatchingDefinition()?.engine?.version || 'generic_matching.v1';
}

const MATCHING_VIEW_STATE_KEY = 'requirementMatchingView';
const MATCHING_VIEW_STATE_DEFAULTS = {
  activeSource: null,
  activeRequirementForScoring: null,
  selectedObject: null,
  matrixSelectedObjectId: null,
  activeTab: 'list',
  sourceSearch: '',
  requirementSearch: '',
  requirementFilter: 'all',
  objectSearch: '',
  objectSort: 'best',
  columnLayout: null,
  bulkMatchFilter: {
    enabled: false,
    minScore: 70
  },
  matchFilters: {}
};

let matchingViewState = readViewState(
  MATCHING_VIEW_STATE_KEY,
  MATCHING_VIEW_STATE_DEFAULTS
);

const syncFeedback = createSyncFeedback({ scope: 'matching-view' });
syncFeedback.wireWebRTCStatus();
let matchingModuleHost = null;

function getMatchingModuleHost() {
  return matchingModuleHost?.isConnected
    ? matchingModuleHost
    : document.querySelector('[data-matching-module="native"]') || document.body;
}

function appendMatchingLayer(element) {
  getMatchingModuleHost().appendChild(element);
}

export async function importObjectFromPdfFile(file) {
  const info = await importObjectsFromPdfFiles([file]);
  return {
    ...info,
    placeholderName: file?.name || info.placeholderName
  };
}

export async function importObjectsFromPdfFiles(files) {
  const list = Array.isArray(files) ? files.filter(Boolean) : [];
  if (!list.length) {
    throw new Error('Keine PDF-Dateien ausgewählt.');
  }

  const queued = await queueObjectParseTask({
    files: list,
    sourceLabel: list.length > 1 ? `upload:pdf:multi:${list.length}` : 'upload:pdf',
    filenames: list.map(f => f?.name || 'resume.pdf')
  });

  return {
    requirementId: queued.command_id || queued.commandId || queued.id || '',
    statusUrl: queued.statusUrl || '',
    importMode: 'pdf',
    sourceLabel: list.length > 1 ? `upload:pdf:multi:${list.length}` : 'upload:pdf',
    placeholderName: list.length > 1
      ? `${list.length} PDFs werden importiert …`
      : (list[0]?.name || 'PDF-Import'),
    fileCount: list.length,
    filenames: list.map(f => f?.name || 'resume.pdf'),
    queued: true
  };
}

function isImageFile(file) {
  if (!file) return false;
  const type = String(file.type || '').toLowerCase();
  const name = String(file.name || '').toLowerCase();
  return type.startsWith('image/') || /\.(png|jpe?g|webp|gif|bmp|avif)$/i.test(name);
}

function resizeImageFileToDataUrl(file, size = 256) {
  return new Promise((resolve, reject) => {
    if (!file || !isImageFile(file)) {
      reject(new Error('Keine Bilddatei ausgewählt.'));
      return;
    }

    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('Canvas konnte nicht initialisiert werden.');

        const sourceWidth = img.naturalWidth || img.width || size;
        const sourceHeight = img.naturalHeight || img.height || size;
        const side = Math.min(sourceWidth, sourceHeight);
        const sx = Math.max(0, Math.floor((sourceWidth - side) / 2));
        const sy = Math.max(0, Math.floor((sourceHeight - side) / 2));

        ctx.clearRect(0, 0, size, size);
        ctx.drawImage(img, sx, sy, side, side, 0, 0, size, size);
        URL.revokeObjectURL(url);
        resolve(canvas.toDataURL('image/jpeg', 0.86));
      } catch (error) {
        URL.revokeObjectURL(url);
        reject(error);
      }
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('Bild konnte nicht gelesen werden.'));
    };
    img.src = url;
  });
}

async function importObjectImageFile(file, targetObjectId = '') {
  if (!rxdb || !rxdb.objects) await loadFromRxdb();
  if (!rxdb || !rxdb.objects) throw new Error('Objekt-Datenbank ist nicht bereit.');

  const photo = await resizeImageFileToDataUrl(file, 256);
  const now = new Date().toISOString();
  const cleanFilename = String(file?.name || 'Bild').trim() || 'Bild';
  const nameFromFile = cleanFilename.replace(/\.[^.]+$/, '').replace(/[_-]+/g, ' ').trim() || cleanFilename;
  const selectedId = String(targetObjectId || '').trim();

  if (selectedId) {
    const existing = await rxdb.objects.findOne({ selector: { id: selectedId } }).exec();
    if (existing) {
      await existing.incrementalModify((prev) => ({
        ...prev,
        definitionId: prev.definitionId || activeDefinitionId(),
        schemaVersion: prev.schemaVersion || activeSchemaVersion(),
        photo,
        profilePhotoBase64: photo,
        updatedAt: now,
        documents: [
          ...(Array.isArray(prev.documents) ? prev.documents : []),
          {
            kind: 'Foto',
            filename: cleanFilename,
            parsed: true,
            meta: { width: 256, height: 256, resized: true, importedAt: now }
          }
        ]
      }));
      objectPhotoDataUrlCache.set(selectedId, photo);
      const uiObject = (objects || []).find((item) => item?.id === selectedId);
      if (uiObject) {
        uiObject.photo = photo;
        uiObject.updatedAt = now;
      }
      __markObjectsDirty();
      await loadFromRxdb();
      renderObjects({ reason: 'image-upload' });
      return { objectId: selectedId, updated: true, photo };
    }
  }

  const idSeed = (typeof crypto !== 'undefined' && crypto.randomUUID)
    ? crypto.randomUUID()
    : `${Date.now()}_${Math.random().toString(16).slice(2)}`;
  const id = `image_object_${String(idSeed).replace(/[^a-zA-Z0-9_-]/g, '').slice(0, 80)}`;
  const doc = {
    id,
    definitionId: activeDefinitionId(),
    schemaVersion: activeSchemaVersion(),
    name: nameFromFile,
    firstName: null,
    lastName: null,
    taxonomy: `importiertes ${defText('labels.objectRecord', 'Objekt')}`,
    photo,
    profilePhotoBase64: photo,
    active: true,
    hasRelation: false,
    skills: [],
    languages: [],
    education: [],
    experience: [],
    executiveInfo: {
      fachlicheQualifikation: '',
      methodenKompetenz: '',
      leadershipFaehigkeit: '',
      gehaltswunschUndOrt: ''
    },
    documents: [{
      kind: 'Foto',
      filename: cleanFilename,
      parsed: true,
      meta: { width: 256, height: 256, resized: true, importedAt: now }
    }],
    additional: [{
      key: 'system.import',
      value: { state: 'done', mode: 'image', filename: cleanFilename, importedAt: now }
    }],
    createdAt: now,
    updatedAt: now,
    status: 'active'
  };

  await rxdb.objects.upsert(doc);
  objectPhotoDataUrlCache.set(id, photo);
  __markObjectsDirty();
  await loadFromRxdb();
  renderObjects({ reason: 'image-upload' });
  return { objectId: id, updated: false, photo };
}

/**
 * Liefert ein sicheres Avatar-src (Attachment oder initials dataUrl).
 * @param {object} object - UI Object
 */
function getObjectAvatarSrc(object) {
  // akzeptiert string | array | object (chunks/base64/dataUrl)
  const raw = (object && object.photo != null) ? object.photo : '';
  const src = normalizeImageSrc(raw);
  if (src) return src;

  const name = (object && object.name) ? object.name : 'Objekt';
  const seed = (object && object.id) ? String(object.id) : name;
  return makeInitialsAvatarDataUrl({ name, seed, size: 100 });
}



function _escapeAttr(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function getObjectImportMetaFromAdditional(additional) {
  const add = Array.isArray(additional) ? additional : [];
  const entry = add.find(a => a && a.key === 'system.import');
  const value = entry && typeof entry.value === 'object' ? entry.value : null;
  if (!value) return null;

  const state = String(value.state || '').trim().toLowerCase();
  if (!state) return null;

  return {
    state,
    error: value.error ? String(value.error) : null
  };
}

function applyCurrentSorting() {
  const sourceSelect = document.getElementById('sourceSortSelect');
  const objectSelect = document.getElementById('objectSortSelect');

  if (sourceSelect) {
    sortSourceList(sourceSelect.value || 'updated_desc');
  }
  if (objectSelect) {
    sortObjectList(objectSelect.value || 'updated_desc');
  }
}

function hydrateImages(root = document) {
  const imgs = root.querySelectorAll('img[data-src], img[data-fallback]');
  imgs.forEach(img => {
    const rawSrc = img.getAttribute('data-src') || '';
    const rawFb  = img.getAttribute('data-fallback') || '';

    const src = normalizeImageSrc(rawSrc);
    const fb  = normalizeImageSrc(rawFb);

    if (!img.__fallbackBound) {
      img.__fallbackBound = true;
      img.addEventListener('error', () => {
        // verhindert Endlosschleife
        if (fb && img.src !== fb) img.src = fb;
      });
    }

    // src erst jetzt setzen (verhindert ERR_INVALID_URL beim HTML-parsen)
    if (src) img.src = src;
    else if (fb) img.src = fb;

    // optional (schadet nie)
    img.loading = img.loading || 'lazy';
    img.decoding = img.decoding || 'async';
  });
}


/**
 * Rendert <img> mit onerror fallback auf initials (NO external).
 * @param {object} opts
 * @param {string} opts.src
 * @param {string} opts.fallbackSrc
 * @param {string} opts.alt
 * @param {string} [opts.style]
 */
/**
 * CSP-safe <img>: KEIN inline onerror.
 * Wir rendern nur data-* und setzen src später via hydrateImages().
 */
function safeImgHtml({ src, fallbackSrc, alt, style = '' } = {}) {
  const esc = (x) => String(x ?? '')
    .replace(/&/g,'&amp;')
    .replace(/"/g,'&quot;')
    .replace(/</g,'&lt;')
    .replace(/>/g,'&gt;');

  return `<img alt="${esc(alt || '')}"
               style="${esc(style || '')}"
               data-src="${esc(src || '')}"
               data-fallback="${esc(fallbackSrc || '')}" />`;
}


function normalizeImageSrc(raw) {
  const joinParts = (arr) =>
    arr.map(x => (x == null ? '' : String(x))).join('');

  const coerce = (v) => {
    if (v == null) return '';

    if (typeof v === 'string') return v;

    if (Array.isArray(v)) return joinParts(v);

    if (typeof v === 'object') {
      if (Array.isArray(v.chunks)) return joinParts(v.chunks);
      if (Array.isArray(v.parts))  return joinParts(v.parts);
      if (Array.isArray(v.data))   return joinParts(v.data);

      if (typeof v.dataUrl === 'string') return v.dataUrl;
      if (typeof v.dataURL === 'string') return v.dataURL;
      if (typeof v.url === 'string')     return v.url;

      const b64 =
        (typeof v.base64 === 'string' ? v.base64 :
         typeof v.b64 === 'string' ? v.b64 : '');

      const mime =
        (typeof v.mime === 'string' ? v.mime :
         typeof v.contentType === 'string' ? v.contentType :
         typeof v.type === 'string' ? v.type : '');

      if (b64) {
        if (b64.startsWith('data:')) return b64;
        if (mime && mime.startsWith('image/')) return `data:${mime};base64,${b64}`;
        return b64;
      }
    }

    return String(v);
  };

  let s = coerce(raw).trim();
  if (!s) return '';

  // JSON-stringified chunk arrays: '["data:image/png;base64,AAA","BBB"]'
  if (s[0] === '[' && s[s.length - 1] === ']') {
    try {
      const parsed = JSON.parse(s);
      if (Array.isArray(parsed)) s = joinParts(parsed).trim();
    } catch (_) {}
  }
  if (!s) return '';

  // data: URL handling
  if (s.startsWith('data:')) {
    // harte Steuerzeichen raus
    s = s.replace(/[\r\n\t]+/g, '');

    // wenn base64: ALLES an whitespace NUR im payload entfernen
    const comma = s.indexOf(',');
    if (comma > 0 && /;base64/i.test(s.slice(0, comma))) {
      const head = s.slice(0, comma + 1);
      const payload = s.slice(comma + 1).replace(/\s+/g, '');
      s = head + payload;
    }
    return s;
  }

  // Nicht-data URLs: whitespace ist immer kaputt -> raus
  s = s.replace(/\s+/g, '');
  if (!s) return '';

  // erlaubte URL-Schemes
  if (s.startsWith('blob:') || s.startsWith('http://') || s.startsWith('https://')) return s;

  // missing "data:" prefix: "image/png;base64,...."
  if (s.startsWith('image/') && s.includes('base64,')) return `data:${s}`;

  // raw base64 heuristics (png/jpeg/webp)
  if (s.startsWith('iVBORw0KGgo')) return `data:image/png;base64,${s}`;
  if (s.startsWith('/9j/'))        return `data:image/jpeg;base64,${s}`;
  if (s.startsWith('UklGR'))       return `data:image/webp;base64,${s}`;

  return '';
}


// ---------------------------
// Rendering – Objekte (links)
// ---------------------------
function renderObjectsEdits() {
  const list = $('#objectList');
  if (!list) return;
  list.innerHTML = '';

  const all = objects || [];

  if (!all.length) {
    list.innerHTML =
      '<div class="muted" style="padding:8px;font-size:12px">Keine Objekte in der Datenbank.</div>';
    return;
  }

  if (activeObjectId && !all.some(c => c.id === activeObjectId)) {
    activeObjectId = null;
  }

  all.forEach(c => {
    const active = c.active !== false;
    const isPlaceholder = !!c.isPlaceholder;

    const card = el(
      'div',
      'object-card' +
      (activeObjectId === c.id ? ' selected' : '') +
      (!active && !isPlaceholder ? ' inactive-entity' : '') +
      (isPlaceholder ? ' pending' : '')
    );

    // 🔹 Daten für Sortierung
    card.dataset.name = c.name || '';
    card.dataset.updated = c.updatedAt || '';
    card.dataset.score = (c.score != null ? String(c.score) : '');
    card.dataset.availability = c.availabilityRaw || c.availabilityFrom || '';
    card.dataset.status = (c.objectStatus || '').toLowerCase();

    // Avatar: attachment (c.photo) oder initials fallback
    const fallbackAvatar = makeInitialsAvatarDataUrl({
      name: c.name || 'Objekt',
      seed: String(c.id || c.name || 'object'),
      size: 100
    });

    const avatarSrc = getObjectAvatarSrc(c); // nutzt normalizeImageSrc + fallback

    // Status-Text für Placeholder
    let placeholderLine = 'Warte auf Object-Service …';
    if (c.importError) {
      placeholderLine = `<span class="muted">Fehler: ${_escapeHtml(c.importError)}</span>`;
    } else if (c.importStatus) {
      placeholderLine = `Object-Service: ${_escapeHtml(c.importStatus)}`;
    }

    card.innerHTML = `
      <div class="object-head">
        <div class="object-avatar-wrap">
          <div class="avatar" style="width:40px;height:40px;border-radius:10px;overflow:hidden;border:1px solid var(--stroke)">
            ${safeImgHtml({
              src: avatarSrc,
              fallbackSrc: fallbackAvatar,
              alt: c.name,
              style: 'width:100%;height:100%;object-fit:cover'
            })}
          </div>
          <div class="toggle-row small">
            ${
              isPlaceholder
                ? '<span class="spinner" aria-label="Import läuft"></span>'
                : `
                  <div class="switch ${active ? 'is-on' : ''}" data-object-toggle="${c.id}">
                    <div class="switch-knob"></div>
                  </div>
                `
            }
          </div>
        </div>
        <div>
          <div class="object-name" style="font-weight:700">${_escapeHtml(c.name)}</div>
          <div class="c-tax">
            ${
              isPlaceholder
                ? (c.importError ? 'Import fehlgeschlagen' : 'Import läuft …')
                : _escapeHtml(c.taxonomy || c.currentRole || '–')
            }
          </div>
        </div>
      </div>
      <div class="c-skill">
        ${
          isPlaceholder
            ? placeholderLine
            : _escapeHtml(c.skillsSummary || '')
        }
      </div>
    `;

    card.addEventListener('click', () => {
      activeObjectId = c.id;
      renderObjectsEdits();
      syncObjectDetailForms();
    });

    if (!isPlaceholder) {
      const objectToggle = card.querySelector('.switch[data-object-toggle]');
      if (objectToggle) {
        objectToggle.addEventListener('click', e => {
          e.stopPropagation();
          setObjectActive(c.id, !c.active);
        });
      }
    }

    list.appendChild(card);
  });

  // ✅ wichtig: src setzen + error fallback via addEventListener (CSP-safe)
  hydrateImages(list);

  // Nach dem Rendern die aktuelle Sortierung anwenden
  applyCurrentSorting();
}

function syncObjectDetailForms() {
  const hint = $('#objectDetailHint');
  const fields = $('#objectEditorFields');
  const object = objects.find(c => c.id === activeObjectId) || null;

  if (!object) {
    if (hint) hint.textContent = 'Bitte links einen Objekte auswählen.';
    if (fields) fields.style.display = 'none';
    return;
  }

  if (object.isPlaceholder) {
    if (hint) {
      hint.textContent = object.importError
        ? 'Import fehlgeschlagen – Details über den Import prüfen.'
        : 'Import läuft – die Objekte-Daten werden automatisch geladen.';
    }
    if (fields) fields.style.display = 'none';
    return;
  }

  if (hint) hint.textContent = 'Bearbeite den ausgewählten Objekte.';
  if (!fields) return;
  fields.style.display = 'block';

  const avatar = $('#objectEditorAvatar');
  const nameInput = $('#objectNameInput');
  const firstNameInput = $('#objectFirstNameInput');
  const lastNameInput = $('#objectLastNameInput');
  const birthInput = $('#objectBirthDateInput');
  const nationalityInput = $('#objectNationalityInput');
  const genderSelect = $('#objectGenderSelect');

  const emailInput = $('#objectEmailInput');
  const phoneInput = $('#objectPhoneInput');
  const prefChannelSelect = $('#objectPreferredChannelSelect');
  const streetInput = $('#objectStreetInput');
  const postalInput = $('#objectPostalInput');
  const cityInput = $('#objectCityInput');
  const countryInput = $('#objectCountryInput');

  const currentRoleInput = $('#objectCurrentRoleInput');
  const desiredPositionInput = $('#objectDesiredPositionInput');
  const taxInput = $('#objectTaxInput');
  const industryInput = $('#objectIndustryInput');
  const availabilityInput = $('#objectAvailabilityInput');
  const regionInput = $('#objectRegionInput');
  const travelOkSelect = $('#objectTravelOkSelect');
  const workModelWishSelect = $('#objectWorkModelWishSelect');
  const driverLicenseInput = $('#objectDriverLicenseInput');

  const highestDegreeInput = $('#objectHighestDegreeInput');
  const degreeInput = $('#objectDegreeInput');
  const languagesInput = $('#objectLanguagesInput');
  const skillsInput = $('#objectSkillsInput');
  const softSkillsInput = $('#objectSoftSkillsInput');

  const execFachInput       = $('#objectExecFachInput');
  const execMethodenInput   = $('#objectExecMethodenInput');
  const execLeadershipInput = $('#objectExecLeadershipInput');
  const execSalaryInput     = $('#objectExecSalaryInput');

  const tagsInput = $('#objectTagsInput');
  const statusSelect = $('#objectStatusSelect');
  const hasRelationSwitch = $('#objectHasRelationSwitch');
  const hasRelationLabel = $('#objectHasRelationLabel');

  const activeSwitch = $('#objectActiveSwitch');
  const activeLabel = $('#objectActiveLabel');

  const exec = object.executiveInfo || {};

  if (avatar) {
    const fallbackAvatar = makeInitialsAvatarDataUrl({
      name: object.name || 'Objekt',
      seed: String(object.id || object.name || 'object'),
      size: 160
    });

    const src = getObjectAvatarSrc(object);

    avatar.innerHTML = safeImgHtml({
      src,
      fallbackSrc: fallbackAvatar,
      alt: object.name,
      style: 'width:100%;height:100%;object-fit:cover'
    });
    hydrateImages(avatar);
  }

  if (nameInput) nameInput.value = object.name || '';
  if (firstNameInput) firstNameInput.value = object.firstName || '';
  if (lastNameInput) lastNameInput.value = object.lastName || '';
  if (birthInput) birthInput.value = object.birthDate || '';
  if (nationalityInput) nationalityInput.value = object.nationality || '';
  if (genderSelect) genderSelect.value = object.gender || '';

  if (emailInput) emailInput.value = object.email || '';
  if (phoneInput) phoneInput.value = object.phone || '';
  if (prefChannelSelect) prefChannelSelect.value = object.preferredChannel || '';
  if (streetInput) streetInput.value = (object.address && object.address.street) || '';
  if (postalInput) postalInput.value = (object.address && object.address.postalCode) || '';
  if (cityInput) cityInput.value = (object.address && object.address.city) || '';
  if (countryInput) countryInput.value = (object.address && object.address.country) || 'DE';

  if (currentRoleInput) currentRoleInput.value = object.currentRole || '';
  if (desiredPositionInput) desiredPositionInput.value = object.desiredPosition || '';
  if (taxInput) taxInput.value = object.taxonomy || '';
  if (industryInput) industryInput.value = object.industry || '';
  if (availabilityInput) availabilityInput.value = object.availabilityFrom || '';
  if (regionInput) regionInput.value = object.region || '';

  if (travelOkSelect) {
    travelOkSelect.value =
      object.travelOk === true ? 'yes' :
      object.travelOk === false ? 'no' : '';
  }

  if (workModelWishSelect) workModelWishSelect.value = object.workModelWish || '';
  if (driverLicenseInput) driverLicenseInput.value = (object.driverLicense || []).join(', ');

  if (highestDegreeInput) highestDegreeInput.value = object.highestDegree || '';
  if (degreeInput) degreeInput.value = object.degree || '';
  if (languagesInput) languagesInput.value = object.languagesText || '';

  // ✅ FIX: Skills-Editor zeigt echte Skills, nicht Exec-Zusammenfassung
  if (skillsInput) {
    const txt =
      (typeof object.skillsEditorText === 'string' && object.skillsEditorText.trim())
        ? object.skillsEditorText
        : (Array.isArray(object.skillsArr) ? object.skillsArr.join(', ') : '');
    skillsInput.value = txt;
  }

  if (softSkillsInput) softSkillsInput.value = object.softSkillsSummary || '';

  if (execFachInput) execFachInput.value = exec.fachlicheQualifikation || '';
  if (execMethodenInput) execMethodenInput.value = exec.methodenKompetenz || '';
  if (execLeadershipInput) execLeadershipInput.value = exec.leadershipFaehigkeit || '';
  if (execSalaryInput) execSalaryInput.value = exec.gehaltswunschUndOrt || '';

  if (tagsInput) tagsInput.value = object.tagsSummary || '';
  if (statusSelect) statusSelect.value = object.objectStatus || 'neu';

  const hasRel = !!object.hasRelation;
  if (hasRelationSwitch) hasRelationSwitch.classList.toggle('is-on', hasRel);
  if (hasRelationLabel) hasRelationLabel.textContent = hasRel ? 'Beziehung vorhanden' : 'Keine aktive Beziehung';

  const isActive = object.active !== false;
  if (activeSwitch) activeSwitch.classList.toggle('is-on', isActive);
  if (activeLabel) activeLabel.textContent = isActive ? 'Aktiv' : 'Inaktiv';

  renderObjectCv(object);

  const objectBody = $('#objectCvBody');
  const objectToggleBtn = $('#objectCvToggleBtn');
  if (objectBody && objectToggleBtn) {
    objectBody.style.display = 'none';
    if (!objectToggleBtn.disabled) objectToggleBtn.textContent = 'Object anzeigen';
  }

  clearAllDirtyMarks(document.getElementById('objectEditorFields') || document);
}

function createUiPlaceholderObject({ placeholderId, displayName, sourceLabel }) {
  const pendingObjectName = `${defText('labels.objectRecord', 'Objekt')} wird importiert …`;
  const id = String(
    placeholderId ||
    `pending_${Date.now()}_${Math.random().toString(16).slice(2)}`
  ).slice(0, 128);

  const existing = (objects || []).find(c => c && c.id === id);
  if (existing) {
    existing.isPlaceholder = true;
    existing.name = String(displayName || existing.name || pendingObjectName).slice(0, 256);
    existing.importError = null;
    existing.importStatus = existing.importStatus || 'queued';
    existing.updatedAt = new Date().toISOString();
    renderObjects();
    return id;
  }

  const name = String(displayName || pendingObjectName).slice(0, 256);
  const uiObject = {
    id,
    definitionId: activeDefinitionId(),
    schemaVersion: activeSchemaVersion(),
    name,
    tax: 'Import läuft …',
    skills: '',
    skillsSummary: '',
    photo: makeInitialsAvatarDataUrl({ name, seed: id, size: 100 }),
    executiveInfo: {
      fachlicheQualifikation: '',
      methodenKompetenz: '',
      leadershipFaehigkeit: '',
      gehaltswunschUndOrt: ''
    },
    isPlaceholder: true,
    importStatus: 'queued',
    importError: null,
    _hasRelation: false,
    active: false,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    objectStatus: 'neu',
    object: { meta: {}, education: [], experience: [], skills: {} },
    _importSourceLabel: sourceLabel || ''
  };

  objects = [uiObject, ...(Array.isArray(objects) ? objects : [])];
  renderObjects();
  return id;
}

function markObjectPlaceholderAsError(placeholderId, errorMsg) {
  const object = (objects || []).find(c => c && c.id === placeholderId);
  if (!object) return;
  object.importError = errorMsg || 'Unbekannter Fehler beim Import.';
  object.importStatus = null;
  object.updatedAt = new Date().toISOString();
  renderObjects();
}

function updateObjectPlaceholderStatus(placeholderId, status) {
  const object = (objects || []).find(c => c && c.id === placeholderId);
  if (!object) return;
  object.importStatus = status || '';
  object.importError = null;
  object.updatedAt = new Date().toISOString();
  renderObjects();
}

function getInitials(name) {
  const s = String(name || '').trim();
  if (!s) return '?';
  const parts = s.split(/\s+/).filter(Boolean);
  const a = parts[0]?.[0] || '?';
  const b = parts.length > 1 ? (parts[parts.length - 1]?.[0] || '') : '';
  return (a + b).toUpperCase();
}

function _escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * DROP-IN REPLACEMENT
 * Deterministisches Inline-SVG als data URI (kein Netzwerk).
 * Fix: Keine echten Spaces in der data-URI erzeugen (sonst killt normalizeImageSrc sie).
 * @param {object} opts
 * @param {string} opts.name
 * @param {string} opts.seed - deterministisch (z.B. objectId)
 * @param {number} [opts.size=100]
 */
function makeInitialsAvatarDataUrl({ name, seed, size = 100 } = {}) {
  const initials = getInitials(name);
  const sd = String(seed || name || 'seed');

  // simple deterministic color from seed (no crypto)
  let h = 0;
  for (let i = 0; i < sd.length; i++) h = (h * 31 + sd.charCodeAt(i)) >>> 0;

  // pleasant-ish HSL (Spaces sind hier ABSICHTLICH im SVG,
  // aber wir lassen sie URL-encoded, damit nix kaputt-normalisiert wird.)
  const hue = h % 360;
  const bg = `hsl(${hue} 55% 35%)`;
  const fg = `hsl(${hue} 80% 92%)`;

  const fontSize = Math.round(size * 0.42);
  const safeInitials = _escapeHtml(initials);

  const svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" rx="${Math.round(size * 0.18)}" fill="${bg}"/>
  <text x="50%" y="52%" text-anchor="middle" dominant-baseline="middle"
        font-family="ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial"
        font-size="${fontSize}" font-weight="700" fill="${fg}">
    ${safeInitials}
  </text>
</svg>`.trim();

  // ✅ Wichtig:
  // - Keine "%20 -> ' '" Rückwandlung mehr
  // - Nur Zeilenumbrüche raus, Rest bleibt encoded
  const encoded = encodeURIComponent(svg)
    .replace(/%0A/g, '')
    .replace(/%0D/g, '');

  // charset ist nett, aber optional; hilft in manchen Browsern/Setups
  return `data:image/svg+xml;charset=utf-8,${encoded}`;
}

async function startObjectImportBackground(requirementInfo) {
  const {
    requirementId,
    statusUrl,
    importMode,
    sourceLabel,
    placeholderName,
    importId: providedImportId,
    placeholderId: providedPlaceholderId
  } = requirementInfo || {};

  if (!requirementId) {
    throw new Error('Objektimport konnte nicht gestartet werden (CTOX Task-ID fehlt).');
  }

  // stabile, eindeutige ID pro Import-Lauf (wichtig für Multi-Select!)
  const importId = providedImportId || (
    (typeof crypto !== 'undefined' && crypto.randomUUID)
      ? crypto.randomUUID()
      : `imp_${Date.now()}_${Math.random().toString(16).slice(2)}`
  );

  const placeholderId = providedPlaceholderId ||
    `pending_${importId.replace(/[^a-zA-Z0-9_:-]/g, '')}`;

  if (providedPlaceholderId) {
    updateObjectPlaceholderStatus(placeholderId, 'queued (CTOX)');
  }

  updateObjectPlaceholderStatus(placeholderId, 'queued (CTOX)');

  return {
    importId,
    placeholderId,
    requirementId,
    statusUrl,
    importMode,
    sourceLabel,
    placeholderName
  };
}

export async function startPdfObjectImport(file) {
  const requirementInfo = await importObjectFromPdfFile(file);
  return startObjectImportBackground(requirementInfo);
}



// Kontakt-/Beziehungsstatus (Demo): true = vorhanden, false = noch nicht
const relations = {
  sources: {},
  objects: {}
};

/* Städte-Koordinaten (schematisch) für die Karte */
const cityPos = {
  'Berlin':             {lon:13.404954, lat:52.520008},
  'München':            {lon:11.576124, lat:48.137154},
  'Hamburg':            {lon:9.993682,  lat:53.551086},
  'Köln':               {lon:6.960279,  lat:50.937531},
  'Leipzig':            {lon:12.373075, lat:51.339695},
  'Basel':              {lon:7.588576,  lat:47.559601},
  'Stuttgart':          {lon:9.181332,  lat:48.777128},
  'Düsseldorf':         {lon:6.773456,  lat:51.227741},
  'Allendorf (Eder)':   {lon:8.78,      lat:51.03},
  'Esslingen am Neckar':{lon:9.31,      lat:48.74},
  'Minden':             {lon:8.92,      lat:52.29},
  'Ditzingen':          {lon:9.07,      lat:48.83},
  'Remote':             null
};

let rxdb = null;
let sources = [];
let objects = [];
let matches = [];
const rawDocCounts = { sources: 0, requirements: 0, objects: 0, matches: 0 };

function hasUnsyncedMatchingData() {
  // True when the RxDB collections hold records but the aggregated UI arrays
  // are still empty. This is the classic "data is there but not yet projected"
  // state — show a sync placeholder instead of "empty database".
  return (rawDocCounts.sources > 0 || rawDocCounts.requirements > 0)
    && sources.length === 0;
}

// requirement matching view globals
const pendingMatchKeys = new Set(); // Schlüssel: `${requirementId}|${objectId}`

// Farben für einzelne Nachweise (Match-Items)
const MATCH_COLORS = [
  '#6c8cff',
  '#3ddc97',
  '#ffb454',
  '#ff6b6b',
  '#e86af0',
  '#35c9e0',
  '#9b8cff',
  '#cddc39'
];

// aktuell im Fokus stehender Match für Detail-Leiste
// { sourceId, requirementId, objectId, items, colorByRequirement: { [requirementId]: color } }
let currentMatchDetail = null;

// ---------------------------
// Object Photo via RxDB Attachment (NEU)
// ---------------------------

// Optional: kleines Cache, damit nicht bei jedem render alles neu gelesen wird
const objectPhotoDataUrlCache = new Map(); // objectId -> dataUrl

function _blobToDataUrl(blob) {
  return new Promise((resolve) => {
    if (!blob) return resolve(null);
    const r = new FileReader();
    r.onload = () => resolve(typeof r.result === 'string' ? r.result : null);
    r.onerror = () => resolve(null);
    r.readAsDataURL(blob);
  });
}

// sucht ein sinnvolles Attachment (id & type sind je nach deinem SW/Schema ggf. anders)
async function _getObjectPhotoAttachment(doc) {
  if (!doc) return null;

  // RxDB v12/v13: doc.getAttachment(id) + doc.allAttachments()
  const knownIds = [
    'profilePhoto',
    'profile_photo',
    'photo',
    'avatar',
    'profilePhoto.jpg',
    'profilePhoto.jpeg',
    'profilePhoto.png'
  ];

  // 1) bekannte IDs direkt probieren
  for (const id of knownIds) {
    try {
      const a = await doc.getAttachment(id);
      if (a) return a;
    } catch (_) {}
  }

  // 2) sonst: erstes Bild-Attachment nehmen
  try {
    const list = await doc.allAttachments(); // returns RxAttachment[]
    if (Array.isArray(list) && list.length) {
      // bevorzugt image/*
      const img = list.find(a => {
        const t = (a?.type || '').toLowerCase();
        return t.startsWith('image/');
      });
      return img || list[0] || null;
    }
  } catch (_) {}

  return null;
}

async function getObjectPhotoDataUrlFromChunks(objectId) {
  if (!rxdb || !rxdb.object_photo_chunks || !objectId) return null;

  try {
    const docs = await rxdb.object_photo_chunks
      .find({ selector: { objectId: objectId }, sort: [{ idx: 'asc' }] })
      .exec();

    const rows = (docs || []).map(d => d.toJSON());
    if (!rows.length) return null;

    const mime = String(rows[0]?.mime || 'image/png').toLowerCase();
    const total = Math.max(1, Number(rows[0]?.total || rows.length) || rows.length);
    const byIdx = new Map();
    for (const row of rows) {
      const idx = Number(row?.idx);
      if (!Number.isFinite(idx) || idx < 0) continue;
      if (!byIdx.has(idx)) byIdx.set(idx, row);
    }

    const ordered = [];
    for (let idx = 0; idx < total; idx++) {
      const row = byIdx.get(idx);
      if (!row) return null;
      ordered.push(row);
    }

    const parsed = normalizeObjectPhotoDataUrl(
      `data:${mime};base64,${ordered.map(r => String(r?.data || '')).join('')}`,
      mime
    );
    return parsed;
  } catch (e) {
    console.warn('[photo] chunks fallback failed', e);
    return null;
  }
}

function normalizeObjectPhotoDataUrl(raw, fallbackMime = 'image/png') {
  let s = String(raw || '').trim();
  if (!s) return null;

  s = s.replace(/^Data:/, 'data:');
  s = s.replace(/;base6(,|$)/i, ';base64$1');

  if (/^data:/i.test(s)) {
    const comma = s.indexOf(',');
    if (comma < 0) return null;
    const head = s.slice(0, comma).trim();
    const body = s.slice(comma + 1).trim().replace(/\s+/g, '').replace(/-/g, '+').replace(/_/g, '/');
    const m = head.match(/^data:([^;]+);base64$/i);
    if (!m) return null;
    const mime = String(m[1] || fallbackMime).toLowerCase();
    if (!mime.startsWith('image/')) return null;
    if (!body) return null;
    const pad = body.length % 4;
    const normalized = pad ? body + '='.repeat(4 - pad) : body;
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(normalized)) return null;
    return `data:${mime};base64,${normalized}`;
  }

  const body = s.replace(/\s+/g, '').replace(/-/g, '+').replace(/_/g, '/');
  if (!body) return null;
  const pad = body.length % 4;
  const normalized = pad ? body + '='.repeat(4 - pad) : body;
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(normalized)) return null;
  return `data:${fallbackMime};base64,${normalized}`;
}


/**
 * Liefert eine DataURL für das Objektefoto aus RxDB Attachments.
 * - null, wenn kein Attachment da ist
 * - cached pro objectId
 */
async function getObjectPhotoDataUrl(objectId, opts = {}) {
  const { bustCache = false } = opts || {};
  if (!rxdb || !rxdb.objects || !objectId) return null;

  if (!bustCache && objectPhotoDataUrlCache.has(objectId)) {
    return objectPhotoDataUrlCache.get(objectId) || null;
  }

  try {
    const doc = await rxdb.objects.findOne({ selector: { id: objectId } }).exec();
    if (!doc) return null;

    // 1) Attachment versuchen
    const att = await _getObjectPhotoAttachment(doc);
    if (att) {
      const blob = await att.getData();
      const dataUrl = await _blobToDataUrl(blob);
      if (dataUrl) {
        objectPhotoDataUrlCache.set(objectId, dataUrl);
        return dataUrl;
      }
    }

    // 2) Fallback: chunks
    const chunkUrl = await getObjectPhotoDataUrlFromChunks(objectId);
    if (chunkUrl) {
      objectPhotoDataUrlCache.set(objectId, chunkUrl);
      return chunkUrl;
    }

    const json = doc.toJSON?.() || {};
    const legacyUrl = normalizeObjectPhotoDataUrl(json.profilePhotoBase64 || '', 'image/png');
    if (legacyUrl) {
      objectPhotoDataUrlCache.set(objectId, legacyUrl);
      return legacyUrl;
    }
  } catch (e) {
    console.warn('[photo] getObjectPhotoDataUrl failed:', e);
  }

  return null;
}

async function inspectObjectPhotoState(objectId) {
  if (!objectId) return { objectId: '', exists: false, error: 'missing-object-id' };
  if (!rxdb || !rxdb.objects) await loadFromRxdb();
  if (!rxdb || !rxdb.objects) return { objectId, exists: false, error: 'rxdb-not-ready' };

  const doc = await rxdb.objects.findOne({ selector: { id: objectId } }).exec();
  if (!doc) return { objectId, exists: false };

  const json = doc.toJSON() || {};
  const chunks = rxdb.object_photo_chunks
    ? await rxdb.object_photo_chunks.find({ selector: { objectId: objectId }, sort: [{ idx: 'asc' }] }).exec()
    : [];
  const rows = (chunks || []).map(d => d.toJSON());
  const photoUrl = await getObjectPhotoDataUrl(objectId, { bustCache: true }).catch(() => null);

  return {
    objectId,
    exists: true,
    name: json.name || '',
    importState: getObjectImportMetaFromAdditional(json.additional)?.state || '',
    chunkCount: rows.length,
    chunkTotal: Number(rows[0]?.total || 0),
    chunkMime: rows[0]?.mime || '',
    hasUiPhotoUrl: !!photoUrl,
    uiPhotoPrefix: photoUrl ? String(photoUrl).slice(0, 32) : '',
    hasLegacyPhotoField: !!(json.profilePhotoBase64 && String(json.profilePhotoBase64).trim())
  };
}

window.__inspectObjectPhotoState = inspectObjectPhotoState;
window.__inspectAllObjectPhotoStates = async function () {
  if (!rxdb || !rxdb.objects) await loadFromRxdb();
  const list = objects || [];
  const results = await Promise.all(list.map(c => inspectObjectPhotoState(c.id).catch(e => ({
    objectId: c.id,
    exists: false,
    error: String(e?.message || e)
  }))));
  console.table(results);
  return results;
};


/** Fallback Avatar (wie vorher) */
function getObjectFallbackAvatarUrl(seed, name = '') {
  const text = (name || seed || '?').toString().trim();
  const parts = text.split(/\s+/).filter(Boolean);
  const initials =
    parts.length >= 2 ? (parts[0][0] + parts[1][0]).toUpperCase()
    : parts.length === 1 ? parts[0].slice(0, 2).toUpperCase()
    : '?';

  const safe = initials.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">` +
    `<rect width="100" height="100" rx="18" fill="#667085"/>` +
    `<text x="50" y="56" text-anchor="middle" font-family="system-ui,Segoe UI,Roboto,Arial" ` +
    `font-size="34" font-weight="800" fill="white">${safe}</text>` +
    `</svg>`;

  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function normalizeMatchPriority(priority) {
  const value = String(priority || '').toLowerCase();
  if (value === 'base' || value === 'performance' || value === 'enthusiasm') return value;
  return 'performance';
}

function normalizeMatchLevel(level, score) {
  const value = String(level || '').toLowerCase();
  if (value === 'full' || value === 'partial' || value === 'none') return value;
  if (typeof score === 'number' && Number.isFinite(score)) {
    if (score >= 0.8) return 'full';
    if (score >= 0.35) return 'partial';
  }
  return 'none';
}

function readItemScore(item) {
  const rawScore = Number(item?.matchScore ?? item?.score ?? item?.confidence ?? item?.match_score);
  if (Number.isFinite(rawScore)) {
    return rawScore > 1 ? Math.max(0, Math.min(1, rawScore / 100)) : Math.max(0, Math.min(1, rawScore));
  }
  const rawKey = Number(item?.matchScoreKey ?? item?.scoreKey ?? item?.score_key);
  if (Number.isFinite(rawKey)) return Math.max(0, Math.min(1, rawKey / 100));
  return null;
}

function normalizeLegacyEvidenceItem(item, index) {
  const score = readItemScore(item);
  const requirementId = String(
    item?.requirementId ||
    item?.requirement_id ||
    item?.criterionId ||
    item?.criterion_id ||
    item?.id ||
    `REQ-${index + 1}`
  );
  const title = String(
    item?.title ||
    item?.requirement ||
    item?.criterion ||
    item?.label ||
    item?.name ||
    `Nachweis ${index + 1}`
  );

  return {
    ...item,
    requirementId,
    title,
    dimension: ['education', 'experience', 'skill', 'language', 'other'].includes(item?.dimension) ? item.dimension : 'other',
    priority: normalizeMatchPriority(item?.priority),
    matchLevel: normalizeMatchLevel(item?.matchLevel || item?.match_level || item?.level, score),
    ...(score == null ? {} : {
      matchScore: score,
      matchScoreKey: Math.round(score * 100)
    }),
    jobSnippet: String(item?.jobSnippet || item?.requirementSnippet || item?.job_snippet || item?.requirement_snippet || item?.requirementText || item?.requirement_text || item?.source || ''),
    cvSnippet: String(item?.cvSnippet || item?.objectSnippet || item?.cv_snippet || item?.object_snippet || item?.objectText || item?.object_text || item?.evidence || item?.text || ''),
    requirementSnippet: String(item?.requirementSnippet || item?.jobSnippet || item?.requirement_snippet || item?.job_snippet || item?.requirementText || item?.requirement_text || item?.source || ''),
    objectSnippet: String(item?.objectSnippet || item?.cvSnippet || item?.object_snippet || item?.cv_snippet || item?.objectText || item?.object_text || item?.evidence || item?.text || ''),
    explanation: String(item?.explanation || item?.reason || item?.rationale || item?.summary || '')
  };
}

function normalizeMatchItemsFromRecord(record) {
  const parsed = record?.parsed_match || record?.parsedMatch || null;
  const data = record?.data && typeof record.data === 'object' ? record.data : null;
  const dataMatch = data?.match && typeof data.match === 'object' ? data.match : null;
  const candidates = [
    record?.items,
    record?.match?.items,
    parsed?.items,
    dataMatch?.items,
    data?.items,
    record?.evidence,
    parsed?.evidence,
    dataMatch?.evidence,
    data?.evidence
  ];
  const rawItems = candidates.find(Array.isArray) || [];
  return rawItems
    .filter(item => item && typeof item === 'object')
    .map(normalizeLegacyEvidenceItem);
}

function hasScoredMatchItems(items) {
  return Array.isArray(items) && items.some(item =>
    readItemScore(item) != null ||
    item?.matchLevel === 'full' ||
    item?.matchLevel === 'partial' ||
    item?.matchLevel === 'none'
  );
}

function scoreFromMatchItems(items) {
  if (!hasScoredMatchItems(items)) return null;
  return computeTotalMatchScoreFromItems(items);
}

async function loadFromRxdb(){
  try {
    const previousDb = rxdb;
    const { getContactsCollection } = await import('./businessOsDataSource.js');
    const contactsCol = await getContactsCollection();
    rxdb = contactsCol.database;

    const [
      sourceDocs,
      requirementDocs,
      objectDocs,
      requirementSourceDocs,
      matchDocs
    ] = await Promise.all([
      rxdb.sources.find().exec(),
      rxdb.requirements.find().exec(),
      rxdb.objects.find().exec(),
      rxdb.requirementSources ? rxdb.requirementSources.find().exec() : [],
      rxdb.matches ? rxdb.matches.find().exec() : []
    ]);

    const sourcesJson  = sourceDocs.map(d => d.toJSON());
    const requirementsJson       = requirementDocs.map(d => d.toJSON());
    const objectsJson = objectDocs.map(d => d.toJSON());
    const requirementSourcesJson   = requirementSourceDocs.map(d => d.toJSON());
    const matchesJson    = matchDocs.map(d => d.toJSON());

    // Track raw doc counts so the UI can distinguish "really empty" from
    // "data is in the database but hasn't been projected into UI shape yet".
    rawDocCounts.sources = sourcesJson.length;
    rawDocCounts.requirements = requirementsJson.length;
    rawDocCounts.objects = objectsJson.length;
    rawDocCounts.matches = matchesJson.length;

    const requirementsBySource = new Map();
    requirementsJson.forEach(j => {
      if (!requirementsBySource.has(j.sourceId)) requirementsBySource.set(j.sourceId, []);
      requirementsBySource.get(j.sourceId).push(j);
    });

    const requirementSourcesByRequirement = new Map();
    requirementSourcesJson.forEach(p => {
      if (!requirementSourcesByRequirement.has(p.requirementId)) requirementSourcesByRequirement.set(p.requirementId, []);
      requirementSourcesByRequirement.get(p.requirementId).push(p);
    });

    function inferLevelFromTitle(title){
      const t = String(title || '').toLowerCase();
      if (t.includes('head of') || t.includes('leiter') || t.includes('lead') || t.includes('manager')) return 'Senior';
      if (t.includes('senior')) return 'Senior';
      if (t.includes('werkstudent') || t.includes('praktikant') || t.includes('intern')) return 'Junior';
      return 'Mid';
    }

    function normalizeParsedFromRequirementSource(requirementDoc){
      const requirementSourceList = requirementSourcesByRequirement.get(requirementDoc.id) || [];
      const requirementSource = requirementSourceList[0] || null;
      const p = requirementSource && requirementSource.parsed ? requirementSource.parsed : null;

      const aboutSource =
        (p && (p.aboutSource || p.about_source)) ||
        requirementDoc.aboutSource || '';

      const aboutRole =
        (p && (p.aboutRole || p.about_role)) ||
        requirementDoc.aboutRole || '';

      const objectRequirements =
        (p && (p.objectRequirements || p.object_requirements)) ||
        requirementDoc.objectRequirements || '';

      let benefitsRaw =
        (p && p.benefits !== undefined ? p.benefits : requirementDoc.benefits);

      let benefits = [];
      if (Array.isArray(benefitsRaw)) {
        benefits = benefitsRaw;
      } else if (typeof benefitsRaw === 'string' && benefitsRaw.trim()) {
        const splitted = benefitsRaw
          .split(/\r?\n|•|–|-/)
          .map(s => s.trim())
          .filter(Boolean);
        benefits = splitted.length ? splitted : [benefitsRaw.trim()];
      }

      const closingNotes =
        (p && (p.closingNotes || p.closing_notes)) ||
        requirementDoc.closingNotes || '';

      const responsibilities = normalizeTextList(
        (p && (p.responsibilities || p.tasks || p.aboutRoleBullets || p.about_role_bullets)) ||
        requirementDoc.responsibilities ||
        []
      );

      const requirements = normalizeTextList(
        (p && (p.requirements || p.objectRequirementsList || p.object_requirements_list)) ||
        requirementDoc.requirements ||
        []
      );

      return {
        aboutSource: aboutSource.trim(),
        aboutRole: aboutRole.trim(),
        objectRequirements: objectRequirements.trim(),
        responsibilities,
        requirements,
        benefits,
        closingNotes: closingNotes.trim(),
        rawText: String(requirementSource?.rawText || requirementDoc.rawText || '').trim()
      };
    }

    // Firmen + Requirements für UI aus RxDB bauen
    const dbSourcesUi = sourcesJson.map(compDoc => {
      const requirementsForComp = requirementsBySource.get(compDoc.id) || [];
      const compLocations = Array.isArray(compDoc.locations) ? compDoc.locations : [];

      const locCountMap = new Map();
      requirementsForComp.forEach(j => {
        const firstLocId = Array.isArray(j.locationIds) && j.locationIds.length
          ? j.locationIds[0]
          : null;
        const loc = firstLocId
          ? compLocations.find(l => l.id === firstLocId)
          : null;
        const city = (loc && loc.city) || 'Remote';
        locCountMap.set(city, (locCountMap.get(city) || 0) + 1);
      });

      const locsUi = Array.from(locCountMap.entries()).map(([city, open]) => ({ city, open }));

      const uiRequirements = requirementsForComp.map(j => {
        const firstLocId = Array.isArray(j.locationIds) && j.locationIds.length
          ? j.locationIds[0]
          : null;
        const loc = firstLocId
          ? compLocations.find(l => l.id === firstLocId)
          : null;
        const city = (loc && loc.city) || 'Remote';
        const level = j.fachlevelClass != null ? (
          j.fachlevelClass === 3 ? 'Senior' :
          j.fachlevelClass === 2 ? 'Mid' :
          'Junior'
        ) : inferLevelFromTitle(j.title);
        const type = j.workModel || 'Vollzeit';

        const normalized = normalizeParsedFromRequirementSource(j);
        const fullDesc = normalized.aboutRole || normalized.aboutSource || '';
        const desc = fullDesc.length > 220 ? fullDesc.slice(0, 220) + '…' : fullDesc;

        const tags = [];
        const kldb = j.kldbKey || j.kldbCode || '';
        if (kldb) tags.push('KldB ' + kldb);

        return {
          id: j.id,
          sourceId: j.sourceId,
          title: j.title,
          internalReferenceId: String(
            j.internalReferenceId ||
            j.internal_reference_id ||
            j.internalReference ||
            j.referenceId ||
            j.reference_id ||
            j.requirementReference ||
            j.requirement_reference ||
            ''
          ).trim(),
          location: city,
          level,
          type,
          desc,
          tags,
          details: normalized
        };
      });

      return {
        id: compDoc.id,
        name: compDoc.name || compDoc.legalName || 'Unbekannte Quelle',
        logoUrl: compDoc.logoUrl || '',
        locations: locsUi.length
          ? locsUi
          : (uiRequirements.length ? [{ city: 'Remote', open: uiRequirements.length }] : []),
        requirements: uiRequirements,
        _hasRelation: !!compDoc.hasRelation,
        active: compDoc.active !== false
      };
    }).filter(c => c.requirements.length > 0);

    sources = dbSourcesUi;

    // Objekte inkl. Object aus RxDB mappen
    // Objekte inkl. Object aus RxDB mappen  (Attachment-Foto NEU)
    const dbObjectsUi = await Promise.all(
      objectsJson.map(async (c) => {
        const fullname = c.name || `${c.firstName || ''} ${c.lastName || ''}`.trim() || 'Unbekannt';
        const tax = c.taxonomy || c.degree || c.highestDegree || 'Objekt';
        const skillsStr = (c.skills || []).slice(0, 8).join(', ');

        const add = Array.isArray(c.additional) ? c.additional : [];
        const importMeta = getObjectImportMetaFromAdditional(add);
        const isPlaceholder = !!(importMeta && importMeta.state !== 'done');
        const educationEntry = add.find(a => a.key === 'object.education');
        const experienceEntry = add.find(a => a.key === 'object.experience');
        const skillsEntry = add.find(a => a.key === 'object.skills');

        const education = Array.isArray(c.education)
          ? c.education
          : (Array.isArray(educationEntry?.value) ? educationEntry.value : []);
        const experience = Array.isArray(c.experience)
          ? c.experience
          : (Array.isArray(experienceEntry?.value) ? experienceEntry.value : []);
        const objectSkills = skillsEntry?.value || {};

	        const languages = Array.isArray(c.languages) ? c.languages : [];
	        const rawText = getImportedObjectRawText(c);

        // Executive-Summaries direkt aus dem Objekte-Dokument
        const execRaw = (c.executiveInfo && typeof c.executiveInfo === 'object') ? c.executiveInfo : {};
        const executiveInfo = {
          fachlicheQualifikation: execRaw.fachlicheQualifikation || '',
          methodenKompetenz:       execRaw.methodenKompetenz       || '',
          leadershipFaehigkeit:    execRaw.leadershipFaehigkeit    || '',
          gehaltswunschUndOrt:     execRaw.gehaltswunschUndOrt     || ''
        };

        // ✅ FOTO: NICHT mehr aus profilePhotoBase64 lesen!
        // Stattdessen Attachment -> dataUrl, sonst Fallback
        const attachmentPhoto =
          await getObjectPhotoDataUrl(c.id).catch(() => null);

        const inlinePhoto =
          normalizeImageSrc(c.photo) ||
          normalizeImageSrc(c.profilePhotoBase64) ||
          normalizeImageSrc(c.object?.photo);
        const photo = attachmentPhoto || inlinePhoto || getObjectFallbackAvatarUrl(c.id || fullname, fullname);


        // createdAt/updatedAt in UI übernehmen
        const createdAt = (typeof c.createdAt === 'string' && c.createdAt.trim()) ? c.createdAt : null;
        const updatedAt = (typeof c.updatedAt === 'string' && c.updatedAt.trim()) ? c.updatedAt : null;

        return {
          id: c.id,
          name: fullname,
          tax: isPlaceholder ? 'Import läuft …' : tax,
          skills: skillsStr,
          skillsSummary: isPlaceholder ? '' : skillsStr,
          photo,
          executiveInfo,
          isPlaceholder,
          importStatus: importMeta ? importMeta.state : '',
          importError: importMeta ? importMeta.error : null,
          _hasRelation: !!c.hasRelation,
          active: isPlaceholder ? false : (c.active !== false),
	          createdAt,
	          updatedAt,
	          rawText,
	          documents: Array.isArray(c.documents) ? c.documents : [],
	          object: {
            meta: {
              birthDate: c.birthDate || null,
              nationality: c.nationality || null,
              highestDegree: c.highestDegree || null,
              degree: c.degree || null,
              languages
            },
            education,
            experience,
            skills: objectSkills
          }
        };
      })
    );


    objects = dbObjectsUi;
    __markObjectsDirty();   // ✅ damit __reloadAllFromRxdbNow objects wirklich rendert

    scheduleDatabaseWarmupPlanning({
      previousDb,
      requirementsJson,
      objectsJson,
      normalizeParsedFromRequirementSource
    });

    const sourceIdByRequirementId = new Map(
      requirementsJson.map((requirement) => [String(requirement.id), requirement.sourceId || ''])
    );

    // Matches aus RxDB auf UI-Struktur mappen
    matches = matchesJson.map(m => {
      const items = normalizeMatchItemsFromRecord(m);
      const score = scoreFromMatchItems(items);
      const idParts = String(m.id || '').split('|');
      const requirementId = m.requirementId || m.requirement_id || m.data?.requirementId || m.data?.requirement_id || (idParts.length >= 3 ? idParts[1] : '');
      const objectId = m.objectId || m.object_id || m.data?.objectId || m.data?.object_id || (idParts.length >= 3 ? idParts[2] : '');
      const sourceId = m.sourceId || m.source_id || m.data?.sourceId || m.data?.source_id || (idParts.length >= 3 ? idParts[0] : '') || sourceIdByRequirementId.get(String(requirementId)) || '';

      const progress = typeof m.progress === 'number'
        ? m.progress
        : DEFAULT_PROGRESS;

      return {
        id: m.id,
        sourceId,
        requirementId,
        objectId,
        active: m.active !== false,
        removed: !!m.removed,
        progress,
        status: m.status || 'prospecting',
        score,
        notes: m.notes || '',
        items
      };
    }).filter(m => !m.removed && m.requirementId && m.objectId);

    // Prozesse-Cache initialisieren
    processes.clear();
    matches.forEach(m => {
      const k = key(m.requirementId, m.objectId);
      const baseNotes = m.notes || '';
      const { statuses, canonical } = deriveStatusesFromNotes(baseNotes, m.status || 'prospecting');

      processes.set(k, {
        progress: m.progress ?? DEFAULT_PROGRESS,
        notes: baseNotes,
        active: m.active,
        statuses,
        status: canonical
      });
    });

    // Demo-Beziehungen simulieren
    if (sources.length && !Object.keys(relations.sources).length) {
      relations.sources[sources[0].id] = true;
    }
    if (objects.length && !Object.keys(relations.objects).length) {
      objects.slice(0, 2).forEach(c => { relations.objects[c.id] = true; });
    }

    reconcilePersistedMatchingSelection();

  } catch (e) {
    console.error('Fehler beim Laden aus RxDB (UI bleibt leer oder minimal):', e);
  }
}

let __databaseWarmupPlanRunId = 0;

function scheduleDatabaseWarmupPlanning({ previousDb, requirementsJson = [], objectsJson = [], normalizeParsedFromRequirementSource } = {}) {
  const runId = ++__databaseWarmupPlanRunId;
  setTimeout(() => {
    void planDatabaseWarmupsInBackground({
      runId,
      previousDb,
      requirementsJson,
      objectsJson,
      normalizeParsedFromRequirementSource
    });
  }, 0);
}

async function planDatabaseWarmupsInBackground() {
  // Basic CTOX matching has no background warmup. Parsing and matching run as explicit CTOX tasks.
}


// ---------------------------
// ✅ RxDB → UI live sync (stabil, Drop-in)
// ---------------------------

let __rxdbLiveSubs = [];
let __reloadTimer = null;
let __reloadInFlight = false;
let __reloadAgainAfterFlight = false;
let __lastReloadAt = 0;

const RXDB_UI_RELOAD_THROTTLE_MS = 250;

// ✅ Objects dirty/versioning
let __objectVersion = 0;
let __lastRenderedObjectVersion = -1;

// ✅ Deferred object render if user is busy
let __objectRenderDeferred = false;
let __userInteracting = false;
let __interactTimer = null;

function __markObjectsDirty() {
  __objectVersion++;
}

function __setUserInteracting() {
  __userInteracting = true;
  clearTimeout(__interactTimer);
  __interactTimer = setTimeout(() => { __userInteracting = false; }, 500);
}

// global listeners once
(function __installInteractionListenersOnce(){
  if (window.__rxdbObjectInteractionListenersInstalled) return;
  window.__rxdbObjectInteractionListenersInstalled = true;

  ['wheel','scroll','keydown','pointerdown','touchstart','input'].forEach(ev => {
    window.addEventListener(ev, __setUserInteracting, { passive: true, capture: true });
  });
})();

function __objectUiIsBusy() {
  const objectSearch = document.getElementById('objectSearch');
  const noteOpen = document.getElementById('noteModal')?.classList.contains('open');
  const objectOpen = document.getElementById('objectPanel')?.classList.contains('open');
  const focusedInObject = objectSearch && document.activeElement === objectSearch;
  return __userInteracting || noteOpen || focusedInObject || objectOpen;
}

// idle flush: zieht deferred object render nach
(function __installDeferredFlushOnce(){
  if (window.__rxdbObjectDeferredFlushInstalled) return;
  window.__rxdbObjectDeferredFlushInstalled = true;

  setInterval(() => {
    if (__objectRenderDeferred && !__objectUiIsBusy()) {
      __objectRenderDeferred = false;

      // render objects now (only if still dirty)
      if (__objectVersion !== __lastRenderedObjectVersion) {
        if (typeof renderObjects === 'function') {
          renderObjects({ reason: 'deferred', fromLiveSync: true });
        }
        __lastRenderedObjectVersion = __objectVersion;
      }
    }
  }, 300);
})();

function __scheduleReloadAllFromRxdb(reason = '', delayMs = 0) {
  if (__reloadTimer) return;

  __reloadTimer = setTimeout(() => {
    __reloadTimer = null;
    __reloadAllFromRxdbNow(reason);
  }, Math.max(0, delayMs));
}

async function __reloadAllFromRxdbNow(reason = '') {
  const now = Date.now();
  const dt = now - __lastReloadAt;

  if (dt < RXDB_UI_RELOAD_THROTTLE_MS) {
    const wait = RXDB_UI_RELOAD_THROTTLE_MS - dt;
    __scheduleReloadAllFromRxdb(`throttle:${reason}`, wait);
    return;
  }

  if (__reloadInFlight) {
    __reloadAgainAfterFlight = true;
    return;
  }

  __reloadInFlight = true;
  __lastReloadAt = now;

  try {
    await loadFromRxdb();

    const shouldRenderObjects =
      (__objectVersion !== __lastRenderedObjectVersion);

    requestAnimationFrame(() => {
      // ✅ links/matrix kann immer stabil neu
      renderSources();
      renderRequirements();
      renderMap();

      // ✅ rechts: nur wenn objects dirty UND nicht busy
      if (shouldRenderObjects) {
        if (__objectUiIsBusy()) {
          __objectRenderDeferred = true;
        } else {
          if (typeof renderObjects === 'function') {
            renderObjects({ reason: `rxdb:${reason}`, fromLiveSync: true });
          }
          __lastRenderedObjectVersion = __objectVersion;
        }
      }
    });

    if (__reloadAgainAfterFlight) {
      __reloadAgainAfterFlight = false;
      __scheduleReloadAllFromRxdb(`again:${reason}`, RXDB_UI_RELOAD_THROTTLE_MS);
    }
  } catch (e) {
    console.error('[rxdb-live] reload failed:', reason, e);
    syncFeedback.reportSyncFailure('Sync fehlgeschlagen: Änderungen konnten nicht geladen werden.');
  } finally {
    __reloadInFlight = false;
  }
}

function __unsubscribeAllRxdbLiveSubs() {
  try {
    (__rxdbLiveSubs || []).forEach(s => {
      try { s && typeof s.unsubscribe === 'function' && s.unsubscribe(); } catch (_) {}
    });
  } finally {
    __rxdbLiveSubs = [];
  }
}

// optional: Object Photo Cache invalidieren
function __bustObjectPhotoCacheFromChangeEvent(evt) {
  const id =
    evt?.documentData?.objectId ||
    evt?.documentData?.docData?.objectId ||
    evt?.documentData?.id ||
    evt?.documentData?.primary ||
    evt?.doc?.objectId ||
    evt?.doc?.id ||
    evt?.doc?._data?.id ||
    evt?.id ||
    null;

  if (id && typeof objectPhotoDataUrlCache?.delete === 'function') {
    objectPhotoDataUrlCache.delete(id);
  }
}

async function loadObjectUiFromRxdbById(objectId){
  if (!rxdb || !rxdb.objects || !objectId) return null;

  const doc = await rxdb.objects.findOne({ selector: { id: objectId } }).exec();
  if (!doc) return null;

  const c = doc.toJSON() || {};
  const fullname = c.name || `${c.firstName || ''} ${c.lastName || ''}`.trim() || 'Unbekannt';
  const tax = c.taxonomy || c.degree || c.highestDegree || 'Objekt';
  const skillsStr = (c.skills || []).slice(0, 8).join(', ');

  const add = Array.isArray(c.additional) ? c.additional : [];
  const importMeta = getObjectImportMetaFromAdditional(add);
  const isPlaceholder = !!(importMeta && importMeta.state !== 'done');
  const educationEntry = add.find(a => a.key === 'object.education');
  const experienceEntry = add.find(a => a.key === 'object.experience');
  const skillsEntry = add.find(a => a.key === 'object.skills');

  const education = Array.isArray(c.education)
    ? c.education
    : (Array.isArray(educationEntry?.value) ? educationEntry.value : []);
  const experience = Array.isArray(c.experience)
    ? c.experience
    : (Array.isArray(experienceEntry?.value) ? experienceEntry.value : []);
  const objectSkills = skillsEntry?.value || {};

	  const languages = Array.isArray(c.languages) ? c.languages : [];
	  const rawText = getImportedObjectRawText(c);

  const execRaw = (c.executiveInfo && typeof c.executiveInfo === 'object') ? c.executiveInfo : {};
  const executiveInfo = {
    fachlicheQualifikation: execRaw.fachlicheQualifikation || '',
    methodenKompetenz:       execRaw.methodenKompetenz       || '',
    leadershipFaehigkeit:    execRaw.leadershipFaehigkeit    || '',
    gehaltswunschUndOrt:     execRaw.gehaltswunschUndOrt     || ''
  };

  // Foto: Attachment -> dataUrl, sonst Fallback
  const attachmentPhoto = await getObjectPhotoDataUrl(c.id, { bustCache: false }).catch(()=> null);
  const inlinePhoto =
    normalizeImageSrc(c.photo) ||
    normalizeImageSrc(c.profilePhotoBase64) ||
    normalizeImageSrc(c.object?.photo);
  const photo = attachmentPhoto || inlinePhoto || getObjectFallbackAvatarUrl(c.id || fullname, fullname);

  const createdAt = (typeof c.createdAt === 'string' && c.createdAt.trim()) ? c.createdAt : null;
  const updatedAt = (typeof c.updatedAt === 'string' && c.updatedAt.trim()) ? c.updatedAt : null;

  return {
    id: c.id,
    name: fullname,
    tax: isPlaceholder ? 'Import läuft …' : tax,
    skills: skillsStr,
    skillsSummary: isPlaceholder ? '' : skillsStr,
    photo,
    executiveInfo,
    isPlaceholder,
    importStatus: importMeta ? importMeta.state : '',
    importError: importMeta ? importMeta.error : null,
    _hasRelation: !!c.hasRelation,
    active: isPlaceholder ? false : (c.active !== false),
	    createdAt,
	    updatedAt,
	    rawText,
	    documents: Array.isArray(c.documents) ? c.documents : [],
	    object: {
      meta: {
        birthDate: c.birthDate || null,
        nationality: c.nationality || null,
        highestDegree: c.highestDegree || null,
        degree: c.degree || null,
        languages
      },
      education,
      experience,
      skills: objectSkills
    }
  };
}

function resetObjectUiDom() {
  const list = document.getElementById('objectList');

  objectDomById.clear();
  objectLastPhotoUrl.clear();

  if (!list) return;

  list.querySelectorAll('[data-role="object-card"]').forEach(card => {
    const id = card.dataset.objectId;
    if (!id) return;
    objectDomById.set(id, card);

    const img = card.querySelector('[data-role="avatar-img"]');
    if (img?.src) objectLastPhotoUrl.set(id, img.src);
  });

  if (!list.dataset.incReady) list.dataset.incReady = '1';
}



async function setupRxdbLiveUiSync() {
  __unsubscribeAllRxdbLiveSubs();

  // ✅ WICHTIG: Kein Hard-Reset der Objekte-DOM mehr
  // (das war der Grund, warum rechts "alles komplett anders" wurde)
  resetObjectUiDom();

  if (!rxdb) await loadFromRxdb();
  if (!rxdb) {
    console.warn('[rxdb-live] rxdb not available');
    return;
  }

  // ✅ NOISY collections: ändern oft, UI muss nicht full reloaden
  const NOISY = new Set();

  const subscribeTo = (col, name) => {
    const obs = col && col.$;
    if (!obs || typeof obs.subscribe !== 'function') return;

    const sub = obs.subscribe((evt) => {
      try {
        if (name === 'objects') {
          syncFeedback.reportDataChange({ collectionName: name });

          // ✅ Cache bust (Foto)
          __bustObjectPhotoCacheFromChangeEvent(evt);

          // ✅ Object-ID robust extrahieren
          const objectId =
            evt?.documentData?.id ||
            evt?.documentData?.primary ||
            evt?.doc?.id ||
            evt?.doc?._data?.id ||
            evt?.id ||
            null;

          if (!objectId) {
            __markObjectsDirty();
            __scheduleReloadAllFromRxdb(`change:${name}`);
            return;
          }

          // ✅ Single-object patch (ohne Full reload)
          (async () => {
            try {
              const ui = await loadObjectUiFromRxdbById(objectId);

              if (!ui) {
                // Objekt gelöscht -> aus arrays entfernen + DOM entfernen
                objects = (objects || []).filter(x => x.id !== objectId);
                const node = objectDomById.get(objectId);
                if (node) node.remove();
                objectDomById.delete(objectId);
                objectLastPhotoUrl.delete(objectId);
                return;
              }

              // in-memory upsert
              const idx = (objects || []).findIndex(x => x.id === objectId);
              if (idx >= 0) objects[idx] = ui;
              else objects.unshift(ui);

              if (!ui.isPlaceholder) {
                void scheduleObjectBackgroundWarmupIfNeeded(objectId);
              }

              // ✅ nur diese Card patchen (wenn vorhanden)
              if (objectDomById.has(objectId)) {
                patchObjectCard(objectId, { ...ui, score: null });
              }

              // ✅ Reconcile nur wenn UI nicht busy
              if (typeof __objectUiIsBusy !== 'function' || !__objectUiIsBusy()) {
                renderObjects({ reason: 'rxdb:object-single', fromLiveSync: true });
              } else {
                __objectRenderDeferred = true;
                __markObjectsDirty();
              }
            } catch (e) {
              console.warn('[rxdb-live] single object patch failed -> fallback reload', e);
              __markObjectsDirty();
              __scheduleReloadAllFromRxdb(`fallback:objects`);
            }
          })();

          return; // ✅ wichtig: KEIN __scheduleReloadAllFromRxdb für objects
        }

        if (name === 'object_photo_chunks') {
          __bustObjectPhotoCacheFromChangeEvent(evt);
          __markObjectsDirty();
          __scheduleReloadAllFromRxdb(`change:${name}`);
          return;
        }

        // ✅ noisy collections nicht reloaden
        if (NOISY.has(name)) return;
      } catch (_) {}

      syncFeedback.reportDataChange({ collectionName: name });
      __scheduleReloadAllFromRxdb(`change:${name}`);
    });

    __rxdbLiveSubs.push(sub);
  };

  // ChangeStreams
  subscribeTo(rxdb.sources,  'sources');
  subscribeTo(rxdb.requirements,       'requirements');
  subscribeTo(rxdb.objects, 'objects');
  subscribeTo(rxdb.object_photo_chunks, 'object_photo_chunks');
  subscribeTo(rxdb.requirementSources,   'requirementSources');
  subscribeTo(rxdb.matches,    'matches');

  // initial
  __scheduleReloadAllFromRxdb('init-live-sync');

  console.log('[rxdb-live] UI sync enabled');
}


// teardown bleibt gleich
function teardownRxdbLiveUiSync() {
  __unsubscribeAllRxdbLiveSubs();
  console.log('[rxdb-live] UI sync disabled');
}

window.__scheduleReloadAllFromRxdb = __scheduleReloadAllFromRxdb;
window.setupRxdbLiveUiSync = setupRxdbLiveUiSync;
window.teardownRxdbLiveUiSync = teardownRxdbLiveUiSync;

// Requirement import: Die UI triggert nur einen CTOX-Task.

/**
 * Importiert eine Anforderungnanzeige:
 *  - HTML/URL werden strukturiert an den CTOX Harness übergeben.
 *  - Parser-Ergebnisse landen danach in den CTOX Collections.
 *
 * @param {string} html  Vollständiges HTML des Tabs
 * @param {string} [url] Optionale kanonische URL (Tab-URL / Requirement-URL)
 * @returns {Promise<{ source: any, requirement: any, record: any }>}
 */
export async function importRequirementHtmlIntoRxdb(html, url) {
  if (typeof html !== 'string' || !html.trim()) {
    throw new Error("Parameter 'html' muss ein nicht-leerer String sein.");
  }
  const queued = await queueRequirementParseTask({ html, url });
  return {
    source: queued.source || null,
    requirement: queued.requirement || null,
    record: queued.record || {},
    queued: true,
    commandId: queued.command_id || queued.commandId || queued.id || ''
  };
}



async function setSourceActive(sourceId, active){
  const comp = sources.find(c => c.id === sourceId);
  if (!comp) return;
  comp.active = !!active;

  try {
    if (rxdb && rxdb.sources){
      const doc = await rxdb.sources
        .findOne({ selector: { id: sourceId } })
        .exec();

      if (doc){
        const now = new Date().toISOString();
        const data = {
          active: !!active,
          activeKey: active ? 1 : 0,
          updatedAt: now
        };

        if (typeof doc.atomicPatch === 'function'){
          await doc.atomicPatch(data);
        } else if (typeof doc.atomicUpdate === 'function'){
          await doc.atomicUpdate(prev => ({ ...prev, ...data }));
        } else if (typeof doc.incrementalModify === 'function'){
          await doc.incrementalModify(prev => Object.assign(prev, data));
        } else if (typeof doc.update === 'function'){
          await doc.update({ $set: data });
        }
      }
    }

    // WICHTIG: Matches NICHT mehr anfassen – sie bleiben vorhanden
    // und behalten ihren eigenen active-Status.

  } catch (e){
    console.error('Fehler beim Aktualisieren des Quellensstatus', e);
  }

  renderSources();
  renderRequirements();
  renderObjects();
  renderMap();
}


async function setObjectActive(objectId, active){
  const c = objects.find(x => x.id === objectId);
  if (!c) return;
  c.active = !!active;

  try {
    if (rxdb && rxdb.objects){
      const doc = await rxdb.objects
        .findOne({ selector: { id: objectId } })
        .exec();

      if (doc){
        const now = new Date().toISOString();
        const data = {
          active: !!active,
          activeKey: active ? 1 : 0,
          updatedAt: now
        };

        if (typeof doc.atomicPatch === 'function'){
          await doc.atomicPatch(data);
        } else if (typeof doc.atomicUpdate === 'function'){
          await doc.atomicUpdate(prev => ({ ...prev, ...data }));
        } else if (typeof doc.incrementalModify === 'function'){
          await doc.incrementalModify(prev => Object.assign(prev, data));
        } else if (typeof doc.update === 'function'){
          await doc.update({ $set: data });
        }
      }
    }
  } catch (e){
    console.error('Fehler beim Aktualisieren des Objektestatus', e);
  }

  renderObjects();
  renderRequirements();
  renderMap();
}

async function setSourceRelation(sourceId, hasRel){
  relations.sources[sourceId] = !!hasRel;

  const comp = sources.find(c => c.id === sourceId);
  if (comp){
    comp._hasRelation = !!hasRel;
  }

  try {
    if (rxdb && rxdb.sources){
      const doc = await rxdb.sources
        .findOne({ selector: { id: sourceId } })
        .exec();

      if (doc){
        const now = new Date().toISOString();
        const data = {
          hasRelation: !!hasRel,
          hasRelationKey: hasRel ? 1 : 0,
          updatedAt: now
        };

        if (typeof doc.atomicPatch === 'function'){
          await doc.atomicPatch(data);
        } else if (typeof doc.atomicUpdate === 'function'){
          await doc.atomicUpdate(prev => ({ ...prev, ...data }));
        } else if (typeof doc.incrementalModify === 'function'){
          await doc.incrementalModify(prev => Object.assign(prev, data));
        } else if (typeof doc.update === 'function'){
          await doc.update({ $set: data });
        }
      }
    }
  } catch (e){
    console.error('Fehler beim Aktualisieren des Kontaktstatus (Source)', e);
  }

  renderSources();
  renderRequirements();
  renderObjects();
  renderMap();
}

async function setObjectRelation(objectId, hasRel){
  relations.objects[objectId] = !!hasRel;

  const c = objects.find(x => x.id === objectId);
  if (c){
    c._hasRelation = !!hasRel;
  }

  try {
    if (rxdb && rxdb.objects){
      const doc = await rxdb.objects
        .findOne({ selector: { id: objectId } })
        .exec();

      if (doc){
        const now = new Date().toISOString();
        const data = {
          hasRelation: !!hasRel,
          hasRelationKey: hasRel ? 1 : 0,
          updatedAt: now
        };

        if (typeof doc.atomicPatch === 'function'){
          await doc.atomicPatch(data);
        } else if (typeof doc.atomicUpdate === 'function'){
          await doc.atomicUpdate(prev => ({ ...prev, ...data }));
        } else if (typeof doc.incrementalModify === 'function'){
          await doc.incrementalModify(prev => Object.assign(prev, data));
        } else if (typeof doc.update === 'function'){
          await doc.update({ $set: data });
        }
      }
    }
  } catch (e){
    console.error('Fehler beim Aktualisieren des Kontaktstatus (Object)', e);
  }

  renderObjects();
  renderRequirements();
  renderMap();
}



// ---------------------------
// Matching-Scores
// ---------------------------

function preMatchScore(requirementId, objectId) {
  const existingMatch = matches.find(m =>
    m.requirementId === requirementId &&
    m.objectId === objectId &&
    !m.removed
  );
  return existingMatch ? scoreFromMatchItems(existingMatch.items) : null;
}


// Objekte mit vorhandenen Match-Scores
// nimmt Top mit Score >= 90 oder Top 1, falls keiner >= 90
function getPromisingForRequirement(requirementId) {
  const scored = objects
    .map(c => {
      const p = preMatchScore(requirementId, c.id);
      return (typeof p === 'number' && !Number.isNaN(p))
        ? { cid: c.id, p }
        : null;
    })
    .filter(Boolean)
    .sort((a, b) => b.p - a.p);

  if (!scored.length) return [];

  const top = scored.filter(s => s.p >= 90).slice(0, 3);
  return (top.length ? top : scored.slice(0, 1)).map(s => s.cid);
}



/* --------- Zustand --------- */
let activeSource = matchingViewState.activeSource || null;
// value: {progress:number, notes:string, active:boolean}
let activeRequirementForScoring = matchingViewState.activeRequirementForScoring || null;
let selectedObject = matchingViewState.selectedObject || null;
let matrixSelectedObjectId = matchingViewState.matrixSelectedObjectId || null;
const processes = new Map();
const removedByRequirement = new Map(); // requirementId -> Set(objectId)
const seededRequirements = new Set();

// Requirements, bei denen gerade ein Bulk-Matching läuft
const bulkMatchingRequirements = new Set();
const bulkMatchingObjects = new Set(); // objectId, bei denen gerade ein Requirement-Matching läuft


/* --------- Helpers --------- */
const $ = (sel, root=document)=> root.querySelector(sel);
const el = (tag, cls)=>{ const e=document.createElement(tag); if(cls) e.className=cls; return e; };
const gradeBucket = (p)=> p>=80? 'high' : p>=50? 'mid' : 'low';
const getObject = (id)=> objects.find(c=>c.id===id);
const key = (requirementId, objectId)=> `${requirementId}:${objectId}`;
const DEFAULT_PROGRESS = 10;   // erste „aktive“ Stufe
const INITIAL_PROGRESS = 0;    // Startzustand
function ensureRemovedSet(requirementId){ if(!removedByRequirement.has(requirementId)) removedByRequirement.set(requirementId,new Set()); return removedByRequirement.get(requirementId); }

function clampScorePercent(value, fallback = 70) {
  const n = Number(value);
  if (!Number.isFinite(n)) return fallback;
  return Math.max(0, Math.min(100, Math.round(n)));
}

function normalizeBulkMatchFilterSettings(raw) {
  const src = raw && typeof raw === 'object' ? raw : {};
  return {
    enabled: !!src.enabled,
    minScore: clampScorePercent(src.minScore, 70)
  };
}

function getBulkMatchFilterSettings() {
  return normalizeBulkMatchFilterSettings(matchingViewState.bulkMatchFilter);
}

function setBulkMatchFilterSettings(next) {
  persistMatchingViewState({
    bulkMatchFilter: normalizeBulkMatchFilterSettings({
      ...getBulkMatchFilterSettings(),
      ...(next || {})
    })
  });
}

function shortlistEntryScorePercent(entry) {
  const raw = Number(entry?.score);
  if (!Number.isFinite(raw)) return 0;
  return raw <= 1 ? Math.round(raw * 100) : clampScorePercent(raw, 0);
}

function filterShortlistByBulkMatchFilter(shortlist) {
  const settings = getBulkMatchFilterSettings();
  const list = Array.isArray(shortlist) ? shortlist : [];
  if (!settings.enabled) return list;
  return list.filter((entry) => shortlistEntryScorePercent(entry) >= settings.minScore);
}

function normalizeMatchFilterSettings(raw) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const statuses = Array.isArray(src.statuses)
    ? src.statuses.map((st) => normalizeStatusToken(st)).filter((st) => ALL_MATCH_STATUSES.includes(st))
    : [];
  return {
    enabled: !!src.enabled,
    minScoreEnabled: src.minScoreEnabled !== false,
    minScore: clampScorePercent(src.minScore, 70),
    onlyActiveObjects: !!src.onlyActiveObjects,
    onlyActiveProcesses: !!src.onlyActiveProcesses,
    statuses
  };
}

function getMatchFilterSettings(requirementId) {
  const all = matchingViewState.matchFilters && typeof matchingViewState.matchFilters === 'object'
    ? matchingViewState.matchFilters
    : {};
  return normalizeMatchFilterSettings(all[String(requirementId || '')]);
}

function setMatchFilterSettings(requirementId, next) {
  const keyId = String(requirementId || '').trim();
  if (!keyId) return;
  const current = matchingViewState.matchFilters && typeof matchingViewState.matchFilters === 'object'
    ? matchingViewState.matchFilters
    : {};
  persistMatchingViewState({
    matchFilters: {
      ...current,
      [keyId]: normalizeMatchFilterSettings({
        ...getMatchFilterSettings(keyId),
        ...(next || {})
      })
    }
  });
}

function matchScorePercent(match) {
  const raw = Number(match?.score);
  if (!Number.isFinite(raw)) return null;
  return raw <= 1 ? Math.round(raw * 100) : clampScorePercent(raw, 0);
}

function matchPassesRequirementFilter(match, object, settings = getMatchFilterSettings(match?.requirementId)) {
  if (!settings.enabled) return true;

  const score = matchScorePercent(match);
  if (settings.minScoreEnabled && (score == null || score < settings.minScore)) return false;
  if (settings.onlyActiveObjects && (!object || !isObjectActive(object.id))) return false;

  const proc = processes.get(key(match.requirementId, match.objectId));
  const processActive = proc ? proc.active !== false : match.active !== false;
  if (settings.onlyActiveProcesses && !processActive) return false;

  if (settings.statuses.length) {
    const activeStatuses = new Set(
      (Array.isArray(match.statuses) ? match.statuses : [])
        .map((st) => normalizeStatusToken(st))
        .filter(Boolean)
    );
    if (match.status) activeStatuses.add(normalizeStatusToken(match.status));
    if (!settings.statuses.some((st) => activeStatuses.has(st))) return false;
  }

  return true;
}

const SEARCH_SKIP_KEYS = new Set([
  'photo',
  'profilephoto',
  'profilephotobase64',
  'avatar',
  'logo',
  'logourl',
  'image',
  'imageurl',
  'base64',
  'dataurl',
  'blob'
]);

function normalizeSearchText(value) {
  return String(value ?? '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase();
}

function collectSearchText(value, depth = 0, seen = new Set()) {
  if (value == null || depth > 5) return '';
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (value instanceof Date) return value.toISOString();
  if (typeof value !== 'object') return '';
  if (seen.has(value)) return '';
  seen.add(value);

  if (Array.isArray(value)) {
    return value.map((item) => collectSearchText(item, depth + 1, seen)).filter(Boolean).join(' ');
  }

  return Object.entries(value)
    .filter(([key]) => !SEARCH_SKIP_KEYS.has(String(key || '').replace(/[^a-z0-9]/gi, '').toLowerCase()))
    .map(([, item]) => collectSearchText(item, depth + 1, seen))
    .filter(Boolean)
    .join(' ');
}

function matchesFullTextSearch(value, query) {
  const terms = normalizeSearchText(query).split(/\s+/).filter(Boolean);
  if (!terms.length) return true;
  const haystack = normalizeSearchText(collectSearchText(value));
  return terms.every((term) => haystack.includes(term));
}

function buildRequirementSearchPayload(requirement, source) {
  return {
    requirement,
    source: {
      name: source?.name,
      website: source?.website,
      industry: source?.industry,
      locations: source?.locations
    }
  };
}

function buildObjectSearchPayload(object) {
  return object || {};
}

function persistMatchingViewState(patch = {}) {
  matchingViewState = patchViewState(
    MATCHING_VIEW_STATE_KEY,
    patch,
    MATCHING_VIEW_STATE_DEFAULTS
  );
  return matchingViewState;
}

function snapshotMatchingControlState() {
  const sourceSearchEl = document.getElementById('sourceSearch');
  const requirementSearchEl = document.getElementById('requirementSearch');
  const requirementFilterEl = document.getElementById('requirementFilter');
  const objectSearchEl = document.getElementById('objectSearch');
  const objectSortEl = document.getElementById('objectSort');
  const mapWrapEl = document.getElementById('mapWrap');
  const activeTab = mapWrapEl && mapWrapEl.classList.contains('active') ? 'matrix' : 'list';

  return {
    sourceSearch: sourceSearchEl ? String(sourceSearchEl.value || '') : String(matchingViewState.sourceSearch || ''),
    requirementSearch: requirementSearchEl ? String(requirementSearchEl.value || '') : String(matchingViewState.requirementSearch || ''),
    requirementFilter: requirementFilterEl ? String(requirementFilterEl.value || 'all') : String(matchingViewState.requirementFilter || 'all'),
    objectSearch: objectSearchEl ? String(objectSearchEl.value || '') : String(matchingViewState.objectSearch || ''),
    objectSort: objectSortEl ? String(objectSortEl.value || 'best') : String(matchingViewState.objectSort || 'best'),
    activeTab,
  };
}

function persistMatchingRuntimeState(extraPatch = {}) {
  persistMatchingViewState({
    activeSource: activeSource || null,
    activeRequirementForScoring: activeRequirementForScoring || null,
    selectedObject: selectedObject || null,
    matrixSelectedObjectId: matrixSelectedObjectId || null,
    ...snapshotMatchingControlState(),
    ...extraPatch
  });
}

function applyPersistedMatchingControls() {
  const sourceSearchEl = document.getElementById('sourceSearch');
  const requirementSearchEl = document.getElementById('requirementSearch');
  const requirementFilterEl = document.getElementById('requirementFilter');
  const objectSearchEl = document.getElementById('objectSearch');
  const objectSortEl = document.getElementById('objectSort');

  if (sourceSearchEl) {
    sourceSearchEl.value = typeof matchingViewState.sourceSearch === 'string'
      ? matchingViewState.sourceSearch
      : '';
  }

  if (requirementSearchEl) {
    requirementSearchEl.value = typeof matchingViewState.requirementSearch === 'string'
      ? matchingViewState.requirementSearch
      : '';
  }

  if (requirementFilterEl) {
    const savedFilter = typeof matchingViewState.requirementFilter === 'string'
      ? matchingViewState.requirementFilter
      : 'all';
    requirementFilterEl.dataset.persistedValue = savedFilter || 'all';
  }

  if (objectSearchEl) {
    objectSearchEl.value = typeof matchingViewState.objectSearch === 'string'
      ? matchingViewState.objectSearch
      : '';
  }

  if (objectSortEl) {
    const savedSort = String(matchingViewState.objectSort || 'best');
    const hasSavedSort = Array.from(objectSortEl.options).some(opt => opt.value === savedSort);
    objectSortEl.value = hasSavedSort ? savedSort : 'best';
  }
}

const MATCHING_COL_MIN = Object.freeze({ left: 260, center: 320, right: 260 });
const MATCHING_COL_SIDE_MAX = 560;

function _clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function _sanitizeColumnLayoutRatios(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const left = Number(raw.left);
  const center = Number(raw.center);
  const right = Number(raw.right);
  if (![left, center, right].every(Number.isFinite)) return null;
  if (left <= 0 || center <= 0 || right <= 0) return null;

  const sum = left + center + right;
  if (sum <= 0) return null;

  return {
    left: left / sum,
    center: center / sum,
    right: right / sum
  };
}

function _readGridTrackPixels(appEl) {
  if (!appEl) return null;
  const tracks = String(getComputedStyle(appEl).gridTemplateColumns || '')
    .split(/\s+/)
    .map(part => Number.parseFloat(part))
    .filter(n => Number.isFinite(n) && n > 0);
  if (tracks.length < 3) return null;
  return {
    left: tracks[0],
    center: tracks[1],
    right: tracks[2]
  };
}

function _getAppGridMetrics(appEl) {
  if (!appEl) return null;
  const cs = getComputedStyle(appEl);
  const gap = Number.parseFloat(cs.columnGap || cs.gap || '0') || 0;
  const padLeft = Number.parseFloat(cs.paddingLeft || '0') || 0;
  const padRight = Number.parseFloat(cs.paddingRight || '0') || 0;
  const contentWidth = Math.max(0, appEl.clientWidth - padLeft - padRight);
  const trackTotal = Math.max(0, contentWidth - (gap * 2));

  return { gap, padLeft, contentWidth, trackTotal };
}

function _clampMatchingColumns(widths, trackTotal) {
  if (!widths || !Number.isFinite(trackTotal) || trackTotal <= 0) return null;

  const minLeft = MATCHING_COL_MIN.left;
  const minCenter = MATCHING_COL_MIN.center;
  const minRight = MATCHING_COL_MIN.right;

  const maxLeft = Math.max(
    minLeft,
    Math.min(MATCHING_COL_SIDE_MAX, trackTotal - minCenter - minRight)
  );
  const maxRight = Math.max(
    minRight,
    Math.min(MATCHING_COL_SIDE_MAX, trackTotal - minCenter - minLeft)
  );

  let left = _clampNumber(Number(widths.left) || minLeft, minLeft, maxLeft);
  let right = _clampNumber(Number(widths.right) || minRight, minRight, maxRight);
  let center = trackTotal - left - right;

  if (center < minCenter) {
    let shortage = minCenter - center;
    const reduceLeft = Math.min(shortage, left - minLeft);
    left -= reduceLeft;
    shortage -= reduceLeft;

    if (shortage > 0) {
      const reduceRight = Math.min(shortage, right - minRight);
      right -= reduceRight;
      shortage -= reduceRight;
    }

    if (shortage > 0) {
      left = minLeft;
      right = minRight;
    }
    center = trackTotal - left - right;
  }

  if (center < minCenter) return null;

  return {
    left: Math.round(left),
    center: Math.round(center),
    right: Math.round(right)
  };
}

function _columnPixelsToRatios(widths) {
  if (!widths) return null;
  const left = Number(widths.left) || 0;
  const center = Number(widths.center) || 0;
  const right = Number(widths.right) || 0;
  const sum = left + center + right;
  if (sum <= 0) return null;
  return {
    left: Number((left / sum).toFixed(6)),
    center: Number((center / sum).toFixed(6)),
    right: Number((right / sum).toFixed(6))
  };
}

function _columnRatiosToPixels(ratios, trackTotal) {
  const safe = _sanitizeColumnLayoutRatios(ratios);
  if (!safe) return null;

  return _clampMatchingColumns({
    left: safe.left * trackTotal,
    center: safe.center * trackTotal,
    right: safe.right * trackTotal
  }, trackTotal);
}

function _isDesktopThreeColumnLayout(appEl) {
  if (!appEl) return false;
  return !!_readGridTrackPixels(appEl);
}

function setupMatchingColumnResizing() {
  const appEl = getMatchingModuleHost().querySelector('.app');
  if (!appEl) return;

  const leftHandle = document.createElement('div');
  leftHandle.className = 'col-resizer col-resizer-left';
  leftHandle.dataset.resizer = 'left';
  leftHandle.setAttribute('role', 'separator');
  leftHandle.setAttribute('aria-orientation', 'vertical');
  leftHandle.setAttribute('aria-label', 'Spaltenbreite links/mittig anpassen');
  leftHandle.dataset.resizer = 'left';

  const rightHandle = document.createElement('div');
  rightHandle.className = 'col-resizer col-resizer-right';
  rightHandle.dataset.resizer = 'right';
  rightHandle.setAttribute('role', 'separator');
  rightHandle.setAttribute('aria-orientation', 'vertical');
  rightHandle.setAttribute('aria-label', 'Spaltenbreite mittig/rechts anpassen');
  rightHandle.dataset.resizer = 'right';

  appEl.appendChild(leftHandle);
  appEl.appendChild(rightHandle);

  const resizerL = new CtoxResizer({
    resizerEl: leftHandle,
    containerEl: appEl,
    cssVar: '--matching-left-width',
    side: 'left',
    minWidth: 260,
    maxWidth: 560,
    onResize: (width) => localStorage.setItem('ctox.matching.layout.leftWidth', width)
  });

  const resizerR = new CtoxResizer({
    resizerEl: rightHandle,
    containerEl: appEl,
    cssVar: '--matching-right-width',
    side: 'right',
    minWidth: 260,
    maxWidth: 560,
    onResize: (width) => localStorage.setItem('ctox.matching.layout.rightWidth', width)
  });

  const leftWidth = localStorage.getItem('ctox.matching.layout.leftWidth') || '280';
  const rightWidth = localStorage.getItem('ctox.matching.layout.rightWidth') || '280';
  appEl.style.setProperty('--matching-left-width', `${leftWidth}px`);
  appEl.style.setProperty('--matching-right-width', `${rightWidth}px`);

  leftHandle.style.display = 'flex';
  rightHandle.style.display = 'flex';
}

function reconcilePersistedMatchingSelection() {
  const sourceIds = new Set((sources || []).map(c => c.id));
  if (activeSource && !sourceIds.has(activeSource)) {
    activeSource = null;
  }

  if (selectedObject) {
    const hasObject = (objects || []).some(c => c.id === selectedObject);
    if (!hasObject) selectedObject = null;
  }

  if (matrixSelectedObjectId) {
    const hasMatrixObject = (objects || []).some(c => c.id === matrixSelectedObjectId);
    if (!hasMatrixObject) {
      matrixSelectedObjectId = null;
    } else if (!selectedObject) {
      selectedObject = matrixSelectedObjectId;
      matrixSelectedObjectId = null;
    } else {
      matrixSelectedObjectId = null;
    }
  }

  if (activeRequirementForScoring) {
    const hasRequirement = (sources || []).some(c =>
      (c.requirements || []).some(j => j.id === activeRequirementForScoring)
    );
    if (!hasRequirement) activeRequirementForScoring = null;
  }
}

const hasSourceRel = (cid)=>{
  if (relations.sources && Object.prototype.hasOwnProperty.call(relations.sources, cid)) {
    return !!relations.sources[cid];
  }
  const comp = sources.find(c => c.id === cid);
  return !!(comp && comp._hasRelation);
};
const hasObjectRel = (cid)=>{
  if (relations.objects && Object.prototype.hasOwnProperty.call(relations.objects, cid)) {
    return !!relations.objects[cid];
  }
  const c = objects.find(x => x.id === cid);
  return !!(c && c._hasRelation);
};

const isSourceActive = (cid)=>{
  const comp = sources.find(c => c.id === cid);
  return comp ? (comp.active !== false) : true;
};

const isObjectActive = (cid)=>{
  const c = objects.find(x => x.id === cid);
  return c ? (c.active !== false) : true;
};


function formatDate(dateStr){
  if (!dateStr) return '';
  const d = new Date(dateStr);
  if (Number.isNaN(d.getTime())) return dateStr;
  return d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' });
}

function formatDateRange(start, end){
  const s = formatDate(start);
  const e = end ? formatDate(end) : '';
  if (!s && !e) return '';
  if (s && !e) return s;
  if (!s && e) return e;
  return `${s} – ${e || 'heute'}`;
}

function getImportedObjectRawText(record = {}) {
  const documents = [
    ...(Array.isArray(record.documents) ? record.documents : []),
    ...(Array.isArray(record.data?.documents) ? record.data.documents : [])
  ];
  const candidates = [
    record.rawText,
    record.raw_text,
    record.data?.rawText,
    record.data?.raw_text,
    ...documents.map(doc => doc?.meta?.rawText),
    ...documents.map(doc => doc?.rawText),
    ...documents.map(doc => doc?.text)
  ];
  return String(candidates.find(value => typeof value === 'string' && value.trim()) || '').trim();
}

function normalizeCvHeading(line) {
  return String(line || '')
    .trim()
    .replace(/[:：]+$/g, '')
    .replace(/\s+/g, ' ')
    .toUpperCase();
}

function isLikelyCvHeading(line) {
  const normalized = normalizeCvHeading(line);
  if (!normalized || normalized.length > 48) return false;
  if (/^\d/.test(normalized)) return false;
  if (/^[-*•·]/.test(normalized)) return false;
  return /^[A-ZÄÖÜẞ&/ -]+$/.test(normalized);
}

function cvHeadingMatches(heading, target) {
  if (!heading || !target) return false;
  return heading === target ||
    heading.startsWith(`${target} `) ||
    heading.startsWith(`${target} &`) ||
    heading.startsWith(`${target}/`);
}

function extractCvRawSection(rawText, startHeadings, stopHeadings) {
  const lines = String(rawText || '').replace(/\r/g, '').split('\n');
  const starts = startHeadings.map(normalizeCvHeading);
  const stops = stopHeadings.map(normalizeCvHeading);
  let startIndex = -1;

  for (let i = 0; i < lines.length; i += 1) {
    const heading = normalizeCvHeading(lines[i]);
    if (starts.some(start => cvHeadingMatches(heading, start) || heading.includes(start))) {
      startIndex = i + 1;
      break;
    }
  }
  if (startIndex < 0) return [];

  const section = [];
  for (let i = startIndex; i < lines.length; i += 1) {
    const line = lines[i];
    const heading = normalizeCvHeading(line);
    if (section.length && isLikelyCvHeading(line) && stops.some(stop => cvHeadingMatches(heading, stop))) {
      break;
    }
    section.push(line);
  }

  return section
    .map(line => String(line || '').trim())
    .filter(Boolean);
}

function looksLikeDateEntry(line) {
  const text = String(line || '').trim();
  return /^(\d{1,2}[./]\d{1,2}[./]\d{2,4}|\d{1,2}[./]\d{4}|\d{4}|[A-Za-zÄÖÜäöüß]+ \d{4})\s*([–-]|bis|to)\s*/i.test(text) ||
    /^(\d{1,2}[./]\d{4}|\d{4})\s+\S/.test(text);
}

function stripBulletPrefix(line) {
  return String(line || '').trim().replace(/^[-–•*]\s*/, '').trim();
}

function groupTimelineEntries(lines) {
  const entries = [];
  let current = null;
  for (const rawLine of Array.isArray(lines) ? lines : []) {
    const line = String(rawLine || '').trim();
    if (!line) continue;
    if (!current || looksLikeDateEntry(line)) {
      current = { title: line, details: [] };
      entries.push(current);
      continue;
    }
    current.details.push(stripBulletPrefix(line));
  }
  return entries;
}

function renderCvRawLines(lines, matchItems, kind = 'plain') {
  const visible = Array.isArray(lines) ? lines.filter(Boolean).slice(0, 80) : [];
  if (!visible.length) return '';
  if (kind === 'experience' || kind === 'education') {
    const entries = groupTimelineEntries(visible);
    if (entries.length) {
      return `
        <div class="drawer-timeline">
          ${entries.map(entry => `
            <article class="drawer-timeline-item">
              <div class="drawer-timeline-title">${highlightTextWithMatchItems(entry.title, matchItems)}</div>
              ${entry.details.length ? `
                <ul class="drawer-compact-list">
                  ${entry.details.slice(0, 12).map(line => `<li>${highlightTextWithMatchItems(line, matchItems)}</li>`).join('')}
                </ul>
              ` : ''}
            </article>
          `).join('')}
        </div>
      `;
    }
  }
  return `
    <div class="drawer-text-stack">
      ${visible.map(line => `<div>${highlightTextWithMatchItems(line, matchItems)}</div>`).join('')}
    </div>
  `;
}

function renderDrawerChips(items, matchItems) {
  const values = (Array.isArray(items) ? items : [])
    .map(value => String(value || '').trim())
    .filter(Boolean);
  if (!values.length) return '';
  return `
    <div class="drawer-chip-row">
      ${values.map(value => `<span class="drawer-chip">${highlightTextWithMatchItems(value, matchItems)}</span>`).join('')}
    </div>
  `;
}

function renderDrawerList(items, matchItems) {
  const values = (Array.isArray(items) ? items : [])
    .map(value => String(value || '').trim())
    .filter(Boolean);
  if (!values.length) return '';
  return `
    <ul class="drawer-compact-list">
      ${values.map(value => `<li>${highlightTextWithMatchItems(value, matchItems)}</li>`).join('')}
    </ul>
  `;
}

function renderDrawerProse(text, matchItems) {
  const source = String(text || '').trim();
  if (!source) return '';
  const chunks = source
    .split(/\n{2,}|(?<=[.!?])\s+(?=[A-ZÄÖÜ])/)
    .map(part => part.trim())
    .filter(Boolean);
  const parts = chunks.length > 1 ? chunks : [source];
  return `
    <div class="drawer-prose">
      ${parts.map(part => `<p>${highlightTextWithMatchItems(part, matchItems)}</p>`).join('')}
    </div>
  `;
}

function normalizeTextList(value) {
  if (Array.isArray(value)) return value.map(item => String(item || '').trim()).filter(Boolean);
  if (typeof value !== 'string') return [];
  return value
    .split(/\r?\n|[•]/)
    .map(item => item.replace(/^[-–]\s*/, '').trim())
    .filter(Boolean);
}

function normalizeSkillList(value) {
  if (Array.isArray(value)) return value.map(item => String(item || '').trim()).filter(Boolean);
  if (typeof value === 'string') {
    return value
      .split(/[,;\n]/)
      .map(item => item.trim())
      .filter(Boolean);
  }
  return [];
}

function fallbackCvSectionLines(rawText, kind) {
  if (kind === 'experience') {
    return extractCvRawSection(
      rawText,
      ['BERUFSERFAHRUNG', 'BERUFLICHER WERDEGANG', 'PRAKTISCHE ERFAHRUNG', 'PROFESSIONAL EXPERIENCE', 'WORK EXPERIENCE', 'EXPERIENCE', 'EMPLOYMENT HISTORY'],
      ['AUSBILDUNG', 'EDUCATION', 'FORTBILDUNG', 'WEITERBILDUNG', 'QUALIFIKATIONEN', 'SKILLS', 'FACHKENNTNISSE', 'SPRACHEN', 'LANGUAGES', 'HOBBIES', 'INTERESSEN', 'DATUM']
    );
  }
  if (kind === 'education') {
    return extractCvRawSection(
      rawText,
      ['AUSBILDUNG', 'AUSBILDUNGSWEG', 'BILDUNGSWEG', 'EDUCATION', 'STUDIUM', 'SCHULE', 'ACADEMIC BACKGROUND'],
      ['BERUFSERFAHRUNG', 'PROFESSIONAL EXPERIENCE', 'WORK EXPERIENCE', 'EXPERIENCE', 'FORTBILDUNG', 'WEITERBILDUNG', 'QUALIFIKATIONEN', 'SKILLS', 'FACHKENNTNISSE', 'SPRACHEN', 'LANGUAGES', 'HOBBIES', 'INTERESSEN', 'DATUM']
    );
  }
  if (kind === 'skills') {
    return extractCvRawSection(
      rawText,
      ['FACHKENNTNISSE', 'KENNTNISSE', 'QUALIFIKATIONEN', 'QUALIFICATIONS', 'SKILLS', 'TECHNICAL SKILLS', 'CORE SKILLS', 'KOMPETENZEN'],
      ['SPRACHEN', 'LANGUAGES', 'AUSBILDUNG', 'EDUCATION', 'BERUFSERFAHRUNG', 'PROFESSIONAL EXPERIENCE', 'WORK EXPERIENCE', 'FORTBILDUNG', 'HOBBIES', 'INTERESSEN', 'DATUM']
    );
  }
  return [];
}


function escapeRegExp(str){
  return String(str).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function escapeHtmlText(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * text:   Plaintext aus Requirement/Object
 * items:  Array von { requirementId, snippet, color }
 *         (für requirementSnippet oder objectSnippet, je nach Seite)
 *
 * Gibt HTML-String mit <span class="match-highlight" ...>wrap</span> zurück.
 */
function highlightTextWithMatchItems(text, items){
  const sourceText = String(text || '');
  if (!sourceText) return '';
  if (!items || !items.length) return escapeHtmlText(sourceText);

  const intervals = [];
  for (const item of items) {
    const snippets = Array.isArray(item?.snippets) && item.snippets.length
      ? item.snippets
      : [item?.snippet];
    const color = item?.color || '#6c8cff';
    const reqId = item?.requirementId || '';
    for (const snippet of snippets) {
      const candidates = buildHighlightCandidates(snippet);
      for (const candidate of candidates) {
        collectHighlightIntervals(sourceText, candidate, { color, reqId, intervals });
      }
    }
  }

  const merged = mergeHighlightIntervals(intervals, sourceText.length);
  if (!merged.length) return escapeHtmlText(sourceText);

  let html = '';
  let cursor = 0;
  for (const interval of merged) {
    if (interval.start > cursor) html += escapeHtmlText(sourceText.slice(cursor, interval.start));
    const match = sourceText.slice(interval.start, interval.end);
    html += `<span class="match-highlight" data-match-req="${escapeHtmlText(interval.reqId)}" style="--matchColor:${escapeHtmlText(interval.color)}">${escapeHtmlText(match)}</span>`;
    cursor = interval.end;
  }
  if (cursor < sourceText.length) html += escapeHtmlText(sourceText.slice(cursor));
  return html;
}

function buildHighlightCandidates(snippet) {
  const raw = String(snippet || '').replace(/\s+/g, ' ').trim();
  if (!raw) return [];

  const withoutEllipsis = raw.replace(/[.…]{2,}/g, ' ').replace(/\s+/g, ' ').trim();
  const withoutLabel = withoutEllipsis
    .replace(/^(titel|rolle|anforderungen|skills|skill|aktuelle rolle|abschluss|rohtext|cv|lebenslauf|stellenausschreibung|berufserfahrung|ausbildung)\s*:\s*/i, '')
    .trim();
  const labelStripped = withoutEllipsis
    .replace(/\b(titel|rolle|anforderungen|skills|skill|aktuelle rolle|abschluss|rohtext|cv|lebenslauf|stellenausschreibung|berufserfahrung|ausbildung)\s*:\s*/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  const candidates = new Set();
  [withoutLabel, labelStripped, withoutEllipsis, raw].forEach((value) => {
    const trimmed = value.trim();
    if (trimmed.length >= 8) candidates.add(trimmed);
  });

  const fragments = [withoutLabel, labelStripped]
    .join(' · ')
    .split(/\s*[;,|•]\s*|\s+-\s+|\s+·\s+|\s+\/\s+/)
    .map((part) => part.trim())
    .filter((part) => part.length >= 8);
  fragments.forEach((part) => candidates.add(part));

  const words = labelStripped.split(/\s+/).filter(Boolean);
  for (let size = Math.min(8, words.length); size >= 3; size -= 1) {
    for (let i = 0; i <= words.length - size; i += 1) {
      const phrase = words.slice(i, i + size).join(' ');
      if (phrase.length >= 14) candidates.add(phrase);
    }
    if (candidates.size >= 12) break;
  }

  return Array.from(candidates)
    .sort((a, b) => b.length - a.length)
    .slice(0, 18);
}

function collectHighlightIntervals(text, candidate, { color, reqId, intervals }) {
  const term = String(candidate || '').trim();
  if (term.length < 3) return;
  const pattern = escapeRegExp(term).replace(/\s+/g, '\\s+');
  let re;
  try {
    re = new RegExp(pattern, 'gi');
  } catch {
    return;
  }
  let match;
  while ((match = re.exec(text))) {
    const start = match.index;
    const end = start + match[0].length;
    if (end > start) intervals.push({ start, end, color, reqId });
    if (re.lastIndex === match.index) re.lastIndex += 1;
  }
}

function mergeHighlightIntervals(intervals, maxLength) {
  const sorted = intervals
    .filter((item) => Number.isFinite(item.start) && Number.isFinite(item.end) && item.end > item.start)
    .map((item) => ({
      ...item,
      start: Math.max(0, Math.min(maxLength, item.start)),
      end: Math.max(0, Math.min(maxLength, item.end)),
    }))
    .sort((a, b) => (a.start - b.start) || ((b.end - b.start) - (a.end - a.start)));

  const accepted = [];
  for (const item of sorted) {
    const overlaps = accepted.some((existing) => item.start < existing.end && item.end > existing.start);
    if (!overlaps) accepted.push(item);
  }
  return accepted.sort((a, b) => a.start - b.start);
}

/**
 * Liefert alle Match-Items aus currentMatchDetail für eine bestimmte Seite
 * side: 'requirement' | 'object'  → nutzt jobSnippet bzw. cvSnippet
 */
function getSideMatchItemsForCurrent(requirementId, objectId, side){
  if (!currentMatchDetail) return [];
  if (currentMatchDetail.requirementId !== requirementId || currentMatchDetail.objectId !== objectId) return [];
  const items = currentMatchDetail.items || [];
  const colorByReq = currentMatchDetail.colorByRequirement || {};
  const fields = side === 'object'
    ? ['cvSnippet', 'objectSnippet']
    : ['jobSnippet', 'requirementSnippet'];

  return items
    .filter(it => fields.some(field => (it[field] || '').trim()))
    .map(it => {
      const snippets = fields
        .map(field => String(it[field] || '').trim())
        .filter(Boolean);
      return {
        requirementId: it.requirementId,
        snippet: snippets[0] || '',
        snippets,
        color: colorByReq[it.requirementId] || MATCH_COLORS[0]
      };
    });
}

function setActiveRequirement(reqId){
  // Pills in der Bottom-Bar
  if (matchDetailBar){
    matchDetailBar.querySelectorAll('.match-pill').forEach(btn=>{
      btn.classList.toggle('is-active', !!reqId && btn.dataset.req === reqId);
    });
  }

  // Karten im mittleren Detailbereich (Notiz-Modal), falls noch genutzt
  const detailHost = document.getElementById('noteMatchDetail');
  if (detailHost){
    detailHost.querySelectorAll('.match-item-card').forEach(card=>{
      card.classList.toggle('is-active', !!reqId && card.dataset.req === reqId);
    });
  }

  // Text-Highlights in Requirement/Object
  getMatchingModuleHost().querySelectorAll('.match-highlight').forEach(span=>{
    const sid = span.getAttribute('data-match-req');
    span.classList.toggle('is-active', !!reqId && sid === reqId);
  });

  // Wenn nichts aktiv ist → Overlay ausblenden
  if (!reqId){
    hideMatchOverlay();
  }
}


const CONFLICT_TYPE_EMOJI = {
  level_scope: '📈',
  compensation_band: '💶',
  location_work_model: '📍',
  career_path: '🧭',
  domain_industry: '🏭',
  role_definition: '🧩',
  availability: '📅',
  other: '⚠️'
};

function _splitConflictTypes(raw) {
  if (!raw) return [];
  if (Array.isArray(raw)) {
    return raw
      .flatMap(v => _splitConflictTypes(v))
      .filter(Boolean);
  }
  if (typeof raw === 'string') {
    return raw
      .split(/[\s,;|]+/g)
      .map(s => s.trim())
      .filter(Boolean);
  }
  return [];
}

function _normalizeConflictType(t) {
  const s = String(t || '').trim();
  return s ? s.toLowerCase() : '';
}

function getConflictTypesFromItem(it) {
  const item = it && typeof it === 'object' ? it : {};
  const set = new Set();

  // 1) explizite Felder (heute + zukünftig)
  const direct = []
    .concat(_splitConflictTypes(item.conflictTypes))
    .concat(_splitConflictTypes(item.conflictType));

  direct.forEach(t => {
    const norm = _normalizeConflictType(t);
    if (norm) set.add(norm);
  });

  // 2) Fallback NUR wenn der Titel EXAKT ein bekannter Konflikt-Key ist (wie "level_scope")
  const titleNorm = _normalizeConflictType(item.title);
  const knownKeys = new Set([
    ...CONFLICT_TYPE_ORDER,
    ...Object.keys(CONFLICT_TYPE_EMOJI)
  ]);

  if (titleNorm && knownKeys.has(titleNorm)) {
    set.add(titleNorm);
  }

  // 3) irgendein anderes conflict*-Key vorhanden → Conflict (aber Typ ggf. unknown)
  const otherConflictKeyPresent = Object.keys(item).some(k => {
    if (!k) return false;
    if (k === 'conflictType' || k === 'conflictTypes') return false;
    return k.startsWith('conflict') && !!item[k];
  });

  const hasExplicitBase = item.baseConflict === true;
  const hasTypes = set.size > 0;

  const isConflict = hasExplicitBase || hasTypes || otherConflictKeyPresent;

  // Wenn Conflict erkannt, aber kein Typ → "other"
  if (isConflict && set.size === 0) set.add('other');

  // Stable order + unknown handling
  const all = Array.from(set).map(_normalizeConflictType).filter(Boolean);
  const known = [];
  const unknown = [];

  for (const t of all) {
    if (CONFLICT_TYPE_ORDER.includes(t)) known.push(t);
    else unknown.push(t);
  }

  const orderedKnown = CONFLICT_TYPE_ORDER.filter(t => known.includes(t));

  const out = orderedKnown.slice();
  if (unknown.length) out.push('other');

  return Array.from(new Set(out));
}

function getOverallConflictInfo(items) {
  const list = Array.isArray(items) ? items : [];
  const types = new Set();
  const tips = [];

  for (const it of list) {
    const t = getConflictTypesFromItem(it);
    t.forEach(x => types.add(x));

    const tip = getConflictTooltipFromItem(it);
    if (tip) tips.push(tip);
  }

  const iconArr = Array.from(types).map(t => CONFLICT_TYPE_EMOJI[t] || '⚠️');
  const icons = iconArr.length ? iconArr.join('') : '';

  // Tooltip: entweder alle Summaries (deduped) oder leer
  const dedupTips = Array.from(new Set(tips)).filter(Boolean);
  const tooltip = dedupTips.join(' • ');

  return { icons, tooltip };
}


function getConflictIconsFromItem(it) {
  const types = getConflictTypesFromItem(it);
  if (!types.length) return [];
  return types.map(t => CONFLICT_TYPE_EMOJI[t] || '⚠️');
}

function getConflictTooltipFromItem(it) {
  const item = it && typeof it === 'object' ? it : {};
  const s = typeof item.conflictSummary === 'string' ? item.conflictSummary.trim() : '';
  return s || '';
}

function hasConflictOnItem(it) {
  return getConflictTypesFromItem(it).length > 0;
}

const CONFLICT_TYPE_ORDER = [
  'level_scope',
  'compensation_band',
  'location_work_model',
  'career_path',
  'domain_industry',
  'role_definition',
  'availability',
  'other'
];

function escapeHtmlAttr(str){
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function normalizeConflictTypeKey(raw){
  const t = String(raw ?? '').trim().toLowerCase();
  if (!t) return '';
  // erlaubt z.B. "location-work-model" / "location work model"
  const norm = t
    .replace(/\s+/g, '_')
    .replace(/-+/g, '_')
    .replace(/__+/g, '_')
    .trim();
  return norm;
}

function parseConflictTypesFromItem(item){
  if (!item || typeof item !== 'object') return [];

  const out = [];

  // future-proof: conflictTypes: [...]
  const rawTypes =
    (Array.isArray(item.conflictTypes) ? item.conflictTypes :
     Array.isArray(item.conflictType) ? item.conflictType :
     (typeof item.conflictTypes === 'string' ? item.conflictTypes :
      typeof item.conflictType === 'string' ? item.conflictType :
      null));

  if (Array.isArray(rawTypes)) {
    rawTypes.forEach(t => {
      if (t == null) return;
      const s = String(t).trim();
      if (s) out.push(s);
    });
  } else if (typeof rawTypes === 'string') {
    // akzeptiert "a,b" | "a|b" | "a b"
    rawTypes
      .split(/[,\|\s]+/g)
      .map(s => s.trim())
      .filter(Boolean)
      .forEach(s => out.push(s));
  }

  return out;
}

function hasConflictIndicator(item){
  if (!item || typeof item !== 'object') return false;

  if (item.baseConflict === true) return true;

  if (item.conflictType != null || item.conflictTypes != null) {
    const t = parseConflictTypesFromItem(item);
    if (t.length) return true;
    // conflictType gesetzt aber leer -> trotzdem conflict
    if (item.conflictType != null || item.conflictTypes != null) return true;
  }

  // weitere conflict-keys wie conflictSummary etc.
  for (const k in item) {
    if (!Object.prototype.hasOwnProperty.call(item, k)) continue;
    const lk = String(k).toLowerCase();
    if (!lk.startsWith('conflict')) continue;
    if (lk === 'conflicttype' || lk === 'conflicttypes') continue;
    if (lk === 'baseconflict') continue;
    if (item[k]) return true;
  }

  return false;
}

function getConflictInfo(item){
  const has = hasConflictIndicator(item);
  if (!has) {
    return { hasConflict: false, types: [], icons: [], tooltip: '' };
  }

  let types = parseConflictTypesFromItem(item)
    .map(normalizeConflictTypeKey)
    .filter(Boolean);

  // baseConflict aber kein konkreter Typ -> other
  if (!types.length) {
    types = ['other'];
  }

  // unknown -> other
  types = types.map(t => (CONFLICT_TYPE_EMOJI[t] ? t : 'other'));

  // dedupe
  const uniq = [];
  const seen = new Set();
  for (const t of types) {
    if (seen.has(t)) continue;
    seen.add(t);
    uniq.push(t);
  }

  // stable order
  uniq.sort((a, b) => {
    const ai = CONFLICT_TYPE_ORDER.indexOf(a);
    const bi = CONFLICT_TYPE_ORDER.indexOf(b);
    return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
  });

  const icons = uniq.map(t => CONFLICT_TYPE_EMOJI[t] || CONFLICT_TYPE_EMOJI.other);

  const tooltip =
    (typeof item?.conflictSummary === 'string' && item.conflictSummary.trim())
      ? item.conflictSummary.trim()
      : '';

  return { hasConflict: true, types: uniq, icons, tooltip };
}

function renderConflictIconsHtml(icons){
  const arr = Array.isArray(icons) ? icons : [];
  if (!arr.length) return '';
  return `
    <span class="conflict-icons"
          style="margin-left:4px;display:inline-flex;align-items:center;gap:2px;line-height:1;">
      ${arr.map(e => `<span class="conflict-icon" aria-hidden="true">${e}</span>`).join('')}
    </span>
  `;
}

function renderScoreWithConflictsHtml(scoreLabel, conflictInfo){
  if (!conflictInfo || !conflictInfo.hasConflict) return String(scoreLabel);
  const tooltipAttr = conflictInfo.tooltip ? ` title="${escapeHtmlAttr(conflictInfo.tooltip)}"` : '';
  return `
    <span class="score-conflict" style="display:inline-flex;align-items:center;"${tooltipAttr}>
      <s>${String(scoreLabel)}</s>
      ${renderConflictIconsHtml(conflictInfo.icons)}
    </span>
  `;
}



function renderMatchItemPreviewInBar(reqId){
  if (!matchBarContent) return;

  if (!currentMatchDetail || !currentMatchDetail.items || !currentMatchDetail.items.length){
    matchBarContent.innerHTML = '';
    matchBarContent.style.display = 'none';
    return;
  }

  const item = currentMatchDetail.items.find(it => it.requirementId === reqId);
  if (!item){
    matchBarContent.innerHTML = '';
    matchBarContent.style.display = 'none';
    return;
  }

  const colorByRequirement = currentMatchDetail.colorByRequirement || {};
  const color = colorByRequirement[item.requirementId] || MATCH_COLORS[0];

  const dimMap = {
    education: 'Ausbildung',
    experience: 'Erfahrung',
    skill: 'Skill',
    language: 'Sprache',
    other: 'Sonstiges'
  };
  const prioMap = {
    base: 'Basis',
    performance: 'Leistung',
    enthusiasm: 'Begeisterung'
  };
  const levelMap = {
    full: 'voll',
    partial: 'teilweise',
    none: 'kein'
  };

  const dimLabel   = dimMap[item.dimension] || item.dimension || 'Sonstiges';
  const prioLabel  = prioMap[item.priority] || item.priority || '';
  const levelLabel = levelMap[item.matchLevel] || item.matchLevel || '';

  const pct = (()=>{
    if (typeof item.matchScore === 'number'){
      const raw = item.matchScore;
      if (raw <= 1 && raw >= 0) return Math.round(raw * 100);
      return Math.round(raw);
    }
    if (typeof item.matchScoreKey === 'number'){
      return Math.round(item.matchScoreKey);
    }
    if (item.matchLevel === 'full')    return 100;
    if (item.matchLevel === 'partial') return 60;
    if (item.matchLevel === 'none')    return 0;
    return null;
  })();

  const conflictInfo = getConflictInfo(item);
  const pctHtml = (pct != null)
    ? renderScoreWithConflictsHtml(`${pct}%`, conflictInfo)
    : '';

  const requirementSnip = item.jobSnippet || item.requirementSnippet || '';
  const objectSnip  = item.cvSnippet || item.objectSnippet || '';
  const expl    = item.explanation || '';

  // Bereich im Modal sichtbar machen
  matchBarContent.style.display   = 'block';
  matchBarContent.style.paddingTop = '4px';
  matchBarContent.style.maxHeight = '150px';
  matchBarContent.style.overflowY = 'auto';

  matchBarContent.innerHTML = `
    <article class="match-item-card" data-req="${item.requirementId}" style="--matchColor:${color};margin-top:6px">
      <div class="match-item-header">
        <div class="match-item-badge"></div>
        <div>
          <div class="match-item-title" style="font-size:12px">${item.title || 'Anforderung'}</div>
          <div class="match-item-meta" style="font-size:11px">
            ${dimLabel}
            ${prioLabel ? ` · Priorität: ${prioLabel}` : ''}
            ${levelLabel ? ` · Match: ${levelLabel}` : ''}
            ${pctHtml ? ` · ${pctHtml}` : ''}
          </div>
        </div>
      </div>
      ${expl ? `<div class="match-item-expl" style="font-size:11px;margin-top:4px">${expl}</div>` : ''}
      <div class="match-item-snips" style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px;margin-top:6px">
        <div class="match-snippet-block">
          <div class="match-snippet-label" style="font-size:10px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:2px">Stellenausschreibung</div>
          <div style="font-size:11px">
            ${requirementSnip
              ? `<span class="match-highlight" data-match-req="${item.requirementId}" style="--matchColor:${color}">${requirementSnip}</span>`
              : '<span class="muted">Kein Nachweis.</span>'}
          </div>
        </div>
        <div class="match-snippet-block">
          <div class="match-snippet-label" style="font-size:10px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:2px">CV</div>
          <div style="font-size:11px">
            ${objectSnip
              ? `<span class="match-highlight" data-match-req="${item.requirementId}" style="--matchColor:${color}">${objectSnip}</span>`
              : '<span class="muted">Kein Nachweis.</span>'}
          </div>
        </div>
      </div>
    </article>
  `;
}




// --- Status-Definitionen & Helfer ---

const ALL_MATCH_STATUSES = [
  'prospecting',
  'prematch',
  'active',
  'interview',
  'offer',
  'hired',
  'rejected',
  'on-hold'
];

// höher = "weiter im Prozess" = relevanter
const MATCH_STATUS_RANK = {
  prospecting: 10,
  prematch: 15,
  active: 20,
  interview: 30,
  offer: 40,
  'on-hold': 25,
  hired: 50,
  rejected: 50
};

function statusLabel(st){
  switch (st){
    case 'prospecting': return 'Prospecting';
    case 'prematch':    return 'Match';
    case 'active':      return 'Aktiv';
    case 'interview':   return 'Interview';
    case 'offer':       return 'Offer';
    case 'hired':       return 'Hired';
    case 'rejected':    return 'Rejected';
    case 'on-hold':     return 'On Hold';
    default:            return st;
  }
}

// erlaubt z.B. #onhold, #on-hold, #match etc.
function normalizeStatusToken(raw){
  const t = String(raw || '').toLowerCase().trim();
  if (t === 'onhold' || t === 'on-hold') return 'on-hold';
  if (t === 'match' || t === 'match') return 'prematch';
  return t;
}

// liest alle #tags aus dem Notiztext und filtert auf bekannte Status
function extractStatusesFromText(text){
  const out = new Set();
  if (!text) return [];
  const regex = /#([a-zA-Z\-]+)/g;
  let m;
  while ((m = regex.exec(text)) !== null){
    const norm = normalizeStatusToken(m[1]);
    if (ALL_MATCH_STATUSES.includes(norm)) out.add(norm);
  }
  return Array.from(out);
}

/**
 * Ermittelt:
 * - statuses: alle im Text vorkommenden Status-Hashtags (#prospecting ...)
 * - canonical: der "relevanteste" Status (für die DB) nach MATCH_STATUS_RANK
 *
 * fallbackStatus:
 *   - wird genutzt, wenn im Text gar keine Status-Hashtags vorkommen
 *   - d.h. UI bleibt dann "ausgeblasst", DB behält aber ihren bisherigen Status
 */
function deriveStatusesFromNotes(notes, fallbackStatus){
  const statuses = extractStatusesFromText(notes);
  const fb = ALL_MATCH_STATUSES.includes(fallbackStatus) ? fallbackStatus : 'prospecting';

  if (!statuses.length){
    return { statuses: [], canonical: fb };
  }

  let best = statuses[0];
  let bestScore = MATCH_STATUS_RANK[best] ?? 0;

  for (const st of statuses){
    const score = MATCH_STATUS_RANK[st] ?? 0;
    if (score > bestScore){
      best = st;
      bestScore = score;
    }
  }
  return { statuses, canonical: best };
}



// --- Matching & Lösch-Helper ---

// --- Matching & Lösch-Helper (RxDB-konform zum matchingResultsSchema) ---

function findRequirementAndSource(requirementId){
  for (const c of sources){
    const j = (c.requirements || []).find(j => j.id === requirementId);
    if (j) return { requirement: j, source: c };
  }
  return { requirement: null, source: null };
}

function getUniqueActiveMatchesByPair(matchList = []){
  const result = [];
  const seen = new Set();

  for (const m of matchList){
    if (!m || m.removed) continue;
    const pair = key(m.requirementId, m.objectId);
    if (seen.has(pair)) continue;
    seen.add(pair);
    result.push(m);
  }

  return result;
}

function getMatchesForObject(objectId){
  return getUniqueActiveMatchesByPair(matches.filter(m => m.objectId === objectId));
}

function getMatchesForRequirement(requirementId){
  return getUniqueActiveMatchesByPair(matches.filter(m => m.requirementId === requirementId));
}

function getMatchesForRequirements(requirementIds){
  const idSet = new Set(
    Array.isArray(requirementIds)
      ? requirementIds.map(id => String(id)).filter(Boolean)
      : []
  );
  return getUniqueActiveMatchesByPair(matches.filter(m => idSet.has(String(m.requirementId))));
}

function getMatchSummaryLines(matchRows){
  const uniqueRequirements = new Set();
  const uniqueObjects = new Set();

  for (const m of matchRows){
    if (!m || m.removed) continue;
    uniqueRequirements.add(String(m.requirementId));
    uniqueObjects.add(String(m.objectId));
  }

  return [
    `- ${matchRows.length} Match${matchRows.length === 1 ? '' : 'es'}`,
    `- ${uniqueRequirements.size} betroffene Anforderung${uniqueRequirements.size === 1 ? '' : 'n'}`,
    `- ${uniqueObjects.size} betroffene Objekt${uniqueObjects.size === 1 ? 'in' : 'en'}`
  ];
}

async function removeMatchDocsFromDb(matchRows = []){
  if (!rxdb || !rxdb.matches || !matchRows.length) return;

  const uniqueRows = getUniqueActiveMatchesByPair(matchRows);
  await Promise.all(
    uniqueRows.map(async m => {
      const docs = await rxdb.matches.find({
        selector: { requirementId: m.requirementId, objectId: m.objectId }
      }).exec();

      if (Array.isArray(docs)) {
        for (const d of docs) {
          if (d && d.remove) await d.remove();
        }
      } else if (docs && docs.remove) {
        await docs.remove();
      }
    })
  );
}

function purgeMatchesFromState(matchRows){
  const unique = getUniqueActiveMatchesByPair(matchRows);
  if (!unique.length) return;

  const removeKeys = new Set(unique.map(m => key(m.requirementId, m.objectId)));
  matches = matches.filter(m => !removeKeys.has(key(m.requirementId, m.objectId)));
  removeKeys.forEach(k => processes.delete(k));
}

function purgeProcessesForRequirementIds(requirementIds = []){
  const idSet = new Set(
    Array.isArray(requirementIds)
      ? requirementIds.map(id => String(id)).filter(Boolean)
      : []
  );
  if (!idSet.size) return;

  for (const k of Array.from(processes.keys())){
    const [jid] = k.split(':');
    if (idSet.has(String(jid))) {
      processes.delete(k);
    }
  }
}

async function removeRequirementsFromDb(requirementIds = []){
  if (!rxdb || !rxdb.requirements || !requirementIds.length) return;

  const uniqueRequirementIds = [...new Set(
    requirementIds
      .map(id => String(id))
      .filter(Boolean)
  )];

  await Promise.all(
    uniqueRequirementIds.map(async requirementId => {
      const doc = await rxdb.requirements.findOne({ selector: { id: requirementId } }).exec();
      if (doc) await doc.remove();
    })
  );
}

async function confirmDeletionWithCount({ subject, dependencyLines = [], totalItemsToDelete }){
  if (!Number.isFinite(totalItemsToDelete) || totalItemsToDelete < 1) return false;

  const lines = [
    `Achtung: ${subject} wird gelöscht und die nachfolgenden Abhängigkeiten werden automatisch entfernt.`,
    '',
    ...dependencyLines,
    '',
    `Gesamtanzahl zu löschender Datensätze: ${totalItemsToDelete}`,
    '',
    `Zum Fortfahren gib bitte genau diese Zahl ein: ${totalItemsToDelete}`
  ].filter(line => line !== '');

  const confirmInput = await showBusinessPrompt(lines.join('\n'), {
    title: 'Löschung bestätigen',
    confirmLabel: 'Löschen',
    kind: 'danger',
  });
  return String(confirmInput || '').trim() === String(totalItemsToDelete);
}

function hasMatchesForObject(objectId){
  // Ein Objekt gilt als "hat Matches", sobald es irgendein nicht-entferntes Match gibt,
  // egal ob der Prozess gerade aktiv oder pausiert ist.
  return matches.some(m =>
    m.objectId === objectId &&
    !m.removed
  );
}


function hasMatchesForRequirement(requirementId){
  // Analog zu oben: paussierte Matches zählen weiterhin als vorhanden.
  return matches.some(m =>
    m.requirementId === requirementId &&
    !m.removed
  );
}


// Match-Dokument in RxDB aktualisieren (progress, notes, active, status, score)
async function updateMatchState(requirementId, objectId, patch){
  const m = matches.find(x => x.requirementId === requirementId && x.objectId === objectId && !x.removed);
  if (!m) return;

  // UI-Match-Objekt aktualisieren
  Object.assign(m, patch);

  const k = key(requirementId, objectId);
  const prev = processes.get(k) || {
    progress: DEFAULT_PROGRESS,
    notes: '',
    active: true,
    statuses: [],
    status: m?.status || 'prospecting'
  };

  const nextNotes = (patch.notes != null) ? patch.notes : prev.notes;
  let statuses = prev.statuses || [];
  let canonical = patch.status || prev.status || m?.status || 'prospecting';

  // Wenn Notes geändert wurden und noch kein explizites status/statuses im Patch:
  // Status aus Notiztext ableiten (#prospecting, #active, ...)
  if (patch.notes != null && !patch.statuses && !patch.status){
    const derived = deriveStatusesFromNotes(nextNotes, canonical);
    statuses = derived.statuses;
    canonical = derived.canonical;
  } else {
    if (patch.statuses) statuses = patch.statuses;
    if (patch.status)   canonical = patch.status;
  }

  const newState = {
    progress: (patch.progress != null) ? patch.progress : prev.progress,
    notes: nextNotes,
    active: (patch.active != null) ? patch.active : prev.active,
    statuses,
    status: canonical
  };
  processes.set(k, newState);

  // Persistenz in RxDB
  if (rxdb && rxdb.matches){
    const doc = await rxdb.matches
      .findOne({ selector: { requirementId, objectId: objectId } })
      .exec();

    if (doc){
      const now = new Date().toISOString();
      const data = { updatedAt: now };

      if (patch.progress != null) {
        data.progress = patch.progress;
      }
      if (patch.notes != null) {
        data.notes = patch.notes;
      }
      if (patch.active != null){
        data.active    = !!patch.active;
        data.activeKey = patch.active ? 1 : 0;
      }
      if (canonical != null){
        // immer den berechneten kanonischen Status speichern
        data.status = canonical;
      }
      if (patch.score != null){
        data.score    = patch.score;
        data.scoreKey = patch.score;
      }

      // wie gehabt: verschiedene RxDB-Update-APIs unterstützen
      if (typeof doc.atomicPatch === 'function'){
        await doc.atomicPatch(data);
      } else if (typeof doc.atomicUpdate === 'function'){
        await doc.atomicUpdate(prevDoc => ({ ...prevDoc, ...data }));
      } else if (typeof doc.incrementalModify === 'function'){
        await doc.incrementalModify(prevDoc => Object.assign(prevDoc, data));
      } else if (typeof doc.update === 'function'){
        await doc.update({ $set: data });
      } else {
        console.warn('RxDB-Dokument hat keine Update-Methode, Match wird nur im UI aktualisiert.');
      }
    }
  }
}

async function persistPendingMatchForCtox(match, queuedCommand) {
  if (!match || !rxdb?.matches) return;
  const now = new Date().toISOString();
  const event = {
    type: 'match.ctox_queued',
    payload: {
      commandId: queuedCommand?.commandId || '',
      status: queuedCommand?.status || 'queued',
      reason: 'business_os.match.compute'
    },
    at: now
  };
  const doc = {
    ...match,
    active: match.active === true,
    removed: match.removed === true,
    progress: typeof match.progress === 'number' ? match.progress : INITIAL_PROGRESS,
    status: match.status || 'prematch',
    statuses: Array.isArray(match.statuses) ? match.statuses : [],
    score: scoreFromMatchItems(match.items) ?? 0,
    notes: match.notes || '',
    interview: match.interview || { attendees: [], reminders: [] },
    events: [...(Array.isArray(match.events) ? match.events : []), event],
    items: Array.isArray(match.items) ? match.items : [],
    createdAt: match.createdAt || now,
    updatedAt: now,
    activeKey: match.active === true ? 1 : 0,
    scoreKey: scoreFromMatchItems(match.items) ?? 0
  };

  if (typeof rxdb.matches.atomicUpsert === 'function') {
    await rxdb.matches.atomicUpsert(doc);
    return;
  }
  if (typeof rxdb.matches.upsert === 'function') {
    await rxdb.matches.upsert(doc);
    return;
  }
  const existing = await rxdb.matches.findOne({ selector: { id: doc.id } }).exec();
  if (existing) {
    const { id, ...patchData } = doc;
    if (typeof existing.atomicPatch === 'function') {
      await existing.atomicPatch(patchData);
    } else if (typeof existing.atomicUpdate === 'function') {
      await existing.atomicUpdate((prevDoc) => ({ ...prevDoc, ...patchData }));
    } else if (typeof existing.incrementalModify === 'function') {
      await existing.incrementalModify((prevDoc) => Object.assign(prevDoc, patchData));
    }
    return;
  }
  await rxdb.matches.insert(doc);
}

function isCtoxQueuedCommandError(error) {
  return error instanceof CtoxQueuedCommandError || error?.commandQueued === true;
}


// Hilfsfunktion: beliebiges gerade offenes "normales" Modal schließen
function closeOpenTransientModal() {
  // Wir schließen nur Modale, die nicht Note/Rel sind
  const openModal = getMatchingModuleHost().querySelector('.modal.open');
  if (!openModal) return;

  if (openModal.id === 'noteModal' || openModal.id === 'relModal') {
    return; // diese Modale lassen wir in Ruhe
  }
  openModal.classList.remove('open');
}

// Hilfsfunktion: Score-Bubble für bestimmtes Requirement/Objekt-Paar auf "Loading" setzen
// WICHTIG: im Render-Code muss die Score-Bubble data-match-key="requirementId|objectId" bekommen (s.u.)
function setScoreLoading(requirementId, objectId, loading) {
  if (typeof key !== 'function') {
    console.warn('setScoreLoading: key(requirementId, objectId) helper nicht vorhanden.');
    return;
  }
  const matchKey = key(requirementId, objectId);
  const scoreEl = getMatchingModuleHost().querySelector(`.score[data-match-key="${matchKey}"]`);
  if (!scoreEl) return;

  if (loading) {
    scoreEl.classList.add('is-loading');
    scoreEl.removeAttribute('data-pct');
    scoreEl.innerHTML = '<span class="spinner"></span>';
  } else {
    scoreEl.classList.remove('is-loading');
    // Inhalt wird von renderRequirements() neu gesetzt – wir fassen ihn hier nicht an
  }
}


async function createMatch(sourceId, requirementId, objectId) {
  const object = getObject(objectId);
  if (!object) throw new Error('Objekt nicht gefunden.');
  if (object.isPlaceholder) {
    throw new Error('Matching ist erst verfügbar, wenn der Objektimport abgeschlossen ist.');
  }

  const procKey = key(requirementId, objectId);
  // Wenn für dieses Paar schon ein Matching läuft, einfach ignorieren
  if (pendingMatchKeys.has(procKey)) {
    return;
  }
  const nowIso = new Date().toISOString();
  const matchId = `${sourceId}|${requirementId}|${objectId}`;

  // Gibt es schon ein Match-Objekt im Speicher?
  let m = matches.find(
    x =>
      x.sourceId === sourceId &&
      x.requirementId === requirementId &&
      x.objectId === objectId &&
      !x.removed
  );

  // Falls nein: optimistisch lokales Match anlegen (ohne Score)
  if (!m) {
    m = {
      id: matchId,
      definitionId: activeDefinitionId(),
      schemaVersion: activeSchemaVersion(),
      sourceId,
      requirementId,
      objectId,

      // NEU: Prozess startet pausiert bei 0%
      active: false,
      removed: false,
      progress: INITIAL_PROGRESS,

      status: 'prematch',
      score: null,
      notes: '',
      interview: {
        bookingLink: null,
        scheduledAt: null,
        attendees: [],
        reminders: [],
        transcriptId: null
      },
      events: [],
      createdAt: nowIso,
      updatedAt: nowIso,
      activeKey: 0,
      scoreKey: 0
    };
    matches.push(m);
  } else {
    m.definitionId = m.definitionId || activeDefinitionId();
    m.schemaVersion = m.schemaVersion || activeSchemaVersion();
  }

  // Prozesseintrag initialisieren (für Fortschritt/Notizen/Status)
  if (!processes.get(procKey)) {
    processes.set(procKey, {
      // NEU: Progress standardmäßig aus dem Match, sonst INITIAL_PROGRESS (0)
      progress: (typeof m.progress === 'number' ? m.progress : INITIAL_PROGRESS),
      notes: m.notes || '',
      // NEU: exakte Übernahme von m.active (true/false), kein "!== false"-Trick
      active: m.active === true,
      statuses: m.statuses || [],
      status: m.status || 'prematch'
    });
  }

  // Matching steht ab jetzt auf "pending" → UI sofort aktualisieren
  pendingMatchKeys.add(procKey);
  renderRequirements();

  // LLM-Matching rechnen & DB + UI aktualisieren
  try {
    const result = await computeRequirementMatch({
      llmChat,
      sourceId,
      requirementId,
      objectId,
      persist: true
    });

    const finalItems = Array.isArray(result.match?.items)
      ? result.match.items.map(normalizeLegacyEvidenceItem)
      : [];
    const finalScore = scoreFromMatchItems(finalItems);

    m.score = finalScore;
    m.scoreKey = finalScore;
    m.items = finalItems;
    m.updatedAt = new Date().toISOString();

    pendingMatchKeys.delete(procKey);
    renderRequirements();
  } catch (err) {
    if (isCtoxQueuedCommandError(err)) {
      try {
        await persistPendingMatchForCtox(m, err);
      } catch (persistErr) {
        console.error('Pending-Match konnte nicht gespeichert werden:', persistErr);
      }
      m.events = [
        ...(Array.isArray(m.events) ? m.events : []),
        {
          type: 'match.ctox_queued',
          payload: { commandId: err.commandId || '', status: err.status || 'queued', definitionId: activeDefinitionId() },
          at: new Date().toISOString()
        }
      ];
      pendingMatchKeys.add(procKey);
      renderRequirements();
      return;
    }
    console.error('Fehler beim Anlegen des LLM-basierten Matchings:', err);
    pendingMatchKeys.delete(procKey);
    showBusinessAlert('Matching konnte nicht berechnet werden: ' + (err?.message || err), { title: 'Matching fehlgeschlagen' });
    renderRequirements();
  }
}

function ensureBulkMatchFilterModal() {
  let modal = document.getElementById('bulkMatchFilterModal');
  if (modal) return modal;

  modal = document.createElement('div');
  modal.id = 'bulkMatchFilterModal';
  modal.className = 'modal';
  modal.setAttribute('aria-hidden', 'true');
  modal.innerHTML = `
    <div class="modal-card match-filter-modal" role="dialog" aria-modal="true" aria-labelledby="bulkMatchFilterTitle">
      <div class="modal-header">
        <div class="modal-title" id="bulkMatchFilterTitle">Match-Filter für Matching-Auswahl</div>
        <div class="modal-close">
          <button class="icon-btn" type="button" data-match-filter-close title="Schließen">
            <svg viewBox="0 0 24 24"><path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/></svg>
          </button>
        </div>
      </div>
      <div class="modal-body">
        <label class="match-filter-toggle">
          <input type="checkbox" data-match-filter-enabled>
          <span>Nur Shortlist-Einträge ab Mindestscore hinzufügen</span>
        </label>
        <label class="match-filter-range">
          <span>Mindestscore <strong data-match-filter-value>70%</strong></span>
          <input type="range" min="0" max="100" step="5" data-match-filter-min-score>
        </label>
        <div class="modal-row" style="justify-content:flex-end;margin-top:14px">
          <button class="btn-pill" type="button" data-match-filter-save>Übernehmen</button>
        </div>
      </div>
    </div>
  `;
  appendMatchingLayer(modal);

  const close = () => {
    modal.classList.remove('open');
    modal.setAttribute('aria-hidden', 'true');
  };

  modal.querySelector('[data-match-filter-close]')?.addEventListener('click', close);
  modal.addEventListener('click', (event) => {
    if (event.target === modal) close();
  });
  modal.querySelector('[data-match-filter-min-score]')?.addEventListener('input', (event) => {
    const value = modal.querySelector('[data-match-filter-value]');
    if (value) value.textContent = `${clampScorePercent(event.target.value, 70)}%`;
  });
  modal.querySelector('[data-match-filter-save]')?.addEventListener('click', () => {
    const enabled = !!modal.querySelector('[data-match-filter-enabled]')?.checked;
    const minScore = clampScorePercent(modal.querySelector('[data-match-filter-min-score]')?.value, 70);
    setBulkMatchFilterSettings({ enabled, minScore });
    close();
    renderRequirements();
    renderObjects();
  });

  return modal;
}

function openBulkMatchFilterModal() {
  const modal = ensureBulkMatchFilterModal();
  const settings = getBulkMatchFilterSettings();
  const enabled = modal.querySelector('[data-match-filter-enabled]');
  const minScore = modal.querySelector('[data-match-filter-min-score]');
  const value = modal.querySelector('[data-match-filter-value]');
  if (enabled) enabled.checked = settings.enabled;
  if (minScore) minScore.value = String(settings.minScore);
  if (value) value.textContent = `${settings.minScore}%`;
  modal.classList.add('open');
  modal.setAttribute('aria-hidden', 'false');
}

function ensureMatchFilterModal() {
  let modal = document.getElementById('matchFilterModal');
  if (modal) return modal;

  modal = document.createElement('div');
  modal.id = 'matchFilterModal';
  modal.className = 'modal';
  modal.setAttribute('aria-hidden', 'true');
  modal.innerHTML = `
    <div class="modal-card match-filter-modal" role="dialog" aria-modal="true" aria-labelledby="matchFilterTitle">
      <div class="modal-header">
        <div>
          <div class="modal-title" id="matchFilterTitle">Match-Filter</div>
          <div class="muted" data-requirement-filter-subtitle style="font-size:12px;margin-top:2px"></div>
        </div>
        <div class="modal-close">
          <button class="icon-btn" type="button" data-requirement-filter-close title="Schließen">
            <svg viewBox="0 0 24 24"><path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/></svg>
          </button>
        </div>
      </div>
      <div class="modal-body">
        <label class="match-filter-toggle">
          <input type="checkbox" data-requirement-filter-enabled>
          <span>Filter für sichtbare Matches aktivieren</span>
        </label>
        <label class="match-filter-toggle">
          <input type="checkbox" data-requirement-filter-min-enabled>
          <span>Mindestscore anwenden</span>
        </label>
        <label class="match-filter-range">
          <span>Mindestscore <strong data-requirement-filter-min-value>70%</strong></span>
          <input type="range" min="0" max="100" step="5" data-requirement-filter-min-score>
        </label>
        <label class="match-filter-toggle">
          <input type="checkbox" data-requirement-filter-active-objects>
          <span>Nur aktive Objekte anzeigen</span>
        </label>
        <label class="match-filter-toggle">
          <input type="checkbox" data-requirement-filter-active-processes>
          <span>Nur aktive Prozesse anzeigen</span>
        </label>
        <div class="match-filter-statuses" data-requirement-filter-statuses></div>
        <div class="match-filter-result muted" data-requirement-filter-result></div>
        <div class="modal-row" style="justify-content:flex-end;margin-top:14px">
          <button class="btn-pill" type="button" data-requirement-filter-save>Ansicht speichern</button>
          <button class="btn-pill danger" type="button" data-requirement-filter-remove-hidden>Filter übernehmen</button>
        </div>
      </div>
    </div>
  `;
  appendMatchingLayer(modal);

  const statusWrap = modal.querySelector('[data-requirement-filter-statuses]');
  if (statusWrap) {
    statusWrap.innerHTML = ALL_MATCH_STATUSES.map((status) => `
      <label class="match-filter-status">
        <input type="checkbox" value="${status}">
        <span>${statusLabel(status)}</span>
      </label>
    `).join('');
  }

  const close = () => {
    modal.classList.remove('open');
    modal.setAttribute('aria-hidden', 'true');
    modal.dataset.requirementId = '';
  };
  const readSettings = () => ({
    enabled: !!modal.querySelector('[data-requirement-filter-enabled]')?.checked,
    minScoreEnabled: !!modal.querySelector('[data-requirement-filter-min-enabled]')?.checked,
    minScore: clampScorePercent(modal.querySelector('[data-requirement-filter-min-score]')?.value, 70),
    onlyActiveObjects: !!modal.querySelector('[data-requirement-filter-active-objects]')?.checked,
    onlyActiveProcesses: !!modal.querySelector('[data-requirement-filter-active-processes]')?.checked,
    statuses: Array.from(modal.querySelectorAll('[data-requirement-filter-statuses] input:checked')).map((input) => input.value)
  });
  const updatePreview = () => {
    const requirementId = modal.dataset.requirementId;
    const settings = normalizeMatchFilterSettings(readSettings());
    const requirementMatches = (matches || []).filter((m) => m.requirementId === requirementId && !m.removed);
    const visible = getVisibleMatchesAfterFilters(requirementId, settings, requirementMatches);
    const hidden = Math.max(0, requirementMatches.length - visible.length);
    const result = modal.querySelector('[data-requirement-filter-result]');
    if (result) result.textContent = `${visible.length} sichtbar · ${hidden} ausgeblendet`;
  };

  modal.querySelector('[data-requirement-filter-close]')?.addEventListener('click', close);
  modal.addEventListener('click', (event) => {
    if (event.target === modal) close();
  });
  modal.querySelector('[data-requirement-filter-min-score]')?.addEventListener('input', (event) => {
    const value = modal.querySelector('[data-requirement-filter-min-value]');
    if (value) value.textContent = `${clampScorePercent(event.target.value, 70)}%`;
    updatePreview();
  });
  modal.querySelectorAll('input').forEach((input) => input.addEventListener('change', updatePreview));
  modal.querySelector('[data-requirement-filter-save]')?.addEventListener('click', () => {
    const requirementId = modal.dataset.requirementId;
    setMatchFilterSettings(requirementId, readSettings());
    close();
    renderRequirements();
  });
  modal.querySelector('[data-requirement-filter-remove-hidden]')?.addEventListener('click', async () => {
    const requirementId = modal.dataset.requirementId;
    const settings = normalizeMatchFilterSettings(readSettings());
    setMatchFilterSettings(requirementId, settings);
    await removeMatchesHiddenByFilter(requirementId, settings);
    close();
  });

  return modal;
}

function openMatchFilterModal(requirementId) {
  const modal = ensureMatchFilterModal();
  const { requirement } = findRequirementAndSource(requirementId);
  const settings = getMatchFilterSettings(requirementId);
  modal.dataset.requirementId = String(requirementId || '');
  const subtitle = modal.querySelector('[data-requirement-filter-subtitle]');
  if (subtitle) subtitle.textContent = requirement?.title || String(requirementId || '');
  modal.querySelector('[data-requirement-filter-enabled]').checked = settings.enabled;
  modal.querySelector('[data-requirement-filter-min-enabled]').checked = settings.minScoreEnabled;
  modal.querySelector('[data-requirement-filter-min-score]').value = String(settings.minScore);
  modal.querySelector('[data-requirement-filter-min-value]').textContent = `${settings.minScore}%`;
  modal.querySelector('[data-requirement-filter-active-objects]').checked = settings.onlyActiveObjects;
  modal.querySelector('[data-requirement-filter-active-processes]').checked = settings.onlyActiveProcesses;
  const selectedStatuses = new Set(settings.statuses);
  modal.querySelectorAll('[data-requirement-filter-statuses] input').forEach((input) => {
    input.checked = selectedStatuses.has(input.value);
  });
  modal.classList.add('open');
  modal.setAttribute('aria-hidden', 'false');
  modal.querySelector('[data-requirement-filter-min-score]')?.dispatchEvent(new Event('input'));
}

function getVisibleMatchesAfterFilters(requirementId, settings, requirementMatches) {
  const all = Array.isArray(requirementMatches)
    ? requirementMatches
    : (matches || []).filter((m) => m.requirementId === requirementId && !m.removed);
  let visible = all.filter((m) => matchPassesRequirementFilter(m, getObject(m.objectId), settings));

  if (typeof applyRequirementGridFilters === 'function') {
    const visiblePairs = applyRequirementGridFilters(requirementId, visible.map((m) => ({
      cid: m.objectId,
      pct: scoreFromMatchItems(m.items) ?? 0,
      match: m
    })));
    const visibleKeys = new Set(
      visiblePairs.map(({ cid, match }) => key(match?.requirementId || requirementId, match?.objectId || cid))
    );
    visible = visible.filter((m) => visibleKeys.has(key(m.requirementId, m.objectId)));
  }

  return visible;
}

async function removeMatchesHiddenByFilter(requirementId, settings = getMatchFilterSettings(requirementId)) {
  const requirementMatches = (matches || []).filter((m) => m.requirementId === requirementId && !m.removed);
  const visibleKeys = new Set(
    getVisibleMatchesAfterFilters(requirementId, settings, requirementMatches)
      .map((m) => key(m.requirementId, m.objectId))
  );
  const hidden = requirementMatches.filter((m) => !visibleKeys.has(key(m.requirementId, m.objectId)));
  if (!hidden.length) {
    await showBusinessAlert('Es gibt keine ausgeblendeten Matches zum Entfernen.');
    return;
  }
  const ok = await showBusinessConfirm(`${hidden.length} ausgeblendete Match(es) für diesen Requirement entfernen?`, {
    title: 'Matches entfernen',
    confirmLabel: 'Entfernen',
  });
  if (!ok) return;

  try {
    if (rxdb && rxdb.matches) {
      for (const match of hidden) {
        const docs = await rxdb.matches.find({
          selector: { requirementId: match.requirementId, objectId: match.objectId }
        }).exec();
        for (const doc of Array.isArray(docs) ? docs : []) {
          if (doc && doc.remove) await doc.remove();
        }
      }
    }
    const removePairs = new Set(hidden.map((m) => key(m.requirementId, m.objectId)));
    matches = matches.filter((m) => !removePairs.has(key(m.requirementId, m.objectId)));
    hidden.forEach((m) => processes.delete(key(m.requirementId, m.objectId)));
    renderRequirements();
    renderMap();
  } catch (err) {
    console.error('removeMatchesHiddenByFilter Fehler:', err);
    await showBusinessAlert('Ausgeblendete Matches konnten nicht entfernt werden.', { title: 'Aktion fehlgeschlagen' });
  }
}

/**
 * Fügt für einen Requirement automatisch N passende Objekte hinzu.
 *
 * Ablauf:
 *  - Nimmt alle aktiven Objekte, die noch KEIN Match mit diesem Requirement haben.
 *  - Ruft shortlistObjectsForRequirement (LLM) auf, um Top-N IDs zu bekommen.
 *  - Erstellt für jede Shortlist-ID ein Match (createMatch), das dann den vollen LLM-Score berechnet.
 */
/**
 * Fügt für einen Requirement automatisch N passende Objekte hinzu.
 *
 * UX:
 *  - Die laufende Bulk-Operation wird für diesen Requirement als Loading-Status markiert.
 *  - Alle Shortlist-Objekte werden sofort als Matches angelegt (lokal),
 *    die Scores werden im Hintergrund per LLM nachgezogen.
 *  - Nach Abschluss wird der Loading-Status wieder freigegeben.
 */
async function handleBulkAutoMatch(requirementId, count) {
  const { requirement, source } = findRequirementAndSource(requirementId);
  if (!requirement || !source) {
    await showBusinessAlert('Anforderung oder Quelle für Matching-Auswahl nicht gefunden.');
    return;
  }

  // Wenn für diesen Requirement gerade schon ein Bulk-Lauf läuft → ignorieren
  if (bulkMatchingRequirements.has(requirementId)) {
    return;
  }

  // Requirement als "busy" markieren und UI neu rendern (Buttons gesperrt)
  bulkMatchingRequirements.add(requirementId);
  renderRequirements();

  try {
    // Objekte, die für diesen Requirement noch kein Match haben
    const alreadyMatchedIds = new Set(
      matches
        .filter(m => m.requirementId === requirementId && !m.removed)
        .map(m => m.objectId)
    );

    const availableObjectIds = objects
      .filter(c => !c.isPlaceholder && isObjectActive(c.id) && !alreadyMatchedIds.has(c.id))
      .map(c => c.id);

    if (!availableObjectIds.length) {
      await showBusinessAlert('Für diese Anforderung gibt es keine weiteren Objekte ohne bestehendes Match.');
      return;
    }

    // LLM-Shortlist holen
    const result = await shortlistObjectsForRequirement({
      llmChat,
      sourceId: source.id,
      requirementId,
      objectIds: availableObjectIds,
      topN: count,
      maxObjectsInPrompt: Math.min(availableObjectIds.length, 60)
    });

    const shortlistRaw = Array.isArray(result.shortlist) ? result.shortlist : [];
    const shortlist = filterShortlistByBulkMatchFilter(shortlistRaw);
    if (!shortlist.length) {
      const settings = getBulkMatchFilterSettings();
      await showBusinessAlert(settings.enabled
        ? `Keine Objekte erfüllen den Match-Filter ab ${settings.minScore}%.`
        : 'Das Modell hat keine passenden Objekte für diese Anforderung gefunden.');
      return;
    }

    // Für jede Shortlist-ID ein Match anlegen.
    // WICHTIG: wir warten nicht einzeln, damit alle sofort als Platzhalter erscheinen.
    const promises = [];

    for (const entry of shortlist) {
      const objectId = entry.objectId;
      if (!objectId) continue;

      // zur Sicherheit: doppelte Matches vermeiden
      const exists = matches.some(
        m =>
          m.requirementId === requirementId &&
          m.objectId === objectId &&
          !m.removed
      );
      if (exists) continue;

      // createMatch:
      //  - legt sofort ein lokales Match mit pending-Status an
      //  - startet dann async computeRequirementMatch (LLM)
      const p = createMatch(source.id, requirementId, objectId).catch(err => {
        console.error('Fehler im Bulk-Matching createMatch:', err);
      });
      promises.push(p);
    }

    // Direkt nach dem Start der createMatch-Calls neu rendern:
    // → alle neuen Objekte sind als "pending" sichtbar.
    renderRequirements();

    // Hintergrund: warten, bis alle LLM-Matches fertig sind (für sauberen finalen Stand)
    await Promise.allSettled(promises);

    renderRequirements();
  } catch (e) {
    console.error('handleBulkAutoMatch Fehler:', e);
    await showBusinessAlert('Matching-Auswahl (+Objekte) ist fehlgeschlagen: ' + (e?.message || e), { title: 'Matching fehlgeschlagen' });
  } finally {
    // Requirement aus "busy"-Set entfernen und Buttons wieder freigeben
    bulkMatchingRequirements.delete(requirementId);
    renderRequirements();
  }
}

async function handleBulkAutoMatchForObject(objectId, count) {
  const object = getObject(objectId);
  if (!object) {
    await showBusinessAlert('Objekt nicht gefunden.');
    return;
  }
  if (object.isPlaceholder) {
    await showBusinessAlert('Matching-Auswahl ist erst verfügbar, wenn der Objektimport abgeschlossen ist.');
    return;
  }

  // Wenn für diesen Objekte gerade schon ein Bulk-Lauf läuft → ignorieren
  if (bulkMatchingObjects.has(objectId)) {
    return;
  }

  bulkMatchingObjects.add(objectId);
  renderObjects();

  try {
    // Requirements sammeln: nur aktive Sources + aktive Requirements
    const allRequirements = [];
    (sources || []).forEach(comp => {
      if (!comp) return;
      if (!isSourceActive(comp.id)) return;

      (comp.requirements || []).forEach(j => {
        if (!j || !j.id) return;
        // optional: Requirement-spezifische Aktivität falls vorhanden -> hier nicht, da UI-requirements keine active-Flag haben
        allRequirements.push({ requirementId: j.id, sourceId: j.sourceId || comp.id });
      });
    });

    if (!allRequirements.length) {
      await showBusinessAlert('Keine Requirements gefunden.');
      return;
    }

    // Requirements rausfiltern, die für diesen Objekte schon ein Match haben
    const alreadyMatchedRequirementIds = new Set(
      (matches || [])
        .filter(m => m.objectId === objectId && !m.removed)
        .map(m => m.requirementId)
    );

    const availableRequirements = allRequirements.filter(j => !alreadyMatchedRequirementIds.has(j.requirementId));

    if (!availableRequirements.length) {
      await showBusinessAlert('Für diese:n Objekt gibt es keine weiteren Requirements ohne bestehendes Match.');
      return;
    }

    // Shortlist (Objekt -> Requirements)
    // WICHTIG: Du musst shortlistRequirementsForObject importieren:
    // shortlistRequirementsForObject kommt aus matchingTools.js.
    const result = await shortlistRequirementsForObject({
      llmChat,
      objectId: objectId,
      requirementIds: availableRequirements.map(j => j.requirementId),
      topN: count,
      maxRequirementsInPrompt: Math.min(availableRequirements.length, 60)
    });

    const shortlistRaw = Array.isArray(result.shortlist) ? result.shortlist : [];
    const shortlist = filterShortlistByBulkMatchFilter(shortlistRaw);
    if (!shortlist.length) {
      const settings = getBulkMatchFilterSettings();
      await showBusinessAlert(settings.enabled
        ? `Keine Requirements erfüllen den Match-Filter ab ${settings.minScore}%.`
        : 'Das Modell hat keine passenden Requirements für diese:n Objekt gefunden.');
      return;
    }

    const byRequirementId = new Map(availableRequirements.map(j => [j.requirementId, j.sourceId]));

    // Matches anlegen (je Shortlist-Requirement ein createMatch)
    const promises = [];

    for (const entry of shortlist) {
      const requirementId = entry.requirementId;
      if (!requirementId) continue;

      const sourceId = byRequirementId.get(requirementId);
      if (!sourceId) continue;

      // doppelte Matches vermeiden
      const exists = (matches || []).some(
        m => m.requirementId === requirementId && m.objectId === objectId && !m.removed
      );
      if (exists) continue;

      const p = createMatch(sourceId, requirementId, objectId).catch(err => {
        console.error('Fehler im Object->Requirements Bulk-Matching createMatch:', err);
      });
      promises.push(p);
    }

    // sofort sichtbar machen
    renderRequirements();
    renderObjects();

    // sauber finalisieren
    await Promise.allSettled(promises);

    renderRequirements();
    renderObjects();
  } catch (e) {
    console.error('handleBulkAutoMatchForObject Fehler:', e);
    await showBusinessAlert('Matching-Auswahl (+Requirements) ist fehlgeschlagen: ' + (e?.message || e), { title: 'Matching fehlgeschlagen' });
  } finally {
    bulkMatchingObjects.delete(objectId);
    renderObjects();
  }
}



// Matching (Requirement-Objekt) vollständig entfernen (hart löschen)
async function removeMatch(requirementId, objectId){
  try {
    if (rxdb && rxdb.matches){
      const docs = await rxdb.matches.find({ selector: { requirementId, objectId: objectId } }).exec();
      if (Array.isArray(docs)) {
        for (const d of docs) {
          if (d && d.remove) await d.remove();
        }
      } else if (docs && docs.remove) {
        await docs.remove();
      }
    }

    matches = matches.filter(m => !(m.requirementId === requirementId && m.objectId === objectId));
    processes.delete(key(requirementId, objectId));

    renderRequirements();
    renderMap();
  } catch (e){
    console.error('Fehler beim Entfernen des Matchings', e);
    await showBusinessAlert('Matching konnte nicht entfernt werden.', { title: 'Aktion fehlgeschlagen' });
  }
}

// Objekt löschen (inkl. automatischer Match-Bereinigung)
async function handleDeleteObject(objectId){
  const object = getObject(objectId);
  const dependentMatches = getMatchesForObject(objectId);
  const totalToDelete = 1 + dependentMatches.length;

  const confirmed = await confirmDeletionWithCount({
    subject: `Objekt "${object?.name || objectId}"`,
    dependencyLines: getMatchSummaryLines(dependentMatches),
    totalItemsToDelete: totalToDelete
  });

  if (!confirmed) return;

  try {
    await removeMatchDocsFromDb(dependentMatches);
    purgeMatchesFromState(dependentMatches);

    if (rxdb && rxdb.objects){
      const doc = await rxdb.objects.findOne({ selector: { id: objectId } }).exec();
      if (doc) await doc.remove();
    }

    objects = objects.filter(c => c.id !== objectId);

    for (const k of Array.from(processes.keys())){
      const parts = k.split(':');
      const objectPart = parts[1];
      if (String(objectPart) === String(objectId)){
        processes.delete(k);
      }
    }

    matches = matches.filter(m => m.objectId !== objectId);
    if (selectedObject === objectId) selectedObject = null;
    if (matrixSelectedObjectId === objectId) matrixSelectedObjectId = null;
    persistMatchingRuntimeState();

    renderObjects();
    renderRequirements();
    renderMap();
  } catch (e){
    console.error('Fehler beim Löschen des Objekte', e);
    await showBusinessAlert('Objekt konnte nicht gelöscht werden (siehe Konsole).', { title: 'Aktion fehlgeschlagen' });
  }
}

// Requirement löschen (inkl. automatischer Match-Bereinigung)
async function handleDeleteRequirement(requirementId){
  const { requirement, source } = findRequirementAndSource(requirementId);
  const dependentMatches = getMatchesForRequirement(requirementId);
  const totalToDelete = 1 + dependentMatches.length;

  const confirmed = await confirmDeletionWithCount({
    subject: `Anforderung "${requirement?.title || requirementId}"`,
    dependencyLines: [
      ...(source ? [`- Zugehöriges Quellen: ${source.name}`] : []),
      ...getMatchSummaryLines(dependentMatches)
    ],
    totalItemsToDelete: totalToDelete
  });
  if (!confirmed) return;

  try {
    await removeMatchDocsFromDb(dependentMatches);
    purgeMatchesFromState(dependentMatches);
    purgeProcessesForRequirementIds([requirementId]);

    if (rxdb && rxdb.requirements){
      const doc = await rxdb.requirements.findOne({ selector: { id: requirementId } }).exec();
      if (doc) await doc.remove();
    }

    sources.forEach(c => {
      c.requirements = (c.requirements || []).filter(j => j.id !== requirementId);
    });

    for (const k of Array.from(processes.keys())){
      const [jid] = k.split(':');
      if (String(jid) === String(requirementId)) processes.delete(k);
    }
    matches = matches.filter(m => m.requirementId !== requirementId);
    if (activeRequirementForScoring === requirementId) activeRequirementForScoring = null;
    persistMatchingRuntimeState();

    renderSources();
    renderRequirements();
    renderMap();
  } catch (e){
    console.error('Fehler beim Löschen der Anforderung', e);
    await showBusinessAlert('Anforderung konnte nicht gelöscht werden (siehe Konsole).', { title: 'Aktion fehlgeschlagen' });
  }
}

// Quellen löschen (inkl. automatischer Requirement- und Match-Bereinigung)
async function handleDeleteSource(sourceId){
  const comp = sources.find(c => c.id === sourceId);
  if (!comp) return;
  const requirementsToDelete = Array.isArray(comp.requirements) ? comp.requirements.map(j => j.id).filter(Boolean) : [];
  const uniqueRequirementIds = [...new Set(requirementsToDelete.map(id => String(id)))];
  const dependentMatches = getMatchesForRequirements(uniqueRequirementIds);
  const totalToDelete = 1 + uniqueRequirementIds.length + dependentMatches.length;

  const confirmed = await confirmDeletionWithCount({
    subject: `Quellen "${comp.name}"`,
    dependencyLines: [
      `- ${uniqueRequirementIds.length} Anforderung${uniqueRequirementIds.length === 1 ? '' : 'en'}`,
      ...getMatchSummaryLines(dependentMatches)
    ],
    totalItemsToDelete: totalToDelete
  });
  if (!confirmed) return;

  try {
    await removeMatchDocsFromDb(dependentMatches);
    purgeMatchesFromState(dependentMatches);
    purgeProcessesForRequirementIds(uniqueRequirementIds);
    await removeRequirementsFromDb(uniqueRequirementIds);

    if (rxdb && rxdb.sources){
      const doc = await rxdb.sources.findOne({ selector: { id: sourceId } }).exec();
      if (doc) await doc.remove();
    }

    sources = sources.filter(c => c.id !== sourceId);
    if (activeSource === sourceId){
      activeSource = null;
    }
    if (activeRequirementForScoring && !sources.some(c => (c.requirements || []).some(j => String(j.id) === String(activeRequirementForScoring)))) {
      activeRequirementForScoring = null;
    }
    persistMatchingRuntimeState();

    renderSources();
    renderRequirements();
    renderMap();
  } catch (e){
    console.error('Fehler beim Löschen des Quellens', e);
    await showBusinessAlert('Quellen konnte nicht gelöscht werden (siehe Konsole).', { title: 'Aktion fehlgeschlagen' });
  }
}


/* --------- Render: Quellen links --------- */
function normalizeListSearchText(value){
  return String(value || '')
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '');
}

function sourceMatchesSearch(source, query){
  const q = normalizeListSearchText(query).trim();
  if (!q) return true;

  const requirementsText = (source.requirements || []).map(requirement => {
    const details = requirement && requirement.details && typeof requirement.details === 'object'
      ? Object.values(requirement.details).join(' ')
      : '';
    return [
      requirement?.title,
      requirement?.location,
      requirement?.level,
      requirement?.type,
      requirement?.desc,
      details
    ].join(' ');
  }).join(' ');

  const locText = (source.locations || []).map(loc => loc?.city || '').join(' ');
  const haystack = normalizeListSearchText([
    source.name,
    source.industry,
    source.website,
    locText,
    requirementsText
  ].join(' '));

  return haystack.includes(q);
}

function renderSources(){
  const grid = $('#sourceGrid'); if (!grid) return;
  grid.innerHTML = '';

  if (!sources.length){
    // If the underlying RxDB collection actually has documents but the
    // aggregation produced none yet, we're mid-sync (peer data arrived but
    // hasn't been projected into the UI shape). Show a sync state instead of
    // the misleading "empty database" message.
    if (hasUnsyncedMatchingData()) {
      grid.innerHTML = '<div class="muted" style="padding:8px">Daten werden synchronisiert…</div>';
    } else {
      grid.innerHTML = '<div class="muted" style="padding:8px">Keine Anforderungen in der Datenbank gefunden.</div>';
    }
    return;
  }

  const prevActiveSource = activeSource;
  if (activeSource && !sources.some(c => c.id === activeSource)) {
    activeSource = null;
  }
  if (prevActiveSource !== activeSource) {
    persistMatchingRuntimeState();
  }

  const sourceSearchEl = $('#sourceSearch');
  const sourceSearchQuery = sourceSearchEl ? sourceSearchEl.value : String(matchingViewState.sourceSearch || '');
  const visibleSources = sources.filter(c => sourceMatchesSearch(c, sourceSearchQuery));

  if (!visibleSources.length){
    grid.innerHTML = '<div class="muted" style="padding:8px">Keine Anforderungen im aktuellen Suchfilter.</div>';
    return;
  }

  visibleSources.forEach(c => {
    const total = (c.locations || []).reduce((s,l)=>s+l.open,0);
    const hasRel = hasSourceRel(c.id);
    const active = isSourceActive(c.id);

    const card = el(
      'div',
      'source-card' +
      (c.id===activeSource ? ' selected' : '') +
      (hasRel ? ' has-rel' : ' needs-contact') +
      (!active ? ' inactive-entity' : '')
    );

    card.innerHTML = `
      <div class="source-head">
        <div class="logo rel-dot">
          ${c.logoUrl
            ? `<img src="${c.logoUrl}" alt="${c.name} Logo"/>`
            : `<span style="color:var(--primary-2);font-weight:700">${(c.name[0]||'?')}</span>`}
        </div>
        <div class="source-main">
          <div class="cname">${c.name}</div>
          <div class="toggle-row">
            <div class="switch ${active?'is-on':''}" data-source-toggle="${c.id}">
              <div class="switch-knob"></div>
            </div>
            <span class="toggle-label">${active?'Aktiv':'Inaktiv'}</span>
          </div>
        </div>
        <span class="total" title="Gesamt offene Anforderungn">${total}</span>
        <button class="icon-btn" data-del-comp title="Quellen löschen">
          <svg viewBox="0 0 24 24">
            <path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/>
          </svg>
        </button>
      </div>
      <div class="locations" data-id="${c.id}"></div>
      <div class="card-footer">
        <span class="meta">${(c.locations||[]).length} Standort${(c.locations||[]).length===1?'':'e'}</span>
        <span class="more" data-more="${c.id}"></span>
      </div>`;
    // Klick auf Punkt / Logo öffnet Kontakt-Modal (Quelle)
    const relTarget = card.querySelector('.logo.rel-dot');
    if (relTarget){
      relTarget.title = hasRel ? 'Kontaktstatus bearbeiten' : 'Kontaktstatus setzen';
      relTarget.addEventListener('click', (e)=>{
        e.stopPropagation();
        openRelationModal('source', c.id);
      });
    }

    card.title = hasRel? 'Beziehung vorhanden' : 'Noch kein Kontakt';

    card.addEventListener('click', (e)=>{
      if(e.target.closest('.more')) return;
      if(e.target.closest('[data-del-comp]')) return;
      if(e.target.closest('.switch[data-source-toggle]')) return;
      activeSource = activeSource === c.id ? null : c.id;
      persistMatchingRuntimeState();
      renderSources();
      renderRequirements();
      renderMap();
      renderObjects();
    });

    const delBtn = card.querySelector('[data-del-comp]');
    if (delBtn){
      delBtn.addEventListener('click', (e)=>{
        e.stopPropagation();
        handleDeleteSource(c.id);
      });
    }

    const compToggle = card.querySelector('.switch[data-source-toggle]');
    if (compToggle){
      compToggle.addEventListener('click', e=>{
        e.stopPropagation();
        const next = !isSourceActive(c.id);
        setSourceActive(c.id, next);
      });
    }

    grid.appendChild(card);

    const locWrap = card.querySelector('.locations');
    const more = card.querySelector('[data-more]');
    const MAX=3; const showAll=c._expanded;
    const locs = c.locations || [];
    (showAll?locs:locs.slice(0,MAX)).forEach(l=>{
      const sp=el('span','loc');
      sp.innerHTML=`${l.city} <b>${l.open}</b>`;
      locWrap.appendChild(sp);
    });
    if(locs.length>MAX){
      more.textContent=c._expanded?'weniger':`+${locs.length-MAX} mehr`;
      more.onclick=(ev)=>{ev.stopPropagation(); c._expanded=!c._expanded; renderSources();};
    } else {
      more.textContent='';
    }
  });
}

function renderRequirements(){
  const compNameEl = $('#sourceName');
  const list = $('#requirementList');
  const searchEl = $('#requirementSearch');
  const filterEl = $('#requirementFilter');
  if (!list || !compNameEl || !searchEl || !filterEl) return;

  // Executive Summary: 4 Zeilen mit mehr Vorschautext
  function buildExecBlock(execInfo){
    const exec = execInfo || {};
    const MAX_LEN = 180; // mehr Vorschautext
    const escapeHtml = (str) => String(str || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');

    const fields = [
      exec.fachlicheQualifikation,
      exec.methodenKompetenz,
      exec.leadershipFaehigkeit,
      exec.gehaltswunschUndOrt
    ];

    const lines = fields
      .map(full => {
        full = (full || '').trim();
        if (!full) return '';
        const short = full.length > MAX_LEN
          ? full.slice(0, MAX_LEN).trimEnd() + '…'
          : full;
        return `
          <div
            class="object-mini-line"
            title="${escapeHtml(full)}"
            style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:12px;line-height:1.4;"
          >
            ${escapeHtml(short)}
          </div>
        `;
      })
      .filter(Boolean);

    if (!lines.length) return '';

    return `
      <div class="object-mini-lines"
           style="display:flex;flex-direction:column;gap:2px;">
        ${lines.join('')}
      </div>
    `;
  }

  // lokale Escape-Helper für Tooltip/HTML
  function escapeHtml(str){
    return String(str || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  if (!sources.length){
    compNameEl.textContent = 'Keine Anforderungen';
    list.innerHTML = '';
    return;
  }

  const selectedComp = activeSource ? sources.find(c=>c.id===activeSource) || null : null;
  const sourceRows = selectedComp ? [selectedComp] : sources;
  compNameEl.textContent = selectedComp ? selectedComp.name : 'Alle Quellen';

  const q = searchEl.value || '';
  const locFilterFallback =
    filterEl.dataset.persistedValue ||
    String(matchingViewState.requirementFilter || 'all');
  const locFilter = filterEl.value || locFilterFallback || 'all';
  list.innerHTML='';

  const locs = [...new Set(sourceRows.flatMap(comp => (comp.requirements || []).map(j=>j.location)).filter(Boolean))];
  filterEl.innerHTML = '<option value="all">Alle Standorte</option>' +
    locs.map(l=>`<option value="${l}">${l}</option>`).join('');
  if (locs.includes(locFilter)) {
    filterEl.value = locFilter;
  } else {
    filterEl.value = 'all';
  }
  filterEl.dataset.persistedValue = filterEl.value || 'all';

  const visibleRequirementRows = sourceRows
    .flatMap(comp => (comp.requirements || []).map(j => ({ requirement: j, source: comp })))
    .filter(({ requirement }) => (locFilter==='all' || requirement.location===locFilter))
    .filter(({ requirement, source }) => matchesFullTextSearch(buildRequirementSearchPayload(requirement, source), q));

  if (!visibleRequirementRows.length) {
    list.innerHTML = '<div class="muted" style="padding:8px">Keine Anforderungen im aktuellen Filter.</div>';
    return;
  }

  visibleRequirementRows.forEach(({ requirement: j, source: rowComp })=>{
      const sourceIsActiveFlag = isSourceActive(rowComp.id);
      const wrap = el(
        'div',
        'requirement' +
        (activeRequirementForScoring === j.id ? ' selected selected-for-score' : '')
      );
      wrap.dataset.requirementId = j.id;

      if (!sourceIsActiveFlag) wrap.classList.add('inactive-entity');
      if (bulkMatchingRequirements.has(j.id)) wrap.classList.add('requirement-bulk-loading');

      const head = el('div','requirement-top');

      head.innerHTML = `
        <button class="requirement-link" title="Anforderung öffnen/schließen">
          <svg viewBox="0 0 24 24"><path d="M14 3h7v7h-2V6.41l-9.29 9.3-1.42-1.42 9.3-9.29H14V3z"/></svg>
        </button>
        <div>
          <div class="requirement-title">${j.title}</div>
          <div class="requirement-meta">${selectedComp ? '' : `${rowComp.name} · `}${j.location} · ${j.level || 'Mid'} · ${j.type || 'Vollzeit'}</div>
        </div>
        <div class="space"></div>
        <button class="icon-btn" data-del-requirement title="Anforderung löschen">
          <svg viewBox="0 0 24 24">
            <path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/>
          </svg>
        </button>`;

      wrap.appendChild(head);

      const tagRow = el('div','tag-row');
      (j.tags||[]).forEach(t=>{
        const chip=el('span','tag');
        chip.textContent=t;
        tagRow.appendChild(chip);
      });
      wrap.appendChild(tagRow);

      const desc = el('div');
      desc.className='requirement-meta';
      desc.style.marginTop='6px';
      desc.textContent=j.desc || '';
      wrap.appendChild(desc);

      const allPairs = matches
        .filter(m => m.requirementId === j.id && !m.removed)
        .map(m => ({
          cid: m.objectId,
          pct: scoreFromMatchItems(m.items) ?? 0,
          match: m
        }))
        .sort((a,b)=>b.pct-a.pct);
      const matchFilter = getMatchFilterSettings(j.id);
      let pairs = allPairs.filter(({ cid, match }) => matchPassesRequirementFilter(match, getObject(cid), matchFilter));

      if(pairs.length===0){
        const empty = el('div','requirement-meta');
        empty.style.marginTop='10px';
        empty.innerHTML = allPairs.length && matchFilter.enabled
          ? 'Keine Matchings im aktuellen Filter sichtbar.'
          : 'Keine Matchings vorhanden.';
        wrap.appendChild(empty);
      } else {
        const grid = el('div','badge-grid');
        pairs.forEach(({cid,pct})=>{
          const c = getObject(cid); if(!c) return;

          const procKey = key(j.id,cid);
          let state = processes.get(procKey);
          if (!state){
            const matchState = matches.find(m => m.requirementId === j.id && m.objectId === cid && !m.removed);
            state = {
              progress: matchState ? (typeof matchState.progress === 'number' ? matchState.progress : INITIAL_PROGRESS) : INITIAL_PROGRESS,
              notes: matchState ? (matchState.notes || '') : '',
              active: matchState ? !!matchState.active : true,
              statuses: matchState && Array.isArray(matchState.statuses) ? matchState.statuses : [],
              status: matchState && matchState.status ? matchState.status : 'prematch'
            };
            processes.set(procKey, state);
          }

          const objectActive = isObjectActive(cid);
          const matchIsActive = state.active && objectActive;
          const showProgress = matchIsActive;
          const progressVal = state.progress || 0;

          const badgeClasses = [
            'object-badge',
            hasObjectRel(cid)? 'has-rel':'needs-contact'
          ];
          if (!matchIsActive) badgeClasses.push('inactive-entity');

          const b = el('div', badgeClasses.join(' '));
          b.dataset.cid=cid; b.dataset.jid=j.id;

          // Layout für die Badge
          b.style.display = 'flex';
          b.style.alignItems = 'flex-start';
          b.style.gap = '12px';

          const activeStatuses = Array.isArray(state.statuses) ? state.statuses : [];
          const tagsHtml = ALL_MATCH_STATUSES.map(st => {
            const isOn = activeStatuses.includes(st);
            const cls = 'status-tag' + (isOn ? ' is-active' : '');
            return `<span class="${cls}" data-status="${st}">${statusLabel(st)}</span>`;
          }).join('');

          const isPending = pendingMatchKeys.has(procKey);
          const matchObj = matches.find(m => m.requirementId === j.id && m.objectId === cid && !m.removed) || null;
          const hasScore = hasScoredMatchItems(matchObj?.items) && typeof pct === 'number';
          const bucket = gradeBucket(pct);

          let scoreInner;
          if (isPending){
            scoreInner = '<span class="score-spinner" aria-label="Matching wird berechnet…"></span>';
          } else if (hasScore){
            const overall = (typeof getOverallConflictInfo === 'function')
              ? getOverallConflictInfo(matchObj?.items)
              : null;

            if (overall && overall.icons){
              const tipAttr = overall.tooltip ? ` title="${escapeHtml(overall.tooltip)}"` : '';
              scoreInner = `<s>${escapeHtml(pct)}%</s><span class="conflict-icons" style="margin-left:6px"${tipAttr}>${escapeHtml(overall.icons)}</span>`;
            } else {
              scoreInner = `${pct}%`;
            }
          } else {
            scoreInner = '—';
          }

          const execBlockHtml = buildExecBlock(c.executiveInfo || {});
          const objectFallback = getObjectFallbackAvatarUrl(c.id, c.name);
          const objectSrc = normalizeImageSrc(c.photo) || '';


          b.innerHTML = `
            <div class="object-left" style="display:flex;flex-direction:column;align-items:center;gap:6px;">
                <div class="object-photo rel-dot">
                  ${safeImgHtml({
                    src: objectSrc,
                    fallbackSrc: objectFallback,
                    alt: c.name,
                    style: 'width:100%;height:100%;object-fit:cover'
                  })}
                </div>

              <div class="toggle-row small">
                <div class="switch ${objectActive?'is-on':''}" data-object-toggle="${c.id}">
                  <div class="switch-knob"></div>
                </div>
              </div>
            </div>

            <div class="object-main" style="display:flex;flex-direction:column;gap:6px;flex:1;min-width:0;">
              <div class="object-header-row" style="display:flex;align-items:flex-start;gap:8px;">
                <div class="object-name"
                     style="flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-weight:600;">
                  ${c.name}
                </div>
                <span
                  class="score ${isPending ? 'is-loading' : ''}"
                  data-pct="${bucket}"
                  data-match-key="${procKey}"
                >${scoreInner}</span>
              </div>

              <div class="object-mini">
                ${execBlockHtml}
              </div>

              <div class="badge-actions" style="margin-top:2px;">
                <button class="icon-btn ${state.active?'active':''}" title="${state.active?'Prozess pausieren':'Prozess aktivieren'}" data-play>
                  ${state.active
                    ? '<svg viewBox="0 0 24 24"><path d="M6 5h4v14H6zM14 5h4v14h-4z"/></svg>'
                    : '<svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>'}
                </button>
                <button class="icon-btn notes-action-btn" title="Notizen und Fortschritt bearbeiten" aria-label="Notizen und Fortschritt bearbeiten" data-notes>
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h9.5L19 8.5V20a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zm8.5 1.8V10h4.2L13.5 5.8zM7 13h6v2H7v-2zm0 4h8v2H7v-2z"/></svg>
                </button>
                <button class="icon-btn match-open-btn" title="Match anzeigen" aria-label="Match anzeigen" data-object>
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5c5.2 0 8.6 4.1 10 7-1.4 2.9-4.8 7-10 7S3.4 14.9 2 12c1.4-2.9 4.8-7 10-7zm0 2C8.3 7 5.6 9.4 4.3 12c1.3 2.6 4 5 7.7 5s6.4-2.4 7.7-5C18.4 9.4 15.7 7 12 7zm0 2.2a2.8 2.8 0 1 1 0 5.6 2.8 2.8 0 0 1 0-5.6z"/></svg>
                </button>

                <button class="icon-btn danger" title="Matching entfernen" data-remove>
                  <svg viewBox="0 0 24 24"><path d="M6 7h12l-1 14H7L6 7zm3-4h6l1 2H8l1-2z"/></svg>
                </button>
              </div>

              <div class="progress-line ${showProgress?'show':''}" data-open-notes style="margin-top:2px;">
                <i style="width:${progressVal}%"></i>
              </div>

              <div class="status-tags" data-open-notes style="margin-top:4px;">
                ${tagsHtml}
              </div>
            </div>
          `;

          const scoreEl = b.querySelector('.score');
          if (scoreEl && !isPending){
            scoreEl.addEventListener('click', (e)=>{
              e.stopPropagation();
              toggleRequirementAndObject(j, cid);
            });
          }

          const relTarget = b.querySelector('.object-photo.rel-dot');
          if (relTarget){
            relTarget.title = hasObjectRel(cid)
              ? 'Kontaktstatus bearbeiten'
              : 'Kontaktstatus setzen';
            relTarget.addEventListener('click', (ev)=>{
              ev.stopPropagation();
              openRelationModal('object', cid);
            });
          }

          const objectToggle = b.querySelector('.switch[data-object-toggle]');
          if (objectToggle){
            objectToggle.addEventListener('click', ev=>{
              ev.stopPropagation();
              const next = !isObjectActive(c.id);
              setObjectActive(c.id, next);
            });
          }

          b.querySelector('[data-play]').addEventListener('click', (e)=>{
            e.stopPropagation();
            const s = processes.get(procKey) || {progress:INITIAL_PROGRESS, notes:'', active:false};
            s.active = !s.active;
            if(s.active && (!s.progress || s.progress===0)) s.progress = DEFAULT_PROGRESS;
            processes.set(procKey, s);
            updateMatchState(j.id, cid, { active: s.active, progress: s.progress });
            renderRequirements();
          });

          const openNotes = ()=> openNoteModal(j.id, cid);
          b.querySelector('[data-notes]').addEventListener('click', (e)=>{
            e.stopPropagation();
            openNotes();
          });
          b.querySelectorAll('[data-open-notes]').forEach(node=>{
            node.addEventListener('click', (e)=>{
              e.stopPropagation();
              openNotes();
            });
          });

          b.querySelector('[data-object]').addEventListener('click', (e)=>{
            e.stopPropagation();
            toggleRequirementAndObject(j, cid);
          });

          b.querySelector('[data-remove]').addEventListener('click', (e)=>{
            e.stopPropagation();
            removeMatch(j.id, cid);
          });

          grid.appendChild(b);
          hydrateImages(grid);

        });
        wrap.appendChild(grid);
        hydrateImages(grid);

      }

      head.querySelector('.requirement-link').addEventListener('click', (e)=>{
        e.stopPropagation();
        toggleRequirementPanel(j);
      });

      const delBtn = head.querySelector('[data-del-requirement]');
      if (delBtn){
        delBtn.addEventListener('click', (e)=>{
          e.stopPropagation();
          handleDeleteRequirement(j.id);
        });
      }

      wrap.addEventListener('click', (e)=>{
        if (
          e.target.closest('.icon-btn') ||
          e.target.closest('.object-badge') ||
          e.target.closest('.switch[data-source-toggle]')
        ) {
          return;
        }

        if (activeRequirementForScoring === j.id) {
          activeRequirementForScoring = null;
        } else {
          activeRequirementForScoring = j.id;
        }

        persistMatchingRuntimeState();
        renderRequirements();
        renderObjects();
      });

      list.appendChild(wrap);
    });
    hydrateImages(list);

}





/* --------- Modal-Logik (Notizen/Progress) --------- */
// --- Zentrale Detail-Spalte im Notiz-Modal --------------------------

function ensureNoteMatchDetailHost(){
  if (!noteModal) return null;

  let host = document.getElementById('noteMatchDetail');
  if (host) return host;

  const body = noteModal.querySelector('.modal-body');
  if (!body) return null;

  const saveRow = body.querySelector('.row:last-of-type') || null;

  host = document.createElement('div');
  host.id = 'noteMatchDetail';
  host.className = 'note-match-detail';

  if (saveRow){
    body.insertBefore(host, saveRow);
  } else {
    body.appendChild(host);
  }
  return host;
}



function renderMatchDetailPanelInNoteModal(){
  const host = ensureNoteMatchDetailHost();
  if (!host) return;

  const escapeHtml = (str) => String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

  if (!currentMatchDetail || !currentMatchDetail.items || !currentMatchDetail.items.length){
    host.innerHTML = '';
    host.style.display = 'none';
    return;
  }

  const { items, colorByRequirement } = currentMatchDetail;
  host.style.display = 'block';

  host.innerHTML = items.map((it, idx)=>{
    const color = colorByRequirement[it.requirementId] || MATCH_COLORS[idx % MATCH_COLORS.length];

    const dimMap = {
      education: 'Ausbildung',
      experience: 'Erfahrung',
      skill: 'Skill',
      language: 'Sprache',
      other: 'Sonstiges'
    };
    const prioMap = {
      base: 'Basis',
      performance: 'Leistung',
      enthusiasm: 'Begeisterung'
    };
    const levelMap = {
      full: 'voll',
      partial: 'teilweise',
      none: 'kein'
    };

    const dimLabel   = dimMap[it.dimension] || it.dimension || 'Sonstiges';
    const prioLabel  = prioMap[it.priority] || it.priority || '';
    const levelLabel = levelMap[it.matchLevel] || it.matchLevel || '';

  const requirementSnip = it.jobSnippet || it.requirementSnippet || '';
  const objectSnip  = it.cvSnippet || it.objectSnippet || '';
    const expl    = it.explanation || '';

    const pct = (()=>{
      if (typeof it.matchScore === 'number'){
        const raw = it.matchScore;
        if (raw <= 1 && raw >= 0) return Math.round(raw * 100);
        return Math.round(raw);
      }
      if (typeof it.matchScoreKey === 'number'){
        return Math.round(it.matchScoreKey);
      }
      if (it.matchLevel === 'full')    return 100;
      if (it.matchLevel === 'partial') return 60;
      if (it.matchLevel === 'none')    return 0;
      return null;
    })();

    const hasConflict = hasConflictOnItem(it);
    const conflictIcons = getConflictIconsFromItem(it);
    const conflictTip = getConflictTooltipFromItem(it);
    const tipAttr = conflictTip ? ` title="${escapeHtml(conflictTip)}"` : '';

    const pctHtml = (pct != null)
      ? (hasConflict
          ? `<s>${pct}%</s><span class="conflict-icons" style="margin-left:4px"${tipAttr}>${escapeHtml(conflictIcons.join(''))}</span>`
          : `${pct}%`)
      : '';

    return `
      <article class="match-item-card" data-req="${it.requirementId}" style="--matchColor:${color}">
        <div class="match-item-header">
          <div class="match-item-badge"></div>
          <div>
            <div class="match-item-title">${it.title || 'Anforderung'}</div>
            <div class="match-item-meta">
              ${dimLabel}
              ${prioLabel ? ` · Priorität: ${prioLabel}` : ''}
              ${levelLabel ? ` · Match: ${levelLabel}` : ''}
              ${pctHtml ? ` · ${pctHtml}` : ''}
            </div>
          </div>
        </div>
        ${expl ? `<div class="match-item-expl">${expl}</div>` : ''}
        <div class="match-item-snips">
          <div class="match-snippet-block">
            <div class="match-snippet-label">Stellenausschreibung</div>
            <div>${requirementSnip ? `<span class="match-highlight" data-match-req="${it.requirementId}" style="--matchColor:${color}">${requirementSnip}</span>` : '<span class="muted">Kein Nachweis.</span>'}</div>
          </div>
          <div class="match-snippet-block">
            <div class="match-snippet-label">CV</div>
            <div>${objectSnip ? `<span class="match-highlight" data-match-req="${it.requirementId}" style="--matchColor:${color}">${objectSnip}</span>` : '<span class="muted">Kein Nachweis.</span>'}</div>
          </div>
        </div>
      </article>
    `;
  }).join('');

  setActiveRequirement(null);

  host.querySelectorAll('.match-item-card').forEach(card=>{
    const req = card.getAttribute('data-req');
    card.addEventListener('mouseenter', ()=>{
      setActiveRequirement(req);
    });
  });
}



const noteModal = $('#noteModal');
const noteText = $('#noteText');
const noteProgress = $('#noteProgress');
const noteProgressVal = $('#noteProgressVal');
const noteTitle = $('#noteTitle');
const saveNoteBtn = $('#saveNote');
let modalCtx = null; // {requirementId, objectId}
let noteInitialText = '';

function setNoteSaveButtonVisible(visible){
  if (!saveNoteBtn) return;
  saveNoteBtn.style.display = visible ? 'inline-flex' : 'none';
}

function syncNoteSaveButtonVisibility(){
  if (!noteText) return;
  const currentText = noteText.value || '';
  setNoteSaveButtonVisible(currentText !== noteInitialText);
}

function commitProgressAutosave(){
  if (!modalCtx) return;
  const k = key(modalCtx.requirementId, modalCtx.objectId);
  const prev = processes.get(k) || {
    active: false,
    notes: '',
    progress: 0,
    statuses: [],
    status: 'prospecting'
  };

  const raw = parseInt(noteProgress.value, 10) || 0;
  const effectiveProgress = (prev.active && raw === 0) ? 10 : raw;

  // nur Progress updaten, Notiz bleibt unverändert → Status bleibt wie er ist
  updateMatchState(modalCtx.requirementId, modalCtx.objectId, {
    progress: effectiveProgress
  }).catch(console.error);

  const line = getMatchingModuleHost().querySelector(
    `.object-badge[data-jid="${modalCtx.requirementId}"][data-cid="${modalCtx.objectId}"] .progress-line i`
  );
  if (line) line.style.width = effectiveProgress + '%';

  if (raw !== effectiveProgress){
    noteProgress.value = effectiveProgress;
    noteProgressVal.textContent = effectiveProgress + '%';
  }
}


if (saveNoteBtn) {
  saveNoteBtn.addEventListener('click', ()=>{
    if (!modalCtx) return;
    const k = key(modalCtx.requirementId, modalCtx.objectId);
    const prev = processes.get(k) || {
      active: false,
      notes: '',
      progress: 0,
      statuses: [],
      status: 'prospecting'
    };

    const raw = parseInt(noteProgress.value, 10) || 0;
    const effectiveProgress = (prev.active && raw === 0) ? 10 : raw;
    const notes = noteText.value || '';

    // zentrale Logik: updateMatchState kümmert sich jetzt um:
    // - progress
    // - notes
    // - Status aus #tags → processes + DB
    updateMatchState(modalCtx.requirementId, modalCtx.objectId, {
      progress: effectiveProgress,
      notes
    }).catch(console.error);

    noteInitialText = notes;
    setNoteSaveButtonVisible(false);

    const line = getMatchingModuleHost().querySelector(
      `.object-badge[data-jid="${modalCtx.requirementId}"][data-cid="${modalCtx.objectId}"] .progress-line i`
    );
    if (line) line.style.width = effectiveProgress + '%';

    closeNoteModal();
    renderRequirements();
  });
}



function openNoteModal(requirementId, objectId){
  modalCtx = { requirementId, objectId };
  const c = getObject(objectId);
  const k = key(requirementId, objectId);
  const state = processes.get(k) || { progress: 0, notes: '', active: false };

  noteTitle.textContent = `Prozess – ${c.name}`;
  noteInitialText = state.notes || '';
  noteText.value = noteInitialText;
  noteProgress.value = state.progress || 0;
  noteProgressVal.textContent = (state.progress || 0) + '%';
  setNoteSaveButtonVisible(false);

  // Modal sichtbar + für Screenreader einblenden
  noteModal.classList.add('open');
  noteModal.setAttribute('aria-hidden', 'false');

  // Fokus explizit ins Textfeld setzen
  if (noteText) {
    noteText.focus();
  }

  // Falls gerade ein Match aktiv ist, Details in der Mittelsäule anzeigen
  renderMatchDetailPanelInNoteModal();
}


function closeNoteModal(){
  if (!noteModal) return;
  noteModal.classList.remove('open');
  noteModal.setAttribute('aria-hidden', 'true');
  modalCtx = null;
  noteInitialText = '';
  setNoteSaveButtonVisible(false);
}



document.querySelector('[data-close-modal]').addEventListener('click', closeNoteModal);
noteModal.addEventListener('click', (e)=>{ if(e.target===noteModal) closeNoteModal(); });
if (noteText) {
  noteText.addEventListener('input', syncNoteSaveButtonVisibility);
}
noteProgress.addEventListener('input', ()=>{
  noteProgressVal.textContent = noteProgress.value + '%';
  commitProgressAutosave();
});
noteProgress.addEventListener('change', commitProgressAutosave);



// --- Match-Modal (Objekt -> Requirement) ---

let matchModalEl = null;
let matchSourceSelect = null;
let matchRequirementSelect = null;
let matchConfirmBtn = null;
let currentMatchObjectId = null;

function ensureMatchModal(){
  if (matchModalEl) return;

  matchModalEl = document.createElement('div');
  matchModalEl.className = 'modal';
  matchModalEl.id = 'matchModal';
  matchModalEl.innerHTML = `
    <div class="modal-card">
      <div class="modal-header">
        <div class="modal-title" id="matchTitle">Objekt matchen</div>
        <div class="modal-close">
          <button class="icon-btn" data-match-close>
            <svg viewBox="0 0 24 24">
              <path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/>
            </svg>
          </button>
        </div>
      </div>
      <div class="modal-body">
        <div class="modal-row">
          <label class="muted" style="min-width:110px">Quellen</label>
          <select id="matchSourceSelect" class="range"></select>
        </div>
        <div class="modal-row">
          <label class="muted" style="min-width:110px">Anforderung</label>
          <select id="matchRequirementSelect" class="range"></select>
        </div>
        <div class="row" style="margin-top:10px">
          <div class="space"></div>
          <button class="icon-btn" id="matchConfirm" title="Matching anlegen">
            <svg viewBox="0 0 24 24">
              <path d="M9 16.17 4.83 12 3.41 13.41 9 19l12-12-1.41-1.41z"/>
            </svg>
          </button>
        </div>
      </div>
    </div>`;
  appendMatchingLayer(matchModalEl);

  matchSourceSelect = matchModalEl.querySelector('#matchSourceSelect');
  matchRequirementSelect = matchModalEl.querySelector('#matchRequirementSelect');
  matchConfirmBtn = matchModalEl.querySelector('#matchConfirm');

  const closeBtn = matchModalEl.querySelector('[data-match-close]');
  const close = ()=> matchModalEl.classList.remove('open');

  closeBtn.addEventListener('click', close);
  matchModalEl.addEventListener('click', (e)=>{ if (e.target === matchModalEl) close(); });

  matchSourceSelect.addEventListener('change', ()=>{
    populateMatchRequirementSelect(matchSourceSelect.value);
  });


  matchConfirmBtn.addEventListener('click', ()=>{
    const sourceId = matchSourceSelect.value;
    const requirementId = matchRequirementSelect.value;

    if (!currentMatchObjectId || !requirementId || !sourceId){
      showBusinessAlert('Bitte Quellen und Anforderung wählen.');
      return;
    }

    // Matching im Hintergrund starten
    createMatch(sourceId, requirementId, currentMatchObjectId)
      .catch(err => {
        console.error('Fehler beim Matching:', err);
        showBusinessAlert('Matching konnte nicht berechnet werden: ' + (err?.message || err), { title: 'Matching fehlgeschlagen' });
      });

    // Popup SOFORT schließen
    matchModalEl.classList.remove('open');
  });


}

function populateMatchSourceSelect(){
  if (!matchSourceSelect) return;
  const compsWithRequirements = sources.filter(c => (c.requirements || []).length);
  matchSourceSelect.innerHTML = compsWithRequirements
    .map(c => `<option value="${c.id}">${c.name}</option>`)
    .join('');
}

function populateMatchRequirementSelect(sourceId){
  if (!matchRequirementSelect) return;
  const comp = sources.find(c => c.id === sourceId);
  if (!comp){
    matchRequirementSelect.innerHTML = '';
    return;
  }
  matchRequirementSelect.innerHTML = (comp.requirements || [])
    .map(j => `<option value="${j.id}">${j.location} – ${j.title}</option>`)
    .join('');
}

function openMatchModalForObject(objectId){
  ensureMatchModal();
  currentMatchObjectId = objectId;
  const object = getObject(objectId);
  if (!object) return;
  if (object.isPlaceholder) {
    showBusinessAlert('Matching ist erst verfügbar, wenn der Objektimport abgeschlossen ist.');
    return;
  }
  const titleEl = matchModalEl.querySelector('#matchTitle');
  if (titleEl && object) titleEl.textContent = `Objekt matchen – ${object.name}`;

  populateMatchSourceSelect();
  if (matchSourceSelect.options.length){
    const defaultSourceId = activeSource || matchSourceSelect.options[0].value;
    matchSourceSelect.value = defaultSourceId;
    populateMatchRequirementSelect(defaultSourceId);
  } else {
    matchRequirementSelect.innerHTML = '';
  }

  matchModalEl.classList.add('open');
}

/* --------- Modal: Kontaktstatus (Quelle/Objekt) --------- */
const relModal = $('#relModal');
const relSaveBtn = $('#relSave');
const relTitle = $('#relTitle');
let relCtx = null; // { kind: 'source'|'object', id: string }
const imageLightbox = $('#imageLightbox');
const imageLightboxImg = $('#imageLightboxImg');
const imageLightboxCloseBtn = $('#imageLightboxClose');

function openImageLightbox(rawSrc, alt = 'Bild') {
  if (!imageLightbox || !imageLightboxImg) return;
  const src = normalizeImageSrc(rawSrc);
  if (!src) return;
  imageLightboxImg.src = src;
  imageLightboxImg.alt = String(alt || 'Bild');
  imageLightbox.classList.add('open');
  imageLightbox.setAttribute('aria-hidden', 'false');
}

function closeImageLightbox() {
  if (!imageLightbox || !imageLightboxImg) return;
  imageLightbox.classList.remove('open');
  imageLightbox.setAttribute('aria-hidden', 'true');
  imageLightboxImg.removeAttribute('src');
}

if (imageLightboxCloseBtn) {
  imageLightboxCloseBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    closeImageLightbox();
  });
}
if (imageLightbox) {
  imageLightbox.addEventListener('click', (e) => {
    if (e.target === imageLightbox) closeImageLightbox();
  });
}
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  if (!imageLightbox || !imageLightbox.classList.contains('open')) return;
  e.stopPropagation();
  closeImageLightbox();
});

function openRelationModal(kind, id){
  if (!relModal) return;
  relCtx = { kind, id };

  // Namen + Bildquelle holen
  let name = '';
  let photoUrl = '';

  if (kind === 'source'){
    const c = sources.find(c => c.id === id);
    name = c ? c.name : 'Quellen';
    photoUrl = c && c.logoUrl ? c.logoUrl : '';
  } else {
    const object = getObject(id);
    name = object ? object.name : 'Objekt';
    photoUrl = object && object.photo ? object.photo : '';
  }

  if (relTitle){
    relTitle.textContent = `${defText('labels.relationTitle', 'Kontaktstatus')} – ${name}`;
  }

  // NEU: Portrait / Logo ins Modal setzen
  const relPhotoEl = relModal.querySelector('#relPhoto');
if (relPhotoEl){
  // normalize + fallback (object: initials, source: letter)
  let fb = '';
  if (kind === 'object'){
    const object = getObject(id);
    fb = getObjectFallbackAvatarUrl(id, object?.name || name || 'Objekt');
  } else {
    // simples Source-Fallback (Letter SVG)
    const letter = (name || '?')[0] || '?';
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">` +
      `<rect width="100" height="100" rx="18" fill="#667085"/>` +
      `<text x="50" y="58" text-anchor="middle" font-family="system-ui,Segoe UI,Roboto,Arial" ` +
      `font-size="44" font-weight="800" fill="white">${letter}</text>` +
      `</svg>`;
    fb = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  }

  const src = normalizeImageSrc(photoUrl) || '';

  relPhotoEl.innerHTML = safeImgHtml({
    src,
    fallbackSrc: fb,
    alt: name,
    style: 'width:120px;height:120px;border-radius:20px;object-fit:cover;border:1px solid var(--stroke)'
  });

  hydrateImages(relPhotoEl);
  const relImg = relPhotoEl.querySelector('img');
  if (relImg) {
    relImg.title = 'Bild in voller Größe öffnen (1024×1024)';
    relImg.addEventListener('click', (ev) => {
      ev.stopPropagation();
      const fullSrc =
        relImg.currentSrc ||
        relImg.src ||
        relImg.getAttribute('data-src') ||
        relImg.getAttribute('data-fallback') ||
        '';
      openImageLightbox(fullSrc, name);
    });
  }
}


  // aktuellen Status setzen (wie vorher)
  const hasRel = (kind === 'source') ? hasSourceRel(id) : hasObjectRel(id);
  const radios = relModal.querySelectorAll('input[name="relState"]');
  radios.forEach(r => {
    r.checked = (r.value === (hasRel ? '1' : '0'));
  });

  relModal.classList.add('open');
  relModal.setAttribute('aria-hidden','false');
}


function closeRelationModal(){
  if (!relModal) return;
  relModal.classList.remove('open');
  relModal.setAttribute('aria-hidden','true');
  closeImageLightbox();
  relCtx = null;
}

if (relModal){
  const closeBtn = relModal.querySelector('[data-close-rel-modal]');
  if (closeBtn){
    closeBtn.addEventListener('click', (e)=>{
      e.stopPropagation();
      closeRelationModal();
    });
  }
  relModal.addEventListener('click', (e)=>{
    if (e.target === relModal) closeRelationModal();
  });

  if (relSaveBtn){
    relSaveBtn.addEventListener('click', async (e)=>{
      e.stopPropagation();
      if (!relCtx) return;

      const checked = relModal.querySelector('input[name="relState"]:checked');
      const hasRel = checked && checked.value === '1';

      if (relCtx.kind === 'source'){
        await setSourceRelation(relCtx.id, hasRel);
      } else {
        await setObjectRelation(relCtx.id, hasRel);
      }
      closeRelationModal();
    });
  }
}


/* --------- Object Slide-over (rechts) --------- */
const object = document.getElementById('objectPanel');
const objectTitle = document.getElementById('objectTitle');
const objectBody = document.getElementById('objectBody');

function closeObject(){
  object.classList.remove('open');
  object.setAttribute('aria-hidden', 'true');
  delete object.dataset.objectId;
}

document.getElementById('objectClose').addEventListener('click', closeObject);

function openObject(objectId){
  const c = getObject(objectId);
  if (!c) return;

  const objectData = c.object || {};
  const meta = objectData.meta || {};
  const education = Array.isArray(objectData.education) ? objectData.education : [];
  const experience = Array.isArray(objectData.experience) ? objectData.experience : [];
  const objectSkills = objectData.skills || {};
  const rawCvText = c.rawText || objectData.rawText || '';
  const fach = normalizeSkillList(objectSkills.Fachkenntnisse).length
    ? normalizeSkillList(objectSkills.Fachkenntnisse)
    : normalizeSkillList(c.skills);
  const sprach = normalizeSkillList(objectSkills.Sprachkenntnisse);
  const other = normalizeSkillList(objectSkills.other_skills);
  const langStr = (meta.languages || []).map(l => l.code + (l.level ? ` (${l.level})` : '')).join(', ');

  const exec = c.executiveInfo || {};
  const execFach = exec.fachlicheQualifikation || '';
  const execMethoden = exec.methodenKompetenz || '';
  const execLeadership = exec.leadershipFaehigkeit || '';
  const execGehaltOrt = exec.gehaltswunschUndOrt || '';

  objectTitle.textContent = `${defText('labels.objectDrawerTitle', 'Object')} – ${c.name}`;

  // Match-Items für Object-Seite (objectSnippet)
  const objectMatchItems = getSideMatchItemsForCurrent(currentMatchDetail?.requirementId, c.id, 'object');

  const execLines = [];
  if (execFach)       execLines.push(`<div><strong>Fachliche Qualifikation:</strong> ${highlightTextWithMatchItems(execFach, objectMatchItems)}</div>`);
  if (execMethoden)   execLines.push(`<div><strong>Methodenkompetenz:</strong> ${highlightTextWithMatchItems(execMethoden, objectMatchItems)}</div>`);
  if (execLeadership) execLines.push(`<div><strong>Leadership-Fähigkeit:</strong> ${highlightTextWithMatchItems(execLeadership, objectMatchItems)}</div>`);
  if (execGehaltOrt)  execLines.push(`<div><strong>Gehaltswunsch &amp; Ort:</strong> ${highlightTextWithMatchItems(execGehaltOrt, objectMatchItems)}</div>`);

  const execHtml = execLines.length
    ? execLines.join('')
    : '<p class="muted">Keine Executive-Info hinterlegt.</p>';

  const rawExperienceHtml = experience.length
    ? ''
    : renderCvRawLines(fallbackCvSectionLines(rawCvText, 'experience'), objectMatchItems, 'experience');
  const rawEducationHtml = education.length
    ? ''
    : renderCvRawLines(fallbackCvSectionLines(rawCvText, 'education'), objectMatchItems, 'education');
  const rawSkillsHtml = fach.length
    ? ''
    : renderCvRawLines(fallbackCvSectionLines(rawCvText, 'skills'), objectMatchItems, 'skills');

  const eduHtml = education.map(e => {
    const head = [
      e.degree || '',
      e.major || ''
    ].filter(Boolean).join(' – ');
    const headHl = highlightTextWithMatchItems(head, objectMatchItems);
    const instLoc = [e.institution || '', e.location || ''].filter(Boolean).join(' · ');
    const instLocHl = highlightTextWithMatchItems(instLoc, objectMatchItems);

    const detailsList = Array.isArray(e.details) && e.details.length
      ? `<ul class="drawer-compact-list">${
          e.details.map(d=>{
            const dh = highlightTextWithMatchItems(String(d), objectMatchItems);
            return `<li>${dh}</li>`;
          }).join('')
        }</ul>`
      : '';

    return `
      <article class="drawer-timeline-item">
        <div class="drawer-timeline-title">${headHl}</div>
        <div class="drawer-timeline-meta">${instLocHl}</div>
        <div class="drawer-timeline-date">${formatDateRange(e.start_date, e.end_date)}</div>
        ${detailsList}
      </article>
    `;
  }).join('');

  const expHtml = experience.map(e => {
    const titleHl = highlightTextWithMatchItems(e.job_title || e.requirement_title || '', objectMatchItems);
    const empLoc = [e.employer || '', e.location || ''].filter(Boolean).join(' · ');
    const empLocHl = highlightTextWithMatchItems(empLoc, objectMatchItems);

    const descriptionItems = Array.isArray(e.job_description) && e.job_description.length
      ? e.job_description
      : (Array.isArray(e.requirement_description) ? e.requirement_description : []);
    const descList = descriptionItems.length
      ? `<ul class="drawer-compact-list">${
          descriptionItems.map(d=>{
            const dh = highlightTextWithMatchItems(String(d), objectMatchItems);
            return `<li>${dh}</li>`;
          }).join('')
        }</ul>`
      : '';

    return `
      <article class="drawer-timeline-item">
        <div class="drawer-timeline-title">${titleHl}</div>
        <div class="drawer-timeline-meta">${empLocHl}</div>
        <div class="drawer-timeline-date">${formatDateRange(e.start_date, e.end_date)}</div>
        ${descList}
      </article>
    `;
  }).join('');

  const fachHtml = fach.length
    ? renderDrawerChips(fach, objectMatchItems)
    : (rawSkillsHtml || '<p class="muted">Keine fachlichen Skills hinterlegt.</p>');

  const sprachHtml = sprach.length
    ? renderDrawerChips(sprach, objectMatchItems)
    : (langStr
        ? `<p>${highlightTextWithMatchItems(langStr, objectMatchItems)}</p>`
        : '<p class="muted">Keine Sprachkenntnisse hinterlegt.</p>');

  const otherHtml = other.length
    ? renderDrawerChips(other, objectMatchItems)
    : '<p class="muted">Keine weiteren Angaben.</p>';

  const profilDegreeLines = [];
  if (meta.highestDegree) profilDegreeLines.push(`<div><strong>Abschluss:</strong> ${highlightTextWithMatchItems(meta.highestDegree, objectMatchItems)}</div>`);
  if (meta.degree)        profilDegreeLines.push(`<div><strong>Schwerpunkt:</strong> ${highlightTextWithMatchItems(meta.degree, objectMatchItems)}</div>`);

  objectBody.innerHTML = `
    <div style="display:flex; align-items:center; gap:12px; margin-bottom:16px" class="${hasObjectRel(objectId)?'has-rel':'needs-contact'} rel-dot">
      ${safeImgHtml({
  src: c.photo,
  fallbackSrc: getObjectFallbackAvatarUrl(c.id, c.name),
  alt: c.name,
  style: 'width:100%;height:100%;object-fit:cover'
})}

      <div>
        <div style="font-weight:800">${c.name}</div>
        <div class="muted">${c.tax}</div>
      </div>
    </div>

    <div style="display:grid; grid-template-columns: 1.3fr 1fr; gap:12px; margin-bottom:14px">
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.profile', 'Profil')}</div>
        <div style="font-size:13px">
          ${profilDegreeLines.join('')}
          ${meta.birthDate ? `<div><strong>Geburtsdatum:</strong> ${formatDate(meta.birthDate)}</div>` : ''}
        </div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.masterData', 'Stammdaten')}</div>
        <div style="font-size:13px">
          ${meta.nationality ? `<div><strong>Nationalität:</strong> ${highlightTextWithMatchItems(meta.nationality, objectMatchItems)}</div>` : ''}
          ${langStr ? `<div><strong>Sprachen:</strong> ${highlightTextWithMatchItems(langStr, objectMatchItems)}</div>` : ''}
        </div>
      </div>
    </div>

    <section style="margin-bottom:14px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.executiveInfo', 'Executive Info')}</div>
      <div style="font-size:13px">
        ${execHtml}
      </div>
    </section>

    <hr style="border:none;border-top:1px solid var(--stroke);margin:6px 0 12px"/>

    <section style="margin-bottom:14px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.experience', 'Berufserfahrung')}</div>
      ${expHtml ? `<div class="drawer-timeline">${expHtml}</div>` : (rawExperienceHtml || '<p class="muted">Keine Berufserfahrung eingetragen.</p>')}
    </section>

    <section style="margin-bottom:14px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.education', 'Ausbildung')}</div>
      ${eduHtml ? `<div class="drawer-timeline">${eduHtml}</div>` : (rawEducationHtml || '<p class="muted">Keine Ausbildungsstationen eingetragen.</p>')}
    </section>

    <section style="margin-bottom:14px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.skills', 'Fachkenntnisse')}</div>
      ${fachHtml}
    </section>

    <section style="margin-bottom:14px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.languages', 'Sprachkenntnisse')}</div>
      ${sprachHtml}
    </section>

    <section style="margin-bottom:4px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.objectSections.other', 'Weitere Fähigkeiten')}</div>
      ${otherHtml}
    </section>
  `;
  hydrateImages(objectBody);

  const relHeader = objectBody.querySelector('.rel-dot');
  if (relHeader){
    relHeader.title = hasObjectRel(objectId)
      ? 'Kontaktstatus bearbeiten'
      : 'Kontaktstatus setzen';
    relHeader.addEventListener('click', (e)=>{
      e.stopPropagation();
      openRelationModal('object', objectId);
    });
  }

  object.dataset.objectId = objectId;
  object.classList.add('open');
  object.setAttribute('aria-hidden', 'false');
}



/* --------- Requirement Slide-over (links) --------- */
const jp = document.getElementById('requirementPanel');
const jpTitle = document.getElementById('jpTitle');
const jpMeta = document.getElementById('jpMeta');
const jpBody = document.getElementById('jpBody');

function closeRequirementPanel(){
  jp.classList.remove('open');
  jp.setAttribute('aria-hidden', 'true');
  delete jp.dataset.requirementId;
}

document.getElementById('jpClose').addEventListener('click', closeRequirementPanel);

function openRequirement(requirement){
  if(!requirement) return;
  const comp = sources.find(c=>c.id===activeSource) || sources.find(c=>c.requirements && c.requirements.some(j=>j.id===requirement.id));
  if (!comp) return;

  const details = requirement.details || {};
  const aboutSource = details.aboutSource || '';
  const aboutRole = details.aboutRole || '';
  const objectReq = details.objectRequirements || '';
  const responsibilitiesArr = Array.isArray(details.responsibilities) ? details.responsibilities : [];
  const requirementsArr = Array.isArray(details.requirements) ? details.requirements : [];
  const benefitsArr = Array.isArray(details.benefits) ? details.benefits : [];
  const closingNotes = details.closingNotes || '';
  const rawRequirementText = details.rawText || requirement.rawText || '';

  jpTitle.textContent = `${defText('labels.sourceDrawerTitle', 'Source')} – ${requirement.title}`;
  jpMeta.textContent = [
    requirement.location,
    requirement.level || 'Mid',
    requirement.type || 'Vollzeit',
    requirement.internalReferenceId ? `Ref. ${requirement.internalReferenceId}` : ''
  ].filter(Boolean).join(' · ');
  const compRelClass = hasSourceRel(comp.id) ? 'has-rel' : 'needs-contact';
  const compActive = isSourceActive(comp.id);

  // Match-Items für Requirement-Seite (requirementSnippet)
  const requirementMatchItems = getSideMatchItemsForCurrent(requirement.id, currentMatchDetail?.objectId, 'requirement');

  const aboutSourceHtml = renderDrawerProse(aboutSource, requirementMatchItems);
  const aboutRoleHtml = aboutRole
    ? renderDrawerProse(aboutRole, requirementMatchItems)
    : renderDrawerList(responsibilitiesArr, requirementMatchItems);
  const objectReqHtml = objectReq
    ? renderDrawerProse(objectReq, requirementMatchItems)
    : renderDrawerList(requirementsArr, requirementMatchItems);
  const rawRequirementHtml = rawRequirementText
    ? renderCvRawLines(String(rawRequirementText).split(/\r?\n/), requirementMatchItems, 'plain')
    : '';

  const benefitsHtml = (benefitsArr.length
    ? `<ul>${
        benefitsArr.map(b=>{
          const bh = highlightTextWithMatchItems(String(b), requirementMatchItems);
          return `<li>${bh}</li>`;
        }).join('')
      }</ul>`
    : '<p class="muted">Keine Benefits explizit genannt.</p>');

  const closingHtml = renderDrawerProse(closingNotes, requirementMatchItems);

  jpBody.innerHTML = `
    <div style="display:flex; align-items:center; gap:10px; margin-bottom:10px" class="${compRelClass}${compActive ? '' : ' inactive-entity'}">
      <div class="logo rel-dot">
        ${comp.logoUrl
          ? `<img src="${comp.logoUrl}" alt="${comp.name} Logo"/>`
          : `<span style="color:var(--primary-2);font-weight:700">${(comp.name||'?')[0]}</span>`}
      </div>
      <div style="flex:1">
        <div style="font-weight:800">${comp.name}</div>
        <div class="muted">${requirement.title} – ${requirement.location}</div>
        <div class="toggle-row">
          <div class="switch ${compActive?'is-on':''}" data-source-toggle="${comp.id}">
            <div class="switch-knob"></div>
          </div>
          <span class="toggle-label">${compActive?'Aktiv':'Inaktiv'}</span>
        </div>
      </div>
    </div>

    <div class="tag-row" style="margin-bottom:10px">
      ${(requirement.tags||[]).map(t=>`<span class="tag">${t}</span>`).join('')}
    </div>

    <section style="margin-bottom:12px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.sourceSections.aboutSource', 'Source')}</div>
      <div class="drawer-copy">${aboutSourceHtml || '<span class="muted">Kein Beschreibungstext vorhanden.</span>'}</div>
    </section>

    <section style="margin-bottom:12px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.sourceSections.role', 'Beschreibung')}</div>
      <div class="drawer-copy">${aboutRoleHtml || rawRequirementHtml || '<span class="muted">Keine Rollenbeschreibung hinterlegt.</span>'}</div>
    </section>

    <section style="margin-bottom:12px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.sourceSections.requirements', 'Kriterien')}</div>
      <div class="drawer-copy">${objectReqHtml || '<span class="muted">Keine Anforderungen hinterlegt.</span>'}</div>
    </section>

    <section style="margin-bottom:12px">
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.sourceSections.benefits', 'Weitere Informationen')}</div>
      ${benefitsHtml}
    </section>

    <section>
      <div class="muted" style="font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:4px">${defText('drawers.sourceSections.closing', 'Hinweise')}</div>
      <div class="drawer-copy">${closingHtml || '<span class="muted">Keine zusätzlichen Hinweise vorhanden.</span>'}</div>
    </section>
  `;

  jp.dataset.requirementId = requirement.id;
  jp.classList.add('open');
  jp.setAttribute('aria-hidden', 'false');

  const compToggle = jpBody.querySelector('.switch[data-source-toggle]');
  if (compToggle){
    compToggle.addEventListener('click', e=>{
      e.stopPropagation();
      const next = !isSourceActive(comp.id);
      setSourceActive(comp.id, next);
    });
  }

  const relLogo = jpBody.querySelector('.logo.rel-dot');
  if (relLogo){
    relLogo.title = hasSourceRel(comp.id)
      ? 'Kontaktstatus bearbeiten'
      : 'Kontaktstatus setzen';
    relLogo.addEventListener('click', (e)=>{
      e.stopPropagation();
      openRelationModal('source', comp.id);
    });
  }
}




function toggleRequirementPanel(requirement){
  if(!requirement) return;
  const isOpen = jp.classList.contains('open');
  const same = isOpen && jp.dataset.requirementId === requirement.id;
  if (same){
    closeRequirementPanel();
  } else {
    openRequirement(requirement);
  }
}

function toggleRequirementAndObject(requirement, objectId){
  if (!requirement || !objectId) return;

  const requirementOpen = jp.classList.contains('open') && jp.dataset.requirementId === requirement.id;
  const objectOpen  = object.classList.contains('open') && object.dataset.objectId === String(objectId);
  const barOpen = matchDetailBar && matchDetailBar.classList.contains('open');
  const sameMatch =
    currentMatchDetail &&
    currentMatchDetail.requirementId === requirement.id &&
    currentMatchDetail.objectId === objectId;

  // Wenn alles offen und es ist derselbe Match → alles schließen
  if (requirementOpen && objectOpen && barOpen && sameMatch){
    closeRequirementPanel();
    closeObject();
    closeMatchDetailBar();
    return;
  }

  // ansonsten: Requirement + Objekt öffnen
  openRequirement(requirement);
  openObject(objectId);

  // und Match-Details dazu holen
  const sourceId = requirement.sourceId || activeSource;
  if (sourceId){
    openMatchDetailFor(sourceId, requirement.id, objectId);
  }
}

// ✅ DROP-IN: Nachname statt Vorname sortieren
// Hilfsfunktionen für die Matching-Ansicht
// und danach die .sort(...) Anforderungn 1:1 ersetzen (siehe unten).

function _normalizeSortStr(v) {
  return String(v || '')
    .trim()
    .replace(/\s+/g, ' ')
    .toLocaleLowerCase('de-DE');
}

// heuristisch: behandelt typische Namenspräfixe als Teil des Nachnamens
// "Ludwig van Beethoven" -> "van Beethoven"
// "Maria von der Leyen"  -> "von der Leyen"
// Fallback: letzter Token
function _splitNameForLastNameSort(fullName) {
  const name = String(fullName || '').trim().replace(/\s+/g, ' ');
  if (!name) return { last: '', first: '' };

  const parts = name.split(' ').filter(Boolean);
  if (parts.length === 1) return { last: parts[0], first: '' };

  const prefixes = new Set([
    'von', 'vom', 'van', 'de', 'del', 'della', 'di', 'da', 'dos', 'du',
    'der', 'den', 'zu', 'zur', 'zum', 'la', 'le', 'st.', 'st'
  ]);

  // von hinten aufsammeln: Nachname + evtl. Präfixkette davor
  let i = parts.length - 1;
  const lastParts = [parts[i]];
  i--;

  while (i >= 0) {
    const p = parts[i];
    const pn = _normalizeSortStr(p).replace(/\.$/, '');
    if (!prefixes.has(pn)) break;
    lastParts.unshift(p);
    i--;
  }

  const firstParts = parts.slice(0, i + 1);

  return {
    last: lastParts.join(' '),
    first: firstParts.join(' ')
  };
}

function compareObjectsByLastName(a, b) {
  const an = _splitNameForLastNameSort(a?.name);
  const bn = _splitNameForLastNameSort(b?.name);

  // primär: Nachname
  const cLast = an.last.localeCompare(bn.last, 'de-DE', { sensitivity: 'base' });
  if (cLast) return cLast;

  // sekundär: Vorname/Rest
  const cFirst = an.first.localeCompare(bn.first, 'de-DE', { sensitivity: 'base' });
  if (cFirst) return cFirst;

  // stabiler Tiebreaker: id
  return String(a?.id || '').localeCompare(String(b?.id || ''), 'de-DE', { sensitivity: 'base' });
}


// ---------------------------
// ✅ Incremental Object Rendering Helpers
// ---------------------------

function __objectEscapeHtml(str){
  return String(str ?? '')
    .replace(/&/g,'&amp;')
    .replace(/</g,'&lt;')
    .replace(/>/g,'&gt;')
    .replace(/"/g,'&quot;')
    .replace(/'/g,'&#39;');
}
function __objectEscapeAttr(str){
  return String(str ?? '')
    .replace(/&/g,'&amp;')
    .replace(/"/g,'&quot;')
    .replace(/</g,'&lt;');
}
function __objectTs(v){
  const t = Date.parse(v || '');
  return Number.isNaN(t) ? 0 : t;
}
function __objectTsObject(c){
  return __objectTs(c?.createdAt) || __objectTs(c?.updatedAt);
}

// Exec-Block: baut DOM (nicht innerHTML) + trägt full/short in dataset
function __objectBuildExecBlock(cardEl, objectUi){
  const execWrap = cardEl.querySelector('[data-role="object-exec"]');
  if (!execWrap) return;

  const execInfo = (objectUi?.executiveInfo && typeof objectUi.executiveInfo === 'object')
    ? objectUi.executiveInfo
    : {};

  const MAX_LINE_LEN = 140;
  const defs = [
    ['Fachliche Qualifikation', execInfo.fachlicheQualifikation],
    ['Methodenkompetenz',       execInfo.methodenKompetenz],
    ['Leadership-Fähigkeit',    execInfo.leadershipFaehigkeit],
    ['Gehaltswunsch & Ort',     execInfo.gehaltswunschUndOrt]
  ];

  const lines = defs
    .filter(([, v]) => typeof v === 'string' && v.trim())
    .map(([label, v]) => {
      const full = v.trim();
      const short = (full.length > MAX_LINE_LEN)
        ? full.slice(0, MAX_LINE_LEN).trimEnd() + '…'
        : full;
      return { label, full, short, truncated: full.length > MAX_LINE_LEN };
    });

  const hasExec = lines.length > 0;
  const anyTruncated = lines.some(x => x.truncated);
  const expanded = !!objectExecExpanded.get(objectUi.id);

  // Inhalt leeren (nur exec-Teil, nicht gesamte Card)
  execWrap.innerHTML = '';

  if (!hasExec){
    const fallback = document.createElement('div');
    fallback.className = 'c-skill';
    fallback.textContent = objectUi.skills || 'Keine Executive-Info hinterlegt.';
    execWrap.appendChild(fallback);

    // Toggle verstecken
    const toggleBtn = cardEl.querySelector('[data-role="exec-toggle"]');
    if (toggleBtn) {
      toggleBtn.style.display = 'none';
      toggleBtn.setAttribute('aria-hidden', 'true');
    }
    return;
  }

  const block = document.createElement('div');
  block.className = 'exec-block';
  block.style.marginTop = '4px';

  for (const ln of lines){
    const row = document.createElement('div');
    row.className = 'exec-line';
    row.dataset.full = ln.full;
    row.dataset.short = ln.short;
    row.dataset.state = expanded ? 'full' : 'short';
    row.style.display = 'block';
    row.style.marginBottom = '4px';

    const lab = document.createElement('span');
    lab.className = 'exec-line-label';
    lab.style.fontSize = '11px';
    lab.style.fontWeight = '600';
    lab.style.display = 'inline-block';
    lab.style.marginRight = '4px';
    lab.textContent = ln.label + ':';

    const txt = document.createElement('span');
    txt.className = 'exec-line-text';
    txt.style.fontSize = '12px';
    txt.style.lineHeight = '1.35';
    txt.textContent = expanded ? ln.full : ln.short;

    row.appendChild(lab);
    row.appendChild(txt);
    block.appendChild(row);
  }

  execWrap.appendChild(block);

  // Toggle konfigurieren (nur wenn wirklich truncation)
  const toggleBtn = cardEl.querySelector('[data-role="exec-toggle"]');
  if (toggleBtn){
    if (!anyTruncated){
      toggleBtn.style.display = 'none';
      toggleBtn.setAttribute('aria-hidden', 'true');
    } else {
      toggleBtn.style.display = '';
      toggleBtn.setAttribute('aria-hidden', 'false');
      toggleBtn.textContent = expanded ? 'Weniger anzeigen' : 'Mehr anzeigen';
      toggleBtn.setAttribute('data-expanded', expanded ? '1' : '0');
    }
  }
}

function __objectApplyExecExpanded(cardEl, objectId, expanded){
  const execWrap = cardEl.querySelector('[data-role="object-exec"]');
  if (!execWrap) return;
  execWrap.querySelectorAll('.exec-line').forEach(line => {
    const full  = line.dataset.full || '';
    const short = line.dataset.short || '';
    const span = line.querySelector('.exec-line-text');
    if (!span) return;
    span.textContent = expanded ? full : short;
    line.dataset.state = expanded ? 'full' : 'short';
  });

  const toggleBtn = cardEl.querySelector('[data-role="exec-toggle"]');
  if (toggleBtn){
    toggleBtn.textContent = expanded ? 'Weniger anzeigen' : 'Mehr anzeigen';
    toggleBtn.setAttribute('data-expanded', expanded ? '1' : '0');
  }
}

/**
 * ✅ createObjectCard(objectUi) -> HTMLElement
 * Baut Card genau 1x. Spätere Updates über patchObjectCard.
 */
function createObjectCard(objectUi){
  const objectId = objectUi.id;

  const card = el('div', 'object-card');
  card.dataset.objectId = String(objectId);
  card.setAttribute('data-role', 'object-card');

  // Struktur einmalig (mit data-role Hooks)
  card.innerHTML = `
    <div class="object-head" style="display:flex;align-items:flex-start;gap:12px;flex-wrap:wrap;">
      <div class="object-avatar-wrap" style="flex:0 0 auto;">
        <div class="avatar rel-dot" data-role="rel-avatar">
          <img data-role="avatar-img" />
        </div>
        <div class="toggle-row small">
          <div class="switch" data-role="object-toggle">
            <div class="switch-knob"></div>
          </div>
        </div>
      </div>

      <div class="object-meta" style="flex:1 1 200px;min-width:180px;">
        <div data-role="object-name" style="font-weight:700;line-height:1.15;word-break:break-word;"></div>
        <div class="c-tax" data-role="object-tax" style="line-height:1.15;margin-top:2px;word-break:break-word;"></div>
      </div>

      <div class="object-actions" style="margin-left:auto;display:flex;align-items:center;gap:8px;flex:0 0 auto;">
        <button type="button" class="icon-btn match-action-btn" data-role="match-btn"
                title="${__objectEscapeAttr(defText('labels.matchActionTitle', 'Objekt matchen'))}" aria-label="${__objectEscapeAttr(defText('labels.matchActionAria', 'Objekt matchen'))}">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 3a9 9 0 0 1 8.95 8H23v2h-2.05A9 9 0 0 1 13 20.95V23h-2v-2.05A9 9 0 0 1 3.05 13H1v-2h2.05A9 9 0 0 1 11 3.05V1h2v2.05A9 9 0 0 1 12 3Zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm0 3.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7Z"/>
          </svg>
          <span>${__objectEscapeHtml(defText('labels.matchAction', 'Matchen'))}</span>
        </button>

        <button type="button" class="icon-btn" data-role="del-btn"
                title="Objekt löschen" aria-label="Löschen"
                style="height:30px;width:30px;display:inline-flex;align-items:center;justify-content:center;">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M18.3 5.71 12 12l6.3 6.29-1.41 1.42L10.59 13.4 4.3 19.71 2.89 18.3 9.18 12 2.89 5.71 4.3 4.29l6.29 6.3 6.29-6.3z"/>
          </svg>
        </button>

        <button type="button" class="view-object btn-pill" data-role="object-btn"
                title="${__objectEscapeAttr(defText('labels.objectRecord', 'Objekt'))} öffnen" aria-label="${__objectEscapeAttr(defText('labels.objectRecord', 'Objekt'))} öffnen">${__objectEscapeHtml(defText('labels.objectViewButton', 'Object'))}</button>
      </div>
    </div>

    <div class="object-exec" data-role="object-exec"></div>
    <button type="button" class="exec-toggle" data-role="exec-toggle" data-expanded="0" style="margin-top:4px"></button>
  `;

  // --- One-time listeners (kein Rebind bei Patch) ---
  // Card select
  card.addEventListener('click', () => {
    selectedObject = selectedObject === objectId ? null : objectId;
    matrixSelectedObjectId = null;
    // ✅ nur Patch/Reconciling – KEIN Full rebuild
    renderObjects({ reason: 'select' });
    renderMap();
  });

  // Relation click
  const relTarget = card.querySelector('[data-role="rel-avatar"]');
  if (relTarget){
    relTarget.addEventListener('click', (e)=>{
      e.stopPropagation();
      openRelationModal('object', objectId);
    });
  }

  // Active switch
  const toggle = card.querySelector('[data-role="object-toggle"]');
  if (toggle){
    toggle.addEventListener('click', (e)=>{
      e.stopPropagation();
      const next = !isObjectActive(objectId);
      setObjectActive(objectId, next);
    });
  }

  // Object
  const objectBtn = card.querySelector('[data-role="object-btn"]');
  if (objectBtn){
    objectBtn.addEventListener('click', (e)=>{
      e.stopPropagation();
      openObject(objectId);
    });
  }

  // Match button
  const matchBtn = card.querySelector('[data-role="match-btn"]');
  if (matchBtn){
    matchBtn.addEventListener('click', (e)=>{
      e.stopPropagation();

      const comp = sources.find(co => co.id === activeSource) || sources[0];
      const sourceId = comp ? comp.id : null;
      const requirementId = activeRequirementForScoring || null;

      if (sourceId && requirementId) {
        createMatch(sourceId, requirementId, objectId)
          .catch(err => {
            console.error('Fehler beim direkten Matching:', err);
            showBusinessAlert('Matching konnte nicht berechnet werden: ' + (err?.message || err), { title: 'Matching fehlgeschlagen' });
          });
        return;
      }

      openMatchModalForObject(objectId);
    });
  }

  // Delete
  const delBtn = card.querySelector('[data-role="del-btn"]');
  if (delBtn){
    delBtn.addEventListener('click', (e)=>{
      e.stopPropagation();
      handleDeleteObject(objectId);
    });
  }

  // Exec toggle
  const execToggle = card.querySelector('[data-role="exec-toggle"]');
  if (execToggle){
    execToggle.addEventListener('click', (e)=>{
      e.stopPropagation();
      const isExpanded = objectExecExpanded.get(objectId) === true;
      const next = !isExpanded;
      objectExecExpanded.set(objectId, next);
      __objectApplyExecExpanded(card, objectId, next);
    });
  }

  return card;
}

/**
 * ✅ patchObjectCard(objectId, objectUi)
 * Patcht nur Text/Classes/src — keine Card-Neuerstellung.
 */
function patchObjectCard(objectId, objectUi){
  const card = objectDomById.get(objectId);
  if (!card) return;

  // Classes (relation/active/selected)
  const relClass = hasObjectRel(objectId) ? 'has-rel' : 'needs-contact';
  const active = isObjectActive(objectId);
  const isPlaceholder = !!objectUi.isPlaceholder;

  card.classList.toggle('has-rel', relClass === 'has-rel');
  card.classList.toggle('needs-contact', relClass === 'needs-contact');
  card.classList.toggle('inactive-entity', !active && !isPlaceholder);
  card.classList.toggle('pending', isPlaceholder);
  card.classList.toggle('selected', selectedObject === objectId);

  // Relation tooltip
  const relTarget = card.querySelector('[data-role="rel-avatar"]');
  if (relTarget){
    relTarget.title = hasObjectRel(objectId) ? 'Kontaktstatus bearbeiten' : 'Kontaktstatus setzen';
  }

  // Name / Tax / PreMatch
  const nameEl = card.querySelector('[data-role="object-name"]');
  if (nameEl) nameEl.textContent = objectUi.name || 'Unbekannt';

  const preMatchLabel =
    (typeof objectUi.score === 'number' && !Number.isNaN(objectUi.score))
      ? `Ø Match ${objectUi.score}%`
      : 'Ø Match –';

  const taxEl = card.querySelector('[data-role="object-tax"]');
  if (taxEl){
    if (isPlaceholder) {
      const importLine = objectUi.importError
        ? `Import fehlgeschlagen: ${objectUi.importError}`
        : `Object-Service: ${objectUi.importStatus || 'queued'}`;
      taxEl.textContent = importLine;
    } else {
      // Tax-Zeile: "tax · Ø Match ..."
      const tax = objectUi.tax || '';
      taxEl.innerHTML = `${__objectEscapeHtml(tax)} · <span title="Ø-Match zum aktiven Quellen">${__objectEscapeHtml(preMatchLabel)}</span>`;
    }
  }

  // Switch on/off
  const sw = card.querySelector('[data-role="object-toggle"]');
  if (sw) {
    sw.classList.toggle('is-on', !!active);
    sw.style.display = isPlaceholder ? 'none' : '';
    sw.style.pointerEvents = isPlaceholder ? 'none' : '';
  }

  const matchBtn = card.querySelector('[data-role="match-btn"]');
  if (matchBtn){
    matchBtn.disabled = isPlaceholder;
    matchBtn.title = isPlaceholder
      ? 'Während des Objektimports deaktiviert'
      : defText('labels.matchActionTitle', 'Objekt matchen');
  }

  // Avatar: nur setzen wenn wirklich anders
  // Avatar: normalize + CSP-safe fallback
  const img = card.querySelector('[data-role="avatar-img"]');
  if (img){
    const fallback = getObjectFallbackAvatarUrl(objectId, objectUi.name);
    const nextSrcRaw = objectUi.photo || '';
    const nextSrc = normalizeImageSrc(nextSrcRaw) || normalizeImageSrc(fallback) || '';

    const prevSrc = objectLastPhotoUrl.get(objectId) || '';
    if (nextSrc && nextSrc !== prevSrc){
      img.src = nextSrc;
      objectLastPhotoUrl.set(objectId, nextSrc);
    }

    img.alt = objectUi.name || 'Objekt';
    img.loading = 'lazy';
    img.decoding = 'async';

    // CSP-safe: kein inline onerror, nur einmal binden
    if (!img.__fallbackBound){
      img.__fallbackBound = true;
      img.addEventListener('error', () => {
        const fb = normalizeImageSrc(fallback);
        if (fb && img.src !== fb) img.src = fb;
      });
    }
  }


  // Exec block (nur dieser Teil wird neu gebaut)
  __objectBuildExecBlock(card, objectUi);
}

/**
 * ✅ reconcileObjectList(listEl, listData)
 * - erstellt neue Cards
 * - patcht bestehende
 * - entfernt fehlende
 * - reorder ohne Neurender (appendChild bewegt Nodes)
 */
function reconcileObjectList(listEl, listData){
  const nextIds = new Set(listData.map(x => x.id));

  // Remove cards not present
  for (const [id, node] of objectDomById.entries()){
    if (!nextIds.has(id)){
      try { node.remove(); } catch (_) {}
      objectDomById.delete(id);
      objectLastPhotoUrl.delete(id);
      // objectExecExpanded absichtlich NICHT löschen (UI-State kann bleiben, wenn Objekt wieder auftaucht)
    }
  }

  // Add/Patch + order
  for (const objectUi of listData){
    try {
      const id = objectUi.id;
      let card = objectDomById.get(id);

      if (!card){
        card = createObjectCard(objectUi);
        objectDomById.set(id, card);
      }

      // In richtige Reihenfolge bringen, bevor der Detail-Patch läuft.
      // So bleibt wenigstens die Card sichtbar, falls ein einzelnes Feld kaputt ist.
      listEl.appendChild(card);
      patchObjectCard(id, objectUi);
    } catch (error) {
      console.error('[matching] Objektkarte konnte nicht gerendert werden', objectUi, error);
      const fallbackCard = el('div', 'object-card');
      fallbackCard.dataset.objectId = String(objectUi?.id || '');
      fallbackCard.innerHTML = `
        <div class="object-head">
          <div class="avatar">${_escapeHtml(getInitials(objectUi?.name || 'Objekt'))}</div>
          <div>
            <div class="object-name" style="font-weight:700">${_escapeHtml(objectUi?.name || 'Unbekanntes Objekt')}</div>
            <div class="c-tax">${_escapeHtml(objectUi?.tax || objectUi?.taxonomy || objectUi?.currentRole || 'Objekt')}</div>
          </div>
        </div>
      `;
      listEl.appendChild(fallbackCard);
    }
  }
}



// ---------------------------
// ✅ Incremental Object Rendering State
// ---------------------------



const objectDomById = new Map();          // objectId -> HTMLElement (Card)
const objectExecExpanded = new Map();     // objectId -> boolean (Mehr anzeigen)
const objectLastPhotoUrl = new Map();     // objectId -> string (verhindert src reset)
let __objectEmptyStateEl = null;          // optional: leeres State-Element


/* --------- Objekte rechts (✅ Incremental) --------- */
function renderObjects(opts = {}){
  const list = document.getElementById('objectList');
  const qEl  = document.getElementById('objectSearch');
  const sortEl = document.getElementById('objectSort');
  if (!list || !qEl || !sortEl) return;

  if (selectedObject && !(objects || []).some(c => c.id === selectedObject)) {
    selectedObject = null;
    persistMatchingRuntimeState();
  }

  // ✅ Scroll/Fokus stabil halten
  const prevScroll = list.scrollTop;
  const hadFocus = (document.activeElement === qEl);

  // Leerer State
  if (!objects || !objects.length){
    // remove existing cards
    for (const [, node] of objectDomById) { try { node.remove(); } catch (_) {} }
    objectDomById.clear();

    list.innerHTML = '<div class="muted" style="padding:8px">Keine Objekte in der Datenbank gefunden.</div>';
    return;
  }

  const q = qEl.value || '';
  const sort = sortEl.value;

  const comp = sources.find(c=>c.id===activeSource) || sources[0];

  function scoreOf(object){
    if (!comp || !(comp.requirements || []).length) return null;

    const requirementsForScore = activeRequirementForScoring
      ? (comp.requirements || []).filter(j => j.id === activeRequirementForScoring)
      : (comp.requirements || []);

    const scores = requirementsForScore
      .map(j => preMatchScore(j.id, object.id))
      .filter(s => typeof s === 'number' && !Number.isNaN(s));

    if (!scores.length) return null;

    const sum = scores.reduce((a,b)=> a+b, 0);
    return Math.round(sum / scores.length);
  }

  // listData berechnen
  let listData = objects
    .map(c => ({ ...c, score: scoreOf(c) }))
    .filter(c => matchesFullTextSearch(buildObjectSearchPayload(c), q));

  if (sort === 'best') {
    listData.sort((a,b)=>{
      const sa = (typeof a.score === 'number') ? a.score : -1;
      const sb = (typeof b.score === 'number') ? b.score : -1;
      return sb - sa;
    });
  }
  if (sort === 'name') listData.sort(compareObjectsByLastName);
  if (sort === 'tax')  listData.sort((a,b)=> String(a.tax||'').localeCompare(String(b.tax||''), 'de-DE', { sensitivity:'base' }));
  if (sort === 'created_desc') listData.sort((a,b)=> __objectTsObject(b) - __objectTsObject(a));
  if (sort === 'created_asc')  listData.sort((a,b)=> __objectTsObject(a) - __objectTsObject(b));

  // ✅ Kein list.innerHTML='' mehr!
  // Falls aus früheren Versionen noch ein "leerer Text" drin ist: nur beim ersten Mal bereinigen
  if (!list.dataset.incReady){
    list.innerHTML = '';
    list.dataset.incReady = '1';
  } else {
    // falls mal empty-state HTML drin war
    if (list.children.length === 1 && list.firstElementChild?.classList?.contains('muted')){
      // nur löschen, wenn das wirklich unser Empty State war
      list.innerHTML = '';
    }
  }

  reconcileObjectList(list, listData);

  // ✅ Scroll/Fokus nur zurück, wenn User nicht busy (du hast __objectUiIsBusy schon)
  if (typeof __objectUiIsBusy === 'function') {
    if (!__objectUiIsBusy()) list.scrollTop = prevScroll;
  } else {
    list.scrollTop = prevScroll;
  }
  if (hadFocus) qEl.focus();
}






/* --------- Match-Detail-Bar Logik --------- */

function findMatch(requirementId, objectId){
  return matches.find(m =>
    m.requirementId === requirementId &&
    m.objectId === objectId &&
    !m.removed
  );
}

async function loadMatchItems(sourceId, requirementId, objectId){
  if (!rxdb || !rxdb.matches) return [];
  try {
    const doc = await rxdb.matches
      .findOne({ selector: { sourceId, requirementId, objectId: objectId } })
      .exec();

    const json = doc ? doc.toJSON() : null;
    const matchFromMemory = findMatch(requirementId, objectId);
    const dbItems = normalizeMatchItemsFromRecord(json);
    const items = dbItems.length
      ? dbItems
      : (Array.isArray(matchFromMemory?.items) ? matchFromMemory.items.slice() : []);

    items.sort((a,b)=>{
      const ak = (a.matchLevelKey ?? 0) * 100 + (a.matchScoreKey ?? 0);
      const bk = (b.matchLevelKey ?? 0) * 100 + (b.matchScoreKey ?? 0);
      return bk - ak;
    });

    return items;
  } catch (e){
    console.error('Fehler beim Laden der Match-Items aus matches', e);
    return [];
  }
}


let matchOverlayEl = null;

function ensureMatchOverlay(){
  if (matchOverlayEl) return matchOverlayEl;

  matchOverlayEl = document.createElement('div');
  matchOverlayEl.className = 'match-detail-overlay';

  const s = matchOverlayEl.style;
  s.position = 'fixed';
  s.zIndex = '9999';
  s.maxWidth = '340px';
  s.pointerEvents = 'none';
  s.background = 'var(--surface-elevated, #111827)';
  s.color = 'inherit';
  s.borderRadius = '8px';
  s.boxShadow = '0 10px 30px rgba(0,0,0,.5)';
  s.padding = '8px 10px';
  s.fontSize = '11px';
  s.border = '1px solid var(--stroke, #374151)';
  s.display = 'none';

  appendMatchingLayer(matchOverlayEl);
  return matchOverlayEl;
}

function hideMatchOverlay(){
  if (!matchOverlayEl) return;
  matchOverlayEl.style.display = 'none';
}



function showMatchOverlayFor(reqId, evt){
  if (!currentMatchDetail || !currentMatchDetail.items) return;
  const item = currentMatchDetail.items.find(it => it.requirementId === reqId);
  if (!item) return;

  const escapeHtml = (str) => String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

  const overlay = ensureMatchOverlay();

  const colorByRequirement = currentMatchDetail.colorByRequirement || {};
  const color = colorByRequirement[item.requirementId] || MATCH_COLORS[0];

  const dimMap = {
    education: 'Ausbildung',
    experience: 'Erfahrung',
    skill: 'Skill',
    language: 'Sprache',
    other: 'Sonstiges'
  };
  const prioMap = {
    base: 'Basis',
    performance: 'Leistung',
    enthusiasm: 'Begeisterung'
  };
  const levelMap = {
    full: 'voll',
    partial: 'teilweise',
    none: 'kein'
  };

  const dimLabel   = dimMap[item.dimension] || item.dimension || 'Sonstiges';
  const prioLabel  = prioMap[item.priority] || item.priority || '';
  const levelLabel = levelMap[item.matchLevel] || item.matchLevel || '';

  const pct = (()=>{
    if (typeof item.matchScore === 'number'){
      const raw = item.matchScore;
      if (raw <= 1 && raw >= 0) return Math.round(raw * 100);
      return Math.round(raw);
    }
    if (typeof item.matchScoreKey === 'number'){
      return Math.round(item.matchScoreKey);
    }
    if (item.matchLevel === 'full')    return 100;
    if (item.matchLevel === 'partial') return 60;
    if (item.matchLevel === 'none')    return 0;
    return null;
  })();

  const requirementSnip = item.jobSnippet || item.requirementSnippet || '';
  const objectSnip  = item.cvSnippet || item.objectSnippet || '';
  const expl    = item.explanation || '';

  const hasConflict = hasConflictOnItem(item);
  const conflictIcons = getConflictIconsFromItem(item);
  const conflictTip = getConflictTooltipFromItem(item);
  const tipAttr = conflictTip ? ` title="${escapeHtml(conflictTip)}"` : '';

  const pctHtml = (pct != null)
    ? (hasConflict
        ? `<s>${pct}%</s><span class="conflict-icons" style="margin-left:4px"${tipAttr}>${escapeHtml(conflictIcons.join(''))}</span>`
        : `${pct}%`)
    : '';

  overlay.innerHTML = `
    <article class="match-item-card" data-req="${item.requirementId}" style="--matchColor:${color}">
      <div class="match-item-header">
        <div class="match-item-badge"></div>
        <div>
          <div class="match-item-title" style="font-size:12px">${item.title || 'Anforderung'}</div>
          <div class="match-item-meta" style="font-size:11px">
            ${dimLabel}
            ${prioLabel ? ` · Priorität: ${prioLabel}` : ''}
            ${levelLabel ? ` · Match: ${levelLabel}` : ''}
            ${pctHtml ? ` · ${pctHtml}` : ''}
          </div>
        </div>
      </div>
      ${expl ? `<div class="match-item-expl" style="font-size:11px;margin-top:4px">${expl}</div>` : ''}
      <div class="match-item-snips" style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px;margin-top:6px">
        <div class="match-snippet-block">
          <div class="match-snippet-label" style="font-size:10px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:2px">Stellenausschreibung</div>
          <div style="font-size:11px">
            ${requirementSnip
              ? `<span class="match-highlight" data-match-req="${item.requirementId}" style="--matchColor:${color}">${requirementSnip}</span>`
              : '<span class="muted">Kein Nachweis.</span>'}
          </div>
        </div>
        <div class="match-snippet-block">
          <div class="match-snippet-label" style="font-size:10px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:2px">CV</div>
          <div style="font-size:11px">
            ${objectSnip
              ? `<span class="match-highlight" data-match-req="${item.requirementId}" style="--matchColor:${color}">${objectSnip}</span>`
              : '<span class="muted">Kein Nachweis.</span>'}
          </div>
        </div>
      </div>
    </article>
  `;

  overlay.style.display = 'block';

  const padding = 12;
  let x = (evt.clientX ?? 0) + padding;
  let y = (evt.clientY ?? 0) + padding;

  const rect = overlay.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  if (x + rect.width > vw - 8){
    x = vw - rect.width - 8;
  }
  if (y + rect.height > vh - 8){
    y = vh - rect.height - 8;
  }

  overlay.style.left = x + 'px';
  overlay.style.top  = y + 'px';
}





function renderMatchDetailBar(){
  if (!matchDetailBar || !matchBarRequirementLabel || !matchBarObjectLabel || !matchBarScore || !matchBarTags || !currentMatchDetail) return;

  const escapeHtml = (str) => String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

  const { sourceId, requirementId, objectId, items, colorByRequirement } = currentMatchDetail;
  const { requirement, source } = findRequirementAndSource(requirementId);
  const object = getObject(objectId);
  const m = findMatch(requirementId, objectId);

  const requirementName  = requirement ? `${requirement.title} (${requirement.location})` : `Requirement ${requirementId}`;
  const compName = source ? source.name : '';
  const objectName = object ? object.name : `Objekt ${objectId}`;
  const score    = scoreFromMatchItems(items);

  matchBarRequirementLabel.textContent  = compName ? `${compName} – ${requirementName}` : requirementName;
  matchBarObjectLabel.textContent = objectName;
  const overall = getOverallConflictInfo(items);
  if (score != null && overall.icons) {
    const tipAttr = overall.tooltip ? ` title="${escapeHtml(overall.tooltip)}"` : '';
    matchBarScore.innerHTML =
      `<s>${escapeHtml(score)}%</s><span class="conflict-icons" style="margin-left:6px"${tipAttr}>${escapeHtml(overall.icons)}</span>`;
  } else {
    matchBarScore.textContent = score != null ? `${score}%` : '–';
  }

  if (!items || !items.length){
    if (matchBarTags){
      matchBarTags.innerHTML = '<span class="muted" style="font-size:11px">Keine Match-Nachweise gespeichert.</span>';
    }
    hideMatchOverlay();
    matchDetailBar.classList.add('open');
    matchDetailBar.setAttribute('aria-hidden','false');
    return;
  }

  const bucketConfig = [
    { key: 'base',        label: 'Basis' },
    { key: 'performance', label: 'Leistung' },
    { key: 'enthusiasm',  label: 'Begeisterung' }
  ];

  const buckets = { base: [], performance: [], enthusiasm: [] };

  function normalizePriority(p){
    const t = String(p || '').toLowerCase();
    if (t === 'performance') return 'performance';
    if (t === 'enthusiasm')  return 'enthusiasm';
    return 'base';
  }

  function inferItemScorePct(it){
    if (typeof it.matchScore === 'number'){
      const raw = it.matchScore;
      if (raw <= 1 && raw >= 0) return Math.round(raw * 100);
      return Math.round(raw);
    }
    if (typeof it.matchScoreKey === 'number'){
      return Math.round(it.matchScoreKey);
    }
    if (it.matchLevel === 'full')    return 100;
    if (it.matchLevel === 'partial') return 60;
    if (it.matchLevel === 'none')    return 0;
    return null;
  }

  function scoreIconAndLabel(pct){
    if (pct == null){
      return { icon: '❓', label: '' };
    }
    if (pct <= 0){
      return { icon: '❌', label: '0%' };
    }
    if (pct > 80){
      return { icon: '✅', label: `${pct}%` };
    }
    return { icon: '☑️', label: `${pct}%` };
  }

  items.forEach((it, idx)=>{
    const prioKey = normalizePriority(it.priority);
    const color   = colorByRequirement[it.requirementId] || MATCH_COLORS[idx % MATCH_COLORS.length];
    const pct     = inferItemScorePct(it);
    const scoreUi = scoreIconAndLabel(pct);

    const conflictIcons = getConflictIconsFromItem(it);
    const conflictTip = getConflictTooltipFromItem(it);
    const hasConflict = conflictIcons.length > 0;

    buckets[prioKey].push({
      ...it,
      _uiColor: color,
      _uiScorePct: pct,
      _uiScoreIcon: scoreUi.icon,
      _uiScoreLabel: scoreUi.label,
      _uiHasConflict: hasConflict,
      _uiConflictIcons: conflictIcons,
      _uiConflictTip: conflictTip
    });
  });

  if (matchBarTags){
    const columnsHtml = bucketConfig.map(cfg => {
      const list = buckets[cfg.key] || [];
      const hasItems = list.length > 0;

      const itemsHtml = hasItems
        ? list.map(it => {
            const title = it.title || 'Anforderung';
            const reqId = it.requirementId;
            const color = it._uiColor;
            const icon  = it._uiScoreIcon;
            const label = it._uiScoreLabel;

            const hasConflict = !!it._uiHasConflict;
            const conflictIcons = Array.isArray(it._uiConflictIcons) ? it._uiConflictIcons : [];
            const conflictTip = (typeof it._uiConflictTip === 'string') ? it._uiConflictTip : '';
            const tipAttr = conflictTip ? ` title="${escapeHtml(conflictTip)}"` : '';

            const labelHtml = label
              ? (hasConflict ? `<s>${escapeHtml(label)}</s>` : `<span>${escapeHtml(label)}</span>`)
              : '';

            const conflictHtml = hasConflict
              ? `<span class="conflict-icons" style="margin-left:4px"${tipAttr}>${escapeHtml(conflictIcons.join(''))}</span>`
              : '';

            return `
              <button
                class="match-pill match-pill-row"
                data-req="${reqId}"
                type="button"
                style="--matchColor:${color}"
              >
                <span style="display:flex;align-items:center;gap:6px;min-width:0">
                  <span class="match-pill-dot"></span>
                  <span class="match-pill-title">
                    ${escapeHtml(title)}
                  </span>
                </span>
                <span class="match-pill-score" style="flex-shrink:0;display:flex;align-items:center;gap:3px;font-variant-numeric:tabular-nums"${tipAttr}>
                  <span>${escapeHtml(icon)}</span>
                  ${labelHtml}
                  ${conflictHtml}
                </span>
              </button>
            `;
          }).join('')
        : '<div class="muted" style="font-size:11px">Keine Einträge.</div>';

      return `
        <section class="match-col">
          <header>
            ${cfg.label}
          </header>
          <div class="match-col-list">
            ${itemsHtml}
          </div>
        </section>
      `;
    }).join('');

    matchBarTags.innerHTML = `
      <div class="match-columns">
        ${columnsHtml}
      </div>
    `;

    matchBarTags.querySelectorAll('.match-pill').forEach(btn => {
      const req = btn.getAttribute('data-req');

      btn.addEventListener('mouseenter', (e) => {
        setActiveRequirement(req);
        showMatchOverlayFor(req, e);
      });

      btn.addEventListener('mousemove', (e) => {
        showMatchOverlayFor(req, e);
      });

      btn.addEventListener('mouseleave', () => {
        setActiveRequirement(null);
        hideMatchOverlay();
      });

      btn.addEventListener('click', (e) => {
        e.preventDefault();
        setActiveRequirement(req);
      });

      btn.addEventListener('focus', (e) => {
        setActiveRequirement(req);
        showMatchOverlayFor(req, e);
      });
    });
  }

  setActiveRequirement(null);

  matchDetailBar.classList.add('open');
  matchDetailBar.setAttribute('aria-hidden','false');
}


function applyMatchHighlights(){
  if (!currentMatchDetail) return;
  const { requirementId, objectId } = currentMatchDetail;

  // Requirement-Panel neu rendern (mit Highlights)
  if (jp.classList.contains('open') && jp.dataset.requirementId === requirementId){
    const { requirement } = findRequirementAndSource(requirementId);
    if (requirement) openRequirement(requirement);
  }

  // Object-Panel neu rendern (mit Highlights)
  if (object.classList.contains('open') && object.dataset.objectId === String(objectId)){
    openObject(objectId);
  }

  // Start: keine aktive Anforderung, Detail erst nach Interaktion
  setActiveRequirement(null);
}

function clearMatchDetailHighlights(){
  if (!currentMatchDetail) return;
  const { requirementId, objectId } = currentMatchDetail;
  currentMatchDetail = null;

  // Requirement & Object ohne Highlights neu rendern
  if (jp.classList.contains('open') && jp.dataset.requirementId === requirementId){
    const { requirement } = findRequirementAndSource(requirementId);
    if (requirement) openRequirement(requirement);
  }
  if (object.classList.contains('open') && object.dataset.objectId === String(objectId)){
    openObject(objectId);
  }

  setActiveRequirement(null);
}

function openMatchDetailBar(){
  if (!matchDetailBar || !currentMatchDetail) return;
  renderMatchDetailBar();
  applyMatchHighlights();
}

function closeMatchDetailBar(){
  if (!matchDetailBar) return;
  matchDetailBar.classList.remove('open');
  matchDetailBar.setAttribute('aria-hidden','true');

  const host = document.getElementById('noteMatchDetail');
  if (host){
    host.innerHTML = '';
    host.style.display = 'none';
  }
  clearMatchDetailHighlights();
  hideMatchOverlay(); // NEU
}


async function openMatchDetailFor(sourceId, requirementId, objectId){
  if (!sourceId || !requirementId || !objectId) return;

  // Wenn wir diesen Match schon geladen haben → Bar nur erneut öffnen
  if (
    currentMatchDetail &&
    currentMatchDetail.sourceId === sourceId &&
    currentMatchDetail.requirementId === requirementId &&
    currentMatchDetail.objectId === objectId &&
    Array.isArray(currentMatchDetail.items) &&
    currentMatchDetail.items.length
  ){
    openMatchDetailBar();
    return;
  }

  // Items aus der DB holen (sortiert in loadMatchItems)
  const items = await loadMatchItems(sourceId, requirementId, objectId);

  const colorByRequirement = {};
  items.forEach((it, idx) => {
    // requirementId nur LESEN bzw. Fallback-Berechnung,
    // aber NICHT mehr zurück ins Objekt schreiben (das ist read-only)
    const key = it.requirementId || `REQ-${idx + 1}`;
    colorByRequirement[key] = MATCH_COLORS[idx % MATCH_COLORS.length];
  });

  currentMatchDetail = {
    sourceId,
    requirementId,
    objectId,
    items,
    colorByRequirement
  };

  openMatchDetailBar();
}


/* --------- Match-Detail-Bar DOM --------- */
const matchDetailBar      = document.getElementById('matchDetailBar');
const matchBarRequirementLabel    = document.getElementById('matchBarRequirementLabel') || document.getElementById('matchBarAnforderungLabel');
const matchBarObjectLabel   = document.getElementById('matchBarObjectLabel');
const matchBarScore       = document.getElementById('matchBarScore');
const matchBarTags        = document.getElementById('matchBarTags');
const matchBarContent     = document.getElementById('matchBarContent');
const matchBarCloseBtn    = document.getElementById('matchBarClose');

if (matchBarCloseBtn){
  matchBarCloseBtn.addEventListener('click', ()=>{
    closeMatchDetailBar();
  });
}

/* --------- Tabs Steuerung (Liste / Matrix) --------- */
const tabList = document.getElementById('tabList');
const tabMap = document.getElementById('tabMap');      // Tab "Matrix"
const mapWrap = document.getElementById('mapWrap');    // Container für Matrix
const requirementListEl = document.getElementById('requirementList');
const listTools = document.getElementById('listTools');
const mapEl = document.getElementById('map');
const toggleUnknown = document.getElementById('toggleUnknown');

// NEU: zentraler Cleanup, damit keine "Matrix-Artefakte" in der Listenansicht bleiben
function cleanupMatrixViewArtifacts() {
  // Overlay sicher aus
  try { hideMatchOverlay(); } catch (_) {}
  // Detailbar schließen (die kann sonst "floating" wirken / Fokus behalten)
  try { closeMatchDetailBar(); } catch (_) {}

  // Matrix DOM leeren (verhindert Sticky/Scroll/Focus-Leichen)
  const mapNode = document.getElementById('map');
  if (mapNode) {
    mapNode.innerHTML = '';
  }

  // optional: Auswahl zurücksetzen (damit beim nächsten Matrix-Open kein "ghost highlight" bleibt)
  // matrixSelectedObjectId = null;
}

// Optional: Checkbox "nur aktive Objekte anzeigen" o.ä.
if (toggleUnknown) toggleUnknown.addEventListener('change', renderMap);

if (tabList && tabMap && mapWrap && requirementListEl && listTools) {
  const setActiveMatchingTab = (tabName, { persist = true } = {}) => {
    const nextTab = tabName === 'matrix' ? 'matrix' : 'list';
    const showMatrix = nextTab === 'matrix';

    if (mapWrap) {
      mapWrap.style.display = showMatrix ? 'block' : 'none';
    }

    if (nextTab === 'matrix') {
      tabMap.classList.add('active');
      tabList.classList.remove('active');
      mapWrap.classList.add('active');
      requirementListEl.style.display = 'none';
      listTools.style.display = 'none';
      renderMap();
    } else {
      tabList.classList.add('active');
      tabMap.classList.remove('active');
      cleanupMatrixViewArtifacts();
      mapWrap.classList.remove('active');
      requirementListEl.style.display = 'block';
      listTools.style.display = 'flex';
    }

    if (persist) {
      persistMatchingRuntimeState({ activeTab: nextTab });
    }
  };

  tabList.addEventListener('click', () => {
    setActiveMatchingTab('list', { persist: true });
  });

  tabMap.addEventListener('click', () => {
    setActiveMatchingTab('matrix', { persist: true });
  });

  const savedTab = matchingViewState.activeTab === 'matrix' ? 'matrix' : 'list';
  setActiveMatchingTab(savedTab, { persist: false });
}



/* --------- Matrix-Ansicht (Anforderung x Objekt) --------- */

// Wir verwenden weiterhin #mapWrap und #map, nur ist es jetzt eine Matrix.
// Kompaktere Kürzung für Header
// - versucht "Vorname N."
// - dann "V. N."
// - sonst hart abschneiden + …
function shortenObjectName(fullName, maxLen) {
  const name = String(fullName || '').trim();
  if (!name) return '';

  if (name.length <= maxLen) return name;

  const parts = name.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    const c1 = `${parts[0]} ${parts[1].charAt(0)}.`;
    if (c1.length <= maxLen) return c1;

    const c2 = `${parts[0].charAt(0)}. ${parts[1].charAt(0)}.`;
    if (c2.length <= maxLen) return c2;
  }

  if (maxLen > 1) {
    return name.slice(0, maxLen - 1).trimEnd() + '…';
  }
  return name.charAt(0);
}

// --- NEU: Objekt in der Matrix auswählen & Matches hervorheben ---

function setMatrixSelectedObject(objectId){
  selectedObject = objectId && selectedObject !== objectId ? objectId : null;
  matrixSelectedObjectId = null;
  persistMatchingRuntimeState();
  renderObjects({ reason: 'matrix-select' });
  renderMap();
}


function renderMap() {
  if (!mapWrap || !mapEl) return;

  // ✅ FIX: Wenn Matrix nicht aktiv ist, IMMER aufräumen (verhindert Artefakte beim View-Wechsel)
  if (!mapWrap.classList.contains('active')) {
    mapEl.innerHTML = '';
    try { hideMatchOverlay(); } catch (_) {}
    return;
  }

  mapEl.innerHTML = '';

  // kleine Escape-Helper
  const escapeHtml = (str) => String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

  if (matrixSelectedObjectId) {
    if (!selectedObject) selectedObject = matrixSelectedObjectId;
    matrixSelectedObjectId = null;
    persistMatchingRuntimeState();
  }

  const selectedSourceForMatrix = activeSource
    ? (sources || []).find(c => c.id === activeSource) || null
    : null;
  const selectedObjectForMatrix = selectedObject
    ? (objects || []).find(c => c.id === selectedObject) || null
    : null;
  const selectedObjectIdForMatrix = selectedObjectForMatrix?.id || null;

  // Quellen, die überhaupt Matches haben (und nicht removed)
  const sourcesForMatrix = selectedSourceForMatrix ? [selectedSourceForMatrix] : (sources || []);
  const sourcesWithMatches = sourcesForMatrix
    .filter(c => c && c.id)
    .map(c => {
      const sourceMatches = (matches || []).filter(m =>
        m.sourceId === c.id &&
        !m.removed &&
        (!selectedObjectIdForMatrix || m.objectId === selectedObjectIdForMatrix)
      );
      return { source: c, sourceMatches };
    })
    .filter(x => x.sourceMatches.length > 0);

  if (!sourcesWithMatches.length) {
    mapEl.innerHTML = `<div class="muted" style="padding:8px">Keine Matches im aktuellen Matrix-Filter.</div>`;
    return;
  }

  // ---------- GLOBAL HEADER (oben einmal) ----------
  const globalHeader = document.createElement('div');
  globalHeader.className = 'matrix-header';
  globalHeader.style.display = 'flex';
  globalHeader.style.alignItems = 'flex-end';
  globalHeader.style.justifyContent = 'space-between';
  globalHeader.style.gap = '12px';

  const leftHead = document.createElement('div');
  const filterLabel = [
    selectedSourceForMatrix ? `Quellen: ${selectedSourceForMatrix.name}` : '',
    selectedObjectForMatrix ? `Objekt: ${selectedObjectForMatrix.name}` : ''
  ].filter(Boolean).join(' · ');
  leftHead.innerHTML = `
    <div class="matrix-title">Matrix – ${selectedSourceForMatrix ? escapeHtml(selectedSourceForMatrix.name) : 'alle Quellen'}</div>
    <div class="matrix-subtitle muted" style="font-size:11px">
      ${filterLabel ? `${escapeHtml(filterLabel)} · ` : ''}Klick auf Quellen oder Objekt hebt die Auswahl wieder auf
    </div>
  `;

  const rightHead = document.createElement('div');
  rightHead.style.display = 'flex';
  rightHead.style.alignItems = 'center';
  rightHead.style.gap = '8px';
  rightHead.innerHTML = `
    <span class="tag" style="font-size:11px;padding:4px 8px;border-radius:999px;border:1px solid var(--stroke, #374151);">
      ${sourcesWithMatches.length} Quellen
    </span>
    ${selectedObjectForMatrix ? `<span class="tag" style="font-size:11px;padding:4px 8px;border-radius:999px;border:1px solid var(--stroke, #374151);">${escapeHtml(selectedObjectForMatrix.name)}</span>` : ''}
  `;

  globalHeader.appendChild(leftHead);
  globalHeader.appendChild(rightHead);
  mapEl.appendChild(globalHeader);

  // ---------- SOURCE SECTIONS ----------
  const container = document.createElement('div');
  container.style.display = 'flex';
  container.style.flexDirection = 'column';
  container.style.gap = '14px';
  container.style.marginTop = '10px';
  mapEl.appendChild(container);

  // Styling helpers (ohne CSS-Datei anfassen)
  const hitCellStyle = `
    background: rgba(108, 140, 255, 0.16);
    outline: 1px solid rgba(108, 140, 255, 0.35);
    border-radius: 6px;
  `;
  const selectedColStyle = `
    background: rgba(61, 220, 151, 0.10);
  `;

  let renderedSections = 0;

  // pro Quellen eine Matrix bauen
  sourcesWithMatches.forEach(({ source: comp, sourceMatches }) => {
    const sourceId = comp.id;

    // Requirements, die Matches haben
    const requirementIdsWithMatches = new Set(sourceMatches.map(m => m.requirementId));
    let rowRequirements = (comp.requirements || []).filter(j => j && requirementIdsWithMatches.has(j.id));
    if (selectedObjectIdForMatrix) {
      rowRequirements = rowRequirements.filter(requirement => sourceMatches.some(m =>
        m.requirementId === requirement.id &&
        m.objectId === selectedObjectIdForMatrix &&
        !m.removed
      ));
    }
    if (!rowRequirements.length) return;

    // Objekte-Spalten: nur Objekte, die bei diesem Quellen Matches haben
    const objectIdsWithMatches = new Set(
      sourceMatches
        .filter(m => requirementIdsWithMatches.has(m.requirementId))
        .map(m => m.objectId)
    );

    let colObjects = (objects || []).filter(c =>
      objectIdsWithMatches.has(c.id) &&
      (!selectedObjectIdForMatrix || c.id === selectedObjectIdForMatrix)
    );
    if (toggleUnknown && toggleUnknown.checked) {
      colObjects = colObjects.filter(c => isObjectActive(c.id));
    }
    if (!colObjects.length) return;

    // Object-Name-Cut abhängig von Anzahl
    let maxNameLen;
    if (colObjects.length <= 6) maxNameLen = 16;
    else if (colObjects.length <= 10) maxNameLen = 13;
    else if (colObjects.length <= 14) maxNameLen = 10;
    else maxNameLen = 8;

    // Kompaktere Kürzung für Header
    function shortenObjectName(fullName, maxLen) {
      const name = String(fullName || '').trim();
      if (!name) return '';
      if (name.length <= maxLen) return name;

      const parts = name.split(/\s+/).filter(Boolean);
      if (parts.length >= 2) {
        const c1 = `${parts[0]} ${parts[1].charAt(0)}.`;
        if (c1.length <= maxLen) return c1;

        const c2 = `${parts[0].charAt(0)}. ${parts[1].charAt(0)}.`;
        if (c2.length <= maxLen) return c2;
      }

      if (maxLen > 1) return name.slice(0, maxLen - 1).trimEnd() + '…';
      return name.charAt(0);
    }

    // Layout-Berechnung
    const section = document.createElement('section');
    section.style.border = '1px solid var(--stroke, #374151)';
    section.style.borderRadius = '12px';
    section.style.background = 'var(--surface, #020617)';
    section.style.padding = '10px';

    const containerWidth = mapEl.clientWidth || mapWrap.clientWidth || 900;
    const REQUIREMENT_COL_WIDTH = 230;
    const visibleColsTarget = Math.min(colObjects.length, 10);

    let colWidth = Math.floor((containerWidth - REQUIREMENT_COL_WIDTH - 40) / visibleColsTarget);
    const MIN_COL_WIDTH = 70;
    const MAX_COL_WIDTH = 110;
    if (colWidth < MIN_COL_WIDTH) colWidth = MIN_COL_WIDTH;
    if (colWidth > MAX_COL_WIDTH) colWidth = MAX_COL_WIDTH;

    const totalMinWidth = REQUIREMENT_COL_WIDTH + colObjects.length * colWidth;

    // Objekt ausgewählt? -> Zählen, ob dieses Quellen Matches dafür hat
    const selectedCount = selectedObjectIdForMatrix
      ? sourceMatches.filter(m => m.objectId === selectedObjectIdForMatrix && !m.removed).length
      : 0;

    const compHeader = document.createElement('div');
    compHeader.style.display = 'flex';
    compHeader.style.alignItems = 'center';
    compHeader.style.justifyContent = 'space-between';
    compHeader.style.gap = '10px';
    compHeader.style.marginBottom = '8px';

    const left = document.createElement('div');
    left.innerHTML = `
      <div style="display:flex;align-items:center;gap:10px;">
        <div class="logo" style="width:28px;height:28px;border-radius:8px;border:1px solid var(--stroke);display:flex;align-items:center;justify-content:center;overflow:hidden;">
          ${
            comp.logoUrl
              ? `<img src="${comp.logoUrl}" alt="${escapeHtml(comp.name)} Logo" style="width:100%;height:100%;object-fit:cover"/>`
              : `<span style="color:var(--primary-2);font-weight:700">${escapeHtml((comp.name?.[0] || '?'))}</span>`
          }
        </div>
        <div>
          <div style="font-weight:800">${escapeHtml(comp.name)}</div>
          <div class="muted" style="font-size:11px">
            ${rowRequirements.length} Requirement(s) mit Matches · ${colObjects.length} Objekt(nen)
          </div>
        </div>
      </div>
    `;

    const right = document.createElement('div');
    right.style.display = 'flex';
    right.style.alignItems = 'center';
    right.style.gap = '8px';

    if (selectedObjectIdForMatrix) {
      const badge = document.createElement('span');
      badge.className = 'tag';
      badge.style.fontSize = '11px';
      badge.style.padding = '4px 8px';
      badge.style.borderRadius = '999px';
      badge.style.border = '1px solid var(--stroke, #374151)';
      badge.style.background = selectedCount > 0 ? 'rgba(61,220,151,.12)' : 'rgba(255,255,255,.04)';
      badge.textContent = selectedCount > 0
        ? `✅ ${selectedCount} Match(es) für Auswahl`
        : `— Keine Matches für Auswahl`;
      right.appendChild(badge);
    }

    compHeader.appendChild(left);
    compHeader.appendChild(right);
    section.appendChild(compHeader);

    // Scroll-Container
    const scroll = document.createElement('div');
    scroll.className = 'matrix-scroll';
    scroll.style.overflowX = 'auto';
    scroll.style.overflowY = 'auto';
    scroll.style.maxHeight = '520px';
    scroll.style.borderRadius = '10px';
    scroll.style.border = '1px solid var(--stroke, #374151)';

    const table = document.createElement('table');
    table.className = 'matrix-table';
    table.style.borderCollapse = 'collapse';
    table.style.tableLayout = 'fixed';
    table.style.minWidth = totalMinWidth + 'px';
    table.dataset.sourceId = sourceId;

    // THEAD
    const thead = document.createElement('thead');
    const headRow = document.createElement('tr');

    const cornerTh = document.createElement('th');
    cornerTh.className = 'matrix-th matrix-th-requirement';
    cornerTh.textContent = 'Anforderung / Objekt';
    cornerTh.style.position = 'sticky';
    cornerTh.style.left = '0';
    cornerTh.style.zIndex = '3';
    cornerTh.style.background = 'var(--surface, #020617)';
    cornerTh.style.minWidth = REQUIREMENT_COL_WIDTH + 'px';
    cornerTh.style.maxWidth = REQUIREMENT_COL_WIDTH + 'px';
    cornerTh.style.padding = '4px 6px';
    headRow.appendChild(cornerTh);

    colObjects.forEach(c => {
      const th = document.createElement('th');
      th.className = 'matrix-th matrix-th-object';
      th.style.whiteSpace = 'nowrap';
      th.style.overflow = 'hidden';
      th.style.textOverflow = 'ellipsis';
      th.style.fontSize = '11px';
      th.style.minWidth = colWidth + 'px';
      th.style.maxWidth = colWidth + 'px';
      th.style.padding = '2px 4px';
      th.title = c.name;

      // Klick auf Spaltenkopf -> Objekt auswählen
      th.style.cursor = 'pointer';
      th.addEventListener('click', (e) => {
        e.stopPropagation();
        setMatrixSelectedObject(c.id);
      });

      const relClass = hasObjectRel(c.id) ? 'has-rel' : 'needs-contact';
      const active = isObjectActive(c.id);
      const shortName = shortenObjectName(c.name, maxNameLen);

      if (selectedObjectIdForMatrix && c.id === selectedObjectIdForMatrix) {
        th.style.cssText += selectedColStyle;
      }

      th.innerHTML = `
        <div class="${relClass}${!active ? ' inactive-entity' : ''}"
             style="display:flex;flex-direction:column;align-items:flex-start;gap:1px;max-width:100%;">
          <span style="font-weight:700;max-width:100%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
            ${escapeHtml(shortName)}
          </span>
          <span class="muted" style="font-size:9px;max-width:100%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
            ${escapeHtml(c.tax || '')}
          </span>
        </div>
      `;

      headRow.appendChild(th);
    });

    thead.appendChild(headRow);
    table.appendChild(thead);

    // TBODY
    const tbody = document.createElement('tbody');

    // schneller Lookup: requirementId|objectId -> match
    const matchByPair = new Map();
    sourceMatches.forEach(m => {
      if (!m || m.removed) return;
      matchByPair.set(`${m.requirementId}::${m.objectId}`, m);
    });

    rowRequirements.forEach(requirement => {
      const tr = document.createElement('tr');

      const requirementTh = document.createElement('th');
      requirementTh.className = 'matrix-th matrix-th-requirement';
      requirementTh.style.position = 'sticky';
      requirementTh.style.left = '0';
      requirementTh.style.zIndex = '2';
      requirementTh.style.background = 'var(--surface, #020617)';
      requirementTh.style.textAlign = 'left';
      requirementTh.style.fontSize = '11px';
      requirementTh.style.padding = '4px 6px';
      requirementTh.style.minWidth = REQUIREMENT_COL_WIDTH + 'px';
      requirementTh.style.maxWidth = REQUIREMENT_COL_WIDTH + 'px';

      const sourceIsActiveFlag = isSourceActive(comp.id);

      requirementTh.innerHTML = `
        <div class="${!sourceIsActiveFlag ? 'inactive-entity' : ''}">
          <div style="font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
            ${escapeHtml(requirement.title)}
          </div>
          <div class="muted" style="font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
            ${escapeHtml(requirement.location)} · ${escapeHtml(requirement.level || 'Mid')} · ${escapeHtml(requirement.type || 'Vollzeit')}
          </div>
        </div>
      `;
      tr.appendChild(requirementTh);

      colObjects.forEach(c => {
        const td = document.createElement('td');
        td.className = 'matrix-td';
        td.style.textAlign = 'center';
        td.style.padding = '2px 4px';
        td.style.minWidth = colWidth + 'px';
        td.style.maxWidth = colWidth + 'px';

        if (selectedObjectIdForMatrix && c.id === selectedObjectIdForMatrix) {
          td.style.cssText += selectedColStyle;
        }

        const procKey = key(requirement.id, c.id);
        const match = matchByPair.get(`${requirement.id}::${c.id}`) || null;

        if (!match) {
          td.classList.add('matrix-td-empty');
          td.innerHTML = '<span class="muted">–</span>';
          tr.appendChild(td);
          return;
        }

        const isPending = pendingMatchKeys.has(procKey);
        const pct = scoreFromMatchItems(match.items);

        const bucket = pct != null ? gradeBucket(pct) : 'low';

        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'matrix-score score' + (isPending ? ' is-loading' : '');
        btn.setAttribute('data-source-id', sourceId);
        btn.setAttribute('data-requirement-id', requirement.id);
        btn.setAttribute('data-object-id', c.id);
        btn.setAttribute('data-match-key', procKey);
        btn.dataset.pct = bucket;
        btn.title = `${comp.name} – ${requirement.title} × ${c.name}`;
        btn.style.minWidth = '100%';

        if (isPending) {
          btn.innerHTML = '<span class="score-spinner" aria-label="Matching wird berechnet…"></span>';
        } else if (pct != null) {
          // ✅ FIX: Durchstreichen + Konflikt-Icons (Ausschluss) in der Matrix
          const overall = (typeof getOverallConflictInfo === 'function')
            ? getOverallConflictInfo(match.items)
            : null;

          if (overall && overall.icons) {
            const tipAttr = overall.tooltip ? ` title="${escapeHtml(overall.tooltip)}"` : '';
            btn.innerHTML =
              `<s>${escapeHtml(pct)}%</s>` +
              `<span class="conflict-icons" style="margin-left:6px"${tipAttr}>${escapeHtml(overall.icons)}</span>`;
          } else {
            btn.textContent = `${pct}%`;
          }
        } else {
          btn.textContent = '–';
        }

        // Treffer markieren, wenn Object ausgewählt
        if (selectedObjectIdForMatrix && c.id === selectedObjectIdForMatrix) {
          td.style.cssText += hitCellStyle;
        }

        td.appendChild(btn);
        tr.appendChild(td);
      });

      tbody.appendChild(tr);
    });

    table.appendChild(tbody);
    scroll.appendChild(table);
    section.appendChild(scroll);
    container.appendChild(section);
    renderedSections++;

    // Klick auf Score → Match-Detail (pro Tabelle)
    table.addEventListener('click', (e) => {
      const btn = e.target.closest('.matrix-score');
      if (!btn) return;

      const requirementId = btn.getAttribute('data-requirement-id');
      const objectId = btn.getAttribute('data-object-id');
      const compId = btn.getAttribute('data-source-id') || sourceId;

      if (!requirementId || !objectId || !compId) return;

      const { requirement } = findRequirementAndSource(requirementId);
      if (requirement) {
        toggleRequirementAndObject(requirement, objectId);
      } else {
        openMatchDetailFor(compId, requirementId, objectId);
      }
    });
  });

  if (!renderedSections) {
    container.innerHTML = `<div class="muted" style="padding:8px">Keine Matches im aktuellen Matrix-Filter.</div>`;
  }
}







/* --------- Events --------- */
const sourceSearchEl = $('#sourceSearch');
const requirementSearchEl = $('#requirementSearch');
const requirementFilterEl = $('#requirementFilter');
const objectSearchEl = $('#objectSearch');
const objectSortEl = $('#objectSort');

applyPersistedMatchingControls();
setupMatchingColumnResizing();

if (sourceSearchEl) {
  sourceSearchEl.addEventListener('input', () => {
    renderSources();
    persistMatchingRuntimeState();
  });
}
if (requirementSearchEl) {
  requirementSearchEl.addEventListener('input', () => {
    renderRequirements();
    persistMatchingRuntimeState();
  });
}
if (requirementFilterEl) {
  requirementFilterEl.addEventListener('change', () => {
    requirementFilterEl.dataset.persistedValue = requirementFilterEl.value || 'all';
    renderRequirements();
    persistMatchingRuntimeState();
  });
}
if (objectSearchEl) {
  objectSearchEl.addEventListener('input', () => {
    renderObjects();
    persistMatchingRuntimeState();
  });
}
if (objectSortEl) {
  objectSortEl.addEventListener('change', () => {
    renderObjects();
    persistMatchingRuntimeState();
  });
}

// --- NEU: Objekte-Import (PDF, LinkedIn, Freitext) ---

const objectImportPdfBtn   = document.getElementById('objectImportPdfBtn');
const objectImportPdfInput = document.getElementById('objectImportPdfInput');
const objectImportText     = document.getElementById('objectImportText');
const objectImportTextBtn  = document.getElementById('objectImportTextBtn');

function looksLikeLinkedinProfileUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return false;

  try {
    const url = new URL(trimmed);
    const hostOk = /(^|\.)linkedin\.com$/i.test(url.hostname);
    const path = url.pathname.toLowerCase();
    // z.B. /in/foobar, /in/foobar/, /in/foobar-123/
    const pathOk = path.startsWith('/in/');
    return hostOk && pathOk;
  } catch {
    return false;
  }
}

// ---------------------------
// PDF Multi-Select Import (Drop-in Replacement)
// ---------------------------

function _isPdfFile(f) {
  if (!f) return false;
  const nameOk = typeof f.name === 'string' && f.name.toLowerCase().endsWith('.pdf');
  const typeOk = typeof f.type === 'string' && f.type.toLowerCase() === 'application/pdf';
  // manche Browser liefern bei lokalen Dateien manchmal type="" -> Name reicht als Fallback
  return typeOk || nameOk;
}

function _fileListToPdfArray(fileList) {
  return Array.from(fileList || []).filter(_isPdfFile);
}

function _fileListToImageArray(fileList) {
  return Array.from(fileList || []).filter(isImageFile);
}

/**
 * Importiert mehrere PDFs mit optionalem Concurrency-Limit.
 * Nutzt DEIN startPdfObjectImport(file) unverändert.
 *
 * @param {File[]} files
 * @param {{ concurrency?: number }} opts
 * @returns {Promise<{ ok: Array<{file:File, placeholderId?:string, requirementId?:string}>, failed: Array<{file:File, error:any}> }>}
 */
async function startPdfObjectImportMulti(files, opts = {}) {
  const list = Array.isArray(files) ? files.filter(_isPdfFile) : [];
  const ok = [];
  const failed = [];
  const concurrency = Math.max(1, Math.min(4, Number(opts.concurrency || 2)));
  if (!list.length) {
    return { ok, failed };
  }

  const queue = list.map((file) => {
    const importId = (typeof crypto !== 'undefined' && crypto.randomUUID)
      ? crypto.randomUUID()
      : `imp_${Date.now()}_${Math.random().toString(16).slice(2)}`;
    const placeholderId = `pending_${importId.replace(/[^a-zA-Z0-9_:-]/g, '')}`;

    createUiPlaceholderObject({
      placeholderId,
      displayName: file?.name || 'PDF-Import',
      sourceLabel: 'upload:pdf'
    });
    updateObjectPlaceholderStatus(placeholderId, 'wird gestartet …');

    return { file, importId, placeholderId };
  });

  let nextIndex = 0;
  async function worker() {
    while (nextIndex < queue.length) {
      const entry = queue[nextIndex++];
      const { file, importId, placeholderId } = entry;
      try {
        updateObjectPlaceholderStatus(placeholderId, 'wird hochgeladen …');
        const requirementInfo = await importObjectFromPdfFile(file);
        const res = await startObjectImportBackground({
          ...requirementInfo,
          importId,
          placeholderId,
          placeholderName: file?.name || requirementInfo?.placeholderName || 'PDF-Import'
        });
        ok.push({ file, ...(res || {}) });
      } catch (e) {
        failed.push({ file, error: e });
        markObjectPlaceholderAsError(placeholderId, e?.message || String(e));
        console.error('PDF-Import Fehler (Multi):', file?.name, e);
      }
    }
  }

  const workers = new Array(Math.min(concurrency, queue.length)).fill(0).map(() => worker());
  await Promise.all(workers);

  return { ok, failed };
}



function bindObjectFileUpload(btnEl, inputEl) {
  if (!btnEl || !inputEl) return;

  // zur Sicherheit: Multi-Select & Dateifilter aktivieren (auch wenn HTML es nicht setzt)
  inputEl.multiple = true;
  inputEl.setAttribute('multiple', '');
  inputEl.accept = 'application/pdf,.pdf,image/*,.png,.jpg,.jpeg,.webp,.gif,.bmp,.avif';

  // In Business OS öffnet der Header-Button zuerst den Spalten-Drawer.
  if (!btnEl.dataset.columnAction) {
    btnEl.addEventListener('click', () => {
      inputEl.value = '';
      inputEl.click();
    });
  }

  // Multi-Select Import
  inputEl.addEventListener('change', async (event) => {
    const selectedFiles = Array.from(event?.target?.files || []);
    const files = _fileListToPdfArray(selectedFiles);
    const images = _fileListToImageArray(selectedFiles);
    if (!files.length && !images.length) return;

    try {
      const imageResults = [];
      const imageFailures = [];
      if (images.length) {
        const selectedTarget =
          images.length === 1 && selectedObject && (objects || []).some((item) => item?.id === selectedObject)
            ? selectedObject
            : '';
        for (const image of images) {
          try {
            imageResults.push(await importObjectImageFile(image, selectedTarget));
          } catch (error) {
            imageFailures.push({ file: image, error });
          }
        }
      }

      const { ok, failed } = files.length
        ? await startPdfObjectImportMulti(files, { concurrency: 2 })
        : { ok: [], failed: [] };

      console.log('Objekt-Dateiimport gestartet:', {
        images: imageResults.map(x => ({ objectId: x.objectId, updated: x.updated })),
        imported: ok.map(x => ({ name: x.file?.name, placeholderId: x.placeholderId, requirementId: x.requirementId })),
        failed: [...failed, ...imageFailures].map(x => ({ name: x.file?.name, error: x.error?.message || String(x.error) }))
      });

      // Optional: kurze UX-Info (kannst du entfernen)
      const allFailed = [...failed, ...imageFailures];
      if (allFailed.length) {
        showBusinessAlert(
          `Import gestartet.\n\nErfolgreich: ${ok.length + imageResults.length}\nFehlgeschlagen: ${allFailed.length}\n\n` +
          allFailed.slice(0, 5).map(f => `- ${f.file?.name}: ${f.error?.message || f.error}`).join('\n') +
          (allFailed.length > 5 ? `\n… (+${allFailed.length - 5} weitere)` : ''),
          { title: 'Import gestartet' }
        );
      }
    } catch (e) {
      console.error('Objekt-Dateiimport Gesamtfehler', e);
      showBusinessAlert('Objekt-Dateiimport fehlgeschlagen: ' + (e?.message || e), { title: 'Import fehlgeschlagen' });
    } finally {
      inputEl.value = '';
    }
  });
}

// Aktivieren (Drop-in)
if (objectImportPdfBtn && objectImportPdfInput) {
  bindObjectFileUpload(objectImportPdfBtn, objectImportPdfInput);
}


// LinkedIn-URL oder Freitext (Textarea)
if (objectImportText && objectImportTextBtn) {
  async function handleTextImport() {
    const raw = objectImportText.value;
    const trimmed = String(raw || '').trim();
    if (!trimmed) return;

    try {
      if (looksLikeLinkedinProfileUrl(trimmed)) {
        // LinkedIn-Profil importieren
        const { placeholderId, requirementId } = await startLinkedinObjectImport(trimmed);
        console.log('LinkedIn-Import gestartet:', { placeholderId, requirementId });
      } else {
        // Object als Freitext / Markdown
        const { placeholderId, requirementId } = await startTextObjectImport(trimmed);
        console.log('Text-Import gestartet:', { placeholderId, requirementId });
      }

      objectImportText.value = '';
    } catch (e) {
      console.error('Objekte-Import Fehler', e);
      showBusinessAlert('Objekte-Import fehlgeschlagen: ' + (e?.message || e), { title: 'Import fehlgeschlagen' });
    }
  }

  objectImportTextBtn.addEventListener('click', (e) => {
    e.preventDefault();
    handleTextImport();
  });

  // Enter (ohne Shift) triggert Import, Shift+Enter erzeugt Zeilenumbruch
  objectImportText.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleTextImport();
    }
  });
}


document.body.addEventListener('click', (e)=>{
  if (!getMatchingModuleHost().contains(e.target)) return;
  const sBtn = e.target.closest('[data-search]');
  if(sBtn){
    const requirementId = sBtn.getAttribute('data-search');
    showBusinessAlert('Objektesuche starten für '+ requirementId +' …');
  }
});

function applyMatchingDefinitionUi(root = getMatchingModuleHost()) {
  if (!root) return;
  const titles = root.querySelectorAll('.column-title');
  if (titles[0]) titles[0].textContent = defText('labels.requirementsColumn', 'Sources');
  if (titles[1]) titles[1].textContent = defText('labels.matchesColumn', 'Matches');
  if (titles[2]) titles[2].textContent = defText('labels.objectsColumn', 'Objects');

  const sourceSearch = root.querySelector('#sourceSearch');
  if (sourceSearch) {
    sourceSearch.placeholder = defText('placeholders.sourceSearch', sourceSearch.placeholder);
    sourceSearch.setAttribute('aria-label', `${defText('labels.sourceRecordPlural', 'Sources')} suchen`);
  }

  const requirementSearch = root.querySelector('#requirementSearch');
  if (requirementSearch) requirementSearch.placeholder = defText('placeholders.matchSearch', requirementSearch.placeholder);

  const objectSearch = root.querySelector('#objectSearch');
  if (objectSearch) objectSearch.placeholder = defText('placeholders.objectSearch', objectSearch.placeholder);

  const objectTitleEl = root.querySelector('#objectTitle');
  if (objectTitleEl) objectTitleEl.textContent = defText('labels.objectRecord', 'Object');

  const requirementTitleEl = root.querySelector('#jpTitle');
  if (requirementTitleEl) requirementTitleEl.textContent = defText('labels.sourceRecord', 'Source');

  const relTitleEl = root.querySelector('#relTitle');
  if (relTitleEl) relTitleEl.textContent = defText('labels.relationTitle', 'Relation');

  const relExists = root.querySelector('[data-rel-label="exists"]');
  const relMissing = root.querySelector('[data-rel-label="missing"]');
  if (relExists) relExists.textContent = defText('labels.relationExists', relExists.textContent);
  if (relMissing) relMissing.textContent = defText('labels.relationMissing', relMissing.textContent);

  root.querySelectorAll('[data-column="requirements"]').forEach((button) => {
    const action = button.getAttribute('data-column-action') || '';
    const plural = defText('labels.sourceRecordPlural', 'Sources');
    if (action === 'configure') button.title = `${plural} konfigurieren`;
    if (action === 'import') button.title = `${plural} importieren`;
    if (action === 'export') button.title = `${plural} exportieren`;
  });

  root.querySelectorAll('[data-column="objects"]').forEach((button) => {
    const action = button.getAttribute('data-column-action') || '';
    const plural = defText('labels.objectRecordPlural', 'Objects');
    if (action === 'configure') button.title = `${plural} konfigurieren`;
    if (action === 'import') button.title = `${plural} importieren`;
    if (action === 'export') button.title = `${plural} exportieren`;
  });
}

// Initial: erst RxDB laden, dann rendern
// Initial: erst RxDB laden, dann rendern, dann Live-Sync aktivieren
export async function mountMatchingDashboard(ctx = {}){
  matchingModuleHost = ctx.host || document.querySelector('[data-matching-module="native"]') || null;
  if (ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION) {
    setActiveMatchingDefinition(ctx.matchingDefinition || globalThis.CTOX_MATCHING_DEFINITION);
  }
  applyMatchingDefinitionUi();
  syncFeedback.setHostRoot?.(matchingModuleHost);
  syncFeedback.ensureHost();
  await ensureMatchScoreFormulaUpdate();
  await loadFromRxdb();

  renderSources();
  renderRequirements();
  renderObjects();
  renderMap();
  persistMatchingRuntimeState();
  bindCreateRequirementButton();

  // ✅ Live UI Sync: reagiert auf Background-Sync/Replication automatisch
  try {
    await setupRxdbLiveUiSync();
  } catch (e) {
    console.warn('[rxdb-live] setup failed:', e);
  }
}

function bindCreateRequirementButton() {
  const root = matchingModuleHost || document;
  const btn = root.querySelector('#createRequirementBtn');
  if (!btn || btn.dataset.bound === '1') return;
  btn.dataset.bound = '1';
  btn.addEventListener('click', (event) => {
    event.preventDefault();
    openCreateRequirementForm();
  });
}

function openCreateRequirementForm() {
  // Lightweight modal, styled to match the existing column-drawer/business
  // dialog look so it fits the column design without depending on the
  // bottom-drawer flow.
  const layer = document.createElement('div');
  layer.className = 'business-dialog-layer is-info is-open';
  layer.style.zIndex = '260';
  layer.innerHTML = `
    <section class="business-dialog" role="dialog" aria-modal="true" aria-labelledby="createRequirementDialogTitle">
      <div class="business-dialog-copy">
        <h2 id="createRequirementDialogTitle">Neue Anforderung anlegen</h2>
        <p>Hinterlege Titel und Beschreibung. Eine zugehörige Quelle wird automatisch angelegt, falls nicht ausgewählt.</p>
      </div>
      <label style="display:grid;gap:4px;margin-top:12px;font-size:11.5px;color:var(--muted);font-weight:700;text-transform:uppercase;letter-spacing:.04em;">
        Quelle
        <input class="business-dialog-input" data-field="sourceName" placeholder="z. B. Firmenname" style="text-transform:none;letter-spacing:0;font-weight:500;">
      </label>
      <label style="display:grid;gap:4px;margin-top:10px;font-size:11.5px;color:var(--muted);font-weight:700;text-transform:uppercase;letter-spacing:.04em;">
        Titel der Anforderung
        <input class="business-dialog-input" data-field="title" placeholder="z. B. Senior Backend Engineer" required style="text-transform:none;letter-spacing:0;font-weight:500;">
      </label>
      <label style="display:grid;gap:4px;margin-top:10px;font-size:11.5px;color:var(--muted);font-weight:700;text-transform:uppercase;letter-spacing:.04em;">
        Beschreibung
        <textarea class="business-dialog-input" data-field="description" rows="4" placeholder="Kurze Beschreibung der Anforderung…" style="text-transform:none;letter-spacing:0;font-weight:500;resize:vertical;min-height:90px;"></textarea>
      </label>
      <div class="business-dialog-actions" style="margin-top:14px;">
        <button class="business-dialog-secondary" type="button" data-action="cancel">Abbrechen</button>
        <button class="business-dialog-primary" type="button" data-action="save">Anlegen</button>
      </div>
    </section>
  `;
  document.body.append(layer);

  const close = () => {
    layer.classList.remove('is-open');
    layer.classList.add('is-closing');
    setTimeout(() => layer.remove(), 120);
  };

  layer.addEventListener('pointerdown', (event) => {
    if (event.target === layer) close();
  });

  layer.querySelector('[data-action="cancel"]')?.addEventListener('click', close);

  layer.querySelector('[data-action="save"]')?.addEventListener('click', async () => {
    const titleEl = layer.querySelector('[data-field="title"]');
    const sourceEl = layer.querySelector('[data-field="sourceName"]');
    const descEl = layer.querySelector('[data-field="description"]');
    const title = String(titleEl?.value || '').trim();
    if (!title) {
      titleEl?.focus?.();
      return;
    }
    const sourceName = String(sourceEl?.value || '').trim() || 'Manuelle Anforderung';
    const description = String(descEl?.value || '').trim();

    try {
      await createRequirementFromForm({ sourceName, title, description });
      close();
      await loadFromRxdb();
      renderSources();
      renderRequirements();
      renderMap();
    } catch (err) {
      console.error('[matching] createRequirementFromForm failed', err);
      if (typeof showBusinessAlert === 'function') {
        showBusinessAlert(`Anforderung konnte nicht angelegt werden: ${err?.message || err}`);
      }
    }
  });

  requestAnimationFrame(() => layer.querySelector('[data-field="title"]')?.focus?.());
}

async function createRequirementFromForm({ sourceName, title, description }) {
  if (!rxdb || !rxdb.sources || !rxdb.requirements) {
    await loadFromRxdb();
  }
  if (!rxdb?.sources || !rxdb?.requirements) {
    throw new Error('RxDB collections sind nicht bereit.');
  }
  const now = Date.now();
  const sourceId = `manualsrc_${now}_${Math.random().toString(16).slice(2, 8)}`;
  const requirementId = `manualreq_${now}_${Math.random().toString(16).slice(2, 8)}`;

  await rxdb.sources.upsert({
    id: sourceId,
    name: sourceName,
    legalName: sourceName,
    industry: '',
    website: '',
    locations: [],
    hasRelation: false,
    active: true,
    createdAt: new Date(now).toISOString(),
    updatedAt: new Date(now).toISOString(),
    status: 'active',
    created_at_ms: now,
    updated_at_ms: now,
  });

  await rxdb.requirements.upsert({
    id: requirementId,
    sourceId,
    title,
    aboutRole: description,
    aboutSource: '',
    objectRequirements: '',
    responsibilities: [],
    requirements: [],
    benefits: [],
    closingNotes: '',
    locationIds: [],
    workModel: 'Vollzeit',
    rawText: description,
    createdAt: new Date(now).toISOString(),
    updatedAt: new Date(now).toISOString(),
    status: 'active',
    created_at_ms: now,
    updated_at_ms: now,
  });
}

if (!document.querySelector('[data-matching-module="native"]')) {
  mountMatchingDashboard();
}

async function ensureMatchScoreFormulaUpdate() {
  try {
    const storedVersion = Number(localStorage.getItem(MATCH_SCORE_FORMULA_VERSION_KEY) || 0);
    if (storedVersion >= MATCH_SCORE_FORMULA_VERSION) {
      return { updated: false, total: 0 };
    }

    const result = await recomputeAllMatchScoresOnce();
    localStorage.setItem(MATCH_SCORE_FORMULA_VERSION_KEY, String(MATCH_SCORE_FORMULA_VERSION));
    console.info('[matching] Match-Scores mit neuer Formel neu berechnet:', result);
    return { ...result, updated: true };
  } catch (e) {
    console.error('[matching] Recompute fehlgeschlagen:', e);
    return { updated: false, total: 0, error: String(e?.message || e) };
  }
}
