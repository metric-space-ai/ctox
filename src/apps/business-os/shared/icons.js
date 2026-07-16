/**
 * Premium, multi-layered, gradient-filled vector icons for ctox Business OS.
 * Custom crafted to deliver a gorgeous glassmorphic look similar to high-end OS interfaces.
 */

const iconMap = {
  desktop: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-desktop">
      <defs>
        <linearGradient id="grad-desktop" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#94a3b8" />
          <stop offset="100%" stop-color="#3b82f6" />
        </linearGradient>
      </defs>
      <!-- Glass monitor panel -->
      <rect x="2" y="3" width="20" height="14" rx="3" ry="3" fill="url(#grad-desktop)" fill-opacity="0.12" stroke="url(#grad-desktop)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <!-- Stand and base -->
      <path d="M12 17v4M8 21h8" stroke="url(#grad-desktop)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Modern layout cards inside screen -->
      <rect x="5" y="6" width="6" height="4" rx="1" fill="url(#grad-desktop)" fill-opacity="0.2" stroke="url(#grad-desktop)" stroke-width="1"></rect>
      <rect x="13" y="6" width="6" height="8" rx="1" fill="url(#grad-desktop)" fill-opacity="0.2" stroke="url(#grad-desktop)" stroke-width="1"></rect>
      <rect x="5" y="12" width="6" height="2" rx="0.5" fill="url(#grad-desktop)" fill-opacity="0.2" stroke="url(#grad-desktop)" stroke-width="1"></rect>
    </svg>
  `,
  browser: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-browser">
      <defs>
        <linearGradient id="grad-browser" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#0ea5e9" />
          <stop offset="100%" stop-color="#22c55e" />
        </linearGradient>
      </defs>
      <rect x="3" y="4" width="18" height="16" rx="3" fill="url(#grad-browser)" fill-opacity="0.12" stroke="url(#grad-browser)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <path d="M3 9h18" stroke="url(#grad-browser)" stroke-width="${stroke}" stroke-linecap="round"></path>
      <circle cx="7" cy="6.5" r="0.8" fill="url(#grad-browser)"></circle>
      <circle cx="10" cy="6.5" r="0.8" fill="url(#grad-browser)"></circle>
      <path d="M8 15h8M12 11v8" stroke="url(#grad-browser)" stroke-width="1.7" stroke-linecap="round"></path>
    </svg>
  `,
  ctox: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-ctox">
      <defs>
        <linearGradient id="grad-ctox" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#10b981" />
          <stop offset="100%" stop-color="#06b6d4" />
        </linearGradient>
      </defs>
      <!-- 3D Glowing cube shell -->
      <polygon points="12 2 22 8 22 16 12 22 2 16 2 8" fill="url(#grad-ctox)" fill-opacity="0.12" stroke="url(#grad-ctox)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polygon>
      <polyline points="12 22 12 12 22 8" stroke="url(#grad-ctox)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <polyline points="12 12 2 8" stroke="url(#grad-ctox)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <polyline points="12 2 12 12" stroke="url(#grad-ctox)" stroke-width="1.5" stroke-dasharray="2 2" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <!-- Core glow node -->
      <circle cx="12" cy="12" r="3.5" fill="url(#grad-ctox)" stroke="#ffffff" stroke-width="1"></circle>
    </svg>
  `,
  documents: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-documents">
      <defs>
        <linearGradient id="grad-documents" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#3b82f6" />
          <stop offset="100%" stop-color="#6366f1" />
        </linearGradient>
      </defs>
      <!-- Folder/Page Stack sheet -->
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z" fill="url(#grad-documents)" fill-opacity="0.12" stroke="url(#grad-documents)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M14 2v4a2 2 0 0 0 2 2h4" stroke="url(#grad-documents)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Page lines -->
      <line x1="8" y1="12" x2="16" y2="12" stroke="url(#grad-documents)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <line x1="8" y1="16" x2="16" y2="16" stroke="url(#grad-documents)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <!-- Top short line -->
      <line x1="8" y1="8" x2="10" y2="8" stroke="url(#grad-documents)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
    </svg>
  `,
  knowledge: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-knowledge">
      <defs>
        <linearGradient id="grad-knowledge" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#8b5cf6" />
          <stop offset="100%" stop-color="#d946ef" />
        </linearGradient>
      </defs>
      <!-- Book spine and covers stack -->
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" stroke="url(#grad-knowledge)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" fill="url(#grad-knowledge)" fill-opacity="0.12" stroke="url(#grad-knowledge)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Modern bookmark / ribbon -->
      <path d="M12 2v10l2.5-2 2.5 2V2z" fill="url(#grad-knowledge)" fill-opacity="0.25" stroke="url(#grad-knowledge)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Tech sparks -->
      <circle cx="9" cy="12" r="1.5" fill="url(#grad-knowledge)"></circle>
      <circle cx="14" cy="15" r="1" fill="url(#grad-knowledge)"></circle>
    </svg>
  `,
  matching: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-matching">
      <defs>
        <linearGradient id="grad-matching" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#f59e0b" />
          <stop offset="100%" stop-color="#ea580c" />
        </linearGradient>
      </defs>
      <!-- Two intersecting, glowing chain links -->
      <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" fill="url(#grad-matching)" fill-opacity="0.12" stroke="url(#grad-matching)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" fill="url(#grad-matching)" fill-opacity="0.12" stroke="url(#grad-matching)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Spark nodes at connection points -->
      <circle cx="12" cy="12" r="2.5" fill="#ffffff" stroke="url(#grad-matching)" stroke-width="1"></circle>
    </svg>
  `,
  outbound: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-outbound">
      <defs>
        <linearGradient id="grad-outbound" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#ec4899" />
          <stop offset="100%" stop-color="#f43f5e" />
        </linearGradient>
      </defs>
      <!-- Paper airplane with wind trails -->
      <line x1="22" y1="2" x2="11" y2="13" stroke="url(#grad-outbound)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <polygon points="22 2 15 22 11 13 2 9 22 2" fill="url(#grad-outbound)" fill-opacity="0.12" stroke="url(#grad-outbound)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polygon>
      <path d="M6 19c3-1 6-1 9-3" stroke="url(#grad-outbound)" stroke-width="1.5" stroke-dasharray="2 2" stroke-linecap="round"></path>
    </svg>
  `,
  reports: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-reports">
      <defs>
        <linearGradient id="grad-reports" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#ef4444" />
          <stop offset="100%" stop-color="#f97316" />
        </linearGradient>
      </defs>
      <!-- Base analytics chart with alert nodes / bug target -->
      <rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-reports)" fill-opacity="0.12" stroke="url(#grad-reports)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <path d="M18 17V10M12 17V6M6 17v-4" stroke="url(#grad-reports)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Indicator dot -->
      <circle cx="12" cy="6" r="2" fill="#ffffff" stroke="url(#grad-reports)" stroke-width="1.2"></circle>
    </svg>
  `,
  research: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-research">
      <defs>
        <linearGradient id="grad-research" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#0891b2" />
          <stop offset="100%" stop-color="#10b981" />
        </linearGradient>
      </defs>
      <!-- Lab Flask / Atoms -->
      <path d="M6 3h12" stroke="url(#grad-research)" stroke-width="${stroke}" stroke-linecap="round"></path>
      <path d="M8 3v4c0 1.66-1.34 3-3 3v0a7 7 0 0 0-2 4.9V20a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5.1a7 7 0 0 0-2-4.9v0c-1.66 0-3-1.34-3-3V3" fill="url(#grad-research)" fill-opacity="0.12" stroke="url(#grad-research)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <line x1="8.5" y1="11" x2="15.5" y2="11" stroke="url(#grad-research)" stroke-width="${stroke}"></line>
      <!-- Atom / Nucleus details -->
      <circle cx="12" cy="16" r="2.5" fill="url(#grad-research)"></circle>
      <circle cx="9" cy="18" r="1" fill="#ffffff"></circle>
      <circle cx="15" cy="15" r="1" fill="#ffffff"></circle>
    </svg>
  `,
  conversations: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-conversations">
      <defs>
        <linearGradient id="grad-conversations" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#4f46e5" />
          <stop offset="100%" stop-color="#7c3aed" />
        </linearGradient>
      </defs>
      <!-- Layered speech capsules -->
      <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" fill="url(#grad-conversations)" fill-opacity="0.12" stroke="url(#grad-conversations)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Communication nodes inside -->
      <circle cx="9" cy="11" r="1.5" fill="url(#grad-conversations)"></circle>
      <circle cx="13" cy="11" r="1.5" fill="url(#grad-conversations)"></circle>
      <circle cx="17" cy="11" r="1.5" fill="url(#grad-conversations)"></circle>
    </svg>
  `,
  explorer: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-explorer">
      <defs>
        <linearGradient id="grad-explorer" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#475569" />
          <stop offset="100%" stop-color="#94a3b8" />
        </linearGradient>
      </defs>
      <!-- Sleek folder with open tab style -->
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" fill="url(#grad-explorer)" fill-opacity="0.12" stroke="url(#grad-explorer)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M2 10h20" stroke="url(#grad-explorer)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
    </svg>
  `,
  files: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-files">
      <defs>
        <linearGradient id="grad-files" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#475569" />
          <stop offset="100%" stop-color="#94a3b8" />
        </linearGradient>
      </defs>
      <!-- Sleek folder with open tab style (same as explorer) -->
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" fill="url(#grad-files)" fill-opacity="0.12" stroke="url(#grad-files)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M2 10h20" stroke="url(#grad-files)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
    </svg>
  `,
  notes: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-notes">
      <defs>
        <linearGradient id="grad-notes" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#eab308" />
          <stop offset="100%" stop-color="#d97706" />
        </linearGradient>
      </defs>
      <!-- Writing notebook -->
      <path d="M16 2H4a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z" fill="url(#grad-notes)" fill-opacity="0.12" stroke="url(#grad-notes)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <!-- Binding loops -->
      <path d="M2 6h2M2 10h2M2 14h2M2 18h2" stroke="url(#grad-notes)" stroke-width="1.5" stroke-linecap="round"></path>
      <!-- Pen/Pencil on top -->
      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L11 16l-4 1 1-4 10.5-10.5z" fill="url(#grad-notes)" fill-opacity="0.3" stroke="url(#grad-notes)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
    </svg>
  `,
  'code-editor': (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-code-editor">
      <defs>
        <linearGradient id="grad-code-editor" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#8b5cf6" />
          <stop offset="100%" stop-color="#3b82f6" />
        </linearGradient>
      </defs>
      <rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-code-editor)" fill-opacity="0.12" stroke="url(#grad-code-editor)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <polyline points="9 8 5 12 9 16" stroke="url(#grad-code-editor)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <polyline points="15 8 19 12 15 16" stroke="url(#grad-code-editor)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <line x1="13" y1="7" x2="11" y2="17" stroke="url(#grad-code-editor)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
    </svg>
  `,
  'file-viewer': (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-file-viewer">
      <defs>
        <linearGradient id="grad-file-viewer" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#475569" />
          <stop offset="100%" stop-color="#1e293b" />
        </linearGradient>
      </defs>
      <rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-file-viewer)" fill-opacity="0.12" stroke="url(#grad-file-viewer)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <!-- Dynamic split-window / image representation -->
      <line x1="9" y1="3" x2="9" y2="21" stroke="url(#grad-file-viewer)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <circle cx="15" cy="8" r="2" stroke="url(#grad-file-viewer)" stroke-width="1.5"></circle>
      <path d="M10 18l3-3 4 4" stroke="url(#grad-file-viewer)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path>
    </svg>
  `,
  shiftflow: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-shiftflow">
      <defs>
        <linearGradient id="grad-shiftflow" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#8b5cf6" />
          <stop offset="100%" stop-color="#7c3aed" />
        </linearGradient>
      </defs>
      <rect x="3" y="4" width="18" height="16" rx="3" fill="url(#grad-shiftflow)" fill-opacity="0.12" stroke="url(#grad-shiftflow)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <line x1="3" y1="9" x2="21" y2="9" stroke="url(#grad-shiftflow)" stroke-width="${stroke}" stroke-linecap="round"></line>
      <line x1="9" y1="9" x2="9" y2="20" stroke="url(#grad-shiftflow)" stroke-width="1" stroke-dasharray="2 2" stroke-linecap="round"></line>
      <line x1="15" y1="9" x2="15" y2="20" stroke="url(#grad-shiftflow)" stroke-width="1" stroke-dasharray="2 2" stroke-linecap="round"></line>
      <rect x="5" y="12" width="8" height="4" rx="1.5" fill="url(#grad-shiftflow)" fill-opacity="0.3" stroke="url(#grad-shiftflow)" stroke-width="1"></rect>
      <circle cx="17" cy="15" r="2.5" stroke="url(#grad-shiftflow)" stroke-width="1.2"></circle>
      <polyline points="17 13.5 17 15 18 15" stroke="url(#grad-shiftflow)" stroke-width="1" stroke-linecap="round"></polyline>
    </svg>
  `,
  spreadsheets: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-spreadsheets">
      <defs>
        <linearGradient id="grad-spreadsheets" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#10b981" />
          <stop offset="100%" stop-color="#059669" />
        </linearGradient>
      </defs>
      <rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-spreadsheets)" fill-opacity="0.12" stroke="url(#grad-spreadsheets)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <line x1="9" y1="3" x2="9" y2="21" stroke="url(#grad-spreadsheets)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <line x1="3" y1="9" x2="21" y2="9" stroke="url(#grad-spreadsheets)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <line x1="3" y1="15" x2="21" y2="15" stroke="url(#grad-spreadsheets)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <path d="M5 17l3-3 4 2 4-4" stroke="url(#grad-spreadsheets)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path>
      <circle cx="16" cy="12" r="1.5" fill="#ffffff" stroke="url(#grad-spreadsheets)" stroke-width="1"></circle>
    </svg>
  `,
  creator: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-creator">
      <defs>
        <linearGradient id="grad-creator" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#06b6d4" />
          <stop offset="100%" stop-color="#0891b2" />
        </linearGradient>
      </defs>
      <polyline points="7 8 3 12 7 16" stroke="url(#grad-creator)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <polyline points="17 8 21 12 17 16" stroke="url(#grad-creator)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></polyline>
      <line x1="14" y1="6" x2="10" y2="18" stroke="url(#grad-creator)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></line>
      <path d="M18 4l.5 1.5L20 6l-1.5.5L18 8l-.5-1.5L16 6l1.5-.5z" fill="url(#grad-creator)"></path>
      <path d="M6 18l.25.75L7 19l-.75.25L6 20l-.25-.75L5 19l.75-.25z" fill="url(#grad-creator)"></path>
    </svg>
  `,
  'app-store': (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-app-store">
      <defs>
        <linearGradient id="grad-app-store" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#f59e0b" />
          <stop offset="100%" stop-color="#ec4899" />
        </linearGradient>
      </defs>
      <path d="M21 8H3a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2z" fill="url(#grad-app-store)" fill-opacity="0.12" stroke="url(#grad-app-store)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <path d="M16 8A4 4 0 0 0 8 8" stroke="url(#grad-app-store)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></path>
      <rect x="5" y="12" width="5" height="5" rx="1" fill="url(#grad-app-store)" fill-opacity="0.25" stroke="url(#grad-app-store)" stroke-width="1.2"></rect>
      <rect x="14" y="12" width="5" height="5" rx="1" fill="url(#grad-app-store)" fill-opacity="0.25" stroke="url(#grad-app-store)" stroke-width="1.2"></rect>
    </svg>
  `,
  fallback: (size, stroke) => `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-fallback">
      <defs>
        <linearGradient id="grad-fallback" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="#475569" />
          <stop offset="100%" stop-color="#3b82f6" />
        </linearGradient>
      </defs>
      <rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-fallback)" fill-opacity="0.12" stroke="url(#grad-fallback)" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <circle cx="12" cy="12" r="4" stroke="url(#grad-fallback)" stroke-width="${stroke}"></circle>
    </svg>
  `
};

/**
 * Functional action icons — the standard glyph set for .ctox-pane-icon and
 * .ctox-icon-button controls. Unlike the gradient module icons above, action
 * icons are monochrome (stroke: currentColor) so they inherit the button's
 * hover/active/disabled color. One style for every app: 24 viewBox, round
 * caps, no fills. Modules must use getActionIcon() instead of inlining SVGs.
 */
const actionIconPaths = {
  add: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6L6 18',
  check: 'M4.5 12.5l5 5L19.5 7',
  search: 'M10.8 4a6.8 6.8 0 1 1 0 13.6 6.8 6.8 0 0 1 0-13.6ZM15.8 15.8 20.5 20.5',
  filter: 'M4 6h16M7 12h10M10 18h4',
  sort: 'M8 5v14M8 5 5 8M8 5l3 3M16 19V5M16 19l-3-3M16 19l3-3',
  edit: 'M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17v3ZM14.5 6.5l3 3',
  trash: 'M5 7h14M10 7V5h4v2M8 7l1 13h6l1-13M10.5 11v5M13.5 11v5',
  refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4',
  download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
  upload: 'M12 15V4M12 4 8 8M12 4l4 4M5 19h14',
  copy: 'M9 9h10v11H9zM5 15V4h10',
  link: 'M10 14a4 4 0 0 0 6 .4l3-3a4 4 0 0 0-5.6-5.6L12 7.2M14 10a4 4 0 0 0-6-.4l-3 3a4 4 0 0 0 5.6 5.6L12 16.8',
  open: 'M14 5h5v5M19 5l-8 8M11 5H5v14h14v-6',
  send: 'M4 12 20 4l-4 16-4.5-6.5L4 12ZM11.5 13.5 20 4',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
  chevronDown: 'M6 9l6 6 6-6',
  chevronUp: 'M6 15l6-6 6 6',
  chevronLeft: 'M15 6l-6 6 6 6',
  chevronRight: 'M9 6l6 6-6 6',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
  calendar: 'M5 6h14v14H5zM5 10h14M9 4v4M15 4v4',
  clock: 'M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15ZM12 8v4.5l3 2',
  tag: 'M4 4h7l9 9-7 7-9-9V4ZM8.5 8.5h.01',
  user: 'M12 5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM5 20a7 7 0 0 1 14 0',
  users: 'M9 6a3 3 0 1 1 0 6 3 3 0 0 1 0-6ZM3.5 19a5.5 5.5 0 0 1 11 0M15.5 6.5a3 3 0 0 1 0 5.5M17 13.5a5.5 5.5 0 0 1 3.5 5',
  eye: 'M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6ZM12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
  pin: 'M9 4h6l-1 7 3 2v2H7v-2l3-2-1-7ZM12 15v5',
  star: 'M12 4l2.35 4.9 5.4.7-4 3.75 1 5.35L12 16.1l-4.75 2.6 1-5.35-4-3.75 5.4-.7L12 4Z',
  folder: 'M4 6h5l2 2h9v11H4V6Z',
  file: 'M7 3h7l4 4v14H7V3ZM14 3v4h4',
  print: 'M7 8V4h10v4M7 16H4V8h16v8h-3M7 13h10v7H7v-7Z',
  archive: 'M4 5h16v4H4zM6 9v10h12V9M10 13h4',
  bell: 'M6 16v-5a6 6 0 0 1 12 0v5l1.5 2.5h-15L6 16ZM10 21a2 2 0 0 0 4 0',
  comment: 'M4 5h16v11H9l-5 4V5Z',
  attach: 'M8 12.5 15.5 5a3.5 3.5 0 0 1 5 5l-9 9a5.5 5.5 0 0 1-7.8-7.8L11 4',
  grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
  columns: 'M4 4h16v16H4zM10 4v16M16 4v16',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  play: 'M8 5.5v13l10-6.5-10-6.5Z',
  pause: 'M8 5h3v14H8zM13 5h3v14h-3z',
  warning: 'M12 4 2.8 19.5h18.4L12 4ZM12 10v4M12 17h.01',
  info: 'M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15ZM12 11v5M12 8h.01',
  lock: 'M6 11h12v9H6v-9ZM8.5 11V8a3.5 3.5 0 0 1 7 0v3M12 14.5v2',
  unlock: 'M6 11h12v9H6v-9ZM8.5 11V8a3.5 3.5 0 0 1 6.8-1.2M12 14.5v2',
  book: 'M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5v-15ZM4 18a2.5 2.5 0 0 1 2.5-2.5H20M8 7h8',
};

export function getActionIcon(name, size = 16, strokeWidth = 1.8) {
  const path = actionIconPaths[name] || actionIconPaths.more;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="ctox-action-icon ctox-action-${name}"><path d="${path}"></path></svg>`;
}

export function listActionIcons() {
  return Object.keys(actionIconPaths);
}

const registeredIcons = new Map();

export function registerSvgIcon(moduleId, svgString) {
  if (!moduleId || !svgString) return;
  let key = String(moduleId).trim().toLowerCase();
  if (key.startsWith('module:')) {
    key = key.slice('module:'.length);
  }
  if (key.startsWith('desktop-app:')) {
    key = key.slice('desktop-app:'.length);
  }
  registeredIcons.set(key, svgString);
}

export function getSvgIcon(moduleId, size = 24, strokeWidth = 2) {
  // Normalize module key
  let key = String(moduleId || '').trim().toLowerCase();
  if (key.startsWith('module:')) {
    key = key.slice('module:'.length);
  }
  if (key.startsWith('desktop-app:')) {
    key = key.slice('desktop-app:'.length);
  }

  // Check registered custom icons first
  if (registeredIcons.has(key)) {
    const rawSvg = registeredIcons.get(key);
    if (typeof rawSvg === 'function') {
      return rawSvg(size, strokeWidth).trim();
    }
    let svg = String(rawSvg).trim();
    if (svg.includes('<svg')) {
      // replace or inject width/height attributes
      svg = svg.replace(/width="[^"]*"/, `width="${size}"`);
      svg = svg.replace(/height="[^"]*"/, `height="${size}"`);
      // replace stroke-width attributes
      svg = svg.replace(/stroke-width="[^"]*"/g, `stroke-width="${strokeWidth}"`);
    }
    return svg;
  }

  const generator = iconMap[key];
  if (generator) {
    return generator(size, strokeWidth).trim();
  }

  // No curated or registered icon: derive a deterministic monogram icon so
  // custom / generated apps look distinct and intentional instead of all
  // sharing one placeholder glyph.
  return buildMonogramIcon(key, size, strokeWidth);
}

// Curated gradient pairs matching the premium module-icon aesthetic. Both
// stops stay vivid enough to read in the light and dark shell themes.
const MONOGRAM_GRADIENTS = [
  ['#6366f1', '#8b5cf6'],
  ['#0ea5e9', '#22c55e'],
  ['#10b981', '#06b6d4'],
  ['#f59e0b', '#ef4444'],
  ['#ec4899', '#8b5cf6'],
  ['#3b82f6', '#06b6d4'],
  ['#14b8a6', '#3b82f6'],
  ['#f97316', '#eab308'],
  ['#8b5cf6', '#ec4899'],
  ['#475569', '#3b82f6'],
];

function hashKey(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i += 1) {
    hash = (hash * 31 + str.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

function monogramLetter(key) {
  const match = String(key).match(/[a-z0-9]/i);
  return match ? match[0].toUpperCase() : '◻';
}

function buildMonogramIcon(key, size, strokeWidth) {
  const safeKey = String(key || '').trim();
  if (!safeKey) {
    return iconMap.fallback(size, strokeWidth).trim();
  }
  const hash = hashKey(safeKey);
  const [from, to] = MONOGRAM_GRADIENTS[hash % MONOGRAM_GRADIENTS.length];
  const letter = monogramLetter(safeKey);
  const gradId = `grad-mono-${hash.toString(36)}`;
  return `
    <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" class="svg-icon svg-monogram">
      <defs>
        <linearGradient id="${gradId}" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stop-color="${from}" />
          <stop offset="100%" stop-color="${to}" />
        </linearGradient>
      </defs>
      <rect x="3" y="3" width="18" height="18" rx="5" fill="url(#${gradId})" fill-opacity="0.14" stroke="url(#${gradId})" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round"></rect>
      <text x="12" y="12" fill="url(#${gradId})" font-size="11" font-weight="600" font-family="ui-sans-serif, -apple-system, 'Segoe UI', sans-serif" text-anchor="middle" dominant-baseline="central">${letter}</text>
    </svg>
  `.trim();
}
