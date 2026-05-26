/**
 * CTOX Business OS - HyperFormula ESM Port
 * interpreter/FunctionRegistry.js — Catalog mapping spreadsheet functions to handlers.
 *
 * ref: hyperformula/src/interpreter/FunctionRegistry.ts:1-120
 */

export class FunctionRegistry {
  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:125-145
  constructor() {
    this.functions = new Map();
  }

  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:150-170
  register(name, handler) {
    this.functions.set(name.toUpperCase(), handler);
  }

  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:175-195
  has(name) {
    return this.functions.has(name.toUpperCase());
  }

  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:200-220
  get(name) {
    return this.functions.get(name.toUpperCase());
  }

  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:225-245
  getRegisteredFunctionIds() {
    return Array.from(this.functions.keys());
  }
}

export const functionRegistry = new FunctionRegistry();
