/**
 * CTOX Business OS - HyperFormula ESM Port
 * HyperFormula.js — Entry facade exposing static factories and CRUD workbook methods.
 *
 * ref: hyperformula/src/HyperFormula.ts:1-143
 */

import { Config } from './Config.js';
import { Sheet } from './Sheet.js';
import { Graph as DependencyGraph } from './DependencyGraph/Graph.js';
import { Evaluator } from './Evaluator.js';
import { cellAddressToString, parseCellAddress } from './parser/Address.js';
import { FormulaParser } from './parser/FormulaParser.js';

export class HyperFormula {
  // ref: hyperformula/src/HyperFormula.ts:127-143
  constructor(config = {}) {
    this.config = new Config(config);
    this.sheets = new Map(); // id -> Sheet
    this.sheetNameMap = new Map(); // name -> Sheet
    this.dependencyGraph = new DependencyGraph();
    this.evaluator = new Evaluator(this.sheets, this.config, this.dependencyGraph);
    this.sheetIdCounter = 0;
  }

  // ref: hyperformula/src/HyperFormula.ts:322-324
  static buildFromSheets(sheetsData, configInput = {}) {
    const instance = new HyperFormula(configInput);
    for (const [name, grid] of Object.entries(sheetsData)) {
      instance.addSheetWithData(name, grid);
    }
    instance.evaluator.recalculate();
    return instance;
  }

  // ref: hyperformula/src/HyperFormula.ts:275-277
  static buildFromArray(arrayData, configInput = {}) {
    return HyperFormula.buildFromSheets({ "Sheet1": arrayData }, configInput);
  }

  // ref: hyperformula/src/HyperFormula.ts:349-351
  static buildEmpty(configInput = {}) {
    return new HyperFormula(configInput);
  }

  addSheetWithData(name, grid) {
    const id = this.sheetIdCounter++;
    const sheet = new Sheet(id, name, grid);
    this.sheets.set(id, sheet);
    this.sheetNameMap.set(name, sheet);

    // Scan initial values for formulas to register in dependency graph
    const parser = new FormulaParser(id);
    for (let r = 0; r < grid.length; r++) {
      const rowData = grid[r] || [];
      for (let c = 0; c < rowData.length; c++) {
        const val = rowData[c];
        if (typeof val === 'string' && val.startsWith('=')) {
          const key = cellAddressToString({ sheet: id, col: c, row: r });
          const node = this.dependencyGraph.addNode(key);
          node.formula = val;
          try {
            node.AST = parser.parse(val);
            const deps = [];
            this.extractDependencies(node.AST, deps);
            for (const depKey of deps) {
              this.dependencyGraph.addEdge(depKey, key);
            }
          } catch (err) {
            node.value = "#ERROR!";
          }
        }
      }
    }
    return id;
  }

  // ref: hyperformula/src/HyperFormula.ts:699-705
  getCellValue(cellAddress) {
    const stdAddr = this.standardizeAddress(cellAddress);
    return this.evaluator.getResolvedCellValue(stdAddr);
  }

  // ref: hyperformula/src/HyperFormula.ts:730-735
  getCellFormula(cellAddress) {
    const stdAddr = this.standardizeAddress(cellAddress);
    const key = cellAddressToString(stdAddr);
    const node = this.dependencyGraph.getNode(key);
    return node ? node.formula : undefined;
  }

  // ref: hyperformula/src/HyperFormula.ts:792-797
  getCellSerialized(cellAddress) {
    const stdAddr = this.standardizeAddress(cellAddress);
    const key = cellAddressToString(stdAddr);
    const node = this.dependencyGraph.getNode(key);
    if (node && node.formula) return node.formula;

    const sheet = this.sheets.get(stdAddr.sheet);
    if (!sheet) return "";
    return sheet.getCell(stdAddr.col, stdAddr.row);
  }

  // ref: hyperformula/src/HyperFormula.ts:1292-1295
  setCellContents(topLeftCornerAddress, cellContents) {
    const stdAddr = this.standardizeAddress(topLeftCornerAddress);
    const changes = [];

    if (Array.isArray(cellContents)) {
      for (let r = 0; r < cellContents.length; r++) {
        const row = cellContents[r];
        for (let c = 0; c < row.length; c++) {
          const val = row[c];
          const targetAddr = {
            sheet: stdAddr.sheet,
            col: stdAddr.col + c,
            row: stdAddr.row + r
          };
          const res = this.setSingleCellContent(targetAddr, val);
          changes.push(...res);
        }
      }
    } else {
      const res = this.setSingleCellContent(stdAddr, cellContents);
      changes.push(...res);
    }

    // Trigger recalculation once after setting all cells
    this.evaluator.recalculate();

    // Build the final ExportedChange list with calculated values
    const exportedChanges = [];
    for (const change of changes) {
      exportedChanges.push({
        address: change.address,
        newValue: this.getCellValue(change.address)
      });
    }
    return exportedChanges;
  }

