import { getOperator, initOptions, Options, QueryOperator } from "./core";
import { Cursor } from "./cursor";
import { Source } from "./lazy";
import { Any, AnyObject, Predicate } from "./types";
import { assert, cloneDeep, isObject, isOperator, normalize } from "./util";

const TOP_LEVEL_OPS = new Set(
  Array.from(["$and", "$or", "$nor", "$expr", "$jsonSchema"])
);

/**
 * An object used to filter input documents
 *
 * @param {AnyObject} condition The condition for constructing predicates
 * @param {Options} options Options for use by operators
 * @constructor
 */
export class Query {
  #compiled: Predicate<Any>[];
  #options: Options;
  #condition: AnyObject;

  constructor(condition: AnyObject, options?: Partial<Options>) {
    this.#condition = cloneDeep(condition);
    this.#options = initOptions(options);
    this.#compiled = [];
    this.compile();
  }

  private compile(): void {
    assert(
      isObject(this.#condition),
      `query criteria must be an object: ${JSON.stringify(this.#condition)}`
    );

    const whereOperator: { field?: string; expr?: Any } = {};

    for (const [field, expr] of Object.entries(this.#condition)) {
      if ("$where" === field) {
        assert(
          this.#options.scriptEnabled,
          "$where operator requires 'scriptEnabled' option to be true."
        );
        Object.assign(whereOperator, { field: field, expr: expr });
      } else if (TOP_LEVEL_OPS.has(field)) {
        this.processOperator(field, field, expr);
      } else {
        // normalize expression
        assert(!isOperator(field), `unknown top level operator: ${field}`);
        for (const [operator, val] of Object.entries(
          normalize(expr) as AnyObject
        )) {
          this.processOperator(field, operator, val);
        }
      }

      if (whereOperator.field) {
        this.processOperator(
          whereOperator.field,
          whereOperator.field,
          whereOperator.expr
        );
      }
    }
  }

  private processOperator(field: string, operator: string, value: Any): void {
    const call = getOperator("query", operator, this.#options) as QueryOperator;
    assert(!!call, `unknown query operator ${operator}`);
    this.#compiled.push(call(field, value, this.#options));
  }

  /**
   * Checks if the object passes the query criteria. Returns true if so, false otherwise.
   *
   * @param obj The object to test
   * @returns {boolean}
   */
  test<T>(obj: T): boolean {
    return this.#compiled.every(p => p(obj));
  }

  /**
   * Returns a cursor to select matching documents from the input source.
   *
   * @param source A source providing a sequence of documents
   * @param projection An optional projection criteria
   * @returns {Cursor} A Cursor for iterating over the results
   */
  find<T>(collection: Source, projection?: AnyObject): Cursor<T> {
    return new Cursor<T>(
      collection,
      o => this.test(o),
      projection || {},
      this.#options
    );
  }

  /**
   * Remove matched documents from the collection returning the remainder
   *
   * @param collection An array of documents
   * @returns {Array} A new array with matching elements removed
   */
  remove<T>(collection: T[]): T[] {
    return collection.reduce<T[]>((acc: T[], obj: T) => {
      if (!this.test(obj)) acc.push(obj);
      return acc;
    }, []);
  }
}
