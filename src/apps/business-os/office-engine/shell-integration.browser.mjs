import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { crc32 } from 'node:zlib';
import { createHash } from 'node:crypto';
import { chromium } from '../node_modules/playwright/index.mjs';

// Serve the repository root on port 8766 before running this harness. It uses
// isolated mock collections/commands, real editor frames and the native CLI;
// this is not tenant WebRTC/permission or deployment verification.

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const engineBin = process.argv.find(value => value.startsWith('--engine-bin='))?.slice('--engine-bin='.length)
  || path.join(root, 'runtime/build/cargo-target/debug/ctox-office-engine');
const output = process.argv.find(value => value.startsWith('--output-dir='))?.slice('--output-dir='.length)
  || path.join(root, 'output/playwright/office-integration');
await mkdir(output, { recursive: true });
const temporary = await mkdtemp(path.join(tmpdir(), 'ctox-office-native-'));
const run = promisify(execFile);
const realtime = process.argv.includes('--realtime');
const duringSave = process.argv.includes('--during-save');
const closeSafety = process.argv.includes('--close-safety');
const emptyLibrary = process.argv.includes('--empty-library');
const formulas = process.argv.includes('--formulas');
const importRoundtrip = process.argv.includes('--import-roundtrip');
const formatting = process.argv.includes('--formatting');
const worksheets = process.argv.includes('--worksheets');
const uiExport = process.argv.includes('--ui-export');
const selectedKind = process.argv.find(value=>value.startsWith('--kind='))?.slice('--kind='.length);
assert.ok(!selectedKind || ['document','spreadsheet'].includes(selectedKind), 'Unknown test kind');

