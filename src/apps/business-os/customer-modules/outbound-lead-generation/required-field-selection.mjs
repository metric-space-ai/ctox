export function optionalKeysForRequiredCheckbox(optionalKeys, fieldKey, checked) {
  if (typeof fieldKey !== 'string' || fieldKey.trim() === '') throw new TypeError('fieldKey must be a non-empty string');
  if (typeof checked !== 'boolean') throw new TypeError('checked must be a boolean');
  const next = new Set(optionalKeys);
  if (checked) next.delete(fieldKey);
  else next.add(fieldKey);
  return next;
}
