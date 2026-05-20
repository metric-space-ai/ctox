export type Any = unknown;
export type AnyObject = Record<string, Any>;
export type ArrayOrObject = AnyObject | Any[];

/** @deprecated use {@link Any}. */
export type AnyVal = Any;
/** @deprecated use {@link AnyObject}. */
export type RawObject = AnyObject;
/** @deprecated use {@link Array<Any>}. */
export type RawArray = Array<Any>;

// Generic callback
export interface Callback<R = Any, T = Any> {
  (...args: T[]): R;
}

// Generic predicate
export interface Predicate<T = Any> {
  (...args: T[]): boolean;
}

// Generic comparator callback
export interface Comparator<T = Any> {
  (left: T, right: T): number;
}

/**
 * Custom function to hash values to improve faster comparaisons
 */
export type HashFunction = (x: Any) => number;

type CommonTypes =
  | "null"
  | "undefined"
  | "string"
  | "date"
  | "array"
  | "object";

// Javascript native types
export type JsType =
  | CommonTypes
  | "boolean"
  | "number"
  | "string"
  | "regexp"
  | "function";

// MongoDB BSON types
export type BsonType =
  | CommonTypes
  | "bool"
  | "int"
  | "long"
  | "double"
  | "decimal"
  | "regex";

export const TIME_UNITS = [
  "year",
  "quarter",
  "month",
  "week",
  "day",
  "hour",
  "minute",
  "second",
  "millisecond"
] as const;

/** Time unit for datetime periods */
export type TimeUnit = (typeof TIME_UNITS)[number];

// Window operator types.
export type Boundary = "current" | "unbounded" | number;

export interface WindowOutputOption {
  readonly documents?: [Boundary, Boundary];
  readonly range?: [Boundary, Boundary];
  readonly unit?: TimeUnit;
}

export interface SetWindowFieldsInput {
  readonly partitionBy?: Any;
  readonly sortBy: Record<string, 1 | -1>;
  readonly output: Record<
    string,
    {
      [x: string]: Any;
      window?: WindowOutputOption;
    }
  >;
}

export interface WindowOperatorInput {
  readonly parentExpr: SetWindowFieldsInput;
  readonly inputExpr: Any;
  readonly documentNumber: number;
  readonly field: string;
}
