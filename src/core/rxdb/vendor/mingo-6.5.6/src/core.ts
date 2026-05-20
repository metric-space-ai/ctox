import { Iterator } from "./lazy";
import {
  Any,
  AnyObject,
  ArrayOrObject,
  Callback,
  HashFunction,
  Predicate,
  WindowOperatorInput
} from "./types";
import {
  assert,
  has,
  isArray,
  isFunction,
  isNil,
  isObject,
  isOperator,
  isString,
  resolve
} from "./util";

/**
 * Resolves the given string to a Collection.
 * This is useful for operators that require a second collection to use such as $lookup and $out.
 * The collection is not cached and will be resolved each time it is used.
 */
export type CollectionResolver = (name: string) => AnyObject[];

/** Specification for collation options */
export interface CollationSpec {
  readonly locale: string;
  readonly caseLevel?: boolean;
  readonly caseFirst?: "upper" | "lower" | "off";
  readonly strength?: 1 | 2 | 3;
  readonly numericOrdering?: boolean;
  readonly alternate?: string;
  readonly maxVariable?: never; // unsupported
  readonly backwards?: never; // unsupported
}

/**
 * JSON schema validator
 */
export type JsonSchemaValidator = (schema: AnyObject) => Predicate<AnyObject>;

/**
 * Specified how input and output documents are processed.
 */
export enum ProcessingMode {
  /** Do not clone inputs or outputs. Resulting documents may share references. @default */
  CLONE_OFF = 0,
  /** Clone input documents to maintain immutability of original input. */
  CLONE_INPUT = 1,
  /** Clone output documents to ensure distinct objects without shared references. */
  CLONE_OUTPUT = 2,
  /** Clone input and output documents. */
  CLONE_ALL = CLONE_INPUT | CLONE_OUTPUT
}

/**
 * Generic options interface passed down to all operators
 */
export interface Options {
  /** The key that is used to lookup the ID value of a document. @default "_id". */
  readonly idKey: string;
  /** The collation specification for string sorting operations. */
  readonly collation?: CollationSpec;
  /** Determines how to treat inputs and outputs. @default ProcessingMode.CLONE_OFF. */
  readonly processingMode: ProcessingMode;
  /** Enforces strict MongoDB compatibilty. See README. @default true. */
  readonly useStrictMode: boolean;
  /** Enable or disable custom script execution using `$where`, `$accumulator`, and `$function` operators. @default true. */
  readonly scriptEnabled: boolean;
  /** Enable or disable falling back to the global context for operators. @default true. */
  readonly useGlobalContext: boolean;
  /** Hash function to replace the Effective Java default implementation. */
  readonly hashFunction?: HashFunction;
  /** Function to resolve strings to arrays for use with operators that reference other collections such as; `$lookup`, `$out` and `$merge`. */
  readonly collectionResolver?: CollectionResolver;
  /** JSON schema validator to use with the '$jsonSchema' operator. Required in order to use the operator. */
  readonly jsonSchemaValidator?: JsonSchemaValidator;
  /** Global variables. */
  readonly variables?: Readonly<AnyObject>;
  /** Extra references to operators to be used for processing. */
  readonly context: Context;
}

interface LocalData {
  /** The groupId computed for a group of documents. */
  readonly groupId?: Any;
  /** Local user-defind variables. */
  readonly variables?: AnyObject;
}

/** Custom type to facilitate type checking for global options */
export class ComputeOptions implements Options {
  #options: Options;
  /** Reference to the root object when processing subgraphs of the object. */
  #root: Any;
  #local: LocalData;

  private constructor(options: Options, root: Any, local?: LocalData) {
    this.#options = options;
    this.update(root, local);
  }

