/**
 * CTOX Business OS - HyperFormula ESM Port
 * Evaluator.js — Recalculation coordinator orchestrating AST walk outputs and dependency graph updates.
 *
 * ref: hyperformula/src/Evaluator.ts:1-33
 */

import { Interpreter } from './interpreter/Interpreter.js';
import { FormulaParser } from './parser/FormulaParser.js';
import { topologicalSort } from './DependencyGraph/TopSort.js';
import { cellAddressToString, parseCellAddress } from './parser/Address.js';

export class Evaluator {
  // ref: hyperformula/src/Evaluator.ts:24-32
  constructor(sheets, config, dependencyGraph) {
    this.sheets = sheets; // Map of sheetId -> Sheet object
    this.config = config;
    this.dependencyGraph = dependencyGraph;
    this.interpreter = new Interpreter(this);
    this.currentlyEvaluating = new Set();
  }

  // ref: hyperformula/src/Evaluator.ts:34-42
  run() {
    this.recalculate();
  }

  /**
   * Performs Kahn's topological sort and evaluates formula cells chronologically.
   * Cycles are detected and marked with "#CYCLE!".
   */
  recalculate() {
    // 1. Reset all computed node values to null so that we recalculate fresh
    for (const key of this.dependencyGraph.keys()) {
      const node = this.dependencyGraph.getNode(key);
      if (node && node.formula) {
        node.value = null;
      }
    }

    // 2. Perform Kahn's topological sort
    const { order, hasCycles, cycles } = topologicalSort(this.dependencyGraph);

    // 3. Mark cycle nodes with "#CYCLE!" and update the sheet cell value
    if (hasCycles) {
      for (const cycleKey of cycles) {
        const node = this.dependencyGraph.getNode(cycleKey);
        if (node) {
          node.value = "#CYCLE!";
          const addr = this.parseKey(cycleKey);
          if (addr) {
            const sheet = this.sheets.get(addr.sheet);
            if (sheet) {
              sheet.setCell(addr.col, addr.row, "#CYCLE!");
            }
          }
        }
      }
    }

    // 4. Chronologically evaluate each node in sorted order
    this.currentlyEvaluating = new Set();
    for (const key of order) {
      const node = this.dependencyGraph.getNode(key);
      if (!node || !node.formula) continue;

      this.currentlyEvaluating.add(key);
      try {
        const addr = this.parseKey(key);
        if (!addr) {
          node.value = "#ERROR!";
          continue;
        }

        const parser = new FormulaParser(addr.sheet);
        if (!node.AST) {
          node.AST = parser.parse(node.formula);
        }

        const result = this.interpreter.evaluate(node.AST, key);
        node.value = result;

        const sheet = this.sheets.get(addr.sheet);
        if (sheet) {
          sheet.setCell(addr.col, addr.row, result);
        }
      } catch (err) {
        node.value = "#ERROR!";
        const addr = this.parseKey(key);
        if (addr) {
          const sheet = this.sheets.get(addr.sheet);
          if (sheet) {
            sheet.setCell(addr.col, addr.row, "#ERROR!");
          }
        }
      } finally {
        this.currentlyEvaluating.delete(key);
      }
    }
  }

  // ref: hyperformula/src/Evaluator.ts:77-97
  getResolvedCellValue(address, currentKey) {
    const key = cellAddressToString(address);

    // Avoid infinite recursion on cycles
    if (this.currentlyEvaluating.has(key)) {
      return "#CYCLE!";
    }

    const node = this.dependencyGraph.getNode(key);
    if (node && node.formula) {
      if (node.value !== null && node.value !== undefined) {
        return node.value;
      }
      // If formula value is null, it means it is not yet calculated in TopSort order.
      // Recursively evaluate it if it's not a circular dependency.
      if (node.formula) {
        this.currentlyEvaluating.add(key);
        try {
          const parser = new FormulaParser(address.sheet);
          if (!node.AST) {
            node.AST = parser.parse(node.formula);
          }
          node.value = this.interpreter.evaluate(node.AST, key);

          const sheet = this.sheets.get(address.sheet);
          if (sheet) {
            sheet.setCell(address.col, address.row, node.value);
          }
          return node.value;
        } catch (err) {
          return "#ERROR!";
        } finally {
          this.currentlyEvaluating.delete(key);
        }
      }
    }

    // Default to direct grid lookup if no formula node exists
    const sheet = this.sheets.get(address.sheet);
    if (!sheet) return "";
    return sheet.getCell(address.col, address.row);
  }

  // ref: hyperformula/src/Evaluator.ts:145-154
  getResolvedRangeValues(startAddress, endAddress, currentKey) {
    const startCol = Math.min(startAddress.col, endAddress.col);
    const endCol = Math.max(startAddress.col, endAddress.col);
    const startRow = Math.min(startAddress.row, endAddress.row);
    const endRow = Math.max(startAddress.row, endAddress.row);

    const values = [];
    for (let r = startRow; r <= endRow; r++) {
      const rowVals = [];
      for (let c = startCol; c <= endCol; c++) {
        const addr = { sheet: startAddress.sheet, col: c, row: r };
        rowVals.push(this.getResolvedCellValue(addr, currentKey));
      }
      values.push(rowVals);
    }
    return values;
  }

  parseKey(key) {
    const firstSheetId = this.sheets.keys().next().value || 0;
    return parseCellAddress(key, firstSheetId);
  }
}
