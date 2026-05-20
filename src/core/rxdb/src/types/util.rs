//! Generic helper type aliases.

// ref: rxdb/src/types/util.d.ts (DeepReadonly, MaybeReadonly, StringKeys)
//
// In Rust, values are immutable by default and there is no `Readonly<T>`
// type-level marker, so `DeepReadonly` and `MaybeReadonly` are simple aliases.
// `StringKeys` in upstream is a TS conditional type that extracts string
// property names of an object; on the Rust side document keys are always
// `String`s already.

pub type DeepReadonly<T> = T;
pub type MaybeReadonly<T> = T;
pub type StringKeys = String;