  /**
   * Initialize new ComputeOptions.
   * @returns {ComputeOptions}
   */
  static init(options: Options, root?: Any, local?: LocalData): ComputeOptions {
    return !(options instanceof ComputeOptions)
      ? new ComputeOptions(options, root, local)
      : new ComputeOptions(options.#options, options.root ?? root, {
          ...options.#local,
          ...local,
          variables: Object.assign(
            {},
            options.#local?.variables,
            local?.variables
          )
        });
  }

  /**
   * Updates the internal state.
   *
   * @param root The new root context for this object.
   * @param local The new local state to merge into current if it exists.
   * @returns
   */
  update(root?: Any, local?: LocalData): ComputeOptions {
    // NOTE: this is done for efficiency to avoid creating too many intermediate options objects.
    this.#root = root;
    // retain existing variables
    const variables = Object.assign(
      {},
      this.#local?.variables,
      local?.variables
    );
    if (Object.keys(variables).length) {
      this.#local = { ...local, variables };
    } else {
      this.#local = local ?? {};
    }
    return this;
  }

  getOptions() {
    return Object.freeze({
      ...this.#options,
      context: Context.from(this.#options.context)
    }) as Options;
  }

  get root() {
    return this.#root;
  }
  get local() {
    return this.#local;
  }
  get idKey() {
    return this.#options.idKey;
  }
  get collation() {
    return this.#options?.collation;
  }
  get processingMode() {
    return this.#options?.processingMode || ProcessingMode.CLONE_OFF;
  }
  get useStrictMode() {
    return this.#options?.useStrictMode;
  }
  get scriptEnabled() {
    return this.#options?.scriptEnabled;
  }
  get useGlobalContext() {
    return this.#options?.useGlobalContext;
  }
  get hashFunction() {
    return this.#options?.hashFunction;
  }
  get collectionResolver() {
    return this.#options?.collectionResolver;
  }
  get jsonSchemaValidator() {
    return this.#options?.jsonSchemaValidator;
  }
  get variables() {
    return this.#options?.variables;
  }
  get context() {
    return this.#options?.context;
  }
}

/**
 * Creates an Option from another where required keys are initialized.
 * @param options Options
 */
export function initOptions(options?: Partial<Options>): Options {
  return options instanceof ComputeOptions
    ? options.getOptions()
    : Object.freeze({
        idKey: "_id",
        scriptEnabled: true,
        useStrictMode: true,
        useGlobalContext: true,
        processingMode: ProcessingMode.CLONE_OFF,
        ...options,
        context: options?.context
          ? Context.from(options?.context)
          : Context.init()
      });
}

/**
 * Supported cloning modes.
 * - "deep": Performs a recursive deep clone of the object.
 * - "copy": Performs a shallow copy of the object.
 * - "none": No cloning. Uses the value as given.
 */
export type CloneMode = "deep" | "copy" | "none";

export interface UpdateOptions {
  /** Specifies whether to deep clone values to persist in the internal store. @default "copy". */
  readonly cloneMode?: CloneMode;
  /** Options to use for processing queries. Unless overriden 'useStrictMode' is false.  */
  readonly queryOptions?: Partial<Options>;
}

/**
 * The different groups of operators
 */
export enum OperatorType {
  ACCUMULATOR = "accumulator",
  EXPRESSION = "expression",
  PIPELINE = "pipeline",
  PROJECTION = "projection",
  QUERY = "query",
  WINDOW = "window"
}

export type AccumulatorOperator<R = Any> = (
  collection: Any[],
  expr: Any,
  options: Options
) => R;

export type ExpressionOperator<R = Any> = (
  obj: AnyObject,
  expr: Any,
  options: Options
) => R;

export type PipelineOperator = (
  collection: Iterator,
  expr: Any,
  options: Options
) => Iterator;

export type ProjectionOperator = (
  obj: AnyObject,
  expr: Any,
  selector: string,
  options: Options
) => Any;

export type QueryOperator = (
  selector: string,
  value: Any,
  options: Options
) => (obj: AnyObject) => boolean;

export type WindowOperator = (
  obj: AnyObject,
  array: AnyObject[],
  expr: WindowOperatorInput,
  options: Options
) => Any;

/** Interface for update operators */
export type UpdateOperator = (
  obj: AnyObject,
  expr: AnyObject,
  arrayFilters: AnyObject[],
  options: UpdateOptions
) => string[];

type Operator =
  | AccumulatorOperator
  | ExpressionOperator
  | PipelineOperator
  | ProjectionOperator
  | QueryOperator
  | WindowOperator;

type AccumulatorOps = Record<string, AccumulatorOperator>;
type ExpressionOps = Record<string, ExpressionOperator>;
type ProjectionOps = Record<string, ProjectionOperator>;
type QueryOps = Record<string, QueryOperator>;
type PipelineOps = Record<string, PipelineOperator>;
type WindowOps = Record<string, WindowOperator>;

/** Kinds of operators that can be registered. */
export type OpType =
  | "accumulator"
  | "expression"
  | "pipeline"
  | "projection"
  | "query"
  | "window";

export class Context {
  #operators = new Map<OpType, Record<string, Operator>>();

  private constructor() {}

  static init(): Context {
    return new Context();
  }

