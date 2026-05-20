import { Any, AnyObject, Callback, Predicate } from "./types";
import { isArray, MingoError } from "./util";

interface Iteratee {
  action: Action;
  func?: Callback<Any>;
  count?: number;
}

/**
 * A value produced by a generator
 */
export interface IteratorResult<T = Any> {
  readonly value?: T;
  readonly done: boolean;
}

/**
 * Simplified generator interface
 */
interface Generator<T> {
  next: () => T;
}

export type Source =
  | Generator<IteratorResult>
  | Callback<IteratorResult>
  | Any[];

/**
 * Returns an iterator
 * @param {*} source An iterable source (Array, Function, Generator, or Iterator)
 */
export function Lazy(source: Source): Iterator {
  return source instanceof Iterator ? source : new Iterator(source);
}

/**
 * Concatenate multiple iterators and return a new iterator.
 *
 * @param iterators The iterators to concatenate
 * @returns {Iterator} A new iterator
 */
export function concat(...iterators: Iterator[]): Iterator {
  let index = 0;
  return Lazy(() => {
    while (index < iterators.length) {
      const o = iterators[index].next();
      if (!o.done) return o;
      index++;
    }
    return { done: true };
  });
}

/**
 * Checks whether the given object is compatible with a generator i.e Object{next:Function}
 * @param {*} o An object
 */
function isGenerator(o: Any) {
  return (
    !!o && typeof o === "object" && (o as AnyObject)?.next instanceof Function
  );
}

function dropItem(array: Any[], i: number) {
  const rest = array.slice(i + 1);
  array.splice(i);
  Array.prototype.push.apply(array, rest);
}

// stop iteration error
const DONE = new Error();

// Lazy function actions
enum Action {
  MAP,
  FILTER,
  TAKE,
  DROP
}

function createCallback(
  nextFn: Callback,
  iteratees: Iteratee[],
  buffer: Any[]
): Callback<IteratorResult, boolean> {
  let done = false;
  let index = -1;
  let bufferIndex = 0; // index for the buffer

  return function (storeResult?: boolean): IteratorResult {
    // special hack to collect all values into buffer
    try {
      outer: while (!done) {
        let o = nextFn();
        index++;

        let i = -1;
        const size = iteratees.length;
        let innerDone = false;

        while (++i < size) {
          const r = iteratees[i];

          switch (r.action) {
            case Action.MAP:
              o = r.func(o, index);
              break;
            case Action.FILTER:
              if (!r.func(o, index)) continue outer;
              break;
            case Action.TAKE:
              --r.count;
              if (!r.count) innerDone = true;
              break;
            case Action.DROP:
              --r.count;
              if (!r.count) dropItem(iteratees, i);
              continue outer;
            default:
              break outer;
          }
        }

        done = innerDone;

        if (storeResult) {
          buffer[bufferIndex++] = o;
        } else {
          return { value: o, done: false };
        }
      }
    } catch (e) {
      if (e !== DONE) throw e;
    }

    done = true;
    return { done };
  };
}

/**
 * A lazy collection iterator yields a single value at a time upon request.
 */
export class Iterator {
  #iteratees: Iteratee[] = [];
  #yieldedValues: Any[] = [];
  #getNext: Callback<IteratorResult, boolean>;

  private isDone = false;

  /**
   * @param {*} source An iterable object or function.
   *    Array - return one element per cycle
   *    Object{next:Function} - call next() for the next value (this also handles generator functions)
   *    Function - call to return the next value
   * @param {Function} fn An optional transformation function
   */
  constructor(source: Source) {
    let nextVal: Callback<Any>;

    if (source instanceof Function) {
      // make iterable
      source = { next: source };
    }

    if (isGenerator(source)) {
      const src = source as Generator<Any>;
      nextVal = () => {
        const o = src.next() as AnyObject;
        if (o.done) throw DONE;
        return o.value;
      };
    } else if (isArray(source)) {
      const data = source;
      const size = data.length;
      let index = 0;
      nextVal = () => {
        if (index < size) return data[index++];
        throw DONE;
      };
    } else if (!(source instanceof Function)) {
      throw new MingoError(
        `Lazy must be initialized with an array, generator, or function.`
      );
    }

    // create next function
    this.#getNext = createCallback(
      nextVal,
      this.#iteratees,
      this.#yieldedValues
    );
  }

  /**
   * Add an iteratee to this lazy sequence
   */
  private push(action: Action, value: Callback<Any> | number) {
    if (typeof value === "function") {
      this.#iteratees.push({ action, func: value });
    } else if (typeof value === "number") {
      this.#iteratees.push({ action, count: value });
    }
    return this;
  }

  next<T = Any>(): IteratorResult<T> {
    return this.#getNext() as IteratorResult<T>;
  }

  // Iteratees methods

  /**
   * Transform each item in the sequence to a new value
   * @param {Function} f
   */
  map<T = Any>(f: Callback<T>): Iterator {
    return this.push(Action.MAP, f);
  }

  /**
   * Select only items matching the given predicate
   * @param {Function} pred
   */
  filter<T = Any>(predicate: Predicate<T>): Iterator {
    return this.push(Action.FILTER, predicate as Callback<T>);
  }

  /**
   * Take given numbe for values from sequence
   * @param {Number} n A number greater than 0
   */
  take(n: number): Iterator {
    return n > 0 ? this.push(Action.TAKE, n) : this;
  }

  /**
   * Drop a number of values from the sequence
   * @param {Number} n Number of items to drop greater than 0
   */
  drop(n: number): Iterator {
    return n > 0 ? this.push(Action.DROP, n) : this;
  }

  // Transformations

  /**
   * Returns a new lazy object with results of the transformation
   * The entire sequence is realized.
   *
   * @param {Callback<Source, Any[]>} fn Tranform function of type (Array) => (Any)
   */
  transform(fn: Callback<Source, Any[]>): Iterator {
    const self = this;
    let iter: Iterator;
    return Lazy(() => {
      if (!iter) {
        iter = Lazy(fn(self.value()));
      }
      return iter.next();
    });
  }

  // Terminal methods

  /**
   * Returns the fully realized values of the iterators.
   * The return value will be an array unless `lazy.first()` was used.
   * The realized values are cached for subsequent calls.
   */
  value<T>(): T[] {
    if (!this.isDone) {
      this.isDone = this.#getNext(true).done;
    }
    return this.#yieldedValues as T[];
  }

  /**
   * Execute the funcion for each value. Will stop when an execution returns false.
   * @param {Function} f
   * @returns {Boolean} false iff `f` return false for AnyVal execution, otherwise true
   */
  each<T = Any>(f: Callback<T>): boolean {
    for (;;) {
      const o = this.next();
      if (o.done) break;
      if ((f(o.value) as Any) === false) return false;
    }
    return true;
  }

  /**
   * Returns the reduction of sequence according the reducing function
   *
   * @param {*} f a reducing function
   * @param {*} initialValue
   */
  reduce<T = Any>(f: Callback<T>, initialValue?: Any): T {
    let o = this.next();

    if (initialValue === undefined && !o.done) {
      initialValue = o.value as T;
      o = this.next();
    }

    while (!o.done) {
      initialValue = f(initialValue, o.value as T);
      o = this.next();
    }

    return initialValue as T;
  }

  /**
   * Returns the number of matched items in the sequence
   */
  size(): number {
    return this.reduce(
      ((acc: number, _: number) => ++acc) as Callback<number>,
      0
    );
  }

  [Symbol.iterator](): Iterator {
    return this;
  }
}
