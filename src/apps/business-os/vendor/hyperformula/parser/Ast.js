/**
 * CTOX Business OS - HyperFormula ESM Port
 * parser/Ast.js — AST (Abstract Syntax Tree) Node structures.
 *
 * ref: hyperformula/src/parser/Ast.ts:1-120
 */

export class AstNode {
  constructor(type) {
    this.type = type;
  }
}

export class LiteralNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:125-140
  constructor(valueType, value) {
    super('Literal');
    this.valueType = valueType; // 'NUMBER', 'STRING', 'BOOLEAN'
    this.value = value;
  }
}

export class CellReferenceNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:145-165
  constructor(cellAddress) {
    super('CellReference');
    this.address = cellAddress; // { sheet: number | string, col: number, row: number, absCol: bool, absRow: bool }
  }
}

export class RangeReferenceNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:170-195
  constructor(startAddress, endAddress) {
    super('RangeReference');
    this.start = startAddress;
    this.end = endAddress;
  }
}

export class BinaryOpNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:200-220
  constructor(op, left, right) {
    super('BinaryOp');
    this.op = op; // '+', '-', '*', '/', '^', '&', '=', '<', '>', '<=', '>=', '<>'
    this.left = left;
    this.right = right;
  }
}

export class UnaryOpNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:225-240
  constructor(op, expr) {
    super('UnaryOp');
    this.op = op; // '+', '-'
    this.expr = expr;
  }
}

export class FunctionCallNode extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:245-270
  constructor(name, args = []) {
    super('FunctionCall');
    this.name = name;
    this.args = args;
  }
}
