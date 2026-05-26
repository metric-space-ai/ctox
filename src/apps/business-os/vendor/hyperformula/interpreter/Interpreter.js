/**
 * CTOX Business OS - HyperFormula ESM Port
 * interpreter/Interpreter.js — AST-walking evaluation engine for cell expressions.
 *
 * ref: hyperformula/src/interpreter/Interpreter.ts:1-250
 */

import { functionRegistry } from './FunctionRegistry.js';

export class Interpreter {
  // ref: hyperformula/src/interpreter/Interpreter.ts:255-275
  constructor(evaluator) {
    this.evaluator = evaluator;
  }

  // ref: hyperformula/src/interpreter/Interpreter.ts:280-305
  evaluate(node, currentKey) {
    if (!node) return "";

    switch (node.type) {
      case 'Literal':
        return node.value;

      case 'CellReference':
        return this.evaluator.getResolvedCellValue(node.address, currentKey);

      case 'RangeReference':
        return this.evaluator.getResolvedRangeValues(node.start, node.end, currentKey);

      case 'UnaryOp': {
        const val = this.evaluate(node.expr, currentKey);
        const num = parseFloat(val);
        if (isNaN(num)) return "#VALUE!";
        return node.op === '-' ? -num : num;
      }

      case 'BinaryOp':
        return this.evaluateBinaryOp(node, currentKey);

      case 'FunctionCall':
        return this.evaluateFunctionCall(node, currentKey);

      default:
        return "#ERROR!";
    }
  }

  // ref: hyperformula/src/interpreter/Interpreter.ts:310-380
  evaluateBinaryOp(node, currentKey) {
    const left = this.evaluate(node.left, currentKey);
    const right = this.evaluate(node.right, currentKey);

    // Concatenation
    if (node.op === '&') {
      return String(left) + String(right);
    }

    const lNum = parseFloat(left);
    const rNum = parseFloat(right);

    // Comparators
    if (['=', '<', '>', '<=', '>=', '<>'].includes(node.op)) {
      const leftVal = isNaN(lNum) ? left : lNum;
      const rightVal = isNaN(rNum) ? right : rNum;

      switch (node.op) {
        case '=': return leftVal === rightVal;
        case '<': return leftVal < rightVal;
        case '>': return leftVal > rightVal;
        case '<=': return leftVal <= rightVal;
        case '>=': return leftVal >= rightVal;
        case '<>': return leftVal !== rightVal;
      }
    }

    if (isNaN(lNum) || isNaN(rNum)) {
      return "#VALUE!";
    }

    switch (node.op) {
      case '+': return lNum + rNum;
      case '-': return lNum - rNum;
      case '*': return lNum * rNum;
      case '/':
        if (rNum === 0) return "#DIV/0!";
        return lNum / rNum;
      case '^': return Math.pow(lNum, rNum);
      default: return "#ERROR!";
    }
  }

  // ref: hyperformula/src/interpreter/Interpreter.ts:385-450
  evaluateFunctionCall(node, currentKey) {
    const handler = functionRegistry.get(node.name);
    if (!handler) {
      // Inline common spreadsheet function evaluations if not registered globally
      const evaluatedArgs = node.args.map(arg => this.evaluate(arg, currentKey));
      return this.evaluateBuiltin(node.name, evaluatedArgs);
    }
    return handler(node.args, this, currentKey);
  }

  // ref: hyperformula/src/interpreter/Interpreter.ts:455-580
  evaluateBuiltin(name, args) {
    const flatArgs = [];

    // Helper to flatten ranges/arrays passed to aggregators (like SUM)
    const flatten = (arr) => {
      if (Array.isArray(arr)) {
        for (const item of arr) {
          flatten(item);
        }
      } else if (arr !== "" && arr !== null && arr !== undefined) {
        flatArgs.push(arr);
      }
    };

    flatten(args);

    const numbers = flatArgs.map(x => parseFloat(x)).filter(x => !isNaN(x));

    switch (name) {
      // --- Math ---
      case 'SUM':
        return numbers.reduce((acc, curr) => acc + curr, 0);
      case 'AVERAGE':
        return numbers.length > 0 ? numbers.reduce((acc, curr) => acc + curr, 0) / numbers.length : "#DIV/0!";
      case 'COUNT':
        return numbers.length;
      case 'MIN':
        return numbers.length > 0 ? Math.min(...numbers) : 0;
      case 'MAX':
        return numbers.length > 0 ? Math.max(...numbers) : 0;
      case 'ABS':
        return numbers.length > 0 ? Math.abs(numbers[0]) : "#VALUE!";
      case 'ROUND': {
        const val = parseFloat(flatArgs[0]);
        const digits = flatArgs[1] !== undefined ? parseInt(flatArgs[1], 10) : 0;
        if (isNaN(val) || isNaN(digits)) return "#VALUE!";
        const factor = Math.pow(10, digits);
        return Math.round(val * factor) / factor;
      }
      case 'SQRT':
        return numbers.length > 0 && numbers[0] >= 0 ? Math.sqrt(numbers[0]) : "#VALUE!";
      case 'PRODUCT':
        return numbers.length > 0 ? numbers.reduce((acc, curr) => acc * curr, 1) : 0;

      // --- Logical ---
      case 'IF': {
        const cond = args[0];
        const condBool = (cond === "TRUE" || cond === true || (typeof cond === "number" && cond !== 0));
        return condBool ? args[1] : (args[2] !== undefined ? args[2] : "");
      }
      case 'AND':
        return flatArgs.every(x => x === "TRUE" || x === true || (typeof x === "number" && x !== 0));
      case 'OR':
        return flatArgs.some(x => x === "TRUE" || x === true || (typeof x === "number" && x !== 0));
      case 'NOT':
        return !(flatArgs[0] === "TRUE" || flatArgs[0] === true || (typeof flatArgs[0] === "number" && flatArgs[0] !== 0));

      // --- Text ---
      case 'CONCAT':
      case 'CONCATENATE':
        return flatArgs.join("");
      case 'UPPER':
        return flatArgs.length > 0 ? String(flatArgs[0]).toUpperCase() : "";
      case 'LOWER':
        return flatArgs.length > 0 ? String(flatArgs[0]).toLowerCase() : "";
      case 'LEN':
        return flatArgs.length > 0 ? String(flatArgs[0]).length : 0;
      case 'TRIM':
        return flatArgs.length > 0 ? String(flatArgs[0]).trim() : "";

      // --- Lookup ---
      case 'VLOOKUP': {
        const lookupVal = args[0];
        const range = args[1]; // 2D array from RangeReference
        const colIdx = parseInt(args[2], 10) - 1;
        const exactMatch = args[3] !== undefined ? (args[3] === "FALSE" || args[3] === false) : true;

        if (!Array.isArray(range) || colIdx < 0 || isNaN(colIdx)) return "#VALUE!";

        for (let r = 0; r < range.length; r++) {
          const row = range[r];
          if (row && String(row[0]) === String(lookupVal)) {
            return row[colIdx] !== undefined ? row[colIdx] : "";
          }
        }
        return "#N/A";
      }

      default:
        return "#NAME?";
    }
  }
}
