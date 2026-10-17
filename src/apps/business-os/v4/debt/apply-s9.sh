#!/usr/bin/env bash
# Shell v4 – Löschpaket S9 (Strang DEL1, vorbereitet Do 15.10.2026; Beschreibung: LOESCHPAKET-S9.md).
#
# Löscht die Shell-Dateien, die an der alten Shell (app.js/index.html/mobile-host/Slot-Pipeline), an CI oder an
# Kern-Gegenstellen hängen. Erst ausführen, wenn der Kern (ctox sync v4) seine Gegenstellen entfernt hat.
#
#   bash src/apps/business-os/v4/debt/apply-s9.sh            # Trockenlauf: nur Bedingungen prüfen, nichts löschen
#   bash src/apps/business-os/v4/debt/apply-s9.sh --apply    # Bedingungen prüfen, dann git rm
#   bash src/apps/business-os/v4/debt/apply-s9.sh --stage 1 [--apply]   # nur Stufe 1 (Kern E37.3, 17.10.2026)
#
# Stufen (E37.3): Stufe 1 direkt nach dem v4→main-Merge, Stufe 2 (STAGE2 unten: die am 17.10. von V2-Gegenstellen
# blockierten Dateien) erst nach dem V2-Ende, wenn THESEN auf v4 ist. In Stufe 1 gehören die Stufe-2-Dateien NICHT
# zur Löschmenge: ein Verweis aus einer Stufe-2-Datei blockiert also auch eine Stufe-1-Datei (sonst bräche z. B.
# shell-generation.test.mjs, wenn shell-generation.js vorher ginge). Die Regel je Datei bleibt unverändert hart.
#
# Bedingung je Datei (generisch): kein Verweis mehr AUSSERHALB der Löschmenge – gemessen mit git grep über das
# ganze Repo (inkl. src/core, .github, tests, scripts), ohne *.md, vendor/, dist/, v4/debt/, v4/tools/guards/ (Löschlisten-Muster),
# generierte Manifeste (v4/manifests, v4/tools/manifest/out – werden danach neu erzeugt). Jeder Resttreffer wird mit
# Datei:Zeile ausgegeben; dann bricht das Skript ab (Exit 1) und löscht nichts. Die bekannten Kern-Gegenstellen
# stehen zusätzlich als Tabelle in LOESCHPAKET-S9.md.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
BOS=src/apps/business-os
APPLY=0
STAGE=all
for a in "$@"; do
  case "$a" in
    --apply) APPLY=1 ;;
    --stage) ;;
    1|2) STAGE=$a ;;
    --stage=1|--stage=2) STAGE=${a#--stage=} ;;
    *) echo "unbekanntes Argument: $a" >&2; exit 64 ;;
  esac
done

