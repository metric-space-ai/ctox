async (page) => {
  const feature = 'document.sections-headers-footers';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.evaluate((featureId) =>
    fetch(`http://127.0.0.1:4180/reset/${featureId}`, { method: 'POST' }),
  feature);
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'), null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-sections-headers-footers.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('sections/header/footer comparison frames are missing');

  const closeNotice = async (frame) => {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
  };

  const inspectSections = async (frame) => frame.evaluate(() => {
    const editorApi = window.editor || window.Asc?.editor;
    const api = editorApi?.getJsApi?.();
    const documentApi = api?.GetDocument?.();
    if (!editorApi || !documentApi) throw new Error('Document Builder API is unavailable');
    const contentText = (content) => {
      if (!content) return '';
      if (typeof content.GetText === 'function') return content.GetText();
      if (typeof content.ToJSON === 'function') {
        try {
          const json = JSON.stringify(JSON.parse(content.ToJSON()));
          return json;
        } catch {
          return content.ToJSON();
        }
      }
      return '';
    };
    const sections = documentApi.GetSections?.() || [];
    return {
      body_text: documentApi.GetText?.() || '',
      pages_status: Array.from(document.querySelectorAll('*'))
        .map((element) => element.textContent?.trim())
        .find((text) => /^Seite \d+ von \d+$/.test(text)) || null,
      sections: sections.map((section, index) => ({
        index,
        type: section.GetType?.() || null,
        width_mm: section.GetPageWidth?.() ?? section.GetW?.() ?? section.get_W?.() ?? null,
        height_mm: section.GetPageHeight?.() ?? section.GetH?.() ?? section.get_H?.() ?? null,
        header_distance_mm: section.GetHeaderDistance?.() ?? null,
        footer_distance_mm: section.GetFooterDistance?.() ?? null,
        title_header: contentText(section.GetHeader?.('title', false)),
        default_header: contentText(section.GetHeader?.('default', false)),
        even_header: contentText(section.GetHeader?.('even', false)),
        title_footer: contentText(section.GetFooter?.('title', false)),
        default_footer: contentText(section.GetFooter?.('default', false)),
        even_footer: contentText(section.GetFooter?.('even', false)),
      })),
      modified: editorApi.isDocumentModified?.() === true,
      can_save: editorApi.asc_isDocumentCanSave?.() === true,
    };
  });

  const assertContains = (value, marker, label) => {
    if (!String(value || '').includes(marker)) {
      throw new Error(`${label} missing ${marker}: ${String(value || '').slice(0, 400)}`);
    }
  };
  const assertInitialState = (name, state) => {
    if (!String(state.pages_status || '').includes('von 2')) {
      throw new Error(`${name} page count mismatch: ${state.pages_status}`);
    }
    if (state.sections.length < 2) throw new Error(`${name} expected at least two sections`);
    assertContains(state.body_text, 'SECTION1_BODY_CONTROL', `${name} body`);
    assertContains(state.body_text, 'PRESERVE_SECTION1_TEXT_38C1', `${name} body`);
    const headerText = state.sections.map((section) =>
      `${section.title_header}\n${section.default_header}\n${section.even_header}`).join('\n');
    const footerText = state.sections.map((section) =>
      `${section.title_footer}\n${section.default_footer}\n${section.even_footer}`).join('\n');
    assertContains(headerText, 'HEADER_SECTION1_FIRST', `${name} headers`);
    assertContains(headerText, 'HEADER_SECTION1_DEFAULT', `${name} headers`);
    assertContains(footerText, 'FOOTER_SECTION1_DEFAULT', `${name} footers`);
  };

  await closeNotice(oracle);
  await closeNotice(ctox);

  const initial = {
    oracle: await inspectSections(oracle),
    ctox: await inspectSections(ctox),
  };
  assertInitialState('oracle', initial.oracle);
  assertInitialState('ctox', initial.ctox);

  for (const frame of [oracle, ctox]) {
    await frame.evaluate(() => {
      const documentApi = (window.editor || window.Asc?.editor)?.getJsApi?.()?.GetDocument?.();
      documentApi?.GoToPage?.(1);
    });
  }
  await page.waitForTimeout(800);
  await closeNotice(oracle);
  await closeNotice(ctox);
  await page.screenshot({ path: `${output}/differential-page-2-footer-check.png` });

  const applySectionEdit = async (name, frame) => {
    const state = await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      const section = documentApi?.GetSections?.()?.[0];
      if (!section?.SetPageSize || !section?.SetHeaderDistance || !section?.SetFooterDistance) {
        throw new Error('Document section editing API is unavailable');
      }
      section.SetPageSize(15840, 12240, false);
      section.SetPageMargins(1152, 1296, 1152, 1296);
      section.SetHeaderDistance(850);
      section.SetFooterDistance(648);
      section.SetTitlePage(true);
      section.SetType('nextPage');
      return {
        type: section.GetType?.() || null,
        width_twips: section.GetPageWidth?.() ?? null,
        height_twips: section.GetPageHeight?.() ?? null,
        header_distance_twips: section.GetHeaderDistance?.() ?? null,
        footer_distance_twips: section.GetFooterDistance?.() ?? null,
        modified: editorApi.isDocumentModified?.() === true,
        can_save: editorApi.asc_isDocumentCanSave?.() === true,
      };
    });
    if (state.type !== 'nextPage'
      || state.width_twips !== 15840
      || state.height_twips !== 12240
      || state.header_distance_twips !== 850
      || state.footer_distance_twips !== 648) {
      throw new Error(`${name} section edit mismatch: ${JSON.stringify(state)}`);
    }
    return state;
  };
  const section_edit = {
    oracle: await applySectionEdit('oracle', oracle),
    ctox: await applySectionEdit('ctox', ctox),
  };

  const unlinkHeaderFromPrevious = async (name, frame) => {
    const state = await frame.evaluate(async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      if (!editorApi?.GoToHeader || !editorApi?.HeadersAndFooters_LinkToPrevious) {
        throw new Error('Document header/footer link-to-previous API is unavailable');
      }
      editorApi.GoToHeader(1);
      await sleep(500);
      editorApi.HeadersAndFooters_LinkToPrevious(false);
      await sleep(500);
      const selected = editorApi.getSelectedElements?.() || [];
      const headerFocus = selected.map((item) => item.get_ObjectValue?.() || item.asc_getObjectValue?.())
        .find((value) => typeof value?.get_LinkToPrevious === 'function');
      const sections = documentApi?.GetSections?.() || [];
      const state = {
        modified: editorApi.isDocumentModified?.() === true,
        can_save: editorApi.asc_isDocumentCanSave?.() === true,
        current_page: editorApi.getCurrentPage?.() ?? null,
        link_to_previous: headerFocus?.get_LinkToPrevious?.() ?? null,
        section_2_default_header: sections[1]?.GetHeader?.('default', false)?.GetText?.() || '',
        section_2_default_footer: sections[1]?.GetFooter?.('default', false)?.GetText?.() || '',
      };
      editorApi.asc_CancelHdrFtrEditing?.();
      documentApi?.GoToPage?.(1);
      await sleep(500);
      return state;
    });
    if (state.link_to_previous !== false) {
      throw new Error(`${name} link-to-previous did not become false: ${JSON.stringify(state)}`);
    }
    assertContains(state.section_2_default_header, 'HEADER_SECTION1_DEFAULT', `${name} unlinked section 2 header`);
    return state;
  };
  const link_to_previous = {
    oracle: await unlinkHeaderFromPrevious('oracle', oracle),
    ctox: await unlinkHeaderFromPrevious('ctox', ctox),
  };

  const insertNextPageSectionBreak = async (name, frame, expectedInitialSections) => {
    const state = await frame.evaluate((expectedSections) => {
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      if (!documentApi?.Search || !documentApi?.GetCurrentParagraph) {
        throw new Error('Document section-break Builder API is unavailable');
      }
      const ranges = documentApi.Search('SECTION2_PAGE_SETUP_TARGET') || [];
      if (!ranges[0]?.Select) throw new Error('SECTION2_PAGE_SETUP_TARGET search result is unavailable');
      ranges[0].Select();
      const paragraph = documentApi.GetCurrentParagraph();
      if (!paragraph) throw new Error('current paragraph unavailable after section-break target selection');
      if (typeof editorApi.add_SectionBreak === 'function'
        && window.Asc?.c_oAscSectionBreakType?.NextPage !== undefined) {
        editorApi.add_SectionBreak(window.Asc.c_oAscSectionBreakType.NextPage);
      } else if (typeof documentApi.CreateSection === 'function') {
        const insertedSection = documentApi.CreateSection(paragraph);
        insertedSection.SetType('nextPage');
      } else {
        throw new Error('Document section-break insertion API is unavailable');
      }
      const sections = documentApi.GetSections?.() || [];
      const inserted = sections[expectedSections] || sections[sections.length - 1];
      return {
        before_sections: expectedSections,
        sections_count: sections.length,
        inserted_type: inserted?.GetType?.() || null,
        section_types: sections.map((section) => section.GetType?.() || null),
        modified: editorApi.isDocumentModified?.() === true,
        can_save: editorApi.asc_isDocumentCanSave?.() === true,
        text: documentApi.GetText?.() || '',
      };
    }, expectedInitialSections);
    if (state.sections_count <= expectedInitialSections || state.inserted_type !== 'nextPage') {
      throw new Error(`${name} section break insert mismatch: ${JSON.stringify(state)}`);
    }
    assertContains(state.text, 'SECTION2_PAGE_SETUP_TARGET', `${name} section break target`);
    return state;
  };
  const section_break = {
    oracle: await insertNextPageSectionBreak('oracle', oracle, initial.oracle.sections.length),
    ctox: await insertNextPageSectionBreak('ctox', ctox, initial.ctox.sections.length),
  };

  const insertSaveMarker = async (name, frame) => {
    const state = await frame.evaluate(() => {
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      if (!documentApi?.MoveCursorToEnd || !documentApi?.EnterText) {
        throw new Error('Document cursor/text API is unavailable');
      }
      documentApi.MoveCursorToEnd();
      documentApi.EnterText('\nSECTION_HDRFTR_SAVE_MARKER');
      return {
        modified: editorApi.isDocumentModified?.() === true,
        can_save: editorApi.asc_isDocumentCanSave?.() === true,
        text: documentApi.GetText?.() || '',
      };
    });
    if (!state.modified || !state.can_save) {
      throw new Error(`${name} did not become dirty/save-enabled after marker insertion`);
    }
    assertContains(state.text, 'SECTION_HDRFTR_SAVE_MARKER', `${name} save marker`);
    return state;
  };
  const mutation = {
    oracle: await insertSaveMarker('oracle', oracle),
    ctox: await insertSaveMarker('ctox', ctox),
  };

  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1,
  null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.sections-headers-footers',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });

  let oracleState = null;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () =>
      (await fetch('http://127.0.0.1:4180/state/document.sections-headers-footers')).json());
    if (oracleState.saved === true) break;
    await page.waitForTimeout(1000);
  }
  if (oracleState?.saved !== true) {
    throw new Error(`Oracle terminal save callback missing: state=${JSON.stringify(oracleState)}`);
  }

  const terminal = {
    oracle: await inspectSections(oracle),
    ctox: await inspectSections(ctox),
  };

  return {
    feature_id: feature,
    interaction: 'real-documenteditor-header-footer-render-save-capture',
    initial,
    terminal,
    section_edit,
    link_to_previous,
    section_break,
    mutation,
    captured,
    oracle_state: oracleState,
  };
}
