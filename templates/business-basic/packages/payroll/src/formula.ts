/**
 * Payroll component formula DSL.
 *
 * Three formula kinds, no host eval:
 *   - fix(<amount>)
 *   - percent_of(<base-name>, <pct>)
 *   - formula(<expression>)        // grammar below
 *
 * formula() expression grammar:
 *   expr   := term (("+" | "-") term)*
 *   term   := factor (("*" | "/") factor)*
 *   factor := number | identifier | "(" expr ")" | "-" factor
 *
 * Identifiers resolve in this order:
 *   1. previously computed component codes (case-sensitive)
 *   2. whitelisted variables: base_salary, payment_days, lwp_days, absent_days, working_days
 *
 * No function calls. No member access. No strings. No bitwise. No keywords.
 */

import type { PayrollComponent } from "./types";

export type FormulaContext = {
  baseSalary: number;
  paymentDays: number;
  lwpDays: number;
  absentDays: number;
  workingDays: number;
  components: Record<string, number>;
};

export class FormulaError extends Error {
  constructor(message: string, readonly position?: number) {
    super(message);
    this.name = "FormulaError";
  }
}

type NumericKey = "baseSalary" | "paymentDays" | "lwpDays" | "absentDays" | "workingDays";
const RESERVED: Record<string, NumericKey> = {
  base_salary: "baseSalary",
  payment_days: "paymentDays",
  lwp_days: "lwpDays",
  absent_days: "absentDays",
  working_days: "workingDays"
};

export function evaluateComponent(component: PayrollComponent, ctx: FormulaContext): number {
  const raw = computeRaw(component, ctx);
  if (component.dependsOnPaymentDays && ctx.workingDays > 0) {
    return round2((raw * ctx.paymentDays) / ctx.workingDays);
  }
  return round2(raw);
}

function computeRaw(component: PayrollComponent, ctx: FormulaContext): number {
  if (component.formulaKind === "fix") {
    return component.formulaAmount ?? 0;
  }
  if (component.formulaKind === "percent_of") {
    const baseName = component.formulaBase ?? "base_salary";
    const pct = component.formulaPercent ?? 0;
    const baseValue = resolveIdentifier(baseName, ctx);
    return (baseValue * pct) / 100;
  }
  if (component.formulaKind === "formula") {
    const expression = component.formulaExpression ?? "0";
    return evaluateExpression(expression, ctx);
  }
  return 0;
}

export function evaluateExpression(source: string, ctx: FormulaContext): number {
  const tokens = tokenize(source);
  const parser = new Parser(tokens, ctx);
  const value = parser.parseExpr();
  parser.expectEnd();
  return value;
}

export function resolveIdentifier(name: string, ctx: FormulaContext): number {
  if (Object.prototype.hasOwnProperty.call(RESERVED, name)) {
    const key = RESERVED[name];
    return ctx[key];
  }
  if (Object.prototype.hasOwnProperty.call(ctx.components, name)) {
    return ctx.components[name];
  }
  throw new FormulaError(`unknown identifier: ${name}`);
}

type Token =
  | { kind: "number"; value: number; pos: number }
  | { kind: "ident"; value: string; pos: number }
  | { kind: "op"; value: "+" | "-" | "*" | "/" | "(" | ")"; pos: number };

function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < source.length) {
    const c = source[i];
    if (c === " " || c === "\t" || c === "\n" || c === "\r") {
      i += 1;
      continue;
    }
    if (c === "+" || c === "-" || c === "*" || c === "/" || c === "(" || c === ")") {
      tokens.push({ kind: "op", value: c, pos: i });
      i += 1;
      continue;
    }
    if (isDigit(c) || (c === "." && isDigit(source[i + 1] ?? ""))) {
      let j = i;
      let dot = false;
      while (j < source.length) {
        const cj = source[j];
        if (isDigit(cj)) {
          j += 1;
          continue;
        }
        if (cj === ".") {
          if (dot) throw new FormulaError("unexpected '.'", j);
          dot = true;
          j += 1;
          continue;
        }
        break;
      }
      const value = Number(source.slice(i, j));
      if (Number.isNaN(value)) throw new FormulaError("invalid number", i);
      tokens.push({ kind: "number", value, pos: i });
      i = j;
      continue;
    }
    if (isIdentStart(c)) {
      let j = i + 1;
      while (j < source.length && isIdentBody(source[j])) j += 1;
      tokens.push({ kind: "ident", value: source.slice(i, j), pos: i });
      i = j;
      continue;
    }
    throw new FormulaError(`unexpected character '${c}'`, i);
  }
  return tokens;
}

function isDigit(c: string) {
  return c >= "0" && c <= "9";
}
function isIdentStart(c: string) {
  return (c >= "a" && c <= "z") || (c >= "A" && c <= "Z") || c === "_";
}
function isIdentBody(c: string) {
  return isIdentStart(c) || isDigit(c);
}

class Parser {
  private idx = 0;
  constructor(private readonly tokens: Token[], private readonly ctx: FormulaContext) {}

  parseExpr(): number {
    let value = this.parseTerm();
    while (true) {
      const t = this.peek();
      if (t && t.kind === "op" && (t.value === "+" || t.value === "-")) {
        this.idx += 1;
        const rhs = this.parseTerm();
        value = t.value === "+" ? value + rhs : value - rhs;
        continue;
      }
      break;
    }
    return value;
  }

  parseTerm(): number {
    let value = this.parseFactor();
    while (true) {
      const t = this.peek();
      if (t && t.kind === "op" && (t.value === "*" || t.value === "/")) {
        this.idx += 1;
        const rhs = this.parseFactor();
        if (t.value === "/" && rhs === 0) throw new FormulaError("division by zero", t.pos);
        value = t.value === "*" ? value * rhs : value / rhs;
        continue;
      }
      break;
    }
    return value;
  }

  parseFactor(): number {
    const t = this.peek();
    if (!t) throw new FormulaError("unexpected end of expression");
    if (t.kind === "op" && t.value === "-") {
      this.idx += 1;
      return -this.parseFactor();
    }
    if (t.kind === "op" && t.value === "(") {
      this.idx += 1;
      const value = this.parseExpr();
      const close = this.peek();
      if (!close || close.kind !== "op" || close.value !== ")") {
        throw new FormulaError("expected ')'", close?.pos);
      }
      this.idx += 1;
      return value;
    }
    if (t.kind === "number") {
      this.idx += 1;
      return t.value;
    }
    if (t.kind === "ident") {
      this.idx += 1;
      return resolveIdentifier(t.value, this.ctx);
    }
    throw new FormulaError(`unexpected token at position ${t.pos}`, t.pos);
  }

  expectEnd() {
    if (this.idx !== this.tokens.length) {
      throw new FormulaError(`unexpected trailing token at position ${this.tokens[this.idx]?.pos ?? -1}`);
    }
  }

  private peek() {
    return this.tokens[this.idx];
  }
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}