# Eintrag: "<Löschplan-Nr>|<Pfad relativ zu BOS>|<git-grep-ERE, das einen Verweis auf die Datei erkennt>"
ENTRIES=(
  "L-01|app.js|business-os/app\.js|['\"/]app\.js(\?|['\"])|\.\./app\.js|\./app\.js"
  "L-03|mobile-host.js|mobile-host\.js"
  "L-03|mobile-host.css|mobile-host\.css"
  "L-04|mobile-host.test.mjs|mobile-host\.test\.mjs"
  "L-03|qa/workjet-mobile-host-layout.test.mjs|workjet-mobile-host-layout"
  "L-05|mobile-apps.json|mobile-apps\.json"
  "L-06|system-apps.json|system-apps\.json"
  "L-07|modules/registry.json|modules/registry\.json|\.\./registry\.json|'registry\.json'"
  "L-11|modules/conversations/schema.js|conversations/schema\.js"
  "L-12|modules/ctox/schema.js|ctox/schema\.js"
  "L-13|modules/outbound/schema.js|outbound/schema\.js"
  "L-16|shared/collection-freshness.js|collection-freshness\.js"
  "L-17|shared/maintenance-state.js|maintenance-state\.js"
  "L-17|shared/maintenance-state.test.mjs|maintenance-state\.test\.mjs"
  "L-18|shared/shell-generation.js|shell-generation\.js"
  "L-18|shared/shell-generation.test.mjs|shell-generation\.test\.mjs"
  "L-19|shared/shell-release-status.js|shell-release-status\.js"
  "L-19|shared/shell-release-status.test.mjs|shell-release-status\.test\.mjs"
  "L-20|shared/startup-deadlines.js|startup-deadlines\.js"
  "L-20|shared/startup-deadlines.test.mjs|startup-deadlines\.test\.mjs"
  "L-20|scripts/assert-startup-deadlines-browser.mjs|assert-startup-deadlines-browser"
  "L-21|shared/command-bus.js|command-bus\.js"
  "L-21|shared/command-bus.test.mjs|command-bus\.test\.mjs"
  "L-21|shared/command-bus-receipt.test.mjs|command-bus-receipt\.test\.mjs"
  "L-24|scripts/assert-shell-v2-contract.mjs|assert-shell-v2-contract"
  "L-25|scripts/shell-v2-geometry-lab.mjs|shell-v2-geometry-lab"
  "L-26|scripts/generate-module-registry.mjs|generate-module-registry"
  "L-27|scripts/build-shell-artifact.mjs|build-shell-artifact\.mjs"
  "L-27|scripts/build-shell-artifact.test.mjs|build-shell-artifact\.test\.mjs"
  "L-27|scripts/sign-shell-release.mjs|sign-shell-release\.mjs"
  "L-27|scripts/sign-shell-release.test.mjs|sign-shell-release\.test\.mjs"
  "L-28|scripts/shell-generation-native-lab.mjs|shell-generation-native-lab"
  "L-28|scripts/shell-generation-native.browser.mjs|shell-generation-native\.browser"
  "L-29|scripts/assert-shell-release-chrome.mjs|assert-shell-release-chrome"
  "L-29|scripts/topbar-status.browser.mjs|topbar-status\.browser"
  "L-30|modules/mail/tests/mail-browser-qa.mjs|mail-browser-qa"
  "L-32a|design-lab.js|design-lab\.js"
  "L-32a|design-lab.html|design-lab\.html"
  "L-32b|shared/business-chat.js|(^|[^a-zA-Z0-9_-])business-chat\.js"
  "L-32b|shared/business-chat.test.mjs|(^|[^a-zA-Z0-9_-])business-chat\.test\.mjs"
  "L-33|shared/permissions.js|shared/permissions\.js|['\"]\./permissions\.js|\.\./permissions\.js"
  "L-33|shared/permissions.test.mjs|(^|[^a-zA-Z0-9_-])permissions\.test\.mjs"
  # L-01 Anhang: Tests/Skripte, die nur den Quelltext von app.js prüfen (keine weitere Importe, kein CI-Aufrufer
  # am 15.10.2026). Workjet-*-Tests gehören Phase 11 (ctox sync v4) und stehen NICHT hier.
  "L-01t|qa/ctox-desktop-shell.test.mjs|ctox-desktop-shell\.test"
  "L-01t|scripts/assert-permissions-ui.mjs|assert-permissions-ui"
  "L-01t|scripts/assert-settings-readiness-race.mjs|assert-settings-readiness-race"
  "L-01t|scripts/business-os-interactive-window-qa.contract.test.mjs|business-os-interactive-window-qa\.contract"
  "L-01t|scripts/smoke-app-module.contract.test.mjs|smoke-app-module\.contract"
  "L-01t|scripts/test-shell-page-exit-browser.mjs|test-shell-page-exit-browser"
  "L-01t|scripts/test-shell-page-exit.mjs|test-shell-page-exit\.mjs"
  "L-01t|shared/audience-route-denial.test.mjs|audience-route-denial\.test"
  "L-01t|shared/context-approval-receipt.test.mjs|context-approval-receipt\.test"
  "L-01t|shared/data-plane-startup.test.mjs|data-plane-startup\.test"
  "L-01t|shared/module-catalog-cache.test.mjs|module-catalog-cache\.test"
  "L-01t|shared/module-icon-startup.test.mjs|module-icon-startup\.test"
  "L-01t|shared/module-navigation.test.mjs|module-navigation\.test"
  "L-01t|shared/module-schema-dependencies.test.mjs|module-schema-dependencies\.test"
  "L-01t|shared/module-stylesheet-cache.test.mjs|module-stylesheet-cache\.test"
  "L-01t|shared/requested-module-startup.test.mjs|requested-module-startup\.test"
  "L-01t|shared/scoped-system-data-facade.test.mjs|scoped-system-data-facade\.test"
  "L-01t|shared/settings-drawer-sync.test.mjs|settings-drawer-sync\.test"
  "L-01t|shared/shell-responsive-header.test.mjs|shell-responsive-header\.test"
  "L-01t|shared/shell-v2-version-menu-contract.test.mjs|shell-v2-version-menu-contract"
  "L-01t|shared/startup-inventory-diagnostics.test.mjs|startup-inventory-diagnostics\.test"
  "L-01t|shared/thesen-browser-release-contract.test.mjs|thesen-browser-release-contract"
  "L-01t|shared/window-geometry-startup.test.mjs|window-geometry-startup\.test"
  "L-01t|shared/window-recovery.test.mjs|window-recovery\.test"
)

