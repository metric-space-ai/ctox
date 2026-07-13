async (page) => {
  const feature = 'document.drawings-charts';
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
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-drawings-charts.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('document drawings comparison frames are missing');

  const closeNotices = async (frame) => {
    for (const label of ['OK', 'Schließen']) {
      const button = frame.getByRole('button', { name: label, exact: true });
      if (await button.count()) await button.first().click();
    }
  };
  const selectionState = (frame) => frame.evaluate(() => {
    const api = window.editor || window.Asc?.editor;
    return api.getSelectedElements().map((element) => {
      const value = element.get_ObjectValue?.();
      const shape = value?.asc_getShapeProperties?.();
      const chart = value?.asc_getChartProperties?.();
      return {
        object_type: element.get_ObjectType?.(),
        width_mm: value?.asc_getWidth?.(),
        height_mm: value?.asc_getHeight?.(),
        rotation_radians: value?.asc_getRot?.(),
        shape_type: shape?.asc_getType?.(),
        chart_type: chart?.getType?.(),
        chart_style: chart?.getStyle?.(),
      };
    });
  });
  const selectObject = async (name, frame, predicate, candidates) => {
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    if (!box) throw new Error(`${name} editor overlay is missing`);
    for (const candidate of candidates) {
      await overlay.click({ position: { x: box.width * candidate.x, y: box.height * candidate.y } });
      await page.waitForTimeout(90);
      const state = await selectionState(frame);
      if (state.some(predicate)) return state;
    }
    throw new Error(`${name} target object was not selected`);
  };
  const shapeCandidates = [
    { x: 0.43, y: 0.29 }, { x: 0.20, y: 0.29 }, { x: 0.55, y: 0.29 },
    { x: 0.43, y: 0.25 }, { x: 0.43, y: 0.33 },
  ];
  const chartCandidates = [];
  for (const y of [0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70, 0.75]) {
    for (const x of [0.25, 0.40, 0.55, 0.70]) chartCandidates.push({ x, y });
  }
  const isShape = (value) => value.shape_type === 'rect'
    && Math.abs(value.width_mm - 63.5) < 0.1
    && Math.abs(value.height_mm - 19.05) < 0.1;
  const isChart = (value) => value.chart_type !== undefined;
  const terminal = {};

  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    await closeNotices(frame);
    await selectObject(name, frame, isShape, shapeCandidates);
    const shapeApply = await frame.evaluate(() => {
      const api = window.editor || window.Asc?.editor;
      const image = new window.Asc.asc_CImgProperty();
      const shape = new window.Asc.asc_CShapeProperty();
      const fill = new window.Asc.asc_CShapeFill();
      fill.put_type(window.Asc.c_oAscFill.FILL_TYPE_SOLID);
      fill.put_fill(new window.Asc.asc_CFillSolid());
      fill.get_fill().put_color(window.Common.Utils.ThemeColor.getRgbColor('F4B183'));
      shape.put_fill(fill);
      image.put_ShapeProperties(shape);
      image.asc_putRot(Math.PI / 2);
      api.ImgApply(image);
      return { modified: api.isDocumentModified(), can_save: api.asc_isDocumentCanSave() };
    });
    if (!shapeApply.modified || !shapeApply.can_save) throw new Error(`${name} shape did not dirty/save-enable`);
    await page.waitForTimeout(350);

    await selectObject(name, frame, isChart, chartCandidates);
    const chartApply = await frame.evaluate(() => {
      const api = window.editor || window.Asc?.editor;
      const chart = api.getSelectedElements()
        .map((element) => element.get_ObjectValue?.()?.asc_getChartProperties?.())
        .find(Boolean);
      if (!chart) throw new Error('selected chart properties are missing');
      chart.putStyle(102);
      const image = new window.Asc.asc_CImgProperty();
      image.put_Width(140);
      image.put_Height(70);
      image.put_ResetCrop(false);
      image.put_ChartProperties(chart);
      api.ImgApply(image);
      return { modified: api.isDocumentModified(), can_save: api.asc_isDocumentCanSave() };
    });
    if (!chartApply.modified || !chartApply.can_save) throw new Error(`${name} chart did not dirty/save-enable`);
    await page.waitForTimeout(500);
    terminal[name] = { shape_apply: shapeApply, chart_apply: chartApply, selection: await selectionState(frame) };
  }

  await page.screenshot({ path: `${output}/differential-terminal-drawings-charts.png` });
  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1,
  null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.drawings-charts',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });
  let oracleState = null;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () =>
      (await fetch('http://127.0.0.1:4180/state/document.drawings-charts')).json());
    if (oracleState.saved === true) break;
    await page.waitForTimeout(1000);
  }
  if (oracleState?.saved !== true) {
    throw new Error(`Oracle terminal save callback missing: state=${JSON.stringify(oracleState)}`);
  }
  return {
    feature_id: feature,
    interaction: 'real-documenteditor-object-selection-shape-fill-rotation-chart-size-style-save',
    terminal,
    captured,
    oracle_state: oracleState,
  };
}
