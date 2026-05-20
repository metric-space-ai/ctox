// Query Logical Operators: https://docs.mongodb.com/manual/reference/operator/query-logical/

import { Options, QueryOperator } from "../../../core";
import { Any, AnyObject, Callback } from "../../../types";
import { assert, isArray } from "../../../util";
import { $or } from "./or";

/**
 * Joins query clauses with a logical NOR returns all documents that fail to match both clauses.
 *
 * @param selector
 * @param rhs
 * @returns {Function}
 */
export const $nor: QueryOperator = (
  _: string,
  rhs: AnyObject[],
  options: Options
): Callback<boolean> => {
  assert(
    isArray(rhs),
    "Invalid expression. $nor expects value to be an array."
  );
  const f: Callback<boolean> = $or("$or", rhs, options);
  return (obj: Any) => !f(obj);
};