  setSingleCellContent(stdAddr, value) {
    const key = cellAddressToString(stdAddr);
    this.dependencyGraph.clearOutgoingEdges(key);

    if (typeof value === 'string' && value.startsWith('=')) {
      const node = this.dependencyGraph.addNode(key);
      node.formula = value;
      node.value = null;
      node.AST = null;

      try {
        const parser = new FormulaParser(stdAddr.sheet);
        node.AST = parser.parse(value);
        const deps = [];
        this.extractDependencies(node.AST, deps);
        for (const depKey of deps) {
          this.dependencyGraph.addEdge(depKey, key);
        }
      } catch (err) {
        node.value = "#ERROR!";
      }
    } else {
      this.dependencyGraph.removeNode(key);
      const sheet = this.sheets.get(stdAddr.sheet);
      if (sheet) {
        sheet.setCell(stdAddr.col, stdAddr.row, value);
      }
    }
    return [{ address: stdAddr }];
  }

  getSheetValues(sheetId) {
    const sheet = this.sheets.get(sheetId);
    if (!sheet) return [];

    const dims = sheet.getDimensions();
    const grid = [];
    for (let r = 0; r < dims.height; r++) {
      const row = [];
      for (let c = 0; c < dims.width; c++) {
        row.push(this.getCellValue({ sheet: sheetId, col: c, row: r }));
      }
      grid.push(row);
    }
    return grid;
  }

  getSheetDimensions(sheetId) {
    const sheet = this.sheets.get(sheetId);
    if (!sheet) return { width: 0, height: 0 };
    return sheet.getDimensions();
  }

  addSheet(sheetName) {
    const id = this.sheetIdCounter++;
    const sheet = new Sheet(id, sheetName, []);
    this.sheets.set(id, sheet);
    this.sheetNameMap.set(sheetName, sheet);
    return id;
  }

  removeSheet(sheetId) {
    const sheet = this.sheets.get(sheetId);
    if (sheet) {
      this.sheets.delete(sheetId);
      this.sheetNameMap.delete(sheet.name);

      // Clean up dependency graph nodes belonging to this sheet
      for (const key of this.dependencyGraph.keys()) {
        const addr = parseCellAddress(key, sheetId);
        if (addr && addr.sheet === sheetId) {
          this.dependencyGraph.removeNode(key);
        }
      }
      this.evaluator.recalculate();
    }
  }

  // Helper: Standardize address sheet index to its numeric ID
  standardizeAddress(addr) {
    let sheetId = addr.sheet;
    if (typeof sheetId === 'string') {
      const sheet = this.sheetNameMap.get(sheetId);
      if (sheet) {
        sheetId = sheet.id;
      }
    }
    return {
      sheet: sheetId !== undefined ? sheetId : 0,
      col: addr.col,
      row: addr.row,
      absCol: addr.absCol,
      absRow: addr.absRow
    };
  }

  // Recursive AST dependency collector
  extractDependencies(node, deps = []) {
    if (!node) return deps;
    if (node.type === 'CellReference') {
      const stdAddr = this.standardizeAddress(node.address);
      deps.push(cellAddressToString(stdAddr));
    } else if (node.type === 'RangeReference') {
      const startStd = this.standardizeAddress(node.start);
      const endStd = this.standardizeAddress(node.end);
      const startCol = Math.min(startStd.col, endStd.col);
      const endCol = Math.max(startStd.col, endStd.col);
      const startRow = Math.min(startStd.row, endStd.row);
      const endRow = Math.max(startStd.row, endStd.row);
      for (let r = startRow; r <= endRow; r++) {
        for (let c = startCol; c <= endCol; c++) {
          deps.push(cellAddressToString({ sheet: startStd.sheet, col: c, row: r }));
        }
      }
    } else if (node.type === 'UnaryOp') {
      this.extractDependencies(node.expr, deps);
    } else if (node.type === 'BinaryOp') {
      this.extractDependencies(node.left, deps);
      this.extractDependencies(node.right, deps);
    } else if (node.type === 'FunctionCall') {
      for (const arg of node.args) {
        this.extractDependencies(arg, deps);
      }
    }
    return deps;
  }
}
