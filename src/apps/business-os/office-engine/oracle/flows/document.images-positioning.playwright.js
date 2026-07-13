async (page) => {
  const feature = 'document.images-positioning';
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
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-images-positioning.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('document images comparison frames are missing');

  const editors = [['oracle', oracle], ['ctox', ctox]];
  const target = {
    inline: {
      description: 'CTOX_INLINE_IMAGE_TARGET',
      clickRatio: { x: 0.50, y: 0.42 },
      widthMm: 69.9,
      heightMm: 34.95,
    },
    floating: {
      description: 'CTOX_FLOATING_IMAGE_TARGET',
      clickRatio: { x: 0.25, y: 0.58 },
      horizontalMm: 76.2,
      verticalMm: 8.89,
    },
  };

  const closeNotice = async (frame) => {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
  };
  const selectedImageState = async (frame) => frame.evaluate(() => {
    const editorApi = window.editor || window.Asc?.editor;
    const values = editorApi?.getSelectedElements?.()
      ?.map((element) => element.get_ObjectValue?.())
      ?.filter((value) => value?.asc_getDescription?.()) || [];
    return values.map((value) => ({
      description: value.asc_getDescription(),
      width_mm: value.asc_getWidth?.(),
      height_mm: value.asc_getHeight?.(),
      wrap: value.asc_getWrappingStyle?.(),
      position_h: value.asc_getPositionH?.() && {
        relative_from: value.asc_getPositionH().get_RelativeFrom?.(),
        value_mm: value.asc_getPositionH().get_Value?.(),
      },
      position_v: value.asc_getPositionV?.() && {
        relative_from: value.asc_getPositionV().get_RelativeFrom?.(),
        value_mm: value.asc_getPositionV().get_Value?.(),
      },
    }));
  });
  const selectImage = async (name, frame, descriptor, ratio = descriptor.clickRatio) => {
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    if (!box) throw new Error(`${name} editor overlay is missing`);
    const candidateRatios = [ratio, ...(descriptor.candidateRatios || [])];
    let selected = [];
    for (const candidate of candidateRatios) {
      await overlay.click({ position: { x: box.width * candidate.x, y: box.height * candidate.y } });
      await page.waitForTimeout(180);
      selected = await selectedImageState(frame);
      if (selected.some((value) => value.description === descriptor.description)) return selected;
    }
    for (const y of [0.50, 0.54, 0.58, 0.62, 0.66, 0.70]) {
      for (const x of [0.15, 0.20, 0.25, 0.30, 0.35, 0.40]) {
        await overlay.click({ position: { x: box.width * x, y: box.height * y } });
        await page.waitForTimeout(90);
        selected = await selectedImageState(frame);
        if (selected.some((value) => value.description === descriptor.description)) return selected;
      }
    }
    throw new Error(`${name} did not select ${descriptor.description}: ${JSON.stringify(selected)}`);
  };
  const applyInlineSize = async (frame) => frame.evaluate(({ description, widthMm, heightMm }) => {
    const editorApi = window.editor || window.Asc?.editor;
    const image = editorApi.getSelectedElements()
      .map((element) => element.get_ObjectValue?.())
      .find((value) => value?.asc_getDescription?.() === description);
    if (!image) throw new Error(`selected inline image is missing: ${description}`);
    image.asc_putWidth(widthMm);
    image.asc_putHeight(heightMm);
    editorApi.ImgApply(image);
    return {
      modified: editorApi.isDocumentModified?.() === true,
      can_save: editorApi.asc_isDocumentCanSave?.() === true,
    };
  }, target.inline);
  const applyFloatingPosition = async (frame) => frame.evaluate(({ description, horizontalMm, verticalMm }) => {
    const editorApi = window.editor || window.Asc?.editor;
    const image = editorApi.getSelectedElements()
      .map((element) => element.get_ObjectValue?.())
      .find((value) => value?.asc_getDescription?.() === description);
    if (!image) throw new Error(`selected floating image is missing: ${description}`);
    const positionH = image.asc_getPositionH();
    const positionV = image.asc_getPositionV();
    positionH.put_RelativeFrom(window.Asc.c_oAscRelativeFromH.Column);
    positionH.put_UseAlign(false);
    positionH.put_Value(horizontalMm);
    positionV.put_RelativeFrom(window.Asc.c_oAscRelativeFromV.Paragraph);
    positionV.put_UseAlign(false);
    positionV.put_Value(verticalMm);
    image.asc_putPositionH(positionH);
    image.asc_putPositionV(positionV);
    image.asc_putWrappingStyle(window.Asc.c_oAscWrapStyle2.Square);
    editorApi.ImgApply(image);
    return {
      modified: editorApi.isDocumentModified?.() === true,
      can_save: editorApi.asc_isDocumentCanSave?.() === true,
      constants: {
        square_wrap: window.Asc.c_oAscWrapStyle2.Square,
        horizontal_relative_from: window.Asc.c_oAscRelativeFromH.Column,
        vertical_relative_from: window.Asc.c_oAscRelativeFromV.Paragraph,
      },
    };
  }, target.floating);
  const assertApprox = (actual, expected, label, tolerance = 0.08) => {
    if (Math.abs(Number(actual) - expected) > tolerance) {
      throw new Error(`${label}: expected ${expected}, got ${actual}`);
    }
  };

  const terminal = {};
  for (const [name, frame] of editors) {
    await closeNotice(frame);
    await selectImage(name, frame, target.inline);
    const inlineApply = await applyInlineSize(frame);
    if (!inlineApply.modified || !inlineApply.can_save) throw new Error(`${name} inline image did not dirty/save-enable`);
    await page.waitForTimeout(500);
    const inlineState = (await selectedImageState(frame)).find((value) => value.description === target.inline.description);
    assertApprox(inlineState?.width_mm, target.inline.widthMm, `${name} inline width`);
    assertApprox(inlineState?.height_mm, target.inline.heightMm, `${name} inline height`);

    await selectImage(name, frame, target.floating);
    const floatingApply = await applyFloatingPosition(frame);
    if (!floatingApply.modified || !floatingApply.can_save) throw new Error(`${name} floating image did not dirty/save-enable`);
    await page.waitForTimeout(500);
    const floatingState = (await selectedImageState(frame)).find((value) => value.description === target.floating.description);
    assertApprox(floatingState?.position_h?.value_mm, target.floating.horizontalMm, `${name} floating horizontal position`);
    assertApprox(floatingState?.position_v?.value_mm, target.floating.verticalMm, `${name} floating vertical position`);
    if (floatingState?.wrap !== floatingApply.constants.square_wrap) {
      throw new Error(`${name} floating wrap mismatch: ${floatingState?.wrap}`);
    }
    if (floatingState?.position_h?.relative_from !== floatingApply.constants.horizontal_relative_from) {
      throw new Error(`${name} floating horizontal relative-from mismatch`);
    }
    if (floatingState?.position_v?.relative_from !== floatingApply.constants.vertical_relative_from) {
      throw new Error(`${name} floating vertical relative-from mismatch`);
    }

    terminal[name] = { inline: inlineState, floating: floatingState };
  }

  await page.screenshot({ path: `${output}/differential-terminal-images-positioning.png` });
  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1,
  null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.images-positioning',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });
  let oracleState = null;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () =>
      (await fetch('http://127.0.0.1:4180/state/document.images-positioning')).json());
    if (oracleState.saved === true) break;
    await page.waitForTimeout(1000);
  }
  if (oracleState?.saved !== true) {
    throw new Error(`Oracle terminal save callback missing: state=${JSON.stringify(oracleState)}`);
  }

  return {
    feature_id: feature,
    interaction: 'real-documenteditor-canvas-selection-and-original-image-property-pipeline-save',
    terminal,
    captured,
    oracle_state: oracleState,
  };
}