// Isolated equivalent of the server's empty CSV -> OOXML conversion. The real
// native DOCY/XLSY converter still prepares every newly created editor payload.
function emptyWorkbook() {
  const parts={
    '[Content_Types].xml':'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>',
    '_rels/.rels':'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    'xl/workbook.xml':'<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Tabelle1" sheetId="1" r:id="rId1"/></sheets></workbook>',
    'xl/_rels/workbook.xml.rels':'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>',
    'xl/worksheets/sheet1.xml':'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1"/><sheetData/></worksheet>',
    'xl/styles.xml':'<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>',
  };
  parts['[Content_Types].xml']=parts['[Content_Types].xml'].replace('</Types>','<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>');
  parts['xl/_rels/workbook.xml.rels']=parts['xl/_rels/workbook.xml.rels'].replace('</Relationships>','<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>');
  const local=[],central=[]; let offset=0;
  for(const [name,xml] of Object.entries(parts)) {
    const filename=Buffer.from(name),data=Buffer.from(xml),checksum=crc32(data);
    const header=Buffer.alloc(30);header.writeUInt32LE(0x04034b50);header.writeUInt16LE(20,4);header.writeUInt32LE(checksum,14);header.writeUInt32LE(data.length,18);header.writeUInt32LE(data.length,22);header.writeUInt16LE(filename.length,26);
    const directory=Buffer.alloc(46);directory.writeUInt32LE(0x02014b50);directory.writeUInt16LE(20,4);directory.writeUInt16LE(20,6);directory.writeUInt32LE(checksum,16);directory.writeUInt32LE(data.length,20);directory.writeUInt32LE(data.length,24);directory.writeUInt16LE(filename.length,28);directory.writeUInt32LE(offset,42);
    local.push(header,filename,data);central.push(directory,filename);offset+=header.length+filename.length+data.length;
  }
  const directory=Buffer.concat(central),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt16LE(Object.keys(parts).length,8);end.writeUInt16LE(Object.keys(parts).length,10);end.writeUInt32LE(directory.length,12);end.writeUInt32LE(offset,16);
  return Buffer.concat([...local,directory,end]);
}
const browser = await chromium.launch({ headless: true });
const failures = [];
let activePage;
let activeErrors;
try {
  for (const kind of selectedKind ? [selectedKind] : ['spreadsheet', 'document']) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    await context.exposeBinding('officeLabExport', async (_, requestedKind, editorData, sourceData) => {
      assert.equal(requestedKind,kind);
      const directory=await mkdtemp(path.join(temporary,'ui-export-'));
      const binary=path.join(directory,'editor.bin'),sourcePath=path.join(directory,'source.input'),exported=path.join(directory,'exported.zip');
      let source=Buffer.from(sourceData,'base64');
      if(kind==='spreadsheet' && source.subarray(0,2).toString()!=='PK') {
        assert.match(source.toString(),/^[,\r\n]*$/);source=emptyWorkbook();
      }
      await writeFile(binary,Buffer.from(editorData,'base64'));await writeFile(sourcePath,source);
      await run(engineBin,['export',kind,binary,sourcePath,exported]);
      const bytes=await readFile(exported);
      return {data:bytes.toString('base64'),sha256:createHash('sha256').update(bytes).digest('hex')};
    });
    await context.exposeBinding('officeLabCanonicalize', async (_, encoded) => {
      assert.equal(kind, 'spreadsheet');
      const source = Buffer.from(encoded, 'base64');
      assert.match(source.toString(), /^[,\r\n]*$/, 'The blank spreadsheet source must be CSV');
      const canonical = emptyWorkbook();
      return {
        data: canonical.toString('base64'),
        sha256: createHash('sha256').update(canonical).digest('hex'),
      };
    });
    await context.exposeBinding('officeLabPrepare', async (_, requestedKind, encoded) => {
      assert.equal(requestedKind, kind);
      let source = Buffer.from(encoded, 'base64');
      if(kind==='spreadsheet' && source.subarray(0,2).toString()!=='PK') {
        assert.match(source.toString(), /^[,\r\n]*$/, 'New spreadsheet must contain no sample data');
        source=emptyWorkbook();
      }
      const input = path.join(temporary, `${kind}.input`);
      const result = path.join(temporary, `${kind}.bin`);
      await writeFile(input, source);
      await run(engineBin, ['prepare-editor', kind, input, result]);
      return (await readFile(result)).toString('base64');
    });
    const page = await context.newPage();
    page.setDefaultTimeout(15000);
    activePage = page;
    await page.addInitScript(() => window.addEventListener('message', event => {
      let message=event.data;
      if(typeof message==='string') { try { message=JSON.parse(message); } catch { return; } }
      if(message?.event==='onDocumentReady') window.__officeLabReady=true;
    }));
    const errors = [];
    activeErrors = errors;
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', event => { if(event.type()==='error') errors.push(event.text()); });
    page.on('response', response => { if(response.status()>=400) console.log(JSON.stringify({kind,failedAsset:response.url(),status:response.status()})); });
    await page.goto(`http://127.0.0.1:8766/src/apps/business-os/office-engine/oracle/shell-v2-office.html?kind=${kind}${realtime?'&realtime=1':''}${duringSave?'&holdFirstCommit=1':''}${closeSafety?'&realWindow=1':''}${emptyLibrary?'&empty=1':''}`);
    const prefix = kind === 'document' ? 'documents' : 'spreadsheets';
    if(emptyLibrary) {
      await page.locator(kind==='document'?'[data-documents-new-markdown]':'[data-spreadsheets-new]').click();
      await page.waitForFunction(()=>window.officeLab.records.length===1);
    } else await page.locator(`.${prefix}-card-main`).first().click({timeout:20000});
    const outer = page.frameLocator(`iframe[data-ctox-office-kind="${kind}"]`);
    const editor = outer.frameLocator('iframe.ctox-office-fork-frame');
    await editor.locator('#viewport').waitFor({state:'visible',timeout:60000});
    await editor.getByRole('tab', {name:'Startseite',exact:true}).waitFor({state:'visible',timeout:30000});
    const runtimeFrame=page.frames().find(frame=>frame.parentFrame()===page.mainFrame());
    await runtimeFrame.waitForFunction(()=>window.__officeLabReady===true,null,{timeout:30000});
    if(kind==='spreadsheet') assert.equal(await page.locator('[data-spreadsheets-add-row],[data-spreadsheets-add-col]').count(),0,'The wrapper must not expose unsupported row/column actions beside the native ribbon');
    const geometry = await page.locator('[data-shell-columns="2"]').evaluate(root => {
      const library=root.querySelector('.ctox-office-library'), editor=root.querySelector('.documents-workbench,.spreadsheets-editor');
      return {root:root.getBoundingClientRect().toJSON(),library:library.getBoundingClientRect().toJSON(),editor:editor.getBoundingClientRect().toJSON(),display:getComputedStyle(root).display,columns:getComputedStyle(root).gridTemplateColumns};
    });
    console.log(JSON.stringify({kind,geometry,errors}));
    assert.ok(geometry.editor.x > geometry.library.x + geometry.library.width - 1, 'Editor must be beside the library');
    assert.ok(geometry.editor.width > 500, 'Editor gets the available work area');
    await page.screenshot({path:path.join(output,`${kind}-light.png`)});
    assert.equal(await page.locator('.ctox-office-library').evaluate(library=>getComputedStyle(library.querySelector('.ctox-pane-header')).backgroundColor===getComputedStyle(library).backgroundColor),true,'Library header must use the same light-theme surface');
    const search=page.locator('.ctox-office-library [data-pg-search]');
    if(!emptyLibrary) {
    await search.fill('Arbeitsdatei 2');
    assert.equal(await page.locator(`.${prefix}-card-main`).count(),10);
    assert.equal(await search.evaluate(node=>node===document.activeElement),true);
    await search.fill('');
    assert.equal(await page.locator(`.${prefix}-card-main`).count(),30);
    await page.locator('[data-pg-tray-toggle]').click();
    assert.equal(await page.locator('[data-pg-tray]').isVisible(),true);
    await page.locator('[data-pg-tray-toggle]').click();
    await page.locator('[data-pg-view-cycle]').click();
    assert.equal(await page.locator('.ctox-office-library').getAttribute('data-office-view'),'cards');
    await page.locator('[data-pg-view-cycle]').click();
    }
    await page.evaluate(()=>document.documentElement.dataset.theme='dark');
    await editor.locator('html[data-ctox-theme="dark"]').waitFor({state:'attached',timeout:5000});
    await page.locator('.ctox-office-library').evaluate(async library=>{await Promise.all(library.getAnimations({subtree:true}).map(animation=>animation.finished.catch(()=>{})));});
    await page.screenshot({path:path.join(output,`${kind}-dark.png`)});
    assert.equal(await editor.locator('#toolbar button svg').first().evaluate(svg=>getComputedStyle(svg).color===getComputedStyle(document.body).color),true,'Monochrome SVG icons must follow Shell text contrast');
    console.log(`${kind}: theme checked; checking custom palette`);
    await page.evaluate(()=>document.querySelector('#app').style.setProperty('--accent','#a855f7'));
    const innerFrame=page.frames().find(frame=>frame.parentFrame()===runtimeFrame);
    await innerFrame.waitForFunction(()=>getComputedStyle(document.documentElement).getPropertyValue('--ctox-shell-accent').trim()==='#a855f7');
    assert.equal(await editor.locator('body').evaluate((body, kind) => {
      const style=getComputedStyle(body),surface=style.getPropertyValue('--ctox-fork-surface-2').trim();
      return style.getPropertyValue('--background-toolbar').trim()===surface && style.getPropertyValue(`--toolbar-header-${kind}`).trim()===surface;
    },kind),true,'Native ribbon colors must inherit the Shell surface, not override it with a product theme');
    for(const width of [960,640,390]) {
      console.log(`${kind}: checking width ${width}`);
      await page.setViewportSize({width,height:800});
      const toggle=page.locator(kind==='document'?'[data-documents-library-toggle]':'[data-spreadsheets-library-toggle]');
      if(width<=768) {
        await toggle.waitFor({state:'visible'});
        await toggle.click();
        assert.equal(await search.isVisible(),true);
        await page.locator(kind==='document'?'[data-documents-drawer-backdrop]':'[data-spreadsheets-library-backdrop]').click({position:{x:width-20,y:500}});
      }
      const bounds=await page.locator(`.${prefix}-module`).boundingBox();
      assert.ok(bounds.width<=width,'Office root must fit the window');
      await page.screenshot({path:path.join(output,`${kind}-${width}.png`)});
    }
    await page.setViewportSize({width:1440,height:960});
    console.log(`${kind}: creating blank file`);
    if(!emptyLibrary) {
    const previousFrame=await page.locator(`iframe[data-ctox-office-kind="${kind}"]`).elementHandle();
    await page.locator(kind==='document'?'[data-documents-new-markdown]':'[data-spreadsheets-new]').click();
    await page.waitForFunction(()=>window.officeLab.records.length===31);
    await page.waitForFunction(()=>window.officeLab.commands.some(command=>command.type.endsWith('.prepare')));
    await previousFrame.waitForElementState('hidden');
    }
    await editor.locator('#viewport').waitFor({state:'visible',timeout:30000});
    const blankRuntime=page.frames().find(frame=>frame.parentFrame()===page.mainFrame());
    await blankRuntime.waitForFunction(()=>window.__officeLabReady===true,null,{timeout:30000});
    assert.equal(await page.locator('#lab-drawer').count(),0,'Blank creation must not require a prompt dialog');
    await editor.locator('#viewport').evaluate(viewport => {
      const sdkWindow = viewport.ownerDocument.defaultView;
      sdkWindow.__officeLabSaveActions = [];
      for (const event of ['asc_onStartAction', 'asc_onEndAction']) {
        sdkWindow.Asc.editor.asc_registerCallback(event, (type, id) => {
          sdkWindow.__officeLabSaveActions.push({event, type, id});
        });
      }
    });
    let replacement=`CTOX_OFFICE_${kind.toUpperCase()}_SAVED`;
    let originalSheetName, addedSheetName;
    if(kind==='spreadsheet') {
      await editor.locator('#ce-cell-name').fill('A1'); await editor.locator('#ce-cell-name').press('Enter');
      assert.equal(await editor.locator('#ce-cell-content').inputValue(),'');
      await editor.locator('#ce-cell-content').fill(replacement);await editor.locator('#ce-cell-content').press('Enter');
      if (duringSave) {
        assert.ok(realtime, '--during-save requires the realtime fixture');
        const activeEditorFrame = await page.locator('iframe[data-ctox-office-kind="spreadsheet"]').elementHandle();
        await page.keyboard.press('Meta+s');
        await page.waitForFunction(()=>window.officeLab.commands.some(command=>command.type.endsWith('.commit')));
        await editor.locator('#ce-cell-name').fill('B1'); await editor.locator('#ce-cell-name').press('Enter');
        await editor.locator('#ce-cell-content').pressSequentially('DURING_SAVE', {delay:50});
        await editor.locator('#ce-cell-content').press('Enter');
        assert.equal(await activeEditorFrame.evaluate(frame=>frame.isConnected), true,
          'A realtime save acknowledgement must not replace the spreadsheet while the user is editing');
      }
    } else {
      const canvas=editor.locator('#id_viewer_overlay');const box=await canvas.boundingBox();
      await canvas.click({position:{x:box.width*0.35,y:120}});await page.keyboard.type(replacement,{delay:realtime?80:0});
      if (duringSave) {
        assert.ok(realtime, '--during-save requires the realtime fixture');
        const activeEditorFrame = await page.locator('iframe[data-ctox-office-kind="document"]').elementHandle();
        await page.keyboard.press('Meta+s');
        await page.waitForFunction(()=>window.officeLab.commands.some(command=>command.type.endsWith('.commit')));
        const continuation = '_DURING_SAVE';
        await page.keyboard.type(continuation, { delay: 50 });
        replacement += continuation;
        assert.equal(await activeEditorFrame.evaluate(frame=>frame.isConnected), true,
          'A realtime save acknowledgement must not replace the editor while the user is typing');
      }
    }
    if(formulas && kind==='spreadsheet') {
      for(const [cell,value] of [['A2','2'],['B2','3'],['C2','=SUM(A2:B2)']]) {
        await editor.locator('#ce-cell-name').fill(cell);await editor.locator('#ce-cell-name').press('Enter');
        await editor.locator('#ce-cell-content').fill(value);await editor.locator('#ce-cell-content').press('Enter');
      }
    }
    if(formatting) {
      if(kind==='spreadsheet') {
        await editor.locator('#ce-cell-name').fill('A1');await editor.locator('#ce-cell-name').press('Enter');
      } else {
        await page.keyboard.press('Meta+a');
      }
      await editor.locator('#id-toolbar-btn-bold').click();
      await editor.locator('#id-toolbar-btn-undo').click();
      await editor.locator('#id-toolbar-btn-redo').click();
    }
    if(worksheets && kind==='spreadsheet') {
      originalSheetName=(await editor.locator('#statusbar li.list-item.active').innerText()).trim();
      await editor.locator('#status-btn-addtab').click();
      addedSheetName=(await editor.locator('#statusbar li.list-item.active').innerText()).trim();
      assert.notEqual(addedSheetName,originalSheetName,'Add sheet must select a distinct named worksheet');
      assert.equal(await editor.locator('#statusbar li.list-item').count(),2);
      await editor.locator('#ce-cell-name').fill('D4');await editor.locator('#ce-cell-name').press('Enter');
      await editor.locator('#ce-cell-content').fill('CTOX_SECOND_SHEET_SAVED');await editor.locator('#ce-cell-content').press('Enter');
      await editor.locator('#statusbar li.list-item').filter({hasText:originalSheetName}).click();
    }
    if(uiExport) {
      await page.locator(`[data-${prefix}-export]`).click();
      const downloadPromise=page.waitForEvent('download',{timeout:30000});
      await page.locator('#lab-drawer button[type="submit"]').click();
      if(duringSave) {
        assert.equal(await page.evaluate(()=>window.officeLab.completedCommits.length),0);
        await page.evaluate(()=>window.officeLab.releaseFirstCommit());
      }
      const download=await downloadPromise;
      const file=path.join(temporary,`${kind}-ui-export.${kind==='document'?'docx':'xlsx'}`);
      await download.saveAs(file);
      const xml=await run('unzip',['-p',file,kind==='document'?'word/document.xml':'xl/sharedStrings.xml']);
      assert.ok(xml.stdout.replace(/<[^>]+>/g,'').includes(replacement),'UI export must contain the latest edits without requiring a separate Save click');
      if(duringSave && kind==='spreadsheet') assert.ok(xml.stdout.includes('DURING_SAVE'),'UI export must not use the stale version while a save is pending');
    } else {
      await editor.getByRole('button',{name:'Speichern (⌘+S)',exact:true}).click();
      if(duringSave) {
        assert.equal(await page.evaluate(()=>window.officeLab.completedCommits.length),0,'Continued editing must occur before the first save is acknowledged');
        await page.evaluate(()=>window.officeLab.releaseFirstCommit());
      }
    }
    await page.waitForFunction(()=>window.officeLab.commands.some(command=>command.type.endsWith('.commit')),null,{timeout:15000});
    await page.waitForFunction(expected=>window.officeLab.completedCommits.length>=expected,duringSave?2:1,{timeout:20000});
    const settledCommitCount=await page.evaluate(()=>window.officeLab.commands.filter(command=>command.type.endsWith('.commit')).length);
    // SDK serialization has deferred callbacks. A successful acknowledgement
    // must not manufacture another dirty revision and an endless autosave loop.
    await page.waitForTimeout(3500);
    assert.equal(await page.evaluate(()=>window.officeLab.commands.filter(command=>command.type.endsWith('.commit')).length),settledCommitCount,'An idle saved editor must not repeatedly commit itself');
    console.log(JSON.stringify(await editor.locator('#viewport').evaluate(viewport => {
      const sdkWindow = viewport.ownerDocument.defaultView;
      return {saveActions: sdkWindow.__officeLabSaveActions, saveActionId: sdkWindow.Asc.c_oAscAsyncAction.Save};
    })));
    assert.equal(await editor.getByText('Dokument wird gespeichert...', { exact: true }).count(), 0,
      'The editor must stop showing an in-progress save after its native commit succeeds');
    const saved=await page.evaluate(()=>{
      const command=window.officeLab.commands.findLast(command=>command.type.endsWith('.commit'));
      return window.officeLab.chunks.filter(row=>row.blob_id===command.payload.editor_blob_id).sort((a,b)=>a.idx-b.idx).map(row=>row.data);
    });
    const savedFile=path.join(temporary,`${kind}-saved.bin`);
    await writeFile(savedFile,Buffer.concat(saved.map(value=>Buffer.from(value,'base64'))));
    const inspection=await run(engineBin,['inspect-editor',kind,savedFile]);
    assert.equal(JSON.parse(inspection.stdout).kind,kind);
    if(kind==='document') {
      const exported=path.join(temporary,'document-saved.docx');
      await run(engineBin,['export',kind,savedFile,path.join(temporary,'document.input'),exported]);
      const xml=await run('unzip',['-p',exported,'word/document.xml']);
      assert.ok(xml.stdout.replace(/<[^>]+>/g,'').includes(replacement),'Native DOCX export must preserve the typed text');
      if(formatting) assert.match(xml.stdout,/<w:b(?:\s[^>]*)?\/>/,'Bold after undo/redo must survive native DOCX export');
    } else {
      assert.ok(inspection.stdout.includes(replacement),'Native inspection must find the cell entered through the UI');
      if(duringSave) assert.ok(inspection.stdout.includes('DURING_SAVE'),'Native inspection must preserve the cell edited during the earlier save');
      if(worksheets) assert.ok(inspection.stdout.includes('CTOX_SECOND_SHEET_SAVED'),'Native save must retain content on the added worksheet');
      const exported=path.join(temporary,'spreadsheet-saved.xlsx');
      await run(engineBin,['export',kind,savedFile,path.join(temporary,'spreadsheet.input'),exported]);
      if(formatting) {
        const styles=await run('unzip',['-p',exported,'xl/styles.xml']);
        assert.match(styles.stdout,/<b(?:\s[^>]*)?\/>/,'Bold after undo/redo must survive native XLSX export');
      }
      if(formulas) {
        const sheet=await run('unzip',['-p',exported,'xl/worksheets/sheet1.xml']);
        const cell=sheet.stdout.match(/<c\b[^>]*\br="C2"[^>]*>[\s\S]*?<\/c>/)?.[0] || '';
        assert.match(cell,/<f\b[^>]*>SUM\(A2:B2\)<\/f>/,'Exported workbook must retain the entered formula');
        assert.match(cell,/<v>5<\/v>/,'SUM must calculate 2 + 3 = 5 in the actual editor before export');
      }
      const reopenedBinary=path.join(temporary,'spreadsheet-export-reopened.bin');
      await run(engineBin,['prepare-editor',kind,exported,reopenedBinary]);
      const reopenedInspection=await run(engineBin,['inspect-editor',kind,reopenedBinary]);
      assert.ok(reopenedInspection.stdout.includes(replacement),'Native XLSX export and re-import must preserve the typed cell');
      if(duringSave) assert.ok(reopenedInspection.stdout.includes('DURING_SAVE'),'Native XLSX export/re-import must preserve edits made during save');
      if(worksheets) assert.ok(reopenedInspection.stdout.includes('CTOX_SECOND_SHEET_SAVED'),'XLSX export/re-import must retain the added worksheet content');
    }
    await page.screenshot({path:path.join(output,`${kind}-blank-edited.png`)});
    const savedIdentity=await page.evaluate(()=>{
      const record=window.officeLab.records.find(row=>row.title.startsWith('Neu'));
      const version=window.officeLab.versions.find(row=>row.id===record.current_version_id);
      return {id:record.id,version:version.id,editorBlob:version.editor_blob_id};
    });
    await page.reload();
    await page.locator(`.${prefix}-card-main`).first().click();
    await editor.locator('#viewport').waitFor({state:'visible',timeout:30000});
    const reopenedRuntime=page.frames().find(frame=>frame.parentFrame()===page.mainFrame());
    await reopenedRuntime.waitForFunction(()=>window.__officeLabReady===true,null,{timeout:30000});
    assert.equal(await page.evaluate(({id,version,editorBlob})=>{
      const record=window.officeLab.records.find(row=>row.id===id);
      return record.current_version_id===version && window.officeLab.versions.find(row=>row.id===version).editor_blob_id===editorBlob;
    },savedIdentity),true,'Reopening must retain the saved version, not the original blank');
    if(kind==='spreadsheet') {
      await editor.locator('#ce-cell-name').fill('A1');await editor.locator('#ce-cell-name').press('Enter');
      assert.equal(await editor.locator('#ce-cell-content').inputValue(),replacement);
      if(formulas) {
        await editor.locator('#ce-cell-name').fill('C2');await editor.locator('#ce-cell-name').press('Enter');
        // OOXML stores English function names; the German editor displays its
        // localized name on reopen while retaining the same range/expression.
        assert.equal(await editor.locator('#ce-cell-content').inputValue(),'=SUMME(A2:B2)','Reopened formula must remain editable in the selected locale');
      }
      if(duringSave) {
        await editor.locator('#ce-cell-name').fill('B1');await editor.locator('#ce-cell-name').press('Enter');
        assert.equal(await editor.locator('#ce-cell-content').inputValue(),'DURING_SAVE');
      }
      if(worksheets) {
        assert.equal(await editor.locator('#statusbar li.list-item').count(),2);
        await editor.locator('#statusbar li.list-item').filter({hasText:addedSheetName}).click();
        await editor.locator('#ce-cell-name').fill('D4');await editor.locator('#ce-cell-name').press('Enter');
        assert.equal(await editor.locator('#ce-cell-content').inputValue(),'CTOX_SECOND_SHEET_SAVED');
        await editor.locator('#statusbar li.list-item').filter({hasText:originalSheetName}).click();
      }
      const chrome = await editor.locator('#ce-cell-content').evaluate(input => {
        const box = node => node ? {id:node.id,classes:node.className,rect:node.getBoundingClientRect().toJSON(),overflow:getComputedStyle(node).overflow,color:getComputedStyle(node).color,fontSize:getComputedStyle(node).fontSize,lineHeight:getComputedStyle(node).lineHeight} : null;
        const textBox = label => {
          const text=[...label.childNodes].find(node=>node.nodeType===Node.TEXT_NODE && node.textContent.trim());
          if(!text) return null;
          const range=document.createRange();range.selectNodeContents(text);
          return range.getBoundingClientRect().toJSON();
        };
        return {input:box(input),parent:box(input.parentElement),grandparent:box(input.parentElement.parentElement),toolbar:box(document.querySelector('#toolbar')),panels:[...document.querySelectorAll('#toolbar .panel,#toolbar .box-controls,.toolbar-fullview-panel')].map(box).filter(node=>node.rect.width && node.rect.height),statusbar:box(document.querySelector('#statusbar')),statusTabs:[...document.querySelectorAll('#statusbar li.list-item')].map(node=>({text:node.textContent,item:box(node),label:box(node.querySelector('span')),textRect:textBox(node.querySelector('span'))}))};
      });
      console.log(JSON.stringify({kind,chrome}));
      assert.ok(chrome.input.rect.top >= chrome.toolbar.rect.bottom - 1, 'Formula bar must not be covered by the ribbon');
      assert.ok(chrome.panels.every(panel=>panel.rect.bottom <= chrome.input.rect.top + 1), 'Visible ribbon panels must not overlap the formula input');
      assert.ok(chrome.statusTabs.length > 0, 'Workbook must expose a named sheet tab');
      // The label has an intentional offscreen ::after width-measuring copy.
      // Assert the real text glyphs, not that hidden measurement box.
      assert.ok(chrome.statusTabs.every(tab=>tab.textRect?.width > 0 && tab.textRect.top >= chrome.statusbar.rect.top && tab.textRect.bottom <= chrome.statusbar.rect.bottom), 'Sheet label text must fit inside the status bar, not below the viewport');
    }
    await page.screenshot({path:path.join(output,`${kind}-reopened.png`)});
    if(importRoundtrip) {
      assert.ok(!closeSafety, 'Import and close failure scenarios run independently');
      const importedName = `${kind}-saved`;
      const sourceFile = path.join(temporary, `${importedName}.${kind==='document'?'docx':'xlsx'}`);
      const recordCount = await page.evaluate(()=>window.officeLab.records.length);
      const previousFrame = await page.locator(`iframe[data-ctox-office-kind="${kind}"]`).elementHandle();
      await page.locator(`[data-${prefix}-import-open]`).click();
      const chooserPromise = page.waitForEvent('filechooser');
      await page.locator('#lab-drawer input[type="file"]').click();
      await (await chooserPromise).setFiles(sourceFile);
      await page.locator('#lab-drawer button[type="submit"]').click();
      await page.waitForFunction(count=>window.officeLab.records.length===count+1, recordCount);
      await previousFrame.waitForElementState('hidden');
      await editor.locator('#viewport').waitFor({state:'visible',timeout:30000});
      const importedRuntime=page.frames().find(frame=>frame.parentFrame()===page.mainFrame());
      await importedRuntime.waitForFunction(()=>window.__officeLabReady===true,null,{timeout:30000});
      assert.equal(await page.locator('#lab-drawer').count(),0,'A completed import must close its dialog');
      if(kind==='document') {
        // Document inspection reports a binary directory, not paragraph text.
        // Verify the actual editor selection through its normal Copy command.
        await context.grantPermissions(['clipboard-read','clipboard-write'],{origin:'http://127.0.0.1:8766'});
        await page.evaluate(()=>navigator.clipboard.writeText('OFFICE_COPY_NOT_COMPLETED'));
        const canvas=editor.locator('#id_viewer_overlay');const box=await canvas.boundingBox();
        await canvas.click({position:{x:box.width*0.35,y:120}});
        await page.keyboard.press('Meta+a');await page.keyboard.press('Meta+c');
        await page.waitForFunction(async()=>await navigator.clipboard.readText()!=='OFFICE_COPY_NOT_COMPLETED',null,{timeout:5000});
        const copied=await page.evaluate(()=>navigator.clipboard.readText());
        assert.equal(copied.trim(),replacement,'Imported Word content must remain complete in the real editor');
        await page.keyboard.press('ArrowRight');
      } else {
        const importedInspection = await run(engineBin, ['inspect-editor',kind,path.join(temporary,`${kind}.bin`)]);
        assert.ok(importedInspection.stdout.includes(replacement),'The imported editor binary must retain all text from the native export');
      }
      if(kind==='spreadsheet') {
        await editor.locator('#ce-cell-name').fill('A1');await editor.locator('#ce-cell-name').press('Enter');
        assert.equal(await editor.locator('#ce-cell-content').inputValue(),replacement);
      }
      await page.screenshot({path:path.join(output,`${kind}-imported.png`)});
      await page.reload();
      await page.locator(`.${prefix}-card-main`).filter({hasText:new RegExp(`${kind}[- ]saved`)}).click();
      await editor.locator('#viewport').waitFor({state:'visible',timeout:30000});
      const importReloadRuntime=page.frames().find(frame=>frame.parentFrame()===page.mainFrame());
      await importReloadRuntime.waitForFunction(()=>window.__officeLabReady===true,null,{timeout:30000});
      if(kind==='spreadsheet') {
        await editor.locator('#ce-cell-name').fill('A1');await editor.locator('#ce-cell-name').press('Enter');
        assert.equal(await editor.locator('#ce-cell-content').inputValue(),replacement);
      } else {
        await page.evaluate(()=>navigator.clipboard.writeText('OFFICE_REOPEN_COPY_NOT_COMPLETED'));
        const canvas=editor.locator('#id_viewer_overlay');const box=await canvas.boundingBox();
        await canvas.click({position:{x:box.width*0.35,y:120}});
        await page.keyboard.press('Meta+a');await page.keyboard.press('Meta+c');
        await page.waitForFunction(async()=>await navigator.clipboard.readText()!=='OFFICE_REOPEN_COPY_NOT_COMPLETED',null,{timeout:5000});
        const copied=await page.evaluate(()=>navigator.clipboard.readText());
        assert.equal(copied.trim(),replacement,'Reopened imported Word content must remain complete');
        await page.keyboard.press('ArrowRight');
      }
      await page.screenshot({path:path.join(output,`${kind}-import-reopened.png`)});
      console.log(`${kind}: native exported file imported through the real file chooser and reopened after reload`);
    }
    if(closeSafety) {
      assert.ok(!duringSave, 'Close safety uses its own save failure/retry scenario');
      const originalFrame = await page.locator(`iframe[data-ctox-office-kind="${kind}"]`).elementHandle();
      const closeMarker = 'CTOX_CLOSE_GUARD_PRESERVED';
      await page.evaluate(()=>window.officeLab.failCommits=true);
      if(kind==='spreadsheet') {
        await editor.locator('#ce-cell-name').fill('C1');await editor.locator('#ce-cell-name').press('Enter');
        await editor.locator('#ce-cell-content').fill(closeMarker);await editor.locator('#ce-cell-content').press('Enter');
      } else {
        const canvas=editor.locator('#id_viewer_overlay');const box=await canvas.boundingBox();
        await canvas.click({position:{x:box.width*0.35,y:120}});
        await page.keyboard.press('Meta+End');await page.keyboard.type(closeMarker);
      }
      await page.waitForTimeout(200);
      await page.locator('[data-window-control="close"]').click();
      await page.waitForFunction(()=>document.querySelector('#notices')?.textContent.includes('OFFICE_LAB_EXPECTED_SAVE_FAILURE'));
      assert.equal(await originalFrame.evaluate(frame=>frame.isConnected),true,'A failed final save must leave the original editor open');
      assert.equal(await page.evaluate(()=>window.officeLab.completedCommits.length),0,'A failed final save must not claim a committed version');
      await page.screenshot({path:path.join(output,`${kind}-close-refused.png`)});
      await page.evaluate(()=>window.officeLab.failCommits=false);
      await page.locator('[data-window-control="close"]').click();
      await page.locator('#office-window').waitFor({state:'detached',timeout:30000});
      assert.ok(await page.evaluate(()=>window.officeLab.completedCommits.length)>0,'Retry must acknowledge the final save before closing');
      const encoded=await page.evaluate(()=>{
        const command=window.officeLab.completedCommits.at(-1);
        return window.officeLab.chunks.filter(row=>row.blob_id===command.payload.editor_blob_id).sort((a,b)=>a.idx-b.idx).map(row=>row.data);
      });
      const finalFile=path.join(temporary,`${kind}-closed.bin`);
      await writeFile(finalFile,Buffer.concat(encoded.map(value=>Buffer.from(value,'base64'))));
      if(kind==='document') {
        const exported=path.join(temporary,'document-closed.docx');
        await run(engineBin,['export',kind,finalFile,path.join(temporary,'document.input'),exported]);
        const xml=await run('unzip',['-p',exported,'word/document.xml']);
        assert.ok(xml.stdout.replace(/<[^>]+>/g,'').includes(closeMarker),'Final DOCX must preserve text typed before the refused close');
      } else {
        const inspection=await run(engineBin,['inspect-editor',kind,finalFile]);
        assert.ok(inspection.stdout.includes(closeMarker),'Final workbook must preserve cells typed before the refused close');
      }
      console.log(`${kind}: actual Shell close refused after failed save; retry persisted final edits before teardown`);
    }
    console.log(JSON.stringify({kind,flows:'open, search, filter, view, theme, custom accent, responsive, create blank, keyboard edit, save, native export/inspection, reopen'}));
    assert.deepEqual(errors.filter(error=>!closeSafety || !error.includes('OFFICE_LAB_EXPECTED_SAVE_FAILURE')), []);
    await context.close();
  }
} catch(error) {
  failures.push(error.stack || String(error));
  console.log(JSON.stringify({errors:activeErrors,frames:activePage?.frames().map(frame=>frame.url())}));
  try {
    for(const frame of activePage?.frames() || []) console.log(JSON.stringify(await frame.evaluate(()=>({url:location.href,theme:window.uitheme?.id,bodyClass:document.body.className,accent:getComputedStyle(document.querySelector('[data-ctox-office-kind]') || document.documentElement).getPropertyValue('--accent'),shellAccent:getComputedStyle(document.documentElement).getPropertyValue('--ctox-shell-accent'),styles:document.documentElement.getAttribute('style')}))));
    if(activePage) { console.log((await activePage.locator('body').innerText()).slice(-6000)); await activePage.screenshot({path:path.join(output,'failure.png')}); }
  } catch(diagnosticError) {
    console.warn('Failure evidence capture was incomplete:', diagnosticError.message);
  }
}
finally { await browser.close(); }
if(failures.length) throw new Error(failures.join('\n'));
