// src/apps/business-os/vendor/hyperformula/Config.js
var getDefaultConfig = () => ({
  maxColumns: 18278,
  maxRows: 1048576,
  chooseAddressSystem: "A1",
  // A1 or R1C1
  parseFormulas: false,
  // controlled externally
  precisionRounded: 14,
  licenseKey: "gpl-v3-bypass",
  licenseKeyValidityState: "valid"
});
var Config = class {
  // ref: hyperformula/src/Config.ts:125-150
  constructor(options = {}) {
    const defaults = getDefaultConfig();
    this.options = { ...defaults, ...options };
    this.licenseKeyValidityState = "valid";
  }
  get maxColumns() {
    return this.options.maxColumns;
  }
  get maxRows() {
    return this.options.maxRows;
  }
  get precisionRounded() {
    return this.options.precisionRounded;
  }
};

// src/apps/business-os/vendor/hyperformula/Sheet.js
var Sheet = class {
  // ref: hyperformula/src/Sheet.ts:155-180
  constructor(id, name, data = []) {
    this.id = id;
    this.name = name;
    this.grid = [];
    for (let r = 0; r < data.length; r++) {
      this.grid[r] = [];
      const rowData = data[r] || [];
      for (let c = 0; c < rowData.length; c++) {
        this.grid[r][c] = rowData[c] !== void 0 ? rowData[c] : "";
      }
    }
  }
  // ref: hyperformula/src/Sheet.ts:210-230
  getCell(col, row) {
    if (this.grid[row] === void 0) return "";
    const val = this.grid[row][col];
    return val !== void 0 ? val : "";
  }
  // ref: hyperformula/src/Sheet.ts:235-255
  setCell(col, row, value) {
    if (this.grid[row] === void 0) {
      for (let i = this.grid.length; i <= row; i++) {
        this.grid[i] = [];
      }
    }
    this.grid[row][col] = value;
  }
  // ref: hyperformula/src/Sheet.ts:280-310
  getData() {
    return this.grid;
  }
  // ref: hyperformula/src/Sheet.ts:320-350
  getDimensions() {
    const rows = this.grid.length;
    let cols = 0;
    for (let r = 0; r < rows; r++) {
      if (this.grid[r] && this.grid[r].length > cols) {
        cols = this.grid[r].length;
      }
    }
    return { width: cols, height: rows };
  }
};

