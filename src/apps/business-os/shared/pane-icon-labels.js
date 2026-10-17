// Icon + Wort (UI-Regel „Icons tragen ein Wort“, workjet#801): Kopf- und Werkzeugknöpfe
// mit `data-short-de` / `data-short-en` bekommen ein sichtbares Kurzwort neben dem Icon.
// Der ausführliche Text bleibt in aria-label/title.
export function applyPaneIconLabels(root, lang = 'de') {
  if (!root?.querySelectorAll) return;
  const key = lang === 'en' ? 'shortEn' : 'shortDe';
  for (const button of root.querySelectorAll('[data-short-de]')) {
    const word = button.dataset[key] || button.dataset.shortDe;
    let label = button.querySelector(':scope > .ctox-pane-icon-label');
    if (!label) {
      label = button.ownerDocument.createElement('span');
      label.className = 'ctox-pane-icon-label';
      button.append(label);
    }
    if (label.textContent !== word) label.textContent = word;
    button.classList.add('has-label');
  }
}
