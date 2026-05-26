/**
 * CTOX Business OS - HyperFormula ESM Port
 * parser/FormulaParser.js — Recursive-descent expression parser for formula syntax.
 *
 * ref: hyperformula/src/parser/FormulaParser.ts:1-250
 */

import { tokenize } from './Lexer.js';
import { parseCellAddress } from './Address.js';
import {
  LiteralNode,
  CellReferenceNode,
  RangeReferenceNode,
  BinaryOpNode,
  UnaryOpNode,
  FunctionCallNode
} from './Ast.js';

export class FormulaParser {
  // ref: hyperformula/src/parser/FormulaParser.ts:255-275
  constructor(currentSheetId = 0) {
    this.currentSheetId = currentSheetId;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:280-300
  parse(formulaStr) {
    let cleanStr = formulaStr;
    if (cleanStr.startsWith('=')) {
      cleanStr = cleanStr.substring(1);
    }
    const tokens = tokenize(cleanStr);
    const state = { tokens, pos: 0 };
    return this.parseExpression(state);
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:310-335
  parseExpression(state) {
    return this.parseLogical(state);
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:340-365
  parseLogical(state) {
    let node = this.parseAdditive(state);
    while (true) {
      const token = this.peek(state);
      if (token && token.type === 'OPERATOR' && ['=', '<', '>', '<=', '>=', '<>'].includes(token.value)) {
        this.next(state);
        const right = this.parseAdditive(state);
        node = new BinaryOpNode(token.value, node, right);
      } else {
        break;
      }
    }
    return node;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:370-395
  parseAdditive(state) {
    let node = this.parseMultiplicative(state);
    while (true) {
      const token = this.peek(state);
      if (token && token.type === 'OPERATOR' && ['+', '-', '&'].includes(token.value)) {
        this.next(state);
        const right = this.parseMultiplicative(state);
        node = new BinaryOpNode(token.value, node, right);
      } else {
        break;
      }
    }
    return node;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:400-425
  parseMultiplicative(state) {
    let node = this.parseExponentiation(state);
    while (true) {
      const token = this.peek(state);
      if (token && token.type === 'OPERATOR' && ['*', '/'].includes(token.value)) {
        this.next(state);
        const right = this.parseExponentiation(state);
        node = new BinaryOpNode(token.value, node, right);
      } else {
        break;
      }
    }
    return node;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:430-455
  parseExponentiation(state) {
    let node = this.parseUnary(state);
    while (true) {
      const token = this.peek(state);
      if (token && token.type === 'OPERATOR' && token.value === '^') {
        this.next(state);
        const right = this.parseUnary(state);
        node = new BinaryOpNode('^', node, right);
      } else {
        break;
      }
    }
    return node;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:460-480
  parseUnary(state) {
    const token = this.peek(state);
    if (token && token.type === 'OPERATOR' && (token.value === '+' || token.value === '-')) {
      this.next(state);
      const expr = this.parseUnary(state);
      return new UnaryOpNode(token.value, expr);
    }
    return this.parsePrimary(state);
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:485-550
  parsePrimary(state) {
    const token = this.peek(state);
    if (!token) return null;

    if (token.type === 'NUMBER') {
      this.next(state);
      return new LiteralNode('NUMBER', token.value);
    }

    if (token.type === 'STRING') {
      this.next(state);
      return new LiteralNode('STRING', token.value);
    }

    if (token.type === 'BOOLEAN') {
      this.next(state);
      return new LiteralNode('BOOLEAN', token.value);
    }

    if (token.type === 'OPERATOR' && token.value === '(') {
      this.next(state); // consume '('
      const node = this.parseExpression(state);
      const nextToken = this.peek(state);
      if (nextToken && nextToken.type === 'OPERATOR' && nextToken.value === ')') {
        this.next(state); // consume ')'
      }
      return node;
    }

    if (token.type === 'FUNCTION') {
      const funcName = token.value;
      this.next(state); // consume function name
      this.next(state); // consume '('

      const args = [];
      let nextToken = this.peek(state);
      if (nextToken && !(nextToken.type === 'OPERATOR' && nextToken.value === ')')) {
        while (true) {
          args.push(this.parseExpression(state));
          const commaOrClose = this.peek(state);
          if (commaOrClose && commaOrClose.type === 'OPERATOR' && commaOrClose.value === ',') {
            this.next(state); // consume ','
          } else {
            break;
          }
        }
      }

      const closing = this.peek(state);
      if (closing && closing.type === 'OPERATOR' && closing.value === ')') {
        this.next(state); // consume ')'
      }
      return new FunctionCallNode(funcName, args);
    }

    if (token.type === 'CELL') {
      this.next(state);
      const addr = parseCellAddress(token.value, this.currentSheetId);
      return new CellReferenceNode(addr);
    }

    if (token.type === 'RANGE') {
      this.next(state);
      const parts = token.value.split(':');
      const start = parseCellAddress(parts[0], this.currentSheetId);
      const end = parseCellAddress(parts[1], this.currentSheetId);
      return new RangeReferenceNode(start, end);
    }

    this.next(state);
    return null;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:555-565
  peek(state) {
    return state.tokens[state.pos] || null;
  }

  // ref: hyperformula/src/parser/FormulaParser.ts:570-580
  next(state) {
    return state.tokens[state.pos++] || null;
  }
}
