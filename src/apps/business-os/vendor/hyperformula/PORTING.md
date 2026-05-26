# HyperFormula → JavaScript ESM Port — Lookup Table

Upstream: `handsontable/hyperformula` tag **v2.7.1**
Scope: Lightweight, licensing-safe, high-performance formula engine for Business OS.

---

## Port Progress Dashboard

| Phase | File / Component | Upstream Reference | Tier | Status | Owner | Notes |
|:---|:---|:---|:---:|:---:|:---:|:---|
| **Phase 0** | `Config.js` | `src/Config.ts`, `src/ConfigParams.ts` | **T3** | done | Antigravity | Syntax-only port of spreadsheet parameters and options. |
| **Phase 0** | `Sheet.js` | `src/Sheet.ts` | **T2** | done | Antigravity | Substantive port of two-dimensional cell grid structures. |
| **Phase 1** | `parser/Lexer.js` | `src/parser/LexerConfig.ts` + Custom | **T3** | done | Antigravity | Lexing / tokenizing formulas into token streams. |
| **Phase 1** | `parser/Ast.js` | `src/parser/Ast.ts` | **T3** | done | Antigravity | AST node representations for math and logic operators. |
| **Phase 1** | `parser/Address.js` | `src/parser/Address.ts`, `src/parser/CellAddress.ts` | **T2** | done | Antigravity | Substantive port of A1/R1C1 and range converters. |
| **Phase 1** | `parser/FormulaParser.js` | `src/parser/FormulaParser.ts` | **T1** | done | Antigravity | Architectural port of the expression recursive-descent parser. |
| **Phase 2** | `DependencyGraph/Graph.js` | `src/DependencyGraph/Graph.ts`, `src/DependencyGraph/Vertex.ts` | **T2** | done | Antigravity | Manages cell/range references and vertex maps. |
| **Phase 2** | `DependencyGraph/TopSort.js` | `src/DependencyGraph/TopSort.ts` | **T1** | done | Antigravity | Kahn's cycle-detecting topological sort for recalculation order. |
| **Phase 3** | `interpreter/FunctionRegistry.js` | `src/interpreter/FunctionRegistry.ts` | **T3** | done | Antigravity | Maps SUM, AVERAGE, IF, VLOOKUP, etc. to evaluation plugins. |
| **Phase 3** | `interpreter/Interpreter.js` | `src/interpreter/Interpreter.ts` | **T1** | done | Antigravity | AST-walking evaluation engine executing mathematical operations. |
| **Phase 4** | `Evaluator.js` | `src/Evaluator.ts` | **T2** | done | Antigravity | Coordinates sheet-wide cell evaluation and recalculation. |
| **Phase 4** | `HyperFormula.js` | `src/HyperFormula.ts` | **T1** | done | Antigravity | Facade exporting public factories and CRUD workbook methods. |

---

## Status Legend
- `pending`: Not yet started.
- `claimed`: Claimed by an agent for implementation.
- `wip`: Work in progress.
- `done`: Completely ported, refactored for ESM, and tested.
- `skipped`: Deemed out of scope for the Business OS MVP.

## Tier Legend
- **T1 — Architectural Redesign**: Heavy algorithmic logic (e.g. topological cycle-detecting solver, operator-precedence parser). Needs senior main-agent attention.
- **T2 — Substantive Port**: File translation with significant ESM refactoring and dynamic data-structure adaptation.
- **T3 — Syntax-Only Port**: Mostly mechanical translation from TypeScript to ES6 (removing types, simple syntax cleanup).
