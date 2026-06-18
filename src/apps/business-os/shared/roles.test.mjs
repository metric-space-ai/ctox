import test from 'node:test';
import assert from 'node:assert/strict';

import {
  assignableRolesForActor,
  normalizeRole,
  roleCanManage,
  roleDescription,
  roleDisplayName,
} from './roles.js';

test('role normalization preserves compatibility aliases', () => {
  assert.equal(normalizeRole('chef'), 'chef');
  assert.equal(normalizeRole('owner'), 'chef');
  assert.equal(normalizeRole('business_os_admin'), 'admin');
  assert.equal(normalizeRole('founder'), 'founder');
  assert.equal(normalizeRole('user'), 'user');
  assert.equal(normalizeRole('business_os_user'), 'user');
  assert.equal(normalizeRole('team'), 'user');
  assert.equal(normalizeRole('business_os_team'), 'user');
  assert.equal(normalizeRole('unknown'), 'user');
});

test('role labels use business-facing names', () => {
  assert.equal(roleDisplayName('chef'), 'Owner');
  assert.equal(roleDisplayName('owner'), 'Owner');
  assert.equal(roleDisplayName('admin'), 'Admin');
  assert.equal(roleDisplayName('founder'), 'App-Verantwortliche:r');
  assert.equal(roleDisplayName('user'), 'Teammitglied');
  assert.equal(roleDisplayName('team'), 'Teammitglied');
});

test('role descriptions avoid legacy raw role labels', () => {
  assert.match(roleDescription('founder'), /zugewiesene Apps/);
  assert.doesNotMatch(roleDescription('founder'), /Founder/);
  assert.match(roleDescription('team'), /freigegebene Business-OS Apps/);
});

test('role management remains owner and admin only', () => {
  assert.equal(roleCanManage('owner'), true);
  assert.equal(roleCanManage('chef'), true);
  assert.equal(roleCanManage('admin'), true);
  assert.equal(roleCanManage('founder'), false);
  assert.equal(roleCanManage('team'), false);
  assert.equal(roleCanManage('user'), false);
});

test('assignable role options keep owner transfer owner-only', () => {
  assert.deepEqual(assignableRolesForActor('chef'), ['user', 'founder', 'admin', 'chef']);
  assert.deepEqual(assignableRolesForActor('owner'), ['user', 'founder', 'admin', 'chef']);
  assert.deepEqual(assignableRolesForActor('admin'), ['user', 'founder', 'admin']);
  assert.deepEqual(assignableRolesForActor('founder'), []);
  assert.deepEqual(assignableRolesForActor('team'), []);
});