# Stufe 2 (E37.3): Trockenlauf 17.10.2026 08:4x auf c3135a252, 27 blockierte Einträge (V2-Gegenstellen), dazu 6 Dateien,
# die nur aus Stufe-2-Dateien importiert werden, und ihre 3 Tests (Modul und Test gehen zusammen), dazu outbound/schema.js (Verweis aus conversations/schema.js) – Stufe 1 = 29.
STAGE2=$'\n'"app.js mobile-host.js mobile-host.css mobile-apps.json system-apps.json modules/registry.json modules/ctox/schema.js
shared/collection-freshness.js shared/shell-generation.test.mjs shared/shell-release-status.js shared/command-bus.js
scripts/assert-shell-v2-contract.mjs scripts/shell-v2-geometry-lab.mjs scripts/generate-module-registry.mjs
scripts/build-shell-artifact.mjs scripts/build-shell-artifact.test.mjs scripts/sign-shell-release.mjs
scripts/sign-shell-release.test.mjs scripts/shell-generation-native.browser.mjs scripts/assert-shell-release-chrome.mjs
modules/mail/tests/mail-browser-qa.mjs design-lab.html shared/business-chat.js shared/business-chat.test.mjs
shared/permissions.js shared/permissions.test.mjs shared/audience-route-denial.test.mjs
modules/conversations/schema.js shared/maintenance-state.js shared/maintenance-state.test.mjs shared/shell-generation.js
shared/startup-deadlines.js shared/startup-deadlines.test.mjs scripts/assert-startup-deadlines-browser.mjs
scripts/shell-generation-native-lab.mjs design-lab.js modules/outbound/schema.js"
STAGE2=$'\n'$(printf '%s\n' $STAGE2)$'\n'
in_stage() { # <rel> → 0, wenn die Datei zur gewählten Stufe gehört
  local rel=$1
  case "$STAGE" in
    all) return 0 ;;
    1) [[ "$STAGE2" != *$'\n'"$rel"$'\n'* ]] ;;
    2) [[ "$STAGE2" == *$'\n'"$rel"$'\n'* ]] ;;
  esac
}

EXCLUDES=(':!*.md' ':!**/vendor/**' ':!**/dist/**' ":!$BOS/v4/debt/**" ":!$BOS/v4/tools/guards/**"
  ":!$BOS/v4/manifests/**" ":!$BOS/v4/tools/manifest/out/**")

DELSET=$'\n'   # Löschmenge als Zeilenliste (bash 3.2 auf macOS kennt keine assoziativen Arrays)
for e in "${ENTRIES[@]}"; do
  rel=${e#*|}; rel=${rel%%|*}
  in_stage "$rel" && DELSET+="$BOS/$rel"$'\n'
done

missing=0; blocked=0; deferred=0
for e in "${ENTRIES[@]}"; do
  id=${e%%|*}; rest=${e#*|}; rel=${rest%%|*}; pat=${rest#*|}
  path="$BOS/$rel"
  if ! in_stage "$rel"; then
    echo "STUFE$([[ $STAGE == 1 ]] && echo 2 || echo 1) $id $rel (nicht in dieser Stufe)"; deferred=$((deferred + 1)); continue
  fi
  if [[ ! -e "$path" ]]; then
    echo "WEG     $id $rel (schon gelöscht)"; missing=$((missing + 1)); continue
  fi
  hits=$(git grep -n -I --untracked -E "$pat" -- ':/' "${EXCLUDES[@]}" 2>/dev/null || true)
  rest_hits=""
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    file=${line%%:*}
    [[ "$file" == "$path" ]] && continue
    [[ "$DELSET" == *$'\n'"$file"$'\n'* ]] && continue
    # Fehltreffer: eigene app.js der Desktop-Apps/Office (Muster L-01) und die Generator-Liste (s. u.).
    if [[ "$id" == "L-01" ]]; then
      case "$file" in
        $BOS/desktop-apps/*|$BOS/office-engine/*|src/scripts/vendor-builds/build-ctox-office.mjs) continue ;;
      esac
    fi
    rest_hits+="    $line"$'\n'
  done <<< "$hits"
  if [[ -n "$rest_hits" ]]; then
    echo "BLOCKT  $id $rel – Verweise außerhalb der Löschmenge:"; printf '%s' "$rest_hits" | cut -c1-220
    blocked=$((blocked + 1))
  else
    echo "FREI    $id $rel"
  fi
done

total=${#ENTRIES[@]}
echo "Summe (Stufe ${STAGE}): ${total} Einträge, andere Stufe ${deferred}, blockiert ${blocked}, schon weg ${missing}, frei $((total - deferred - blocked - missing))"
if (( blocked > 0 )); then
  echo "Abbruch: erst die Verweise entfernen (Kern-Gegenstellen: LOESCHPAKET-S9.md §2). Nichts gelöscht."
  exit 1
fi
if (( APPLY == 0 )); then
  echo "Trockenlauf ok – mit --apply ausführen."
  exit 0
fi

for e in "${ENTRIES[@]}"; do
  rel=${e#*|}; rel=${rel%%|*}
  in_stage "$rel" || continue
  [[ -e "$BOS/$rel" ]] && git rm -q -- "$BOS/$rel"
done
echo "gelöscht. Jetzt die Prüfkommandos aus LOESCHPAKET-S9.md §4 ausführen (Generator-Liste kürzen, Manifeste neu)."