  static from(ctx?: Context): Context {
    const instance = Context.init();
    if (isNil(ctx)) return instance;
    ctx.#operators.forEach((v, k) => instance.addOperators(k, v));
    return instance;
  }

  private addOperators(
    type: OpType,
    operators: Record<string, Operator>
  ): Context {
    if (!this.#operators.has(type)) this.#operators.set(type, {});
    for (const [name, fn] of Object.entries(operators)) {
      if (!this.getOperator(type, name)) {
        this.#operators.get(type)[name] = fn;
      }
    }
    return this;
  }

  getOperator(type: OpType, name: string): Callback | null {
    const ops = this.#operators.get(type) ?? {};
    return ops[name] ?? null;
  }

  addAccumulatorOps(ops: AccumulatorOps) {
    return this.addOperators("accumulator", ops);
  }

  addExpressionOps(ops: ExpressionOps) {
    return this.addOperators("expression", ops);
  }

  addQueryOps(ops: QueryOps) {
    return this.addOperators("query", ops);
  }

  addPipelineOps(ops: PipelineOps) {
    return this.addOperators("pipeline", ops);
  }

  addProjectionOps(ops: ProjectionOps) {
    return this.addOperators("projection", ops);
  }

  addWindowOps(ops: WindowOps) {
    return this.addOperators("window", ops);
  }
}

// global context
const GLOBAL_CONTEXT = Context.init();

/**
 * Register global operators that are available when {@link Options.useGlobalContext} is enabled.
 *
 * @param type Operator type
 * @param operators Map of operator name to functions
 */
export function useOperators(
  type: OpType,
  operators: Record<string, Operator>
): void {
  for (const [name, fn] of Object.entries(operators)) {
    assert(
      isFunction(fn) && isOperator(name),
      `'${name}' is not a valid operator`
    );
    const currentFn = getOperator(type, name, null);
    assert(
      !currentFn || fn === currentFn,
      `${name} already exists for '${type}' operators. Cannot change operator function once registered.`
    );
  }
  // toss the operator salad :)
  switch (type) {
    case "accumulator":
      GLOBAL_CONTEXT.addAccumulatorOps(operators as AccumulatorOps);
      break;
    case "expression":
      GLOBAL_CONTEXT.addExpressionOps(operators as ExpressionOps);
      break;
    case "pipeline":
      GLOBAL_CONTEXT.addPipelineOps(operators as PipelineOps);
      break;
    case "projection":
      GLOBAL_CONTEXT.addProjectionOps(operators as ProjectionOps);
      break;
    case "query":
      GLOBAL_CONTEXT.addQueryOps(operators as QueryOps);
      break;
    case "window":
      GLOBAL_CONTEXT.addWindowOps(operators as WindowOps);
      break;
  }
}

/**
 * Returns the operator function or undefined if it is not found
 * @param type Type of operator
 * @param name Name of the operator
 * @param options
 */
export function getOperator(
  type: OpType,
  name: string,
  options: Pick<Options, "useGlobalContext" | "context">
): Operator {
  const { context: ctx, useGlobalContext: fallback } = options || {};
  const fn = ctx ? (ctx.getOperator(type, name) as Operator) : null;
  return !fn && fallback ? GLOBAL_CONTEXT.getOperator(type, name) : fn;
}

/**
 * Computes the value of the expression on the object for the given operator
 *
 * @param obj the current object from the collection
 * @param expr the expression for the given field
 * @param operator the operator to resolve the field with
 * @param options {Object} extra options
 * @returns {*}
 */
export function computeValue(
  obj: Any,
  expr: Any,
  operator: string | null,
  options?: Options
): Any {
  const copts = ComputeOptions.init(options, obj);
  // ensure valid options exist on first invocation
  return !!operator && isOperator(operator)
    ? computeOperator(obj, expr, operator, copts)
    : computeExpression(obj, expr, copts);
}

const SYSTEM_VARS = ["$$ROOT", "$$CURRENT", "$$REMOVE", "$$NOW"] as const;
type SystemVar = (typeof SYSTEM_VARS)[number];

/** Computes the value of the expr given for the object. */
function computeExpression(obj: Any, expr: Any, options: ComputeOptions): Any {
  // if expr is a string and begins with "$$", then we have a variable.
  //  this can be one of; redact variable, system variable, user-defined variable.
  //  we check and process them in that order.
  //
  // if expr begins only a single "$", then it is a path to a field on the object.
  if (isString(expr) && expr.length > 0 && expr[0] === "$") {
    // we return redact variables as literals
    if (REDACT_ACTIONS.includes(expr as RedactAction)) return expr;

    // default to root for resolving path.
    let ctx = options.root;

    // handle selectors with explicit prefix
    const arr = expr.split(".");
    if (SYSTEM_VARS.includes(arr[0] as SystemVar)) {
      // set 'root' only the first time it is required to be used for all subsequent calls
      // if it already available on the options, it will be used
      switch (arr[0] as SystemVar) {
        case "$$ROOT":
          break;
        case "$$CURRENT":
          ctx = obj;
          break;
        case "$$REMOVE":
          ctx = undefined;
          break;
        case "$$NOW":
          ctx = new Date();
          break;
      }
      expr = expr.slice(arr[0].length + 1); //  +1 for '.'
    } else if (arr[0].slice(0, 2) === "$$") {
      // handle user-defined variables
      ctx = Object.assign(
        {},
        // global vars
        options.variables,
        // current item is added before local variables because the binding may be changed.
        { this: obj },
        // local vars
        options?.local?.variables
      );
      // the variable name
      const name = arr[0].slice(2);
      assert(has(ctx as AnyObject, name), `Use of undefined variable: ${name}`);
      expr = expr.slice(2);
    } else {
      // 'expr' is a path to a field on the object.
      expr = expr.slice(1);
    }

    return expr === "" ? ctx : resolve(ctx as ArrayOrObject, expr as string);
  }

  // check and return value if already in a resolved state
  if (isArray(expr)) {
    return expr.map(item => computeExpression(obj, item, options));
  }

  if (isObject(expr)) {
    const result: AnyObject = {};
    const elems = Object.entries(expr as AnyObject);
    for (const [key, val] of elems) {
      // if object represents an operator expression, there should only be a single key
      if (isOperator(key)) {
        assert(elems.length == 1, "expression must have single operator.");
        return computeOperator(obj, val, key, options);
      }
      result[key] = computeExpression(obj, val, options);
    }
    return result;
  }

  return expr;
}

function computeOperator(
  obj: Any,
  expr: Any,
  operator: string,
  options: ComputeOptions
): Any {
  // if the field of the object is a valid operator
  const callExpression = getOperator(
    "expression",
    operator,
    options
  ) as ExpressionOperator;
  if (callExpression) return callExpression(obj as AnyObject, expr, options);

  // handle accumulators
  const callAccumulator = getOperator(
    "accumulator",
    operator,
    options
  ) as AccumulatorOperator;

  // operator was not found
  assert(!!callAccumulator, `accumulator '${operator}' is not registered.`);

  // if object is not an array, attempt to resolve to array.
  if (!isArray(obj)) {
    obj = computeExpression(obj, expr, options);
    expr = null;
  }

  assert(isArray(obj), `arguments must resolve to array for ${operator}.`);

  // accumulator must override the root accordingly. we pass the full context as is.
  return callAccumulator(obj as Any[], expr, options);
}

const REDACT_ACTIONS = ["$$KEEP", "$$PRUNE", "$$DESCEND"] as const;
type RedactAction = (typeof REDACT_ACTIONS)[number];

/**
 * Redact an object
 * @param  {Object} obj The object to redact
 * @param  {*} expr The redact expression
 * @param  {*} options  Options for value
 * @return {*} returns the result of the redacted object
 */
export function redact(
  obj: AnyObject,
  expr: Any,
  options: ComputeOptions
): Any {
  const action = computeValue(obj, expr, null, options) as RedactAction;
  switch (action) {
    case "$$KEEP":
      return obj;
    case "$$PRUNE":
      return undefined;
    case "$$DESCEND": {
      // traverse nested documents iff there is a $cond
      if (!has(expr as AnyObject, "$cond")) return obj;

      const output = {};

      for (const [key, value] of Object.entries(obj)) {
        if (isArray(value)) {
          const res = new Array<Any>();
          for (let elem of value) {
            if (isObject(elem)) {
              elem = redact(elem as AnyObject, expr, options.update(elem));
            }
            if (!isNil(elem)) res.push(elem);
          }
          output[key] = res;
        } else if (isObject(value)) {
          const res = redact(
            value as AnyObject,
            expr,
            options.update(value)
          ) as ArrayOrObject;
          if (!isNil(res)) output[key] = res;
        } else {
          output[key] = value;
        }
      }
      return output;
    }
    default:
      return action;
  }
}
