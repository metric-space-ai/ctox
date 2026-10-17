import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd, rejects } from './_ctx.mjs';
import { setup, topic } from './_fixtures.mjs';

const { default: handle } = await import('../news.topic.schedule.js');

function scheduler(initial = []) {
  const tasks = [...initial];
  return {
    tasks,
    caps: {
      'schedule.list': async () => tasks,
      'schedule.upsert': async (req) => {
        let t = tasks.find((x) => x.name === req.name);
        if (!t) tasks.push((t = { task_id: 'sched_1', enabled: false, next_run_at: '2026-10-16T07:00:00+02:00' }));
        Object.assign(t, req);
        return { ...t };
      },
      'schedule.set_enabled': async ({ task_id, enabled }) => {
        const t = tasks.find((x) => x.task_id === task_id);
        t.enabled = enabled;
        return { ...t };
      },
      'schedule.remove': async ({ task_id }) => tasks.splice(tasks.findIndex((x) => x.task_id === task_id), 1),
    },
  };
}

test('aktiv: legt den Termin mit Route an, aktiviert ihn und schreibt schedule_task_id', async () => {
  const s = scheduler();
  const t = await setup({ seed: { news_topics: [topic()] }, caps: s.caps });
  const r = await handle(cmd('news.topic.schedule', {
    topic_id: 'topic_1', cadence: 'deadline', cron_expr: ' 0 7 1 11 * ', enabled: true, deadline_date: '2026-11-01', timezone: 'Mars/Olympus',
  }), t.ctx);
  assert.deepEqual(r, { ok: true, topic_id: 'topic_1', enabled: true, schedule_task_id: 'sched_1', next_run_at: '2026-10-16T07:00:00+02:00' });
  const ensured = s.tasks[0];
  assert.equal(ensured.name, 'news:topic_1');
  assert.equal(ensured.thread_key, 'business-os/news/topic_1');
  assert.equal(ensured.cron_expr, '0 7 1 11 *');
  assert.deepEqual(ensured.calendar, { timezone: 'Europe/Berlin', lead_minutes: 0 });
  // Rust-Test schedule_route_round_trips_through_the_prompt
  assert.deepEqual(JSON.parse(ensured.prompt.slice('ctox-news-schedule:'.length)), {
    topic_id: 'topic_1', owner_user_id: 'user_owner', cadence: 'deadline', deadline_date: '2026-11-01',
  });
  assert.equal(ensured.command.type, 'news.issue.generate');
  assert.equal(ensured.command.payload.trigger, 'schedule');
  assert.equal(ensured.enabled, true);
  assert.equal(t.doc('news_topics', 'topic_1').schedule_task_id, 'sched_1');
});

test('deaktivieren pausiert, loeschen entfernt (ohne Rueckschrieb)', async () => {
  const s = scheduler([{ task_id: 'sched_9', name: 'news:topic_1', thread_key: 'business-os/news/topic_1', enabled: true }]);
  const t = await setup({ seed: { news_topics: [topic({ schedule_task_id: 'sched_9' })] }, caps: s.caps });
  const off = await handle(cmd('news.topic.schedule', { topic_id: 'topic_1', cron_expr: '0 7 * * *', enabled: false }), t.ctx);
  assert.deepEqual(off, { ok: true, topic_id: 'topic_1', enabled: false, removed: false, schedule_task_id: 'sched_9' });
  assert.equal(s.tasks[0].enabled, false);
  const gone = await handle(cmd('news.topic.schedule', { topic_id: 'topic_1', deleted: true }), t.ctx);
  assert.equal(gone.removed, true);
  assert.equal(s.tasks.length, 0);
});

test('leerer cron gilt als inaktiv; topic_id-Rueckfall auf record_id; Pruefungen', async () => {
  const s = scheduler();
  const t = await setup({ seed: { news_topics: [topic()] }, caps: s.caps });
  const r = await handle(cmd('news.topic.schedule', { cron_expr: '  ' }, { record_id: 'topic_1' }), t.ctx);
  assert.equal(r.enabled, false);
  assert.equal(t.doc('news_topics', 'topic_1').schedule_task_id, '');
  await rejects(handle(cmd('news.topic.schedule', { topic_id: '../x' }), t.ctx), 'invalid');
  await rejects(handle(cmd('news.topic.schedule', { topic_id: 'topic_1' }, { app_id: 'outbound' }), t.ctx), 'forbidden');
  await rejects(handle(cmd('news.topic.schedule', { topic_id: 'topic_1', cron_expr: '0 7 * * *' }, { actor: {} }), t.ctx), 'forbidden', /authenticated user/);
});
