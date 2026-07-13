async (page) => {
  const feature = 'document.comments-track-changes';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.evaluate((featureId) => fetch(`http://127.0.0.1:4180/reset/${featureId}`, { method: 'POST' }), feature);
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'), null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const oracle = page.frames().find((frame) => frame.url().includes('127.0.0.1:8088/') && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = page.frames().find((frame) => frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = page.frames().find((frame) => frame.url().includes('/ctox-document-comments-track-changes.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('comments/review comparison frames are missing');

  const inspect = (frame) => frame.evaluate(() => {
    const editorApi = window.editor || window.Asc?.editor;
    const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
    return {
      text: documentApi?.GetText?.() || '',
      comments: editorApi?.pluginMethod_GetAllComments?.() || [],
      has_revisions: editorApi?.asc_HaveRevisionsChanges?.() === true,
      revision_stack: editorApi?.asc_GetRevisionsChangesStack?.()?.length || 0,
      modified: editorApi?.isDocumentModified?.() === true,
      can_save: editorApi?.asc_isDocumentCanSave?.() === true,
    };
  });
  const initial = { oracle: await inspect(oracle), ctox: await inspect(ctox) };
  for (const [name, state] of Object.entries(initial)) {
    for (const marker of ['CTOX_EXISTING_INSERTION', 'CTOX_EXISTING_DELETION', 'PRESERVE_COMMENTS_TRACK_CHANGES_UNRELATED_C9E4']) {
      if (!state.text.includes(marker)) throw new Error(`${name} initial text missing ${marker}`);
    }
    if (!state.comments.some((comment) => comment.Data?.Text === 'CTOX_EXISTING_COMMENT_BODY')) {
      throw new Error(`${name} existing comment is missing`);
    }
    if (!state.has_revisions) throw new Error(`${name} existing revisions are missing`);
  }
  await page.screenshot({ path: `${output}/differential-initial.png` });

  const mutateComment = async (name, frame) => {
    const state = await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      documentApi.Search('COMMENT_CREATE_TARGET')[0].Select();
      const commentId = editorApi.pluginMethod_AddComment({
        Text: 'CTOX_ORACLE_COMMENT_BODY', UserName: 'CTOX', Solved: false, Replies: [],
      });
      const comment = editorApi.pluginMethod_GetAllComments().find((item) => item.Id === commentId);
      comment.Data.Replies.push({ Text: 'CTOX_ORACLE_COMMENT_REPLY', UserName: 'CTOX', Solved: false, Replies: [] });
      comment.Data.Solved = true;
      editorApi.pluginMethod_ChangeComment(comment.Id, comment.Data);

      return {
        text: documentApi.GetText(),
        comments: editorApi.pluginMethod_GetAllComments(),
        has_revisions: editorApi.asc_HaveRevisionsChanges(),
      };
    });
    if (!state.comments.some((comment) => comment.Data?.Text === 'CTOX_ORACLE_COMMENT_BODY')) throw new Error(`${name} comment creation failed`);
    return state;
  };
  const comments = { oracle: await mutateComment('oracle', oracle), ctox: await mutateComment('ctox', ctox) };
  const mutateReview = async (name, frame) => {
    await frame.getByRole('button', { name: 'Nachverfolgen von Änderungen' }).first().click();
    await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      const target = editorApi.getJsApi().GetDocument().Search('TRACK_INSERT_TARGET')[0];
      target.Select();
      window.focus();
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.type('_CTOX_TRACKED_INSERT');
    await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      editorApi.getJsApi().GetDocument().Search('TRACK_DELETE_TARGET')[0].Select();
      window.focus();
    });
    await page.keyboard.press('Backspace');
    await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      editorApi.pluginMethod_RejectReviewChanges(false);
      editorApi.pluginMethod_AcceptReviewChanges(true);
    });
    await page.waitForTimeout(1200);
    if (!await frame.evaluate(() => (window.editor || window.Asc?.editor).asc_IsTrackRevisions())) {
      await frame.getByRole('button', { name: 'Nachverfolgen von Änderungen' }).first().click();
    }
    await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      editorApi.getJsApi().GetDocument().Search('TRACK_INSERT_TARGET_CTOX_TRACKED_INSERT')[0].Select();
      window.focus();
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.type('_CTOX_FINAL_REVIEW');
    const state = await inspect(frame);
    if (!state.text.includes('TRACK_DELETE_TARGET') || state.text.includes('CTOX_EXISTING_DELETION')) {
      throw new Error(`${name} accept/reject state mismatch`);
    }
    return state;
  };
  const mutation = { oracle: await mutateReview('oracle', oracle), ctox: await mutateReview('ctox', ctox) };
  await page.waitForTimeout(750);
  const beforeSave = { oracle: await inspect(oracle), ctox: await inspect(ctox) };
  for (const [name, state] of Object.entries(beforeSave)) {
    for (const marker of ['_CTOX_TRACKED_INSERT', 'TRACK_DELETE_TARGET', '_CTOX_FINAL_REVIEW', 'CTOX_EXISTING_INSERTION']) {
      if (!state.text.includes(marker)) throw new Error(`${name} terminal text missing ${marker}`);
    }
    if (state.text.includes('CTOX_EXISTING_DELETION')) throw new Error(`${name} accepted deletion remains visible`);
    const root = state.comments.find((comment) => comment.Data?.Text === 'CTOX_ORACLE_COMMENT_BODY');
    if (!root?.Data?.Solved || root.Data.Replies?.[0]?.Text !== 'CTOX_ORACLE_COMMENT_REPLY') {
      throw new Error(`${name} comment thread mismatch: ${JSON.stringify(root)}`);
    }
    if (state.revision_stack < 1) throw new Error(`${name} final tracked revision is missing`);
    if (name === 'ctox' && (!state.modified || !state.can_save)) throw new Error(`${name} review state is not saveable`);
  }
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });

  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1, null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/document.comments-track-changes', { method: 'POST', body: bytes })).json();
  });
  let oracleState;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () => (await fetch('http://127.0.0.1:4180/state/document.comments-track-changes')).json());
    if (oracleState.saved) break;
    await page.waitForTimeout(1000);
  }
  if (!oracleState?.saved) throw new Error(`Oracle terminal save callback missing: ${JSON.stringify(oracleState)}`);
  return { feature_id: feature, interaction: 'real-documenteditor-comments-review-save-capture', initial, mutation, before_save: beforeSave, captured, oracle_state: oracleState };
}
