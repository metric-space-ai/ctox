export function normalizeRole(role) {
  const value = String(role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (value === 'owner') return 'chef';
  if (value === 'team') return 'user';
  if (['chef', 'admin', 'founder', 'user'].includes(value)) return value;
  return 'user';
}

export function roleDisplayName(role) {
  const value = normalizeRole(role);
  return {
    chef: 'Owner',
    admin: 'Admin',
    founder: 'App-Verantwortliche:r',
    user: 'Teammitglied',
  }[value] || value;
}

export function roleDescription(role) {
  const value = normalizeRole(role);
  return {
    chef: 'Verantwortet Instanz, Rollen, kritische Einstellungen, Apps und Agentenzugriff.',
    admin: 'Verwaltet Nutzer, Apps, Zuweisungen, Runtime und operative Einstellungen.',
    founder: 'Verantwortet zugewiesene Apps und darf diese fachlich bearbeiten.',
    user: 'Nutzt freigegebene Business-OS Apps und Daten.',
  }[value] || 'Rolle dieser Business-OS Sitzung.';
}

export function roleCanManage(role) {
  return ['chef', 'admin'].includes(normalizeRole(role));
}

export function assignableRolesForActor(role) {
  const value = normalizeRole(role);
  if (value === 'chef') return ['user', 'founder', 'admin', 'chef'];
  if (value === 'admin') return ['user', 'founder', 'admin'];
  return [];
}