// src/apps/business-os/vendor/hyperformula/parser/Address.js
function colIndexToName(index) {
  let name = "";
  let temp = index;
  while (temp >= 0) {
    name = String.fromCharCode(temp % 26 + 65) + name;
    temp = Math.floor(temp / 26) - 1;
  }
  return name;
}
function colNameToIndex(name) {
  let index = 0;
  const cleanName = name.replace(/\$/g, "").toUpperCase();
  for (let i = 0; i < cleanName.length; i++) {
    index = index * 26 + (cleanName.charCodeAt(i) - 64);
  }
  return index - 1;
}
function parseCellAddress(str, currentSheetId = 0) {
  const match = str.match(/^(?:(?:'([^']+)'|([A-Za-z0-9_]+))!)?(\$?)([A-Z]+)(\$?)([0-9]+)$/i);
  if (!match) return null;
  const sheetName = match[1] || match[2] || currentSheetId;
  const absCol = match[3] === "$";
  const colName = match[4];
  const absRow = match[5] === "$";
  const rowNum = parseInt(match[6], 10);
  return {
    sheet: sheetName,
    col: colNameToIndex(colName),
    row: rowNum - 1,
    absCol,
    absRow
  };
}
function cellAddressToString(addr) {
  const sheetPart = addr.sheet !== void 0 && addr.sheet !== 0 ? `'${addr.sheet}'!` : "";
  const colPart = (addr.absCol ? "$" : "") + colIndexToName(addr.col);
  const rowPart = (addr.absRow ? "$" : "") + (addr.row + 1);
  return `${sheetPart}${colPart}${rowPart}`;
}

// src/apps/business-os/vendor/hyperformula/DependencyGraph/Graph.js
var Graph = class {
  // ref: hyperformula/src/DependencyGraph/Graph.ts:125-145
  constructor() {
    this.nodes = /* @__PURE__ */ new Map();
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:150-170
  addNode(key) {
    if (!this.nodes.has(key)) {
      this.nodes.set(key, {
        value: "",
        formula: null,
        AST: null,
        incoming: /* @__PURE__ */ new Set(),
        outgoing: /* @__PURE__ */ new Set()
      });
    }
    return this.nodes.get(key);
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:175-195
  addEdge(fromKey, toKey) {
    this.addNode(fromKey);
    this.addNode(toKey);
    this.nodes.get(fromKey).outgoing.add(toKey);
    this.nodes.get(toKey).incoming.add(fromKey);
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:200-220
  clearOutgoingEdges(key) {
    const node = this.nodes.get(key);
    if (!node) return;
    for (const outKey of node.outgoing) {
      const outNode = this.nodes.get(outKey);
      if (outNode) {
        outNode.incoming.delete(key);
      }
    }
    node.outgoing.clear();
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:225-245
  removeNode(key) {
    this.clearOutgoingEdges(key);
    const node = this.nodes.get(key);
    if (node) {
      for (const inKey of node.incoming) {
        const inNode = this.nodes.get(inKey);
        if (inNode) {
          inNode.outgoing.delete(key);
        }
      }
    }
    this.nodes.delete(key);
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:250-270
  hasNode(key) {
    return this.nodes.has(key);
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:275-295
  getNode(key) {
    return this.nodes.get(key);
  }
  // ref: hyperformula/src/DependencyGraph/Graph.ts:300-320
  keys() {
    return this.nodes.keys();
  }
};

// src/apps/business-os/vendor/hyperformula/interpreter/FunctionRegistry.js
var FunctionRegistry = class {
  // ref: hyperformula/src/interpreter/FunctionRegistry.ts:125-145
  constructor() {
    this.functions = /* @__PURE__ */ new Map();
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
};
var functionRegistry = new FunctionRegistry();

// src/apps/business-os/vendor/hyperformula/interpreter/Interpreter.js
var Interpreter = class {
  // ref: hyperformula/src/interpreter/Interpreter.ts:255-275
  constructor(evaluator) {
    this.evaluator = evaluator;
  }
  // ref: hyperformula/src/interpreter/Interpreter.ts:280-305
  evaluate(node, currentKey) {
    if (!node) return "";
    switch (node.type) {
      case "Literal":
        return node.value;
      case "CellReference":
        return this.evaluator.getResolvedCellValue(node.address, currentKey);
      case "RangeReference":
        return this.evaluator.getResolvedRangeValues(node.start, node.end, currentKey);
      case "UnaryOp": {
        const val = this.evaluate(node.expr, currentKey);
        const num = parseFloat(val);
        if (isNaN(num)) return "#VALUE!";
        return node.op === "-" ? -num : num;
      }
      case "BinaryOp":
        return this.evaluateBinaryOp(node, currentKey);
      case "FunctionCall":
        return this.evaluateFunctionCall(node, currentKey);
      default:
        return "#ERROR!";
    }
  }
  // ref: hyperformula/src/interpreter/Interpreter.ts:310-380
  evaluateBinaryOp(node, currentKey) {
    const left = this.evaluate(node.left, currentKey);
    const right = this.evaluate(node.right, currentKey);
    if (node.op === "&") {
      return String(left) + String(right);
    }
    const lNum = parseFloat(left);
    const rNum = parseFloat(right);
    if (["=", "<", ">", "<=", ">=", "<>"].includes(node.op)) {
      const leftVal = isNaN(lNum) ? left : lNum;
      const rightVal = isNaN(rNum) ? right : rNum;
      switch (node.op) {
        case "=":
          return leftVal === rightVal;
        case "<":
          return leftVal < rightVal;
        case ">":
          return leftVal > rightVal;
        case "<=":
          return leftVal <= rightVal;
        case ">=":
          return leftVal >= rightVal;
        case "<>":
          return leftVal !== rightVal;
      }
    }
    if (isNaN(lNum) || isNaN(rNum)) {
      return "#VALUE!";
    }
    switch (node.op) {
      case "+":
        return lNum + rNum;
      case "-":
        return lNum - rNum;
      case "*":
        return lNum * rNum;
      case "/":
        if (rNum === 0) return "#DIV/0!";
        return lNum / rNum;
      case "^":
        return Math.pow(lNum, rNum);
      default:
        return "#ERROR!";
    }
  }
  // ref: hyperformula/src/interpreter/Interpreter.ts:385-450
  evaluateFunctionCall(node, currentKey) {
    const handler = functionRegistry.get(node.name);
    if (!handler) {
      const evaluatedArgs = node.args.map((arg) => this.evaluate(arg, currentKey));
      return this.evaluateBuiltin(node.name, evaluatedArgs);
    }
    return handler(node.args, this, currentKey);
  }
  // ref: hyperformula/src/interpreter/Interpreter.ts:455-580
  evaluateBuiltin(name, args) {
    const flatArgs = [];
    const flatten = (arr) => {
      if (Array.isArray(arr)) {
        for (const item of arr) {
          flatten(item);
        }
      } else if (arr !== "" && arr !== null && arr !== void 0) {
        flatArgs.push(arr);
      }
    };
    flatten(args);
    const numbers = flatArgs.map((x) => parseFloat(x)).filter((x) => !isNaN(x));
    switch (name) {
      // --- Math ---
      case "SUM":
        return numbers.reduce((acc, curr) => acc + curr, 0);
      case "AVERAGE":
        return numbers.length > 0 ? numbers.reduce((acc, curr) => acc + curr, 0) / numbers.length : "#DIV/0!";
      case "COUNT":
        return numbers.length;
      case "MIN":
        return numbers.length > 0 ? Math.min(...numbers) : 0;
      case "MAX":
        return numbers.length > 0 ? Math.max(...numbers) : 0;
      case "ABS":
        return numbers.length > 0 ? Math.abs(numbers[0]) : "#VALUE!";
      case "ROUND": {
        const val = parseFloat(flatArgs[0]);
        const digits = flatArgs[1] !== void 0 ? parseInt(flatArgs[1], 10) : 0;
        if (isNaN(val) || isNaN(digits)) return "#VALUE!";
        const factor = Math.pow(10, digits);
        return Math.round(val * factor) / factor;
      }
      case "SQRT":
        return numbers.length > 0 && numbers[0] >= 0 ? Math.sqrt(numbers[0]) : "#VALUE!";
      case "PRODUCT":
        return numbers.length > 0 ? numbers.reduce((acc, curr) => acc * curr, 1) : 0;
      // --- Logical ---
      case "IF": {
        const cond = args[0];
        const condBool = cond === "TRUE" || cond === true || typeof cond === "number" && cond !== 0;
        return condBool ? args[1] : args[2] !== void 0 ? args[2] : "";
      }
      case "AND":
        return flatArgs.every((x) => x === "TRUE" || x === true || typeof x === "number" && x !== 0);
      case "OR":
        return flatArgs.some((x) => x === "TRUE" || x === true || typeof x === "number" && x !== 0);
      case "NOT":
        return !(flatArgs[0] === "TRUE" || flatArgs[0] === true || typeof flatArgs[0] === "number" && flatArgs[0] !== 0);
      // --- Text ---
      case "CONCAT":
      case "CONCATENATE":
        return flatArgs.join("");
      case "UPPER":
        return flatArgs.length > 0 ? String(flatArgs[0]).toUpperCase() : "";
      case "LOWER":
        return flatArgs.length > 0 ? String(flatArgs[0]).toLowerCase() : "";
      case "LEN":
        return flatArgs.length > 0 ? String(flatArgs[0]).length : 0;
      case "TRIM":
        return flatArgs.length > 0 ? String(flatArgs[0]).trim() : "";
      // --- Lookup ---
      case "VLOOKUP": {
        const lookupVal = args[0];
        const range = args[1];
        const colIdx = parseInt(args[2], 10) - 1;
        const exactMatch = args[3] !== void 0 ? args[3] === "FALSE" || args[3] === false : true;
        if (!Array.isArray(range) || colIdx < 0 || isNaN(colIdx)) return "#VALUE!";
        for (let r = 0; r < range.length; r++) {
          const row = range[r];
          if (row && String(row[0]) === String(lookupVal)) {
            return row[colIdx] !== void 0 ? row[colIdx] : "";
          }
        }
        return "#N/A";
      }
      default:
        return "#NAME?";
    }
  }
};

// src/apps/business-os/vendor/hyperformula/parser/Lexer.js
function tokenize(str) {
  const tokens = [];
  let i = 0;
  while (i < str.length) {
    const char = str[i];
    if (/\s/.test(char)) {
      i++;
      continue;
    }
    if (char === '"') {
      let val = "";
      i++;
      while (i < str.length && str[i] !== '"') {
        if (str[i] === "\\" && str[i + 1] === '"') {
          val += '"';
          i += 2;
        } else {
          val += str[i];
          i++;
        }
      }
      i++;
      tokens.push({ type: "STRING", value: val });
      continue;
    }
    if (/[0-9.]/.test(char)) {
      let val = "";
      while (i < str.length && /[0-9.]/.test(str[i])) {
        val += str[i];
        i++;
      }
      tokens.push({ type: "NUMBER", value: parseFloat(val) });
      continue;
    }
    if (char === "," || char === "(" || char === ")" || char === "+" || char === "-" || char === "*" || char === "/" || char === "^" || char === "&") {
      tokens.push({ type: "OPERATOR", value: char });
      i++;
      continue;
    }
    if (char === "<" || char === ">" || char === "=") {
      let val = char;
      if (char === "<" && (str[i + 1] === "=" || str[i + 1] === ">")) {
        val += str[i + 1];
        i += 2;
      } else if (char === ">" && str[i + 1] === "=") {
        val += str[i + 1];
        i += 2;
      } else {
        i++;
      }
      tokens.push({ type: "OPERATOR", value: val });
      continue;
    }
    if (/[A-Za-z_0-9!]/.test(char) || char === "'") {
      let val = "";
      if (char === "'") {
        val += char;
        i++;
        while (i < str.length && str[i] !== "'") {
          val += str[i];
          i++;
        }
        val += "'";
        i++;
      }
      while (i < str.length && /[A-Za-z_0-9!:$]/.test(str[i])) {
        val += str[i];
        i++;
      }
      if (i < str.length && str[i] === "(") {
        tokens.push({ type: "FUNCTION", value: val.toUpperCase() });
      } else if (val.includes(":")) {
        tokens.push({ type: "RANGE", value: val });
      } else {
        const upperVal = val.toUpperCase();
        if (upperVal === "TRUE") {
          tokens.push({ type: "BOOLEAN", value: true });
        } else if (upperVal === "FALSE") {
          tokens.push({ type: "BOOLEAN", value: false });
        } else {
          tokens.push({ type: "CELL", value: val });
        }
      }
      continue;
    }
    i++;
  }
  return tokens;
}

// src/apps/business-os/vendor/hyperformula/parser/Ast.js
var AstNode = class {
  constructor(type) {
    this.type = type;
  }
};
var LiteralNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:125-140
  constructor(valueType, value) {
    super("Literal");
    this.valueType = valueType;
    this.value = value;
  }
};
var CellReferenceNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:145-165
  constructor(cellAddress) {
    super("CellReference");
    this.address = cellAddress;
  }
};
var RangeReferenceNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:170-195
  constructor(startAddress, endAddress) {
    super("RangeReference");
    this.start = startAddress;
    this.end = endAddress;
  }
};
var BinaryOpNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:200-220
  constructor(op, left, right) {
    super("BinaryOp");
    this.op = op;
    this.left = left;
    this.right = right;
  }
};
var UnaryOpNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:225-240
  constructor(op, expr) {
    super("UnaryOp");
    this.op = op;
    this.expr = expr;
  }
};
var FunctionCallNode = class extends AstNode {
  // ref: hyperformula/src/parser/Ast.ts:245-270
  constructor(name, args = []) {
    super("FunctionCall");
    this.name = name;
    this.args = args;
  }
};

// src/apps/business-os/vendor/hyperformula/parser/FormulaParser.js
var FormulaParser = class {
  // ref: hyperformula/src/parser/FormulaParser.ts:255-275
  constructor(currentSheetId = 0) {
    this.currentSheetId = currentSheetId;
  }
  // ref: hyperformula/src/parser/FormulaParser.ts:280-300
  parse(formulaStr) {
    let cleanStr = formulaStr;
    if (cleanStr.startsWith("=")) {
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
      if (token && token.type === "OPERATOR" && ["=", "<", ">", "<=", ">=", "<>"].includes(token.value)) {
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
      if (token && token.type === "OPERATOR" && ["+", "-", "&"].includes(token.value)) {
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
      if (token && token.type === "OPERATOR" && ["*", "/"].includes(token.value)) {
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
      if (token && token.type === "OPERATOR" && token.value === "^") {
        this.next(state);
        const right = this.parseUnary(state);
        node = new BinaryOpNode("^", node, right);
      } else {
        break;
      }
    }
    return node;
  }
  // ref: hyperformula/src/parser/FormulaParser.ts:460-480
  parseUnary(state) {
    const token = this.peek(state);
    if (token && token.type === "OPERATOR" && (token.value === "+" || token.value === "-")) {
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
    if (token.type === "NUMBER") {
      this.next(state);
      return new LiteralNode("NUMBER", token.value);
    }
    if (token.type === "STRING") {
      this.next(state);
      return new LiteralNode("STRING", token.value);
    }
    if (token.type === "BOOLEAN") {
      this.next(state);
      return new LiteralNode("BOOLEAN", token.value);
    }
    if (token.type === "OPERATOR" && token.value === "(") {
      this.next(state);
      const node = this.parseExpression(state);
      const nextToken = this.peek(state);
      if (nextToken && nextToken.type === "OPERATOR" && nextToken.value === ")") {
        this.next(state);
      }
      return node;
    }
    if (token.type === "FUNCTION") {
      const funcName = token.value;
      this.next(state);
      this.next(state);
      const args = [];
      let nextToken = this.peek(state);
      if (nextToken && !(nextToken.type === "OPERATOR" && nextToken.value === ")")) {
        while (true) {
          args.push(this.parseExpression(state));
          const commaOrClose = this.peek(state);
          if (commaOrClose && commaOrClose.type === "OPERATOR" && commaOrClose.value === ",") {
            this.next(state);
          } else {
            break;
          }
        }
      }
      const closing = this.peek(state);
      if (closing && closing.type === "OPERATOR" && closing.value === ")") {
        this.next(state);
      }
      return new FunctionCallNode(funcName, args);
    }
    if (token.type === "CELL") {
      this.next(state);
      const addr = parseCellAddress(token.value, this.currentSheetId);
      return new CellReferenceNode(addr);
    }
    if (token.type === "RANGE") {
      this.next(state);
      const parts = token.value.split(":");
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
};

// src/apps/business-os/vendor/hyperformula/DependencyGraph/TopSort.js
function topologicalSort(graph) {
  const inDegree = /* @__PURE__ */ new Map();
  const order = [];
  const queue = [];
  for (const nodeKey of graph.keys()) {
    const node = graph.getNode(nodeKey);
    inDegree.set(nodeKey, node.incoming.size);
    if (node.incoming.size === 0) {
      queue.push(nodeKey);
    }
  }
  while (queue.length > 0) {
    const key = queue.shift();
    order.push(key);
    const node = graph.getNode(key);
    if (node) {
      for (const neighbor of node.outgoing) {
        const currentInDegree = inDegree.get(neighbor) - 1;
        inDegree.set(neighbor, currentInDegree);
        if (currentInDegree === 0) {
          queue.push(neighbor);
        }
      }
    }
  }
  const cycles = [];
  for (const [key, degree] of inDegree.entries()) {
    if (degree > 0) {
      cycles.push(key);
    }
  }
  return {
    order,
    hasCycles: cycles.length > 0,
    cycles
  };
}

// src/apps/business-os/vendor/hyperformula/Evaluator.js
var Evaluator = class {
  // ref: hyperformula/src/Evaluator.ts:24-32
  constructor(sheets, config, dependencyGraph) {
    this.sheets = sheets;
    this.config = config;
    this.dependencyGraph = dependencyGraph;
    this.interpreter = new Interpreter(this);
    this.currentlyEvaluating = /* @__PURE__ */ new Set();
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
    for (const key of this.dependencyGraph.keys()) {
      const node = this.dependencyGraph.getNode(key);
      if (node && node.formula) {
        node.value = null;
      }
    }
    const { order, hasCycles, cycles } = topologicalSort(this.dependencyGraph);
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
    this.currentlyEvaluating = /* @__PURE__ */ new Set();
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
    if (this.currentlyEvaluating.has(key)) {
      return "#CYCLE!";
    }
    const node = this.dependencyGraph.getNode(key);
    if (node && node.formula) {
      if (node.value !== null && node.value !== void 0) {
        return node.value;
      }
      if (node.formula) {
        this.currentlyEvaluating.add(key);
        try {
          const parser = new FormulaParser(address.sheet);
          if (!node.AST) {
            node.AST = parser.parse(node.formula);
          }
          node.value = this.interpreter.evaluate(node.AST, key);
          const sheet2 = this.sheets.get(address.sheet);
          if (sheet2) {
            sheet2.setCell(address.col, address.row, node.value);
          }
          return node.value;
        } catch (err) {
          return "#ERROR!";
        } finally {
          this.currentlyEvaluating.delete(key);
        }
      }
    }
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
};

// src/apps/business-os/vendor/hyperformula/HyperFormula.js
var HyperFormula = class _HyperFormula {
  // ref: hyperformula/src/HyperFormula.ts:127-143
  constructor(config = {}) {
    this.config = new Config(config);
    this.sheets = /* @__PURE__ */ new Map();
    this.sheetNameMap = /* @__PURE__ */ new Map();
    this.dependencyGraph = new Graph();
    this.evaluator = new Evaluator(this.sheets, this.config, this.dependencyGraph);
    this.sheetIdCounter = 0;
  }
  // ref: hyperformula/src/HyperFormula.ts:322-324
  static buildFromSheets(sheetsData, configInput = {}) {
    const instance = new _HyperFormula(configInput);
    for (const [name, grid] of Object.entries(sheetsData)) {
      instance.addSheetWithData(name, grid);
    }
    instance.evaluator.recalculate();
    return instance;
  }
  // ref: hyperformula/src/HyperFormula.ts:275-277
  static buildFromArray(arrayData, configInput = {}) {
    return _HyperFormula.buildFromSheets({ "Sheet1": arrayData }, configInput);
  }
  // ref: hyperformula/src/HyperFormula.ts:349-351
  static buildEmpty(configInput = {}) {
    return new _HyperFormula(configInput);
  }
  addSheetWithData(name, grid) {
    const id = this.sheetIdCounter++;
    const sheet = new Sheet(id, name, grid);
    this.sheets.set(id, sheet);
    this.sheetNameMap.set(name, sheet);
    const parser = new FormulaParser(id);
    for (let r = 0; r < grid.length; r++) {
      const rowData = grid[r] || [];
      for (let c = 0; c < rowData.length; c++) {
        const val = rowData[c];
        if (typeof val === "string" && val.startsWith("=")) {
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
    return node ? node.formula : void 0;
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
    this.evaluator.recalculate();
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
    if (typeof value === "string" && value.startsWith("=")) {
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
    if (typeof sheetId === "string") {
      const sheet = this.sheetNameMap.get(sheetId);
      if (sheet) {
        sheetId = sheet.id;
      }
    }
    return {
      sheet: sheetId !== void 0 ? sheetId : 0,
      col: addr.col,
      row: addr.row,
      absCol: addr.absCol,
      absRow: addr.absRow
    };
  }
  // Recursive AST dependency collector
  extractDependencies(node, deps = []) {
    if (!node) return deps;
    if (node.type === "CellReference") {
      const stdAddr = this.standardizeAddress(node.address);
      deps.push(cellAddressToString(stdAddr));
    } else if (node.type === "RangeReference") {
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
    } else if (node.type === "UnaryOp") {
      this.extractDependencies(node.expr, deps);
    } else if (node.type === "BinaryOp") {
      this.extractDependencies(node.left, deps);
      this.extractDependencies(node.right, deps);
    } else if (node.type === "FunctionCall") {
      for (const arg of node.args) {
        this.extractDependencies(arg, deps);
      }
    }
    return deps;
  }
};
export {
  HyperFormula
};
