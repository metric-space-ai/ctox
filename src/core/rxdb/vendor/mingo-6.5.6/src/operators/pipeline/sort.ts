import { CollationSpec, Options, PipelineOperator } from "../../core";
import { Iterator } from "../../lazy";
import { Any, AnyObject, Comparator } from "../../types";
import {
  assert,
  compare,
  groupBy,
  isEmpty,
  isObject,
  isString,
  resolve
} from "../../util";

/**
 * Sorts all input documents and returns them to the pipeline in sorted order.
 *
 * See {@link https://www.mongodb.com/docs/manual/reference/operator/aggregation/sort/ usage}.
 *
 * @param collection
 * @param sortKeys
 * @param options
 * @returns
 */
export const $sort: PipelineOperator = (
  collection: Iterator,
  sortKeys: Record<string, 1 | -1>,
  options: Options
): Iterator => {
  if (isEmpty(sortKeys) || !isObject(sortKeys)) return collection;

  let cmp = compare;
  // check for collation spec on the options
  const collationSpec = options.collation;

  // use collation comparator if provided
  if (isObject(collationSpec) && isString(collationSpec.locale)) {
    cmp = collationComparator(collationSpec);
  }

  return collection.transform((coll: Any[]) => {
    const modifiers = Object.keys(sortKeys);
    for (const key of modifiers.reverse()) {
      const groups = groupBy(
        coll,
        (obj: AnyObject) => resolve(obj, key),
        options.hashFunction
      );
      const sortedKeys = Array.from(groups.keys()).sort(cmp);
      if (sortKeys[key] === -1) sortedKeys.reverse();

      // modify collection in place.
      let i = 0;
      for (const k of sortedKeys) for (const v of groups.get(k)) coll[i++] = v;
      assert(i == coll.length, "bug: counter must match collection size.");
    }
    return coll;
  });
};

// MongoDB collation strength to JS localeCompare sensitivity mapping.
// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/localeCompare
const COLLATION_STRENGTH: Record<number, "base" | "accent" | "variant"> = {
  // Only strings that differ in base letters compare as unequal. Examples: a ≠ b, a = á, a = A.
  1: "base",
  //  Only strings that differ in base letters or accents and other diacritic marks compare as unequal.
  // Examples: a ≠ b, a ≠ á, a = A.
  2: "accent",
  // Strings that differ in base letters, accents and other diacritic marks, or case compare as unequal.
  // Other differences may also be taken into consideration. Examples: a ≠ b, a ≠ á, a ≠ A
  3: "variant"
  // case - Only strings that differ in base letters or case compare as unequal. Examples: a ≠ b, a = á, a ≠ A.
};

/**
 * Creates a comparator function for the given collation spec. See https://docs.mongodb.com/manual/reference/collation/
 *
 * @param spec {AnyObject} The MongoDB collation spec.
 * {
 *   locale: string,
 *   caseLevel: boolean,
 *   caseFirst: string,
 *   strength: int,
 *   numericOrdering: boolean,
 *   alternate: string,
 *   maxVariable: never, // unsupported
 *   backwards: never // unsupported
 * }
 */
function collationComparator(spec: CollationSpec): Comparator<Any> {
  const localeOpt: Intl.CollatorOptions = {
    sensitivity: COLLATION_STRENGTH[spec.strength || 3],
    caseFirst: spec.caseFirst === "off" ? "false" : spec.caseFirst || "false",
    numeric: spec.numericOrdering || false,
    ignorePunctuation: spec.alternate === "shifted"
  };

  // when caseLevel is true for strength  1:base and 2:accent, bump sensitivity to the nearest that supports case comparison
  if ((spec.caseLevel || false) === true) {
    if (localeOpt.sensitivity === "base") localeOpt.sensitivity = "case";
    if (localeOpt.sensitivity === "accent") localeOpt.sensitivity = "variant";
  }

  const collator = new Intl.Collator(spec.locale, localeOpt);

  return (a: Any, b: Any) => {
    // non strings
    if (!isString(a) || !isString(b)) return compare(a, b);

    // only for strings
    const i = collator.compare(a, b);
    if (i < 0) return -1;
    if (i > 0) return 1;
    return 0;
  };
}
