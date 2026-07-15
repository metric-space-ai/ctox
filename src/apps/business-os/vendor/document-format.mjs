var __require = /* @__PURE__ */ ((x) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(x, {
  get: (a, b) => (typeof require !== "undefined" ? require : a)[b]
}) : x)(function(x) {
  if (typeof require !== "undefined") return require.apply(this, arguments);
  throw Error('Dynamic require of "' + x + '" is not supported');
});

// archive/reorg-review/templates/business-basic/apps/web/vendor/jszip.mjs
var __getOwnPropNames = Object.getOwnPropertyNames;
var __require2 = /* @__PURE__ */ ((x) => typeof __require !== "undefined" ? __require : typeof Proxy !== "undefined" ? new Proxy(x, {
  get: (a, b) => (typeof __require !== "undefined" ? __require : a)[b]
}) : x)(function(x) {
  if (typeof __require !== "undefined") return __require.apply(this, arguments);
  throw Error('Dynamic require of "' + x + '" is not supported');
});
var __commonJS = (cb, mod) => function __require22() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
var require_process_nextick_args = __commonJS({
  "../../node_modules/.pnpm/process-nextick-args@2.0.1/node_modules/process-nextick-args/index.js"(exports, module) {
    "use strict";
    if (typeof process === "undefined" || !process.version || process.version.indexOf("v0.") === 0 || process.version.indexOf("v1.") === 0 && process.version.indexOf("v1.8.") !== 0) {
      module.exports = { nextTick };
    } else {
      module.exports = process;
    }
    function nextTick(fn, arg1, arg2, arg3) {
      if (typeof fn !== "function") {
        throw new TypeError('"callback" argument must be a function');
      }
      var len = arguments.length;
      var args, i;
      switch (len) {
        case 0:
        case 1:
          return process.nextTick(fn);
        case 2:
          return process.nextTick(function afterTickOne() {
            fn.call(null, arg1);
          });
        case 3:
          return process.nextTick(function afterTickTwo() {
            fn.call(null, arg1, arg2);
          });
        case 4:
          return process.nextTick(function afterTickThree() {
            fn.call(null, arg1, arg2, arg3);
          });
        default:
          args = new Array(len - 1);
          i = 0;
          while (i < args.length) {
            args[i++] = arguments[i];
          }
          return process.nextTick(function afterTick() {
            fn.apply(null, args);
          });
      }
    }
  }
});
var require_isarray = __commonJS({
  "../../node_modules/.pnpm/isarray@1.0.0/node_modules/isarray/index.js"(exports, module) {
    var toString = {}.toString;
    module.exports = Array.isArray || function(arr) {
      return toString.call(arr) == "[object Array]";
    };
  }
});
var require_stream = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/internal/streams/stream.js"(exports, module) {
    module.exports = __require2("stream");
  }
});
var require_safe_buffer = __commonJS({
  "../../node_modules/.pnpm/safe-buffer@5.1.2/node_modules/safe-buffer/index.js"(exports, module) {
    var buffer = __require2("buffer");
    var Buffer2 = buffer.Buffer;
    function copyProps(src, dst) {
      for (var key in src) {
        dst[key] = src[key];
      }
    }
    if (Buffer2.from && Buffer2.alloc && Buffer2.allocUnsafe && Buffer2.allocUnsafeSlow) {
      module.exports = buffer;
    } else {
      copyProps(buffer, exports);
      exports.Buffer = SafeBuffer;
    }
    function SafeBuffer(arg, encodingOrOffset, length) {
      return Buffer2(arg, encodingOrOffset, length);
    }
    copyProps(Buffer2, SafeBuffer);
    SafeBuffer.from = function(arg, encodingOrOffset, length) {
      if (typeof arg === "number") {
        throw new TypeError("Argument must not be a number");
      }
      return Buffer2(arg, encodingOrOffset, length);
    };
    SafeBuffer.alloc = function(size, fill, encoding) {
      if (typeof size !== "number") {
        throw new TypeError("Argument must be a number");
      }
      var buf = Buffer2(size);
      if (fill !== void 0) {
        if (typeof encoding === "string") {
          buf.fill(fill, encoding);
        } else {
          buf.fill(fill);
        }
      } else {
        buf.fill(0);
      }
      return buf;
    };
    SafeBuffer.allocUnsafe = function(size) {
      if (typeof size !== "number") {
        throw new TypeError("Argument must be a number");
      }
      return Buffer2(size);
    };
    SafeBuffer.allocUnsafeSlow = function(size) {
      if (typeof size !== "number") {
        throw new TypeError("Argument must be a number");
      }
      return buffer.SlowBuffer(size);
    };
  }
});
var require_util = __commonJS({
  "../../node_modules/.pnpm/core-util-is@1.0.3/node_modules/core-util-is/lib/util.js"(exports) {
    function isArray(arg) {
      if (Array.isArray) {
        return Array.isArray(arg);
      }
      return objectToString(arg) === "[object Array]";
    }
    exports.isArray = isArray;
    function isBoolean(arg) {
      return typeof arg === "boolean";
    }
    exports.isBoolean = isBoolean;
    function isNull(arg) {
      return arg === null;
    }
    exports.isNull = isNull;
    function isNullOrUndefined(arg) {
      return arg == null;
    }
    exports.isNullOrUndefined = isNullOrUndefined;
    function isNumber(arg) {
      return typeof arg === "number";
    }
    exports.isNumber = isNumber;
    function isString(arg) {
      return typeof arg === "string";
    }
    exports.isString = isString;
    function isSymbol(arg) {
      return typeof arg === "symbol";
    }
    exports.isSymbol = isSymbol;
    function isUndefined(arg) {
      return arg === void 0;
    }
    exports.isUndefined = isUndefined;
    function isRegExp(re) {
      return objectToString(re) === "[object RegExp]";
    }
    exports.isRegExp = isRegExp;
    function isObject(arg) {
      return typeof arg === "object" && arg !== null;
    }
    exports.isObject = isObject;
    function isDate(d) {
      return objectToString(d) === "[object Date]";
    }
    exports.isDate = isDate;
    function isError(e) {
      return objectToString(e) === "[object Error]" || e instanceof Error;
    }
    exports.isError = isError;
    function isFunction(arg) {
      return typeof arg === "function";
    }
    exports.isFunction = isFunction;
    function isPrimitive(arg) {
      return arg === null || typeof arg === "boolean" || typeof arg === "number" || typeof arg === "string" || typeof arg === "symbol" || // ES6 symbol
      typeof arg === "undefined";
    }
    exports.isPrimitive = isPrimitive;
    exports.isBuffer = __require2("buffer").Buffer.isBuffer;
    function objectToString(o) {
      return Object.prototype.toString.call(o);
    }
  }
});
var require_inherits_browser = __commonJS({
  "../../node_modules/.pnpm/inherits@2.0.4/node_modules/inherits/inherits_browser.js"(exports, module) {
    if (typeof Object.create === "function") {
      module.exports = function inherits(ctor, superCtor) {
        if (superCtor) {
          ctor.super_ = superCtor;
          ctor.prototype = Object.create(superCtor.prototype, {
            constructor: {
              value: ctor,
              enumerable: false,
              writable: true,
              configurable: true
            }
          });
        }
      };
    } else {
      module.exports = function inherits(ctor, superCtor) {
        if (superCtor) {
          ctor.super_ = superCtor;
          var TempCtor = function() {
          };
          TempCtor.prototype = superCtor.prototype;
          ctor.prototype = new TempCtor();
          ctor.prototype.constructor = ctor;
        }
      };
    }
  }
});
var require_inherits = __commonJS({
  "../../node_modules/.pnpm/inherits@2.0.4/node_modules/inherits/inherits.js"(exports, module) {
    try {
      util = __require2("util");
      if (typeof util.inherits !== "function") throw "";
      module.exports = util.inherits;
    } catch (e) {
      module.exports = require_inherits_browser();
    }
    var util;
  }
});
var require_BufferList = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/internal/streams/BufferList.js"(exports, module) {
    "use strict";
    function _classCallCheck(instance, Constructor) {
      if (!(instance instanceof Constructor)) {
        throw new TypeError("Cannot call a class as a function");
      }
    }
    var Buffer2 = require_safe_buffer().Buffer;
    var util = __require2("util");
    function copyBuffer(src, target, offset) {
      src.copy(target, offset);
    }
    module.exports = (function() {
      function BufferList() {
        _classCallCheck(this, BufferList);
        this.head = null;
        this.tail = null;
        this.length = 0;
      }
      BufferList.prototype.push = function push(v) {
        var entry = { data: v, next: null };
        if (this.length > 0) this.tail.next = entry;
        else this.head = entry;
        this.tail = entry;
        ++this.length;
      };
      BufferList.prototype.unshift = function unshift(v) {
        var entry = { data: v, next: this.head };
        if (this.length === 0) this.tail = entry;
        this.head = entry;
        ++this.length;
      };
      BufferList.prototype.shift = function shift() {
        if (this.length === 0) return;
        var ret = this.head.data;
        if (this.length === 1) this.head = this.tail = null;
        else this.head = this.head.next;
        --this.length;
        return ret;
      };
      BufferList.prototype.clear = function clear() {
        this.head = this.tail = null;
        this.length = 0;
      };
      BufferList.prototype.join = function join(s) {
        if (this.length === 0) return "";
        var p = this.head;
        var ret = "" + p.data;
        while (p = p.next) {
          ret += s + p.data;
        }
        return ret;
      };
      BufferList.prototype.concat = function concat(n) {
        if (this.length === 0) return Buffer2.alloc(0);
        var ret = Buffer2.allocUnsafe(n >>> 0);
        var p = this.head;
        var i = 0;
        while (p) {
          copyBuffer(p.data, ret, i);
          i += p.data.length;
          p = p.next;
        }
        return ret;
      };
      return BufferList;
    })();
    if (util && util.inspect && util.inspect.custom) {
      module.exports.prototype[util.inspect.custom] = function() {
        var obj = util.inspect({ length: this.length });
        return this.constructor.name + " " + obj;
      };
    }
  }
});
var require_destroy = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/internal/streams/destroy.js"(exports, module) {
    "use strict";
    var pna = require_process_nextick_args();
    function destroy(err, cb) {
      var _this = this;
      var readableDestroyed = this._readableState && this._readableState.destroyed;
      var writableDestroyed = this._writableState && this._writableState.destroyed;
      if (readableDestroyed || writableDestroyed) {
        if (cb) {
          cb(err);
        } else if (err) {
          if (!this._writableState) {
            pna.nextTick(emitErrorNT, this, err);
          } else if (!this._writableState.errorEmitted) {
            this._writableState.errorEmitted = true;
            pna.nextTick(emitErrorNT, this, err);
          }
        }
        return this;
      }
      if (this._readableState) {
        this._readableState.destroyed = true;
      }
      if (this._writableState) {
        this._writableState.destroyed = true;
      }
      this._destroy(err || null, function(err2) {
        if (!cb && err2) {
          if (!_this._writableState) {
            pna.nextTick(emitErrorNT, _this, err2);
          } else if (!_this._writableState.errorEmitted) {
            _this._writableState.errorEmitted = true;
            pna.nextTick(emitErrorNT, _this, err2);
          }
        } else if (cb) {
          cb(err2);
        }
      });
      return this;
    }
    function undestroy() {
      if (this._readableState) {
        this._readableState.destroyed = false;
        this._readableState.reading = false;
        this._readableState.ended = false;
        this._readableState.endEmitted = false;
      }
      if (this._writableState) {
        this._writableState.destroyed = false;
        this._writableState.ended = false;
        this._writableState.ending = false;
        this._writableState.finalCalled = false;
        this._writableState.prefinished = false;
        this._writableState.finished = false;
        this._writableState.errorEmitted = false;
      }
    }
    function emitErrorNT(self2, err) {
      self2.emit("error", err);
    }
    module.exports = {
      destroy,
      undestroy
    };
  }
});
var require_node = __commonJS({
  "../../node_modules/.pnpm/util-deprecate@1.0.2/node_modules/util-deprecate/node.js"(exports, module) {
    module.exports = __require2("util").deprecate;
  }
});
var require_stream_writable = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/_stream_writable.js"(exports, module) {
    "use strict";
    var pna = require_process_nextick_args();
    module.exports = Writable;
    function CorkedRequest(state) {
      var _this = this;
      this.next = null;
      this.entry = null;
      this.finish = function() {
        onCorkedFinish(_this, state);
      };
    }
    var asyncWrite = !process.browser && ["v0.10", "v0.9."].indexOf(process.version.slice(0, 5)) > -1 ? setImmediate : pna.nextTick;
    var Duplex;
    Writable.WritableState = WritableState;
    var util = Object.create(require_util());
    util.inherits = require_inherits();
    var internalUtil = {
      deprecate: require_node()
    };
    var Stream = require_stream();
    var Buffer2 = require_safe_buffer().Buffer;
    var OurUint8Array = (typeof global !== "undefined" ? global : typeof window !== "undefined" ? window : typeof self !== "undefined" ? self : {}).Uint8Array || function() {
    };
    function _uint8ArrayToBuffer(chunk) {
      return Buffer2.from(chunk);
    }
    function _isUint8Array(obj) {
      return Buffer2.isBuffer(obj) || obj instanceof OurUint8Array;
    }
    var destroyImpl = require_destroy();
    util.inherits(Writable, Stream);
    function nop() {
    }
    function WritableState(options, stream) {
      Duplex = Duplex || require_stream_duplex();
      options = options || {};
      var isDuplex = stream instanceof Duplex;
      this.objectMode = !!options.objectMode;
      if (isDuplex) this.objectMode = this.objectMode || !!options.writableObjectMode;
      var hwm = options.highWaterMark;
      var writableHwm = options.writableHighWaterMark;
      var defaultHwm = this.objectMode ? 16 : 16 * 1024;
      if (hwm || hwm === 0) this.highWaterMark = hwm;
      else if (isDuplex && (writableHwm || writableHwm === 0)) this.highWaterMark = writableHwm;
      else this.highWaterMark = defaultHwm;
      this.highWaterMark = Math.floor(this.highWaterMark);
      this.finalCalled = false;
      this.needDrain = false;
      this.ending = false;
      this.ended = false;
      this.finished = false;
      this.destroyed = false;
      var noDecode = options.decodeStrings === false;
      this.decodeStrings = !noDecode;
      this.defaultEncoding = options.defaultEncoding || "utf8";
      this.length = 0;
      this.writing = false;
      this.corked = 0;
      this.sync = true;
      this.bufferProcessing = false;
      this.onwrite = function(er) {
        onwrite(stream, er);
      };
      this.writecb = null;
      this.writelen = 0;
      this.bufferedRequest = null;
      this.lastBufferedRequest = null;
      this.pendingcb = 0;
      this.prefinished = false;
      this.errorEmitted = false;
      this.bufferedRequestCount = 0;
      this.corkedRequestsFree = new CorkedRequest(this);
    }
    WritableState.prototype.getBuffer = function getBuffer() {
      var current = this.bufferedRequest;
      var out = [];
      while (current) {
        out.push(current);
        current = current.next;
      }
      return out;
    };
    (function() {
      try {
        Object.defineProperty(WritableState.prototype, "buffer", {
          get: internalUtil.deprecate(function() {
            return this.getBuffer();
          }, "_writableState.buffer is deprecated. Use _writableState.getBuffer instead.", "DEP0003")
        });
      } catch (_) {
      }
    })();
    var realHasInstance;
    if (typeof Symbol === "function" && Symbol.hasInstance && typeof Function.prototype[Symbol.hasInstance] === "function") {
      realHasInstance = Function.prototype[Symbol.hasInstance];
      Object.defineProperty(Writable, Symbol.hasInstance, {
        value: function(object) {
          if (realHasInstance.call(this, object)) return true;
          if (this !== Writable) return false;
          return object && object._writableState instanceof WritableState;
        }
      });
    } else {
      realHasInstance = function(object) {
        return object instanceof this;
      };
    }
    function Writable(options) {
      Duplex = Duplex || require_stream_duplex();
      if (!realHasInstance.call(Writable, this) && !(this instanceof Duplex)) {
        return new Writable(options);
      }
      this._writableState = new WritableState(options, this);
      this.writable = true;
      if (options) {
        if (typeof options.write === "function") this._write = options.write;
        if (typeof options.writev === "function") this._writev = options.writev;
        if (typeof options.destroy === "function") this._destroy = options.destroy;
        if (typeof options.final === "function") this._final = options.final;
      }
      Stream.call(this);
    }
    Writable.prototype.pipe = function() {
      this.emit("error", new Error("Cannot pipe, not readable"));
    };
    function writeAfterEnd(stream, cb) {
      var er = new Error("write after end");
      stream.emit("error", er);
      pna.nextTick(cb, er);
    }
    function validChunk(stream, state, chunk, cb) {
      var valid = true;
      var er = false;
      if (chunk === null) {
        er = new TypeError("May not write null values to stream");
      } else if (typeof chunk !== "string" && chunk !== void 0 && !state.objectMode) {
        er = new TypeError("Invalid non-string/buffer chunk");
      }
      if (er) {
        stream.emit("error", er);
        pna.nextTick(cb, er);
        valid = false;
      }
      return valid;
    }
    Writable.prototype.write = function(chunk, encoding, cb) {
      var state = this._writableState;
      var ret = false;
      var isBuf = !state.objectMode && _isUint8Array(chunk);
      if (isBuf && !Buffer2.isBuffer(chunk)) {
        chunk = _uint8ArrayToBuffer(chunk);
      }
      if (typeof encoding === "function") {
        cb = encoding;
        encoding = null;
      }
      if (isBuf) encoding = "buffer";
      else if (!encoding) encoding = state.defaultEncoding;
      if (typeof cb !== "function") cb = nop;
      if (state.ended) writeAfterEnd(this, cb);
      else if (isBuf || validChunk(this, state, chunk, cb)) {
        state.pendingcb++;
        ret = writeOrBuffer(this, state, isBuf, chunk, encoding, cb);
      }
      return ret;
    };
    Writable.prototype.cork = function() {
      var state = this._writableState;
      state.corked++;
    };
    Writable.prototype.uncork = function() {
      var state = this._writableState;
      if (state.corked) {
        state.corked--;
        if (!state.writing && !state.corked && !state.bufferProcessing && state.bufferedRequest) clearBuffer(this, state);
      }
    };
    Writable.prototype.setDefaultEncoding = function setDefaultEncoding(encoding) {
      if (typeof encoding === "string") encoding = encoding.toLowerCase();
      if (!(["hex", "utf8", "utf-8", "ascii", "binary", "base64", "ucs2", "ucs-2", "utf16le", "utf-16le", "raw"].indexOf((encoding + "").toLowerCase()) > -1)) throw new TypeError("Unknown encoding: " + encoding);
      this._writableState.defaultEncoding = encoding;
      return this;
    };
    function decodeChunk(state, chunk, encoding) {
      if (!state.objectMode && state.decodeStrings !== false && typeof chunk === "string") {
        chunk = Buffer2.from(chunk, encoding);
      }
      return chunk;
    }
    Object.defineProperty(Writable.prototype, "writableHighWaterMark", {
      // making it explicit this property is not enumerable
      // because otherwise some prototype manipulation in
      // userland will fail
      enumerable: false,
      get: function() {
        return this._writableState.highWaterMark;
      }
    });
    function writeOrBuffer(stream, state, isBuf, chunk, encoding, cb) {
      if (!isBuf) {
        var newChunk = decodeChunk(state, chunk, encoding);
        if (chunk !== newChunk) {
          isBuf = true;
          encoding = "buffer";
          chunk = newChunk;
        }
      }
      var len = state.objectMode ? 1 : chunk.length;
      state.length += len;
      var ret = state.length < state.highWaterMark;
      if (!ret) state.needDrain = true;
      if (state.writing || state.corked) {
        var last = state.lastBufferedRequest;
        state.lastBufferedRequest = {
          chunk,
          encoding,
          isBuf,
          callback: cb,
          next: null
        };
        if (last) {
          last.next = state.lastBufferedRequest;
        } else {
          state.bufferedRequest = state.lastBufferedRequest;
        }
        state.bufferedRequestCount += 1;
      } else {
        doWrite(stream, state, false, len, chunk, encoding, cb);
      }
      return ret;
    }
    function doWrite(stream, state, writev, len, chunk, encoding, cb) {
      state.writelen = len;
      state.writecb = cb;
      state.writing = true;
      state.sync = true;
      if (writev) stream._writev(chunk, state.onwrite);
      else stream._write(chunk, encoding, state.onwrite);
      state.sync = false;
    }
    function onwriteError(stream, state, sync, er, cb) {
      --state.pendingcb;
      if (sync) {
        pna.nextTick(cb, er);
        pna.nextTick(finishMaybe, stream, state);
        stream._writableState.errorEmitted = true;
        stream.emit("error", er);
      } else {
        cb(er);
        stream._writableState.errorEmitted = true;
        stream.emit("error", er);
        finishMaybe(stream, state);
      }
    }
    function onwriteStateUpdate(state) {
      state.writing = false;
      state.writecb = null;
      state.length -= state.writelen;
      state.writelen = 0;
    }
    function onwrite(stream, er) {
      var state = stream._writableState;
      var sync = state.sync;
      var cb = state.writecb;
      onwriteStateUpdate(state);
      if (er) onwriteError(stream, state, sync, er, cb);
      else {
        var finished = needFinish(state);
        if (!finished && !state.corked && !state.bufferProcessing && state.bufferedRequest) {
          clearBuffer(stream, state);
        }
        if (sync) {
          asyncWrite(afterWrite, stream, state, finished, cb);
        } else {
          afterWrite(stream, state, finished, cb);
        }
      }
    }
    function afterWrite(stream, state, finished, cb) {
      if (!finished) onwriteDrain(stream, state);
      state.pendingcb--;
      cb();
      finishMaybe(stream, state);
    }
    function onwriteDrain(stream, state) {
      if (state.length === 0 && state.needDrain) {
        state.needDrain = false;
        stream.emit("drain");
      }
    }
    function clearBuffer(stream, state) {
      state.bufferProcessing = true;
      var entry = state.bufferedRequest;
      if (stream._writev && entry && entry.next) {
        var l = state.bufferedRequestCount;
        var buffer = new Array(l);
        var holder = state.corkedRequestsFree;
        holder.entry = entry;
        var count = 0;
        var allBuffers = true;
        while (entry) {
          buffer[count] = entry;
          if (!entry.isBuf) allBuffers = false;
          entry = entry.next;
          count += 1;
        }
        buffer.allBuffers = allBuffers;
        doWrite(stream, state, true, state.length, buffer, "", holder.finish);
        state.pendingcb++;
        state.lastBufferedRequest = null;
        if (holder.next) {
          state.corkedRequestsFree = holder.next;
          holder.next = null;
        } else {
          state.corkedRequestsFree = new CorkedRequest(state);
        }
        state.bufferedRequestCount = 0;
      } else {
        while (entry) {
          var chunk = entry.chunk;
          var encoding = entry.encoding;
          var cb = entry.callback;
          var len = state.objectMode ? 1 : chunk.length;
          doWrite(stream, state, false, len, chunk, encoding, cb);
          entry = entry.next;
          state.bufferedRequestCount--;
          if (state.writing) {
            break;
          }
        }
        if (entry === null) state.lastBufferedRequest = null;
      }
      state.bufferedRequest = entry;
      state.bufferProcessing = false;
    }
    Writable.prototype._write = function(chunk, encoding, cb) {
      cb(new Error("_write() is not implemented"));
    };
    Writable.prototype._writev = null;
    Writable.prototype.end = function(chunk, encoding, cb) {
      var state = this._writableState;
      if (typeof chunk === "function") {
        cb = chunk;
        chunk = null;
        encoding = null;
      } else if (typeof encoding === "function") {
        cb = encoding;
        encoding = null;
      }
      if (chunk !== null && chunk !== void 0) this.write(chunk, encoding);
      if (state.corked) {
        state.corked = 1;
        this.uncork();
      }
      if (!state.ending) endWritable(this, state, cb);
    };
    function needFinish(state) {
      return state.ending && state.length === 0 && state.bufferedRequest === null && !state.finished && !state.writing;
    }
    function callFinal(stream, state) {
      stream._final(function(err) {
        state.pendingcb--;
        if (err) {
          stream.emit("error", err);
        }
        state.prefinished = true;
        stream.emit("prefinish");
        finishMaybe(stream, state);
      });
    }
    function prefinish(stream, state) {
      if (!state.prefinished && !state.finalCalled) {
        if (typeof stream._final === "function") {
          state.pendingcb++;
          state.finalCalled = true;
          pna.nextTick(callFinal, stream, state);
        } else {
          state.prefinished = true;
          stream.emit("prefinish");
        }
      }
    }
    function finishMaybe(stream, state) {
      var need = needFinish(state);
      if (need) {
        prefinish(stream, state);
        if (state.pendingcb === 0) {
          state.finished = true;
          stream.emit("finish");
        }
      }
      return need;
    }
    function endWritable(stream, state, cb) {
      state.ending = true;
      finishMaybe(stream, state);
      if (cb) {
        if (state.finished) pna.nextTick(cb);
        else stream.once("finish", cb);
      }
      state.ended = true;
      stream.writable = false;
    }
    function onCorkedFinish(corkReq, state, err) {
      var entry = corkReq.entry;
      corkReq.entry = null;
      while (entry) {
        var cb = entry.callback;
        state.pendingcb--;
        cb(err);
        entry = entry.next;
      }
      state.corkedRequestsFree.next = corkReq;
    }
    Object.defineProperty(Writable.prototype, "destroyed", {
      get: function() {
        if (this._writableState === void 0) {
          return false;
        }
        return this._writableState.destroyed;
      },
      set: function(value) {
        if (!this._writableState) {
          return;
        }
        this._writableState.destroyed = value;
      }
    });
    Writable.prototype.destroy = destroyImpl.destroy;
    Writable.prototype._undestroy = destroyImpl.undestroy;
    Writable.prototype._destroy = function(err, cb) {
      this.end();
      cb(err);
    };
  }
});
var require_stream_duplex = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/_stream_duplex.js"(exports, module) {
    "use strict";
    var pna = require_process_nextick_args();
    var objectKeys = Object.keys || function(obj) {
      var keys2 = [];
      for (var key in obj) {
        keys2.push(key);
      }
      return keys2;
    };
    module.exports = Duplex;
    var util = Object.create(require_util());
    util.inherits = require_inherits();
    var Readable = require_stream_readable();
    var Writable = require_stream_writable();
    util.inherits(Duplex, Readable);
    {
      keys = objectKeys(Writable.prototype);
      for (v = 0; v < keys.length; v++) {
        method = keys[v];
        if (!Duplex.prototype[method]) Duplex.prototype[method] = Writable.prototype[method];
      }
    }
    var keys;
    var method;
    var v;
    function Duplex(options) {
      if (!(this instanceof Duplex)) return new Duplex(options);
      Readable.call(this, options);
      Writable.call(this, options);
      if (options && options.readable === false) this.readable = false;
      if (options && options.writable === false) this.writable = false;
      this.allowHalfOpen = true;
      if (options && options.allowHalfOpen === false) this.allowHalfOpen = false;
      this.once("end", onend);
    }
    Object.defineProperty(Duplex.prototype, "writableHighWaterMark", {
      // making it explicit this property is not enumerable
      // because otherwise some prototype manipulation in
      // userland will fail
      enumerable: false,
      get: function() {
        return this._writableState.highWaterMark;
      }
    });
    function onend() {
      if (this.allowHalfOpen || this._writableState.ended) return;
      pna.nextTick(onEndNT, this);
    }
    function onEndNT(self2) {
      self2.end();
    }
    Object.defineProperty(Duplex.prototype, "destroyed", {
      get: function() {
        if (this._readableState === void 0 || this._writableState === void 0) {
          return false;
        }
        return this._readableState.destroyed && this._writableState.destroyed;
      },
      set: function(value) {
        if (this._readableState === void 0 || this._writableState === void 0) {
          return;
        }
        this._readableState.destroyed = value;
        this._writableState.destroyed = value;
      }
    });
    Duplex.prototype._destroy = function(err, cb) {
      this.push(null);
      this.end();
      pna.nextTick(cb, err);
    };
  }
});
var require_string_decoder = __commonJS({
  "../../node_modules/.pnpm/string_decoder@1.1.1/node_modules/string_decoder/lib/string_decoder.js"(exports) {
    "use strict";
    var Buffer2 = require_safe_buffer().Buffer;
    var isEncoding = Buffer2.isEncoding || function(encoding) {
      encoding = "" + encoding;
      switch (encoding && encoding.toLowerCase()) {
        case "hex":
        case "utf8":
        case "utf-8":
        case "ascii":
        case "binary":
        case "base64":
        case "ucs2":
        case "ucs-2":
        case "utf16le":
        case "utf-16le":
        case "raw":
          return true;
        default:
          return false;
      }
    };
    function _normalizeEncoding(enc) {
      if (!enc) return "utf8";
      var retried;
      while (true) {
        switch (enc) {
          case "utf8":
          case "utf-8":
            return "utf8";
          case "ucs2":
          case "ucs-2":
          case "utf16le":
          case "utf-16le":
            return "utf16le";
          case "latin1":
          case "binary":
            return "latin1";
          case "base64":
          case "ascii":
          case "hex":
            return enc;
          default:
            if (retried) return;
            enc = ("" + enc).toLowerCase();
            retried = true;
        }
      }
    }
    function normalizeEncoding(enc) {
      var nenc = _normalizeEncoding(enc);
      if (typeof nenc !== "string" && (Buffer2.isEncoding === isEncoding || !isEncoding(enc))) throw new Error("Unknown encoding: " + enc);
      return nenc || enc;
    }
    exports.StringDecoder = StringDecoder;
    function StringDecoder(encoding) {
      this.encoding = normalizeEncoding(encoding);
      var nb;
      switch (this.encoding) {
        case "utf16le":
          this.text = utf16Text;
          this.end = utf16End;
          nb = 4;
          break;
        case "utf8":
          this.fillLast = utf8FillLast;
          nb = 4;
          break;
        case "base64":
          this.text = base64Text;
          this.end = base64End;
          nb = 3;
          break;
        default:
          this.write = simpleWrite;
          this.end = simpleEnd;
          return;
      }
      this.lastNeed = 0;
      this.lastTotal = 0;
      this.lastChar = Buffer2.allocUnsafe(nb);
    }
    StringDecoder.prototype.write = function(buf) {
      if (buf.length === 0) return "";
      var r;
      var i;
      if (this.lastNeed) {
        r = this.fillLast(buf);
        if (r === void 0) return "";
        i = this.lastNeed;
        this.lastNeed = 0;
      } else {
        i = 0;
      }
      if (i < buf.length) return r ? r + this.text(buf, i) : this.text(buf, i);
      return r || "";
    };
    StringDecoder.prototype.end = utf8End;
    StringDecoder.prototype.text = utf8Text;
    StringDecoder.prototype.fillLast = function(buf) {
      if (this.lastNeed <= buf.length) {
        buf.copy(this.lastChar, this.lastTotal - this.lastNeed, 0, this.lastNeed);
        return this.lastChar.toString(this.encoding, 0, this.lastTotal);
      }
      buf.copy(this.lastChar, this.lastTotal - this.lastNeed, 0, buf.length);
      this.lastNeed -= buf.length;
    };
    function utf8CheckByte(byte) {
      if (byte <= 127) return 0;
      else if (byte >> 5 === 6) return 2;
      else if (byte >> 4 === 14) return 3;
      else if (byte >> 3 === 30) return 4;
      return byte >> 6 === 2 ? -1 : -2;
    }
    function utf8CheckIncomplete(self2, buf, i) {
      var j = buf.length - 1;
      if (j < i) return 0;
      var nb = utf8CheckByte(buf[j]);
      if (nb >= 0) {
        if (nb > 0) self2.lastNeed = nb - 1;
        return nb;
      }
      if (--j < i || nb === -2) return 0;
      nb = utf8CheckByte(buf[j]);
      if (nb >= 0) {
        if (nb > 0) self2.lastNeed = nb - 2;
        return nb;
      }
      if (--j < i || nb === -2) return 0;
      nb = utf8CheckByte(buf[j]);
      if (nb >= 0) {
        if (nb > 0) {
          if (nb === 2) nb = 0;
          else self2.lastNeed = nb - 3;
        }
        return nb;
      }
      return 0;
    }
    function utf8CheckExtraBytes(self2, buf, p) {
      if ((buf[0] & 192) !== 128) {
        self2.lastNeed = 0;
        return "\uFFFD";
      }
      if (self2.lastNeed > 1 && buf.length > 1) {
        if ((buf[1] & 192) !== 128) {
          self2.lastNeed = 1;
          return "\uFFFD";
        }
        if (self2.lastNeed > 2 && buf.length > 2) {
          if ((buf[2] & 192) !== 128) {
            self2.lastNeed = 2;
            return "\uFFFD";
          }
        }
      }
    }
    function utf8FillLast(buf) {
      var p = this.lastTotal - this.lastNeed;
      var r = utf8CheckExtraBytes(this, buf, p);
      if (r !== void 0) return r;
      if (this.lastNeed <= buf.length) {
        buf.copy(this.lastChar, p, 0, this.lastNeed);
        return this.lastChar.toString(this.encoding, 0, this.lastTotal);
      }
      buf.copy(this.lastChar, p, 0, buf.length);
      this.lastNeed -= buf.length;
    }
    function utf8Text(buf, i) {
      var total = utf8CheckIncomplete(this, buf, i);
      if (!this.lastNeed) return buf.toString("utf8", i);
      this.lastTotal = total;
      var end = buf.length - (total - this.lastNeed);
      buf.copy(this.lastChar, 0, end);
      return buf.toString("utf8", i, end);
    }
    function utf8End(buf) {
      var r = buf && buf.length ? this.write(buf) : "";
      if (this.lastNeed) return r + "\uFFFD";
      return r;
    }
    function utf16Text(buf, i) {
      if ((buf.length - i) % 2 === 0) {
        var r = buf.toString("utf16le", i);
        if (r) {
          var c = r.charCodeAt(r.length - 1);
          if (c >= 55296 && c <= 56319) {
            this.lastNeed = 2;
            this.lastTotal = 4;
            this.lastChar[0] = buf[buf.length - 2];
            this.lastChar[1] = buf[buf.length - 1];
            return r.slice(0, -1);
          }
        }
        return r;
      }
      this.lastNeed = 1;
      this.lastTotal = 2;
      this.lastChar[0] = buf[buf.length - 1];
      return buf.toString("utf16le", i, buf.length - 1);
    }
    function utf16End(buf) {
      var r = buf && buf.length ? this.write(buf) : "";
      if (this.lastNeed) {
        var end = this.lastTotal - this.lastNeed;
        return r + this.lastChar.toString("utf16le", 0, end);
      }
      return r;
    }
    function base64Text(buf, i) {
      var n = (buf.length - i) % 3;
      if (n === 0) return buf.toString("base64", i);
      this.lastNeed = 3 - n;
      this.lastTotal = 3;
      if (n === 1) {
        this.lastChar[0] = buf[buf.length - 1];
      } else {
        this.lastChar[0] = buf[buf.length - 2];
        this.lastChar[1] = buf[buf.length - 1];
      }
      return buf.toString("base64", i, buf.length - n);
    }
    function base64End(buf) {
      var r = buf && buf.length ? this.write(buf) : "";
      if (this.lastNeed) return r + this.lastChar.toString("base64", 0, 3 - this.lastNeed);
      return r;
    }
    function simpleWrite(buf) {
      return buf.toString(this.encoding);
    }
    function simpleEnd(buf) {
      return buf && buf.length ? this.write(buf) : "";
    }
  }
});
var require_stream_readable = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/_stream_readable.js"(exports, module) {
    "use strict";
    var pna = require_process_nextick_args();
    module.exports = Readable;
    var isArray = require_isarray();
    var Duplex;
    Readable.ReadableState = ReadableState;
    var EE = __require2("events").EventEmitter;
    var EElistenerCount = function(emitter, type) {
      return emitter.listeners(type).length;
    };
    var Stream = require_stream();
    var Buffer2 = require_safe_buffer().Buffer;
    var OurUint8Array = (typeof global !== "undefined" ? global : typeof window !== "undefined" ? window : typeof self !== "undefined" ? self : {}).Uint8Array || function() {
    };
    function _uint8ArrayToBuffer(chunk) {
      return Buffer2.from(chunk);
    }
    function _isUint8Array(obj) {
      return Buffer2.isBuffer(obj) || obj instanceof OurUint8Array;
    }
    var util = Object.create(require_util());
    util.inherits = require_inherits();
    var debugUtil = __require2("util");
    var debug = void 0;
    if (debugUtil && debugUtil.debuglog) {
      debug = debugUtil.debuglog("stream");
    } else {
      debug = function() {
      };
    }
    var BufferList = require_BufferList();
    var destroyImpl = require_destroy();
    var StringDecoder;
    util.inherits(Readable, Stream);
    var kProxyEvents = ["error", "close", "destroy", "pause", "resume"];
    function prependListener(emitter, event, fn) {
      if (typeof emitter.prependListener === "function") return emitter.prependListener(event, fn);
      if (!emitter._events || !emitter._events[event]) emitter.on(event, fn);
      else if (isArray(emitter._events[event])) emitter._events[event].unshift(fn);
      else emitter._events[event] = [fn, emitter._events[event]];
    }
    function ReadableState(options, stream) {
      Duplex = Duplex || require_stream_duplex();
      options = options || {};
      var isDuplex = stream instanceof Duplex;
      this.objectMode = !!options.objectMode;
      if (isDuplex) this.objectMode = this.objectMode || !!options.readableObjectMode;
      var hwm = options.highWaterMark;
      var readableHwm = options.readableHighWaterMark;
      var defaultHwm = this.objectMode ? 16 : 16 * 1024;
      if (hwm || hwm === 0) this.highWaterMark = hwm;
      else if (isDuplex && (readableHwm || readableHwm === 0)) this.highWaterMark = readableHwm;
      else this.highWaterMark = defaultHwm;
      this.highWaterMark = Math.floor(this.highWaterMark);
      this.buffer = new BufferList();
      this.length = 0;
      this.pipes = null;
      this.pipesCount = 0;
      this.flowing = null;
      this.ended = false;
      this.endEmitted = false;
      this.reading = false;
      this.sync = true;
      this.needReadable = false;
      this.emittedReadable = false;
      this.readableListening = false;
      this.resumeScheduled = false;
      this.destroyed = false;
      this.defaultEncoding = options.defaultEncoding || "utf8";
      this.awaitDrain = 0;
      this.readingMore = false;
      this.decoder = null;
      this.encoding = null;
      if (options.encoding) {
        if (!StringDecoder) StringDecoder = require_string_decoder().StringDecoder;
        this.decoder = new StringDecoder(options.encoding);
        this.encoding = options.encoding;
      }
    }
    function Readable(options) {
      Duplex = Duplex || require_stream_duplex();
      if (!(this instanceof Readable)) return new Readable(options);
      this._readableState = new ReadableState(options, this);
      this.readable = true;
      if (options) {
        if (typeof options.read === "function") this._read = options.read;
        if (typeof options.destroy === "function") this._destroy = options.destroy;
      }
      Stream.call(this);
    }
    Object.defineProperty(Readable.prototype, "destroyed", {
      get: function() {
        if (this._readableState === void 0) {
          return false;
        }
        return this._readableState.destroyed;
      },
      set: function(value) {
        if (!this._readableState) {
          return;
        }
        this._readableState.destroyed = value;
      }
    });
    Readable.prototype.destroy = destroyImpl.destroy;
    Readable.prototype._undestroy = destroyImpl.undestroy;
    Readable.prototype._destroy = function(err, cb) {
      this.push(null);
      cb(err);
    };
    Readable.prototype.push = function(chunk, encoding) {
      var state = this._readableState;
      var skipChunkCheck;
      if (!state.objectMode) {
        if (typeof chunk === "string") {
          encoding = encoding || state.defaultEncoding;
          if (encoding !== state.encoding) {
            chunk = Buffer2.from(chunk, encoding);
            encoding = "";
          }
          skipChunkCheck = true;
        }
      } else {
        skipChunkCheck = true;
      }
      return readableAddChunk(this, chunk, encoding, false, skipChunkCheck);
    };
    Readable.prototype.unshift = function(chunk) {
      return readableAddChunk(this, chunk, null, true, false);
    };
    function readableAddChunk(stream, chunk, encoding, addToFront, skipChunkCheck) {
      var state = stream._readableState;
      if (chunk === null) {
        state.reading = false;
        onEofChunk(stream, state);
      } else {
        var er;
        if (!skipChunkCheck) er = chunkInvalid(state, chunk);
        if (er) {
          stream.emit("error", er);
        } else if (state.objectMode || chunk && chunk.length > 0) {
          if (typeof chunk !== "string" && !state.objectMode && Object.getPrototypeOf(chunk) !== Buffer2.prototype) {
            chunk = _uint8ArrayToBuffer(chunk);
          }
          if (addToFront) {
            if (state.endEmitted) stream.emit("error", new Error("stream.unshift() after end event"));
            else addChunk(stream, state, chunk, true);
          } else if (state.ended) {
            stream.emit("error", new Error("stream.push() after EOF"));
          } else {
            state.reading = false;
            if (state.decoder && !encoding) {
              chunk = state.decoder.write(chunk);
              if (state.objectMode || chunk.length !== 0) addChunk(stream, state, chunk, false);
              else maybeReadMore(stream, state);
            } else {
              addChunk(stream, state, chunk, false);
            }
          }
        } else if (!addToFront) {
          state.reading = false;
        }
      }
      return needMoreData(state);
    }
    function addChunk(stream, state, chunk, addToFront) {
      if (state.flowing && state.length === 0 && !state.sync) {
        stream.emit("data", chunk);
        stream.read(0);
      } else {
        state.length += state.objectMode ? 1 : chunk.length;
        if (addToFront) state.buffer.unshift(chunk);
        else state.buffer.push(chunk);
        if (state.needReadable) emitReadable(stream);
      }
      maybeReadMore(stream, state);
    }
    function chunkInvalid(state, chunk) {
      var er;
      if (!_isUint8Array(chunk) && typeof chunk !== "string" && chunk !== void 0 && !state.objectMode) {
        er = new TypeError("Invalid non-string/buffer chunk");
      }
      return er;
    }
    function needMoreData(state) {
      return !state.ended && (state.needReadable || state.length < state.highWaterMark || state.length === 0);
    }
    Readable.prototype.isPaused = function() {
      return this._readableState.flowing === false;
    };
    Readable.prototype.setEncoding = function(enc) {
      if (!StringDecoder) StringDecoder = require_string_decoder().StringDecoder;
      this._readableState.decoder = new StringDecoder(enc);
      this._readableState.encoding = enc;
      return this;
    };
    var MAX_HWM = 8388608;
    function computeNewHighWaterMark(n) {
      if (n >= MAX_HWM) {
        n = MAX_HWM;
      } else {
        n--;
        n |= n >>> 1;
        n |= n >>> 2;
        n |= n >>> 4;
        n |= n >>> 8;
        n |= n >>> 16;
        n++;
      }
      return n;
    }
    function howMuchToRead(n, state) {
      if (n <= 0 || state.length === 0 && state.ended) return 0;
      if (state.objectMode) return 1;
      if (n !== n) {
        if (state.flowing && state.length) return state.buffer.head.data.length;
        else return state.length;
      }
      if (n > state.highWaterMark) state.highWaterMark = computeNewHighWaterMark(n);
      if (n <= state.length) return n;
      if (!state.ended) {
        state.needReadable = true;
        return 0;
      }
      return state.length;
    }
    Readable.prototype.read = function(n) {
      debug("read", n);
      n = parseInt(n, 10);
      var state = this._readableState;
      var nOrig = n;
      if (n !== 0) state.emittedReadable = false;
      if (n === 0 && state.needReadable && (state.length >= state.highWaterMark || state.ended)) {
        debug("read: emitReadable", state.length, state.ended);
        if (state.length === 0 && state.ended) endReadable(this);
        else emitReadable(this);
        return null;
      }
      n = howMuchToRead(n, state);
      if (n === 0 && state.ended) {
        if (state.length === 0) endReadable(this);
        return null;
      }
      var doRead = state.needReadable;
      debug("need readable", doRead);
      if (state.length === 0 || state.length - n < state.highWaterMark) {
        doRead = true;
        debug("length less than watermark", doRead);
      }
      if (state.ended || state.reading) {
        doRead = false;
        debug("reading or ended", doRead);
      } else if (doRead) {
        debug("do read");
        state.reading = true;
        state.sync = true;
        if (state.length === 0) state.needReadable = true;
        this._read(state.highWaterMark);
        state.sync = false;
        if (!state.reading) n = howMuchToRead(nOrig, state);
      }
      var ret;
      if (n > 0) ret = fromList(n, state);
      else ret = null;
      if (ret === null) {
        state.needReadable = true;
        n = 0;
      } else {
        state.length -= n;
      }
      if (state.length === 0) {
        if (!state.ended) state.needReadable = true;
        if (nOrig !== n && state.ended) endReadable(this);
      }
      if (ret !== null) this.emit("data", ret);
      return ret;
    };
    function onEofChunk(stream, state) {
      if (state.ended) return;
      if (state.decoder) {
        var chunk = state.decoder.end();
        if (chunk && chunk.length) {
          state.buffer.push(chunk);
          state.length += state.objectMode ? 1 : chunk.length;
        }
      }
      state.ended = true;
      emitReadable(stream);
    }
    function emitReadable(stream) {
      var state = stream._readableState;
      state.needReadable = false;
      if (!state.emittedReadable) {
        debug("emitReadable", state.flowing);
        state.emittedReadable = true;
        if (state.sync) pna.nextTick(emitReadable_, stream);
        else emitReadable_(stream);
      }
    }
    function emitReadable_(stream) {
      debug("emit readable");
      stream.emit("readable");
      flow(stream);
    }
    function maybeReadMore(stream, state) {
      if (!state.readingMore) {
        state.readingMore = true;
        pna.nextTick(maybeReadMore_, stream, state);
      }
    }
    function maybeReadMore_(stream, state) {
      var len = state.length;
      while (!state.reading && !state.flowing && !state.ended && state.length < state.highWaterMark) {
        debug("maybeReadMore read 0");
        stream.read(0);
        if (len === state.length)
          break;
        else len = state.length;
      }
      state.readingMore = false;
    }
    Readable.prototype._read = function(n) {
      this.emit("error", new Error("_read() is not implemented"));
    };
    Readable.prototype.pipe = function(dest, pipeOpts) {
      var src = this;
      var state = this._readableState;
      switch (state.pipesCount) {
        case 0:
          state.pipes = dest;
          break;
        case 1:
          state.pipes = [state.pipes, dest];
          break;
        default:
          state.pipes.push(dest);
          break;
      }
      state.pipesCount += 1;
      debug("pipe count=%d opts=%j", state.pipesCount, pipeOpts);
      var doEnd = (!pipeOpts || pipeOpts.end !== false) && dest !== process.stdout && dest !== process.stderr;
      var endFn = doEnd ? onend : unpipe;
      if (state.endEmitted) pna.nextTick(endFn);
      else src.once("end", endFn);
      dest.on("unpipe", onunpipe);
      function onunpipe(readable, unpipeInfo) {
        debug("onunpipe");
        if (readable === src) {
          if (unpipeInfo && unpipeInfo.hasUnpiped === false) {
            unpipeInfo.hasUnpiped = true;
            cleanup();
          }
        }
      }
      function onend() {
        debug("onend");
        dest.end();
      }
      var ondrain = pipeOnDrain(src);
      dest.on("drain", ondrain);
      var cleanedUp = false;
      function cleanup() {
        debug("cleanup");
        dest.removeListener("close", onclose);
        dest.removeListener("finish", onfinish);
        dest.removeListener("drain", ondrain);
        dest.removeListener("error", onerror);
        dest.removeListener("unpipe", onunpipe);
        src.removeListener("end", onend);
        src.removeListener("end", unpipe);
        src.removeListener("data", ondata);
        cleanedUp = true;
        if (state.awaitDrain && (!dest._writableState || dest._writableState.needDrain)) ondrain();
      }
      var increasedAwaitDrain = false;
      src.on("data", ondata);
      function ondata(chunk) {
        debug("ondata");
        increasedAwaitDrain = false;
        var ret = dest.write(chunk);
        if (false === ret && !increasedAwaitDrain) {
          if ((state.pipesCount === 1 && state.pipes === dest || state.pipesCount > 1 && indexOf(state.pipes, dest) !== -1) && !cleanedUp) {
            debug("false write response, pause", state.awaitDrain);
            state.awaitDrain++;
            increasedAwaitDrain = true;
          }
          src.pause();
        }
      }
      function onerror(er) {
        debug("onerror", er);
        unpipe();
        dest.removeListener("error", onerror);
        if (EElistenerCount(dest, "error") === 0) dest.emit("error", er);
      }
      prependListener(dest, "error", onerror);
      function onclose() {
        dest.removeListener("finish", onfinish);
        unpipe();
      }
      dest.once("close", onclose);
      function onfinish() {
        debug("onfinish");
        dest.removeListener("close", onclose);
        unpipe();
      }
      dest.once("finish", onfinish);
      function unpipe() {
        debug("unpipe");
        src.unpipe(dest);
      }
      dest.emit("pipe", src);
      if (!state.flowing) {
        debug("pipe resume");
        src.resume();
      }
      return dest;
    };
    function pipeOnDrain(src) {
      return function() {
        var state = src._readableState;
        debug("pipeOnDrain", state.awaitDrain);
        if (state.awaitDrain) state.awaitDrain--;
        if (state.awaitDrain === 0 && EElistenerCount(src, "data")) {
          state.flowing = true;
          flow(src);
        }
      };
    }
    Readable.prototype.unpipe = function(dest) {
      var state = this._readableState;
      var unpipeInfo = { hasUnpiped: false };
      if (state.pipesCount === 0) return this;
      if (state.pipesCount === 1) {
        if (dest && dest !== state.pipes) return this;
        if (!dest) dest = state.pipes;
        state.pipes = null;
        state.pipesCount = 0;
        state.flowing = false;
        if (dest) dest.emit("unpipe", this, unpipeInfo);
        return this;
      }
      if (!dest) {
        var dests = state.pipes;
        var len = state.pipesCount;
        state.pipes = null;
        state.pipesCount = 0;
        state.flowing = false;
        for (var i = 0; i < len; i++) {
          dests[i].emit("unpipe", this, { hasUnpiped: false });
        }
        return this;
      }
      var index = indexOf(state.pipes, dest);
      if (index === -1) return this;
      state.pipes.splice(index, 1);
      state.pipesCount -= 1;
      if (state.pipesCount === 1) state.pipes = state.pipes[0];
      dest.emit("unpipe", this, unpipeInfo);
      return this;
    };
    Readable.prototype.on = function(ev, fn) {
      var res = Stream.prototype.on.call(this, ev, fn);
      if (ev === "data") {
        if (this._readableState.flowing !== false) this.resume();
      } else if (ev === "readable") {
        var state = this._readableState;
        if (!state.endEmitted && !state.readableListening) {
          state.readableListening = state.needReadable = true;
          state.emittedReadable = false;
          if (!state.reading) {
            pna.nextTick(nReadingNextTick, this);
          } else if (state.length) {
            emitReadable(this);
          }
        }
      }
      return res;
    };
    Readable.prototype.addListener = Readable.prototype.on;
    function nReadingNextTick(self2) {
      debug("readable nexttick read 0");
      self2.read(0);
    }
    Readable.prototype.resume = function() {
      var state = this._readableState;
      if (!state.flowing) {
        debug("resume");
        state.flowing = true;
        resume(this, state);
      }
      return this;
    };
    function resume(stream, state) {
      if (!state.resumeScheduled) {
        state.resumeScheduled = true;
        pna.nextTick(resume_, stream, state);
      }
    }
    function resume_(stream, state) {
      if (!state.reading) {
        debug("resume read 0");
        stream.read(0);
      }
      state.resumeScheduled = false;
      state.awaitDrain = 0;
      stream.emit("resume");
      flow(stream);
      if (state.flowing && !state.reading) stream.read(0);
    }
    Readable.prototype.pause = function() {
      debug("call pause flowing=%j", this._readableState.flowing);
      if (false !== this._readableState.flowing) {
        debug("pause");
        this._readableState.flowing = false;
        this.emit("pause");
      }
      return this;
    };
    function flow(stream) {
      var state = stream._readableState;
      debug("flow", state.flowing);
      while (state.flowing && stream.read() !== null) {
      }
    }
    Readable.prototype.wrap = function(stream) {
      var _this = this;
      var state = this._readableState;
      var paused = false;
      stream.on("end", function() {
        debug("wrapped end");
        if (state.decoder && !state.ended) {
          var chunk = state.decoder.end();
          if (chunk && chunk.length) _this.push(chunk);
        }
        _this.push(null);
      });
      stream.on("data", function(chunk) {
        debug("wrapped data");
        if (state.decoder) chunk = state.decoder.write(chunk);
        if (state.objectMode && (chunk === null || chunk === void 0)) return;
        else if (!state.objectMode && (!chunk || !chunk.length)) return;
        var ret = _this.push(chunk);
        if (!ret) {
          paused = true;
          stream.pause();
        }
      });
      for (var i in stream) {
        if (this[i] === void 0 && typeof stream[i] === "function") {
          this[i] = /* @__PURE__ */ (function(method) {
            return function() {
              return stream[method].apply(stream, arguments);
            };
          })(i);
        }
      }
      for (var n = 0; n < kProxyEvents.length; n++) {
        stream.on(kProxyEvents[n], this.emit.bind(this, kProxyEvents[n]));
      }
      this._read = function(n2) {
        debug("wrapped _read", n2);
        if (paused) {
          paused = false;
          stream.resume();
        }
      };
      return this;
    };
    Object.defineProperty(Readable.prototype, "readableHighWaterMark", {
      // making it explicit this property is not enumerable
      // because otherwise some prototype manipulation in
      // userland will fail
      enumerable: false,
      get: function() {
        return this._readableState.highWaterMark;
      }
    });
    Readable._fromList = fromList;
    function fromList(n, state) {
      if (state.length === 0) return null;
      var ret;
      if (state.objectMode) ret = state.buffer.shift();
      else if (!n || n >= state.length) {
        if (state.decoder) ret = state.buffer.join("");
        else if (state.buffer.length === 1) ret = state.buffer.head.data;
        else ret = state.buffer.concat(state.length);
        state.buffer.clear();
      } else {
        ret = fromListPartial(n, state.buffer, state.decoder);
      }
      return ret;
    }
    function fromListPartial(n, list, hasStrings) {
      var ret;
      if (n < list.head.data.length) {
        ret = list.head.data.slice(0, n);
        list.head.data = list.head.data.slice(n);
      } else if (n === list.head.data.length) {
        ret = list.shift();
      } else {
        ret = hasStrings ? copyFromBufferString(n, list) : copyFromBuffer(n, list);
      }
      return ret;
    }
    function copyFromBufferString(n, list) {
      var p = list.head;
      var c = 1;
      var ret = p.data;
      n -= ret.length;
      while (p = p.next) {
        var str = p.data;
        var nb = n > str.length ? str.length : n;
        if (nb === str.length) ret += str;
        else ret += str.slice(0, n);
        n -= nb;
        if (n === 0) {
          if (nb === str.length) {
            ++c;
            if (p.next) list.head = p.next;
            else list.head = list.tail = null;
          } else {
            list.head = p;
            p.data = str.slice(nb);
          }
          break;
        }
        ++c;
      }
      list.length -= c;
      return ret;
    }
    function copyFromBuffer(n, list) {
      var ret = Buffer2.allocUnsafe(n);
      var p = list.head;
      var c = 1;
      p.data.copy(ret);
      n -= p.data.length;
      while (p = p.next) {
        var buf = p.data;
        var nb = n > buf.length ? buf.length : n;
        buf.copy(ret, ret.length - n, 0, nb);
        n -= nb;
        if (n === 0) {
          if (nb === buf.length) {
            ++c;
            if (p.next) list.head = p.next;
            else list.head = list.tail = null;
          } else {
            list.head = p;
            p.data = buf.slice(nb);
          }
          break;
        }
        ++c;
      }
      list.length -= c;
      return ret;
    }
    function endReadable(stream) {
      var state = stream._readableState;
      if (state.length > 0) throw new Error('"endReadable()" called on non-empty stream');
      if (!state.endEmitted) {
        state.ended = true;
        pna.nextTick(endReadableNT, state, stream);
      }
    }
    function endReadableNT(state, stream) {
      if (!state.endEmitted && state.length === 0) {
        state.endEmitted = true;
        stream.readable = false;
        stream.emit("end");
      }
    }
    function indexOf(xs, x) {
      for (var i = 0, l = xs.length; i < l; i++) {
        if (xs[i] === x) return i;
      }
      return -1;
    }
  }
});
var require_stream_transform = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/_stream_transform.js"(exports, module) {
    "use strict";
    module.exports = Transform;
    var Duplex = require_stream_duplex();
    var util = Object.create(require_util());
    util.inherits = require_inherits();
    util.inherits(Transform, Duplex);
    function afterTransform(er, data) {
      var ts = this._transformState;
      ts.transforming = false;
      var cb = ts.writecb;
      if (!cb) {
        return this.emit("error", new Error("write callback called multiple times"));
      }
      ts.writechunk = null;
      ts.writecb = null;
      if (data != null)
        this.push(data);
      cb(er);
      var rs = this._readableState;
      rs.reading = false;
      if (rs.needReadable || rs.length < rs.highWaterMark) {
        this._read(rs.highWaterMark);
      }
    }
    function Transform(options) {
      if (!(this instanceof Transform)) return new Transform(options);
      Duplex.call(this, options);
      this._transformState = {
        afterTransform: afterTransform.bind(this),
        needTransform: false,
        transforming: false,
        writecb: null,
        writechunk: null,
        writeencoding: null
      };
      this._readableState.needReadable = true;
      this._readableState.sync = false;
      if (options) {
        if (typeof options.transform === "function") this._transform = options.transform;
        if (typeof options.flush === "function") this._flush = options.flush;
      }
      this.on("prefinish", prefinish);
    }
    function prefinish() {
      var _this = this;
      if (typeof this._flush === "function") {
        this._flush(function(er, data) {
          done(_this, er, data);
        });
      } else {
        done(this, null, null);
      }
    }
    Transform.prototype.push = function(chunk, encoding) {
      this._transformState.needTransform = false;
      return Duplex.prototype.push.call(this, chunk, encoding);
    };
    Transform.prototype._transform = function(chunk, encoding, cb) {
      throw new Error("_transform() is not implemented");
    };
    Transform.prototype._write = function(chunk, encoding, cb) {
      var ts = this._transformState;
      ts.writecb = cb;
      ts.writechunk = chunk;
      ts.writeencoding = encoding;
      if (!ts.transforming) {
        var rs = this._readableState;
        if (ts.needTransform || rs.needReadable || rs.length < rs.highWaterMark) this._read(rs.highWaterMark);
      }
    };
    Transform.prototype._read = function(n) {
      var ts = this._transformState;
      if (ts.writechunk !== null && ts.writecb && !ts.transforming) {
        ts.transforming = true;
        this._transform(ts.writechunk, ts.writeencoding, ts.afterTransform);
      } else {
        ts.needTransform = true;
      }
    };
    Transform.prototype._destroy = function(err, cb) {
      var _this2 = this;
      Duplex.prototype._destroy.call(this, err, function(err2) {
        cb(err2);
        _this2.emit("close");
      });
    };
    function done(stream, er, data) {
      if (er) return stream.emit("error", er);
      if (data != null)
        stream.push(data);
      if (stream._writableState.length) throw new Error("Calling transform done when ws.length != 0");
      if (stream._transformState.transforming) throw new Error("Calling transform done when still transforming");
      return stream.push(null);
    }
  }
});
var require_stream_passthrough = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/lib/_stream_passthrough.js"(exports, module) {
    "use strict";
    module.exports = PassThrough;
    var Transform = require_stream_transform();
    var util = Object.create(require_util());
    util.inherits = require_inherits();
    util.inherits(PassThrough, Transform);
    function PassThrough(options) {
      if (!(this instanceof PassThrough)) return new PassThrough(options);
      Transform.call(this, options);
    }
    PassThrough.prototype._transform = function(chunk, encoding, cb) {
      cb(null, chunk);
    };
  }
});
var require_readable = __commonJS({
  "../../node_modules/.pnpm/readable-stream@2.3.8/node_modules/readable-stream/readable.js"(exports, module) {
    var Stream = __require2("stream");
    if (process.env.READABLE_STREAM === "disable" && Stream) {
      module.exports = Stream;
      exports = module.exports = Stream.Readable;
      exports.Readable = Stream.Readable;
      exports.Writable = Stream.Writable;
      exports.Duplex = Stream.Duplex;
      exports.Transform = Stream.Transform;
      exports.PassThrough = Stream.PassThrough;
      exports.Stream = Stream;
    } else {
      exports = module.exports = require_stream_readable();
      exports.Stream = Stream || exports;
      exports.Readable = exports;
      exports.Writable = require_stream_writable();
      exports.Duplex = require_stream_duplex();
      exports.Transform = require_stream_transform();
      exports.PassThrough = require_stream_passthrough();
    }
  }
});
var require_support = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/support.js"(exports) {
    "use strict";
    exports.base64 = true;
    exports.array = true;
    exports.string = true;
    exports.arraybuffer = typeof ArrayBuffer !== "undefined" && typeof Uint8Array !== "undefined";
    exports.nodebuffer = typeof Buffer !== "undefined";
    exports.uint8array = typeof Uint8Array !== "undefined";
    if (typeof ArrayBuffer === "undefined") {
      exports.blob = false;
    } else {
      buffer = new ArrayBuffer(0);
      try {
        exports.blob = new Blob([buffer], {
          type: "application/zip"
        }).size === 0;
      } catch (e) {
        try {
          Builder2 = self.BlobBuilder || self.WebKitBlobBuilder || self.MozBlobBuilder || self.MSBlobBuilder;
          builder = new Builder2();
          builder.append(buffer);
          exports.blob = builder.getBlob("application/zip").size === 0;
        } catch (e2) {
          exports.blob = false;
        }
      }
    }
    var buffer;
    var Builder2;
    var builder;
    try {
      exports.nodestream = !!require_readable().Readable;
    } catch (e) {
      exports.nodestream = false;
    }
  }
});
var require_base64 = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/base64.js"(exports) {
    "use strict";
    var utils = require_utils();
    var support = require_support();
    var _keyStr = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
    exports.encode = function(input) {
      var output = [];
      var chr1, chr2, chr3, enc1, enc2, enc3, enc4;
      var i = 0, len = input.length, remainingBytes = len;
      var isArray = utils.getTypeOf(input) !== "string";
      while (i < input.length) {
        remainingBytes = len - i;
        if (!isArray) {
          chr1 = input.charCodeAt(i++);
          chr2 = i < len ? input.charCodeAt(i++) : 0;
          chr3 = i < len ? input.charCodeAt(i++) : 0;
        } else {
          chr1 = input[i++];
          chr2 = i < len ? input[i++] : 0;
          chr3 = i < len ? input[i++] : 0;
        }
        enc1 = chr1 >> 2;
        enc2 = (chr1 & 3) << 4 | chr2 >> 4;
        enc3 = remainingBytes > 1 ? (chr2 & 15) << 2 | chr3 >> 6 : 64;
        enc4 = remainingBytes > 2 ? chr3 & 63 : 64;
        output.push(_keyStr.charAt(enc1) + _keyStr.charAt(enc2) + _keyStr.charAt(enc3) + _keyStr.charAt(enc4));
      }
      return output.join("");
    };
    exports.decode = function(input) {
      var chr1, chr2, chr3;
      var enc1, enc2, enc3, enc4;
      var i = 0, resultIndex = 0;
      var dataUrlPrefix = "data:";
      if (input.substr(0, dataUrlPrefix.length) === dataUrlPrefix) {
        throw new Error("Invalid base64 input, it looks like a data url.");
      }
      input = input.replace(/[^A-Za-z0-9+/=]/g, "");
      var totalLength = input.length * 3 / 4;
      if (input.charAt(input.length - 1) === _keyStr.charAt(64)) {
        totalLength--;
      }
      if (input.charAt(input.length - 2) === _keyStr.charAt(64)) {
        totalLength--;
      }
      if (totalLength % 1 !== 0) {
        throw new Error("Invalid base64 input, bad content length.");
      }
      var output;
      if (support.uint8array) {
        output = new Uint8Array(totalLength | 0);
      } else {
        output = new Array(totalLength | 0);
      }
      while (i < input.length) {
        enc1 = _keyStr.indexOf(input.charAt(i++));
        enc2 = _keyStr.indexOf(input.charAt(i++));
        enc3 = _keyStr.indexOf(input.charAt(i++));
        enc4 = _keyStr.indexOf(input.charAt(i++));
        chr1 = enc1 << 2 | enc2 >> 4;
        chr2 = (enc2 & 15) << 4 | enc3 >> 2;
        chr3 = (enc3 & 3) << 6 | enc4;
        output[resultIndex++] = chr1;
        if (enc3 !== 64) {
          output[resultIndex++] = chr2;
        }
        if (enc4 !== 64) {
          output[resultIndex++] = chr3;
        }
      }
      return output;
    };
  }
});
var require_nodejsUtils = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/nodejsUtils.js"(exports, module) {
    "use strict";
    module.exports = {
      /**
       * True if this is running in Nodejs, will be undefined in a browser.
       * In a browser, browserify won't include this file and the whole module
       * will be resolved an empty object.
       */
      isNode: typeof Buffer !== "undefined",
      /**
       * Create a new nodejs Buffer from an existing content.
       * @param {Object} data the data to pass to the constructor.
       * @param {String} encoding the encoding to use.
       * @return {Buffer} a new Buffer.
       */
      newBufferFrom: function(data, encoding) {
        if (Buffer.from && Buffer.from !== Uint8Array.from) {
          return Buffer.from(data, encoding);
        } else {
          if (typeof data === "number") {
            throw new Error('The "data" argument must not be a number');
          }
          return new Buffer(data, encoding);
        }
      },
      /**
       * Create a new nodejs Buffer with the specified size.
       * @param {Integer} size the size of the buffer.
       * @return {Buffer} a new Buffer.
       */
      allocBuffer: function(size) {
        if (Buffer.alloc) {
          return Buffer.alloc(size);
        } else {
          var buf = new Buffer(size);
          buf.fill(0);
          return buf;
        }
      },
      /**
       * Find out if an object is a Buffer.
       * @param {Object} b the object to test.
       * @return {Boolean} true if the object is a Buffer, false otherwise.
       */
      isBuffer: function(b) {
        return Buffer.isBuffer(b);
      },
      isStream: function(obj) {
        return obj && typeof obj.on === "function" && typeof obj.pause === "function" && typeof obj.resume === "function";
      }
    };
  }
});
var require_lib = __commonJS({
  "../../node_modules/.pnpm/immediate@3.0.6/node_modules/immediate/lib/index.js"(exports, module) {
    "use strict";
    var Mutation = global.MutationObserver || global.WebKitMutationObserver;
    var scheduleDrain;
    if (process.browser) {
      if (Mutation) {
        called = 0;
        observer = new Mutation(nextTick);
        element = global.document.createTextNode("");
        observer.observe(element, {
          characterData: true
        });
        scheduleDrain = function() {
          element.data = called = ++called % 2;
        };
      } else if (!global.setImmediate && typeof global.MessageChannel !== "undefined") {
        channel = new global.MessageChannel();
        channel.port1.onmessage = nextTick;
        scheduleDrain = function() {
          channel.port2.postMessage(0);
        };
      } else if ("document" in global && "onreadystatechange" in global.document.createElement("script")) {
        scheduleDrain = function() {
          var scriptEl = global.document.createElement("script");
          scriptEl.onreadystatechange = function() {
            nextTick();
            scriptEl.onreadystatechange = null;
            scriptEl.parentNode.removeChild(scriptEl);
            scriptEl = null;
          };
          global.document.documentElement.appendChild(scriptEl);
        };
      } else {
        scheduleDrain = function() {
          setTimeout(nextTick, 0);
        };
      }
    } else {
      scheduleDrain = function() {
        process.nextTick(nextTick);
      };
    }
    var called;
    var observer;
    var element;
    var channel;
    var draining;
    var queue = [];
    function nextTick() {
      draining = true;
      var i, oldQueue;
      var len = queue.length;
      while (len) {
        oldQueue = queue;
        queue = [];
        i = -1;
        while (++i < len) {
          oldQueue[i]();
        }
        len = queue.length;
      }
      draining = false;
    }
    module.exports = immediate;
    function immediate(task) {
      if (queue.push(task) === 1 && !draining) {
        scheduleDrain();
      }
    }
  }
});
var require_lib2 = __commonJS({
  "../../node_modules/.pnpm/lie@3.3.0/node_modules/lie/lib/index.js"(exports, module) {
    "use strict";
    var immediate = require_lib();
    function INTERNAL() {
    }
    var handlers = {};
    var REJECTED = ["REJECTED"];
    var FULFILLED = ["FULFILLED"];
    var PENDING = ["PENDING"];
    if (!process.browser) {
      UNHANDLED = ["UNHANDLED"];
    }
    var UNHANDLED;
    module.exports = Promise2;
    function Promise2(resolver) {
      if (typeof resolver !== "function") {
        throw new TypeError("resolver must be a function");
      }
      this.state = PENDING;
      this.queue = [];
      this.outcome = void 0;
      if (!process.browser) {
        this.handled = UNHANDLED;
      }
      if (resolver !== INTERNAL) {
        safelyResolveThenable(this, resolver);
      }
    }
    Promise2.prototype.finally = function(callback) {
      if (typeof callback !== "function") {
        return this;
      }
      var p = this.constructor;
      return this.then(resolve2, reject2);
      function resolve2(value) {
        function yes() {
          return value;
        }
        return p.resolve(callback()).then(yes);
      }
      function reject2(reason) {
        function no() {
          throw reason;
        }
        return p.resolve(callback()).then(no);
      }
    };
    Promise2.prototype.catch = function(onRejected) {
      return this.then(null, onRejected);
    };
    Promise2.prototype.then = function(onFulfilled, onRejected) {
      if (typeof onFulfilled !== "function" && this.state === FULFILLED || typeof onRejected !== "function" && this.state === REJECTED) {
        return this;
      }
      var promise = new this.constructor(INTERNAL);
      if (!process.browser) {
        if (this.handled === UNHANDLED) {
          this.handled = null;
        }
      }
      if (this.state !== PENDING) {
        var resolver = this.state === FULFILLED ? onFulfilled : onRejected;
        unwrap(promise, resolver, this.outcome);
      } else {
        this.queue.push(new QueueItem(promise, onFulfilled, onRejected));
      }
      return promise;
    };
    function QueueItem(promise, onFulfilled, onRejected) {
      this.promise = promise;
      if (typeof onFulfilled === "function") {
        this.onFulfilled = onFulfilled;
        this.callFulfilled = this.otherCallFulfilled;
      }
      if (typeof onRejected === "function") {
        this.onRejected = onRejected;
        this.callRejected = this.otherCallRejected;
      }
    }
    QueueItem.prototype.callFulfilled = function(value) {
      handlers.resolve(this.promise, value);
    };
    QueueItem.prototype.otherCallFulfilled = function(value) {
      unwrap(this.promise, this.onFulfilled, value);
    };
    QueueItem.prototype.callRejected = function(value) {
      handlers.reject(this.promise, value);
    };
    QueueItem.prototype.otherCallRejected = function(value) {
      unwrap(this.promise, this.onRejected, value);
    };
    function unwrap(promise, func, value) {
      immediate(function() {
        var returnValue;
        try {
          returnValue = func(value);
        } catch (e) {
          return handlers.reject(promise, e);
        }
        if (returnValue === promise) {
          handlers.reject(promise, new TypeError("Cannot resolve promise with itself"));
        } else {
          handlers.resolve(promise, returnValue);
        }
      });
    }
    handlers.resolve = function(self2, value) {
      var result = tryCatch(getThen, value);
      if (result.status === "error") {
        return handlers.reject(self2, result.value);
      }
      var thenable = result.value;
      if (thenable) {
        safelyResolveThenable(self2, thenable);
      } else {
        self2.state = FULFILLED;
        self2.outcome = value;
        var i = -1;
        var len = self2.queue.length;
        while (++i < len) {
          self2.queue[i].callFulfilled(value);
        }
      }
      return self2;
    };
    handlers.reject = function(self2, error) {
      self2.state = REJECTED;
      self2.outcome = error;
      if (!process.browser) {
        if (self2.handled === UNHANDLED) {
          immediate(function() {
            if (self2.handled === UNHANDLED) {
              process.emit("unhandledRejection", error, self2);
            }
          });
        }
      }
      var i = -1;
      var len = self2.queue.length;
      while (++i < len) {
        self2.queue[i].callRejected(error);
      }
      return self2;
    };
    function getThen(obj) {
      var then = obj && obj.then;
      if (obj && (typeof obj === "object" || typeof obj === "function") && typeof then === "function") {
        return function appyThen() {
          then.apply(obj, arguments);
        };
      }
    }
    function safelyResolveThenable(self2, thenable) {
      var called = false;
      function onError(value) {
        if (called) {
          return;
        }
        called = true;
        handlers.reject(self2, value);
      }
      function onSuccess(value) {
        if (called) {
          return;
        }
        called = true;
        handlers.resolve(self2, value);
      }
      function tryToUnwrap() {
        thenable(onSuccess, onError);
      }
      var result = tryCatch(tryToUnwrap);
      if (result.status === "error") {
        onError(result.value);
      }
    }
    function tryCatch(func, value) {
      var out = {};
      try {
        out.value = func(value);
        out.status = "success";
      } catch (e) {
        out.status = "error";
        out.value = e;
      }
      return out;
    }
    Promise2.resolve = resolve;
    function resolve(value) {
      if (value instanceof this) {
        return value;
      }
      return handlers.resolve(new this(INTERNAL), value);
    }
    Promise2.reject = reject;
    function reject(reason) {
      var promise = new this(INTERNAL);
      return handlers.reject(promise, reason);
    }
    Promise2.all = all;
    function all(iterable) {
      var self2 = this;
      if (Object.prototype.toString.call(iterable) !== "[object Array]") {
        return this.reject(new TypeError("must be an array"));
      }
      var len = iterable.length;
      var called = false;
      if (!len) {
        return this.resolve([]);
      }
      var values = new Array(len);
      var resolved = 0;
      var i = -1;
      var promise = new this(INTERNAL);
      while (++i < len) {
        allResolver(iterable[i], i);
      }
      return promise;
      function allResolver(value, i2) {
        self2.resolve(value).then(resolveFromAll, function(error) {
          if (!called) {
            called = true;
            handlers.reject(promise, error);
          }
        });
        function resolveFromAll(outValue) {
          values[i2] = outValue;
          if (++resolved === len && !called) {
            called = true;
            handlers.resolve(promise, values);
          }
        }
      }
    }
    Promise2.race = race;
    function race(iterable) {
      var self2 = this;
      if (Object.prototype.toString.call(iterable) !== "[object Array]") {
        return this.reject(new TypeError("must be an array"));
      }
      var len = iterable.length;
      var called = false;
      if (!len) {
        return this.resolve([]);
      }
      var i = -1;
      var promise = new this(INTERNAL);
      while (++i < len) {
        resolver(iterable[i]);
      }
      return promise;
      function resolver(value) {
        self2.resolve(value).then(function(response) {
          if (!called) {
            called = true;
            handlers.resolve(promise, response);
          }
        }, function(error) {
          if (!called) {
            called = true;
            handlers.reject(promise, error);
          }
        });
      }
    }
  }
});
var require_external = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/external.js"(exports, module) {
    "use strict";
    var ES6Promise = null;
    if (typeof Promise !== "undefined") {
      ES6Promise = Promise;
    } else {
      ES6Promise = require_lib2();
    }
    module.exports = {
      Promise: ES6Promise
    };
  }
});
var require_setImmediate = __commonJS({
  "../../node_modules/.pnpm/setimmediate@1.0.5/node_modules/setimmediate/setImmediate.js"(exports) {
    (function(global2, undefined2) {
      "use strict";
      if (global2.setImmediate) {
        return;
      }
      var nextHandle = 1;
      var tasksByHandle = {};
      var currentlyRunningATask = false;
      var doc = global2.document;
      var registerImmediate;
      function setImmediate2(callback) {
        if (typeof callback !== "function") {
          callback = new Function("" + callback);
        }
        var args = new Array(arguments.length - 1);
        for (var i = 0; i < args.length; i++) {
          args[i] = arguments[i + 1];
        }
        var task = { callback, args };
        tasksByHandle[nextHandle] = task;
        registerImmediate(nextHandle);
        return nextHandle++;
      }
      function clearImmediate(handle) {
        delete tasksByHandle[handle];
      }
      function run(task) {
        var callback = task.callback;
        var args = task.args;
        switch (args.length) {
          case 0:
            callback();
            break;
          case 1:
            callback(args[0]);
            break;
          case 2:
            callback(args[0], args[1]);
            break;
          case 3:
            callback(args[0], args[1], args[2]);
            break;
          default:
            callback.apply(undefined2, args);
            break;
        }
      }
      function runIfPresent(handle) {
        if (currentlyRunningATask) {
          setTimeout(runIfPresent, 0, handle);
        } else {
          var task = tasksByHandle[handle];
          if (task) {
            currentlyRunningATask = true;
            try {
              run(task);
            } finally {
              clearImmediate(handle);
              currentlyRunningATask = false;
            }
          }
        }
      }
      function installNextTickImplementation() {
        registerImmediate = function(handle) {
          process.nextTick(function() {
            runIfPresent(handle);
          });
        };
      }
      function canUsePostMessage() {
        if (global2.postMessage && !global2.importScripts) {
          var postMessageIsAsynchronous = true;
          var oldOnMessage = global2.onmessage;
          global2.onmessage = function() {
            postMessageIsAsynchronous = false;
          };
          global2.postMessage("", "*");
          global2.onmessage = oldOnMessage;
          return postMessageIsAsynchronous;
        }
      }
      function installPostMessageImplementation() {
        var messagePrefix = "setImmediate$" + Math.random() + "$";
        var onGlobalMessage = function(event) {
          if (event.source === global2 && typeof event.data === "string" && event.data.indexOf(messagePrefix) === 0) {
            runIfPresent(+event.data.slice(messagePrefix.length));
          }
        };
        if (global2.addEventListener) {
          global2.addEventListener("message", onGlobalMessage, false);
        } else {
          global2.attachEvent("onmessage", onGlobalMessage);
        }
        registerImmediate = function(handle) {
          global2.postMessage(messagePrefix + handle, "*");
        };
      }
      function installMessageChannelImplementation() {
        var channel = new MessageChannel();
        channel.port1.onmessage = function(event) {
          var handle = event.data;
          runIfPresent(handle);
        };
        registerImmediate = function(handle) {
          channel.port2.postMessage(handle);
        };
      }
      function installReadyStateChangeImplementation() {
        var html = doc.documentElement;
        registerImmediate = function(handle) {
          var script = doc.createElement("script");
          script.onreadystatechange = function() {
            runIfPresent(handle);
            script.onreadystatechange = null;
            html.removeChild(script);
            script = null;
          };
          html.appendChild(script);
        };
      }
      function installSetTimeoutImplementation() {
        registerImmediate = function(handle) {
          setTimeout(runIfPresent, 0, handle);
        };
      }
      var attachTo = Object.getPrototypeOf && Object.getPrototypeOf(global2);
      attachTo = attachTo && attachTo.setTimeout ? attachTo : global2;
      if ({}.toString.call(global2.process) === "[object process]") {
        installNextTickImplementation();
      } else if (canUsePostMessage()) {
        installPostMessageImplementation();
      } else if (global2.MessageChannel) {
        installMessageChannelImplementation();
      } else if (doc && "onreadystatechange" in doc.createElement("script")) {
        installReadyStateChangeImplementation();
      } else {
        installSetTimeoutImplementation();
      }
      attachTo.setImmediate = setImmediate2;
      attachTo.clearImmediate = clearImmediate;
    })(typeof self === "undefined" ? typeof global === "undefined" ? exports : global : self);
  }
});
var require_utils = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/utils.js"(exports) {
    "use strict";
    var support = require_support();
    var base64 = require_base64();
    var nodejsUtils = require_nodejsUtils();
    var external = require_external();
    require_setImmediate();
    function string2binary(str) {
      var result = null;
      if (support.uint8array) {
        result = new Uint8Array(str.length);
      } else {
        result = new Array(str.length);
      }
      return stringToArrayLike(str, result);
    }
    exports.newBlob = function(part, type) {
      exports.checkSupport("blob");
      try {
        return new Blob([part], {
          type
        });
      } catch (e) {
        try {
          var Builder2 = self.BlobBuilder || self.WebKitBlobBuilder || self.MozBlobBuilder || self.MSBlobBuilder;
          var builder = new Builder2();
          builder.append(part);
          return builder.getBlob(type);
        } catch (e2) {
          throw new Error("Bug : can't construct the Blob.");
        }
      }
    };
    function identity(input) {
      return input;
    }
    function stringToArrayLike(str, array) {
      for (var i = 0; i < str.length; ++i) {
        array[i] = str.charCodeAt(i) & 255;
      }
      return array;
    }
    var arrayToStringHelper = {
      /**
       * Transform an array of int into a string, chunk by chunk.
       * See the performances notes on arrayLikeToString.
       * @param {Array|ArrayBuffer|Uint8Array|Buffer} array the array to transform.
       * @param {String} type the type of the array.
       * @param {Integer} chunk the chunk size.
       * @return {String} the resulting string.
       * @throws Error if the chunk is too big for the stack.
       */
      stringifyByChunk: function(array, type, chunk) {
        var result = [], k = 0, len = array.length;
        if (len <= chunk) {
          return String.fromCharCode.apply(null, array);
        }
        while (k < len) {
          if (type === "array" || type === "nodebuffer") {
            result.push(String.fromCharCode.apply(null, array.slice(k, Math.min(k + chunk, len))));
          } else {
            result.push(String.fromCharCode.apply(null, array.subarray(k, Math.min(k + chunk, len))));
          }
          k += chunk;
        }
        return result.join("");
      },
      /**
       * Call String.fromCharCode on every item in the array.
       * This is the naive implementation, which generate A LOT of intermediate string.
       * This should be used when everything else fail.
       * @param {Array|ArrayBuffer|Uint8Array|Buffer} array the array to transform.
       * @return {String} the result.
       */
      stringifyByChar: function(array) {
        var resultStr = "";
        for (var i = 0; i < array.length; i++) {
          resultStr += String.fromCharCode(array[i]);
        }
        return resultStr;
      },
      applyCanBeUsed: {
        /**
         * true if the browser accepts to use String.fromCharCode on Uint8Array
         */
        uint8array: (function() {
          try {
            return support.uint8array && String.fromCharCode.apply(null, new Uint8Array(1)).length === 1;
          } catch (e) {
            return false;
          }
        })(),
        /**
         * true if the browser accepts to use String.fromCharCode on nodejs Buffer.
         */
        nodebuffer: (function() {
          try {
            return support.nodebuffer && String.fromCharCode.apply(null, nodejsUtils.allocBuffer(1)).length === 1;
          } catch (e) {
            return false;
          }
        })()
      }
    };
    function arrayLikeToString(array) {
      var chunk = 65536, type = exports.getTypeOf(array), canUseApply = true;
      if (type === "uint8array") {
        canUseApply = arrayToStringHelper.applyCanBeUsed.uint8array;
      } else if (type === "nodebuffer") {
        canUseApply = arrayToStringHelper.applyCanBeUsed.nodebuffer;
      }
      if (canUseApply) {
        while (chunk > 1) {
          try {
            return arrayToStringHelper.stringifyByChunk(array, type, chunk);
          } catch (e) {
            chunk = Math.floor(chunk / 2);
          }
        }
      }
      return arrayToStringHelper.stringifyByChar(array);
    }
    exports.applyFromCharCode = arrayLikeToString;
    function arrayLikeToArrayLike(arrayFrom, arrayTo) {
      for (var i = 0; i < arrayFrom.length; i++) {
        arrayTo[i] = arrayFrom[i];
      }
      return arrayTo;
    }
    var transform = {};
    transform["string"] = {
      "string": identity,
      "array": function(input) {
        return stringToArrayLike(input, new Array(input.length));
      },
      "arraybuffer": function(input) {
        return transform["string"]["uint8array"](input).buffer;
      },
      "uint8array": function(input) {
        return stringToArrayLike(input, new Uint8Array(input.length));
      },
      "nodebuffer": function(input) {
        return stringToArrayLike(input, nodejsUtils.allocBuffer(input.length));
      }
    };
    transform["array"] = {
      "string": arrayLikeToString,
      "array": identity,
      "arraybuffer": function(input) {
        return new Uint8Array(input).buffer;
      },
      "uint8array": function(input) {
        return new Uint8Array(input);
      },
      "nodebuffer": function(input) {
        return nodejsUtils.newBufferFrom(input);
      }
    };
    transform["arraybuffer"] = {
      "string": function(input) {
        return arrayLikeToString(new Uint8Array(input));
      },
      "array": function(input) {
        return arrayLikeToArrayLike(new Uint8Array(input), new Array(input.byteLength));
      },
      "arraybuffer": identity,
      "uint8array": function(input) {
        return new Uint8Array(input);
      },
      "nodebuffer": function(input) {
        return nodejsUtils.newBufferFrom(new Uint8Array(input));
      }
    };
    transform["uint8array"] = {
      "string": arrayLikeToString,
      "array": function(input) {
        return arrayLikeToArrayLike(input, new Array(input.length));
      },
      "arraybuffer": function(input) {
        return input.buffer;
      },
      "uint8array": identity,
      "nodebuffer": function(input) {
        return nodejsUtils.newBufferFrom(input);
      }
    };
    transform["nodebuffer"] = {
      "string": arrayLikeToString,
      "array": function(input) {
        return arrayLikeToArrayLike(input, new Array(input.length));
      },
      "arraybuffer": function(input) {
        return transform["nodebuffer"]["uint8array"](input).buffer;
      },
      "uint8array": function(input) {
        return arrayLikeToArrayLike(input, new Uint8Array(input.length));
      },
      "nodebuffer": identity
    };
    exports.transformTo = function(outputType, input) {
      if (!input) {
        input = "";
      }
      if (!outputType) {
        return input;
      }
      exports.checkSupport(outputType);
      var inputType = exports.getTypeOf(input);
      var result = transform[inputType][outputType](input);
      return result;
    };
    exports.resolve = function(path) {
      var parts = path.split("/");
      var result = [];
      for (var index = 0; index < parts.length; index++) {
        var part = parts[index];
        if (part === "." || part === "" && index !== 0 && index !== parts.length - 1) {
          continue;
        } else if (part === "..") {
          result.pop();
        } else {
          result.push(part);
        }
      }
      return result.join("/");
    };
    exports.getTypeOf = function(input) {
      if (typeof input === "string") {
        return "string";
      }
      if (Object.prototype.toString.call(input) === "[object Array]") {
        return "array";
      }
      if (support.nodebuffer && nodejsUtils.isBuffer(input)) {
        return "nodebuffer";
      }
      if (support.uint8array && input instanceof Uint8Array) {
        return "uint8array";
      }
      if (support.arraybuffer && input instanceof ArrayBuffer) {
        return "arraybuffer";
      }
    };
    exports.checkSupport = function(type) {
      var supported = support[type.toLowerCase()];
      if (!supported) {
        throw new Error(type + " is not supported by this platform");
      }
    };
    exports.MAX_VALUE_16BITS = 65535;
    exports.MAX_VALUE_32BITS = -1;
    exports.pretty = function(str) {
      var res = "", code, i;
      for (i = 0; i < (str || "").length; i++) {
        code = str.charCodeAt(i);
        res += "\\x" + (code < 16 ? "0" : "") + code.toString(16).toUpperCase();
      }
      return res;
    };
    exports.delay = function(callback, args, self2) {
      setImmediate(function() {
        callback.apply(self2 || null, args || []);
      });
    };
    exports.inherits = function(ctor, superCtor) {
      var Obj = function() {
      };
      Obj.prototype = superCtor.prototype;
      ctor.prototype = new Obj();
    };
    exports.extend = function() {
      var result = {}, i, attr2;
      for (i = 0; i < arguments.length; i++) {
        for (attr2 in arguments[i]) {
          if (Object.prototype.hasOwnProperty.call(arguments[i], attr2) && typeof result[attr2] === "undefined") {
            result[attr2] = arguments[i][attr2];
          }
        }
      }
      return result;
    };
    exports.prepareContent = function(name, inputData, isBinary, isOptimizedBinaryString, isBase64) {
      var promise = external.Promise.resolve(inputData).then(function(data) {
        var isBlob = support.blob && (data instanceof Blob || ["[object File]", "[object Blob]"].indexOf(Object.prototype.toString.call(data)) !== -1);
        if (isBlob && typeof FileReader !== "undefined") {
          return new external.Promise(function(resolve, reject) {
            var reader = new FileReader();
            reader.onload = function(e) {
              resolve(e.target.result);
            };
            reader.onerror = function(e) {
              reject(e.target.error);
            };
            reader.readAsArrayBuffer(data);
          });
        } else {
          return data;
        }
      });
      return promise.then(function(data) {
        var dataType = exports.getTypeOf(data);
        if (!dataType) {
          return external.Promise.reject(
            new Error("Can't read the data of '" + name + "'. Is it in a supported JavaScript type (String, Blob, ArrayBuffer, etc) ?")
          );
        }
        if (dataType === "arraybuffer") {
          data = exports.transformTo("uint8array", data);
        } else if (dataType === "string") {
          if (isBase64) {
            data = base64.decode(data);
          } else if (isBinary) {
            if (isOptimizedBinaryString !== true) {
              data = string2binary(data);
            }
          }
        }
        return data;
      });
    };
  }
});
var require_GenericWorker = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/GenericWorker.js"(exports, module) {
    "use strict";
    function GenericWorker(name) {
      this.name = name || "default";
      this.streamInfo = {};
      this.generatedError = null;
      this.extraStreamInfo = {};
      this.isPaused = true;
      this.isFinished = false;
      this.isLocked = false;
      this._listeners = {
        "data": [],
        "end": [],
        "error": []
      };
      this.previous = null;
    }
    GenericWorker.prototype = {
      /**
       * Push a chunk to the next workers.
       * @param {Object} chunk the chunk to push
       */
      push: function(chunk) {
        this.emit("data", chunk);
      },
      /**
       * End the stream.
       * @return {Boolean} true if this call ended the worker, false otherwise.
       */
      end: function() {
        if (this.isFinished) {
          return false;
        }
        this.flush();
        try {
          this.emit("end");
          this.cleanUp();
          this.isFinished = true;
        } catch (e) {
          this.emit("error", e);
        }
        return true;
      },
      /**
       * End the stream with an error.
       * @param {Error} e the error which caused the premature end.
       * @return {Boolean} true if this call ended the worker with an error, false otherwise.
       */
      error: function(e) {
        if (this.isFinished) {
          return false;
        }
        if (this.isPaused) {
          this.generatedError = e;
        } else {
          this.isFinished = true;
          this.emit("error", e);
          if (this.previous) {
            this.previous.error(e);
          }
          this.cleanUp();
        }
        return true;
      },
      /**
       * Add a callback on an event.
       * @param {String} name the name of the event (data, end, error)
       * @param {Function} listener the function to call when the event is triggered
       * @return {GenericWorker} the current object for chainability
       */
      on: function(name, listener) {
        this._listeners[name].push(listener);
        return this;
      },
      /**
       * Clean any references when a worker is ending.
       */
      cleanUp: function() {
        this.streamInfo = this.generatedError = this.extraStreamInfo = null;
        this._listeners = [];
      },
      /**
       * Trigger an event. This will call registered callback with the provided arg.
       * @param {String} name the name of the event (data, end, error)
       * @param {Object} arg the argument to call the callback with.
       */
      emit: function(name, arg) {
        if (this._listeners[name]) {
          for (var i = 0; i < this._listeners[name].length; i++) {
            this._listeners[name][i].call(this, arg);
          }
        }
      },
      /**
       * Chain a worker with an other.
       * @param {Worker} next the worker receiving events from the current one.
       * @return {worker} the next worker for chainability
       */
      pipe: function(next) {
        return next.registerPrevious(this);
      },
      /**
       * Same as `pipe` in the other direction.
       * Using an API with `pipe(next)` is very easy.
       * Implementing the API with the point of view of the next one registering
       * a source is easier, see the ZipFileWorker.
       * @param {Worker} previous the previous worker, sending events to this one
       * @return {Worker} the current worker for chainability
       */
      registerPrevious: function(previous) {
        if (this.isLocked) {
          throw new Error("The stream '" + this + "' has already been used.");
        }
        this.streamInfo = previous.streamInfo;
        this.mergeStreamInfo();
        this.previous = previous;
        var self2 = this;
        previous.on("data", function(chunk) {
          self2.processChunk(chunk);
        });
        previous.on("end", function() {
          self2.end();
        });
        previous.on("error", function(e) {
          self2.error(e);
        });
        return this;
      },
      /**
       * Pause the stream so it doesn't send events anymore.
       * @return {Boolean} true if this call paused the worker, false otherwise.
       */
      pause: function() {
        if (this.isPaused || this.isFinished) {
          return false;
        }
        this.isPaused = true;
        if (this.previous) {
          this.previous.pause();
        }
        return true;
      },
      /**
       * Resume a paused stream.
       * @return {Boolean} true if this call resumed the worker, false otherwise.
       */
      resume: function() {
        if (!this.isPaused || this.isFinished) {
          return false;
        }
        this.isPaused = false;
        var withError = false;
        if (this.generatedError) {
          this.error(this.generatedError);
          withError = true;
        }
        if (this.previous) {
          this.previous.resume();
        }
        return !withError;
      },
      /**
       * Flush any remaining bytes as the stream is ending.
       */
      flush: function() {
      },
      /**
       * Process a chunk. This is usually the method overridden.
       * @param {Object} chunk the chunk to process.
       */
      processChunk: function(chunk) {
        this.push(chunk);
      },
      /**
       * Add a key/value to be added in the workers chain streamInfo once activated.
       * @param {String} key the key to use
       * @param {Object} value the associated value
       * @return {Worker} the current worker for chainability
       */
      withStreamInfo: function(key, value) {
        this.extraStreamInfo[key] = value;
        this.mergeStreamInfo();
        return this;
      },
      /**
       * Merge this worker's streamInfo into the chain's streamInfo.
       */
      mergeStreamInfo: function() {
        for (var key in this.extraStreamInfo) {
          if (!Object.prototype.hasOwnProperty.call(this.extraStreamInfo, key)) {
            continue;
          }
          this.streamInfo[key] = this.extraStreamInfo[key];
        }
      },
      /**
       * Lock the stream to prevent further updates on the workers chain.
       * After calling this method, all calls to pipe will fail.
       */
      lock: function() {
        if (this.isLocked) {
          throw new Error("The stream '" + this + "' has already been used.");
        }
        this.isLocked = true;
        if (this.previous) {
          this.previous.lock();
        }
      },
      /**
       *
       * Pretty print the workers chain.
       */
      toString: function() {
        var me = "Worker " + this.name;
        if (this.previous) {
          return this.previous + " -> " + me;
        } else {
          return me;
        }
      }
    };
    module.exports = GenericWorker;
  }
});
var require_utf8 = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/utf8.js"(exports) {
    "use strict";
    var utils = require_utils();
    var support = require_support();
    var nodejsUtils = require_nodejsUtils();
    var GenericWorker = require_GenericWorker();
    var _utf8len = new Array(256);
    for (i = 0; i < 256; i++) {
      _utf8len[i] = i >= 252 ? 6 : i >= 248 ? 5 : i >= 240 ? 4 : i >= 224 ? 3 : i >= 192 ? 2 : 1;
    }
    var i;
    _utf8len[254] = _utf8len[254] = 1;
    var string2buf = function(str) {
      var buf, c, c2, m_pos, i2, str_len = str.length, buf_len = 0;
      for (m_pos = 0; m_pos < str_len; m_pos++) {
        c = str.charCodeAt(m_pos);
        if ((c & 64512) === 55296 && m_pos + 1 < str_len) {
          c2 = str.charCodeAt(m_pos + 1);
          if ((c2 & 64512) === 56320) {
            c = 65536 + (c - 55296 << 10) + (c2 - 56320);
            m_pos++;
          }
        }
        buf_len += c < 128 ? 1 : c < 2048 ? 2 : c < 65536 ? 3 : 4;
      }
      if (support.uint8array) {
        buf = new Uint8Array(buf_len);
      } else {
        buf = new Array(buf_len);
      }
      for (i2 = 0, m_pos = 0; i2 < buf_len; m_pos++) {
        c = str.charCodeAt(m_pos);
        if ((c & 64512) === 55296 && m_pos + 1 < str_len) {
          c2 = str.charCodeAt(m_pos + 1);
          if ((c2 & 64512) === 56320) {
            c = 65536 + (c - 55296 << 10) + (c2 - 56320);
            m_pos++;
          }
        }
        if (c < 128) {
          buf[i2++] = c;
        } else if (c < 2048) {
          buf[i2++] = 192 | c >>> 6;
          buf[i2++] = 128 | c & 63;
        } else if (c < 65536) {
          buf[i2++] = 224 | c >>> 12;
          buf[i2++] = 128 | c >>> 6 & 63;
          buf[i2++] = 128 | c & 63;
        } else {
          buf[i2++] = 240 | c >>> 18;
          buf[i2++] = 128 | c >>> 12 & 63;
          buf[i2++] = 128 | c >>> 6 & 63;
          buf[i2++] = 128 | c & 63;
        }
      }
      return buf;
    };
    var utf8border = function(buf, max) {
      var pos;
      max = max || buf.length;
      if (max > buf.length) {
        max = buf.length;
      }
      pos = max - 1;
      while (pos >= 0 && (buf[pos] & 192) === 128) {
        pos--;
      }
      if (pos < 0) {
        return max;
      }
      if (pos === 0) {
        return max;
      }
      return pos + _utf8len[buf[pos]] > max ? pos : max;
    };
    var buf2string = function(buf) {
      var i2, out, c, c_len;
      var len = buf.length;
      var utf16buf = new Array(len * 2);
      for (out = 0, i2 = 0; i2 < len; ) {
        c = buf[i2++];
        if (c < 128) {
          utf16buf[out++] = c;
          continue;
        }
        c_len = _utf8len[c];
        if (c_len > 4) {
          utf16buf[out++] = 65533;
          i2 += c_len - 1;
          continue;
        }
        c &= c_len === 2 ? 31 : c_len === 3 ? 15 : 7;
        while (c_len > 1 && i2 < len) {
          c = c << 6 | buf[i2++] & 63;
          c_len--;
        }
        if (c_len > 1) {
          utf16buf[out++] = 65533;
          continue;
        }
        if (c < 65536) {
          utf16buf[out++] = c;
        } else {
          c -= 65536;
          utf16buf[out++] = 55296 | c >> 10 & 1023;
          utf16buf[out++] = 56320 | c & 1023;
        }
      }
      if (utf16buf.length !== out) {
        if (utf16buf.subarray) {
          utf16buf = utf16buf.subarray(0, out);
        } else {
          utf16buf.length = out;
        }
      }
      return utils.applyFromCharCode(utf16buf);
    };
    exports.utf8encode = function utf8encode(str) {
      if (support.nodebuffer) {
        return nodejsUtils.newBufferFrom(str, "utf-8");
      }
      return string2buf(str);
    };
    exports.utf8decode = function utf8decode(buf) {
      if (support.nodebuffer) {
        return utils.transformTo("nodebuffer", buf).toString("utf-8");
      }
      buf = utils.transformTo(support.uint8array ? "uint8array" : "array", buf);
      return buf2string(buf);
    };
    function Utf8DecodeWorker() {
      GenericWorker.call(this, "utf-8 decode");
      this.leftOver = null;
    }
    utils.inherits(Utf8DecodeWorker, GenericWorker);
    Utf8DecodeWorker.prototype.processChunk = function(chunk) {
      var data = utils.transformTo(support.uint8array ? "uint8array" : "array", chunk.data);
      if (this.leftOver && this.leftOver.length) {
        if (support.uint8array) {
          var previousData = data;
          data = new Uint8Array(previousData.length + this.leftOver.length);
          data.set(this.leftOver, 0);
          data.set(previousData, this.leftOver.length);
        } else {
          data = this.leftOver.concat(data);
        }
        this.leftOver = null;
      }
      var nextBoundary = utf8border(data);
      var usableData = data;
      if (nextBoundary !== data.length) {
        if (support.uint8array) {
          usableData = data.subarray(0, nextBoundary);
          this.leftOver = data.subarray(nextBoundary, data.length);
        } else {
          usableData = data.slice(0, nextBoundary);
          this.leftOver = data.slice(nextBoundary, data.length);
        }
      }
      this.push({
        data: exports.utf8decode(usableData),
        meta: chunk.meta
      });
    };
    Utf8DecodeWorker.prototype.flush = function() {
      if (this.leftOver && this.leftOver.length) {
        this.push({
          data: exports.utf8decode(this.leftOver),
          meta: {}
        });
        this.leftOver = null;
      }
    };
    exports.Utf8DecodeWorker = Utf8DecodeWorker;
    function Utf8EncodeWorker() {
      GenericWorker.call(this, "utf-8 encode");
    }
    utils.inherits(Utf8EncodeWorker, GenericWorker);
    Utf8EncodeWorker.prototype.processChunk = function(chunk) {
      this.push({
        data: exports.utf8encode(chunk.data),
        meta: chunk.meta
      });
    };
    exports.Utf8EncodeWorker = Utf8EncodeWorker;
  }
});
var require_ConvertWorker = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/ConvertWorker.js"(exports, module) {
    "use strict";
    var GenericWorker = require_GenericWorker();
    var utils = require_utils();
    function ConvertWorker(destType) {
      GenericWorker.call(this, "ConvertWorker to " + destType);
      this.destType = destType;
    }
    utils.inherits(ConvertWorker, GenericWorker);
    ConvertWorker.prototype.processChunk = function(chunk) {
      this.push({
        data: utils.transformTo(this.destType, chunk.data),
        meta: chunk.meta
      });
    };
    module.exports = ConvertWorker;
  }
});
var require_NodejsStreamOutputAdapter = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/nodejs/NodejsStreamOutputAdapter.js"(exports, module) {
    "use strict";
    var Readable = require_readable().Readable;
    var utils = require_utils();
    utils.inherits(NodejsStreamOutputAdapter, Readable);
    function NodejsStreamOutputAdapter(helper, options, updateCb) {
      Readable.call(this, options);
      this._helper = helper;
      var self2 = this;
      helper.on("data", function(data, meta) {
        if (!self2.push(data)) {
          self2._helper.pause();
        }
        if (updateCb) {
          updateCb(meta);
        }
      }).on("error", function(e) {
        self2.emit("error", e);
      }).on("end", function() {
        self2.push(null);
      });
    }
    NodejsStreamOutputAdapter.prototype._read = function() {
      this._helper.resume();
    };
    module.exports = NodejsStreamOutputAdapter;
  }
});
var require_StreamHelper = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/StreamHelper.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var ConvertWorker = require_ConvertWorker();
    var GenericWorker = require_GenericWorker();
    var base64 = require_base64();
    var support = require_support();
    var external = require_external();
    var NodejsStreamOutputAdapter = null;
    if (support.nodestream) {
      try {
        NodejsStreamOutputAdapter = require_NodejsStreamOutputAdapter();
      } catch (e) {
      }
    }
    function transformZipOutput(type, content, mimeType) {
      switch (type) {
        case "blob":
          return utils.newBlob(utils.transformTo("arraybuffer", content), mimeType);
        case "base64":
          return base64.encode(content);
        default:
          return utils.transformTo(type, content);
      }
    }
    function concat(type, dataArray) {
      var i, index = 0, res = null, totalLength = 0;
      for (i = 0; i < dataArray.length; i++) {
        totalLength += dataArray[i].length;
      }
      switch (type) {
        case "string":
          return dataArray.join("");
        case "array":
          return Array.prototype.concat.apply([], dataArray);
        case "uint8array":
          res = new Uint8Array(totalLength);
          for (i = 0; i < dataArray.length; i++) {
            res.set(dataArray[i], index);
            index += dataArray[i].length;
          }
          return res;
        case "nodebuffer":
          return Buffer.concat(dataArray);
        default:
          throw new Error("concat : unsupported type '" + type + "'");
      }
    }
    function accumulate(helper, updateCallback) {
      return new external.Promise(function(resolve, reject) {
        var dataArray = [];
        var chunkType = helper._internalType, resultType = helper._outputType, mimeType = helper._mimeType;
        helper.on("data", function(data, meta) {
          dataArray.push(data);
          if (updateCallback) {
            updateCallback(meta);
          }
        }).on("error", function(err) {
          dataArray = [];
          reject(err);
        }).on("end", function() {
          try {
            var result = transformZipOutput(resultType, concat(chunkType, dataArray), mimeType);
            resolve(result);
          } catch (e) {
            reject(e);
          }
          dataArray = [];
        }).resume();
      });
    }
    function StreamHelper(worker, outputType, mimeType) {
      var internalType = outputType;
      switch (outputType) {
        case "blob":
        case "arraybuffer":
          internalType = "uint8array";
          break;
        case "base64":
          internalType = "string";
          break;
      }
      try {
        this._internalType = internalType;
        this._outputType = outputType;
        this._mimeType = mimeType;
        utils.checkSupport(internalType);
        this._worker = worker.pipe(new ConvertWorker(internalType));
        worker.lock();
      } catch (e) {
        this._worker = new GenericWorker("error");
        this._worker.error(e);
      }
    }
    StreamHelper.prototype = {
      /**
       * Listen a StreamHelper, accumulate its content and concatenate it into a
       * complete block.
       * @param {Function} updateCb the update callback.
       * @return Promise the promise for the accumulation.
       */
      accumulate: function(updateCb) {
        return accumulate(this, updateCb);
      },
      /**
       * Add a listener on an event triggered on a stream.
       * @param {String} evt the name of the event
       * @param {Function} fn the listener
       * @return {StreamHelper} the current helper.
       */
      on: function(evt, fn) {
        var self2 = this;
        if (evt === "data") {
          this._worker.on(evt, function(chunk) {
            fn.call(self2, chunk.data, chunk.meta);
          });
        } else {
          this._worker.on(evt, function() {
            utils.delay(fn, arguments, self2);
          });
        }
        return this;
      },
      /**
       * Resume the flow of chunks.
       * @return {StreamHelper} the current helper.
       */
      resume: function() {
        utils.delay(this._worker.resume, [], this._worker);
        return this;
      },
      /**
       * Pause the flow of chunks.
       * @return {StreamHelper} the current helper.
       */
      pause: function() {
        this._worker.pause();
        return this;
      },
      /**
       * Return a nodejs stream for this helper.
       * @param {Function} updateCb the update callback.
       * @return {NodejsStreamOutputAdapter} the nodejs stream.
       */
      toNodejsStream: function(updateCb) {
        utils.checkSupport("nodestream");
        if (this._outputType !== "nodebuffer") {
          throw new Error(this._outputType + " is not supported by this method");
        }
        return new NodejsStreamOutputAdapter(this, {
          objectMode: this._outputType !== "nodebuffer"
        }, updateCb);
      }
    };
    module.exports = StreamHelper;
  }
});
var require_defaults = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/defaults.js"(exports) {
    "use strict";
    exports.base64 = false;
    exports.binary = false;
    exports.dir = false;
    exports.createFolders = true;
    exports.date = null;
    exports.compression = null;
    exports.compressionOptions = null;
    exports.comment = null;
    exports.unixPermissions = null;
    exports.dosPermissions = null;
  }
});
var require_DataWorker = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/DataWorker.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    var DEFAULT_BLOCK_SIZE = 16 * 1024;
    function DataWorker(dataP) {
      GenericWorker.call(this, "DataWorker");
      var self2 = this;
      this.dataIsReady = false;
      this.index = 0;
      this.max = 0;
      this.data = null;
      this.type = "";
      this._tickScheduled = false;
      dataP.then(function(data) {
        self2.dataIsReady = true;
        self2.data = data;
        self2.max = data && data.length || 0;
        self2.type = utils.getTypeOf(data);
        if (!self2.isPaused) {
          self2._tickAndRepeat();
        }
      }, function(e) {
        self2.error(e);
      });
    }
    utils.inherits(DataWorker, GenericWorker);
    DataWorker.prototype.cleanUp = function() {
      GenericWorker.prototype.cleanUp.call(this);
      this.data = null;
    };
    DataWorker.prototype.resume = function() {
      if (!GenericWorker.prototype.resume.call(this)) {
        return false;
      }
      if (!this._tickScheduled && this.dataIsReady) {
        this._tickScheduled = true;
        utils.delay(this._tickAndRepeat, [], this);
      }
      return true;
    };
    DataWorker.prototype._tickAndRepeat = function() {
      this._tickScheduled = false;
      if (this.isPaused || this.isFinished) {
        return;
      }
      this._tick();
      if (!this.isFinished) {
        utils.delay(this._tickAndRepeat, [], this);
        this._tickScheduled = true;
      }
    };
    DataWorker.prototype._tick = function() {
      if (this.isPaused || this.isFinished) {
        return false;
      }
      var size = DEFAULT_BLOCK_SIZE;
      var data = null, nextIndex = Math.min(this.max, this.index + size);
      if (this.index >= this.max) {
        return this.end();
      } else {
        switch (this.type) {
          case "string":
            data = this.data.substring(this.index, nextIndex);
            break;
          case "uint8array":
            data = this.data.subarray(this.index, nextIndex);
            break;
          case "array":
          case "nodebuffer":
            data = this.data.slice(this.index, nextIndex);
            break;
        }
        this.index = nextIndex;
        return this.push({
          data,
          meta: {
            percent: this.max ? this.index / this.max * 100 : 0
          }
        });
      }
    };
    module.exports = DataWorker;
  }
});
var require_crc32 = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/crc32.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    function makeTable() {
      var c, table = [];
      for (var n = 0; n < 256; n++) {
        c = n;
        for (var k = 0; k < 8; k++) {
          c = c & 1 ? 3988292384 ^ c >>> 1 : c >>> 1;
        }
        table[n] = c;
      }
      return table;
    }
    var crcTable = makeTable();
    function crc32(crc, buf, len, pos) {
      var t = crcTable, end = pos + len;
      crc = crc ^ -1;
      for (var i = pos; i < end; i++) {
        crc = crc >>> 8 ^ t[(crc ^ buf[i]) & 255];
      }
      return crc ^ -1;
    }
    function crc32str(crc, str, len, pos) {
      var t = crcTable, end = pos + len;
      crc = crc ^ -1;
      for (var i = pos; i < end; i++) {
        crc = crc >>> 8 ^ t[(crc ^ str.charCodeAt(i)) & 255];
      }
      return crc ^ -1;
    }
    module.exports = function crc32wrapper(input, crc) {
      if (typeof input === "undefined" || !input.length) {
        return 0;
      }
      var isArray = utils.getTypeOf(input) !== "string";
      if (isArray) {
        return crc32(crc | 0, input, input.length, 0);
      } else {
        return crc32str(crc | 0, input, input.length, 0);
      }
    };
  }
});
var require_Crc32Probe = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/Crc32Probe.js"(exports, module) {
    "use strict";
    var GenericWorker = require_GenericWorker();
    var crc32 = require_crc32();
    var utils = require_utils();
    function Crc32Probe() {
      GenericWorker.call(this, "Crc32Probe");
      this.withStreamInfo("crc32", 0);
    }
    utils.inherits(Crc32Probe, GenericWorker);
    Crc32Probe.prototype.processChunk = function(chunk) {
      this.streamInfo.crc32 = crc32(chunk.data, this.streamInfo.crc32 || 0);
      this.push(chunk);
    };
    module.exports = Crc32Probe;
  }
});
var require_DataLengthProbe = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/stream/DataLengthProbe.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    function DataLengthProbe(propName3) {
      GenericWorker.call(this, "DataLengthProbe for " + propName3);
      this.propName = propName3;
      this.withStreamInfo(propName3, 0);
    }
    utils.inherits(DataLengthProbe, GenericWorker);
    DataLengthProbe.prototype.processChunk = function(chunk) {
      if (chunk) {
        var length = this.streamInfo[this.propName] || 0;
        this.streamInfo[this.propName] = length + chunk.data.length;
      }
      GenericWorker.prototype.processChunk.call(this, chunk);
    };
    module.exports = DataLengthProbe;
  }
});
var require_compressedObject = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/compressedObject.js"(exports, module) {
    "use strict";
    var external = require_external();
    var DataWorker = require_DataWorker();
    var Crc32Probe = require_Crc32Probe();
    var DataLengthProbe = require_DataLengthProbe();
    function CompressedObject(compressedSize, uncompressedSize, crc32, compression, data) {
      this.compressedSize = compressedSize;
      this.uncompressedSize = uncompressedSize;
      this.crc32 = crc32;
      this.compression = compression;
      this.compressedContent = data;
    }
    CompressedObject.prototype = {
      /**
       * Create a worker to get the uncompressed content.
       * @return {GenericWorker} the worker.
       */
      getContentWorker: function() {
        var worker = new DataWorker(external.Promise.resolve(this.compressedContent)).pipe(this.compression.uncompressWorker()).pipe(new DataLengthProbe("data_length"));
        var that = this;
        worker.on("end", function() {
          if (this.streamInfo["data_length"] !== that.uncompressedSize) {
            throw new Error("Bug : uncompressed data size mismatch");
          }
        });
        return worker;
      },
      /**
       * Create a worker to get the compressed content.
       * @return {GenericWorker} the worker.
       */
      getCompressedWorker: function() {
        return new DataWorker(external.Promise.resolve(this.compressedContent)).withStreamInfo("compressedSize", this.compressedSize).withStreamInfo("uncompressedSize", this.uncompressedSize).withStreamInfo("crc32", this.crc32).withStreamInfo("compression", this.compression);
      }
    };
    CompressedObject.createWorkerFrom = function(uncompressedWorker, compression, compressionOptions) {
      return uncompressedWorker.pipe(new Crc32Probe()).pipe(new DataLengthProbe("uncompressedSize")).pipe(compression.compressWorker(compressionOptions)).pipe(new DataLengthProbe("compressedSize")).withStreamInfo("compression", compression);
    };
    module.exports = CompressedObject;
  }
});
var require_zipObject = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/zipObject.js"(exports, module) {
    "use strict";
    var StreamHelper = require_StreamHelper();
    var DataWorker = require_DataWorker();
    var utf8 = require_utf8();
    var CompressedObject = require_compressedObject();
    var GenericWorker = require_GenericWorker();
    var ZipObject = function(name, data, options) {
      this.name = name;
      this.dir = options.dir;
      this.date = options.date;
      this.comment = options.comment;
      this.unixPermissions = options.unixPermissions;
      this.dosPermissions = options.dosPermissions;
      this._data = data;
      this._dataBinary = options.binary;
      this.options = {
        compression: options.compression,
        compressionOptions: options.compressionOptions
      };
    };
    ZipObject.prototype = {
      /**
       * Create an internal stream for the content of this object.
       * @param {String} type the type of each chunk.
       * @return StreamHelper the stream.
       */
      internalStream: function(type) {
        var result = null, outputType = "string";
        try {
          if (!type) {
            throw new Error("No output type specified.");
          }
          outputType = type.toLowerCase();
          var askUnicodeString = outputType === "string" || outputType === "text";
          if (outputType === "binarystring" || outputType === "text") {
            outputType = "string";
          }
          result = this._decompressWorker();
          var isUnicodeString = !this._dataBinary;
          if (isUnicodeString && !askUnicodeString) {
            result = result.pipe(new utf8.Utf8EncodeWorker());
          }
          if (!isUnicodeString && askUnicodeString) {
            result = result.pipe(new utf8.Utf8DecodeWorker());
          }
        } catch (e) {
          result = new GenericWorker("error");
          result.error(e);
        }
        return new StreamHelper(result, outputType, "");
      },
      /**
       * Prepare the content in the asked type.
       * @param {String} type the type of the result.
       * @param {Function} onUpdate a function to call on each internal update.
       * @return Promise the promise of the result.
       */
      async: function(type, onUpdate) {
        return this.internalStream(type).accumulate(onUpdate);
      },
      /**
       * Prepare the content as a nodejs stream.
       * @param {String} type the type of each chunk.
       * @param {Function} onUpdate a function to call on each internal update.
       * @return Stream the stream.
       */
      nodeStream: function(type, onUpdate) {
        return this.internalStream(type || "nodebuffer").toNodejsStream(onUpdate);
      },
      /**
       * Return a worker for the compressed content.
       * @private
       * @param {Object} compression the compression object to use.
       * @param {Object} compressionOptions the options to use when compressing.
       * @return Worker the worker.
       */
      _compressWorker: function(compression, compressionOptions) {
        if (this._data instanceof CompressedObject && this._data.compression.magic === compression.magic) {
          return this._data.getCompressedWorker();
        } else {
          var result = this._decompressWorker();
          if (!this._dataBinary) {
            result = result.pipe(new utf8.Utf8EncodeWorker());
          }
          return CompressedObject.createWorkerFrom(result, compression, compressionOptions);
        }
      },
      /**
       * Return a worker for the decompressed content.
       * @private
       * @return Worker the worker.
       */
      _decompressWorker: function() {
        if (this._data instanceof CompressedObject) {
          return this._data.getContentWorker();
        } else if (this._data instanceof GenericWorker) {
          return this._data;
        } else {
          return new DataWorker(this._data);
        }
      }
    };
    var removedMethods = ["asText", "asBinary", "asNodeBuffer", "asUint8Array", "asArrayBuffer"];
    var removedFn = function() {
      throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
    };
    for (i = 0; i < removedMethods.length; i++) {
      ZipObject.prototype[removedMethods[i]] = removedFn;
    }
    var i;
    module.exports = ZipObject;
  }
});
var require_common = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/utils/common.js"(exports) {
    "use strict";
    var TYPED_OK = typeof Uint8Array !== "undefined" && typeof Uint16Array !== "undefined" && typeof Int32Array !== "undefined";
    function _has(obj, key) {
      return Object.prototype.hasOwnProperty.call(obj, key);
    }
    exports.assign = function(obj) {
      var sources = Array.prototype.slice.call(arguments, 1);
      while (sources.length) {
        var source = sources.shift();
        if (!source) {
          continue;
        }
        if (typeof source !== "object") {
          throw new TypeError(source + "must be non-object");
        }
        for (var p in source) {
          if (_has(source, p)) {
            obj[p] = source[p];
          }
        }
      }
      return obj;
    };
    exports.shrinkBuf = function(buf, size) {
      if (buf.length === size) {
        return buf;
      }
      if (buf.subarray) {
        return buf.subarray(0, size);
      }
      buf.length = size;
      return buf;
    };
    var fnTyped = {
      arraySet: function(dest, src, src_offs, len, dest_offs) {
        if (src.subarray && dest.subarray) {
          dest.set(src.subarray(src_offs, src_offs + len), dest_offs);
          return;
        }
        for (var i = 0; i < len; i++) {
          dest[dest_offs + i] = src[src_offs + i];
        }
      },
      // Join array of chunks to single array.
      flattenChunks: function(chunks) {
        var i, l, len, pos, chunk, result;
        len = 0;
        for (i = 0, l = chunks.length; i < l; i++) {
          len += chunks[i].length;
        }
        result = new Uint8Array(len);
        pos = 0;
        for (i = 0, l = chunks.length; i < l; i++) {
          chunk = chunks[i];
          result.set(chunk, pos);
          pos += chunk.length;
        }
        return result;
      }
    };
    var fnUntyped = {
      arraySet: function(dest, src, src_offs, len, dest_offs) {
        for (var i = 0; i < len; i++) {
          dest[dest_offs + i] = src[src_offs + i];
        }
      },
      // Join array of chunks to single array.
      flattenChunks: function(chunks) {
        return [].concat.apply([], chunks);
      }
    };
    exports.setTyped = function(on) {
      if (on) {
        exports.Buf8 = Uint8Array;
        exports.Buf16 = Uint16Array;
        exports.Buf32 = Int32Array;
        exports.assign(exports, fnTyped);
      } else {
        exports.Buf8 = Array;
        exports.Buf16 = Array;
        exports.Buf32 = Array;
        exports.assign(exports, fnUntyped);
      }
    };
    exports.setTyped(TYPED_OK);
  }
});
var require_trees = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/trees.js"(exports) {
    "use strict";
    var utils = require_common();
    var Z_FIXED = 4;
    var Z_BINARY = 0;
    var Z_TEXT = 1;
    var Z_UNKNOWN = 2;
    function zero(buf) {
      var len = buf.length;
      while (--len >= 0) {
        buf[len] = 0;
      }
    }
    var STORED_BLOCK = 0;
    var STATIC_TREES = 1;
    var DYN_TREES = 2;
    var MIN_MATCH = 3;
    var MAX_MATCH = 258;
    var LENGTH_CODES = 29;
    var LITERALS = 256;
    var L_CODES = LITERALS + 1 + LENGTH_CODES;
    var D_CODES = 30;
    var BL_CODES = 19;
    var HEAP_SIZE = 2 * L_CODES + 1;
    var MAX_BITS = 15;
    var Buf_size = 16;
    var MAX_BL_BITS = 7;
    var END_BLOCK = 256;
    var REP_3_6 = 16;
    var REPZ_3_10 = 17;
    var REPZ_11_138 = 18;
    var extra_lbits = (
      /* extra bits for each length code */
      [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0]
    );
    var extra_dbits = (
      /* extra bits for each distance code */
      [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13]
    );
    var extra_blbits = (
      /* extra bits for each bit length code */
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7]
    );
    var bl_order = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    var DIST_CODE_LEN = 512;
    var static_ltree = new Array((L_CODES + 2) * 2);
    zero(static_ltree);
    var static_dtree = new Array(D_CODES * 2);
    zero(static_dtree);
    var _dist_code = new Array(DIST_CODE_LEN);
    zero(_dist_code);
    var _length_code = new Array(MAX_MATCH - MIN_MATCH + 1);
    zero(_length_code);
    var base_length = new Array(LENGTH_CODES);
    zero(base_length);
    var base_dist = new Array(D_CODES);
    zero(base_dist);
    function StaticTreeDesc(static_tree, extra_bits, extra_base, elems, max_length) {
      this.static_tree = static_tree;
      this.extra_bits = extra_bits;
      this.extra_base = extra_base;
      this.elems = elems;
      this.max_length = max_length;
      this.has_stree = static_tree && static_tree.length;
    }
    var static_l_desc;
    var static_d_desc;
    var static_bl_desc;
    function TreeDesc(dyn_tree, stat_desc) {
      this.dyn_tree = dyn_tree;
      this.max_code = 0;
      this.stat_desc = stat_desc;
    }
    function d_code(dist) {
      return dist < 256 ? _dist_code[dist] : _dist_code[256 + (dist >>> 7)];
    }
    function put_short(s, w) {
      s.pending_buf[s.pending++] = w & 255;
      s.pending_buf[s.pending++] = w >>> 8 & 255;
    }
    function send_bits(s, value, length) {
      if (s.bi_valid > Buf_size - length) {
        s.bi_buf |= value << s.bi_valid & 65535;
        put_short(s, s.bi_buf);
        s.bi_buf = value >> Buf_size - s.bi_valid;
        s.bi_valid += length - Buf_size;
      } else {
        s.bi_buf |= value << s.bi_valid & 65535;
        s.bi_valid += length;
      }
    }
    function send_code(s, c, tree) {
      send_bits(
        s,
        tree[c * 2],
        tree[c * 2 + 1]
        /*.Len*/
      );
    }
    function bi_reverse(code, len) {
      var res = 0;
      do {
        res |= code & 1;
        code >>>= 1;
        res <<= 1;
      } while (--len > 0);
      return res >>> 1;
    }
    function bi_flush(s) {
      if (s.bi_valid === 16) {
        put_short(s, s.bi_buf);
        s.bi_buf = 0;
        s.bi_valid = 0;
      } else if (s.bi_valid >= 8) {
        s.pending_buf[s.pending++] = s.bi_buf & 255;
        s.bi_buf >>= 8;
        s.bi_valid -= 8;
      }
    }
    function gen_bitlen(s, desc) {
      var tree = desc.dyn_tree;
      var max_code = desc.max_code;
      var stree = desc.stat_desc.static_tree;
      var has_stree = desc.stat_desc.has_stree;
      var extra = desc.stat_desc.extra_bits;
      var base = desc.stat_desc.extra_base;
      var max_length = desc.stat_desc.max_length;
      var h;
      var n, m;
      var bits;
      var xbits;
      var f;
      var overflow = 0;
      for (bits = 0; bits <= MAX_BITS; bits++) {
        s.bl_count[bits] = 0;
      }
      tree[s.heap[s.heap_max] * 2 + 1] = 0;
      for (h = s.heap_max + 1; h < HEAP_SIZE; h++) {
        n = s.heap[h];
        bits = tree[tree[n * 2 + 1] * 2 + 1] + 1;
        if (bits > max_length) {
          bits = max_length;
          overflow++;
        }
        tree[n * 2 + 1] = bits;
        if (n > max_code) {
          continue;
        }
        s.bl_count[bits]++;
        xbits = 0;
        if (n >= base) {
          xbits = extra[n - base];
        }
        f = tree[n * 2];
        s.opt_len += f * (bits + xbits);
        if (has_stree) {
          s.static_len += f * (stree[n * 2 + 1] + xbits);
        }
      }
      if (overflow === 0) {
        return;
      }
      do {
        bits = max_length - 1;
        while (s.bl_count[bits] === 0) {
          bits--;
        }
        s.bl_count[bits]--;
        s.bl_count[bits + 1] += 2;
        s.bl_count[max_length]--;
        overflow -= 2;
      } while (overflow > 0);
      for (bits = max_length; bits !== 0; bits--) {
        n = s.bl_count[bits];
        while (n !== 0) {
          m = s.heap[--h];
          if (m > max_code) {
            continue;
          }
          if (tree[m * 2 + 1] !== bits) {
            s.opt_len += (bits - tree[m * 2 + 1]) * tree[m * 2];
            tree[m * 2 + 1] = bits;
          }
          n--;
        }
      }
    }
    function gen_codes(tree, max_code, bl_count) {
      var next_code = new Array(MAX_BITS + 1);
      var code = 0;
      var bits;
      var n;
      for (bits = 1; bits <= MAX_BITS; bits++) {
        next_code[bits] = code = code + bl_count[bits - 1] << 1;
      }
      for (n = 0; n <= max_code; n++) {
        var len = tree[n * 2 + 1];
        if (len === 0) {
          continue;
        }
        tree[n * 2] = bi_reverse(next_code[len]++, len);
      }
    }
    function tr_static_init() {
      var n;
      var bits;
      var length;
      var code;
      var dist;
      var bl_count = new Array(MAX_BITS + 1);
      length = 0;
      for (code = 0; code < LENGTH_CODES - 1; code++) {
        base_length[code] = length;
        for (n = 0; n < 1 << extra_lbits[code]; n++) {
          _length_code[length++] = code;
        }
      }
      _length_code[length - 1] = code;
      dist = 0;
      for (code = 0; code < 16; code++) {
        base_dist[code] = dist;
        for (n = 0; n < 1 << extra_dbits[code]; n++) {
          _dist_code[dist++] = code;
        }
      }
      dist >>= 7;
      for (; code < D_CODES; code++) {
        base_dist[code] = dist << 7;
        for (n = 0; n < 1 << extra_dbits[code] - 7; n++) {
          _dist_code[256 + dist++] = code;
        }
      }
      for (bits = 0; bits <= MAX_BITS; bits++) {
        bl_count[bits] = 0;
      }
      n = 0;
      while (n <= 143) {
        static_ltree[n * 2 + 1] = 8;
        n++;
        bl_count[8]++;
      }
      while (n <= 255) {
        static_ltree[n * 2 + 1] = 9;
        n++;
        bl_count[9]++;
      }
      while (n <= 279) {
        static_ltree[n * 2 + 1] = 7;
        n++;
        bl_count[7]++;
      }
      while (n <= 287) {
        static_ltree[n * 2 + 1] = 8;
        n++;
        bl_count[8]++;
      }
      gen_codes(static_ltree, L_CODES + 1, bl_count);
      for (n = 0; n < D_CODES; n++) {
        static_dtree[n * 2 + 1] = 5;
        static_dtree[n * 2] = bi_reverse(n, 5);
      }
      static_l_desc = new StaticTreeDesc(static_ltree, extra_lbits, LITERALS + 1, L_CODES, MAX_BITS);
      static_d_desc = new StaticTreeDesc(static_dtree, extra_dbits, 0, D_CODES, MAX_BITS);
      static_bl_desc = new StaticTreeDesc(new Array(0), extra_blbits, 0, BL_CODES, MAX_BL_BITS);
    }
    function init_block(s) {
      var n;
      for (n = 0; n < L_CODES; n++) {
        s.dyn_ltree[n * 2] = 0;
      }
      for (n = 0; n < D_CODES; n++) {
        s.dyn_dtree[n * 2] = 0;
      }
      for (n = 0; n < BL_CODES; n++) {
        s.bl_tree[n * 2] = 0;
      }
      s.dyn_ltree[END_BLOCK * 2] = 1;
      s.opt_len = s.static_len = 0;
      s.last_lit = s.matches = 0;
    }
    function bi_windup(s) {
      if (s.bi_valid > 8) {
        put_short(s, s.bi_buf);
      } else if (s.bi_valid > 0) {
        s.pending_buf[s.pending++] = s.bi_buf;
      }
      s.bi_buf = 0;
      s.bi_valid = 0;
    }
    function copy_block(s, buf, len, header) {
      bi_windup(s);
      if (header) {
        put_short(s, len);
        put_short(s, ~len);
      }
      utils.arraySet(s.pending_buf, s.window, buf, len, s.pending);
      s.pending += len;
    }
    function smaller(tree, n, m, depth) {
      var _n2 = n * 2;
      var _m2 = m * 2;
      return tree[_n2] < tree[_m2] || tree[_n2] === tree[_m2] && depth[n] <= depth[m];
    }
    function pqdownheap(s, tree, k) {
      var v = s.heap[k];
      var j = k << 1;
      while (j <= s.heap_len) {
        if (j < s.heap_len && smaller(tree, s.heap[j + 1], s.heap[j], s.depth)) {
          j++;
        }
        if (smaller(tree, v, s.heap[j], s.depth)) {
          break;
        }
        s.heap[k] = s.heap[j];
        k = j;
        j <<= 1;
      }
      s.heap[k] = v;
    }
    function compress_block(s, ltree, dtree) {
      var dist;
      var lc;
      var lx = 0;
      var code;
      var extra;
      if (s.last_lit !== 0) {
        do {
          dist = s.pending_buf[s.d_buf + lx * 2] << 8 | s.pending_buf[s.d_buf + lx * 2 + 1];
          lc = s.pending_buf[s.l_buf + lx];
          lx++;
          if (dist === 0) {
            send_code(s, lc, ltree);
          } else {
            code = _length_code[lc];
            send_code(s, code + LITERALS + 1, ltree);
            extra = extra_lbits[code];
            if (extra !== 0) {
              lc -= base_length[code];
              send_bits(s, lc, extra);
            }
            dist--;
            code = d_code(dist);
            send_code(s, code, dtree);
            extra = extra_dbits[code];
            if (extra !== 0) {
              dist -= base_dist[code];
              send_bits(s, dist, extra);
            }
          }
        } while (lx < s.last_lit);
      }
      send_code(s, END_BLOCK, ltree);
    }
    function build_tree(s, desc) {
      var tree = desc.dyn_tree;
      var stree = desc.stat_desc.static_tree;
      var has_stree = desc.stat_desc.has_stree;
      var elems = desc.stat_desc.elems;
      var n, m;
      var max_code = -1;
      var node;
      s.heap_len = 0;
      s.heap_max = HEAP_SIZE;
      for (n = 0; n < elems; n++) {
        if (tree[n * 2] !== 0) {
          s.heap[++s.heap_len] = max_code = n;
          s.depth[n] = 0;
        } else {
          tree[n * 2 + 1] = 0;
        }
      }
      while (s.heap_len < 2) {
        node = s.heap[++s.heap_len] = max_code < 2 ? ++max_code : 0;
        tree[node * 2] = 1;
        s.depth[node] = 0;
        s.opt_len--;
        if (has_stree) {
          s.static_len -= stree[node * 2 + 1];
        }
      }
      desc.max_code = max_code;
      for (n = s.heap_len >> 1; n >= 1; n--) {
        pqdownheap(s, tree, n);
      }
      node = elems;
      do {
        n = s.heap[
          1
          /*SMALLEST*/
        ];
        s.heap[
          1
          /*SMALLEST*/
        ] = s.heap[s.heap_len--];
        pqdownheap(
          s,
          tree,
          1
          /*SMALLEST*/
        );
        m = s.heap[
          1
          /*SMALLEST*/
        ];
        s.heap[--s.heap_max] = n;
        s.heap[--s.heap_max] = m;
        tree[node * 2] = tree[n * 2] + tree[m * 2];
        s.depth[node] = (s.depth[n] >= s.depth[m] ? s.depth[n] : s.depth[m]) + 1;
        tree[n * 2 + 1] = tree[m * 2 + 1] = node;
        s.heap[
          1
          /*SMALLEST*/
        ] = node++;
        pqdownheap(
          s,
          tree,
          1
          /*SMALLEST*/
        );
      } while (s.heap_len >= 2);
      s.heap[--s.heap_max] = s.heap[
        1
        /*SMALLEST*/
      ];
      gen_bitlen(s, desc);
      gen_codes(tree, max_code, s.bl_count);
    }
    function scan_tree(s, tree, max_code) {
      var n;
      var prevlen = -1;
      var curlen;
      var nextlen = tree[0 * 2 + 1];
      var count = 0;
      var max_count = 7;
      var min_count = 4;
      if (nextlen === 0) {
        max_count = 138;
        min_count = 3;
      }
      tree[(max_code + 1) * 2 + 1] = 65535;
      for (n = 0; n <= max_code; n++) {
        curlen = nextlen;
        nextlen = tree[(n + 1) * 2 + 1];
        if (++count < max_count && curlen === nextlen) {
          continue;
        } else if (count < min_count) {
          s.bl_tree[curlen * 2] += count;
        } else if (curlen !== 0) {
          if (curlen !== prevlen) {
            s.bl_tree[curlen * 2]++;
          }
          s.bl_tree[REP_3_6 * 2]++;
        } else if (count <= 10) {
          s.bl_tree[REPZ_3_10 * 2]++;
        } else {
          s.bl_tree[REPZ_11_138 * 2]++;
        }
        count = 0;
        prevlen = curlen;
        if (nextlen === 0) {
          max_count = 138;
          min_count = 3;
        } else if (curlen === nextlen) {
          max_count = 6;
          min_count = 3;
        } else {
          max_count = 7;
          min_count = 4;
        }
      }
    }
    function send_tree(s, tree, max_code) {
      var n;
      var prevlen = -1;
      var curlen;
      var nextlen = tree[0 * 2 + 1];
      var count = 0;
      var max_count = 7;
      var min_count = 4;
      if (nextlen === 0) {
        max_count = 138;
        min_count = 3;
      }
      for (n = 0; n <= max_code; n++) {
        curlen = nextlen;
        nextlen = tree[(n + 1) * 2 + 1];
        if (++count < max_count && curlen === nextlen) {
          continue;
        } else if (count < min_count) {
          do {
            send_code(s, curlen, s.bl_tree);
          } while (--count !== 0);
        } else if (curlen !== 0) {
          if (curlen !== prevlen) {
            send_code(s, curlen, s.bl_tree);
            count--;
          }
          send_code(s, REP_3_6, s.bl_tree);
          send_bits(s, count - 3, 2);
        } else if (count <= 10) {
          send_code(s, REPZ_3_10, s.bl_tree);
          send_bits(s, count - 3, 3);
        } else {
          send_code(s, REPZ_11_138, s.bl_tree);
          send_bits(s, count - 11, 7);
        }
        count = 0;
        prevlen = curlen;
        if (nextlen === 0) {
          max_count = 138;
          min_count = 3;
        } else if (curlen === nextlen) {
          max_count = 6;
          min_count = 3;
        } else {
          max_count = 7;
          min_count = 4;
        }
      }
    }
    function build_bl_tree(s) {
      var max_blindex;
      scan_tree(s, s.dyn_ltree, s.l_desc.max_code);
      scan_tree(s, s.dyn_dtree, s.d_desc.max_code);
      build_tree(s, s.bl_desc);
      for (max_blindex = BL_CODES - 1; max_blindex >= 3; max_blindex--) {
        if (s.bl_tree[bl_order[max_blindex] * 2 + 1] !== 0) {
          break;
        }
      }
      s.opt_len += 3 * (max_blindex + 1) + 5 + 5 + 4;
      return max_blindex;
    }
    function send_all_trees(s, lcodes, dcodes, blcodes) {
      var rank;
      send_bits(s, lcodes - 257, 5);
      send_bits(s, dcodes - 1, 5);
      send_bits(s, blcodes - 4, 4);
      for (rank = 0; rank < blcodes; rank++) {
        send_bits(s, s.bl_tree[bl_order[rank] * 2 + 1], 3);
      }
      send_tree(s, s.dyn_ltree, lcodes - 1);
      send_tree(s, s.dyn_dtree, dcodes - 1);
    }
    function detect_data_type(s) {
      var black_mask = 4093624447;
      var n;
      for (n = 0; n <= 31; n++, black_mask >>>= 1) {
        if (black_mask & 1 && s.dyn_ltree[n * 2] !== 0) {
          return Z_BINARY;
        }
      }
      if (s.dyn_ltree[9 * 2] !== 0 || s.dyn_ltree[10 * 2] !== 0 || s.dyn_ltree[13 * 2] !== 0) {
        return Z_TEXT;
      }
      for (n = 32; n < LITERALS; n++) {
        if (s.dyn_ltree[n * 2] !== 0) {
          return Z_TEXT;
        }
      }
      return Z_BINARY;
    }
    var static_init_done = false;
    function _tr_init(s) {
      if (!static_init_done) {
        tr_static_init();
        static_init_done = true;
      }
      s.l_desc = new TreeDesc(s.dyn_ltree, static_l_desc);
      s.d_desc = new TreeDesc(s.dyn_dtree, static_d_desc);
      s.bl_desc = new TreeDesc(s.bl_tree, static_bl_desc);
      s.bi_buf = 0;
      s.bi_valid = 0;
      init_block(s);
    }
    function _tr_stored_block(s, buf, stored_len, last) {
      send_bits(s, (STORED_BLOCK << 1) + (last ? 1 : 0), 3);
      copy_block(s, buf, stored_len, true);
    }
    function _tr_align(s) {
      send_bits(s, STATIC_TREES << 1, 3);
      send_code(s, END_BLOCK, static_ltree);
      bi_flush(s);
    }
    function _tr_flush_block(s, buf, stored_len, last) {
      var opt_lenb, static_lenb;
      var max_blindex = 0;
      if (s.level > 0) {
        if (s.strm.data_type === Z_UNKNOWN) {
          s.strm.data_type = detect_data_type(s);
        }
        build_tree(s, s.l_desc);
        build_tree(s, s.d_desc);
        max_blindex = build_bl_tree(s);
        opt_lenb = s.opt_len + 3 + 7 >>> 3;
        static_lenb = s.static_len + 3 + 7 >>> 3;
        if (static_lenb <= opt_lenb) {
          opt_lenb = static_lenb;
        }
      } else {
        opt_lenb = static_lenb = stored_len + 5;
      }
      if (stored_len + 4 <= opt_lenb && buf !== -1) {
        _tr_stored_block(s, buf, stored_len, last);
      } else if (s.strategy === Z_FIXED || static_lenb === opt_lenb) {
        send_bits(s, (STATIC_TREES << 1) + (last ? 1 : 0), 3);
        compress_block(s, static_ltree, static_dtree);
      } else {
        send_bits(s, (DYN_TREES << 1) + (last ? 1 : 0), 3);
        send_all_trees(s, s.l_desc.max_code + 1, s.d_desc.max_code + 1, max_blindex + 1);
        compress_block(s, s.dyn_ltree, s.dyn_dtree);
      }
      init_block(s);
      if (last) {
        bi_windup(s);
      }
    }
    function _tr_tally(s, dist, lc) {
      s.pending_buf[s.d_buf + s.last_lit * 2] = dist >>> 8 & 255;
      s.pending_buf[s.d_buf + s.last_lit * 2 + 1] = dist & 255;
      s.pending_buf[s.l_buf + s.last_lit] = lc & 255;
      s.last_lit++;
      if (dist === 0) {
        s.dyn_ltree[lc * 2]++;
      } else {
        s.matches++;
        dist--;
        s.dyn_ltree[(_length_code[lc] + LITERALS + 1) * 2]++;
        s.dyn_dtree[d_code(dist) * 2]++;
      }
      return s.last_lit === s.lit_bufsize - 1;
    }
    exports._tr_init = _tr_init;
    exports._tr_stored_block = _tr_stored_block;
    exports._tr_flush_block = _tr_flush_block;
    exports._tr_tally = _tr_tally;
    exports._tr_align = _tr_align;
  }
});
var require_adler32 = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/adler32.js"(exports, module) {
    "use strict";
    function adler32(adler, buf, len, pos) {
      var s1 = adler & 65535 | 0, s2 = adler >>> 16 & 65535 | 0, n = 0;
      while (len !== 0) {
        n = len > 2e3 ? 2e3 : len;
        len -= n;
        do {
          s1 = s1 + buf[pos++] | 0;
          s2 = s2 + s1 | 0;
        } while (--n);
        s1 %= 65521;
        s2 %= 65521;
      }
      return s1 | s2 << 16 | 0;
    }
    module.exports = adler32;
  }
});
var require_crc322 = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/crc32.js"(exports, module) {
    "use strict";
    function makeTable() {
      var c, table = [];
      for (var n = 0; n < 256; n++) {
        c = n;
        for (var k = 0; k < 8; k++) {
          c = c & 1 ? 3988292384 ^ c >>> 1 : c >>> 1;
        }
        table[n] = c;
      }
      return table;
    }
    var crcTable = makeTable();
    function crc32(crc, buf, len, pos) {
      var t = crcTable, end = pos + len;
      crc ^= -1;
      for (var i = pos; i < end; i++) {
        crc = crc >>> 8 ^ t[(crc ^ buf[i]) & 255];
      }
      return crc ^ -1;
    }
    module.exports = crc32;
  }
});
var require_messages = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/messages.js"(exports, module) {
    "use strict";
    module.exports = {
      2: "need dictionary",
      /* Z_NEED_DICT       2  */
      1: "stream end",
      /* Z_STREAM_END      1  */
      0: "",
      /* Z_OK              0  */
      "-1": "file error",
      /* Z_ERRNO         (-1) */
      "-2": "stream error",
      /* Z_STREAM_ERROR  (-2) */
      "-3": "data error",
      /* Z_DATA_ERROR    (-3) */
      "-4": "insufficient memory",
      /* Z_MEM_ERROR     (-4) */
      "-5": "buffer error",
      /* Z_BUF_ERROR     (-5) */
      "-6": "incompatible version"
      /* Z_VERSION_ERROR (-6) */
    };
  }
});
var require_deflate = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/deflate.js"(exports) {
    "use strict";
    var utils = require_common();
    var trees = require_trees();
    var adler32 = require_adler32();
    var crc32 = require_crc322();
    var msg = require_messages();
    var Z_NO_FLUSH = 0;
    var Z_PARTIAL_FLUSH = 1;
    var Z_FULL_FLUSH = 3;
    var Z_FINISH = 4;
    var Z_BLOCK = 5;
    var Z_OK = 0;
    var Z_STREAM_END = 1;
    var Z_STREAM_ERROR = -2;
    var Z_DATA_ERROR = -3;
    var Z_BUF_ERROR = -5;
    var Z_DEFAULT_COMPRESSION = -1;
    var Z_FILTERED = 1;
    var Z_HUFFMAN_ONLY = 2;
    var Z_RLE = 3;
    var Z_FIXED = 4;
    var Z_DEFAULT_STRATEGY = 0;
    var Z_UNKNOWN = 2;
    var Z_DEFLATED = 8;
    var MAX_MEM_LEVEL = 9;
    var MAX_WBITS = 15;
    var DEF_MEM_LEVEL = 8;
    var LENGTH_CODES = 29;
    var LITERALS = 256;
    var L_CODES = LITERALS + 1 + LENGTH_CODES;
    var D_CODES = 30;
    var BL_CODES = 19;
    var HEAP_SIZE = 2 * L_CODES + 1;
    var MAX_BITS = 15;
    var MIN_MATCH = 3;
    var MAX_MATCH = 258;
    var MIN_LOOKAHEAD = MAX_MATCH + MIN_MATCH + 1;
    var PRESET_DICT = 32;
    var INIT_STATE = 42;
    var EXTRA_STATE = 69;
    var NAME_STATE = 73;
    var COMMENT_STATE = 91;
    var HCRC_STATE = 103;
    var BUSY_STATE = 113;
    var FINISH_STATE = 666;
    var BS_NEED_MORE = 1;
    var BS_BLOCK_DONE = 2;
    var BS_FINISH_STARTED = 3;
    var BS_FINISH_DONE = 4;
    var OS_CODE = 3;
    function err(strm, errorCode) {
      strm.msg = msg[errorCode];
      return errorCode;
    }
    function rank(f) {
      return (f << 1) - (f > 4 ? 9 : 0);
    }
    function zero(buf) {
      var len = buf.length;
      while (--len >= 0) {
        buf[len] = 0;
      }
    }
    function flush_pending(strm) {
      var s = strm.state;
      var len = s.pending;
      if (len > strm.avail_out) {
        len = strm.avail_out;
      }
      if (len === 0) {
        return;
      }
      utils.arraySet(strm.output, s.pending_buf, s.pending_out, len, strm.next_out);
      strm.next_out += len;
      s.pending_out += len;
      strm.total_out += len;
      strm.avail_out -= len;
      s.pending -= len;
      if (s.pending === 0) {
        s.pending_out = 0;
      }
    }
    function flush_block_only(s, last) {
      trees._tr_flush_block(s, s.block_start >= 0 ? s.block_start : -1, s.strstart - s.block_start, last);
      s.block_start = s.strstart;
      flush_pending(s.strm);
    }
    function put_byte(s, b) {
      s.pending_buf[s.pending++] = b;
    }
    function putShortMSB(s, b) {
      s.pending_buf[s.pending++] = b >>> 8 & 255;
      s.pending_buf[s.pending++] = b & 255;
    }
    function read_buf(strm, buf, start, size) {
      var len = strm.avail_in;
      if (len > size) {
        len = size;
      }
      if (len === 0) {
        return 0;
      }
      strm.avail_in -= len;
      utils.arraySet(buf, strm.input, strm.next_in, len, start);
      if (strm.state.wrap === 1) {
        strm.adler = adler32(strm.adler, buf, len, start);
      } else if (strm.state.wrap === 2) {
        strm.adler = crc32(strm.adler, buf, len, start);
      }
      strm.next_in += len;
      strm.total_in += len;
      return len;
    }
    function longest_match(s, cur_match) {
      var chain_length = s.max_chain_length;
      var scan = s.strstart;
      var match;
      var len;
      var best_len = s.prev_length;
      var nice_match = s.nice_match;
      var limit = s.strstart > s.w_size - MIN_LOOKAHEAD ? s.strstart - (s.w_size - MIN_LOOKAHEAD) : 0;
      var _win = s.window;
      var wmask = s.w_mask;
      var prev = s.prev;
      var strend = s.strstart + MAX_MATCH;
      var scan_end1 = _win[scan + best_len - 1];
      var scan_end = _win[scan + best_len];
      if (s.prev_length >= s.good_match) {
        chain_length >>= 2;
      }
      if (nice_match > s.lookahead) {
        nice_match = s.lookahead;
      }
      do {
        match = cur_match;
        if (_win[match + best_len] !== scan_end || _win[match + best_len - 1] !== scan_end1 || _win[match] !== _win[scan] || _win[++match] !== _win[scan + 1]) {
          continue;
        }
        scan += 2;
        match++;
        do {
        } while (_win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && _win[++scan] === _win[++match] && scan < strend);
        len = MAX_MATCH - (strend - scan);
        scan = strend - MAX_MATCH;
        if (len > best_len) {
          s.match_start = cur_match;
          best_len = len;
          if (len >= nice_match) {
            break;
          }
          scan_end1 = _win[scan + best_len - 1];
          scan_end = _win[scan + best_len];
        }
      } while ((cur_match = prev[cur_match & wmask]) > limit && --chain_length !== 0);
      if (best_len <= s.lookahead) {
        return best_len;
      }
      return s.lookahead;
    }
    function fill_window(s) {
      var _w_size = s.w_size;
      var p, n, m, more, str;
      do {
        more = s.window_size - s.lookahead - s.strstart;
        if (s.strstart >= _w_size + (_w_size - MIN_LOOKAHEAD)) {
          utils.arraySet(s.window, s.window, _w_size, _w_size, 0);
          s.match_start -= _w_size;
          s.strstart -= _w_size;
          s.block_start -= _w_size;
          n = s.hash_size;
          p = n;
          do {
            m = s.head[--p];
            s.head[p] = m >= _w_size ? m - _w_size : 0;
          } while (--n);
          n = _w_size;
          p = n;
          do {
            m = s.prev[--p];
            s.prev[p] = m >= _w_size ? m - _w_size : 0;
          } while (--n);
          more += _w_size;
        }
        if (s.strm.avail_in === 0) {
          break;
        }
        n = read_buf(s.strm, s.window, s.strstart + s.lookahead, more);
        s.lookahead += n;
        if (s.lookahead + s.insert >= MIN_MATCH) {
          str = s.strstart - s.insert;
          s.ins_h = s.window[str];
          s.ins_h = (s.ins_h << s.hash_shift ^ s.window[str + 1]) & s.hash_mask;
          while (s.insert) {
            s.ins_h = (s.ins_h << s.hash_shift ^ s.window[str + MIN_MATCH - 1]) & s.hash_mask;
            s.prev[str & s.w_mask] = s.head[s.ins_h];
            s.head[s.ins_h] = str;
            str++;
            s.insert--;
            if (s.lookahead + s.insert < MIN_MATCH) {
              break;
            }
          }
        }
      } while (s.lookahead < MIN_LOOKAHEAD && s.strm.avail_in !== 0);
    }
    function deflate_stored(s, flush) {
      var max_block_size = 65535;
      if (max_block_size > s.pending_buf_size - 5) {
        max_block_size = s.pending_buf_size - 5;
      }
      for (; ; ) {
        if (s.lookahead <= 1) {
          fill_window(s);
          if (s.lookahead === 0 && flush === Z_NO_FLUSH) {
            return BS_NEED_MORE;
          }
          if (s.lookahead === 0) {
            break;
          }
        }
        s.strstart += s.lookahead;
        s.lookahead = 0;
        var max_start = s.block_start + max_block_size;
        if (s.strstart === 0 || s.strstart >= max_start) {
          s.lookahead = s.strstart - max_start;
          s.strstart = max_start;
          flush_block_only(s, false);
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        }
        if (s.strstart - s.block_start >= s.w_size - MIN_LOOKAHEAD) {
          flush_block_only(s, false);
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        }
      }
      s.insert = 0;
      if (flush === Z_FINISH) {
        flush_block_only(s, true);
        if (s.strm.avail_out === 0) {
          return BS_FINISH_STARTED;
        }
        return BS_FINISH_DONE;
      }
      if (s.strstart > s.block_start) {
        flush_block_only(s, false);
        if (s.strm.avail_out === 0) {
          return BS_NEED_MORE;
        }
      }
      return BS_NEED_MORE;
    }
    function deflate_fast(s, flush) {
      var hash_head;
      var bflush;
      for (; ; ) {
        if (s.lookahead < MIN_LOOKAHEAD) {
          fill_window(s);
          if (s.lookahead < MIN_LOOKAHEAD && flush === Z_NO_FLUSH) {
            return BS_NEED_MORE;
          }
          if (s.lookahead === 0) {
            break;
          }
        }
        hash_head = 0;
        if (s.lookahead >= MIN_MATCH) {
          s.ins_h = (s.ins_h << s.hash_shift ^ s.window[s.strstart + MIN_MATCH - 1]) & s.hash_mask;
          hash_head = s.prev[s.strstart & s.w_mask] = s.head[s.ins_h];
          s.head[s.ins_h] = s.strstart;
        }
        if (hash_head !== 0 && s.strstart - hash_head <= s.w_size - MIN_LOOKAHEAD) {
          s.match_length = longest_match(s, hash_head);
        }
        if (s.match_length >= MIN_MATCH) {
          bflush = trees._tr_tally(s, s.strstart - s.match_start, s.match_length - MIN_MATCH);
          s.lookahead -= s.match_length;
          if (s.match_length <= s.max_lazy_match && s.lookahead >= MIN_MATCH) {
            s.match_length--;
            do {
              s.strstart++;
              s.ins_h = (s.ins_h << s.hash_shift ^ s.window[s.strstart + MIN_MATCH - 1]) & s.hash_mask;
              hash_head = s.prev[s.strstart & s.w_mask] = s.head[s.ins_h];
              s.head[s.ins_h] = s.strstart;
            } while (--s.match_length !== 0);
            s.strstart++;
          } else {
            s.strstart += s.match_length;
            s.match_length = 0;
            s.ins_h = s.window[s.strstart];
            s.ins_h = (s.ins_h << s.hash_shift ^ s.window[s.strstart + 1]) & s.hash_mask;
          }
        } else {
          bflush = trees._tr_tally(s, 0, s.window[s.strstart]);
          s.lookahead--;
          s.strstart++;
        }
        if (bflush) {
          flush_block_only(s, false);
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        }
      }
      s.insert = s.strstart < MIN_MATCH - 1 ? s.strstart : MIN_MATCH - 1;
      if (flush === Z_FINISH) {
        flush_block_only(s, true);
        if (s.strm.avail_out === 0) {
          return BS_FINISH_STARTED;
        }
        return BS_FINISH_DONE;
      }
      if (s.last_lit) {
        flush_block_only(s, false);
        if (s.strm.avail_out === 0) {
          return BS_NEED_MORE;
        }
      }
      return BS_BLOCK_DONE;
    }
    function deflate_slow(s, flush) {
      var hash_head;
      var bflush;
      var max_insert;
      for (; ; ) {
        if (s.lookahead < MIN_LOOKAHEAD) {
          fill_window(s);
          if (s.lookahead < MIN_LOOKAHEAD && flush === Z_NO_FLUSH) {
            return BS_NEED_MORE;
          }
          if (s.lookahead === 0) {
            break;
          }
        }
        hash_head = 0;
        if (s.lookahead >= MIN_MATCH) {
          s.ins_h = (s.ins_h << s.hash_shift ^ s.window[s.strstart + MIN_MATCH - 1]) & s.hash_mask;
          hash_head = s.prev[s.strstart & s.w_mask] = s.head[s.ins_h];
          s.head[s.ins_h] = s.strstart;
        }
        s.prev_length = s.match_length;
        s.prev_match = s.match_start;
        s.match_length = MIN_MATCH - 1;
        if (hash_head !== 0 && s.prev_length < s.max_lazy_match && s.strstart - hash_head <= s.w_size - MIN_LOOKAHEAD) {
          s.match_length = longest_match(s, hash_head);
          if (s.match_length <= 5 && (s.strategy === Z_FILTERED || s.match_length === MIN_MATCH && s.strstart - s.match_start > 4096)) {
            s.match_length = MIN_MATCH - 1;
          }
        }
        if (s.prev_length >= MIN_MATCH && s.match_length <= s.prev_length) {
          max_insert = s.strstart + s.lookahead - MIN_MATCH;
          bflush = trees._tr_tally(s, s.strstart - 1 - s.prev_match, s.prev_length - MIN_MATCH);
          s.lookahead -= s.prev_length - 1;
          s.prev_length -= 2;
          do {
            if (++s.strstart <= max_insert) {
              s.ins_h = (s.ins_h << s.hash_shift ^ s.window[s.strstart + MIN_MATCH - 1]) & s.hash_mask;
              hash_head = s.prev[s.strstart & s.w_mask] = s.head[s.ins_h];
              s.head[s.ins_h] = s.strstart;
            }
          } while (--s.prev_length !== 0);
          s.match_available = 0;
          s.match_length = MIN_MATCH - 1;
          s.strstart++;
          if (bflush) {
            flush_block_only(s, false);
            if (s.strm.avail_out === 0) {
              return BS_NEED_MORE;
            }
          }
        } else if (s.match_available) {
          bflush = trees._tr_tally(s, 0, s.window[s.strstart - 1]);
          if (bflush) {
            flush_block_only(s, false);
          }
          s.strstart++;
          s.lookahead--;
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        } else {
          s.match_available = 1;
          s.strstart++;
          s.lookahead--;
        }
      }
      if (s.match_available) {
        bflush = trees._tr_tally(s, 0, s.window[s.strstart - 1]);
        s.match_available = 0;
      }
      s.insert = s.strstart < MIN_MATCH - 1 ? s.strstart : MIN_MATCH - 1;
      if (flush === Z_FINISH) {
        flush_block_only(s, true);
        if (s.strm.avail_out === 0) {
          return BS_FINISH_STARTED;
        }
        return BS_FINISH_DONE;
      }
      if (s.last_lit) {
        flush_block_only(s, false);
        if (s.strm.avail_out === 0) {
          return BS_NEED_MORE;
        }
      }
      return BS_BLOCK_DONE;
    }
    function deflate_rle(s, flush) {
      var bflush;
      var prev;
      var scan, strend;
      var _win = s.window;
      for (; ; ) {
        if (s.lookahead <= MAX_MATCH) {
          fill_window(s);
          if (s.lookahead <= MAX_MATCH && flush === Z_NO_FLUSH) {
            return BS_NEED_MORE;
          }
          if (s.lookahead === 0) {
            break;
          }
        }
        s.match_length = 0;
        if (s.lookahead >= MIN_MATCH && s.strstart > 0) {
          scan = s.strstart - 1;
          prev = _win[scan];
          if (prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan]) {
            strend = s.strstart + MAX_MATCH;
            do {
            } while (prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && prev === _win[++scan] && scan < strend);
            s.match_length = MAX_MATCH - (strend - scan);
            if (s.match_length > s.lookahead) {
              s.match_length = s.lookahead;
            }
          }
        }
        if (s.match_length >= MIN_MATCH) {
          bflush = trees._tr_tally(s, 1, s.match_length - MIN_MATCH);
          s.lookahead -= s.match_length;
          s.strstart += s.match_length;
          s.match_length = 0;
        } else {
          bflush = trees._tr_tally(s, 0, s.window[s.strstart]);
          s.lookahead--;
          s.strstart++;
        }
        if (bflush) {
          flush_block_only(s, false);
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        }
      }
      s.insert = 0;
      if (flush === Z_FINISH) {
        flush_block_only(s, true);
        if (s.strm.avail_out === 0) {
          return BS_FINISH_STARTED;
        }
        return BS_FINISH_DONE;
      }
      if (s.last_lit) {
        flush_block_only(s, false);
        if (s.strm.avail_out === 0) {
          return BS_NEED_MORE;
        }
      }
      return BS_BLOCK_DONE;
    }
    function deflate_huff(s, flush) {
      var bflush;
      for (; ; ) {
        if (s.lookahead === 0) {
          fill_window(s);
          if (s.lookahead === 0) {
            if (flush === Z_NO_FLUSH) {
              return BS_NEED_MORE;
            }
            break;
          }
        }
        s.match_length = 0;
        bflush = trees._tr_tally(s, 0, s.window[s.strstart]);
        s.lookahead--;
        s.strstart++;
        if (bflush) {
          flush_block_only(s, false);
          if (s.strm.avail_out === 0) {
            return BS_NEED_MORE;
          }
        }
      }
      s.insert = 0;
      if (flush === Z_FINISH) {
        flush_block_only(s, true);
        if (s.strm.avail_out === 0) {
          return BS_FINISH_STARTED;
        }
        return BS_FINISH_DONE;
      }
      if (s.last_lit) {
        flush_block_only(s, false);
        if (s.strm.avail_out === 0) {
          return BS_NEED_MORE;
        }
      }
      return BS_BLOCK_DONE;
    }
    function Config(good_length, max_lazy, nice_length, max_chain, func) {
      this.good_length = good_length;
      this.max_lazy = max_lazy;
      this.nice_length = nice_length;
      this.max_chain = max_chain;
      this.func = func;
    }
    var configuration_table;
    configuration_table = [
      /*      good lazy nice chain */
      new Config(0, 0, 0, 0, deflate_stored),
      /* 0 store only */
      new Config(4, 4, 8, 4, deflate_fast),
      /* 1 max speed, no lazy matches */
      new Config(4, 5, 16, 8, deflate_fast),
      /* 2 */
      new Config(4, 6, 32, 32, deflate_fast),
      /* 3 */
      new Config(4, 4, 16, 16, deflate_slow),
      /* 4 lazy matches */
      new Config(8, 16, 32, 32, deflate_slow),
      /* 5 */
      new Config(8, 16, 128, 128, deflate_slow),
      /* 6 */
      new Config(8, 32, 128, 256, deflate_slow),
      /* 7 */
      new Config(32, 128, 258, 1024, deflate_slow),
      /* 8 */
      new Config(32, 258, 258, 4096, deflate_slow)
      /* 9 max compression */
    ];
    function lm_init(s) {
      s.window_size = 2 * s.w_size;
      zero(s.head);
      s.max_lazy_match = configuration_table[s.level].max_lazy;
      s.good_match = configuration_table[s.level].good_length;
      s.nice_match = configuration_table[s.level].nice_length;
      s.max_chain_length = configuration_table[s.level].max_chain;
      s.strstart = 0;
      s.block_start = 0;
      s.lookahead = 0;
      s.insert = 0;
      s.match_length = s.prev_length = MIN_MATCH - 1;
      s.match_available = 0;
      s.ins_h = 0;
    }
    function DeflateState() {
      this.strm = null;
      this.status = 0;
      this.pending_buf = null;
      this.pending_buf_size = 0;
      this.pending_out = 0;
      this.pending = 0;
      this.wrap = 0;
      this.gzhead = null;
      this.gzindex = 0;
      this.method = Z_DEFLATED;
      this.last_flush = -1;
      this.w_size = 0;
      this.w_bits = 0;
      this.w_mask = 0;
      this.window = null;
      this.window_size = 0;
      this.prev = null;
      this.head = null;
      this.ins_h = 0;
      this.hash_size = 0;
      this.hash_bits = 0;
      this.hash_mask = 0;
      this.hash_shift = 0;
      this.block_start = 0;
      this.match_length = 0;
      this.prev_match = 0;
      this.match_available = 0;
      this.strstart = 0;
      this.match_start = 0;
      this.lookahead = 0;
      this.prev_length = 0;
      this.max_chain_length = 0;
      this.max_lazy_match = 0;
      this.level = 0;
      this.strategy = 0;
      this.good_match = 0;
      this.nice_match = 0;
      this.dyn_ltree = new utils.Buf16(HEAP_SIZE * 2);
      this.dyn_dtree = new utils.Buf16((2 * D_CODES + 1) * 2);
      this.bl_tree = new utils.Buf16((2 * BL_CODES + 1) * 2);
      zero(this.dyn_ltree);
      zero(this.dyn_dtree);
      zero(this.bl_tree);
      this.l_desc = null;
      this.d_desc = null;
      this.bl_desc = null;
      this.bl_count = new utils.Buf16(MAX_BITS + 1);
      this.heap = new utils.Buf16(2 * L_CODES + 1);
      zero(this.heap);
      this.heap_len = 0;
      this.heap_max = 0;
      this.depth = new utils.Buf16(2 * L_CODES + 1);
      zero(this.depth);
      this.l_buf = 0;
      this.lit_bufsize = 0;
      this.last_lit = 0;
      this.d_buf = 0;
      this.opt_len = 0;
      this.static_len = 0;
      this.matches = 0;
      this.insert = 0;
      this.bi_buf = 0;
      this.bi_valid = 0;
    }
    function deflateResetKeep(strm) {
      var s;
      if (!strm || !strm.state) {
        return err(strm, Z_STREAM_ERROR);
      }
      strm.total_in = strm.total_out = 0;
      strm.data_type = Z_UNKNOWN;
      s = strm.state;
      s.pending = 0;
      s.pending_out = 0;
      if (s.wrap < 0) {
        s.wrap = -s.wrap;
      }
      s.status = s.wrap ? INIT_STATE : BUSY_STATE;
      strm.adler = s.wrap === 2 ? 0 : 1;
      s.last_flush = Z_NO_FLUSH;
      trees._tr_init(s);
      return Z_OK;
    }
    function deflateReset(strm) {
      var ret = deflateResetKeep(strm);
      if (ret === Z_OK) {
        lm_init(strm.state);
      }
      return ret;
    }
    function deflateSetHeader(strm, head) {
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      if (strm.state.wrap !== 2) {
        return Z_STREAM_ERROR;
      }
      strm.state.gzhead = head;
      return Z_OK;
    }
    function deflateInit2(strm, level, method, windowBits, memLevel, strategy) {
      if (!strm) {
        return Z_STREAM_ERROR;
      }
      var wrap = 1;
      if (level === Z_DEFAULT_COMPRESSION) {
        level = 6;
      }
      if (windowBits < 0) {
        wrap = 0;
        windowBits = -windowBits;
      } else if (windowBits > 15) {
        wrap = 2;
        windowBits -= 16;
      }
      if (memLevel < 1 || memLevel > MAX_MEM_LEVEL || method !== Z_DEFLATED || windowBits < 8 || windowBits > 15 || level < 0 || level > 9 || strategy < 0 || strategy > Z_FIXED) {
        return err(strm, Z_STREAM_ERROR);
      }
      if (windowBits === 8) {
        windowBits = 9;
      }
      var s = new DeflateState();
      strm.state = s;
      s.strm = strm;
      s.wrap = wrap;
      s.gzhead = null;
      s.w_bits = windowBits;
      s.w_size = 1 << s.w_bits;
      s.w_mask = s.w_size - 1;
      s.hash_bits = memLevel + 7;
      s.hash_size = 1 << s.hash_bits;
      s.hash_mask = s.hash_size - 1;
      s.hash_shift = ~~((s.hash_bits + MIN_MATCH - 1) / MIN_MATCH);
      s.window = new utils.Buf8(s.w_size * 2);
      s.head = new utils.Buf16(s.hash_size);
      s.prev = new utils.Buf16(s.w_size);
      s.lit_bufsize = 1 << memLevel + 6;
      s.pending_buf_size = s.lit_bufsize * 4;
      s.pending_buf = new utils.Buf8(s.pending_buf_size);
      s.d_buf = 1 * s.lit_bufsize;
      s.l_buf = (1 + 2) * s.lit_bufsize;
      s.level = level;
      s.strategy = strategy;
      s.method = method;
      return deflateReset(strm);
    }
    function deflateInit(strm, level) {
      return deflateInit2(strm, level, Z_DEFLATED, MAX_WBITS, DEF_MEM_LEVEL, Z_DEFAULT_STRATEGY);
    }
    function deflate(strm, flush) {
      var old_flush, s;
      var beg, val2;
      if (!strm || !strm.state || flush > Z_BLOCK || flush < 0) {
        return strm ? err(strm, Z_STREAM_ERROR) : Z_STREAM_ERROR;
      }
      s = strm.state;
      if (!strm.output || !strm.input && strm.avail_in !== 0 || s.status === FINISH_STATE && flush !== Z_FINISH) {
        return err(strm, strm.avail_out === 0 ? Z_BUF_ERROR : Z_STREAM_ERROR);
      }
      s.strm = strm;
      old_flush = s.last_flush;
      s.last_flush = flush;
      if (s.status === INIT_STATE) {
        if (s.wrap === 2) {
          strm.adler = 0;
          put_byte(s, 31);
          put_byte(s, 139);
          put_byte(s, 8);
          if (!s.gzhead) {
            put_byte(s, 0);
            put_byte(s, 0);
            put_byte(s, 0);
            put_byte(s, 0);
            put_byte(s, 0);
            put_byte(s, s.level === 9 ? 2 : s.strategy >= Z_HUFFMAN_ONLY || s.level < 2 ? 4 : 0);
            put_byte(s, OS_CODE);
            s.status = BUSY_STATE;
          } else {
            put_byte(
              s,
              (s.gzhead.text ? 1 : 0) + (s.gzhead.hcrc ? 2 : 0) + (!s.gzhead.extra ? 0 : 4) + (!s.gzhead.name ? 0 : 8) + (!s.gzhead.comment ? 0 : 16)
            );
            put_byte(s, s.gzhead.time & 255);
            put_byte(s, s.gzhead.time >> 8 & 255);
            put_byte(s, s.gzhead.time >> 16 & 255);
            put_byte(s, s.gzhead.time >> 24 & 255);
            put_byte(s, s.level === 9 ? 2 : s.strategy >= Z_HUFFMAN_ONLY || s.level < 2 ? 4 : 0);
            put_byte(s, s.gzhead.os & 255);
            if (s.gzhead.extra && s.gzhead.extra.length) {
              put_byte(s, s.gzhead.extra.length & 255);
              put_byte(s, s.gzhead.extra.length >> 8 & 255);
            }
            if (s.gzhead.hcrc) {
              strm.adler = crc32(strm.adler, s.pending_buf, s.pending, 0);
            }
            s.gzindex = 0;
            s.status = EXTRA_STATE;
          }
        } else {
          var header = Z_DEFLATED + (s.w_bits - 8 << 4) << 8;
          var level_flags = -1;
          if (s.strategy >= Z_HUFFMAN_ONLY || s.level < 2) {
            level_flags = 0;
          } else if (s.level < 6) {
            level_flags = 1;
          } else if (s.level === 6) {
            level_flags = 2;
          } else {
            level_flags = 3;
          }
          header |= level_flags << 6;
          if (s.strstart !== 0) {
            header |= PRESET_DICT;
          }
          header += 31 - header % 31;
          s.status = BUSY_STATE;
          putShortMSB(s, header);
          if (s.strstart !== 0) {
            putShortMSB(s, strm.adler >>> 16);
            putShortMSB(s, strm.adler & 65535);
          }
          strm.adler = 1;
        }
      }
      if (s.status === EXTRA_STATE) {
        if (s.gzhead.extra) {
          beg = s.pending;
          while (s.gzindex < (s.gzhead.extra.length & 65535)) {
            if (s.pending === s.pending_buf_size) {
              if (s.gzhead.hcrc && s.pending > beg) {
                strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
              }
              flush_pending(strm);
              beg = s.pending;
              if (s.pending === s.pending_buf_size) {
                break;
              }
            }
            put_byte(s, s.gzhead.extra[s.gzindex] & 255);
            s.gzindex++;
          }
          if (s.gzhead.hcrc && s.pending > beg) {
            strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
          }
          if (s.gzindex === s.gzhead.extra.length) {
            s.gzindex = 0;
            s.status = NAME_STATE;
          }
        } else {
          s.status = NAME_STATE;
        }
      }
      if (s.status === NAME_STATE) {
        if (s.gzhead.name) {
          beg = s.pending;
          do {
            if (s.pending === s.pending_buf_size) {
              if (s.gzhead.hcrc && s.pending > beg) {
                strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
              }
              flush_pending(strm);
              beg = s.pending;
              if (s.pending === s.pending_buf_size) {
                val2 = 1;
                break;
              }
            }
            if (s.gzindex < s.gzhead.name.length) {
              val2 = s.gzhead.name.charCodeAt(s.gzindex++) & 255;
            } else {
              val2 = 0;
            }
            put_byte(s, val2);
          } while (val2 !== 0);
          if (s.gzhead.hcrc && s.pending > beg) {
            strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
          }
          if (val2 === 0) {
            s.gzindex = 0;
            s.status = COMMENT_STATE;
          }
        } else {
          s.status = COMMENT_STATE;
        }
      }
      if (s.status === COMMENT_STATE) {
        if (s.gzhead.comment) {
          beg = s.pending;
          do {
            if (s.pending === s.pending_buf_size) {
              if (s.gzhead.hcrc && s.pending > beg) {
                strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
              }
              flush_pending(strm);
              beg = s.pending;
              if (s.pending === s.pending_buf_size) {
                val2 = 1;
                break;
              }
            }
            if (s.gzindex < s.gzhead.comment.length) {
              val2 = s.gzhead.comment.charCodeAt(s.gzindex++) & 255;
            } else {
              val2 = 0;
            }
            put_byte(s, val2);
          } while (val2 !== 0);
          if (s.gzhead.hcrc && s.pending > beg) {
            strm.adler = crc32(strm.adler, s.pending_buf, s.pending - beg, beg);
          }
          if (val2 === 0) {
            s.status = HCRC_STATE;
          }
        } else {
          s.status = HCRC_STATE;
        }
      }
      if (s.status === HCRC_STATE) {
        if (s.gzhead.hcrc) {
          if (s.pending + 2 > s.pending_buf_size) {
            flush_pending(strm);
          }
          if (s.pending + 2 <= s.pending_buf_size) {
            put_byte(s, strm.adler & 255);
            put_byte(s, strm.adler >> 8 & 255);
            strm.adler = 0;
            s.status = BUSY_STATE;
          }
        } else {
          s.status = BUSY_STATE;
        }
      }
      if (s.pending !== 0) {
        flush_pending(strm);
        if (strm.avail_out === 0) {
          s.last_flush = -1;
          return Z_OK;
        }
      } else if (strm.avail_in === 0 && rank(flush) <= rank(old_flush) && flush !== Z_FINISH) {
        return err(strm, Z_BUF_ERROR);
      }
      if (s.status === FINISH_STATE && strm.avail_in !== 0) {
        return err(strm, Z_BUF_ERROR);
      }
      if (strm.avail_in !== 0 || s.lookahead !== 0 || flush !== Z_NO_FLUSH && s.status !== FINISH_STATE) {
        var bstate = s.strategy === Z_HUFFMAN_ONLY ? deflate_huff(s, flush) : s.strategy === Z_RLE ? deflate_rle(s, flush) : configuration_table[s.level].func(s, flush);
        if (bstate === BS_FINISH_STARTED || bstate === BS_FINISH_DONE) {
          s.status = FINISH_STATE;
        }
        if (bstate === BS_NEED_MORE || bstate === BS_FINISH_STARTED) {
          if (strm.avail_out === 0) {
            s.last_flush = -1;
          }
          return Z_OK;
        }
        if (bstate === BS_BLOCK_DONE) {
          if (flush === Z_PARTIAL_FLUSH) {
            trees._tr_align(s);
          } else if (flush !== Z_BLOCK) {
            trees._tr_stored_block(s, 0, 0, false);
            if (flush === Z_FULL_FLUSH) {
              zero(s.head);
              if (s.lookahead === 0) {
                s.strstart = 0;
                s.block_start = 0;
                s.insert = 0;
              }
            }
          }
          flush_pending(strm);
          if (strm.avail_out === 0) {
            s.last_flush = -1;
            return Z_OK;
          }
        }
      }
      if (flush !== Z_FINISH) {
        return Z_OK;
      }
      if (s.wrap <= 0) {
        return Z_STREAM_END;
      }
      if (s.wrap === 2) {
        put_byte(s, strm.adler & 255);
        put_byte(s, strm.adler >> 8 & 255);
        put_byte(s, strm.adler >> 16 & 255);
        put_byte(s, strm.adler >> 24 & 255);
        put_byte(s, strm.total_in & 255);
        put_byte(s, strm.total_in >> 8 & 255);
        put_byte(s, strm.total_in >> 16 & 255);
        put_byte(s, strm.total_in >> 24 & 255);
      } else {
        putShortMSB(s, strm.adler >>> 16);
        putShortMSB(s, strm.adler & 65535);
      }
      flush_pending(strm);
      if (s.wrap > 0) {
        s.wrap = -s.wrap;
      }
      return s.pending !== 0 ? Z_OK : Z_STREAM_END;
    }
    function deflateEnd(strm) {
      var status;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      status = strm.state.status;
      if (status !== INIT_STATE && status !== EXTRA_STATE && status !== NAME_STATE && status !== COMMENT_STATE && status !== HCRC_STATE && status !== BUSY_STATE && status !== FINISH_STATE) {
        return err(strm, Z_STREAM_ERROR);
      }
      strm.state = null;
      return status === BUSY_STATE ? err(strm, Z_DATA_ERROR) : Z_OK;
    }
    function deflateSetDictionary(strm, dictionary) {
      var dictLength = dictionary.length;
      var s;
      var str, n;
      var wrap;
      var avail;
      var next;
      var input;
      var tmpDict;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      s = strm.state;
      wrap = s.wrap;
      if (wrap === 2 || wrap === 1 && s.status !== INIT_STATE || s.lookahead) {
        return Z_STREAM_ERROR;
      }
      if (wrap === 1) {
        strm.adler = adler32(strm.adler, dictionary, dictLength, 0);
      }
      s.wrap = 0;
      if (dictLength >= s.w_size) {
        if (wrap === 0) {
          zero(s.head);
          s.strstart = 0;
          s.block_start = 0;
          s.insert = 0;
        }
        tmpDict = new utils.Buf8(s.w_size);
        utils.arraySet(tmpDict, dictionary, dictLength - s.w_size, s.w_size, 0);
        dictionary = tmpDict;
        dictLength = s.w_size;
      }
      avail = strm.avail_in;
      next = strm.next_in;
      input = strm.input;
      strm.avail_in = dictLength;
      strm.next_in = 0;
      strm.input = dictionary;
      fill_window(s);
      while (s.lookahead >= MIN_MATCH) {
        str = s.strstart;
        n = s.lookahead - (MIN_MATCH - 1);
        do {
          s.ins_h = (s.ins_h << s.hash_shift ^ s.window[str + MIN_MATCH - 1]) & s.hash_mask;
          s.prev[str & s.w_mask] = s.head[s.ins_h];
          s.head[s.ins_h] = str;
          str++;
        } while (--n);
        s.strstart = str;
        s.lookahead = MIN_MATCH - 1;
        fill_window(s);
      }
      s.strstart += s.lookahead;
      s.block_start = s.strstart;
      s.insert = s.lookahead;
      s.lookahead = 0;
      s.match_length = s.prev_length = MIN_MATCH - 1;
      s.match_available = 0;
      strm.next_in = next;
      strm.input = input;
      strm.avail_in = avail;
      s.wrap = wrap;
      return Z_OK;
    }
    exports.deflateInit = deflateInit;
    exports.deflateInit2 = deflateInit2;
    exports.deflateReset = deflateReset;
    exports.deflateResetKeep = deflateResetKeep;
    exports.deflateSetHeader = deflateSetHeader;
    exports.deflate = deflate;
    exports.deflateEnd = deflateEnd;
    exports.deflateSetDictionary = deflateSetDictionary;
    exports.deflateInfo = "pako deflate (from Nodeca project)";
  }
});
var require_strings = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/utils/strings.js"(exports) {
    "use strict";
    var utils = require_common();
    var STR_APPLY_OK = true;
    var STR_APPLY_UIA_OK = true;
    try {
      String.fromCharCode.apply(null, [0]);
    } catch (__) {
      STR_APPLY_OK = false;
    }
    try {
      String.fromCharCode.apply(null, new Uint8Array(1));
    } catch (__) {
      STR_APPLY_UIA_OK = false;
    }
    var _utf8len = new utils.Buf8(256);
    for (q = 0; q < 256; q++) {
      _utf8len[q] = q >= 252 ? 6 : q >= 248 ? 5 : q >= 240 ? 4 : q >= 224 ? 3 : q >= 192 ? 2 : 1;
    }
    var q;
    _utf8len[254] = _utf8len[254] = 1;
    exports.string2buf = function(str) {
      var buf, c, c2, m_pos, i, str_len = str.length, buf_len = 0;
      for (m_pos = 0; m_pos < str_len; m_pos++) {
        c = str.charCodeAt(m_pos);
        if ((c & 64512) === 55296 && m_pos + 1 < str_len) {
          c2 = str.charCodeAt(m_pos + 1);
          if ((c2 & 64512) === 56320) {
            c = 65536 + (c - 55296 << 10) + (c2 - 56320);
            m_pos++;
          }
        }
        buf_len += c < 128 ? 1 : c < 2048 ? 2 : c < 65536 ? 3 : 4;
      }
      buf = new utils.Buf8(buf_len);
      for (i = 0, m_pos = 0; i < buf_len; m_pos++) {
        c = str.charCodeAt(m_pos);
        if ((c & 64512) === 55296 && m_pos + 1 < str_len) {
          c2 = str.charCodeAt(m_pos + 1);
          if ((c2 & 64512) === 56320) {
            c = 65536 + (c - 55296 << 10) + (c2 - 56320);
            m_pos++;
          }
        }
        if (c < 128) {
          buf[i++] = c;
        } else if (c < 2048) {
          buf[i++] = 192 | c >>> 6;
          buf[i++] = 128 | c & 63;
        } else if (c < 65536) {
          buf[i++] = 224 | c >>> 12;
          buf[i++] = 128 | c >>> 6 & 63;
          buf[i++] = 128 | c & 63;
        } else {
          buf[i++] = 240 | c >>> 18;
          buf[i++] = 128 | c >>> 12 & 63;
          buf[i++] = 128 | c >>> 6 & 63;
          buf[i++] = 128 | c & 63;
        }
      }
      return buf;
    };
    function buf2binstring(buf, len) {
      if (len < 65534) {
        if (buf.subarray && STR_APPLY_UIA_OK || !buf.subarray && STR_APPLY_OK) {
          return String.fromCharCode.apply(null, utils.shrinkBuf(buf, len));
        }
      }
      var result = "";
      for (var i = 0; i < len; i++) {
        result += String.fromCharCode(buf[i]);
      }
      return result;
    }
    exports.buf2binstring = function(buf) {
      return buf2binstring(buf, buf.length);
    };
    exports.binstring2buf = function(str) {
      var buf = new utils.Buf8(str.length);
      for (var i = 0, len = buf.length; i < len; i++) {
        buf[i] = str.charCodeAt(i);
      }
      return buf;
    };
    exports.buf2string = function(buf, max) {
      var i, out, c, c_len;
      var len = max || buf.length;
      var utf16buf = new Array(len * 2);
      for (out = 0, i = 0; i < len; ) {
        c = buf[i++];
        if (c < 128) {
          utf16buf[out++] = c;
          continue;
        }
        c_len = _utf8len[c];
        if (c_len > 4) {
          utf16buf[out++] = 65533;
          i += c_len - 1;
          continue;
        }
        c &= c_len === 2 ? 31 : c_len === 3 ? 15 : 7;
        while (c_len > 1 && i < len) {
          c = c << 6 | buf[i++] & 63;
          c_len--;
        }
        if (c_len > 1) {
          utf16buf[out++] = 65533;
          continue;
        }
        if (c < 65536) {
          utf16buf[out++] = c;
        } else {
          c -= 65536;
          utf16buf[out++] = 55296 | c >> 10 & 1023;
          utf16buf[out++] = 56320 | c & 1023;
        }
      }
      return buf2binstring(utf16buf, out);
    };
    exports.utf8border = function(buf, max) {
      var pos;
      max = max || buf.length;
      if (max > buf.length) {
        max = buf.length;
      }
      pos = max - 1;
      while (pos >= 0 && (buf[pos] & 192) === 128) {
        pos--;
      }
      if (pos < 0) {
        return max;
      }
      if (pos === 0) {
        return max;
      }
      return pos + _utf8len[buf[pos]] > max ? pos : max;
    };
  }
});
var require_zstream = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/zstream.js"(exports, module) {
    "use strict";
    function ZStream() {
      this.input = null;
      this.next_in = 0;
      this.avail_in = 0;
      this.total_in = 0;
      this.output = null;
      this.next_out = 0;
      this.avail_out = 0;
      this.total_out = 0;
      this.msg = "";
      this.state = null;
      this.data_type = 2;
      this.adler = 0;
    }
    module.exports = ZStream;
  }
});
var require_deflate2 = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/deflate.js"(exports) {
    "use strict";
    var zlib_deflate = require_deflate();
    var utils = require_common();
    var strings = require_strings();
    var msg = require_messages();
    var ZStream = require_zstream();
    var toString = Object.prototype.toString;
    var Z_NO_FLUSH = 0;
    var Z_FINISH = 4;
    var Z_OK = 0;
    var Z_STREAM_END = 1;
    var Z_SYNC_FLUSH = 2;
    var Z_DEFAULT_COMPRESSION = -1;
    var Z_DEFAULT_STRATEGY = 0;
    var Z_DEFLATED = 8;
    function Deflate(options) {
      if (!(this instanceof Deflate)) return new Deflate(options);
      this.options = utils.assign({
        level: Z_DEFAULT_COMPRESSION,
        method: Z_DEFLATED,
        chunkSize: 16384,
        windowBits: 15,
        memLevel: 8,
        strategy: Z_DEFAULT_STRATEGY,
        to: ""
      }, options || {});
      var opt = this.options;
      if (opt.raw && opt.windowBits > 0) {
        opt.windowBits = -opt.windowBits;
      } else if (opt.gzip && opt.windowBits > 0 && opt.windowBits < 16) {
        opt.windowBits += 16;
      }
      this.err = 0;
      this.msg = "";
      this.ended = false;
      this.chunks = [];
      this.strm = new ZStream();
      this.strm.avail_out = 0;
      var status = zlib_deflate.deflateInit2(
        this.strm,
        opt.level,
        opt.method,
        opt.windowBits,
        opt.memLevel,
        opt.strategy
      );
      if (status !== Z_OK) {
        throw new Error(msg[status]);
      }
      if (opt.header) {
        zlib_deflate.deflateSetHeader(this.strm, opt.header);
      }
      if (opt.dictionary) {
        var dict;
        if (typeof opt.dictionary === "string") {
          dict = strings.string2buf(opt.dictionary);
        } else if (toString.call(opt.dictionary) === "[object ArrayBuffer]") {
          dict = new Uint8Array(opt.dictionary);
        } else {
          dict = opt.dictionary;
        }
        status = zlib_deflate.deflateSetDictionary(this.strm, dict);
        if (status !== Z_OK) {
          throw new Error(msg[status]);
        }
        this._dict_set = true;
      }
    }
    Deflate.prototype.push = function(data, mode) {
      var strm = this.strm;
      var chunkSize = this.options.chunkSize;
      var status, _mode;
      if (this.ended) {
        return false;
      }
      _mode = mode === ~~mode ? mode : mode === true ? Z_FINISH : Z_NO_FLUSH;
      if (typeof data === "string") {
        strm.input = strings.string2buf(data);
      } else if (toString.call(data) === "[object ArrayBuffer]") {
        strm.input = new Uint8Array(data);
      } else {
        strm.input = data;
      }
      strm.next_in = 0;
      strm.avail_in = strm.input.length;
      do {
        if (strm.avail_out === 0) {
          strm.output = new utils.Buf8(chunkSize);
          strm.next_out = 0;
          strm.avail_out = chunkSize;
        }
        status = zlib_deflate.deflate(strm, _mode);
        if (status !== Z_STREAM_END && status !== Z_OK) {
          this.onEnd(status);
          this.ended = true;
          return false;
        }
        if (strm.avail_out === 0 || strm.avail_in === 0 && (_mode === Z_FINISH || _mode === Z_SYNC_FLUSH)) {
          if (this.options.to === "string") {
            this.onData(strings.buf2binstring(utils.shrinkBuf(strm.output, strm.next_out)));
          } else {
            this.onData(utils.shrinkBuf(strm.output, strm.next_out));
          }
        }
      } while ((strm.avail_in > 0 || strm.avail_out === 0) && status !== Z_STREAM_END);
      if (_mode === Z_FINISH) {
        status = zlib_deflate.deflateEnd(this.strm);
        this.onEnd(status);
        this.ended = true;
        return status === Z_OK;
      }
      if (_mode === Z_SYNC_FLUSH) {
        this.onEnd(Z_OK);
        strm.avail_out = 0;
        return true;
      }
      return true;
    };
    Deflate.prototype.onData = function(chunk) {
      this.chunks.push(chunk);
    };
    Deflate.prototype.onEnd = function(status) {
      if (status === Z_OK) {
        if (this.options.to === "string") {
          this.result = this.chunks.join("");
        } else {
          this.result = utils.flattenChunks(this.chunks);
        }
      }
      this.chunks = [];
      this.err = status;
      this.msg = this.strm.msg;
    };
    function deflate(input, options) {
      var deflator = new Deflate(options);
      deflator.push(input, true);
      if (deflator.err) {
        throw deflator.msg || msg[deflator.err];
      }
      return deflator.result;
    }
    function deflateRaw(input, options) {
      options = options || {};
      options.raw = true;
      return deflate(input, options);
    }
    function gzip(input, options) {
      options = options || {};
      options.gzip = true;
      return deflate(input, options);
    }
    exports.Deflate = Deflate;
    exports.deflate = deflate;
    exports.deflateRaw = deflateRaw;
    exports.gzip = gzip;
  }
});
var require_inffast = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/inffast.js"(exports, module) {
    "use strict";
    var BAD = 30;
    var TYPE = 12;
    module.exports = function inflate_fast(strm, start) {
      var state;
      var _in;
      var last;
      var _out;
      var beg;
      var end;
      var dmax;
      var wsize;
      var whave;
      var wnext;
      var s_window;
      var hold;
      var bits;
      var lcode;
      var dcode;
      var lmask;
      var dmask;
      var here;
      var op;
      var len;
      var dist;
      var from;
      var from_source;
      var input, output;
      state = strm.state;
      _in = strm.next_in;
      input = strm.input;
      last = _in + (strm.avail_in - 5);
      _out = strm.next_out;
      output = strm.output;
      beg = _out - (start - strm.avail_out);
      end = _out + (strm.avail_out - 257);
      dmax = state.dmax;
      wsize = state.wsize;
      whave = state.whave;
      wnext = state.wnext;
      s_window = state.window;
      hold = state.hold;
      bits = state.bits;
      lcode = state.lencode;
      dcode = state.distcode;
      lmask = (1 << state.lenbits) - 1;
      dmask = (1 << state.distbits) - 1;
      top:
        do {
          if (bits < 15) {
            hold += input[_in++] << bits;
            bits += 8;
            hold += input[_in++] << bits;
            bits += 8;
          }
          here = lcode[hold & lmask];
          dolen:
            for (; ; ) {
              op = here >>> 24;
              hold >>>= op;
              bits -= op;
              op = here >>> 16 & 255;
              if (op === 0) {
                output[_out++] = here & 65535;
              } else if (op & 16) {
                len = here & 65535;
                op &= 15;
                if (op) {
                  if (bits < op) {
                    hold += input[_in++] << bits;
                    bits += 8;
                  }
                  len += hold & (1 << op) - 1;
                  hold >>>= op;
                  bits -= op;
                }
                if (bits < 15) {
                  hold += input[_in++] << bits;
                  bits += 8;
                  hold += input[_in++] << bits;
                  bits += 8;
                }
                here = dcode[hold & dmask];
                dodist:
                  for (; ; ) {
                    op = here >>> 24;
                    hold >>>= op;
                    bits -= op;
                    op = here >>> 16 & 255;
                    if (op & 16) {
                      dist = here & 65535;
                      op &= 15;
                      if (bits < op) {
                        hold += input[_in++] << bits;
                        bits += 8;
                        if (bits < op) {
                          hold += input[_in++] << bits;
                          bits += 8;
                        }
                      }
                      dist += hold & (1 << op) - 1;
                      if (dist > dmax) {
                        strm.msg = "invalid distance too far back";
                        state.mode = BAD;
                        break top;
                      }
                      hold >>>= op;
                      bits -= op;
                      op = _out - beg;
                      if (dist > op) {
                        op = dist - op;
                        if (op > whave) {
                          if (state.sane) {
                            strm.msg = "invalid distance too far back";
                            state.mode = BAD;
                            break top;
                          }
                        }
                        from = 0;
                        from_source = s_window;
                        if (wnext === 0) {
                          from += wsize - op;
                          if (op < len) {
                            len -= op;
                            do {
                              output[_out++] = s_window[from++];
                            } while (--op);
                            from = _out - dist;
                            from_source = output;
                          }
                        } else if (wnext < op) {
                          from += wsize + wnext - op;
                          op -= wnext;
                          if (op < len) {
                            len -= op;
                            do {
                              output[_out++] = s_window[from++];
                            } while (--op);
                            from = 0;
                            if (wnext < len) {
                              op = wnext;
                              len -= op;
                              do {
                                output[_out++] = s_window[from++];
                              } while (--op);
                              from = _out - dist;
                              from_source = output;
                            }
                          }
                        } else {
                          from += wnext - op;
                          if (op < len) {
                            len -= op;
                            do {
                              output[_out++] = s_window[from++];
                            } while (--op);
                            from = _out - dist;
                            from_source = output;
                          }
                        }
                        while (len > 2) {
                          output[_out++] = from_source[from++];
                          output[_out++] = from_source[from++];
                          output[_out++] = from_source[from++];
                          len -= 3;
                        }
                        if (len) {
                          output[_out++] = from_source[from++];
                          if (len > 1) {
                            output[_out++] = from_source[from++];
                          }
                        }
                      } else {
                        from = _out - dist;
                        do {
                          output[_out++] = output[from++];
                          output[_out++] = output[from++];
                          output[_out++] = output[from++];
                          len -= 3;
                        } while (len > 2);
                        if (len) {
                          output[_out++] = output[from++];
                          if (len > 1) {
                            output[_out++] = output[from++];
                          }
                        }
                      }
                    } else if ((op & 64) === 0) {
                      here = dcode[(here & 65535) + (hold & (1 << op) - 1)];
                      continue dodist;
                    } else {
                      strm.msg = "invalid distance code";
                      state.mode = BAD;
                      break top;
                    }
                    break;
                  }
              } else if ((op & 64) === 0) {
                here = lcode[(here & 65535) + (hold & (1 << op) - 1)];
                continue dolen;
              } else if (op & 32) {
                state.mode = TYPE;
                break top;
              } else {
                strm.msg = "invalid literal/length code";
                state.mode = BAD;
                break top;
              }
              break;
            }
        } while (_in < last && _out < end);
      len = bits >> 3;
      _in -= len;
      bits -= len << 3;
      hold &= (1 << bits) - 1;
      strm.next_in = _in;
      strm.next_out = _out;
      strm.avail_in = _in < last ? 5 + (last - _in) : 5 - (_in - last);
      strm.avail_out = _out < end ? 257 + (end - _out) : 257 - (_out - end);
      state.hold = hold;
      state.bits = bits;
      return;
    };
  }
});
var require_inftrees = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/inftrees.js"(exports, module) {
    "use strict";
    var utils = require_common();
    var MAXBITS = 15;
    var ENOUGH_LENS = 852;
    var ENOUGH_DISTS = 592;
    var CODES = 0;
    var LENS = 1;
    var DISTS = 2;
    var lbase = [
      /* Length codes 257..285 base */
      3,
      4,
      5,
      6,
      7,
      8,
      9,
      10,
      11,
      13,
      15,
      17,
      19,
      23,
      27,
      31,
      35,
      43,
      51,
      59,
      67,
      83,
      99,
      115,
      131,
      163,
      195,
      227,
      258,
      0,
      0
    ];
    var lext = [
      /* Length codes 257..285 extra */
      16,
      16,
      16,
      16,
      16,
      16,
      16,
      16,
      17,
      17,
      17,
      17,
      18,
      18,
      18,
      18,
      19,
      19,
      19,
      19,
      20,
      20,
      20,
      20,
      21,
      21,
      21,
      21,
      16,
      72,
      78
    ];
    var dbase = [
      /* Distance codes 0..29 base */
      1,
      2,
      3,
      4,
      5,
      7,
      9,
      13,
      17,
      25,
      33,
      49,
      65,
      97,
      129,
      193,
      257,
      385,
      513,
      769,
      1025,
      1537,
      2049,
      3073,
      4097,
      6145,
      8193,
      12289,
      16385,
      24577,
      0,
      0
    ];
    var dext = [
      /* Distance codes 0..29 extra */
      16,
      16,
      16,
      16,
      17,
      17,
      18,
      18,
      19,
      19,
      20,
      20,
      21,
      21,
      22,
      22,
      23,
      23,
      24,
      24,
      25,
      25,
      26,
      26,
      27,
      27,
      28,
      28,
      29,
      29,
      64,
      64
    ];
    module.exports = function inflate_table(type, lens, lens_index, codes, table, table_index, work, opts) {
      var bits = opts.bits;
      var len = 0;
      var sym = 0;
      var min = 0, max = 0;
      var root = 0;
      var curr = 0;
      var drop = 0;
      var left = 0;
      var used = 0;
      var huff = 0;
      var incr;
      var fill;
      var low;
      var mask;
      var next;
      var base = null;
      var base_index = 0;
      var end;
      var count = new utils.Buf16(MAXBITS + 1);
      var offs = new utils.Buf16(MAXBITS + 1);
      var extra = null;
      var extra_index = 0;
      var here_bits, here_op, here_val;
      for (len = 0; len <= MAXBITS; len++) {
        count[len] = 0;
      }
      for (sym = 0; sym < codes; sym++) {
        count[lens[lens_index + sym]]++;
      }
      root = bits;
      for (max = MAXBITS; max >= 1; max--) {
        if (count[max] !== 0) {
          break;
        }
      }
      if (root > max) {
        root = max;
      }
      if (max === 0) {
        table[table_index++] = 1 << 24 | 64 << 16 | 0;
        table[table_index++] = 1 << 24 | 64 << 16 | 0;
        opts.bits = 1;
        return 0;
      }
      for (min = 1; min < max; min++) {
        if (count[min] !== 0) {
          break;
        }
      }
      if (root < min) {
        root = min;
      }
      left = 1;
      for (len = 1; len <= MAXBITS; len++) {
        left <<= 1;
        left -= count[len];
        if (left < 0) {
          return -1;
        }
      }
      if (left > 0 && (type === CODES || max !== 1)) {
        return -1;
      }
      offs[1] = 0;
      for (len = 1; len < MAXBITS; len++) {
        offs[len + 1] = offs[len] + count[len];
      }
      for (sym = 0; sym < codes; sym++) {
        if (lens[lens_index + sym] !== 0) {
          work[offs[lens[lens_index + sym]]++] = sym;
        }
      }
      if (type === CODES) {
        base = extra = work;
        end = 19;
      } else if (type === LENS) {
        base = lbase;
        base_index -= 257;
        extra = lext;
        extra_index -= 257;
        end = 256;
      } else {
        base = dbase;
        extra = dext;
        end = -1;
      }
      huff = 0;
      sym = 0;
      len = min;
      next = table_index;
      curr = root;
      drop = 0;
      low = -1;
      used = 1 << root;
      mask = used - 1;
      if (type === LENS && used > ENOUGH_LENS || type === DISTS && used > ENOUGH_DISTS) {
        return 1;
      }
      for (; ; ) {
        here_bits = len - drop;
        if (work[sym] < end) {
          here_op = 0;
          here_val = work[sym];
        } else if (work[sym] > end) {
          here_op = extra[extra_index + work[sym]];
          here_val = base[base_index + work[sym]];
        } else {
          here_op = 32 + 64;
          here_val = 0;
        }
        incr = 1 << len - drop;
        fill = 1 << curr;
        min = fill;
        do {
          fill -= incr;
          table[next + (huff >> drop) + fill] = here_bits << 24 | here_op << 16 | here_val | 0;
        } while (fill !== 0);
        incr = 1 << len - 1;
        while (huff & incr) {
          incr >>= 1;
        }
        if (incr !== 0) {
          huff &= incr - 1;
          huff += incr;
        } else {
          huff = 0;
        }
        sym++;
        if (--count[len] === 0) {
          if (len === max) {
            break;
          }
          len = lens[lens_index + work[sym]];
        }
        if (len > root && (huff & mask) !== low) {
          if (drop === 0) {
            drop = root;
          }
          next += min;
          curr = len - drop;
          left = 1 << curr;
          while (curr + drop < max) {
            left -= count[curr + drop];
            if (left <= 0) {
              break;
            }
            curr++;
            left <<= 1;
          }
          used += 1 << curr;
          if (type === LENS && used > ENOUGH_LENS || type === DISTS && used > ENOUGH_DISTS) {
            return 1;
          }
          low = huff & mask;
          table[low] = root << 24 | curr << 16 | next - table_index | 0;
        }
      }
      if (huff !== 0) {
        table[next + huff] = len - drop << 24 | 64 << 16 | 0;
      }
      opts.bits = root;
      return 0;
    };
  }
});
var require_inflate = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/inflate.js"(exports) {
    "use strict";
    var utils = require_common();
    var adler32 = require_adler32();
    var crc32 = require_crc322();
    var inflate_fast = require_inffast();
    var inflate_table = require_inftrees();
    var CODES = 0;
    var LENS = 1;
    var DISTS = 2;
    var Z_FINISH = 4;
    var Z_BLOCK = 5;
    var Z_TREES = 6;
    var Z_OK = 0;
    var Z_STREAM_END = 1;
    var Z_NEED_DICT = 2;
    var Z_STREAM_ERROR = -2;
    var Z_DATA_ERROR = -3;
    var Z_MEM_ERROR = -4;
    var Z_BUF_ERROR = -5;
    var Z_DEFLATED = 8;
    var HEAD = 1;
    var FLAGS = 2;
    var TIME = 3;
    var OS = 4;
    var EXLEN = 5;
    var EXTRA = 6;
    var NAME = 7;
    var COMMENT = 8;
    var HCRC = 9;
    var DICTID = 10;
    var DICT = 11;
    var TYPE = 12;
    var TYPEDO = 13;
    var STORED = 14;
    var COPY_ = 15;
    var COPY = 16;
    var TABLE = 17;
    var LENLENS = 18;
    var CODELENS = 19;
    var LEN_ = 20;
    var LEN = 21;
    var LENEXT = 22;
    var DIST = 23;
    var DISTEXT = 24;
    var MATCH = 25;
    var LIT = 26;
    var CHECK = 27;
    var LENGTH = 28;
    var DONE = 29;
    var BAD = 30;
    var MEM = 31;
    var SYNC = 32;
    var ENOUGH_LENS = 852;
    var ENOUGH_DISTS = 592;
    var MAX_WBITS = 15;
    var DEF_WBITS = MAX_WBITS;
    function zswap32(q) {
      return (q >>> 24 & 255) + (q >>> 8 & 65280) + ((q & 65280) << 8) + ((q & 255) << 24);
    }
    function InflateState() {
      this.mode = 0;
      this.last = false;
      this.wrap = 0;
      this.havedict = false;
      this.flags = 0;
      this.dmax = 0;
      this.check = 0;
      this.total = 0;
      this.head = null;
      this.wbits = 0;
      this.wsize = 0;
      this.whave = 0;
      this.wnext = 0;
      this.window = null;
      this.hold = 0;
      this.bits = 0;
      this.length = 0;
      this.offset = 0;
      this.extra = 0;
      this.lencode = null;
      this.distcode = null;
      this.lenbits = 0;
      this.distbits = 0;
      this.ncode = 0;
      this.nlen = 0;
      this.ndist = 0;
      this.have = 0;
      this.next = null;
      this.lens = new utils.Buf16(320);
      this.work = new utils.Buf16(288);
      this.lendyn = null;
      this.distdyn = null;
      this.sane = 0;
      this.back = 0;
      this.was = 0;
    }
    function inflateResetKeep(strm) {
      var state;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      strm.total_in = strm.total_out = state.total = 0;
      strm.msg = "";
      if (state.wrap) {
        strm.adler = state.wrap & 1;
      }
      state.mode = HEAD;
      state.last = 0;
      state.havedict = 0;
      state.dmax = 32768;
      state.head = null;
      state.hold = 0;
      state.bits = 0;
      state.lencode = state.lendyn = new utils.Buf32(ENOUGH_LENS);
      state.distcode = state.distdyn = new utils.Buf32(ENOUGH_DISTS);
      state.sane = 1;
      state.back = -1;
      return Z_OK;
    }
    function inflateReset(strm) {
      var state;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      state.wsize = 0;
      state.whave = 0;
      state.wnext = 0;
      return inflateResetKeep(strm);
    }
    function inflateReset2(strm, windowBits) {
      var wrap;
      var state;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      if (windowBits < 0) {
        wrap = 0;
        windowBits = -windowBits;
      } else {
        wrap = (windowBits >> 4) + 1;
        if (windowBits < 48) {
          windowBits &= 15;
        }
      }
      if (windowBits && (windowBits < 8 || windowBits > 15)) {
        return Z_STREAM_ERROR;
      }
      if (state.window !== null && state.wbits !== windowBits) {
        state.window = null;
      }
      state.wrap = wrap;
      state.wbits = windowBits;
      return inflateReset(strm);
    }
    function inflateInit2(strm, windowBits) {
      var ret;
      var state;
      if (!strm) {
        return Z_STREAM_ERROR;
      }
      state = new InflateState();
      strm.state = state;
      state.window = null;
      ret = inflateReset2(strm, windowBits);
      if (ret !== Z_OK) {
        strm.state = null;
      }
      return ret;
    }
    function inflateInit(strm) {
      return inflateInit2(strm, DEF_WBITS);
    }
    var virgin = true;
    var lenfix;
    var distfix;
    function fixedtables(state) {
      if (virgin) {
        var sym;
        lenfix = new utils.Buf32(512);
        distfix = new utils.Buf32(32);
        sym = 0;
        while (sym < 144) {
          state.lens[sym++] = 8;
        }
        while (sym < 256) {
          state.lens[sym++] = 9;
        }
        while (sym < 280) {
          state.lens[sym++] = 7;
        }
        while (sym < 288) {
          state.lens[sym++] = 8;
        }
        inflate_table(LENS, state.lens, 0, 288, lenfix, 0, state.work, { bits: 9 });
        sym = 0;
        while (sym < 32) {
          state.lens[sym++] = 5;
        }
        inflate_table(DISTS, state.lens, 0, 32, distfix, 0, state.work, { bits: 5 });
        virgin = false;
      }
      state.lencode = lenfix;
      state.lenbits = 9;
      state.distcode = distfix;
      state.distbits = 5;
    }
    function updatewindow(strm, src, end, copy) {
      var dist;
      var state = strm.state;
      if (state.window === null) {
        state.wsize = 1 << state.wbits;
        state.wnext = 0;
        state.whave = 0;
        state.window = new utils.Buf8(state.wsize);
      }
      if (copy >= state.wsize) {
        utils.arraySet(state.window, src, end - state.wsize, state.wsize, 0);
        state.wnext = 0;
        state.whave = state.wsize;
      } else {
        dist = state.wsize - state.wnext;
        if (dist > copy) {
          dist = copy;
        }
        utils.arraySet(state.window, src, end - copy, dist, state.wnext);
        copy -= dist;
        if (copy) {
          utils.arraySet(state.window, src, end - copy, copy, 0);
          state.wnext = copy;
          state.whave = state.wsize;
        } else {
          state.wnext += dist;
          if (state.wnext === state.wsize) {
            state.wnext = 0;
          }
          if (state.whave < state.wsize) {
            state.whave += dist;
          }
        }
      }
      return 0;
    }
    function inflate(strm, flush) {
      var state;
      var input, output;
      var next;
      var put;
      var have, left;
      var hold;
      var bits;
      var _in, _out;
      var copy;
      var from;
      var from_source;
      var here = 0;
      var here_bits, here_op, here_val;
      var last_bits, last_op, last_val;
      var len;
      var ret;
      var hbuf = new utils.Buf8(4);
      var opts;
      var n;
      var order = (
        /* permutation of code lengths */
        [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15]
      );
      if (!strm || !strm.state || !strm.output || !strm.input && strm.avail_in !== 0) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      if (state.mode === TYPE) {
        state.mode = TYPEDO;
      }
      put = strm.next_out;
      output = strm.output;
      left = strm.avail_out;
      next = strm.next_in;
      input = strm.input;
      have = strm.avail_in;
      hold = state.hold;
      bits = state.bits;
      _in = have;
      _out = left;
      ret = Z_OK;
      inf_leave:
        for (; ; ) {
          switch (state.mode) {
            case HEAD:
              if (state.wrap === 0) {
                state.mode = TYPEDO;
                break;
              }
              while (bits < 16) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if (state.wrap & 2 && hold === 35615) {
                state.check = 0;
                hbuf[0] = hold & 255;
                hbuf[1] = hold >>> 8 & 255;
                state.check = crc32(state.check, hbuf, 2, 0);
                hold = 0;
                bits = 0;
                state.mode = FLAGS;
                break;
              }
              state.flags = 0;
              if (state.head) {
                state.head.done = false;
              }
              if (!(state.wrap & 1) || /* check if zlib header allowed */
              (((hold & 255) << 8) + (hold >> 8)) % 31) {
                strm.msg = "incorrect header check";
                state.mode = BAD;
                break;
              }
              if ((hold & 15) !== Z_DEFLATED) {
                strm.msg = "unknown compression method";
                state.mode = BAD;
                break;
              }
              hold >>>= 4;
              bits -= 4;
              len = (hold & 15) + 8;
              if (state.wbits === 0) {
                state.wbits = len;
              } else if (len > state.wbits) {
                strm.msg = "invalid window size";
                state.mode = BAD;
                break;
              }
              state.dmax = 1 << len;
              strm.adler = state.check = 1;
              state.mode = hold & 512 ? DICTID : TYPE;
              hold = 0;
              bits = 0;
              break;
            case FLAGS:
              while (bits < 16) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              state.flags = hold;
              if ((state.flags & 255) !== Z_DEFLATED) {
                strm.msg = "unknown compression method";
                state.mode = BAD;
                break;
              }
              if (state.flags & 57344) {
                strm.msg = "unknown header flags set";
                state.mode = BAD;
                break;
              }
              if (state.head) {
                state.head.text = hold >> 8 & 1;
              }
              if (state.flags & 512) {
                hbuf[0] = hold & 255;
                hbuf[1] = hold >>> 8 & 255;
                state.check = crc32(state.check, hbuf, 2, 0);
              }
              hold = 0;
              bits = 0;
              state.mode = TIME;
            /* falls through */
            case TIME:
              while (bits < 32) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if (state.head) {
                state.head.time = hold;
              }
              if (state.flags & 512) {
                hbuf[0] = hold & 255;
                hbuf[1] = hold >>> 8 & 255;
                hbuf[2] = hold >>> 16 & 255;
                hbuf[3] = hold >>> 24 & 255;
                state.check = crc32(state.check, hbuf, 4, 0);
              }
              hold = 0;
              bits = 0;
              state.mode = OS;
            /* falls through */
            case OS:
              while (bits < 16) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if (state.head) {
                state.head.xflags = hold & 255;
                state.head.os = hold >> 8;
              }
              if (state.flags & 512) {
                hbuf[0] = hold & 255;
                hbuf[1] = hold >>> 8 & 255;
                state.check = crc32(state.check, hbuf, 2, 0);
              }
              hold = 0;
              bits = 0;
              state.mode = EXLEN;
            /* falls through */
            case EXLEN:
              if (state.flags & 1024) {
                while (bits < 16) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                state.length = hold;
                if (state.head) {
                  state.head.extra_len = hold;
                }
                if (state.flags & 512) {
                  hbuf[0] = hold & 255;
                  hbuf[1] = hold >>> 8 & 255;
                  state.check = crc32(state.check, hbuf, 2, 0);
                }
                hold = 0;
                bits = 0;
              } else if (state.head) {
                state.head.extra = null;
              }
              state.mode = EXTRA;
            /* falls through */
            case EXTRA:
              if (state.flags & 1024) {
                copy = state.length;
                if (copy > have) {
                  copy = have;
                }
                if (copy) {
                  if (state.head) {
                    len = state.head.extra_len - state.length;
                    if (!state.head.extra) {
                      state.head.extra = new Array(state.head.extra_len);
                    }
                    utils.arraySet(
                      state.head.extra,
                      input,
                      next,
                      // extra field is limited to 65536 bytes
                      // - no need for additional size check
                      copy,
                      /*len + copy > state.head.extra_max - len ? state.head.extra_max : copy,*/
                      len
                    );
                  }
                  if (state.flags & 512) {
                    state.check = crc32(state.check, input, copy, next);
                  }
                  have -= copy;
                  next += copy;
                  state.length -= copy;
                }
                if (state.length) {
                  break inf_leave;
                }
              }
              state.length = 0;
              state.mode = NAME;
            /* falls through */
            case NAME:
              if (state.flags & 2048) {
                if (have === 0) {
                  break inf_leave;
                }
                copy = 0;
                do {
                  len = input[next + copy++];
                  if (state.head && len && state.length < 65536) {
                    state.head.name += String.fromCharCode(len);
                  }
                } while (len && copy < have);
                if (state.flags & 512) {
                  state.check = crc32(state.check, input, copy, next);
                }
                have -= copy;
                next += copy;
                if (len) {
                  break inf_leave;
                }
              } else if (state.head) {
                state.head.name = null;
              }
              state.length = 0;
              state.mode = COMMENT;
            /* falls through */
            case COMMENT:
              if (state.flags & 4096) {
                if (have === 0) {
                  break inf_leave;
                }
                copy = 0;
                do {
                  len = input[next + copy++];
                  if (state.head && len && state.length < 65536) {
                    state.head.comment += String.fromCharCode(len);
                  }
                } while (len && copy < have);
                if (state.flags & 512) {
                  state.check = crc32(state.check, input, copy, next);
                }
                have -= copy;
                next += copy;
                if (len) {
                  break inf_leave;
                }
              } else if (state.head) {
                state.head.comment = null;
              }
              state.mode = HCRC;
            /* falls through */
            case HCRC:
              if (state.flags & 512) {
                while (bits < 16) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                if (hold !== (state.check & 65535)) {
                  strm.msg = "header crc mismatch";
                  state.mode = BAD;
                  break;
                }
                hold = 0;
                bits = 0;
              }
              if (state.head) {
                state.head.hcrc = state.flags >> 9 & 1;
                state.head.done = true;
              }
              strm.adler = state.check = 0;
              state.mode = TYPE;
              break;
            case DICTID:
              while (bits < 32) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              strm.adler = state.check = zswap32(hold);
              hold = 0;
              bits = 0;
              state.mode = DICT;
            /* falls through */
            case DICT:
              if (state.havedict === 0) {
                strm.next_out = put;
                strm.avail_out = left;
                strm.next_in = next;
                strm.avail_in = have;
                state.hold = hold;
                state.bits = bits;
                return Z_NEED_DICT;
              }
              strm.adler = state.check = 1;
              state.mode = TYPE;
            /* falls through */
            case TYPE:
              if (flush === Z_BLOCK || flush === Z_TREES) {
                break inf_leave;
              }
            /* falls through */
            case TYPEDO:
              if (state.last) {
                hold >>>= bits & 7;
                bits -= bits & 7;
                state.mode = CHECK;
                break;
              }
              while (bits < 3) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              state.last = hold & 1;
              hold >>>= 1;
              bits -= 1;
              switch (hold & 3) {
                case 0:
                  state.mode = STORED;
                  break;
                case 1:
                  fixedtables(state);
                  state.mode = LEN_;
                  if (flush === Z_TREES) {
                    hold >>>= 2;
                    bits -= 2;
                    break inf_leave;
                  }
                  break;
                case 2:
                  state.mode = TABLE;
                  break;
                case 3:
                  strm.msg = "invalid block type";
                  state.mode = BAD;
              }
              hold >>>= 2;
              bits -= 2;
              break;
            case STORED:
              hold >>>= bits & 7;
              bits -= bits & 7;
              while (bits < 32) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if ((hold & 65535) !== (hold >>> 16 ^ 65535)) {
                strm.msg = "invalid stored block lengths";
                state.mode = BAD;
                break;
              }
              state.length = hold & 65535;
              hold = 0;
              bits = 0;
              state.mode = COPY_;
              if (flush === Z_TREES) {
                break inf_leave;
              }
            /* falls through */
            case COPY_:
              state.mode = COPY;
            /* falls through */
            case COPY:
              copy = state.length;
              if (copy) {
                if (copy > have) {
                  copy = have;
                }
                if (copy > left) {
                  copy = left;
                }
                if (copy === 0) {
                  break inf_leave;
                }
                utils.arraySet(output, input, next, copy, put);
                have -= copy;
                next += copy;
                left -= copy;
                put += copy;
                state.length -= copy;
                break;
              }
              state.mode = TYPE;
              break;
            case TABLE:
              while (bits < 14) {
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              state.nlen = (hold & 31) + 257;
              hold >>>= 5;
              bits -= 5;
              state.ndist = (hold & 31) + 1;
              hold >>>= 5;
              bits -= 5;
              state.ncode = (hold & 15) + 4;
              hold >>>= 4;
              bits -= 4;
              if (state.nlen > 286 || state.ndist > 30) {
                strm.msg = "too many length or distance symbols";
                state.mode = BAD;
                break;
              }
              state.have = 0;
              state.mode = LENLENS;
            /* falls through */
            case LENLENS:
              while (state.have < state.ncode) {
                while (bits < 3) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                state.lens[order[state.have++]] = hold & 7;
                hold >>>= 3;
                bits -= 3;
              }
              while (state.have < 19) {
                state.lens[order[state.have++]] = 0;
              }
              state.lencode = state.lendyn;
              state.lenbits = 7;
              opts = { bits: state.lenbits };
              ret = inflate_table(CODES, state.lens, 0, 19, state.lencode, 0, state.work, opts);
              state.lenbits = opts.bits;
              if (ret) {
                strm.msg = "invalid code lengths set";
                state.mode = BAD;
                break;
              }
              state.have = 0;
              state.mode = CODELENS;
            /* falls through */
            case CODELENS:
              while (state.have < state.nlen + state.ndist) {
                for (; ; ) {
                  here = state.lencode[hold & (1 << state.lenbits) - 1];
                  here_bits = here >>> 24;
                  here_op = here >>> 16 & 255;
                  here_val = here & 65535;
                  if (here_bits <= bits) {
                    break;
                  }
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                if (here_val < 16) {
                  hold >>>= here_bits;
                  bits -= here_bits;
                  state.lens[state.have++] = here_val;
                } else {
                  if (here_val === 16) {
                    n = here_bits + 2;
                    while (bits < n) {
                      if (have === 0) {
                        break inf_leave;
                      }
                      have--;
                      hold += input[next++] << bits;
                      bits += 8;
                    }
                    hold >>>= here_bits;
                    bits -= here_bits;
                    if (state.have === 0) {
                      strm.msg = "invalid bit length repeat";
                      state.mode = BAD;
                      break;
                    }
                    len = state.lens[state.have - 1];
                    copy = 3 + (hold & 3);
                    hold >>>= 2;
                    bits -= 2;
                  } else if (here_val === 17) {
                    n = here_bits + 3;
                    while (bits < n) {
                      if (have === 0) {
                        break inf_leave;
                      }
                      have--;
                      hold += input[next++] << bits;
                      bits += 8;
                    }
                    hold >>>= here_bits;
                    bits -= here_bits;
                    len = 0;
                    copy = 3 + (hold & 7);
                    hold >>>= 3;
                    bits -= 3;
                  } else {
                    n = here_bits + 7;
                    while (bits < n) {
                      if (have === 0) {
                        break inf_leave;
                      }
                      have--;
                      hold += input[next++] << bits;
                      bits += 8;
                    }
                    hold >>>= here_bits;
                    bits -= here_bits;
                    len = 0;
                    copy = 11 + (hold & 127);
                    hold >>>= 7;
                    bits -= 7;
                  }
                  if (state.have + copy > state.nlen + state.ndist) {
                    strm.msg = "invalid bit length repeat";
                    state.mode = BAD;
                    break;
                  }
                  while (copy--) {
                    state.lens[state.have++] = len;
                  }
                }
              }
              if (state.mode === BAD) {
                break;
              }
              if (state.lens[256] === 0) {
                strm.msg = "invalid code -- missing end-of-block";
                state.mode = BAD;
                break;
              }
              state.lenbits = 9;
              opts = { bits: state.lenbits };
              ret = inflate_table(LENS, state.lens, 0, state.nlen, state.lencode, 0, state.work, opts);
              state.lenbits = opts.bits;
              if (ret) {
                strm.msg = "invalid literal/lengths set";
                state.mode = BAD;
                break;
              }
              state.distbits = 6;
              state.distcode = state.distdyn;
              opts = { bits: state.distbits };
              ret = inflate_table(DISTS, state.lens, state.nlen, state.ndist, state.distcode, 0, state.work, opts);
              state.distbits = opts.bits;
              if (ret) {
                strm.msg = "invalid distances set";
                state.mode = BAD;
                break;
              }
              state.mode = LEN_;
              if (flush === Z_TREES) {
                break inf_leave;
              }
            /* falls through */
            case LEN_:
              state.mode = LEN;
            /* falls through */
            case LEN:
              if (have >= 6 && left >= 258) {
                strm.next_out = put;
                strm.avail_out = left;
                strm.next_in = next;
                strm.avail_in = have;
                state.hold = hold;
                state.bits = bits;
                inflate_fast(strm, _out);
                put = strm.next_out;
                output = strm.output;
                left = strm.avail_out;
                next = strm.next_in;
                input = strm.input;
                have = strm.avail_in;
                hold = state.hold;
                bits = state.bits;
                if (state.mode === TYPE) {
                  state.back = -1;
                }
                break;
              }
              state.back = 0;
              for (; ; ) {
                here = state.lencode[hold & (1 << state.lenbits) - 1];
                here_bits = here >>> 24;
                here_op = here >>> 16 & 255;
                here_val = here & 65535;
                if (here_bits <= bits) {
                  break;
                }
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if (here_op && (here_op & 240) === 0) {
                last_bits = here_bits;
                last_op = here_op;
                last_val = here_val;
                for (; ; ) {
                  here = state.lencode[last_val + ((hold & (1 << last_bits + last_op) - 1) >> last_bits)];
                  here_bits = here >>> 24;
                  here_op = here >>> 16 & 255;
                  here_val = here & 65535;
                  if (last_bits + here_bits <= bits) {
                    break;
                  }
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                hold >>>= last_bits;
                bits -= last_bits;
                state.back += last_bits;
              }
              hold >>>= here_bits;
              bits -= here_bits;
              state.back += here_bits;
              state.length = here_val;
              if (here_op === 0) {
                state.mode = LIT;
                break;
              }
              if (here_op & 32) {
                state.back = -1;
                state.mode = TYPE;
                break;
              }
              if (here_op & 64) {
                strm.msg = "invalid literal/length code";
                state.mode = BAD;
                break;
              }
              state.extra = here_op & 15;
              state.mode = LENEXT;
            /* falls through */
            case LENEXT:
              if (state.extra) {
                n = state.extra;
                while (bits < n) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                state.length += hold & (1 << state.extra) - 1;
                hold >>>= state.extra;
                bits -= state.extra;
                state.back += state.extra;
              }
              state.was = state.length;
              state.mode = DIST;
            /* falls through */
            case DIST:
              for (; ; ) {
                here = state.distcode[hold & (1 << state.distbits) - 1];
                here_bits = here >>> 24;
                here_op = here >>> 16 & 255;
                here_val = here & 65535;
                if (here_bits <= bits) {
                  break;
                }
                if (have === 0) {
                  break inf_leave;
                }
                have--;
                hold += input[next++] << bits;
                bits += 8;
              }
              if ((here_op & 240) === 0) {
                last_bits = here_bits;
                last_op = here_op;
                last_val = here_val;
                for (; ; ) {
                  here = state.distcode[last_val + ((hold & (1 << last_bits + last_op) - 1) >> last_bits)];
                  here_bits = here >>> 24;
                  here_op = here >>> 16 & 255;
                  here_val = here & 65535;
                  if (last_bits + here_bits <= bits) {
                    break;
                  }
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                hold >>>= last_bits;
                bits -= last_bits;
                state.back += last_bits;
              }
              hold >>>= here_bits;
              bits -= here_bits;
              state.back += here_bits;
              if (here_op & 64) {
                strm.msg = "invalid distance code";
                state.mode = BAD;
                break;
              }
              state.offset = here_val;
              state.extra = here_op & 15;
              state.mode = DISTEXT;
            /* falls through */
            case DISTEXT:
              if (state.extra) {
                n = state.extra;
                while (bits < n) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                state.offset += hold & (1 << state.extra) - 1;
                hold >>>= state.extra;
                bits -= state.extra;
                state.back += state.extra;
              }
              if (state.offset > state.dmax) {
                strm.msg = "invalid distance too far back";
                state.mode = BAD;
                break;
              }
              state.mode = MATCH;
            /* falls through */
            case MATCH:
              if (left === 0) {
                break inf_leave;
              }
              copy = _out - left;
              if (state.offset > copy) {
                copy = state.offset - copy;
                if (copy > state.whave) {
                  if (state.sane) {
                    strm.msg = "invalid distance too far back";
                    state.mode = BAD;
                    break;
                  }
                }
                if (copy > state.wnext) {
                  copy -= state.wnext;
                  from = state.wsize - copy;
                } else {
                  from = state.wnext - copy;
                }
                if (copy > state.length) {
                  copy = state.length;
                }
                from_source = state.window;
              } else {
                from_source = output;
                from = put - state.offset;
                copy = state.length;
              }
              if (copy > left) {
                copy = left;
              }
              left -= copy;
              state.length -= copy;
              do {
                output[put++] = from_source[from++];
              } while (--copy);
              if (state.length === 0) {
                state.mode = LEN;
              }
              break;
            case LIT:
              if (left === 0) {
                break inf_leave;
              }
              output[put++] = state.length;
              left--;
              state.mode = LEN;
              break;
            case CHECK:
              if (state.wrap) {
                while (bits < 32) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold |= input[next++] << bits;
                  bits += 8;
                }
                _out -= left;
                strm.total_out += _out;
                state.total += _out;
                if (_out) {
                  strm.adler = state.check = /*UPDATE(state.check, put - _out, _out);*/
                  state.flags ? crc32(state.check, output, _out, put - _out) : adler32(state.check, output, _out, put - _out);
                }
                _out = left;
                if ((state.flags ? hold : zswap32(hold)) !== state.check) {
                  strm.msg = "incorrect data check";
                  state.mode = BAD;
                  break;
                }
                hold = 0;
                bits = 0;
              }
              state.mode = LENGTH;
            /* falls through */
            case LENGTH:
              if (state.wrap && state.flags) {
                while (bits < 32) {
                  if (have === 0) {
                    break inf_leave;
                  }
                  have--;
                  hold += input[next++] << bits;
                  bits += 8;
                }
                if (hold !== (state.total & 4294967295)) {
                  strm.msg = "incorrect length check";
                  state.mode = BAD;
                  break;
                }
                hold = 0;
                bits = 0;
              }
              state.mode = DONE;
            /* falls through */
            case DONE:
              ret = Z_STREAM_END;
              break inf_leave;
            case BAD:
              ret = Z_DATA_ERROR;
              break inf_leave;
            case MEM:
              return Z_MEM_ERROR;
            case SYNC:
            /* falls through */
            default:
              return Z_STREAM_ERROR;
          }
        }
      strm.next_out = put;
      strm.avail_out = left;
      strm.next_in = next;
      strm.avail_in = have;
      state.hold = hold;
      state.bits = bits;
      if (state.wsize || _out !== strm.avail_out && state.mode < BAD && (state.mode < CHECK || flush !== Z_FINISH)) {
        if (updatewindow(strm, strm.output, strm.next_out, _out - strm.avail_out)) {
          state.mode = MEM;
          return Z_MEM_ERROR;
        }
      }
      _in -= strm.avail_in;
      _out -= strm.avail_out;
      strm.total_in += _in;
      strm.total_out += _out;
      state.total += _out;
      if (state.wrap && _out) {
        strm.adler = state.check = /*UPDATE(state.check, strm.next_out - _out, _out);*/
        state.flags ? crc32(state.check, output, _out, strm.next_out - _out) : adler32(state.check, output, _out, strm.next_out - _out);
      }
      strm.data_type = state.bits + (state.last ? 64 : 0) + (state.mode === TYPE ? 128 : 0) + (state.mode === LEN_ || state.mode === COPY_ ? 256 : 0);
      if ((_in === 0 && _out === 0 || flush === Z_FINISH) && ret === Z_OK) {
        ret = Z_BUF_ERROR;
      }
      return ret;
    }
    function inflateEnd(strm) {
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      var state = strm.state;
      if (state.window) {
        state.window = null;
      }
      strm.state = null;
      return Z_OK;
    }
    function inflateGetHeader(strm, head) {
      var state;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      if ((state.wrap & 2) === 0) {
        return Z_STREAM_ERROR;
      }
      state.head = head;
      head.done = false;
      return Z_OK;
    }
    function inflateSetDictionary(strm, dictionary) {
      var dictLength = dictionary.length;
      var state;
      var dictid;
      var ret;
      if (!strm || !strm.state) {
        return Z_STREAM_ERROR;
      }
      state = strm.state;
      if (state.wrap !== 0 && state.mode !== DICT) {
        return Z_STREAM_ERROR;
      }
      if (state.mode === DICT) {
        dictid = 1;
        dictid = adler32(dictid, dictionary, dictLength, 0);
        if (dictid !== state.check) {
          return Z_DATA_ERROR;
        }
      }
      ret = updatewindow(strm, dictionary, dictLength, dictLength);
      if (ret) {
        state.mode = MEM;
        return Z_MEM_ERROR;
      }
      state.havedict = 1;
      return Z_OK;
    }
    exports.inflateReset = inflateReset;
    exports.inflateReset2 = inflateReset2;
    exports.inflateResetKeep = inflateResetKeep;
    exports.inflateInit = inflateInit;
    exports.inflateInit2 = inflateInit2;
    exports.inflate = inflate;
    exports.inflateEnd = inflateEnd;
    exports.inflateGetHeader = inflateGetHeader;
    exports.inflateSetDictionary = inflateSetDictionary;
    exports.inflateInfo = "pako inflate (from Nodeca project)";
  }
});
var require_constants = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/constants.js"(exports, module) {
    "use strict";
    module.exports = {
      /* Allowed flush values; see deflate() and inflate() below for details */
      Z_NO_FLUSH: 0,
      Z_PARTIAL_FLUSH: 1,
      Z_SYNC_FLUSH: 2,
      Z_FULL_FLUSH: 3,
      Z_FINISH: 4,
      Z_BLOCK: 5,
      Z_TREES: 6,
      /* Return codes for the compression/decompression functions. Negative values
      * are errors, positive values are used for special but normal events.
      */
      Z_OK: 0,
      Z_STREAM_END: 1,
      Z_NEED_DICT: 2,
      Z_ERRNO: -1,
      Z_STREAM_ERROR: -2,
      Z_DATA_ERROR: -3,
      //Z_MEM_ERROR:     -4,
      Z_BUF_ERROR: -5,
      //Z_VERSION_ERROR: -6,
      /* compression levels */
      Z_NO_COMPRESSION: 0,
      Z_BEST_SPEED: 1,
      Z_BEST_COMPRESSION: 9,
      Z_DEFAULT_COMPRESSION: -1,
      Z_FILTERED: 1,
      Z_HUFFMAN_ONLY: 2,
      Z_RLE: 3,
      Z_FIXED: 4,
      Z_DEFAULT_STRATEGY: 0,
      /* Possible values of the data_type field (though see inflate()) */
      Z_BINARY: 0,
      Z_TEXT: 1,
      //Z_ASCII:                1, // = Z_TEXT (deprecated)
      Z_UNKNOWN: 2,
      /* The deflate compression method */
      Z_DEFLATED: 8
      //Z_NULL:                 null // Use -1 or null inline, depending on var type
    };
  }
});
var require_gzheader = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/zlib/gzheader.js"(exports, module) {
    "use strict";
    function GZheader() {
      this.text = 0;
      this.time = 0;
      this.xflags = 0;
      this.os = 0;
      this.extra = null;
      this.extra_len = 0;
      this.name = "";
      this.comment = "";
      this.hcrc = 0;
      this.done = false;
    }
    module.exports = GZheader;
  }
});
var require_inflate2 = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/lib/inflate.js"(exports) {
    "use strict";
    var zlib_inflate = require_inflate();
    var utils = require_common();
    var strings = require_strings();
    var c = require_constants();
    var msg = require_messages();
    var ZStream = require_zstream();
    var GZheader = require_gzheader();
    var toString = Object.prototype.toString;
    function Inflate(options) {
      if (!(this instanceof Inflate)) return new Inflate(options);
      this.options = utils.assign({
        chunkSize: 16384,
        windowBits: 0,
        to: ""
      }, options || {});
      var opt = this.options;
      if (opt.raw && opt.windowBits >= 0 && opt.windowBits < 16) {
        opt.windowBits = -opt.windowBits;
        if (opt.windowBits === 0) {
          opt.windowBits = -15;
        }
      }
      if (opt.windowBits >= 0 && opt.windowBits < 16 && !(options && options.windowBits)) {
        opt.windowBits += 32;
      }
      if (opt.windowBits > 15 && opt.windowBits < 48) {
        if ((opt.windowBits & 15) === 0) {
          opt.windowBits |= 15;
        }
      }
      this.err = 0;
      this.msg = "";
      this.ended = false;
      this.chunks = [];
      this.strm = new ZStream();
      this.strm.avail_out = 0;
      var status = zlib_inflate.inflateInit2(
        this.strm,
        opt.windowBits
      );
      if (status !== c.Z_OK) {
        throw new Error(msg[status]);
      }
      this.header = new GZheader();
      zlib_inflate.inflateGetHeader(this.strm, this.header);
      if (opt.dictionary) {
        if (typeof opt.dictionary === "string") {
          opt.dictionary = strings.string2buf(opt.dictionary);
        } else if (toString.call(opt.dictionary) === "[object ArrayBuffer]") {
          opt.dictionary = new Uint8Array(opt.dictionary);
        }
        if (opt.raw) {
          status = zlib_inflate.inflateSetDictionary(this.strm, opt.dictionary);
          if (status !== c.Z_OK) {
            throw new Error(msg[status]);
          }
        }
      }
    }
    Inflate.prototype.push = function(data, mode) {
      var strm = this.strm;
      var chunkSize = this.options.chunkSize;
      var dictionary = this.options.dictionary;
      var status, _mode;
      var next_out_utf8, tail, utf8str;
      var allowBufError = false;
      if (this.ended) {
        return false;
      }
      _mode = mode === ~~mode ? mode : mode === true ? c.Z_FINISH : c.Z_NO_FLUSH;
      if (typeof data === "string") {
        strm.input = strings.binstring2buf(data);
      } else if (toString.call(data) === "[object ArrayBuffer]") {
        strm.input = new Uint8Array(data);
      } else {
        strm.input = data;
      }
      strm.next_in = 0;
      strm.avail_in = strm.input.length;
      do {
        if (strm.avail_out === 0) {
          strm.output = new utils.Buf8(chunkSize);
          strm.next_out = 0;
          strm.avail_out = chunkSize;
        }
        status = zlib_inflate.inflate(strm, c.Z_NO_FLUSH);
        if (status === c.Z_NEED_DICT && dictionary) {
          status = zlib_inflate.inflateSetDictionary(this.strm, dictionary);
        }
        if (status === c.Z_BUF_ERROR && allowBufError === true) {
          status = c.Z_OK;
          allowBufError = false;
        }
        if (status !== c.Z_STREAM_END && status !== c.Z_OK) {
          this.onEnd(status);
          this.ended = true;
          return false;
        }
        if (strm.next_out) {
          if (strm.avail_out === 0 || status === c.Z_STREAM_END || strm.avail_in === 0 && (_mode === c.Z_FINISH || _mode === c.Z_SYNC_FLUSH)) {
            if (this.options.to === "string") {
              next_out_utf8 = strings.utf8border(strm.output, strm.next_out);
              tail = strm.next_out - next_out_utf8;
              utf8str = strings.buf2string(strm.output, next_out_utf8);
              strm.next_out = tail;
              strm.avail_out = chunkSize - tail;
              if (tail) {
                utils.arraySet(strm.output, strm.output, next_out_utf8, tail, 0);
              }
              this.onData(utf8str);
            } else {
              this.onData(utils.shrinkBuf(strm.output, strm.next_out));
            }
          }
        }
        if (strm.avail_in === 0 && strm.avail_out === 0) {
          allowBufError = true;
        }
      } while ((strm.avail_in > 0 || strm.avail_out === 0) && status !== c.Z_STREAM_END);
      if (status === c.Z_STREAM_END) {
        _mode = c.Z_FINISH;
      }
      if (_mode === c.Z_FINISH) {
        status = zlib_inflate.inflateEnd(this.strm);
        this.onEnd(status);
        this.ended = true;
        return status === c.Z_OK;
      }
      if (_mode === c.Z_SYNC_FLUSH) {
        this.onEnd(c.Z_OK);
        strm.avail_out = 0;
        return true;
      }
      return true;
    };
    Inflate.prototype.onData = function(chunk) {
      this.chunks.push(chunk);
    };
    Inflate.prototype.onEnd = function(status) {
      if (status === c.Z_OK) {
        if (this.options.to === "string") {
          this.result = this.chunks.join("");
        } else {
          this.result = utils.flattenChunks(this.chunks);
        }
      }
      this.chunks = [];
      this.err = status;
      this.msg = this.strm.msg;
    };
    function inflate(input, options) {
      var inflator = new Inflate(options);
      inflator.push(input, true);
      if (inflator.err) {
        throw inflator.msg || msg[inflator.err];
      }
      return inflator.result;
    }
    function inflateRaw(input, options) {
      options = options || {};
      options.raw = true;
      return inflate(input, options);
    }
    exports.Inflate = Inflate;
    exports.inflate = inflate;
    exports.inflateRaw = inflateRaw;
    exports.ungzip = inflate;
  }
});
var require_pako = __commonJS({
  "../../node_modules/.pnpm/pako@1.0.11/node_modules/pako/index.js"(exports, module) {
    "use strict";
    var assign = require_common().assign;
    var deflate = require_deflate2();
    var inflate = require_inflate2();
    var constants = require_constants();
    var pako = {};
    assign(pako, deflate, inflate, constants);
    module.exports = pako;
  }
});
var require_flate = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/flate.js"(exports) {
    "use strict";
    var USE_TYPEDARRAY = typeof Uint8Array !== "undefined" && typeof Uint16Array !== "undefined" && typeof Uint32Array !== "undefined";
    var pako = require_pako();
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    var ARRAY_TYPE = USE_TYPEDARRAY ? "uint8array" : "array";
    exports.magic = "\b\0";
    function FlateWorker(action, options) {
      GenericWorker.call(this, "FlateWorker/" + action);
      this._pako = null;
      this._pakoAction = action;
      this._pakoOptions = options;
      this.meta = {};
    }
    utils.inherits(FlateWorker, GenericWorker);
    FlateWorker.prototype.processChunk = function(chunk) {
      this.meta = chunk.meta;
      if (this._pako === null) {
        this._createPako();
      }
      this._pako.push(utils.transformTo(ARRAY_TYPE, chunk.data), false);
    };
    FlateWorker.prototype.flush = function() {
      GenericWorker.prototype.flush.call(this);
      if (this._pako === null) {
        this._createPako();
      }
      this._pako.push([], true);
    };
    FlateWorker.prototype.cleanUp = function() {
      GenericWorker.prototype.cleanUp.call(this);
      this._pako = null;
    };
    FlateWorker.prototype._createPako = function() {
      this._pako = new pako[this._pakoAction]({
        raw: true,
        level: this._pakoOptions.level || -1
        // default compression
      });
      var self2 = this;
      this._pako.onData = function(data) {
        self2.push({
          data,
          meta: self2.meta
        });
      };
    };
    exports.compressWorker = function(compressionOptions) {
      return new FlateWorker("Deflate", compressionOptions);
    };
    exports.uncompressWorker = function() {
      return new FlateWorker("Inflate", {});
    };
  }
});
var require_compressions = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/compressions.js"(exports) {
    "use strict";
    var GenericWorker = require_GenericWorker();
    exports.STORE = {
      magic: "\0\0",
      compressWorker: function() {
        return new GenericWorker("STORE compression");
      },
      uncompressWorker: function() {
        return new GenericWorker("STORE decompression");
      }
    };
    exports.DEFLATE = require_flate();
  }
});
var require_signature = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/signature.js"(exports) {
    "use strict";
    exports.LOCAL_FILE_HEADER = "PK";
    exports.CENTRAL_FILE_HEADER = "PK";
    exports.CENTRAL_DIRECTORY_END = "PK";
    exports.ZIP64_CENTRAL_DIRECTORY_LOCATOR = "PK\x07";
    exports.ZIP64_CENTRAL_DIRECTORY_END = "PK";
    exports.DATA_DESCRIPTOR = "PK\x07\b";
  }
});
var require_ZipFileWorker = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/generate/ZipFileWorker.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    var utf8 = require_utf8();
    var crc32 = require_crc32();
    var signature = require_signature();
    var decToHex = function(dec, bytes) {
      var hex = "", i;
      for (i = 0; i < bytes; i++) {
        hex += String.fromCharCode(dec & 255);
        dec = dec >>> 8;
      }
      return hex;
    };
    var generateUnixExternalFileAttr = function(unixPermissions, isDir) {
      var result = unixPermissions;
      if (!unixPermissions) {
        result = isDir ? 16893 : 33204;
      }
      return (result & 65535) << 16;
    };
    var generateDosExternalFileAttr = function(dosPermissions) {
      return (dosPermissions || 0) & 63;
    };
    var generateZipParts = function(streamInfo, streamedContent, streamingEnded, offset, platform, encodeFileName) {
      var file = streamInfo["file"], compression = streamInfo["compression"], useCustomEncoding = encodeFileName !== utf8.utf8encode, encodedFileName = utils.transformTo("string", encodeFileName(file.name)), utfEncodedFileName = utils.transformTo("string", utf8.utf8encode(file.name)), comment = file.comment, encodedComment = utils.transformTo("string", encodeFileName(comment)), utfEncodedComment = utils.transformTo("string", utf8.utf8encode(comment)), useUTF8ForFileName = utfEncodedFileName.length !== file.name.length, useUTF8ForComment = utfEncodedComment.length !== comment.length, dosTime, dosDate, extraFields = "", unicodePathExtraField = "", unicodeCommentExtraField = "", dir = file.dir, date = file.date;
      var dataInfo = {
        crc32: 0,
        compressedSize: 0,
        uncompressedSize: 0
      };
      if (!streamedContent || streamingEnded) {
        dataInfo.crc32 = streamInfo["crc32"];
        dataInfo.compressedSize = streamInfo["compressedSize"];
        dataInfo.uncompressedSize = streamInfo["uncompressedSize"];
      }
      var bitflag = 0;
      if (streamedContent) {
        bitflag |= 8;
      }
      if (!useCustomEncoding && (useUTF8ForFileName || useUTF8ForComment)) {
        bitflag |= 2048;
      }
      var extFileAttr = 0;
      var versionMadeBy = 0;
      if (dir) {
        extFileAttr |= 16;
      }
      if (platform === "UNIX") {
        versionMadeBy = 798;
        extFileAttr |= generateUnixExternalFileAttr(file.unixPermissions, dir);
      } else {
        versionMadeBy = 20;
        extFileAttr |= generateDosExternalFileAttr(file.dosPermissions, dir);
      }
      dosTime = date.getUTCHours();
      dosTime = dosTime << 6;
      dosTime = dosTime | date.getUTCMinutes();
      dosTime = dosTime << 5;
      dosTime = dosTime | date.getUTCSeconds() / 2;
      dosDate = date.getUTCFullYear() - 1980;
      dosDate = dosDate << 4;
      dosDate = dosDate | date.getUTCMonth() + 1;
      dosDate = dosDate << 5;
      dosDate = dosDate | date.getUTCDate();
      if (useUTF8ForFileName) {
        unicodePathExtraField = // Version
        decToHex(1, 1) + // NameCRC32
        decToHex(crc32(encodedFileName), 4) + // UnicodeName
        utfEncodedFileName;
        extraFields += // Info-ZIP Unicode Path Extra Field
        "up" + // size
        decToHex(unicodePathExtraField.length, 2) + // content
        unicodePathExtraField;
      }
      if (useUTF8ForComment) {
        unicodeCommentExtraField = // Version
        decToHex(1, 1) + // CommentCRC32
        decToHex(crc32(encodedComment), 4) + // UnicodeName
        utfEncodedComment;
        extraFields += // Info-ZIP Unicode Path Extra Field
        "uc" + // size
        decToHex(unicodeCommentExtraField.length, 2) + // content
        unicodeCommentExtraField;
      }
      var header = "";
      header += "\n\0";
      header += decToHex(bitflag, 2);
      header += compression.magic;
      header += decToHex(dosTime, 2);
      header += decToHex(dosDate, 2);
      header += decToHex(dataInfo.crc32, 4);
      header += decToHex(dataInfo.compressedSize, 4);
      header += decToHex(dataInfo.uncompressedSize, 4);
      header += decToHex(encodedFileName.length, 2);
      header += decToHex(extraFields.length, 2);
      var fileRecord = signature.LOCAL_FILE_HEADER + header + encodedFileName + extraFields;
      var dirRecord = signature.CENTRAL_FILE_HEADER + // version made by (00: DOS)
      decToHex(versionMadeBy, 2) + // file header (common to file and central directory)
      header + // file comment length
      decToHex(encodedComment.length, 2) + // disk number start
      "\0\0\0\0" + // external file attributes
      decToHex(extFileAttr, 4) + // relative offset of local header
      decToHex(offset, 4) + // file name
      encodedFileName + // extra field
      extraFields + // file comment
      encodedComment;
      return {
        fileRecord,
        dirRecord
      };
    };
    var generateCentralDirectoryEnd = function(entriesCount, centralDirLength, localDirLength, comment, encodeFileName) {
      var dirEnd = "";
      var encodedComment = utils.transformTo("string", encodeFileName(comment));
      dirEnd = signature.CENTRAL_DIRECTORY_END + // number of this disk
      "\0\0\0\0" + // total number of entries in the central directory on this disk
      decToHex(entriesCount, 2) + // total number of entries in the central directory
      decToHex(entriesCount, 2) + // size of the central directory   4 bytes
      decToHex(centralDirLength, 4) + // offset of start of central directory with respect to the starting disk number
      decToHex(localDirLength, 4) + // .ZIP file comment length
      decToHex(encodedComment.length, 2) + // .ZIP file comment
      encodedComment;
      return dirEnd;
    };
    var generateDataDescriptors = function(streamInfo) {
      var descriptor = "";
      descriptor = signature.DATA_DESCRIPTOR + // crc-32                          4 bytes
      decToHex(streamInfo["crc32"], 4) + // compressed size                 4 bytes
      decToHex(streamInfo["compressedSize"], 4) + // uncompressed size               4 bytes
      decToHex(streamInfo["uncompressedSize"], 4);
      return descriptor;
    };
    function ZipFileWorker(streamFiles, comment, platform, encodeFileName) {
      GenericWorker.call(this, "ZipFileWorker");
      this.bytesWritten = 0;
      this.zipComment = comment;
      this.zipPlatform = platform;
      this.encodeFileName = encodeFileName;
      this.streamFiles = streamFiles;
      this.accumulate = false;
      this.contentBuffer = [];
      this.dirRecords = [];
      this.currentSourceOffset = 0;
      this.entriesCount = 0;
      this.currentFile = null;
      this._sources = [];
    }
    utils.inherits(ZipFileWorker, GenericWorker);
    ZipFileWorker.prototype.push = function(chunk) {
      var currentFilePercent = chunk.meta.percent || 0;
      var entriesCount = this.entriesCount;
      var remainingFiles = this._sources.length;
      if (this.accumulate) {
        this.contentBuffer.push(chunk);
      } else {
        this.bytesWritten += chunk.data.length;
        GenericWorker.prototype.push.call(this, {
          data: chunk.data,
          meta: {
            currentFile: this.currentFile,
            percent: entriesCount ? (currentFilePercent + 100 * (entriesCount - remainingFiles - 1)) / entriesCount : 100
          }
        });
      }
    };
    ZipFileWorker.prototype.openedSource = function(streamInfo) {
      this.currentSourceOffset = this.bytesWritten;
      this.currentFile = streamInfo["file"].name;
      var streamedContent = this.streamFiles && !streamInfo["file"].dir;
      if (streamedContent) {
        var record = generateZipParts(streamInfo, streamedContent, false, this.currentSourceOffset, this.zipPlatform, this.encodeFileName);
        this.push({
          data: record.fileRecord,
          meta: { percent: 0 }
        });
      } else {
        this.accumulate = true;
      }
    };
    ZipFileWorker.prototype.closedSource = function(streamInfo) {
      this.accumulate = false;
      var streamedContent = this.streamFiles && !streamInfo["file"].dir;
      var record = generateZipParts(streamInfo, streamedContent, true, this.currentSourceOffset, this.zipPlatform, this.encodeFileName);
      this.dirRecords.push(record.dirRecord);
      if (streamedContent) {
        this.push({
          data: generateDataDescriptors(streamInfo),
          meta: { percent: 100 }
        });
      } else {
        this.push({
          data: record.fileRecord,
          meta: { percent: 0 }
        });
        while (this.contentBuffer.length) {
          this.push(this.contentBuffer.shift());
        }
      }
      this.currentFile = null;
    };
    ZipFileWorker.prototype.flush = function() {
      var localDirLength = this.bytesWritten;
      for (var i = 0; i < this.dirRecords.length; i++) {
        this.push({
          data: this.dirRecords[i],
          meta: { percent: 100 }
        });
      }
      var centralDirLength = this.bytesWritten - localDirLength;
      var dirEnd = generateCentralDirectoryEnd(this.dirRecords.length, centralDirLength, localDirLength, this.zipComment, this.encodeFileName);
      this.push({
        data: dirEnd,
        meta: { percent: 100 }
      });
    };
    ZipFileWorker.prototype.prepareNextSource = function() {
      this.previous = this._sources.shift();
      this.openedSource(this.previous.streamInfo);
      if (this.isPaused) {
        this.previous.pause();
      } else {
        this.previous.resume();
      }
    };
    ZipFileWorker.prototype.registerPrevious = function(previous) {
      this._sources.push(previous);
      var self2 = this;
      previous.on("data", function(chunk) {
        self2.processChunk(chunk);
      });
      previous.on("end", function() {
        self2.closedSource(self2.previous.streamInfo);
        if (self2._sources.length) {
          self2.prepareNextSource();
        } else {
          self2.end();
        }
      });
      previous.on("error", function(e) {
        self2.error(e);
      });
      return this;
    };
    ZipFileWorker.prototype.resume = function() {
      if (!GenericWorker.prototype.resume.call(this)) {
        return false;
      }
      if (!this.previous && this._sources.length) {
        this.prepareNextSource();
        return true;
      }
      if (!this.previous && !this._sources.length && !this.generatedError) {
        this.end();
        return true;
      }
    };
    ZipFileWorker.prototype.error = function(e) {
      var sources = this._sources;
      if (!GenericWorker.prototype.error.call(this, e)) {
        return false;
      }
      for (var i = 0; i < sources.length; i++) {
        try {
          sources[i].error(e);
        } catch (e2) {
        }
      }
      return true;
    };
    ZipFileWorker.prototype.lock = function() {
      GenericWorker.prototype.lock.call(this);
      var sources = this._sources;
      for (var i = 0; i < sources.length; i++) {
        sources[i].lock();
      }
    };
    module.exports = ZipFileWorker;
  }
});
var require_generate = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/generate/index.js"(exports) {
    "use strict";
    var compressions = require_compressions();
    var ZipFileWorker = require_ZipFileWorker();
    var getCompression = function(fileCompression, zipCompression) {
      var compressionName = fileCompression || zipCompression;
      var compression = compressions[compressionName];
      if (!compression) {
        throw new Error(compressionName + " is not a valid compression method !");
      }
      return compression;
    };
    exports.generateWorker = function(zip, options, comment) {
      var zipFileWorker = new ZipFileWorker(options.streamFiles, comment, options.platform, options.encodeFileName);
      var entriesCount = 0;
      try {
        zip.forEach(function(relativePath, file) {
          entriesCount++;
          var compression = getCompression(file.options.compression, options.compression);
          var compressionOptions = file.options.compressionOptions || options.compressionOptions || {};
          var dir = file.dir, date = file.date;
          file._compressWorker(compression, compressionOptions).withStreamInfo("file", {
            name: relativePath,
            dir,
            date,
            comment: file.comment || "",
            unixPermissions: file.unixPermissions,
            dosPermissions: file.dosPermissions
          }).pipe(zipFileWorker);
        });
        zipFileWorker.entriesCount = entriesCount;
      } catch (e) {
        zipFileWorker.error(e);
      }
      return zipFileWorker;
    };
  }
});
var require_NodejsStreamInputAdapter = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/nodejs/NodejsStreamInputAdapter.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    function NodejsStreamInputAdapter(filename, stream) {
      GenericWorker.call(this, "Nodejs stream input adapter for " + filename);
      this._upstreamEnded = false;
      this._bindStream(stream);
    }
    utils.inherits(NodejsStreamInputAdapter, GenericWorker);
    NodejsStreamInputAdapter.prototype._bindStream = function(stream) {
      var self2 = this;
      this._stream = stream;
      stream.pause();
      stream.on("data", function(chunk) {
        self2.push({
          data: chunk,
          meta: {
            percent: 0
          }
        });
      }).on("error", function(e) {
        if (self2.isPaused) {
          this.generatedError = e;
        } else {
          self2.error(e);
        }
      }).on("end", function() {
        if (self2.isPaused) {
          self2._upstreamEnded = true;
        } else {
          self2.end();
        }
      });
    };
    NodejsStreamInputAdapter.prototype.pause = function() {
      if (!GenericWorker.prototype.pause.call(this)) {
        return false;
      }
      this._stream.pause();
      return true;
    };
    NodejsStreamInputAdapter.prototype.resume = function() {
      if (!GenericWorker.prototype.resume.call(this)) {
        return false;
      }
      if (this._upstreamEnded) {
        this.end();
      } else {
        this._stream.resume();
      }
      return true;
    };
    module.exports = NodejsStreamInputAdapter;
  }
});
var require_object = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/object.js"(exports, module) {
    "use strict";
    var utf8 = require_utf8();
    var utils = require_utils();
    var GenericWorker = require_GenericWorker();
    var StreamHelper = require_StreamHelper();
    var defaults = require_defaults();
    var CompressedObject = require_compressedObject();
    var ZipObject = require_zipObject();
    var generate = require_generate();
    var nodejsUtils = require_nodejsUtils();
    var NodejsStreamInputAdapter = require_NodejsStreamInputAdapter();
    var fileAdd = function(name, data, originalOptions) {
      var dataType = utils.getTypeOf(data), parent;
      var o = utils.extend(originalOptions || {}, defaults);
      o.date = o.date || /* @__PURE__ */ new Date();
      if (o.compression !== null) {
        o.compression = o.compression.toUpperCase();
      }
      if (typeof o.unixPermissions === "string") {
        o.unixPermissions = parseInt(o.unixPermissions, 8);
      }
      if (o.unixPermissions && o.unixPermissions & 16384) {
        o.dir = true;
      }
      if (o.dosPermissions && o.dosPermissions & 16) {
        o.dir = true;
      }
      if (o.dir) {
        name = forceTrailingSlash(name);
      }
      if (o.createFolders && (parent = parentFolder(name))) {
        folderAdd.call(this, parent, true);
      }
      var isUnicodeString = dataType === "string" && o.binary === false && o.base64 === false;
      if (!originalOptions || typeof originalOptions.binary === "undefined") {
        o.binary = !isUnicodeString;
      }
      var isCompressedEmpty = data instanceof CompressedObject && data.uncompressedSize === 0;
      if (isCompressedEmpty || o.dir || !data || data.length === 0) {
        o.base64 = false;
        o.binary = true;
        data = "";
        o.compression = "STORE";
        dataType = "string";
      }
      var zipObjectContent = null;
      if (data instanceof CompressedObject || data instanceof GenericWorker) {
        zipObjectContent = data;
      } else if (nodejsUtils.isNode && nodejsUtils.isStream(data)) {
        zipObjectContent = new NodejsStreamInputAdapter(name, data);
      } else {
        zipObjectContent = utils.prepareContent(name, data, o.binary, o.optimizedBinaryString, o.base64);
      }
      var object = new ZipObject(name, zipObjectContent, o);
      this.files[name] = object;
    };
    var parentFolder = function(path) {
      if (path.slice(-1) === "/") {
        path = path.substring(0, path.length - 1);
      }
      var lastSlash = path.lastIndexOf("/");
      return lastSlash > 0 ? path.substring(0, lastSlash) : "";
    };
    var forceTrailingSlash = function(path) {
      if (path.slice(-1) !== "/") {
        path += "/";
      }
      return path;
    };
    var folderAdd = function(name, createFolders) {
      createFolders = typeof createFolders !== "undefined" ? createFolders : defaults.createFolders;
      name = forceTrailingSlash(name);
      if (!this.files[name]) {
        fileAdd.call(this, name, null, {
          dir: true,
          createFolders
        });
      }
      return this.files[name];
    };
    function isRegExp(object) {
      return Object.prototype.toString.call(object) === "[object RegExp]";
    }
    var out = {
      /**
       * @see loadAsync
       */
      load: function() {
        throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
      },
      /**
       * Call a callback function for each entry at this folder level.
       * @param {Function} cb the callback function:
       * function (relativePath, file) {...}
       * It takes 2 arguments : the relative path and the file.
       */
      forEach: function(cb) {
        var filename, relativePath, file;
        for (filename in this.files) {
          file = this.files[filename];
          relativePath = filename.slice(this.root.length, filename.length);
          if (relativePath && filename.slice(0, this.root.length) === this.root) {
            cb(relativePath, file);
          }
        }
      },
      /**
       * Filter nested files/folders with the specified function.
       * @param {Function} search the predicate to use :
       * function (relativePath, file) {...}
       * It takes 2 arguments : the relative path and the file.
       * @return {Array} An array of matching elements.
       */
      filter: function(search) {
        var result = [];
        this.forEach(function(relativePath, entry) {
          if (search(relativePath, entry)) {
            result.push(entry);
          }
        });
        return result;
      },
      /**
       * Add a file to the zip file, or search a file.
       * @param   {string|RegExp} name The name of the file to add (if data is defined),
       * the name of the file to find (if no data) or a regex to match files.
       * @param   {String|ArrayBuffer|Uint8Array|Buffer} data  The file data, either raw or base64 encoded
       * @param   {Object} o     File options
       * @return  {JSZip|Object|Array} this JSZip object (when adding a file),
       * a file (when searching by string) or an array of files (when searching by regex).
       */
      file: function(name, data, o) {
        if (arguments.length === 1) {
          if (isRegExp(name)) {
            var regexp = name;
            return this.filter(function(relativePath, file) {
              return !file.dir && regexp.test(relativePath);
            });
          } else {
            var obj = this.files[this.root + name];
            if (obj && !obj.dir) {
              return obj;
            } else {
              return null;
            }
          }
        } else {
          name = this.root + name;
          fileAdd.call(this, name, data, o);
        }
        return this;
      },
      /**
       * Add a directory to the zip file, or search.
       * @param   {String|RegExp} arg The name of the directory to add, or a regex to search folders.
       * @return  {JSZip} an object with the new directory as the root, or an array containing matching folders.
       */
      folder: function(arg) {
        if (!arg) {
          return this;
        }
        if (isRegExp(arg)) {
          return this.filter(function(relativePath, file) {
            return file.dir && arg.test(relativePath);
          });
        }
        var name = this.root + arg;
        var newFolder = folderAdd.call(this, name);
        var ret = this.clone();
        ret.root = newFolder.name;
        return ret;
      },
      /**
       * Delete a file, or a directory and all sub-files, from the zip
       * @param {string} name the name of the file to delete
       * @return {JSZip} this JSZip object
       */
      remove: function(name) {
        name = this.root + name;
        var file = this.files[name];
        if (!file) {
          if (name.slice(-1) !== "/") {
            name += "/";
          }
          file = this.files[name];
        }
        if (file && !file.dir) {
          delete this.files[name];
        } else {
          var kids = this.filter(function(relativePath, file2) {
            return file2.name.slice(0, name.length) === name;
          });
          for (var i = 0; i < kids.length; i++) {
            delete this.files[kids[i].name];
          }
        }
        return this;
      },
      /**
       * @deprecated This method has been removed in JSZip 3.0, please check the upgrade guide.
       */
      generate: function() {
        throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
      },
      /**
       * Generate the complete zip file as an internal stream.
       * @param {Object} options the options to generate the zip file :
       * - compression, "STORE" by default.
       * - type, "base64" by default. Values are : string, base64, uint8array, arraybuffer, blob.
       * @return {StreamHelper} the streamed zip file.
       */
      generateInternalStream: function(options) {
        var worker, opts = {};
        try {
          opts = utils.extend(options || {}, {
            streamFiles: false,
            compression: "STORE",
            compressionOptions: null,
            type: "",
            platform: "DOS",
            comment: null,
            mimeType: "application/zip",
            encodeFileName: utf8.utf8encode
          });
          opts.type = opts.type.toLowerCase();
          opts.compression = opts.compression.toUpperCase();
          if (opts.type === "binarystring") {
            opts.type = "string";
          }
          if (!opts.type) {
            throw new Error("No output type specified.");
          }
          utils.checkSupport(opts.type);
          if (opts.platform === "darwin" || opts.platform === "freebsd" || opts.platform === "linux" || opts.platform === "sunos") {
            opts.platform = "UNIX";
          }
          if (opts.platform === "win32") {
            opts.platform = "DOS";
          }
          var comment = opts.comment || this.comment || "";
          worker = generate.generateWorker(this, opts, comment);
        } catch (e) {
          worker = new GenericWorker("error");
          worker.error(e);
        }
        return new StreamHelper(worker, opts.type || "string", opts.mimeType);
      },
      /**
       * Generate the complete zip file asynchronously.
       * @see generateInternalStream
       */
      generateAsync: function(options, onUpdate) {
        return this.generateInternalStream(options).accumulate(onUpdate);
      },
      /**
       * Generate the complete zip file asynchronously.
       * @see generateInternalStream
       */
      generateNodeStream: function(options, onUpdate) {
        options = options || {};
        if (!options.type) {
          options.type = "nodebuffer";
        }
        return this.generateInternalStream(options).toNodejsStream(onUpdate);
      }
    };
    module.exports = out;
  }
});
var require_DataReader = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/DataReader.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    function DataReader(data) {
      this.data = data;
      this.length = data.length;
      this.index = 0;
      this.zero = 0;
    }
    DataReader.prototype = {
      /**
       * Check that the offset will not go too far.
       * @param {string} offset the additional offset to check.
       * @throws {Error} an Error if the offset is out of bounds.
       */
      checkOffset: function(offset) {
        this.checkIndex(this.index + offset);
      },
      /**
       * Check that the specified index will not be too far.
       * @param {string} newIndex the index to check.
       * @throws {Error} an Error if the index is out of bounds.
       */
      checkIndex: function(newIndex) {
        if (this.length < this.zero + newIndex || newIndex < 0) {
          throw new Error("End of data reached (data length = " + this.length + ", asked index = " + newIndex + "). Corrupted zip ?");
        }
      },
      /**
       * Change the index.
       * @param {number} newIndex The new index.
       * @throws {Error} if the new index is out of the data.
       */
      setIndex: function(newIndex) {
        this.checkIndex(newIndex);
        this.index = newIndex;
      },
      /**
       * Skip the next n bytes.
       * @param {number} n the number of bytes to skip.
       * @throws {Error} if the new index is out of the data.
       */
      skip: function(n) {
        this.setIndex(this.index + n);
      },
      /**
       * Get the byte at the specified index.
       * @param {number} i the index to use.
       * @return {number} a byte.
       */
      byteAt: function() {
      },
      /**
       * Get the next number with a given byte size.
       * @param {number} size the number of bytes to read.
       * @return {number} the corresponding number.
       */
      readInt: function(size) {
        var result = 0, i;
        this.checkOffset(size);
        for (i = this.index + size - 1; i >= this.index; i--) {
          result = (result << 8) + this.byteAt(i);
        }
        this.index += size;
        return result;
      },
      /**
       * Get the next string with a given byte size.
       * @param {number} size the number of bytes to read.
       * @return {string} the corresponding string.
       */
      readString: function(size) {
        return utils.transformTo("string", this.readData(size));
      },
      /**
       * Get raw data without conversion, <size> bytes.
       * @param {number} size the number of bytes to read.
       * @return {Object} the raw data, implementation specific.
       */
      readData: function() {
      },
      /**
       * Find the last occurrence of a zip signature (4 bytes).
       * @param {string} sig the signature to find.
       * @return {number} the index of the last occurrence, -1 if not found.
       */
      lastIndexOfSignature: function() {
      },
      /**
       * Read the signature (4 bytes) at the current position and compare it with sig.
       * @param {string} sig the expected signature
       * @return {boolean} true if the signature matches, false otherwise.
       */
      readAndCheckSignature: function() {
      },
      /**
       * Get the next date.
       * @return {Date} the date.
       */
      readDate: function() {
        var dostime = this.readInt(4);
        return new Date(Date.UTC(
          (dostime >> 25 & 127) + 1980,
          // year
          (dostime >> 21 & 15) - 1,
          // month
          dostime >> 16 & 31,
          // day
          dostime >> 11 & 31,
          // hour
          dostime >> 5 & 63,
          // minute
          (dostime & 31) << 1
        ));
      }
    };
    module.exports = DataReader;
  }
});
var require_ArrayReader = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/ArrayReader.js"(exports, module) {
    "use strict";
    var DataReader = require_DataReader();
    var utils = require_utils();
    function ArrayReader(data) {
      DataReader.call(this, data);
      for (var i = 0; i < this.data.length; i++) {
        data[i] = data[i] & 255;
      }
    }
    utils.inherits(ArrayReader, DataReader);
    ArrayReader.prototype.byteAt = function(i) {
      return this.data[this.zero + i];
    };
    ArrayReader.prototype.lastIndexOfSignature = function(sig) {
      var sig0 = sig.charCodeAt(0), sig1 = sig.charCodeAt(1), sig2 = sig.charCodeAt(2), sig3 = sig.charCodeAt(3);
      for (var i = this.length - 4; i >= 0; --i) {
        if (this.data[i] === sig0 && this.data[i + 1] === sig1 && this.data[i + 2] === sig2 && this.data[i + 3] === sig3) {
          return i - this.zero;
        }
      }
      return -1;
    };
    ArrayReader.prototype.readAndCheckSignature = function(sig) {
      var sig0 = sig.charCodeAt(0), sig1 = sig.charCodeAt(1), sig2 = sig.charCodeAt(2), sig3 = sig.charCodeAt(3), data = this.readData(4);
      return sig0 === data[0] && sig1 === data[1] && sig2 === data[2] && sig3 === data[3];
    };
    ArrayReader.prototype.readData = function(size) {
      this.checkOffset(size);
      if (size === 0) {
        return [];
      }
      var result = this.data.slice(this.zero + this.index, this.zero + this.index + size);
      this.index += size;
      return result;
    };
    module.exports = ArrayReader;
  }
});
var require_StringReader = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/StringReader.js"(exports, module) {
    "use strict";
    var DataReader = require_DataReader();
    var utils = require_utils();
    function StringReader(data) {
      DataReader.call(this, data);
    }
    utils.inherits(StringReader, DataReader);
    StringReader.prototype.byteAt = function(i) {
      return this.data.charCodeAt(this.zero + i);
    };
    StringReader.prototype.lastIndexOfSignature = function(sig) {
      return this.data.lastIndexOf(sig) - this.zero;
    };
    StringReader.prototype.readAndCheckSignature = function(sig) {
      var data = this.readData(4);
      return sig === data;
    };
    StringReader.prototype.readData = function(size) {
      this.checkOffset(size);
      var result = this.data.slice(this.zero + this.index, this.zero + this.index + size);
      this.index += size;
      return result;
    };
    module.exports = StringReader;
  }
});
var require_Uint8ArrayReader = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/Uint8ArrayReader.js"(exports, module) {
    "use strict";
    var ArrayReader = require_ArrayReader();
    var utils = require_utils();
    function Uint8ArrayReader(data) {
      ArrayReader.call(this, data);
    }
    utils.inherits(Uint8ArrayReader, ArrayReader);
    Uint8ArrayReader.prototype.readData = function(size) {
      this.checkOffset(size);
      if (size === 0) {
        return new Uint8Array(0);
      }
      var result = this.data.subarray(this.zero + this.index, this.zero + this.index + size);
      this.index += size;
      return result;
    };
    module.exports = Uint8ArrayReader;
  }
});
var require_NodeBufferReader = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/NodeBufferReader.js"(exports, module) {
    "use strict";
    var Uint8ArrayReader = require_Uint8ArrayReader();
    var utils = require_utils();
    function NodeBufferReader(data) {
      Uint8ArrayReader.call(this, data);
    }
    utils.inherits(NodeBufferReader, Uint8ArrayReader);
    NodeBufferReader.prototype.readData = function(size) {
      this.checkOffset(size);
      var result = this.data.slice(this.zero + this.index, this.zero + this.index + size);
      this.index += size;
      return result;
    };
    module.exports = NodeBufferReader;
  }
});
var require_readerFor = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/reader/readerFor.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var support = require_support();
    var ArrayReader = require_ArrayReader();
    var StringReader = require_StringReader();
    var NodeBufferReader = require_NodeBufferReader();
    var Uint8ArrayReader = require_Uint8ArrayReader();
    module.exports = function(data) {
      var type = utils.getTypeOf(data);
      utils.checkSupport(type);
      if (type === "string" && !support.uint8array) {
        return new StringReader(data);
      }
      if (type === "nodebuffer") {
        return new NodeBufferReader(data);
      }
      if (support.uint8array) {
        return new Uint8ArrayReader(utils.transformTo("uint8array", data));
      }
      return new ArrayReader(utils.transformTo("array", data));
    };
  }
});
var require_zipEntry = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/zipEntry.js"(exports, module) {
    "use strict";
    var readerFor = require_readerFor();
    var utils = require_utils();
    var CompressedObject = require_compressedObject();
    var crc32fn = require_crc32();
    var utf8 = require_utf8();
    var compressions = require_compressions();
    var support = require_support();
    var MADE_BY_DOS = 0;
    var MADE_BY_UNIX = 3;
    var findCompression = function(compressionMethod) {
      for (var method in compressions) {
        if (!Object.prototype.hasOwnProperty.call(compressions, method)) {
          continue;
        }
        if (compressions[method].magic === compressionMethod) {
          return compressions[method];
        }
      }
      return null;
    };
    function ZipEntry(options, loadOptions) {
      this.options = options;
      this.loadOptions = loadOptions;
    }
    ZipEntry.prototype = {
      /**
       * say if the file is encrypted.
       * @return {boolean} true if the file is encrypted, false otherwise.
       */
      isEncrypted: function() {
        return (this.bitFlag & 1) === 1;
      },
      /**
       * say if the file has utf-8 filename/comment.
       * @return {boolean} true if the filename/comment is in utf-8, false otherwise.
       */
      useUTF8: function() {
        return (this.bitFlag & 2048) === 2048;
      },
      /**
       * Read the local part of a zip file and add the info in this object.
       * @param {DataReader} reader the reader to use.
       */
      readLocalPart: function(reader) {
        var compression, localExtraFieldsLength;
        reader.skip(22);
        this.fileNameLength = reader.readInt(2);
        localExtraFieldsLength = reader.readInt(2);
        this.fileName = reader.readData(this.fileNameLength);
        reader.skip(localExtraFieldsLength);
        if (this.compressedSize === -1 || this.uncompressedSize === -1) {
          throw new Error("Bug or corrupted zip : didn't get enough information from the central directory (compressedSize === -1 || uncompressedSize === -1)");
        }
        compression = findCompression(this.compressionMethod);
        if (compression === null) {
          throw new Error("Corrupted zip : compression " + utils.pretty(this.compressionMethod) + " unknown (inner file : " + utils.transformTo("string", this.fileName) + ")");
        }
        this.decompressed = new CompressedObject(this.compressedSize, this.uncompressedSize, this.crc32, compression, reader.readData(this.compressedSize));
      },
      /**
       * Read the central part of a zip file and add the info in this object.
       * @param {DataReader} reader the reader to use.
       */
      readCentralPart: function(reader) {
        this.versionMadeBy = reader.readInt(2);
        reader.skip(2);
        this.bitFlag = reader.readInt(2);
        this.compressionMethod = reader.readString(2);
        this.date = reader.readDate();
        this.crc32 = reader.readInt(4);
        this.compressedSize = reader.readInt(4);
        this.uncompressedSize = reader.readInt(4);
        var fileNameLength = reader.readInt(2);
        this.extraFieldsLength = reader.readInt(2);
        this.fileCommentLength = reader.readInt(2);
        this.diskNumberStart = reader.readInt(2);
        this.internalFileAttributes = reader.readInt(2);
        this.externalFileAttributes = reader.readInt(4);
        this.localHeaderOffset = reader.readInt(4);
        if (this.isEncrypted()) {
          throw new Error("Encrypted zip are not supported");
        }
        reader.skip(fileNameLength);
        this.readExtraFields(reader);
        this.parseZIP64ExtraField(reader);
        this.fileComment = reader.readData(this.fileCommentLength);
      },
      /**
       * Parse the external file attributes and get the unix/dos permissions.
       */
      processAttributes: function() {
        this.unixPermissions = null;
        this.dosPermissions = null;
        var madeBy = this.versionMadeBy >> 8;
        this.dir = this.externalFileAttributes & 16 ? true : false;
        if (madeBy === MADE_BY_DOS) {
          this.dosPermissions = this.externalFileAttributes & 63;
        }
        if (madeBy === MADE_BY_UNIX) {
          this.unixPermissions = this.externalFileAttributes >> 16 & 65535;
        }
        if (!this.dir && this.fileNameStr.slice(-1) === "/") {
          this.dir = true;
        }
      },
      /**
       * Parse the ZIP64 extra field and merge the info in the current ZipEntry.
       * @param {DataReader} reader the reader to use.
       */
      parseZIP64ExtraField: function() {
        if (!this.extraFields[1]) {
          return;
        }
        var extraReader = readerFor(this.extraFields[1].value);
        if (this.uncompressedSize === utils.MAX_VALUE_32BITS) {
          this.uncompressedSize = extraReader.readInt(8);
        }
        if (this.compressedSize === utils.MAX_VALUE_32BITS) {
          this.compressedSize = extraReader.readInt(8);
        }
        if (this.localHeaderOffset === utils.MAX_VALUE_32BITS) {
          this.localHeaderOffset = extraReader.readInt(8);
        }
        if (this.diskNumberStart === utils.MAX_VALUE_32BITS) {
          this.diskNumberStart = extraReader.readInt(4);
        }
      },
      /**
       * Read the central part of a zip file and add the info in this object.
       * @param {DataReader} reader the reader to use.
       */
      readExtraFields: function(reader) {
        var end = reader.index + this.extraFieldsLength, extraFieldId, extraFieldLength, extraFieldValue;
        if (!this.extraFields) {
          this.extraFields = {};
        }
        while (reader.index + 4 < end) {
          extraFieldId = reader.readInt(2);
          extraFieldLength = reader.readInt(2);
          extraFieldValue = reader.readData(extraFieldLength);
          this.extraFields[extraFieldId] = {
            id: extraFieldId,
            length: extraFieldLength,
            value: extraFieldValue
          };
        }
        reader.setIndex(end);
      },
      /**
       * Apply an UTF8 transformation if needed.
       */
      handleUTF8: function() {
        var decodeParamType = support.uint8array ? "uint8array" : "array";
        if (this.useUTF8()) {
          this.fileNameStr = utf8.utf8decode(this.fileName);
          this.fileCommentStr = utf8.utf8decode(this.fileComment);
        } else {
          var upath = this.findExtraFieldUnicodePath();
          if (upath !== null) {
            this.fileNameStr = upath;
          } else {
            var fileNameByteArray = utils.transformTo(decodeParamType, this.fileName);
            this.fileNameStr = this.loadOptions.decodeFileName(fileNameByteArray);
          }
          var ucomment = this.findExtraFieldUnicodeComment();
          if (ucomment !== null) {
            this.fileCommentStr = ucomment;
          } else {
            var commentByteArray = utils.transformTo(decodeParamType, this.fileComment);
            this.fileCommentStr = this.loadOptions.decodeFileName(commentByteArray);
          }
        }
      },
      /**
       * Find the unicode path declared in the extra field, if any.
       * @return {String} the unicode path, null otherwise.
       */
      findExtraFieldUnicodePath: function() {
        var upathField = this.extraFields[28789];
        if (upathField) {
          var extraReader = readerFor(upathField.value);
          if (extraReader.readInt(1) !== 1) {
            return null;
          }
          if (crc32fn(this.fileName) !== extraReader.readInt(4)) {
            return null;
          }
          return utf8.utf8decode(extraReader.readData(upathField.length - 5));
        }
        return null;
      },
      /**
       * Find the unicode comment declared in the extra field, if any.
       * @return {String} the unicode comment, null otherwise.
       */
      findExtraFieldUnicodeComment: function() {
        var ucommentField = this.extraFields[25461];
        if (ucommentField) {
          var extraReader = readerFor(ucommentField.value);
          if (extraReader.readInt(1) !== 1) {
            return null;
          }
          if (crc32fn(this.fileComment) !== extraReader.readInt(4)) {
            return null;
          }
          return utf8.utf8decode(extraReader.readData(ucommentField.length - 5));
        }
        return null;
      }
    };
    module.exports = ZipEntry;
  }
});
var require_zipEntries = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/zipEntries.js"(exports, module) {
    "use strict";
    var readerFor = require_readerFor();
    var utils = require_utils();
    var sig = require_signature();
    var ZipEntry = require_zipEntry();
    var support = require_support();
    function ZipEntries(loadOptions) {
      this.files = [];
      this.loadOptions = loadOptions;
    }
    ZipEntries.prototype = {
      /**
       * Check that the reader is on the specified signature.
       * @param {string} expectedSignature the expected signature.
       * @throws {Error} if it is an other signature.
       */
      checkSignature: function(expectedSignature) {
        if (!this.reader.readAndCheckSignature(expectedSignature)) {
          this.reader.index -= 4;
          var signature = this.reader.readString(4);
          throw new Error("Corrupted zip or bug: unexpected signature (" + utils.pretty(signature) + ", expected " + utils.pretty(expectedSignature) + ")");
        }
      },
      /**
       * Check if the given signature is at the given index.
       * @param {number} askedIndex the index to check.
       * @param {string} expectedSignature the signature to expect.
       * @return {boolean} true if the signature is here, false otherwise.
       */
      isSignature: function(askedIndex, expectedSignature) {
        var currentIndex = this.reader.index;
        this.reader.setIndex(askedIndex);
        var signature = this.reader.readString(4);
        var result = signature === expectedSignature;
        this.reader.setIndex(currentIndex);
        return result;
      },
      /**
       * Read the end of the central directory.
       */
      readBlockEndOfCentral: function() {
        this.diskNumber = this.reader.readInt(2);
        this.diskWithCentralDirStart = this.reader.readInt(2);
        this.centralDirRecordsOnThisDisk = this.reader.readInt(2);
        this.centralDirRecords = this.reader.readInt(2);
        this.centralDirSize = this.reader.readInt(4);
        this.centralDirOffset = this.reader.readInt(4);
        this.zipCommentLength = this.reader.readInt(2);
        var zipComment = this.reader.readData(this.zipCommentLength);
        var decodeParamType = support.uint8array ? "uint8array" : "array";
        var decodeContent = utils.transformTo(decodeParamType, zipComment);
        this.zipComment = this.loadOptions.decodeFileName(decodeContent);
      },
      /**
       * Read the end of the Zip 64 central directory.
       * Not merged with the method readEndOfCentral :
       * The end of central can coexist with its Zip64 brother,
       * I don't want to read the wrong number of bytes !
       */
      readBlockZip64EndOfCentral: function() {
        this.zip64EndOfCentralSize = this.reader.readInt(8);
        this.reader.skip(4);
        this.diskNumber = this.reader.readInt(4);
        this.diskWithCentralDirStart = this.reader.readInt(4);
        this.centralDirRecordsOnThisDisk = this.reader.readInt(8);
        this.centralDirRecords = this.reader.readInt(8);
        this.centralDirSize = this.reader.readInt(8);
        this.centralDirOffset = this.reader.readInt(8);
        this.zip64ExtensibleData = {};
        var extraDataSize = this.zip64EndOfCentralSize - 44, index = 0, extraFieldId, extraFieldLength, extraFieldValue;
        while (index < extraDataSize) {
          extraFieldId = this.reader.readInt(2);
          extraFieldLength = this.reader.readInt(4);
          extraFieldValue = this.reader.readData(extraFieldLength);
          this.zip64ExtensibleData[extraFieldId] = {
            id: extraFieldId,
            length: extraFieldLength,
            value: extraFieldValue
          };
        }
      },
      /**
       * Read the end of the Zip 64 central directory locator.
       */
      readBlockZip64EndOfCentralLocator: function() {
        this.diskWithZip64CentralDirStart = this.reader.readInt(4);
        this.relativeOffsetEndOfZip64CentralDir = this.reader.readInt(8);
        this.disksCount = this.reader.readInt(4);
        if (this.disksCount > 1) {
          throw new Error("Multi-volumes zip are not supported");
        }
      },
      /**
       * Read the local files, based on the offset read in the central part.
       */
      readLocalFiles: function() {
        var i, file;
        for (i = 0; i < this.files.length; i++) {
          file = this.files[i];
          this.reader.setIndex(file.localHeaderOffset);
          this.checkSignature(sig.LOCAL_FILE_HEADER);
          file.readLocalPart(this.reader);
          file.handleUTF8();
          file.processAttributes();
        }
      },
      /**
       * Read the central directory.
       */
      readCentralDir: function() {
        var file;
        this.reader.setIndex(this.centralDirOffset);
        while (this.reader.readAndCheckSignature(sig.CENTRAL_FILE_HEADER)) {
          file = new ZipEntry({
            zip64: this.zip64
          }, this.loadOptions);
          file.readCentralPart(this.reader);
          this.files.push(file);
        }
        if (this.centralDirRecords !== this.files.length) {
          if (this.centralDirRecords !== 0 && this.files.length === 0) {
            throw new Error("Corrupted zip or bug: expected " + this.centralDirRecords + " records in central dir, got " + this.files.length);
          } else {
          }
        }
      },
      /**
       * Read the end of central directory.
       */
      readEndOfCentral: function() {
        var offset = this.reader.lastIndexOfSignature(sig.CENTRAL_DIRECTORY_END);
        if (offset < 0) {
          var isGarbage = !this.isSignature(0, sig.LOCAL_FILE_HEADER);
          if (isGarbage) {
            throw new Error("Can't find end of central directory : is this a zip file ? If it is, see https://stuk.github.io/jszip/documentation/howto/read_zip.html");
          } else {
            throw new Error("Corrupted zip: can't find end of central directory");
          }
        }
        this.reader.setIndex(offset);
        var endOfCentralDirOffset = offset;
        this.checkSignature(sig.CENTRAL_DIRECTORY_END);
        this.readBlockEndOfCentral();
        if (this.diskNumber === utils.MAX_VALUE_16BITS || this.diskWithCentralDirStart === utils.MAX_VALUE_16BITS || this.centralDirRecordsOnThisDisk === utils.MAX_VALUE_16BITS || this.centralDirRecords === utils.MAX_VALUE_16BITS || this.centralDirSize === utils.MAX_VALUE_32BITS || this.centralDirOffset === utils.MAX_VALUE_32BITS) {
          this.zip64 = true;
          offset = this.reader.lastIndexOfSignature(sig.ZIP64_CENTRAL_DIRECTORY_LOCATOR);
          if (offset < 0) {
            throw new Error("Corrupted zip: can't find the ZIP64 end of central directory locator");
          }
          this.reader.setIndex(offset);
          this.checkSignature(sig.ZIP64_CENTRAL_DIRECTORY_LOCATOR);
          this.readBlockZip64EndOfCentralLocator();
          if (!this.isSignature(this.relativeOffsetEndOfZip64CentralDir, sig.ZIP64_CENTRAL_DIRECTORY_END)) {
            this.relativeOffsetEndOfZip64CentralDir = this.reader.lastIndexOfSignature(sig.ZIP64_CENTRAL_DIRECTORY_END);
            if (this.relativeOffsetEndOfZip64CentralDir < 0) {
              throw new Error("Corrupted zip: can't find the ZIP64 end of central directory");
            }
          }
          this.reader.setIndex(this.relativeOffsetEndOfZip64CentralDir);
          this.checkSignature(sig.ZIP64_CENTRAL_DIRECTORY_END);
          this.readBlockZip64EndOfCentral();
        }
        var expectedEndOfCentralDirOffset = this.centralDirOffset + this.centralDirSize;
        if (this.zip64) {
          expectedEndOfCentralDirOffset += 20;
          expectedEndOfCentralDirOffset += 12 + this.zip64EndOfCentralSize;
        }
        var extraBytes = endOfCentralDirOffset - expectedEndOfCentralDirOffset;
        if (extraBytes > 0) {
          if (this.isSignature(endOfCentralDirOffset, sig.CENTRAL_FILE_HEADER)) {
          } else {
            this.reader.zero = extraBytes;
          }
        } else if (extraBytes < 0) {
          throw new Error("Corrupted zip: missing " + Math.abs(extraBytes) + " bytes.");
        }
      },
      prepareReader: function(data) {
        this.reader = readerFor(data);
      },
      /**
       * Read a zip file and create ZipEntries.
       * @param {String|ArrayBuffer|Uint8Array|Buffer} data the binary string representing a zip file.
       */
      load: function(data) {
        this.prepareReader(data);
        this.readEndOfCentral();
        this.readCentralDir();
        this.readLocalFiles();
      }
    };
    module.exports = ZipEntries;
  }
});
var require_load = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/load.js"(exports, module) {
    "use strict";
    var utils = require_utils();
    var external = require_external();
    var utf8 = require_utf8();
    var ZipEntries = require_zipEntries();
    var Crc32Probe = require_Crc32Probe();
    var nodejsUtils = require_nodejsUtils();
    function checkEntryCRC32(zipEntry) {
      return new external.Promise(function(resolve, reject) {
        var worker = zipEntry.decompressed.getContentWorker().pipe(new Crc32Probe());
        worker.on("error", function(e) {
          reject(e);
        }).on("end", function() {
          if (worker.streamInfo.crc32 !== zipEntry.decompressed.crc32) {
            reject(new Error("Corrupted zip : CRC32 mismatch"));
          } else {
            resolve();
          }
        }).resume();
      });
    }
    module.exports = function(data, options) {
      var zip = this;
      options = utils.extend(options || {}, {
        base64: false,
        checkCRC32: false,
        optimizedBinaryString: false,
        createFolders: false,
        decodeFileName: utf8.utf8decode
      });
      if (nodejsUtils.isNode && nodejsUtils.isStream(data)) {
        return external.Promise.reject(new Error("JSZip can't accept a stream when loading a zip file."));
      }
      return utils.prepareContent("the loaded zip file", data, true, options.optimizedBinaryString, options.base64).then(function(data2) {
        var zipEntries = new ZipEntries(options);
        zipEntries.load(data2);
        return zipEntries;
      }).then(function checkCRC32(zipEntries) {
        var promises = [external.Promise.resolve(zipEntries)];
        var files = zipEntries.files;
        if (options.checkCRC32) {
          for (var i = 0; i < files.length; i++) {
            promises.push(checkEntryCRC32(files[i]));
          }
        }
        return external.Promise.all(promises);
      }).then(function addFiles(results) {
        var zipEntries = results.shift();
        var files = zipEntries.files;
        for (var i = 0; i < files.length; i++) {
          var input = files[i];
          var unsafeName = input.fileNameStr;
          var safeName = utils.resolve(input.fileNameStr);
          zip.file(safeName, input.decompressed, {
            binary: true,
            optimizedBinaryString: true,
            date: input.date,
            dir: input.dir,
            comment: input.fileCommentStr.length ? input.fileCommentStr : null,
            unixPermissions: input.unixPermissions,
            dosPermissions: input.dosPermissions,
            createFolders: options.createFolders
          });
          if (!input.dir) {
            zip.file(safeName).unsafeOriginalName = unsafeName;
          }
        }
        if (zipEntries.zipComment.length) {
          zip.comment = zipEntries.zipComment;
        }
        return zip;
      });
    };
  }
});
var require_lib3 = __commonJS({
  "../../node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/lib/index.js"(exports, module) {
    function JSZip() {
      if (!(this instanceof JSZip)) {
        return new JSZip();
      }
      if (arguments.length) {
        throw new Error("The constructor with parameters has been removed in JSZip 3.0, please check the upgrade guide.");
      }
      this.files = /* @__PURE__ */ Object.create(null);
      this.comment = null;
      this.root = "";
      this.clone = function() {
        var newObj = new JSZip();
        for (var i in this) {
          if (typeof this[i] !== "function") {
            newObj[i] = this[i];
          }
        }
        return newObj;
      };
    }
    JSZip.prototype = require_object();
    JSZip.prototype.loadAsync = require_load();
    JSZip.support = require_support();
    JSZip.defaults = require_defaults();
    JSZip.version = "3.10.1";
    JSZip.loadAsync = function(content, options) {
      return new JSZip().loadAsync(content, options);
    };
    JSZip.external = require_external();
    module.exports = JSZip;
  }
});
var jszip_default = require_lib3();

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/diagnostics.ts
var WordPortDiagnostics = class {
  items = [];
  info(code, message, detail) {
    this.push("info", code, message, detail);
  }
  warning(code, message, detail) {
    this.push("warning", code, message, detail);
  }
  error(code, message, detail) {
    this.push("error", code, message, detail);
  }
  push(severity, code, message, detail) {
    this.items.push({ severity, code, message, ...detail ?? {} });
  }
  merge(diagnostics) {
    if (!diagnostics) return;
    for (const diagnostic of diagnostics) this.items.push(diagnostic);
  }
  hasErrors() {
    return this.items.some((item) => item.severity === "error");
  }
};

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/paths.ts
function normalizeOpcPath(path) {
  return path.replace(/\\/g, "/").replace(/^\/+/, "");
}
function dirname(path) {
  const normalized = normalizeOpcPath(path);
  const index = normalized.lastIndexOf("/");
  return index === -1 ? "" : normalized.slice(0, index);
}
function resolveOpcTargetPath(sourcePartPath, target) {
  if (target.startsWith("/")) return normalizeOpcPath(target);
  const base = dirname(sourcePartPath);
  const segments = [...base ? base.split("/") : [], ...target.split("/")];
  const resolved = [];
  for (const segment of segments) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      resolved.pop();
      continue;
    }
    resolved.push(segment);
  }
  return resolved.join("/");
}
function relationshipsPathForPart(partPath) {
  const normalized = normalizeOpcPath(partPath);
  const base = dirname(normalized);
  const file = normalized.slice(base ? base.length + 1 : 0);
  return `${base ? `${base}/` : ""}_rels/${file}.rels`;
}
function sourcePartPathForRelationships(relsPath) {
  const normalized = normalizeOpcPath(relsPath);
  const match = normalized.match(/^(?:(.*)\/)?_rels\/(.+)\.rels$/);
  if (!match) return void 0;
  return normalizeOpcPath(`${match[1] ? `${match[1]}/` : ""}${match[2]}`);
}
function extensionForPath(path) {
  const filename = normalizeOpcPath(path).split("/").pop() ?? "";
  const index = filename.lastIndexOf(".");
  return index === -1 ? void 0 : filename.slice(index + 1).toLowerCase();
}

// archive/reorg-review/templates/business-basic/apps/web/vendor/fast-xml-parser.mjs
var nameStartChar = ":A-Za-z_\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD";
var nameChar = nameStartChar + "\\-.\\d\\u00B7\\u0300-\\u036F\\u203F-\\u2040";
var nameRegexp = "[" + nameStartChar + "][" + nameChar + "]*";
var regexName = new RegExp("^" + nameRegexp + "$");
function getAllMatches(string, regex) {
  const matches = [];
  let match = regex.exec(string);
  while (match) {
    const allmatches = [];
    allmatches.startIndex = regex.lastIndex - match[0].length;
    const len = match.length;
    for (let index = 0; index < len; index++) {
      allmatches.push(match[index]);
    }
    matches.push(allmatches);
    match = regex.exec(string);
  }
  return matches;
}
var isName = function(string) {
  const match = regexName.exec(string);
  return !(match === null || typeof match === "undefined");
};
function isExist(v) {
  return typeof v !== "undefined";
}
var DANGEROUS_PROPERTY_NAMES = [
  // '__proto__',
  // 'constructor',
  // 'prototype',
  "hasOwnProperty",
  "toString",
  "valueOf",
  "__defineGetter__",
  "__defineSetter__",
  "__lookupGetter__",
  "__lookupSetter__"
];
var criticalProperties = ["__proto__", "constructor", "prototype"];
var defaultOptions = {
  allowBooleanAttributes: false,
  //A tag can have attributes without any value
  unpairedTags: []
};
function validate(xmlData, options) {
  options = Object.assign({}, defaultOptions, options);
  const tags = [];
  let tagFound = false;
  let reachedRoot = false;
  if (xmlData[0] === "\uFEFF") {
    xmlData = xmlData.substr(1);
  }
  for (let i = 0; i < xmlData.length; i++) {
    if (xmlData[i] === "<" && xmlData[i + 1] === "?") {
      i += 2;
      i = readPI(xmlData, i);
      if (i.err) return i;
    } else if (xmlData[i] === "<") {
      let tagStartPos = i;
      i++;
      if (xmlData[i] === "!") {
        i = readCommentAndCDATA(xmlData, i);
        continue;
      } else {
        let closingTag = false;
        if (xmlData[i] === "/") {
          closingTag = true;
          i++;
        }
        let tagName = "";
        for (; i < xmlData.length && xmlData[i] !== ">" && xmlData[i] !== " " && xmlData[i] !== "	" && xmlData[i] !== "\n" && xmlData[i] !== "\r"; i++) {
          tagName += xmlData[i];
        }
        tagName = tagName.trim();
        if (tagName[tagName.length - 1] === "/") {
          tagName = tagName.substring(0, tagName.length - 1);
          i--;
        }
        if (!validateTagName(tagName)) {
          let msg;
          if (tagName.trim().length === 0) {
            msg = "Invalid space after '<'.";
          } else {
            msg = "Tag '" + tagName + "' is an invalid name.";
          }
          return getErrorObject("InvalidTag", msg, getLineNumberForPosition(xmlData, i));
        }
        const result = readAttributeStr(xmlData, i);
        if (result === false) {
          return getErrorObject("InvalidAttr", "Attributes for '" + tagName + "' have open quote.", getLineNumberForPosition(xmlData, i));
        }
        let attrStr = result.value;
        i = result.index;
        if (attrStr[attrStr.length - 1] === "/") {
          const attrStrStart = i - attrStr.length;
          attrStr = attrStr.substring(0, attrStr.length - 1);
          const isValid = validateAttributeString(attrStr, options);
          if (isValid === true) {
            tagFound = true;
          } else {
            return getErrorObject(isValid.err.code, isValid.err.msg, getLineNumberForPosition(xmlData, attrStrStart + isValid.err.line));
          }
        } else if (closingTag) {
          if (!result.tagClosed) {
            return getErrorObject("InvalidTag", "Closing tag '" + tagName + "' doesn't have proper closing.", getLineNumberForPosition(xmlData, i));
          } else if (attrStr.trim().length > 0) {
            return getErrorObject("InvalidTag", "Closing tag '" + tagName + "' can't have attributes or invalid starting.", getLineNumberForPosition(xmlData, tagStartPos));
          } else if (tags.length === 0) {
            return getErrorObject("InvalidTag", "Closing tag '" + tagName + "' has not been opened.", getLineNumberForPosition(xmlData, tagStartPos));
          } else {
            const otg = tags.pop();
            if (tagName !== otg.tagName) {
              let openPos = getLineNumberForPosition(xmlData, otg.tagStartPos);
              return getErrorObject(
                "InvalidTag",
                "Expected closing tag '" + otg.tagName + "' (opened in line " + openPos.line + ", col " + openPos.col + ") instead of closing tag '" + tagName + "'.",
                getLineNumberForPosition(xmlData, tagStartPos)
              );
            }
            if (tags.length == 0) {
              reachedRoot = true;
            }
          }
        } else {
          const isValid = validateAttributeString(attrStr, options);
          if (isValid !== true) {
            return getErrorObject(isValid.err.code, isValid.err.msg, getLineNumberForPosition(xmlData, i - attrStr.length + isValid.err.line));
          }
          if (reachedRoot === true) {
            return getErrorObject("InvalidXml", "Multiple possible root nodes found.", getLineNumberForPosition(xmlData, i));
          } else if (options.unpairedTags.indexOf(tagName) !== -1) {
          } else {
            tags.push({ tagName, tagStartPos });
          }
          tagFound = true;
        }
        for (i++; i < xmlData.length; i++) {
          if (xmlData[i] === "<") {
            if (xmlData[i + 1] === "!") {
              i++;
              i = readCommentAndCDATA(xmlData, i);
              continue;
            } else if (xmlData[i + 1] === "?") {
              i = readPI(xmlData, ++i);
              if (i.err) return i;
            } else {
              break;
            }
          } else if (xmlData[i] === "&") {
            const afterAmp = validateAmpersand(xmlData, i);
            if (afterAmp == -1)
              return getErrorObject("InvalidChar", "char '&' is not expected.", getLineNumberForPosition(xmlData, i));
            i = afterAmp;
          } else {
            if (reachedRoot === true && !isWhiteSpace(xmlData[i])) {
              return getErrorObject("InvalidXml", "Extra text at the end", getLineNumberForPosition(xmlData, i));
            }
          }
        }
        if (xmlData[i] === "<") {
          i--;
        }
      }
    } else {
      if (isWhiteSpace(xmlData[i])) {
        continue;
      }
      return getErrorObject("InvalidChar", "char '" + xmlData[i] + "' is not expected.", getLineNumberForPosition(xmlData, i));
    }
  }
  if (!tagFound) {
    return getErrorObject("InvalidXml", "Start tag expected.", 1);
  } else if (tags.length == 1) {
    return getErrorObject("InvalidTag", "Unclosed tag '" + tags[0].tagName + "'.", getLineNumberForPosition(xmlData, tags[0].tagStartPos));
  } else if (tags.length > 0) {
    return getErrorObject("InvalidXml", "Invalid '" + JSON.stringify(tags.map((t) => t.tagName), null, 4).replace(/\r?\n/g, "") + "' found.", { line: 1, col: 1 });
  }
  return true;
}
function isWhiteSpace(char) {
  return char === " " || char === "	" || char === "\n" || char === "\r";
}
function readPI(xmlData, i) {
  const start = i;
  for (; i < xmlData.length; i++) {
    if (xmlData[i] == "?" || xmlData[i] == " ") {
      const tagname = xmlData.substr(start, i - start);
      if (i > 5 && tagname === "xml") {
        return getErrorObject("InvalidXml", "XML declaration allowed only at the start of the document.", getLineNumberForPosition(xmlData, i));
      } else if (xmlData[i] == "?" && xmlData[i + 1] == ">") {
        i++;
        break;
      } else {
        continue;
      }
    }
  }
  return i;
}
function readCommentAndCDATA(xmlData, i) {
  if (xmlData.length > i + 5 && xmlData[i + 1] === "-" && xmlData[i + 2] === "-") {
    for (i += 3; i < xmlData.length; i++) {
      if (xmlData[i] === "-" && xmlData[i + 1] === "-" && xmlData[i + 2] === ">") {
        i += 2;
        break;
      }
    }
  } else if (xmlData.length > i + 8 && xmlData[i + 1] === "D" && xmlData[i + 2] === "O" && xmlData[i + 3] === "C" && xmlData[i + 4] === "T" && xmlData[i + 5] === "Y" && xmlData[i + 6] === "P" && xmlData[i + 7] === "E") {
    let angleBracketsCount = 1;
    for (i += 8; i < xmlData.length; i++) {
      if (xmlData[i] === "<") {
        angleBracketsCount++;
      } else if (xmlData[i] === ">") {
        angleBracketsCount--;
        if (angleBracketsCount === 0) {
          break;
        }
      }
    }
  } else if (xmlData.length > i + 9 && xmlData[i + 1] === "[" && xmlData[i + 2] === "C" && xmlData[i + 3] === "D" && xmlData[i + 4] === "A" && xmlData[i + 5] === "T" && xmlData[i + 6] === "A" && xmlData[i + 7] === "[") {
    for (i += 8; i < xmlData.length; i++) {
      if (xmlData[i] === "]" && xmlData[i + 1] === "]" && xmlData[i + 2] === ">") {
        i += 2;
        break;
      }
    }
  }
  return i;
}
var doubleQuote = '"';
var singleQuote = "'";
function readAttributeStr(xmlData, i) {
  let attrStr = "";
  let startChar = "";
  let tagClosed = false;
  for (; i < xmlData.length; i++) {
    if (xmlData[i] === doubleQuote || xmlData[i] === singleQuote) {
      if (startChar === "") {
        startChar = xmlData[i];
      } else if (startChar !== xmlData[i]) {
      } else {
        startChar = "";
      }
    } else if (xmlData[i] === ">") {
      if (startChar === "") {
        tagClosed = true;
        break;
      }
    }
    attrStr += xmlData[i];
  }
  if (startChar !== "") {
    return false;
  }
  return {
    value: attrStr,
    index: i,
    tagClosed
  };
}
var validAttrStrRegxp = new RegExp(`(\\s*)([^\\s=]+)(\\s*=)?(\\s*(['"])(([\\s\\S])*?)\\5)?`, "g");
function validateAttributeString(attrStr, options) {
  const matches = getAllMatches(attrStr, validAttrStrRegxp);
  const attrNames = {};
  for (let i = 0; i < matches.length; i++) {
    if (matches[i][1].length === 0) {
      return getErrorObject("InvalidAttr", "Attribute '" + matches[i][2] + "' has no space in starting.", getPositionFromMatch(matches[i]));
    } else if (matches[i][3] !== void 0 && matches[i][4] === void 0) {
      return getErrorObject("InvalidAttr", "Attribute '" + matches[i][2] + "' is without value.", getPositionFromMatch(matches[i]));
    } else if (matches[i][3] === void 0 && !options.allowBooleanAttributes) {
      return getErrorObject("InvalidAttr", "boolean attribute '" + matches[i][2] + "' is not allowed.", getPositionFromMatch(matches[i]));
    }
    const attrName = matches[i][2];
    if (!validateAttrName(attrName)) {
      return getErrorObject("InvalidAttr", "Attribute '" + attrName + "' is an invalid name.", getPositionFromMatch(matches[i]));
    }
    if (!Object.prototype.hasOwnProperty.call(attrNames, attrName)) {
      attrNames[attrName] = 1;
    } else {
      return getErrorObject("InvalidAttr", "Attribute '" + attrName + "' is repeated.", getPositionFromMatch(matches[i]));
    }
  }
  return true;
}
function validateNumberAmpersand(xmlData, i) {
  let re = /\d/;
  if (xmlData[i] === "x") {
    i++;
    re = /[\da-fA-F]/;
  }
  for (; i < xmlData.length; i++) {
    if (xmlData[i] === ";")
      return i;
    if (!xmlData[i].match(re))
      break;
  }
  return -1;
}
function validateAmpersand(xmlData, i) {
  i++;
  if (xmlData[i] === ";")
    return -1;
  if (xmlData[i] === "#") {
    i++;
    return validateNumberAmpersand(xmlData, i);
  }
  let count = 0;
  for (; i < xmlData.length; i++, count++) {
    if (xmlData[i].match(/\w/) && count < 20)
      continue;
    if (xmlData[i] === ";")
      break;
    return -1;
  }
  return i;
}
function getErrorObject(code, message, lineNumber) {
  return {
    err: {
      code,
      msg: message,
      line: lineNumber.line || lineNumber,
      col: lineNumber.col
    }
  };
}
function validateAttrName(attrName) {
  return isName(attrName);
}
function validateTagName(tagname) {
  return isName(tagname);
}
function getLineNumberForPosition(xmlData, index) {
  const lines = xmlData.substring(0, index).split(/\r?\n/);
  return {
    line: lines.length,
    // column number is last line's length + 1, because column numbering starts at 1:
    col: lines[lines.length - 1].length + 1
  };
}
function getPositionFromMatch(match) {
  return match.startIndex + match[1].length;
}
var BASIC_LATIN = {
  amp: "&",
  AMP: "&",
  lt: "<",
  LT: "<",
  gt: ">",
  GT: ">",
  quot: '"',
  QUOT: '"',
  apos: "'",
  lsquo: "\u2018",
  rsquo: "\u2019",
  ldquo: "\u201C",
  rdquo: "\u201D",
  lsquor: "\u201A",
  rsquor: "\u2019",
  ldquor: "\u201E",
  bdquo: "\u201E",
  comma: ",",
  period: ".",
  colon: ":",
  semi: ";",
  excl: "!",
  quest: "?",
  num: "#",
  dollar: "$",
  percent: "%",
  amp: "&",
  ast: "*",
  commat: "@",
  lowbar: "_",
  verbar: "|",
  vert: "|",
  sol: "/",
  bsol: "\\",
  lbrace: "{",
  rbrace: "}",
  lbrack: "[",
  rbrack: "]",
  lpar: "(",
  rpar: ")",
  nbsp: "\xA0",
  iexcl: "\xA1",
  cent: "\xA2",
  pound: "\xA3",
  curren: "\xA4",
  yen: "\xA5",
  brvbar: "\xA6",
  sect: "\xA7",
  uml: "\xA8",
  copy: "\xA9",
  COPY: "\xA9",
  ordf: "\xAA",
  laquo: "\xAB",
  not: "\xAC",
  shy: "\xAD",
  reg: "\xAE",
  REG: "\xAE",
  macr: "\xAF",
  deg: "\xB0",
  plusmn: "\xB1",
  sup2: "\xB2",
  sup3: "\xB3",
  acute: "\xB4",
  micro: "\xB5",
  para: "\xB6",
  middot: "\xB7",
  cedil: "\xB8",
  sup1: "\xB9",
  ordm: "\xBA",
  raquo: "\xBB",
  frac14: "\xBC",
  frac12: "\xBD",
  half: "\xBD",
  frac34: "\xBE",
  iquest: "\xBF",
  times: "\xD7",
  div: "\xF7",
  divide: "\xF7"
};
var LATIN_ACCENTS = {
  Agrave: "\xC0",
  agrave: "\xE0",
  Aacute: "\xC1",
  aacute: "\xE1",
  Acirc: "\xC2",
  acirc: "\xE2",
  Atilde: "\xC3",
  atilde: "\xE3",
  Auml: "\xC4",
  auml: "\xE4",
  Aring: "\xC5",
  aring: "\xE5",
  AElig: "\xC6",
  aelig: "\xE6",
  Ccedil: "\xC7",
  ccedil: "\xE7",
  Egrave: "\xC8",
  egrave: "\xE8",
  Eacute: "\xC9",
  eacute: "\xE9",
  Ecirc: "\xCA",
  ecirc: "\xEA",
  Euml: "\xCB",
  euml: "\xEB",
  Igrave: "\xCC",
  igrave: "\xEC",
  Iacute: "\xCD",
  iacute: "\xED",
  Icirc: "\xCE",
  icirc: "\xEE",
  Iuml: "\xCF",
  iuml: "\xEF",
  ETH: "\xD0",
  eth: "\xF0",
  Ntilde: "\xD1",
  ntilde: "\xF1",
  Ograve: "\xD2",
  ograve: "\xF2",
  Oacute: "\xD3",
  oacute: "\xF3",
  Ocirc: "\xD4",
  ocirc: "\xF4",
  Otilde: "\xD5",
  otilde: "\xF5",
  Ouml: "\xD6",
  ouml: "\xF6",
  Oslash: "\xD8",
  oslash: "\xF8",
  Ugrave: "\xD9",
  ugrave: "\xF9",
  Uacute: "\xDA",
  uacute: "\xFA",
  Ucirc: "\xDB",
  ucirc: "\xFB",
  Uuml: "\xDC",
  uuml: "\xFC",
  Yacute: "\xDD",
  yacute: "\xFD",
  THORN: "\xDE",
  thorn: "\xFE",
  szlig: "\xDF",
  yuml: "\xFF",
  Yuml: "\u0178"
};
var LATIN_EXTENDED = {
  Amacr: "\u0100",
  amacr: "\u0101",
  Abreve: "\u0102",
  abreve: "\u0103",
  Aogon: "\u0104",
  aogon: "\u0105",
  Cacute: "\u0106",
  cacute: "\u0107",
  Ccirc: "\u0108",
  ccirc: "\u0109",
  Cdot: "\u010A",
  cdot: "\u010B",
  Ccaron: "\u010C",
  ccaron: "\u010D",
  Dcaron: "\u010E",
  dcaron: "\u010F",
  Dstrok: "\u0110",
  dstrok: "\u0111",
  Emacr: "\u0112",
  emacr: "\u0113",
  Ecaron: "\u011A",
  ecaron: "\u011B",
  Edot: "\u0116",
  edot: "\u0117",
  Eogon: "\u0118",
  eogon: "\u0119",
  Gcirc: "\u011C",
  gcirc: "\u011D",
  Gbreve: "\u011E",
  gbreve: "\u011F",
  Gdot: "\u0120",
  gdot: "\u0121",
  Gcedil: "\u0122",
  Hcirc: "\u0124",
  hcirc: "\u0125",
  Hstrok: "\u0126",
  hstrok: "\u0127",
  Itilde: "\u0128",
  itilde: "\u0129",
  Imacr: "\u012A",
  imacr: "\u012B",
  Iogon: "\u012E",
  iogon: "\u012F",
  Idot: "\u0130",
  IJlig: "\u0132",
  ijlig: "\u0133",
  Jcirc: "\u0134",
  jcirc: "\u0135",
  Kcedil: "\u0136",
  kcedil: "\u0137",
  kgreen: "\u0138",
  Lacute: "\u0139",
  lacute: "\u013A",
  Lcedil: "\u013B",
  lcedil: "\u013C",
  Lcaron: "\u013D",
  lcaron: "\u013E",
  Lmidot: "\u013F",
  lmidot: "\u0140",
  Lstrok: "\u0141",
  lstrok: "\u0142",
  Nacute: "\u0143",
  nacute: "\u0144",
  Ncaron: "\u0147",
  ncaron: "\u0148",
  Ncedil: "\u0145",
  ncedil: "\u0146",
  ENG: "\u014A",
  eng: "\u014B",
  Omacr: "\u014C",
  omacr: "\u014D",
  Odblac: "\u0150",
  odblac: "\u0151",
  OElig: "\u0152",
  oelig: "\u0153",
  Racute: "\u0154",
  racute: "\u0155",
  Rcaron: "\u0158",
  rcaron: "\u0159",
  Rcedil: "\u0156",
  rcedil: "\u0157",
  Sacute: "\u015A",
  sacute: "\u015B",
  Scirc: "\u015C",
  scirc: "\u015D",
  Scedil: "\u015E",
  scedil: "\u015F",
  Scaron: "\u0160",
  scaron: "\u0161",
  Tcedil: "\u0162",
  tcedil: "\u0163",
  Tcaron: "\u0164",
  tcaron: "\u0165",
  Tstrok: "\u0166",
  tstrok: "\u0167",
  Utilde: "\u0168",
  utilde: "\u0169",
  Umacr: "\u016A",
  umacr: "\u016B",
  Ubreve: "\u016C",
  ubreve: "\u016D",
  Uring: "\u016E",
  uring: "\u016F",
  Udblac: "\u0170",
  udblac: "\u0171",
  Uogon: "\u0172",
  uogon: "\u0173",
  Wcirc: "\u0174",
  wcirc: "\u0175",
  Ycirc: "\u0176",
  ycirc: "\u0177",
  Zacute: "\u0179",
  zacute: "\u017A",
  Zdot: "\u017B",
  zdot: "\u017C",
  Zcaron: "\u017D",
  zcaron: "\u017E"
};
var GREEK = {
  Alpha: "\u0391",
  alpha: "\u03B1",
  Beta: "\u0392",
  beta: "\u03B2",
  Gamma: "\u0393",
  gamma: "\u03B3",
  Delta: "\u0394",
  delta: "\u03B4",
  Epsilon: "\u0395",
  epsilon: "\u03B5",
  epsiv: "\u03F5",
  varepsilon: "\u03F5",
  Zeta: "\u0396",
  zeta: "\u03B6",
  Eta: "\u0397",
  eta: "\u03B7",
  Theta: "\u0398",
  theta: "\u03B8",
  thetasym: "\u03D1",
  vartheta: "\u03D1",
  Iota: "\u0399",
  iota: "\u03B9",
  Kappa: "\u039A",
  kappa: "\u03BA",
  kappav: "\u03F0",
  varkappa: "\u03F0",
  Lambda: "\u039B",
  lambda: "\u03BB",
  Mu: "\u039C",
  mu: "\u03BC",
  Nu: "\u039D",
  nu: "\u03BD",
  Xi: "\u039E",
  xi: "\u03BE",
  Omicron: "\u039F",
  omicron: "\u03BF",
  Pi: "\u03A0",
  pi: "\u03C0",
  piv: "\u03D6",
  varpi: "\u03D6",
  Rho: "\u03A1",
  rho: "\u03C1",
  rhov: "\u03F1",
  varrho: "\u03F1",
  Sigma: "\u03A3",
  sigma: "\u03C3",
  sigmaf: "\u03C2",
  sigmav: "\u03C2",
  varsigma: "\u03C2",
  Tau: "\u03A4",
  tau: "\u03C4",
  Upsilon: "\u03A5",
  upsilon: "\u03C5",
  upsi: "\u03C5",
  Upsi: "\u03D2",
  upsih: "\u03D2",
  Phi: "\u03A6",
  phi: "\u03C6",
  phiv: "\u03D5",
  varphi: "\u03D5",
  Chi: "\u03A7",
  chi: "\u03C7",
  Psi: "\u03A8",
  psi: "\u03C8",
  Omega: "\u03A9",
  omega: "\u03C9",
  ohm: "\u03A9",
  Gammad: "\u03DC",
  gammad: "\u03DD",
  digamma: "\u03DD"
};
var CYRILLIC = {
  Afr: "\u{1D504}",
  afr: "\u{1D51E}",
  Acy: "\u0410",
  acy: "\u0430",
  Bcy: "\u0411",
  bcy: "\u0431",
  Vcy: "\u0412",
  vcy: "\u0432",
  Gcy: "\u0413",
  gcy: "\u0433",
  Dcy: "\u0414",
  dcy: "\u0434",
  IEcy: "\u0415",
  iecy: "\u0435",
  IOcy: "\u0401",
  iocy: "\u0451",
  ZHcy: "\u0416",
  zhcy: "\u0436",
  Zcy: "\u0417",
  zcy: "\u0437",
  Icy: "\u0418",
  icy: "\u0438",
  Jcy: "\u0419",
  jcy: "\u0439",
  Kcy: "\u041A",
  kcy: "\u043A",
  Lcy: "\u041B",
  lcy: "\u043B",
  Mcy: "\u041C",
  mcy: "\u043C",
  Ncy: "\u041D",
  ncy: "\u043D",
  Ocy: "\u041E",
  ocy: "\u043E",
  Pcy: "\u041F",
  pcy: "\u043F",
  Rcy: "\u0420",
  rcy: "\u0440",
  Scy: "\u0421",
  scy: "\u0441",
  Tcy: "\u0422",
  tcy: "\u0442",
  Ucy: "\u0423",
  ucy: "\u0443",
  Fcy: "\u0424",
  fcy: "\u0444",
  KHcy: "\u0425",
  khcy: "\u0445",
  TScy: "\u0426",
  tscy: "\u0446",
  CHcy: "\u0427",
  chcy: "\u0447",
  SHcy: "\u0428",
  shcy: "\u0448",
  SHCHcy: "\u0429",
  shchcy: "\u0449",
  HARDcy: "\u042A",
  hardcy: "\u044A",
  Ycy: "\u042B",
  ycy: "\u044B",
  SOFTcy: "\u042C",
  softcy: "\u044C",
  Ecy: "\u042D",
  ecy: "\u044D",
  YUcy: "\u042E",
  yucy: "\u044E",
  YAcy: "\u042F",
  yacy: "\u044F",
  DJcy: "\u0402",
  djcy: "\u0452",
  GJcy: "\u0403",
  gjcy: "\u0453",
  Jukcy: "\u0404",
  jukcy: "\u0454",
  DScy: "\u0405",
  dscy: "\u0455",
  Iukcy: "\u0406",
  iukcy: "\u0456",
  YIcy: "\u0407",
  yicy: "\u0457",
  Jsercy: "\u0408",
  jsercy: "\u0458",
  LJcy: "\u0409",
  ljcy: "\u0459",
  NJcy: "\u040A",
  njcy: "\u045A",
  TSHcy: "\u040B",
  tshcy: "\u045B",
  KJcy: "\u040C",
  kjcy: "\u045C",
  Ubrcy: "\u040E",
  ubrcy: "\u045E",
  DZcy: "\u040F",
  dzcy: "\u045F"
};
var MATH = {
  plus: "+",
  minus: "\u2212",
  mnplus: "\u2213",
  mp: "\u2213",
  pm: "\xB1",
  times: "\xD7",
  div: "\xF7",
  divide: "\xF7",
  sdot: "\u22C5",
  star: "\u2606",
  starf: "\u2605",
  bigstar: "\u2605",
  lowast: "\u2217",
  ast: "*",
  midast: "*",
  compfn: "\u2218",
  smallcircle: "\u2218",
  bullet: "\u2022",
  bull: "\u2022",
  nbsp: "\xA0",
  hellip: "\u2026",
  mldr: "\u2026",
  prime: "\u2032",
  Prime: "\u2033",
  tprime: "\u2034",
  bprime: "\u2035",
  backprime: "\u2035",
  minus: "\u2212",
  minusd: "\u2238",
  dotminus: "\u2238",
  plusdo: "\u2214",
  dotplus: "\u2214",
  plusmn: "\xB1",
  minusplus: "\u2213",
  mnplus: "\u2213",
  mp: "\u2213",
  setminus: "\u2216",
  smallsetminus: "\u2216",
  Backslash: "\u2216",
  setmn: "\u2216",
  ssetmn: "\u2216",
  lowbar: "_",
  verbar: "|",
  vert: "|",
  VerticalLine: "|",
  colon: ":",
  Colon: "\u2237",
  Proportion: "\u2237",
  ratio: "\u2236",
  equals: "=",
  ne: "\u2260",
  nequiv: "\u2262",
  equiv: "\u2261",
  Congruent: "\u2261",
  sim: "\u223C",
  thicksim: "\u223C",
  thksim: "\u223C",
  sime: "\u2243",
  simeq: "\u2243",
  TildeEqual: "\u2243",
  asymp: "\u2248",
  approx: "\u2248",
  thickapprox: "\u2248",
  thkap: "\u2248",
  TildeTilde: "\u2248",
  ncong: "\u2247",
  cong: "\u2245",
  TildeFullEqual: "\u2245",
  asympeq: "\u224D",
  CupCap: "\u224D",
  bump: "\u224E",
  Bumpeq: "\u224E",
  HumpDownHump: "\u224E",
  bumpe: "\u224F",
  bumpeq: "\u224F",
  HumpEqual: "\u224F",
  dotminus: "\u2238",
  minusd: "\u2238",
  plusdo: "\u2214",
  dotplus: "\u2214",
  le: "\u2264",
  LessEqual: "\u2264",
  ge: "\u2265",
  GreaterEqual: "\u2265",
  lesseqgtr: "\u22DA",
  lesseqqgtr: "\u2A8B",
  greater: ">",
  less: "<"
};
var MATH_ADVANCED = {
  alefsym: "\u2135",
  aleph: "\u2135",
  beth: "\u2136",
  gimel: "\u2137",
  daleth: "\u2138",
  forall: "\u2200",
  ForAll: "\u2200",
  part: "\u2202",
  PartialD: "\u2202",
  exist: "\u2203",
  Exists: "\u2203",
  nexist: "\u2204",
  nexists: "\u2204",
  empty: "\u2205",
  emptyset: "\u2205",
  emptyv: "\u2205",
  varnothing: "\u2205",
  nabla: "\u2207",
  Del: "\u2207",
  isin: "\u2208",
  isinv: "\u2208",
  in: "\u2208",
  Element: "\u2208",
  notin: "\u2209",
  notinva: "\u2209",
  ni: "\u220B",
  niv: "\u220B",
  SuchThat: "\u220B",
  ReverseElement: "\u220B",
  notni: "\u220C",
  notniva: "\u220C",
  prod: "\u220F",
  Product: "\u220F",
  coprod: "\u2210",
  Coproduct: "\u2210",
  sum: "\u2211",
  Sum: "\u2211",
  minus: "\u2212",
  mp: "\u2213",
  plusdo: "\u2214",
  dotplus: "\u2214",
  setminus: "\u2216",
  lowast: "\u2217",
  radic: "\u221A",
  Sqrt: "\u221A",
  prop: "\u221D",
  propto: "\u221D",
  Proportional: "\u221D",
  varpropto: "\u221D",
  infin: "\u221E",
  infintie: "\u29DD",
  ang: "\u2220",
  angle: "\u2220",
  angmsd: "\u2221",
  measuredangle: "\u2221",
  angsph: "\u2222",
  mid: "\u2223",
  VerticalBar: "\u2223",
  nmid: "\u2224",
  nsmid: "\u2224",
  npar: "\u2226",
  parallel: "\u2225",
  spar: "\u2225",
  nparallel: "\u2226",
  nspar: "\u2226",
  and: "\u2227",
  wedge: "\u2227",
  or: "\u2228",
  vee: "\u2228",
  cap: "\u2229",
  cup: "\u222A",
  int: "\u222B",
  Integral: "\u222B",
  conint: "\u222E",
  ContourIntegral: "\u222E",
  Conint: "\u222F",
  DoubleContourIntegral: "\u222F",
  Cconint: "\u2230",
  there4: "\u2234",
  therefore: "\u2234",
  Therefore: "\u2234",
  becaus: "\u2235",
  because: "\u2235",
  Because: "\u2235",
  ratio: "\u2236",
  Proportion: "\u2237",
  minusd: "\u2238",
  dotminus: "\u2238",
  mDDot: "\u223A",
  homtht: "\u223B",
  sim: "\u223C",
  bsimg: "\u223D",
  backsim: "\u223D",
  ac: "\u223E",
  mstpos: "\u223E",
  acd: "\u223F",
  VerticalTilde: "\u2240",
  wr: "\u2240",
  wreath: "\u2240",
  nsime: "\u2244",
  nsimeq: "\u2244",
  nsimeq: "\u2244",
  ncong: "\u2247",
  simne: "\u2246",
  ncongdot: "\u2A6D\u0338",
  ngsim: "\u2275",
  nsim: "\u2241",
  napprox: "\u2249",
  nap: "\u2249",
  ngeq: "\u2271",
  nge: "\u2271",
  nleq: "\u2270",
  nle: "\u2270",
  ngtr: "\u226F",
  ngt: "\u226F",
  nless: "\u226E",
  nlt: "\u226E",
  nprec: "\u2280",
  npr: "\u2280",
  nsucc: "\u2281",
  nsc: "\u2281"
};
var ARROWS = {
  larr: "\u2190",
  leftarrow: "\u2190",
  LeftArrow: "\u2190",
  uarr: "\u2191",
  uparrow: "\u2191",
  UpArrow: "\u2191",
  rarr: "\u2192",
  rightarrow: "\u2192",
  RightArrow: "\u2192",
  darr: "\u2193",
  downarrow: "\u2193",
  DownArrow: "\u2193",
  harr: "\u2194",
  leftrightarrow: "\u2194",
  LeftRightArrow: "\u2194",
  varr: "\u2195",
  updownarrow: "\u2195",
  UpDownArrow: "\u2195",
  nwarr: "\u2196",
  nwarrow: "\u2196",
  UpperLeftArrow: "\u2196",
  nearr: "\u2197",
  nearrow: "\u2197",
  UpperRightArrow: "\u2197",
  searr: "\u2198",
  searrow: "\u2198",
  LowerRightArrow: "\u2198",
  swarr: "\u2199",
  swarrow: "\u2199",
  LowerLeftArrow: "\u2199",
  lArr: "\u21D0",
  Leftarrow: "\u21D0",
  uArr: "\u21D1",
  Uparrow: "\u21D1",
  rArr: "\u21D2",
  Rightarrow: "\u21D2",
  dArr: "\u21D3",
  Downarrow: "\u21D3",
  hArr: "\u21D4",
  Leftrightarrow: "\u21D4",
  iff: "\u21D4",
  vArr: "\u21D5",
  Updownarrow: "\u21D5",
  lAarr: "\u21DA",
  Lleftarrow: "\u21DA",
  rAarr: "\u21DB",
  Rrightarrow: "\u21DB",
  lrarr: "\u21C6",
  leftrightarrows: "\u21C6",
  rlarr: "\u21C4",
  rightleftarrows: "\u21C4",
  lrhar: "\u21CB",
  leftrightharpoons: "\u21CB",
  ReverseEquilibrium: "\u21CB",
  rlhar: "\u21CC",
  rightleftharpoons: "\u21CC",
  Equilibrium: "\u21CC",
  udarr: "\u21C5",
  UpArrowDownArrow: "\u21C5",
  duarr: "\u21F5",
  DownArrowUpArrow: "\u21F5",
  llarr: "\u21C7",
  leftleftarrows: "\u21C7",
  rrarr: "\u21C9",
  rightrightarrows: "\u21C9",
  ddarr: "\u21CA",
  downdownarrows: "\u21CA",
  har: "\u21BD",
  lhard: "\u21BD",
  leftharpoondown: "\u21BD",
  lharu: "\u21BC",
  leftharpoonup: "\u21BC",
  rhard: "\u21C1",
  rightharpoondown: "\u21C1",
  rharu: "\u21C0",
  rightharpoonup: "\u21C0",
  lsh: "\u21B0",
  Lsh: "\u21B0",
  rsh: "\u21B1",
  Rsh: "\u21B1",
  ldsh: "\u21B2",
  rdsh: "\u21B3",
  hookleftarrow: "\u21A9",
  hookrightarrow: "\u21AA",
  mapstoleft: "\u21A4",
  mapstoup: "\u21A5",
  map: "\u21A6",
  mapsto: "\u21A6",
  mapstodown: "\u21A7",
  crarr: "\u21B5",
  nwarrow: "\u2196",
  nearrow: "\u2197",
  searrow: "\u2198",
  swarrow: "\u2199",
  nleftarrow: "\u219A",
  nleftrightarrow: "\u21AE",
  nrightarrow: "\u219B",
  nrarr: "\u219B",
  larrtl: "\u21A2",
  rarrtl: "\u21A3",
  leftarrowtail: "\u21A2",
  rightarrowtail: "\u21A3",
  twoheadleftarrow: "\u219E",
  twoheadrightarrow: "\u21A0",
  Larr: "\u219E",
  Rarr: "\u21A0",
  larrhk: "\u21A9",
  rarrhk: "\u21AA",
  larrlp: "\u21AB",
  looparrowleft: "\u21AB",
  rarrlp: "\u21AC",
  looparrowright: "\u21AC",
  harrw: "\u21AD",
  leftrightsquigarrow: "\u21AD",
  nrarrw: "\u219D\u0338",
  rarrw: "\u219D",
  rightsquigarrow: "\u219D",
  larrbfs: "\u291F",
  rarrbfs: "\u2920",
  nvHarr: "\u2904",
  nvlArr: "\u2902",
  nvrArr: "\u2903",
  larrfs: "\u291D",
  rarrfs: "\u291E",
  Map: "\u2905",
  larrsim: "\u2973",
  rarrsim: "\u2974",
  harrcir: "\u2948",
  Uarrocir: "\u2949",
  lurdshar: "\u294A",
  ldrdhar: "\u2967",
  ldrushar: "\u294B",
  rdldhar: "\u2969",
  lrhard: "\u296D",
  rlhar: "\u21CC",
  uharr: "\u21BE",
  uharl: "\u21BF",
  dharr: "\u21C2",
  dharl: "\u21C3",
  Uarr: "\u219F",
  Darr: "\u21A1",
  zigrarr: "\u21DD",
  nwArr: "\u21D6",
  neArr: "\u21D7",
  seArr: "\u21D8",
  swArr: "\u21D9",
  nharr: "\u21AE",
  nhArr: "\u21CE",
  nlarr: "\u219A",
  nlArr: "\u21CD",
  nrarr: "\u219B",
  nrArr: "\u21CF",
  larrb: "\u21E4",
  LeftArrowBar: "\u21E4",
  rarrb: "\u21E5",
  RightArrowBar: "\u21E5"
};
var SHAPES = {
  square: "\u25A1",
  Square: "\u25A1",
  squ: "\u25A1",
  squf: "\u25AA",
  squarf: "\u25AA",
  blacksquar: "\u25AA",
  blacksquare: "\u25AA",
  FilledVerySmallSquare: "\u25AA",
  blk34: "\u2593",
  blk12: "\u2592",
  blk14: "\u2591",
  block: "\u2588",
  srect: "\u25AD",
  rect: "\u25AD",
  sdot: "\u22C5",
  sdotb: "\u22A1",
  dotsquare: "\u22A1",
  triangle: "\u25B5",
  tri: "\u25B5",
  trine: "\u25B5",
  utri: "\u25B5",
  triangledown: "\u25BF",
  dtri: "\u25BF",
  tridown: "\u25BF",
  triangleleft: "\u25C3",
  ltri: "\u25C3",
  triangleright: "\u25B9",
  rtri: "\u25B9",
  blacktriangle: "\u25B4",
  utrif: "\u25B4",
  blacktriangledown: "\u25BE",
  dtrif: "\u25BE",
  blacktriangleleft: "\u25C2",
  ltrif: "\u25C2",
  blacktriangleright: "\u25B8",
  rtrif: "\u25B8",
  loz: "\u25CA",
  lozenge: "\u25CA",
  blacklozenge: "\u29EB",
  lozf: "\u29EB",
  bigcirc: "\u25EF",
  xcirc: "\u25EF",
  circ: "\u02C6",
  Circle: "\u25CB",
  cir: "\u25CB",
  o: "\u25CB",
  bullet: "\u2022",
  bull: "\u2022",
  hellip: "\u2026",
  mldr: "\u2026",
  nldr: "\u2025",
  boxh: "\u2500",
  HorizontalLine: "\u2500",
  boxv: "\u2502",
  boxdr: "\u250C",
  boxdl: "\u2510",
  boxur: "\u2514",
  boxul: "\u2518",
  boxvr: "\u251C",
  boxvl: "\u2524",
  boxhd: "\u252C",
  boxhu: "\u2534",
  boxvh: "\u253C",
  boxH: "\u2550",
  boxV: "\u2551",
  boxdR: "\u2552",
  boxDr: "\u2553",
  boxDR: "\u2554",
  boxDl: "\u2555",
  boxdL: "\u2556",
  boxDL: "\u2557",
  boxuR: "\u2558",
  boxUr: "\u2559",
  boxUR: "\u255A",
  boxUl: "\u255C",
  boxuL: "\u255B",
  boxUL: "\u255D",
  boxvR: "\u255E",
  boxVr: "\u255F",
  boxVR: "\u2560",
  boxVl: "\u2562",
  boxvL: "\u2561",
  boxVL: "\u2563",
  boxHd: "\u2564",
  boxhD: "\u2565",
  boxHD: "\u2566",
  boxHu: "\u2567",
  boxhU: "\u2568",
  boxHU: "\u2569",
  boxvH: "\u256A",
  boxVh: "\u256B",
  boxVH: "\u256C"
};
var PUNCTUATION = {
  excl: "!",
  iexcl: "\xA1",
  brvbar: "\xA6",
  sect: "\xA7",
  uml: "\xA8",
  copy: "\xA9",
  ordf: "\xAA",
  laquo: "\xAB",
  not: "\xAC",
  shy: "\xAD",
  reg: "\xAE",
  macr: "\xAF",
  deg: "\xB0",
  plusmn: "\xB1",
  sup2: "\xB2",
  sup3: "\xB3",
  acute: "\xB4",
  micro: "\xB5",
  para: "\xB6",
  middot: "\xB7",
  cedil: "\xB8",
  sup1: "\xB9",
  ordm: "\xBA",
  raquo: "\xBB",
  frac14: "\xBC",
  frac12: "\xBD",
  frac34: "\xBE",
  iquest: "\xBF",
  nbsp: "\xA0",
  comma: ",",
  period: ".",
  colon: ":",
  semi: ";",
  vert: "|",
  Verbar: "\u2016",
  verbar: "|",
  dblac: "\u02DD",
  circ: "\u02C6",
  caron: "\u02C7",
  breve: "\u02D8",
  dot: "\u02D9",
  ring: "\u02DA",
  ogon: "\u02DB",
  tilde: "\u02DC",
  DiacriticalGrave: "`",
  DiacriticalAcute: "\xB4",
  DiacriticalTilde: "\u02DC",
  DiacriticalDot: "\u02D9",
  DiacriticalDoubleAcute: "\u02DD",
  grave: "`",
  acute: "\xB4"
};
var CURRENCY = {
  cent: "\xA2",
  pound: "\xA3",
  curren: "\xA4",
  yen: "\xA5",
  euro: "\u20AC",
  dollar: "$",
  euro: "\u20AC",
  fnof: "\u0192",
  inr: "\u20B9",
  af: "\u060B",
  birr: "\u1265\u122D",
  peso: "\u20B1",
  rub: "\u20BD",
  won: "\u20A9",
  yuan: "\xA5",
  cedil: "\xB8"
};
var FRACTIONS = {
  frac12: "\xBD",
  half: "\xBD",
  frac13: "\u2153",
  frac14: "\xBC",
  frac15: "\u2155",
  frac16: "\u2159",
  frac18: "\u215B",
  frac23: "\u2154",
  frac25: "\u2156",
  frac34: "\xBE",
  frac35: "\u2157",
  frac38: "\u215C",
  frac45: "\u2158",
  frac56: "\u215A",
  frac58: "\u215D",
  frac78: "\u215E",
  frasl: "\u2044"
};
var MISC_SYMBOLS = {
  trade: "\u2122",
  TRADE: "\u2122",
  telrec: "\u2315",
  target: "\u2316",
  ulcorn: "\u231C",
  ulcorner: "\u231C",
  urcorn: "\u231D",
  urcorner: "\u231D",
  dlcorn: "\u231E",
  llcorner: "\u231E",
  drcorn: "\u231F",
  lrcorner: "\u231F",
  intercal: "\u22BA",
  intcal: "\u22BA",
  oplus: "\u2295",
  CirclePlus: "\u2295",
  ominus: "\u2296",
  CircleMinus: "\u2296",
  otimes: "\u2297",
  CircleTimes: "\u2297",
  osol: "\u2298",
  odot: "\u2299",
  CircleDot: "\u2299",
  oast: "\u229B",
  circledast: "\u229B",
  odash: "\u229D",
  circleddash: "\u229D",
  ocirc: "\u229A",
  circledcirc: "\u229A",
  boxplus: "\u229E",
  plusb: "\u229E",
  boxminus: "\u229F",
  minusb: "\u229F",
  boxtimes: "\u22A0",
  timesb: "\u22A0",
  boxdot: "\u22A1",
  sdotb: "\u22A1",
  veebar: "\u22BB",
  vee: "\u2228",
  barvee: "\u22BD",
  and: "\u2227",
  wedge: "\u2227",
  Cap: "\u22D2",
  Cup: "\u22D3",
  Fork: "\u22D4",
  pitchfork: "\u22D4",
  epar: "\u22D5",
  ltlarr: "\u2976",
  nvap: "\u224D\u20D2",
  nvsim: "\u223C\u20D2",
  nvge: "\u2265\u20D2",
  nvle: "\u2264\u20D2",
  nvlt: "<\u20D2",
  nvgt: ">\u20D2",
  nvltrie: "\u22B4\u20D2",
  nvrtrie: "\u22B5\u20D2",
  Vdash: "\u22A9",
  dashv: "\u22A3",
  vDash: "\u22A8",
  Vdash: "\u22A9",
  Vvdash: "\u22AA",
  nvdash: "\u22AC",
  nvDash: "\u22AD",
  nVdash: "\u22AE",
  nVDash: "\u22AF"
};
var ALL_ENTITIES = {
  ...BASIC_LATIN,
  ...LATIN_ACCENTS,
  ...LATIN_EXTENDED,
  ...GREEK,
  ...CYRILLIC,
  ...MATH,
  ...MATH_ADVANCED,
  ...ARROWS,
  ...SHAPES,
  ...PUNCTUATION,
  ...CURRENCY,
  ...FRACTIONS,
  ...MISC_SYMBOLS
};
var XML = {
  amp: "&",
  apos: "'",
  gt: ">",
  lt: "<",
  quot: '"'
};
var COMMON_HTML = {
  nbsp: "\xA0",
  copy: "\xA9",
  reg: "\xAE",
  trade: "\u2122",
  mdash: "\u2014",
  ndash: "\u2013",
  hellip: "\u2026",
  laquo: "\xAB",
  raquo: "\xBB",
  lsquo: "\u2018",
  rsquo: "\u2019",
  ldquo: "\u201C",
  rdquo: "\u201D",
  bull: "\u2022",
  para: "\xB6",
  sect: "\xA7",
  deg: "\xB0",
  frac12: "\xBD",
  frac14: "\xBC",
  frac34: "\xBE"
};
var SPECIAL_CHARS = new Set("!?\\\\/[]$%{}^&*()<>|+");
function validateEntityName(name) {
  if (name[0] === "#") {
    throw new Error(`[EntityReplacer] Invalid character '#' in entity name: "${name}"`);
  }
  for (const ch of name) {
    if (SPECIAL_CHARS.has(ch)) {
      throw new Error(`[EntityReplacer] Invalid character '${ch}' in entity name: "${name}"`);
    }
  }
  return name;
}
function mergeEntityMaps(...maps) {
  const out = /* @__PURE__ */ Object.create(null);
  for (const map of maps) {
    if (!map) continue;
    for (const key of Object.keys(map)) {
      const raw = map[key];
      if (typeof raw === "string") {
        out[key] = raw;
      } else if (raw && typeof raw === "object" && raw.val !== void 0) {
        const val2 = raw.val;
        if (typeof val2 === "string") {
          out[key] = val2;
        }
      }
    }
  }
  return out;
}
var LIMIT_TIER_EXTERNAL = "external";
var LIMIT_TIER_BASE = "base";
var LIMIT_TIER_ALL = "all";
function parseLimitTiers(raw) {
  if (!raw || raw === LIMIT_TIER_EXTERNAL) return /* @__PURE__ */ new Set([LIMIT_TIER_EXTERNAL]);
  if (raw === LIMIT_TIER_ALL) return /* @__PURE__ */ new Set([LIMIT_TIER_ALL]);
  if (raw === LIMIT_TIER_BASE) return /* @__PURE__ */ new Set([LIMIT_TIER_BASE]);
  if (Array.isArray(raw)) return new Set(raw);
  return /* @__PURE__ */ new Set([LIMIT_TIER_EXTERNAL]);
}
var NCR_LEVEL = Object.freeze({ allow: 0, leave: 1, remove: 2, throw: 3 });
var XML10_ALLOWED_C0 = /* @__PURE__ */ new Set([9, 10, 13]);
function parseNCRConfig(ncr) {
  if (!ncr) {
    return { xmlVersion: 1, onLevel: NCR_LEVEL.allow, nullLevel: NCR_LEVEL.remove };
  }
  const xmlVersion = ncr.xmlVersion === 1.1 ? 1.1 : 1;
  const onLevel = NCR_LEVEL[ncr.onNCR] ?? NCR_LEVEL.allow;
  const nullLevel = NCR_LEVEL[ncr.nullNCR] ?? NCR_LEVEL.remove;
  const clampedNull = Math.max(nullLevel, NCR_LEVEL.remove);
  return { xmlVersion, onLevel, nullLevel: clampedNull };
}
var EntityDecoder = class {
  /**
   * @param {object} [options]
   * @param {object|null}  [options.namedEntities]        — extra named entities merged into base map
   * @param {object}  [options.limit]                 — security limits
   * @param {number}       [options.limit.maxTotalExpansions=0]  — 0 = unlimited
   * @param {number}       [options.limit.maxExpandedLength=0]   — 0 = unlimited
   * @param {'external'|'base'|'all'|string[]} [options.limit.applyLimitsTo='external']
   *   Which entity tiers count against the security limits:
   *   - 'external' (default) — only input/runtime + persistent external entities
   *   - 'base'               — only DEFAULT_XML_ENTITIES + namedEntities
   *   - 'all'                — every entity regardless of tier
   *   - string[]             — explicit combination, e.g. ['external', 'base']
   * @param {((resolved: string, original: string) => string)|null} [options.postCheck=null]
   * @param {string[]} [options.remove=[]] — entity names (e.g. ['nbsp', '#13']) to delete (replace with empty string)
   * @param {string[]} [options.leave=[]]  — entity names to keep as literal (unchanged in output)
   * @param {object}   [options.ncr]       — Numeric Character Reference controls
   * @param {1.0|1.1}  [options.ncr.xmlVersion=1.0]
   *   XML version governing which codepoint ranges are restricted:
   *   - 1.0 — C0 controls U+0001–U+001F (except U+0009/000A/000D) are prohibited
   *   - 1.1 — C0 controls are allowed when written as NCRs; C1 (U+007F–U+009F) decoded as-is
   * @param {'allow'|'leave'|'remove'|'throw'} [options.ncr.onNCR='allow']
   *   Base action for numeric references. Severity order: allow < leave < remove < throw.
   *   For codepoint ranges that carry a minimum level (surrogates → remove, XML 1.0 C0 → remove),
   *   the effective action is max(onNCR, rangeMinimum).
   * @param {'remove'|'throw'} [options.ncr.nullNCR='remove']
   *   Action for U+0000 (null). 'allow' and 'leave' are clamped to 'remove' since null is never safe.
   */
  constructor(options = {}) {
    this._limit = options.limit || {};
    this._maxTotalExpansions = this._limit.maxTotalExpansions || 0;
    this._maxExpandedLength = this._limit.maxExpandedLength || 0;
    this._postCheck = typeof options.postCheck === "function" ? options.postCheck : (r) => r;
    this._limitTiers = parseLimitTiers(this._limit.applyLimitsTo ?? LIMIT_TIER_EXTERNAL);
    this._numericAllowed = options.numericAllowed ?? true;
    this._baseMap = mergeEntityMaps(XML, options.namedEntities || null);
    this._externalMap = /* @__PURE__ */ Object.create(null);
    this._inputMap = /* @__PURE__ */ Object.create(null);
    this._totalExpansions = 0;
    this._expandedLength = 0;
    this._removeSet = new Set(options.remove && Array.isArray(options.remove) ? options.remove : []);
    this._leaveSet = new Set(options.leave && Array.isArray(options.leave) ? options.leave : []);
    const ncrCfg = parseNCRConfig(options.ncr);
    this._ncrXmlVersion = ncrCfg.xmlVersion;
    this._ncrOnLevel = ncrCfg.onLevel;
    this._ncrNullLevel = ncrCfg.nullLevel;
  }
  // -------------------------------------------------------------------------
  // Persistent external entity registration
  // -------------------------------------------------------------------------
  /**
   * Replace the full set of persistent external entities.
   * All keys are validated — throws on invalid characters.
   * @param {Record<string, string | { regex?: RegExp, val: string }>} map
   */
  setExternalEntities(map) {
    if (map) {
      for (const key of Object.keys(map)) {
        validateEntityName(key);
      }
    }
    this._externalMap = mergeEntityMaps(map);
  }
  /**
   * Add a single persistent external entity.
   * @param {string} key
   * @param {string} value
   */
  addExternalEntity(key, value) {
    validateEntityName(key);
    if (typeof value === "string" && value.indexOf("&") === -1) {
      this._externalMap[key] = value;
    }
  }
  // -------------------------------------------------------------------------
  // Input / runtime entity registration (per document)
  // -------------------------------------------------------------------------
  /**
   * Inject DOCTYPE entities for the current document.
   * Also resets per-document expansion counters.
   * @param {Record<string, string | { regx?: RegExp, regex?: RegExp, val: string }>} map
   */
  addInputEntities(map) {
    this._totalExpansions = 0;
    this._expandedLength = 0;
    this._inputMap = mergeEntityMaps(map);
  }
  // -------------------------------------------------------------------------
  // Per-document reset
  // -------------------------------------------------------------------------
  /**
   * Wipe input/runtime entities and reset counters.
   * Call this before processing each new document.
   * @returns {this}
   */
  reset() {
    this._inputMap = /* @__PURE__ */ Object.create(null);
    this._totalExpansions = 0;
    this._expandedLength = 0;
    return this;
  }
  // -------------------------------------------------------------------------
  // XML version (can be set after construction, e.g. once parser reads <?xml?>)
  // -------------------------------------------------------------------------
  /**
   * Update the XML version used for NCR classification.
   * Call this as soon as the document's `<?xml version="...">` declaration is parsed.
   * @param {1.0|1.1|number} version
   */
  setXmlVersion(version) {
    this._ncrXmlVersion = version === 1.1 ? 1.1 : 1;
  }
  // -------------------------------------------------------------------------
  // Primary API
  // -------------------------------------------------------------------------
  /**
   * Replace all entity references in `str` in a single pass.
   *
   * @param {string} str
   * @returns {string}
   */
  decode(str) {
    if (typeof str !== "string" || str.length === 0) return str;
    const original = str;
    const chunks = [];
    const len = str.length;
    let last = 0;
    let i = 0;
    const limitExpansions = this._maxTotalExpansions > 0;
    const limitLength = this._maxExpandedLength > 0;
    const checkLimits = limitExpansions || limitLength;
    while (i < len) {
      if (str.charCodeAt(i) !== 38) {
        i++;
        continue;
      }
      let j = i + 1;
      while (j < len && str.charCodeAt(j) !== 59 && j - i <= 32) j++;
      if (j >= len || str.charCodeAt(j) !== 59) {
        i++;
        continue;
      }
      const token = str.slice(i + 1, j);
      if (token.length === 0) {
        i++;
        continue;
      }
      let replacement;
      let tier;
      if (this._removeSet.has(token)) {
        replacement = "";
        if (tier === void 0) {
          tier = LIMIT_TIER_EXTERNAL;
        }
      } else if (this._leaveSet.has(token)) {
        i++;
        continue;
      } else if (token.charCodeAt(0) === 35) {
        const ncrResult = this._resolveNCR(token);
        if (ncrResult === void 0) {
          i++;
          continue;
        }
        replacement = ncrResult;
        tier = LIMIT_TIER_BASE;
      } else {
        const resolved = this._resolveName(token);
        replacement = resolved?.value;
        tier = resolved?.tier;
      }
      if (replacement === void 0) {
        i++;
        continue;
      }
      if (i > last) chunks.push(str.slice(last, i));
      chunks.push(replacement);
      last = j + 1;
      i = last;
      if (checkLimits && this._tierCounts(tier)) {
        if (limitExpansions) {
          this._totalExpansions++;
          if (this._totalExpansions > this._maxTotalExpansions) {
            throw new Error(
              `[EntityReplacer] Entity expansion count limit exceeded: ${this._totalExpansions} > ${this._maxTotalExpansions}`
            );
          }
        }
        if (limitLength) {
          const delta = replacement.length - (token.length + 2);
          if (delta > 0) {
            this._expandedLength += delta;
            if (this._expandedLength > this._maxExpandedLength) {
              throw new Error(
                `[EntityReplacer] Expanded content length limit exceeded: ${this._expandedLength} > ${this._maxExpandedLength}`
              );
            }
          }
        }
      }
    }
    if (last < len) chunks.push(str.slice(last));
    const result = chunks.length === 0 ? str : chunks.join("");
    return this._postCheck(result, original);
  }
  // -------------------------------------------------------------------------
  // Private: limit tier check
  // -------------------------------------------------------------------------
  /**
   * Returns true if a resolved entity of the given tier should count
   * against the expansion/length limits.
   * @param {string} tier  — LIMIT_TIER_EXTERNAL | LIMIT_TIER_BASE
   * @returns {boolean}
   */
  _tierCounts(tier) {
    if (this._limitTiers.has(LIMIT_TIER_ALL)) return true;
    return this._limitTiers.has(tier);
  }
  // -------------------------------------------------------------------------
  // Private: entity resolution
  // -------------------------------------------------------------------------
  /**
   * Resolve a named entity token (without & and ;).
   * Priority: inputMap > externalMap > baseMap
   * Returns the resolved value tagged with its limit tier.
   *
   * @param {string} name
   * @returns {{ value: string, tier: string }|undefined}
   */
  _resolveName(name) {
    if (name in this._inputMap) return { value: this._inputMap[name], tier: LIMIT_TIER_EXTERNAL };
    if (name in this._externalMap) return { value: this._externalMap[name], tier: LIMIT_TIER_EXTERNAL };
    if (name in this._baseMap) return { value: this._baseMap[name], tier: LIMIT_TIER_BASE };
    return void 0;
  }
  /**
   * Classify a codepoint and return the minimum action level that must be applied.
   * Returns -1 when no minimum is imposed (normal allow path).
   *
   * Ranges checked (in priority order):
   *   1. U+0000            — null, governed by nullNCR (always ≥ remove)
   *   2. U+D800–U+DFFF     — surrogates, always prohibited (min: remove)
   *   3. U+0001–U+001F \ {0x09,0x0A,0x0D}  — XML 1.0 restricted C0 (min: remove)
   *      (skipped in XML 1.1 — C0 controls are allowed when written as NCRs)
   *
   * @param {number} cp  — codepoint
   * @returns {number}   — minimum NCR_LEVEL value, or -1 for no restriction
   */
  _classifyNCR(cp) {
    if (cp === 0) return this._ncrNullLevel;
    if (cp >= 55296 && cp <= 57343) return NCR_LEVEL.remove;
    if (this._ncrXmlVersion === 1) {
      if (cp >= 1 && cp <= 31 && !XML10_ALLOWED_C0.has(cp)) return NCR_LEVEL.remove;
    }
    return -1;
  }
  /**
   * Execute a resolved NCR action.
   *
   * @param {number} action   — NCR_LEVEL value
   * @param {string} token    — raw token (e.g. '#38') for error messages
   * @param {number} cp       — codepoint, used only for error messages
   * @returns {string|undefined}
   *   - decoded character string  → 'allow'
   *   - ''                        → 'remove'
   *   - undefined                 → 'leave' (caller must skip past '&' only)
   *   - throws Error              → 'throw'
   */
  _applyNCRAction(action, token, cp) {
    switch (action) {
      case NCR_LEVEL.allow:
        return String.fromCodePoint(cp);
      case NCR_LEVEL.remove:
        return "";
      case NCR_LEVEL.leave:
        return void 0;
      // signal: keep literal
      case NCR_LEVEL.throw:
        throw new Error(
          `[EntityDecoder] Prohibited numeric character reference &${token}; (U+${cp.toString(16).toUpperCase().padStart(4, "0")})`
        );
      default:
        return String.fromCodePoint(cp);
    }
  }
  /**
   * Full NCR resolution pipeline for a numeric token.
   *
   * Steps:
   *   1. Parse the codepoint (decimal or hex).
   *   2. Validate the raw codepoint range (NaN, <0, >0x10FFFF).
   *   3. If numericAllowed is false and no minimum restriction applies → leave as-is.
   *   4. Classify the codepoint to find the minimum required action level.
   *   5. Resolve effective action = max(onNCR, minimum).
   *   6. Apply and return.
   *
   * @param {string} token  — e.g. '#38', '#x26', '#X26'
   * @returns {string|undefined}
   *   - string (incl. '')  — replacement ('' = remove)
   *   - undefined          — leave original &token; as-is
   */
  _resolveNCR(token) {
    const second = token.charCodeAt(1);
    let cp;
    if (second === 120 || second === 88) {
      cp = parseInt(token.slice(2), 16);
    } else {
      cp = parseInt(token.slice(1), 10);
    }
    if (Number.isNaN(cp) || cp < 0 || cp > 1114111) return void 0;
    const minimum = this._classifyNCR(cp);
    if (!this._numericAllowed && minimum < NCR_LEVEL.remove) return void 0;
    const effective = minimum === -1 ? this._ncrOnLevel : Math.max(this._ncrOnLevel, minimum);
    return this._applyNCRAction(effective, token, cp);
  }
};
var defaultOnDangerousProperty = (name) => {
  if (DANGEROUS_PROPERTY_NAMES.includes(name)) {
    return "__" + name;
  }
  return name;
};
var defaultOptions2 = {
  preserveOrder: false,
  attributeNamePrefix: "@_",
  attributesGroupName: false,
  textNodeName: "#text",
  ignoreAttributes: true,
  removeNSPrefix: false,
  // remove NS from tag name or attribute name if true
  allowBooleanAttributes: false,
  //a tag can have attributes without any value
  //ignoreRootElement : false,
  parseTagValue: true,
  parseAttributeValue: false,
  trimValues: true,
  //Trim string values of tag and attributes
  cdataPropName: false,
  numberParseOptions: {
    hex: true,
    leadingZeros: true,
    eNotation: true
  },
  tagValueProcessor: function(tagName, val2) {
    return val2;
  },
  attributeValueProcessor: function(attrName, val2) {
    return val2;
  },
  stopNodes: [],
  //nested tags will not be parsed even for errors
  alwaysCreateTextNode: false,
  isArray: () => false,
  commentPropName: false,
  unpairedTags: [],
  processEntities: true,
  htmlEntities: false,
  entityDecoder: null,
  ignoreDeclaration: false,
  ignorePiTags: false,
  transformTagName: false,
  transformAttributeName: false,
  updateTag: function(tagName, jPath, attrs) {
    return tagName;
  },
  // skipEmptyListItem: false
  captureMetaData: false,
  maxNestedTags: 100,
  strictReservedNames: true,
  jPath: true,
  // if true, pass jPath string to callbacks; if false, pass matcher instance
  onDangerousProperty: defaultOnDangerousProperty
};
function validatePropertyName(propertyName, optionName) {
  if (typeof propertyName !== "string") {
    return;
  }
  const normalized = propertyName.toLowerCase();
  if (DANGEROUS_PROPERTY_NAMES.some((dangerous) => normalized === dangerous.toLowerCase())) {
    throw new Error(
      `[SECURITY] Invalid ${optionName}: "${propertyName}" is a reserved JavaScript keyword that could cause prototype pollution`
    );
  }
  if (criticalProperties.some((dangerous) => normalized === dangerous.toLowerCase())) {
    throw new Error(
      `[SECURITY] Invalid ${optionName}: "${propertyName}" is a reserved JavaScript keyword that could cause prototype pollution`
    );
  }
}
function normalizeProcessEntities(value, htmlEntities) {
  if (typeof value === "boolean") {
    return {
      enabled: value,
      // true or false
      maxEntitySize: 1e4,
      maxExpansionDepth: 1e4,
      maxTotalExpansions: Infinity,
      maxExpandedLength: 1e5,
      maxEntityCount: 1e3,
      allowedTags: null,
      tagFilter: null,
      appliesTo: "all"
    };
  }
  if (typeof value === "object" && value !== null) {
    return {
      enabled: value.enabled !== false,
      maxEntitySize: Math.max(1, value.maxEntitySize ?? 1e4),
      maxExpansionDepth: Math.max(1, value.maxExpansionDepth ?? 1e4),
      maxTotalExpansions: Math.max(1, value.maxTotalExpansions ?? Infinity),
      maxExpandedLength: Math.max(1, value.maxExpandedLength ?? 1e5),
      maxEntityCount: Math.max(1, value.maxEntityCount ?? 1e3),
      allowedTags: value.allowedTags ?? null,
      tagFilter: value.tagFilter ?? null,
      appliesTo: value.appliesTo ?? "all"
    };
  }
  return normalizeProcessEntities(true);
}
var buildOptions = function(options) {
  const built = Object.assign({}, defaultOptions2, options);
  const propertyNameOptions = [
    { value: built.attributeNamePrefix, name: "attributeNamePrefix" },
    { value: built.attributesGroupName, name: "attributesGroupName" },
    { value: built.textNodeName, name: "textNodeName" },
    { value: built.cdataPropName, name: "cdataPropName" },
    { value: built.commentPropName, name: "commentPropName" }
  ];
  for (const { value, name } of propertyNameOptions) {
    if (value) {
      validatePropertyName(value, name);
    }
  }
  if (built.onDangerousProperty === null) {
    built.onDangerousProperty = defaultOnDangerousProperty;
  }
  built.processEntities = normalizeProcessEntities(built.processEntities, built.htmlEntities);
  built.unpairedTagsSet = new Set(built.unpairedTags);
  if (built.stopNodes && Array.isArray(built.stopNodes)) {
    built.stopNodes = built.stopNodes.map((node) => {
      if (typeof node === "string" && node.startsWith("*.")) {
        return ".." + node.substring(2);
      }
      return node;
    });
  }
  return built;
};
var METADATA_SYMBOL;
if (typeof Symbol !== "function") {
  METADATA_SYMBOL = "@@xmlMetadata";
} else {
  METADATA_SYMBOL = /* @__PURE__ */ Symbol("XML Node Metadata");
}
var XmlNode = class {
  constructor(tagname) {
    this.tagname = tagname;
    this.child = [];
    this[":@"] = /* @__PURE__ */ Object.create(null);
  }
  add(key, val2) {
    if (key === "__proto__") key = "#__proto__";
    this.child.push({ [key]: val2 });
  }
  addChild(node, startIndex) {
    if (node.tagname === "__proto__") node.tagname = "#__proto__";
    if (node[":@"] && Object.keys(node[":@"]).length > 0) {
      this.child.push({ [node.tagname]: node.child, [":@"]: node[":@"] });
    } else {
      this.child.push({ [node.tagname]: node.child });
    }
    if (startIndex !== void 0) {
      this.child[this.child.length - 1][METADATA_SYMBOL] = { startIndex };
    }
  }
  /** symbol used for metadata */
  static getMetaDataSymbol() {
    return METADATA_SYMBOL;
  }
};
var nameStartChar10 = ":A-Za-z_\xC0-\xD6\xD8-\xF6\xF8-\u02FF\u0370-\u037D\u037F-\u0486\u0488-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD";
var nameChar10 = nameStartChar10 + "\\-\\.\\d\xB7\u0300-\u036F\u203F-\u2040";
var nameStartChar11 = ":A-Za-z_\xC0-\u02FF\u0370-\u037D\u037F-\u0486\u0488-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD\u{10000}-\u{EFFFF}";
var nameChar11 = nameStartChar11 + "\\-\\.\\d\xB7\u0300-\u036F\u0487\u203F-\u2040";
var buildRegexes = (startChar, char, flags = "") => {
  const ncStart = startChar.replace(":", "");
  const ncChar = char.replace(":", "");
  const ncNamePat = `[${ncStart}][${ncChar}]*`;
  return {
    name: new RegExp(`^[${startChar}][${char}]*$`, flags),
    ncName: new RegExp(`^${ncNamePat}$`, flags),
    qName: new RegExp(`^${ncNamePat}(?::${ncNamePat})?$`, flags),
    nmToken: new RegExp(`^[${char}]+$`, flags),
    nmTokens: new RegExp(`^[${char}]+(?:\\s+[${char}]+)*$`, flags)
  };
};
var regexes10 = buildRegexes(nameStartChar10, nameChar10);
var regexes11 = buildRegexes(nameStartChar11, nameChar11, "u");
var getRegexes = (xmlVersion = "1.0") => xmlVersion === "1.1" ? regexes11 : regexes10;
var qName = (str, { xmlVersion = "1.0" } = {}) => getRegexes(xmlVersion).qName.test(str);
var DocTypeReader = class {
  constructor(options, xmlVersion) {
    this.suppressValidationErr = !options;
    this.options = options;
    this.xmlVersion = xmlVersion || 1;
  }
  setXmlVersion(xmlVersion = 1) {
    this.xmlVersion = xmlVersion;
  }
  readDocType(xmlData, i) {
    const entities = /* @__PURE__ */ Object.create(null);
    let entityCount = 0;
    if (xmlData[i + 3] === "O" && xmlData[i + 4] === "C" && xmlData[i + 5] === "T" && xmlData[i + 6] === "Y" && xmlData[i + 7] === "P" && xmlData[i + 8] === "E") {
      i = i + 9;
      let angleBracketsCount = 1;
      let hasBody = false, comment = false;
      let exp = "";
      for (; i < xmlData.length; i++) {
        if (xmlData[i] === "<" && !comment) {
          if (hasBody && hasSeq(xmlData, "!ENTITY", i)) {
            i += 7;
            let entityName, val2;
            [entityName, val2, i] = this.readEntityExp(xmlData, i + 1, this.suppressValidationErr);
            if (val2.indexOf("&") === -1) {
              if (this.options.enabled !== false && this.options.maxEntityCount != null && entityCount >= this.options.maxEntityCount) {
                throw new Error(
                  `Entity count (${entityCount + 1}) exceeds maximum allowed (${this.options.maxEntityCount})`
                );
              }
              entities[entityName] = val2;
              entityCount++;
            }
          } else if (hasBody && hasSeq(xmlData, "!ELEMENT", i)) {
            i += 8;
            const { index } = this.readElementExp(xmlData, i + 1);
            i = index;
          } else if (hasBody && hasSeq(xmlData, "!ATTLIST", i)) {
            i += 8;
          } else if (hasBody && hasSeq(xmlData, "!NOTATION", i)) {
            i += 9;
            const { index } = this.readNotationExp(xmlData, i + 1, this.suppressValidationErr);
            i = index;
          } else if (hasSeq(xmlData, "!--", i)) comment = true;
          else throw new Error(`Invalid DOCTYPE`);
          angleBracketsCount++;
          exp = "";
        } else if (xmlData[i] === ">") {
          if (comment) {
            if (xmlData[i - 1] === "-" && xmlData[i - 2] === "-") {
              comment = false;
              angleBracketsCount--;
            }
          } else {
            angleBracketsCount--;
          }
          if (angleBracketsCount === 0) {
            break;
          }
        } else if (xmlData[i] === "[") {
          hasBody = true;
        } else {
          exp += xmlData[i];
        }
      }
      if (angleBracketsCount !== 0) {
        throw new Error(`Unclosed DOCTYPE`);
      }
    } else {
      throw new Error(`Invalid Tag instead of DOCTYPE`);
    }
    return { entities, i };
  }
  readEntityExp(xmlData, i) {
    i = skipWhitespace(xmlData, i);
    const startIndex = i;
    while (i < xmlData.length && !/\s/.test(xmlData[i]) && xmlData[i] !== '"' && xmlData[i] !== "'") {
      i++;
    }
    let entityName = xmlData.substring(startIndex, i);
    validateEntityName2(entityName, { xmlVersion: this.xmlVersion });
    i = skipWhitespace(xmlData, i);
    if (!this.suppressValidationErr) {
      if (xmlData.substring(i, i + 6).toUpperCase() === "SYSTEM") {
        throw new Error("External entities are not supported");
      } else if (xmlData[i] === "%") {
        throw new Error("Parameter entities are not supported");
      }
    }
    let entityValue = "";
    [i, entityValue] = this.readIdentifierVal(xmlData, i, "entity");
    if (this.options.enabled !== false && this.options.maxEntitySize != null && entityValue.length > this.options.maxEntitySize) {
      throw new Error(
        `Entity "${entityName}" size (${entityValue.length}) exceeds maximum allowed size (${this.options.maxEntitySize})`
      );
    }
    i--;
    return [entityName, entityValue, i];
  }
  readNotationExp(xmlData, i) {
    i = skipWhitespace(xmlData, i);
    const startIndex = i;
    while (i < xmlData.length && !/\s/.test(xmlData[i])) {
      i++;
    }
    let notationName = xmlData.substring(startIndex, i);
    !this.suppressValidationErr && validateEntityName2(notationName, { xmlVersion: this.xmlVersion });
    i = skipWhitespace(xmlData, i);
    const identifierType = xmlData.substring(i, i + 6).toUpperCase();
    if (!this.suppressValidationErr && identifierType !== "SYSTEM" && identifierType !== "PUBLIC") {
      throw new Error(`Expected SYSTEM or PUBLIC, found "${identifierType}"`);
    }
    i += identifierType.length;
    i = skipWhitespace(xmlData, i);
    let publicIdentifier = null;
    let systemIdentifier = null;
    if (identifierType === "PUBLIC") {
      [i, publicIdentifier] = this.readIdentifierVal(xmlData, i, "publicIdentifier");
      i = skipWhitespace(xmlData, i);
      if (xmlData[i] === '"' || xmlData[i] === "'") {
        [i, systemIdentifier] = this.readIdentifierVal(xmlData, i, "systemIdentifier");
      }
    } else if (identifierType === "SYSTEM") {
      [i, systemIdentifier] = this.readIdentifierVal(xmlData, i, "systemIdentifier");
      if (!this.suppressValidationErr && !systemIdentifier) {
        throw new Error("Missing mandatory system identifier for SYSTEM notation");
      }
    }
    return { notationName, publicIdentifier, systemIdentifier, index: --i };
  }
  readIdentifierVal(xmlData, i, type) {
    let identifierVal = "";
    const startChar = xmlData[i];
    if (startChar !== '"' && startChar !== "'") {
      throw new Error(`Expected quoted string, found "${startChar}"`);
    }
    i++;
    const startIndex = i;
    while (i < xmlData.length && xmlData[i] !== startChar) {
      i++;
    }
    identifierVal = xmlData.substring(startIndex, i);
    if (xmlData[i] !== startChar) {
      throw new Error(`Unterminated ${type} value`);
    }
    i++;
    return [i, identifierVal];
  }
  readElementExp(xmlData, i) {
    i = skipWhitespace(xmlData, i);
    const startIndex = i;
    while (i < xmlData.length && !/\s/.test(xmlData[i])) {
      i++;
    }
    let elementName = xmlData.substring(startIndex, i);
    if (!this.suppressValidationErr && !qName(elementName, { xmlVersion: this.xmlVersion })) {
      throw new Error(`Invalid element name: "${elementName}"`);
    }
    i = skipWhitespace(xmlData, i);
    let contentModel = "";
    if (xmlData[i] === "E" && hasSeq(xmlData, "MPTY", i)) i += 4;
    else if (xmlData[i] === "A" && hasSeq(xmlData, "NY", i)) i += 2;
    else if (xmlData[i] === "(") {
      i++;
      const startIndex2 = i;
      while (i < xmlData.length && xmlData[i] !== ")") {
        i++;
      }
      contentModel = xmlData.substring(startIndex2, i);
      if (xmlData[i] !== ")") {
        throw new Error("Unterminated content model");
      }
    } else if (!this.suppressValidationErr) {
      throw new Error(`Invalid Element Expression, found "${xmlData[i]}"`);
    }
    return {
      elementName,
      contentModel: contentModel.trim(),
      index: i
    };
  }
  readAttlistExp(xmlData, i) {
    i = skipWhitespace(xmlData, i);
    let startIndex = i;
    while (i < xmlData.length && !/\s/.test(xmlData[i])) {
      i++;
    }
    let elementName = xmlData.substring(startIndex, i);
    validateEntityName2(elementName, { xmlVersion: this.xmlVersion });
    i = skipWhitespace(xmlData, i);
    startIndex = i;
    while (i < xmlData.length && !/\s/.test(xmlData[i])) {
      i++;
    }
    let attributeName = xmlData.substring(startIndex, i);
    if (!validateEntityName2(attributeName, { xmlVersion: this.xmlVersion })) {
      throw new Error(`Invalid attribute name: "${attributeName}"`);
    }
    i = skipWhitespace(xmlData, i);
    let attributeType = "";
    if (xmlData.substring(i, i + 8).toUpperCase() === "NOTATION") {
      attributeType = "NOTATION";
      i += 8;
      i = skipWhitespace(xmlData, i);
      if (xmlData[i] !== "(") {
        throw new Error(`Expected '(', found "${xmlData[i]}"`);
      }
      i++;
      let allowedNotations = [];
      while (i < xmlData.length && xmlData[i] !== ")") {
        const startIndex2 = i;
        while (i < xmlData.length && xmlData[i] !== "|" && xmlData[i] !== ")") {
          i++;
        }
        let notation = xmlData.substring(startIndex2, i);
        notation = notation.trim();
        if (!validateEntityName2(notation, { xmlVersion: this.xmlVersion })) {
          throw new Error(`Invalid notation name: "${notation}"`);
        }
        allowedNotations.push(notation);
        if (xmlData[i] === "|") {
          i++;
          i = skipWhitespace(xmlData, i);
        }
      }
      if (xmlData[i] !== ")") {
        throw new Error("Unterminated list of notations");
      }
      i++;
      attributeType += " (" + allowedNotations.join("|") + ")";
    } else {
      const startIndex2 = i;
      while (i < xmlData.length && !/\s/.test(xmlData[i])) {
        i++;
      }
      attributeType += xmlData.substring(startIndex2, i);
      const validTypes = ["CDATA", "ID", "IDREF", "IDREFS", "ENTITY", "ENTITIES", "NMTOKEN", "NMTOKENS"];
      if (!this.suppressValidationErr && !validTypes.includes(attributeType.toUpperCase())) {
        throw new Error(`Invalid attribute type: "${attributeType}"`);
      }
    }
    i = skipWhitespace(xmlData, i);
    let defaultValue = "";
    if (xmlData.substring(i, i + 8).toUpperCase() === "#REQUIRED") {
      defaultValue = "#REQUIRED";
      i += 8;
    } else if (xmlData.substring(i, i + 7).toUpperCase() === "#IMPLIED") {
      defaultValue = "#IMPLIED";
      i += 7;
    } else {
      [i, defaultValue] = this.readIdentifierVal(xmlData, i, "ATTLIST");
    }
    return {
      elementName,
      attributeName,
      attributeType,
      defaultValue,
      index: i
    };
  }
};
var skipWhitespace = (data, index) => {
  while (index < data.length && /\s/.test(data[index])) {
    index++;
  }
  return index;
};
function hasSeq(data, seq, i) {
  for (let j = 0; j < seq.length; j++) {
    if (seq[j] !== data[i + j + 1]) return false;
  }
  return true;
}
function validateEntityName2(name, xmlVersion) {
  if (qName(name, { xmlVersion }))
    return name;
  else
    throw new Error(`Invalid entity name ${name}`);
}
var hexRegex = /^[-+]?0x[a-fA-F0-9]+$/;
var binRegex = /^0b[01]+$/;
var octRegex = /^0o[0-7]+$/;
var numRegex = /^([\-\+])?(0*)([0-9]*(\.[0-9]*)?)$/;
var consider = {
  hex: true,
  binary: false,
  octal: false,
  leadingZeros: true,
  decimalPoint: ".",
  eNotation: true,
  //skipLike: /regex/,
  infinity: "original"
  // "null", "infinity" (Infinity type), "string" ("Infinity" (the string literal))
};
function toNumber(str, options = {}) {
  options = Object.assign({}, consider, options);
  if (!str || typeof str !== "string") return str;
  let trimmedStr = str.trim();
  if (trimmedStr.length === 0) return str;
  else if (options.skipLike !== void 0 && options.skipLike.test(trimmedStr)) return str;
  else if (trimmedStr === "0") return 0;
  else if (options.hex && hexRegex.test(trimmedStr)) {
    return parse_int(trimmedStr, 16);
  } else if (options.binary && binRegex.test(trimmedStr)) {
    return parse_int(trimmedStr, 2);
  } else if (options.octal && octRegex.test(trimmedStr)) {
    return parse_int(trimmedStr, 8);
  } else if (!isFinite(trimmedStr)) {
    return handleInfinity(str, Number(trimmedStr), options);
  } else if (trimmedStr.includes("e") || trimmedStr.includes("E")) {
    return resolveEnotation(str, trimmedStr, options);
  } else {
    const match = numRegex.exec(trimmedStr);
    if (match) {
      const sign = match[1] || "";
      const leadingZeros = match[2];
      let numTrimmedByZeros = trimZeros(match[3]);
      const decimalAdjacentToLeadingZeros = sign ? (
        // 0., -00., 000.
        str[leadingZeros.length + 1] === "."
      ) : str[leadingZeros.length] === ".";
      if (!options.leadingZeros && (leadingZeros.length > 1 || leadingZeros.length === 1 && !decimalAdjacentToLeadingZeros)) {
        return str;
      } else {
        const num = Number(trimmedStr);
        const parsedStr = String(num);
        if (num === 0) return num;
        if (parsedStr.search(/[eE]/) !== -1) {
          if (options.eNotation) return num;
          else return str;
        } else if (trimmedStr.indexOf(".") !== -1) {
          if (parsedStr === "0") return num;
          else if (parsedStr === numTrimmedByZeros) return num;
          else if (parsedStr === `${sign}${numTrimmedByZeros}`) return num;
          else return str;
        }
        let n = leadingZeros ? numTrimmedByZeros : trimmedStr;
        if (leadingZeros) {
          return n === parsedStr || sign + n === parsedStr ? num : str;
        } else {
          return n === parsedStr || n === sign + parsedStr ? num : str;
        }
      }
    } else {
      return str;
    }
  }
}
var eNotationRegx = /^([-+])?(0*)(\d*(\.\d*)?[eE][-\+]?\d+)$/;
function resolveEnotation(str, trimmedStr, options) {
  if (!options.eNotation) return str;
  const notation = trimmedStr.match(eNotationRegx);
  if (notation) {
    let sign = notation[1] || "";
    const eChar = notation[3].indexOf("e") === -1 ? "E" : "e";
    const leadingZeros = notation[2];
    const eAdjacentToLeadingZeros = sign ? (
      // 0E.
      str[leadingZeros.length + 1] === eChar
    ) : str[leadingZeros.length] === eChar;
    if (leadingZeros.length > 1 && eAdjacentToLeadingZeros) return str;
    else if (leadingZeros.length === 1 && (notation[3].startsWith(`.${eChar}`) || notation[3][0] === eChar)) {
      return Number(trimmedStr);
    } else if (leadingZeros.length > 0) {
      if (options.leadingZeros && !eAdjacentToLeadingZeros) {
        trimmedStr = (notation[1] || "") + notation[3];
        return Number(trimmedStr);
      } else return str;
    } else {
      return Number(trimmedStr);
    }
  } else {
    return str;
  }
}
function trimZeros(numStr) {
  if (numStr && numStr.indexOf(".") !== -1) {
    numStr = numStr.replace(/0+$/, "");
    if (numStr === ".") numStr = "0";
    else if (numStr[0] === ".") numStr = "0" + numStr;
    else if (numStr[numStr.length - 1] === ".") numStr = numStr.substring(0, numStr.length - 1);
    return numStr;
  }
  return numStr;
}
function parse_int(numStr, base) {
  const str = numStr.trim();
  if (base === 2 || base === 8) numStr = str.substring(2);
  if (parseInt) return parseInt(numStr, base);
  else if (Number.parseInt) return Number.parseInt(numStr, base);
  else if (window && window.parseInt) return window.parseInt(numStr, base);
  else throw new Error("parseInt, Number.parseInt, window.parseInt are not supported");
}
function handleInfinity(str, num, options) {
  const isPositive = num === Infinity;
  switch (options.infinity.toLowerCase()) {
    case "null":
      return null;
    case "infinity":
      return num;
    // Return Infinity or -Infinity
    case "string":
      return isPositive ? "Infinity" : "-Infinity";
    case "original":
    default:
      return str;
  }
}
function getIgnoreAttributesFn(ignoreAttributes) {
  if (typeof ignoreAttributes === "function") {
    return ignoreAttributes;
  }
  if (Array.isArray(ignoreAttributes)) {
    return (attrName) => {
      for (const pattern of ignoreAttributes) {
        if (typeof pattern === "string" && attrName === pattern) {
          return true;
        }
        if (pattern instanceof RegExp && pattern.test(attrName)) {
          return true;
        }
      }
    };
  }
  return () => false;
}
var Expression = class {
  /**
   * Create a new Expression
   * @param {string} pattern - Pattern string (e.g., "root.users.user", "..user[id]")
   * @param {Object} options - Configuration options
   * @param {string} options.separator - Path separator (default: '.')
   */
  constructor(pattern, options = {}, data) {
    this.pattern = pattern;
    this.separator = options.separator || ".";
    this.segments = this._parse(pattern);
    this.data = data;
    this._hasDeepWildcard = this.segments.some((seg) => seg.type === "deep-wildcard");
    this._hasAttributeCondition = this.segments.some((seg) => seg.attrName !== void 0);
    this._hasPositionSelector = this.segments.some((seg) => seg.position !== void 0);
  }
  /**
   * Parse pattern string into segments
   * @private
   * @param {string} pattern - Pattern to parse
   * @returns {Array} Array of segment objects
   */
  _parse(pattern) {
    const segments = [];
    let i = 0;
    let currentPart = "";
    while (i < pattern.length) {
      if (pattern[i] === this.separator) {
        if (i + 1 < pattern.length && pattern[i + 1] === this.separator) {
          if (currentPart.trim()) {
            segments.push(this._parseSegment(currentPart.trim()));
            currentPart = "";
          }
          segments.push({ type: "deep-wildcard" });
          i += 2;
        } else {
          if (currentPart.trim()) {
            segments.push(this._parseSegment(currentPart.trim()));
          }
          currentPart = "";
          i++;
        }
      } else {
        currentPart += pattern[i];
        i++;
      }
    }
    if (currentPart.trim()) {
      segments.push(this._parseSegment(currentPart.trim()));
    }
    return segments;
  }
  /**
   * Parse a single segment
   * @private
   * @param {string} part - Segment string (e.g., "user", "ns::user", "user[id]", "ns::user:first")
   * @returns {Object} Segment object
   */
  _parseSegment(part) {
    const segment = { type: "tag" };
    let bracketContent = null;
    let withoutBrackets = part;
    const bracketMatch = part.match(/^([^\[]+)(\[[^\]]*\])(.*)$/);
    if (bracketMatch) {
      withoutBrackets = bracketMatch[1] + bracketMatch[3];
      if (bracketMatch[2]) {
        const content = bracketMatch[2].slice(1, -1);
        if (content) {
          bracketContent = content;
        }
      }
    }
    let namespace = void 0;
    let tagAndPosition = withoutBrackets;
    if (withoutBrackets.includes("::")) {
      const nsIndex = withoutBrackets.indexOf("::");
      namespace = withoutBrackets.substring(0, nsIndex).trim();
      tagAndPosition = withoutBrackets.substring(nsIndex + 2).trim();
      if (!namespace) {
        throw new Error(`Invalid namespace in pattern: ${part}`);
      }
    }
    let tag = void 0;
    let positionMatch = null;
    if (tagAndPosition.includes(":")) {
      const colonIndex = tagAndPosition.lastIndexOf(":");
      const tagPart = tagAndPosition.substring(0, colonIndex).trim();
      const posPart = tagAndPosition.substring(colonIndex + 1).trim();
      const isPositionKeyword = ["first", "last", "odd", "even"].includes(posPart) || /^nth\(\d+\)$/.test(posPart);
      if (isPositionKeyword) {
        tag = tagPart;
        positionMatch = posPart;
      } else {
        tag = tagAndPosition;
      }
    } else {
      tag = tagAndPosition;
    }
    if (!tag) {
      throw new Error(`Invalid segment pattern: ${part}`);
    }
    segment.tag = tag;
    if (namespace) {
      segment.namespace = namespace;
    }
    if (bracketContent) {
      if (bracketContent.includes("=")) {
        const eqIndex = bracketContent.indexOf("=");
        segment.attrName = bracketContent.substring(0, eqIndex).trim();
        segment.attrValue = bracketContent.substring(eqIndex + 1).trim();
      } else {
        segment.attrName = bracketContent.trim();
      }
    }
    if (positionMatch) {
      const nthMatch = positionMatch.match(/^nth\((\d+)\)$/);
      if (nthMatch) {
        segment.position = "nth";
        segment.positionValue = parseInt(nthMatch[1], 10);
      } else {
        segment.position = positionMatch;
      }
    }
    return segment;
  }
  /**
   * Get the number of segments
   * @returns {number}
   */
  get length() {
    return this.segments.length;
  }
  /**
   * Check if expression contains deep wildcard
   * @returns {boolean}
   */
  hasDeepWildcard() {
    return this._hasDeepWildcard;
  }
  /**
   * Check if expression has attribute conditions
   * @returns {boolean}
   */
  hasAttributeCondition() {
    return this._hasAttributeCondition;
  }
  /**
   * Check if expression has position selectors
   * @returns {boolean}
   */
  hasPositionSelector() {
    return this._hasPositionSelector;
  }
  /**
   * Get string representation
   * @returns {string}
   */
  toString() {
    return this.pattern;
  }
};
var ExpressionSet = class {
  constructor() {
    this._byDepthAndTag = /* @__PURE__ */ new Map();
    this._wildcardByDepth = /* @__PURE__ */ new Map();
    this._deepWildcards = [];
    this._patterns = /* @__PURE__ */ new Set();
    this._sealed = false;
  }
  /**
   * Add an Expression to the set.
   * Duplicate patterns (same pattern string) are silently ignored.
   *
   * @param {import('./Expression.js').default} expression - A pre-constructed Expression instance
   * @returns {this} for chaining
   * @throws {TypeError} if called after seal()
   *
   * @example
   * set.add(new Expression('root.users.user'));
   * set.add(new Expression('..script'));
   */
  add(expression) {
    if (this._sealed) {
      throw new TypeError(
        "ExpressionSet is sealed. Create a new ExpressionSet to add more expressions."
      );
    }
    if (this._patterns.has(expression.pattern)) return this;
    this._patterns.add(expression.pattern);
    if (expression.hasDeepWildcard()) {
      this._deepWildcards.push(expression);
      return this;
    }
    const depth = expression.length;
    const lastSeg = expression.segments[expression.segments.length - 1];
    const tag = lastSeg?.tag;
    if (!tag || tag === "*") {
      if (!this._wildcardByDepth.has(depth)) this._wildcardByDepth.set(depth, []);
      this._wildcardByDepth.get(depth).push(expression);
    } else {
      const key = `${depth}:${tag}`;
      if (!this._byDepthAndTag.has(key)) this._byDepthAndTag.set(key, []);
      this._byDepthAndTag.get(key).push(expression);
    }
    return this;
  }
  /**
   * Add multiple expressions at once.
   *
   * @param {import('./Expression.js').default[]} expressions - Array of Expression instances
   * @returns {this} for chaining
   *
   * @example
   * set.addAll([
   *   new Expression('root.users.user'),
   *   new Expression('root.config.setting'),
   * ]);
   */
  addAll(expressions) {
    for (const expr of expressions) this.add(expr);
    return this;
  }
  /**
   * Check whether a pattern string is already present in the set.
   *
   * @param {import('./Expression.js').default} expression
   * @returns {boolean}
   */
  has(expression) {
    return this._patterns.has(expression.pattern);
  }
  /**
   * Number of expressions in the set.
   * @type {number}
   */
  get size() {
    return this._patterns.size;
  }
  /**
   * Seal the set against further modifications.
   * Useful to prevent accidental mutations after config is built.
   * Calling add() or addAll() on a sealed set throws a TypeError.
   *
   * @returns {this}
   */
  seal() {
    this._sealed = true;
    return this;
  }
  /**
   * Whether the set has been sealed.
   * @type {boolean}
   */
  get isSealed() {
    return this._sealed;
  }
  /**
   * Test whether the matcher's current path matches any expression in the set.
   *
   * Evaluation order (cheapest → most expensive):
   *  1. Exact depth + tag bucket  — O(1) lookup, typically 0–2 expressions
   *  2. Depth-only wildcard bucket — O(1) lookup, rare
   *  3. Deep-wildcard list         — always checked, but usually small
   *
   * @param {import('./Matcher.js').default} matcher - Matcher instance (or readOnly view)
   * @returns {boolean} true if any expression matches the current path
   *
   * @example
   * if (stopNodes.matchesAny(matcher)) {
   *   // handle stop node
   * }
   */
  matchesAny(matcher) {
    return this.findMatch(matcher) !== null;
  }
  /**
  * Find and return the first Expression that matches the matcher's current path.
  *
  * Uses the same evaluation order as matchesAny (cheapest → most expensive):
  *  1. Exact depth + tag bucket
  *  2. Depth-only wildcard bucket
  *  3. Deep-wildcard list
  *
  * @param {import('./Matcher.js').default} matcher - Matcher instance (or readOnly view)
  * @returns {import('./Expression.js').default | null} the first matching Expression, or null
  *
  * @example
  * const expr = stopNodes.findMatch(matcher);
  * if (expr) {
  *   // access expr.config, expr.pattern, etc.
  * }
  */
  findMatch(matcher) {
    const depth = matcher.getDepth();
    const tag = matcher.getCurrentTag();
    const exactKey = `${depth}:${tag}`;
    const exactBucket = this._byDepthAndTag.get(exactKey);
    if (exactBucket) {
      for (let i = 0; i < exactBucket.length; i++) {
        if (matcher.matches(exactBucket[i])) return exactBucket[i];
      }
    }
    const wildcardBucket = this._wildcardByDepth.get(depth);
    if (wildcardBucket) {
      for (let i = 0; i < wildcardBucket.length; i++) {
        if (matcher.matches(wildcardBucket[i])) return wildcardBucket[i];
      }
    }
    for (let i = 0; i < this._deepWildcards.length; i++) {
      if (matcher.matches(this._deepWildcards[i])) return this._deepWildcards[i];
    }
    return null;
  }
};
var MatcherView = class {
  /**
   * @param {Matcher} matcher - The parent Matcher instance to read from.
   */
  constructor(matcher) {
    this._matcher = matcher;
  }
  /**
   * Get the path separator used by the parent matcher.
   * @returns {string}
   */
  get separator() {
    return this._matcher.separator;
  }
  /**
   * Get current tag name.
   * @returns {string|undefined}
   */
  getCurrentTag() {
    const path = this._matcher.path;
    return path.length > 0 ? path[path.length - 1].tag : void 0;
  }
  /**
   * Get current namespace.
   * @returns {string|undefined}
   */
  getCurrentNamespace() {
    const path = this._matcher.path;
    return path.length > 0 ? path[path.length - 1].namespace : void 0;
  }
  /**
   * Get current node's attribute value.
   * @param {string} attrName
   * @returns {*}
   */
  getAttrValue(attrName) {
    const path = this._matcher.path;
    if (path.length === 0) return void 0;
    return path[path.length - 1].values?.[attrName];
  }
  /**
   * Check if current node has an attribute.
   * @param {string} attrName
   * @returns {boolean}
   */
  hasAttr(attrName) {
    const path = this._matcher.path;
    if (path.length === 0) return false;
    const current = path[path.length - 1];
    return current.values !== void 0 && attrName in current.values;
  }
  /**
   * Get current node's sibling position (child index in parent).
   * @returns {number}
   */
  getPosition() {
    const path = this._matcher.path;
    if (path.length === 0) return -1;
    return path[path.length - 1].position ?? 0;
  }
  /**
   * Get current node's repeat counter (occurrence count of this tag name).
   * @returns {number}
   */
  getCounter() {
    const path = this._matcher.path;
    if (path.length === 0) return -1;
    return path[path.length - 1].counter ?? 0;
  }
  /**
   * Get current node's sibling index (alias for getPosition).
   * @returns {number}
   * @deprecated Use getPosition() or getCounter() instead
   */
  getIndex() {
    return this.getPosition();
  }
  /**
   * Get current path depth.
   * @returns {number}
   */
  getDepth() {
    return this._matcher.path.length;
  }
  /**
   * Get path as string.
   * @param {string} [separator] - Optional separator (uses default if not provided)
   * @param {boolean} [includeNamespace=true]
   * @returns {string}
   */
  toString(separator, includeNamespace = true) {
    return this._matcher.toString(separator, includeNamespace);
  }
  /**
   * Get path as array of tag names.
   * @returns {string[]}
   */
  toArray() {
    return this._matcher.path.map((n) => n.tag);
  }
  /**
   * Match current path against an Expression.
   * @param {Expression} expression
   * @returns {boolean}
   */
  matches(expression) {
    return this._matcher.matches(expression);
  }
  /**
   * Match any expression in the given set against the current path.
   * @param {ExpressionSet} exprSet
   * @returns {boolean}
   */
  matchesAny(exprSet) {
    return exprSet.matchesAny(this._matcher);
  }
};
var Matcher = class {
  /**
   * Create a new Matcher.
   * @param {Object} [options={}]
   * @param {string} [options.separator='.'] - Default path separator
   */
  constructor(options = {}) {
    this.separator = options.separator || ".";
    this.path = [];
    this.siblingStacks = [];
    this._pathStringCache = null;
    this._view = new MatcherView(this);
  }
  /**
   * Push a new tag onto the path.
   * @param {string} tagName
   * @param {Object|null} [attrValues=null]
   * @param {string|null} [namespace=null]
   */
  push(tagName, attrValues = null, namespace = null) {
    this._pathStringCache = null;
    if (this.path.length > 0) {
      this.path[this.path.length - 1].values = void 0;
    }
    const currentLevel = this.path.length;
    if (!this.siblingStacks[currentLevel]) {
      this.siblingStacks[currentLevel] = /* @__PURE__ */ new Map();
    }
    const siblings = this.siblingStacks[currentLevel];
    const siblingKey = namespace ? `${namespace}:${tagName}` : tagName;
    const counter = siblings.get(siblingKey) || 0;
    let position = 0;
    for (const count of siblings.values()) {
      position += count;
    }
    siblings.set(siblingKey, counter + 1);
    const node = {
      tag: tagName,
      position,
      counter
    };
    if (namespace !== null && namespace !== void 0) {
      node.namespace = namespace;
    }
    if (attrValues !== null && attrValues !== void 0) {
      node.values = attrValues;
    }
    this.path.push(node);
  }
  /**
   * Pop the last tag from the path.
   * @returns {Object|undefined} The popped node
   */
  pop() {
    if (this.path.length === 0) return void 0;
    this._pathStringCache = null;
    const node = this.path.pop();
    if (this.siblingStacks.length > this.path.length + 1) {
      this.siblingStacks.length = this.path.length + 1;
    }
    return node;
  }
  /**
   * Update current node's attribute values.
   * Useful when attributes are parsed after push.
   * @param {Object} attrValues
   */
  updateCurrent(attrValues) {
    if (this.path.length > 0) {
      const current = this.path[this.path.length - 1];
      if (attrValues !== null && attrValues !== void 0) {
        current.values = attrValues;
      }
    }
  }
  /**
   * Get current tag name.
   * @returns {string|undefined}
   */
  getCurrentTag() {
    return this.path.length > 0 ? this.path[this.path.length - 1].tag : void 0;
  }
  /**
   * Get current namespace.
   * @returns {string|undefined}
   */
  getCurrentNamespace() {
    return this.path.length > 0 ? this.path[this.path.length - 1].namespace : void 0;
  }
  /**
   * Get current node's attribute value.
   * @param {string} attrName
   * @returns {*}
   */
  getAttrValue(attrName) {
    if (this.path.length === 0) return void 0;
    return this.path[this.path.length - 1].values?.[attrName];
  }
  /**
   * Check if current node has an attribute.
   * @param {string} attrName
   * @returns {boolean}
   */
  hasAttr(attrName) {
    if (this.path.length === 0) return false;
    const current = this.path[this.path.length - 1];
    return current.values !== void 0 && attrName in current.values;
  }
  /**
   * Get current node's sibling position (child index in parent).
   * @returns {number}
   */
  getPosition() {
    if (this.path.length === 0) return -1;
    return this.path[this.path.length - 1].position ?? 0;
  }
  /**
   * Get current node's repeat counter (occurrence count of this tag name).
   * @returns {number}
   */
  getCounter() {
    if (this.path.length === 0) return -1;
    return this.path[this.path.length - 1].counter ?? 0;
  }
  /**
   * Get current node's sibling index (alias for getPosition).
   * @returns {number}
   * @deprecated Use getPosition() or getCounter() instead
   */
  getIndex() {
    return this.getPosition();
  }
  /**
   * Get current path depth.
   * @returns {number}
   */
  getDepth() {
    return this.path.length;
  }
  /**
   * Get path as string.
   * @param {string} [separator] - Optional separator (uses default if not provided)
   * @param {boolean} [includeNamespace=true]
   * @returns {string}
   */
  toString(separator, includeNamespace = true) {
    const sep = separator || this.separator;
    const isDefault = sep === this.separator && includeNamespace === true;
    if (isDefault) {
      if (this._pathStringCache !== null) {
        return this._pathStringCache;
      }
      const result = this.path.map(
        (n) => n.namespace ? `${n.namespace}:${n.tag}` : n.tag
      ).join(sep);
      this._pathStringCache = result;
      return result;
    }
    return this.path.map(
      (n) => includeNamespace && n.namespace ? `${n.namespace}:${n.tag}` : n.tag
    ).join(sep);
  }
  /**
   * Get path as array of tag names.
   * @returns {string[]}
   */
  toArray() {
    return this.path.map((n) => n.tag);
  }
  /**
   * Reset the path to empty.
   */
  reset() {
    this._pathStringCache = null;
    this.path = [];
    this.siblingStacks = [];
  }
  /**
   * Match current path against an Expression.
   * @param {Expression} expression
   * @returns {boolean}
   */
  matches(expression) {
    const segments = expression.segments;
    if (segments.length === 0) {
      return false;
    }
    if (expression.hasDeepWildcard()) {
      return this._matchWithDeepWildcard(segments);
    }
    return this._matchSimple(segments);
  }
  /**
   * @private
   */
  _matchSimple(segments) {
    if (this.path.length !== segments.length) {
      return false;
    }
    for (let i = 0; i < segments.length; i++) {
      if (!this._matchSegment(segments[i], this.path[i], i === this.path.length - 1)) {
        return false;
      }
    }
    return true;
  }
  /**
   * @private
   */
  _matchWithDeepWildcard(segments) {
    let pathIdx = this.path.length - 1;
    let segIdx = segments.length - 1;
    while (segIdx >= 0 && pathIdx >= 0) {
      const segment = segments[segIdx];
      if (segment.type === "deep-wildcard") {
        segIdx--;
        if (segIdx < 0) {
          return true;
        }
        const nextSeg = segments[segIdx];
        let found = false;
        for (let i = pathIdx; i >= 0; i--) {
          if (this._matchSegment(nextSeg, this.path[i], i === this.path.length - 1)) {
            pathIdx = i - 1;
            segIdx--;
            found = true;
            break;
          }
        }
        if (!found) {
          return false;
        }
      } else {
        if (!this._matchSegment(segment, this.path[pathIdx], pathIdx === this.path.length - 1)) {
          return false;
        }
        pathIdx--;
        segIdx--;
      }
    }
    return segIdx < 0;
  }
  /**
   * @private
   */
  _matchSegment(segment, node, isCurrentNode) {
    if (segment.tag !== "*" && segment.tag !== node.tag) {
      return false;
    }
    if (segment.namespace !== void 0) {
      if (segment.namespace !== "*" && segment.namespace !== node.namespace) {
        return false;
      }
    }
    if (segment.attrName !== void 0) {
      if (!isCurrentNode) {
        return false;
      }
      if (!node.values || !(segment.attrName in node.values)) {
        return false;
      }
      if (segment.attrValue !== void 0) {
        if (String(node.values[segment.attrName]) !== String(segment.attrValue)) {
          return false;
        }
      }
    }
    if (segment.position !== void 0) {
      if (!isCurrentNode) {
        return false;
      }
      const counter = node.counter ?? 0;
      if (segment.position === "first" && counter !== 0) {
        return false;
      } else if (segment.position === "odd" && counter % 2 !== 1) {
        return false;
      } else if (segment.position === "even" && counter % 2 !== 0) {
        return false;
      } else if (segment.position === "nth" && counter !== segment.positionValue) {
        return false;
      }
    }
    return true;
  }
  /**
   * Match any expression in the given set against the current path.
   * @param {ExpressionSet} exprSet
   * @returns {boolean}
   */
  matchesAny(exprSet) {
    return exprSet.matchesAny(this);
  }
  /**
   * Create a snapshot of current state.
   * @returns {Object}
   */
  snapshot() {
    return {
      path: this.path.map((node) => ({ ...node })),
      siblingStacks: this.siblingStacks.map((map) => new Map(map))
    };
  }
  /**
   * Restore state from snapshot.
   * @param {Object} snapshot
   */
  restore(snapshot) {
    this._pathStringCache = null;
    this.path = snapshot.path.map((node) => ({ ...node }));
    this.siblingStacks = snapshot.siblingStacks.map((map) => new Map(map));
  }
  /**
   * Return the read-only {@link MatcherView} for this matcher.
   *
   * The same instance is returned on every call — no allocation occurs.
   * It always reflects the current parser state and is safe to pass to
   * user callbacks without risk of accidental mutation.
   *
   * @returns {MatcherView}
   *
   * @example
   * const view = matcher.readOnly();
   * // pass view to callbacks — it stays in sync automatically
   * view.matches(expr);       // ✓
   * view.getCurrentTag();     // ✓
   * // view.push(...)         // ✗ method does not exist — caught by TypeScript
   */
  readOnly() {
    return this._view;
  }
};
function extractRawAttributes(prefixedAttrs, options) {
  if (!prefixedAttrs) return {};
  const attrs = options.attributesGroupName ? prefixedAttrs[options.attributesGroupName] : prefixedAttrs;
  if (!attrs) return {};
  const rawAttrs = {};
  for (const key in attrs) {
    if (key.startsWith(options.attributeNamePrefix)) {
      const rawName = key.substring(options.attributeNamePrefix.length);
      rawAttrs[rawName] = attrs[key];
    } else {
      rawAttrs[key] = attrs[key];
    }
  }
  return rawAttrs;
}
function extractNamespace(rawTagName) {
  if (!rawTagName || typeof rawTagName !== "string") return void 0;
  const colonIndex = rawTagName.indexOf(":");
  if (colonIndex !== -1 && colonIndex > 0) {
    const ns = rawTagName.substring(0, colonIndex);
    if (ns !== "xmlns") {
      return ns;
    }
  }
  return void 0;
}
var OrderedObjParser = class {
  constructor(options, externalEntities) {
    this.options = options;
    this.currentNode = null;
    this.tagsNodeStack = [];
    this.parseXml = parseXml;
    this.parseTextData = parseTextData;
    this.resolveNameSpace = resolveNameSpace;
    this.buildAttributesMap = buildAttributesMap;
    this.isItStopNode = isItStopNode;
    this.replaceEntitiesValue = replaceEntitiesValue;
    this.readStopNodeData = readStopNodeData;
    this.saveTextToParentTag = saveTextToParentTag;
    this.addChild = addChild;
    this.ignoreAttributesFn = getIgnoreAttributesFn(this.options.ignoreAttributes);
    this.entityExpansionCount = 0;
    this.currentExpandedLength = 0;
    let namedEntities = { ...XML };
    if (this.options.entityDecoder) {
      this.entityDecoder = this.options.entityDecoder;
    } else {
      if (typeof this.options.htmlEntities === "object") namedEntities = this.options.htmlEntities;
      else if (this.options.htmlEntities === true) namedEntities = { ...COMMON_HTML, ...CURRENCY };
      this.entityDecoder = new EntityDecoder({
        namedEntities: { ...namedEntities, ...externalEntities },
        numericAllowed: this.options.htmlEntities,
        limit: {
          maxTotalExpansions: this.options.processEntities.maxTotalExpansions,
          maxExpandedLength: this.options.processEntities.maxExpandedLength,
          applyLimitsTo: this.options.processEntities.appliesTo
        }
        //postCheck: resolved => resolved
      });
    }
    this.matcher = new Matcher();
    this.readonlyMatcher = this.matcher.readOnly();
    this.isCurrentNodeStopNode = false;
    this.stopNodeExpressionsSet = new ExpressionSet();
    const stopNodesOpts = this.options.stopNodes;
    if (stopNodesOpts && stopNodesOpts.length > 0) {
      for (let i = 0; i < stopNodesOpts.length; i++) {
        const stopNodeExp = stopNodesOpts[i];
        if (typeof stopNodeExp === "string") {
          this.stopNodeExpressionsSet.add(new Expression(stopNodeExp));
        } else if (stopNodeExp instanceof Expression) {
          this.stopNodeExpressionsSet.add(stopNodeExp);
        }
      }
      this.stopNodeExpressionsSet.seal();
    }
  }
};
function parseTextData(val2, tagName, jPath, dontTrim, hasAttributes, isLeafNode, escapeEntities) {
  const options = this.options;
  if (val2 !== void 0) {
    if (options.trimValues && !dontTrim) {
      val2 = val2.trim();
    }
    if (val2.length > 0) {
      if (!escapeEntities) val2 = this.replaceEntitiesValue(val2, tagName, jPath);
      const jPathOrMatcher = options.jPath ? jPath.toString() : jPath;
      const newval = options.tagValueProcessor(tagName, val2, jPathOrMatcher, hasAttributes, isLeafNode);
      if (newval === null || newval === void 0) {
        return val2;
      } else if (typeof newval !== typeof val2 || newval !== val2) {
        return newval;
      } else if (options.trimValues) {
        return parseValue(val2, options.parseTagValue, options.numberParseOptions);
      } else {
        const trimmedVal = val2.trim();
        if (trimmedVal === val2) {
          return parseValue(val2, options.parseTagValue, options.numberParseOptions);
        } else {
          return val2;
        }
      }
    }
  }
}
function resolveNameSpace(tagname) {
  if (this.options.removeNSPrefix) {
    const tags = tagname.split(":");
    const prefix = tagname.charAt(0) === "/" ? "/" : "";
    if (tags[0] === "xmlns") {
      return "";
    }
    if (tags.length === 2) {
      tagname = prefix + tags[1];
    }
  }
  return tagname;
}
var attrsRegx = new RegExp(`([^\\s=]+)\\s*(=\\s*(['"])([\\s\\S]*?)\\3)?`, "gm");
function buildAttributesMap(attrStr, jPath, tagName, force = false) {
  const options = this.options;
  if (force === true || options.ignoreAttributes !== true && typeof attrStr === "string") {
    const matches = getAllMatches(attrStr, attrsRegx);
    const len = matches.length;
    const attrs = {};
    const processedVals = new Array(len);
    let hasRawAttrs = false;
    const rawAttrsForMatcher = {};
    for (let i = 0; i < len; i++) {
      const attrName = this.resolveNameSpace(matches[i][1]);
      const oldVal = matches[i][4];
      if (attrName.length && oldVal !== void 0) {
        let val2 = oldVal;
        if (options.trimValues) val2 = val2.trim();
        val2 = this.replaceEntitiesValue(val2, tagName, this.readonlyMatcher);
        processedVals[i] = val2;
        rawAttrsForMatcher[attrName] = val2;
        hasRawAttrs = true;
      }
    }
    if (hasRawAttrs && typeof jPath === "object" && jPath.updateCurrent) {
      jPath.updateCurrent(rawAttrsForMatcher);
    }
    const jPathStr = options.jPath ? jPath.toString() : this.readonlyMatcher;
    let hasAttrs = false;
    for (let i = 0; i < len; i++) {
      const attrName = this.resolveNameSpace(matches[i][1]);
      if (this.ignoreAttributesFn(attrName, jPathStr)) continue;
      let aName = options.attributeNamePrefix + attrName;
      if (attrName.length) {
        if (options.transformAttributeName) {
          aName = options.transformAttributeName(aName);
        }
        aName = sanitizeName(aName, options);
        if (matches[i][4] !== void 0) {
          const oldVal = processedVals[i];
          const newVal = options.attributeValueProcessor(attrName, oldVal, jPathStr);
          if (newVal === null || newVal === void 0) {
            attrs[aName] = oldVal;
          } else if (typeof newVal !== typeof oldVal || newVal !== oldVal) {
            attrs[aName] = newVal;
          } else {
            attrs[aName] = parseValue(oldVal, options.parseAttributeValue, options.numberParseOptions);
          }
          hasAttrs = true;
        } else if (options.allowBooleanAttributes) {
          attrs[aName] = true;
          hasAttrs = true;
        }
      }
    }
    if (!hasAttrs) return;
    if (options.attributesGroupName && !options.preserveOrder) {
      const attrCollection = {};
      attrCollection[options.attributesGroupName] = attrs;
      return attrCollection;
    }
    return attrs;
  }
}
var parseXml = function(xmlData) {
  xmlData = xmlData.replace(/\r\n?/g, "\n");
  const xmlObj = new XmlNode("!xml");
  let currentNode = xmlObj;
  let textData = "";
  this.matcher.reset();
  this.entityDecoder.reset();
  this.entityExpansionCount = 0;
  this.currentExpandedLength = 0;
  const options = this.options;
  const docTypeReader = new DocTypeReader(options.processEntities);
  const xmlLen = xmlData.length;
  for (let i = 0; i < xmlLen; i++) {
    const ch = xmlData[i];
    if (ch === "<") {
      const c1 = xmlData.charCodeAt(i + 1);
      if (c1 === 47) {
        const closeIndex = findClosingIndex(xmlData, ">", i, "Closing Tag is not closed.");
        let tagName = xmlData.substring(i + 2, closeIndex).trim();
        if (options.removeNSPrefix) {
          const colonIndex = tagName.indexOf(":");
          if (colonIndex !== -1) {
            tagName = tagName.substr(colonIndex + 1);
          }
        }
        tagName = transformTagName(options.transformTagName, tagName, "", options).tagName;
        if (currentNode) {
          textData = this.saveTextToParentTag(textData, currentNode, this.readonlyMatcher);
        }
        const lastTagName = this.matcher.getCurrentTag();
        if (tagName && options.unpairedTagsSet.has(tagName)) {
          throw new Error(`Unpaired tag can not be used as closing tag: </${tagName}>`);
        }
        if (lastTagName && options.unpairedTagsSet.has(lastTagName)) {
          this.matcher.pop();
          this.tagsNodeStack.pop();
        }
        this.matcher.pop();
        this.isCurrentNodeStopNode = false;
        currentNode = this.tagsNodeStack.pop();
        textData = "";
        i = closeIndex;
      } else if (c1 === 63) {
        let tagData = readTagExp(xmlData, i, false, "?>");
        if (!tagData) throw new Error("Pi Tag is not closed.");
        textData = this.saveTextToParentTag(textData, currentNode, this.readonlyMatcher);
        const attsMap = this.buildAttributesMap(tagData.tagExp, this.matcher, tagData.tagName, true);
        if (attsMap) {
          const ver = attsMap[this.options.attributeNamePrefix + "version"];
          this.entityDecoder.setXmlVersion(Number(ver) || 1);
          docTypeReader.setXmlVersion(Number(ver) || 1);
        }
        if (options.ignoreDeclaration && tagData.tagName === "?xml" || options.ignorePiTags) {
        } else {
          const childNode = new XmlNode(tagData.tagName);
          childNode.add(options.textNodeName, "");
          if (tagData.tagName !== tagData.tagExp && tagData.attrExpPresent && options.ignoreAttributes !== true) {
            childNode[":@"] = attsMap;
          }
          this.addChild(currentNode, childNode, this.readonlyMatcher, i);
        }
        i = tagData.closeIndex + 1;
      } else if (c1 === 33 && xmlData.charCodeAt(i + 2) === 45 && xmlData.charCodeAt(i + 3) === 45) {
        const endIndex = findClosingIndex(xmlData, "-->", i + 4, "Comment is not closed.");
        if (options.commentPropName) {
          const comment = xmlData.substring(i + 4, endIndex - 2);
          textData = this.saveTextToParentTag(textData, currentNode, this.readonlyMatcher);
          currentNode.add(options.commentPropName, [{ [options.textNodeName]: comment }]);
        }
        i = endIndex;
      } else if (c1 === 33 && xmlData.charCodeAt(i + 2) === 68) {
        const result = docTypeReader.readDocType(xmlData, i);
        this.entityDecoder.addInputEntities(result.entities);
        i = result.i;
      } else if (c1 === 33 && xmlData.charCodeAt(i + 2) === 91) {
        const closeIndex = findClosingIndex(xmlData, "]]>", i, "CDATA is not closed.") - 2;
        const tagExp = xmlData.substring(i + 9, closeIndex);
        textData = this.saveTextToParentTag(textData, currentNode, this.readonlyMatcher);
        let val2 = this.parseTextData(tagExp, currentNode.tagname, this.readonlyMatcher, true, false, true, true);
        if (val2 == void 0) val2 = "";
        if (options.cdataPropName) {
          currentNode.add(options.cdataPropName, [{ [options.textNodeName]: tagExp }]);
        } else {
          currentNode.add(options.textNodeName, val2);
        }
        i = closeIndex + 2;
      } else {
        let result = readTagExp(xmlData, i, options.removeNSPrefix);
        if (!result) {
          const context = xmlData.substring(Math.max(0, i - 50), Math.min(xmlLen, i + 50));
          throw new Error(`readTagExp returned undefined at position ${i}. Context: "${context}"`);
        }
        let tagName = result.tagName;
        const rawTagName = result.rawTagName;
        let tagExp = result.tagExp;
        let attrExpPresent = result.attrExpPresent;
        let closeIndex = result.closeIndex;
        ({ tagName, tagExp } = transformTagName(options.transformTagName, tagName, tagExp, options));
        if (options.strictReservedNames && (tagName === options.commentPropName || tagName === options.cdataPropName || tagName === options.textNodeName || tagName === options.attributesGroupName)) {
          throw new Error(`Invalid tag name: ${tagName}`);
        }
        if (currentNode && textData) {
          if (currentNode.tagname !== "!xml") {
            textData = this.saveTextToParentTag(textData, currentNode, this.readonlyMatcher, false);
          }
        }
        const lastTag = currentNode;
        if (lastTag && options.unpairedTagsSet.has(lastTag.tagname)) {
          currentNode = this.tagsNodeStack.pop();
          this.matcher.pop();
        }
        let isSelfClosing = false;
        if (tagExp.length > 0 && tagExp.lastIndexOf("/") === tagExp.length - 1) {
          isSelfClosing = true;
          if (tagName[tagName.length - 1] === "/") {
            tagName = tagName.substr(0, tagName.length - 1);
            tagExp = tagName;
          } else {
            tagExp = tagExp.substr(0, tagExp.length - 1);
          }
          attrExpPresent = tagName !== tagExp;
        }
        let prefixedAttrs = null;
        let rawAttrs = {};
        let namespace = void 0;
        namespace = extractNamespace(rawTagName);
        if (tagName !== xmlObj.tagname) {
          this.matcher.push(tagName, {}, namespace);
        }
        if (tagName !== tagExp && attrExpPresent) {
          prefixedAttrs = this.buildAttributesMap(tagExp, this.matcher, tagName);
          if (prefixedAttrs) {
            rawAttrs = extractRawAttributes(prefixedAttrs, options);
          }
        }
        if (tagName !== xmlObj.tagname) {
          this.isCurrentNodeStopNode = this.isItStopNode();
        }
        const startIndex = i;
        if (this.isCurrentNodeStopNode) {
          let tagContent = "";
          if (isSelfClosing) {
            i = result.closeIndex;
          } else if (options.unpairedTagsSet.has(tagName)) {
            i = result.closeIndex;
          } else {
            const result2 = this.readStopNodeData(xmlData, rawTagName, closeIndex + 1);
            if (!result2) throw new Error(`Unexpected end of ${rawTagName}`);
            i = result2.i;
            tagContent = result2.tagContent;
          }
          const childNode = new XmlNode(tagName);
          if (prefixedAttrs) {
            childNode[":@"] = prefixedAttrs;
          }
          childNode.add(options.textNodeName, tagContent);
          this.matcher.pop();
          this.isCurrentNodeStopNode = false;
          this.addChild(currentNode, childNode, this.readonlyMatcher, startIndex);
        } else {
          if (isSelfClosing) {
            ({ tagName, tagExp } = transformTagName(options.transformTagName, tagName, tagExp, options));
            const childNode = new XmlNode(tagName);
            if (prefixedAttrs) {
              childNode[":@"] = prefixedAttrs;
            }
            this.addChild(currentNode, childNode, this.readonlyMatcher, startIndex);
            this.matcher.pop();
            this.isCurrentNodeStopNode = false;
          } else if (options.unpairedTagsSet.has(tagName)) {
            const childNode = new XmlNode(tagName);
            if (prefixedAttrs) {
              childNode[":@"] = prefixedAttrs;
            }
            this.addChild(currentNode, childNode, this.readonlyMatcher, startIndex);
            this.matcher.pop();
            this.isCurrentNodeStopNode = false;
            i = result.closeIndex;
            continue;
          } else {
            const childNode = new XmlNode(tagName);
            if (this.tagsNodeStack.length > options.maxNestedTags) {
              throw new Error("Maximum nested tags exceeded");
            }
            this.tagsNodeStack.push(currentNode);
            if (prefixedAttrs) {
              childNode[":@"] = prefixedAttrs;
            }
            this.addChild(currentNode, childNode, this.readonlyMatcher, startIndex);
            currentNode = childNode;
          }
          textData = "";
          i = closeIndex;
        }
      }
    } else {
      textData += xmlData[i];
    }
  }
  return xmlObj.child;
};
function addChild(currentNode, childNode, matcher, startIndex) {
  if (!this.options.captureMetaData) startIndex = void 0;
  const jPathOrMatcher = this.options.jPath ? matcher.toString() : matcher;
  const result = this.options.updateTag(childNode.tagname, jPathOrMatcher, childNode[":@"]);
  if (result === false) {
  } else if (typeof result === "string") {
    childNode.tagname = result;
    currentNode.addChild(childNode, startIndex);
  } else {
    currentNode.addChild(childNode, startIndex);
  }
}
function replaceEntitiesValue(val2, tagName, jPath) {
  const entityConfig = this.options.processEntities;
  if (!entityConfig || !entityConfig.enabled) {
    return val2;
  }
  if (entityConfig.allowedTags) {
    const jPathOrMatcher = this.options.jPath ? jPath.toString() : jPath;
    const allowed = Array.isArray(entityConfig.allowedTags) ? entityConfig.allowedTags.includes(tagName) : entityConfig.allowedTags(tagName, jPathOrMatcher);
    if (!allowed) {
      return val2;
    }
  }
  if (entityConfig.tagFilter) {
    const jPathOrMatcher = this.options.jPath ? jPath.toString() : jPath;
    if (!entityConfig.tagFilter(tagName, jPathOrMatcher)) {
      return val2;
    }
  }
  return this.entityDecoder.decode(val2);
}
function saveTextToParentTag(textData, parentNode, matcher, isLeafNode) {
  if (textData) {
    if (isLeafNode === void 0) isLeafNode = parentNode.child.length === 0;
    textData = this.parseTextData(
      textData,
      parentNode.tagname,
      matcher,
      false,
      parentNode[":@"] ? Object.keys(parentNode[":@"]).length !== 0 : false,
      isLeafNode
    );
    if (textData !== void 0 && textData !== "")
      parentNode.add(this.options.textNodeName, textData);
    textData = "";
  }
  return textData;
}
function isItStopNode() {
  if (this.stopNodeExpressionsSet.size === 0) return false;
  return this.matcher.matchesAny(this.stopNodeExpressionsSet);
}
function tagExpWithClosingIndex(xmlData, i, closingChar = ">") {
  let attrBoundary = 0;
  const len = xmlData.length;
  const closeCode0 = closingChar.charCodeAt(0);
  const closeCode1 = closingChar.length > 1 ? closingChar.charCodeAt(1) : -1;
  let result = "";
  let segmentStart = i;
  for (let index = i; index < len; index++) {
    const code = xmlData.charCodeAt(index);
    if (attrBoundary) {
      if (code === attrBoundary) attrBoundary = 0;
    } else if (code === 34 || code === 39) {
      attrBoundary = code;
    } else if (code === closeCode0) {
      if (closeCode1 !== -1) {
        if (xmlData.charCodeAt(index + 1) === closeCode1) {
          result += xmlData.substring(segmentStart, index);
          return { data: result, index };
        }
      } else {
        result += xmlData.substring(segmentStart, index);
        return { data: result, index };
      }
    } else if (code === 9 && !attrBoundary) {
      result += xmlData.substring(segmentStart, index) + " ";
      segmentStart = index + 1;
    }
  }
}
function findClosingIndex(xmlData, str, i, errMsg) {
  const closingIndex = xmlData.indexOf(str, i);
  if (closingIndex === -1) {
    throw new Error(errMsg);
  } else {
    return closingIndex + str.length - 1;
  }
}
function findClosingChar(xmlData, char, i, errMsg) {
  const closingIndex = xmlData.indexOf(char, i);
  if (closingIndex === -1) throw new Error(errMsg);
  return closingIndex;
}
function readTagExp(xmlData, i, removeNSPrefix, closingChar = ">") {
  const result = tagExpWithClosingIndex(xmlData, i + 1, closingChar);
  if (!result) return;
  let tagExp = result.data;
  const closeIndex = result.index;
  const separatorIndex = tagExp.search(/\s/);
  let tagName = tagExp;
  let attrExpPresent = true;
  if (separatorIndex !== -1) {
    tagName = tagExp.substring(0, separatorIndex);
    tagExp = tagExp.substring(separatorIndex + 1).trimStart();
  }
  const rawTagName = tagName;
  if (removeNSPrefix) {
    const colonIndex = tagName.indexOf(":");
    if (colonIndex !== -1) {
      tagName = tagName.substr(colonIndex + 1);
      attrExpPresent = tagName !== result.data.substr(colonIndex + 1);
    }
  }
  return {
    tagName,
    tagExp,
    closeIndex,
    attrExpPresent,
    rawTagName
  };
}
function readStopNodeData(xmlData, tagName, i) {
  const startIndex = i;
  let openTagCount = 1;
  const xmllen = xmlData.length;
  for (; i < xmllen; i++) {
    if (xmlData[i] === "<") {
      const c1 = xmlData.charCodeAt(i + 1);
      if (c1 === 47) {
        const closeIndex = findClosingChar(xmlData, ">", i, `${tagName} is not closed`);
        let closeTagName = xmlData.substring(i + 2, closeIndex).trim();
        if (closeTagName === tagName) {
          openTagCount--;
          if (openTagCount === 0) {
            return {
              tagContent: xmlData.substring(startIndex, i),
              i: closeIndex
            };
          }
        }
        i = closeIndex;
      } else if (c1 === 63) {
        const closeIndex = findClosingIndex(xmlData, "?>", i + 1, "StopNode is not closed.");
        i = closeIndex;
      } else if (c1 === 33 && xmlData.charCodeAt(i + 2) === 45 && xmlData.charCodeAt(i + 3) === 45) {
        const closeIndex = findClosingIndex(xmlData, "-->", i + 3, "StopNode is not closed.");
        i = closeIndex;
      } else if (c1 === 33 && xmlData.charCodeAt(i + 2) === 91) {
        const closeIndex = findClosingIndex(xmlData, "]]>", i, "StopNode is not closed.") - 2;
        i = closeIndex;
      } else {
        const tagData = readTagExp(xmlData, i, false);
        if (tagData) {
          const openTagName = tagData && tagData.tagName;
          if (openTagName === tagName && tagData.tagExp[tagData.tagExp.length - 1] !== "/") {
            openTagCount++;
          }
          i = tagData.closeIndex;
        }
      }
    }
  }
}
function parseValue(val2, shouldParse, options) {
  if (shouldParse && typeof val2 === "string") {
    const newval = val2.trim();
    if (newval === "true") return true;
    else if (newval === "false") return false;
    else return toNumber(val2, options);
  } else {
    if (isExist(val2)) {
      return val2;
    } else {
      return "";
    }
  }
}
function transformTagName(fn, tagName, tagExp, options) {
  if (fn) {
    const newTagName = fn(tagName);
    if (tagExp === tagName) {
      tagExp = newTagName;
    }
    tagName = newTagName;
  }
  tagName = sanitizeName(tagName, options);
  return { tagName, tagExp };
}
function sanitizeName(name, options) {
  if (criticalProperties.includes(name)) {
    throw new Error(`[SECURITY] Invalid name: "${name}" is a reserved JavaScript keyword that could cause prototype pollution`);
  } else if (DANGEROUS_PROPERTY_NAMES.includes(name)) {
    return options.onDangerousProperty(name);
  }
  return name;
}
var METADATA_SYMBOL2 = XmlNode.getMetaDataSymbol();
function stripAttributePrefix(attrs, prefix) {
  if (!attrs || typeof attrs !== "object") return {};
  if (!prefix) return attrs;
  const rawAttrs = {};
  for (const key in attrs) {
    if (key.startsWith(prefix)) {
      const rawName = key.substring(prefix.length);
      rawAttrs[rawName] = attrs[key];
    } else {
      rawAttrs[key] = attrs[key];
    }
  }
  return rawAttrs;
}
function prettify(node, options, matcher, readonlyMatcher) {
  return compress(node, options, matcher, readonlyMatcher);
}
function compress(arr, options, matcher, readonlyMatcher) {
  let text;
  const compressedObj = {};
  for (let i = 0; i < arr.length; i++) {
    const tagObj = arr[i];
    const property = propName(tagObj);
    if (property !== void 0 && property !== options.textNodeName) {
      const rawAttrs = stripAttributePrefix(
        tagObj[":@"] || {},
        options.attributeNamePrefix
      );
      matcher.push(property, rawAttrs);
    }
    if (property === options.textNodeName) {
      if (text === void 0) text = tagObj[property];
      else text += "" + tagObj[property];
    } else if (property === void 0) {
      continue;
    } else if (tagObj[property]) {
      let val2 = compress(tagObj[property], options, matcher, readonlyMatcher);
      const isLeaf = isLeafTag(val2, options);
      if (Object.keys(val2).length === 0 && options.alwaysCreateTextNode) {
        val2[options.textNodeName] = "";
      }
      if (tagObj[":@"]) {
        assignAttributes(val2, tagObj[":@"], readonlyMatcher, options);
      } else if (Object.keys(val2).length === 1 && val2[options.textNodeName] !== void 0 && !options.alwaysCreateTextNode) {
        val2 = val2[options.textNodeName];
      } else if (Object.keys(val2).length === 0) {
        if (options.alwaysCreateTextNode) val2[options.textNodeName] = "";
        else val2 = "";
      }
      if (tagObj[METADATA_SYMBOL2] !== void 0 && typeof val2 === "object" && val2 !== null) {
        val2[METADATA_SYMBOL2] = tagObj[METADATA_SYMBOL2];
      }
      if (compressedObj[property] !== void 0 && Object.prototype.hasOwnProperty.call(compressedObj, property)) {
        if (!Array.isArray(compressedObj[property])) {
          compressedObj[property] = [compressedObj[property]];
        }
        compressedObj[property].push(val2);
      } else {
        const jPathOrMatcher = options.jPath ? readonlyMatcher.toString() : readonlyMatcher;
        if (options.isArray(property, jPathOrMatcher, isLeaf)) {
          compressedObj[property] = [val2];
        } else {
          compressedObj[property] = val2;
        }
      }
      if (property !== void 0 && property !== options.textNodeName) {
        matcher.pop();
      }
    }
  }
  if (typeof text === "string") {
    if (text.length > 0) compressedObj[options.textNodeName] = text;
  } else if (text !== void 0) compressedObj[options.textNodeName] = text;
  return compressedObj;
}
function propName(obj) {
  const keys = Object.keys(obj);
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    if (key !== ":@") return key;
  }
}
function assignAttributes(obj, attrMap, readonlyMatcher, options) {
  if (attrMap) {
    const keys = Object.keys(attrMap);
    const len = keys.length;
    for (let i = 0; i < len; i++) {
      const atrrName = keys[i];
      const rawAttrName = atrrName.startsWith(options.attributeNamePrefix) ? atrrName.substring(options.attributeNamePrefix.length) : atrrName;
      const jPathOrMatcher = options.jPath ? readonlyMatcher.toString() + "." + rawAttrName : readonlyMatcher;
      if (options.isArray(atrrName, jPathOrMatcher, true, true)) {
        obj[atrrName] = [attrMap[atrrName]];
      } else {
        obj[atrrName] = attrMap[atrrName];
      }
    }
  }
}
function isLeafTag(obj, options) {
  const { textNodeName } = options;
  const propCount = Object.keys(obj).length;
  if (propCount === 0) {
    return true;
  }
  if (propCount === 1 && (obj[textNodeName] || typeof obj[textNodeName] === "boolean" || obj[textNodeName] === 0)) {
    return true;
  }
  return false;
}
var XMLParser = class {
  constructor(options) {
    this.externalEntities = {};
    this.options = buildOptions(options);
  }
  /**
   * Parse XML dats to JS object
   * @param {string|Uint8Array} xmlData
   * @param {boolean|Object} validationOption
   */
  parse(xmlData, validationOption) {
    if (typeof xmlData !== "string" && xmlData.toString) {
      xmlData = xmlData.toString();
    } else if (typeof xmlData !== "string") {
      throw new Error("XML data is accepted in String or Bytes[] form.");
    }
    if (validationOption) {
      if (validationOption === true) validationOption = {};
      const result = validate(xmlData, validationOption);
      if (result !== true) {
        throw Error(`${result.err.msg}:${result.err.line}:${result.err.col}`);
      }
    }
    const orderedObjParser = new OrderedObjParser(this.options, this.externalEntities);
    const orderedResult = orderedObjParser.parseXml(xmlData);
    if (this.options.preserveOrder || orderedResult === void 0) return orderedResult;
    else return prettify(orderedResult, this.options, orderedObjParser.matcher, orderedObjParser.readonlyMatcher);
  }
  /**
   * Add Entity which is not by default supported by this library
   * @param {string} key
   * @param {string} value
   */
  addEntity(key, value) {
    if (value.indexOf("&") !== -1) {
      throw new Error("Entity value can't have '&'");
    } else if (key.indexOf("&") !== -1 || key.indexOf(";") !== -1) {
      throw new Error("An entity must be set without '&' and ';'. Eg. use '#xD' for '&#xD;'");
    } else if (value === "&") {
      throw new Error("An entity with value '&' is not permitted");
    } else {
      this.externalEntities[key] = value;
    }
  }
  /**
   * Returns a Symbol that can be used to access the metadata
   * property on a node.
   *
   * If Symbol is not available in the environment, an ordinary property is used
   * and the name of the property is here returned.
   *
   * The XMLMetaData property is only present when `captureMetaData`
   * is true in the options.
   */
  static getMetaDataSymbol() {
    return XmlNode.getMetaDataSymbol();
  }
};
function safeComment(val2) {
  return String(val2).replace(/--/g, "- -").replace(/--/g, "- -").replace(/-$/, "- ");
}
function safeCdata(val2) {
  return String(val2).replace(/\]\]>/g, "]]]]><![CDATA[>");
}
function escapeAttribute(val2) {
  return String(val2).replace(/"/g, "&quot;").replace(/'/g, "&apos;");
}
var EOL = "\n";
function detectXmlVersionFromArray(jArray, options) {
  if (!Array.isArray(jArray) || jArray.length === 0) return "1.0";
  const first = jArray[0];
  const firstKey = propName2(first);
  if (firstKey === "?xml") {
    const attrs = first[":@"];
    if (attrs) {
      const versionKey = options.attributeNamePrefix + "version";
      if (attrs[versionKey]) return attrs[versionKey];
    }
  }
  return "1.0";
}
function resolveTagName(name, isAttribute2, options, matcher, xmlVersion) {
  if (!options.sanitizeName) return name;
  if (qName(name, { xmlVersion })) return name;
  return options.sanitizeName(name, { isAttribute: isAttribute2, matcher: matcher.readOnly() });
}
function toXml(jArray, options) {
  let indentation = "";
  if (options.format) {
    indentation = EOL;
  }
  const stopNodeExpressions = [];
  if (options.stopNodes && Array.isArray(options.stopNodes)) {
    for (let i = 0; i < options.stopNodes.length; i++) {
      const node = options.stopNodes[i];
      if (typeof node === "string") {
        stopNodeExpressions.push(new Expression(node));
      } else if (node instanceof Expression) {
        stopNodeExpressions.push(node);
      }
    }
  }
  const xmlVersion = detectXmlVersionFromArray(jArray, options);
  const matcher = new Matcher();
  return arrToStr(jArray, options, indentation, matcher, stopNodeExpressions, xmlVersion);
}
function arrToStr(arr, options, indentation, matcher, stopNodeExpressions, xmlVersion) {
  let xmlStr = "";
  let isPreviousElementTag = false;
  if (options.maxNestedTags && matcher.getDepth() > options.maxNestedTags) {
    throw new Error("Maximum nested tags exceeded");
  }
  if (!Array.isArray(arr)) {
    if (arr !== void 0 && arr !== null) {
      let text = arr.toString();
      text = replaceEntitiesValue2(text, options);
      return text;
    }
    return "";
  }
  for (let i = 0; i < arr.length; i++) {
    const tagObj = arr[i];
    const rawTagName = propName2(tagObj);
    if (rawTagName === void 0) continue;
    const isSpecialName = rawTagName === options.textNodeName || rawTagName === options.cdataPropName || rawTagName === options.commentPropName || rawTagName[0] === "?";
    const tagName = isSpecialName ? rawTagName : resolveTagName(rawTagName, false, options, matcher, xmlVersion);
    const attrValues = extractAttributeValues(tagObj[":@"], options);
    matcher.push(tagName, attrValues);
    const isStopNode = checkStopNode(matcher, stopNodeExpressions);
    if (tagName === options.textNodeName) {
      let tagText = tagObj[rawTagName];
      if (!isStopNode) {
        tagText = options.tagValueProcessor(tagName, tagText);
        tagText = replaceEntitiesValue2(tagText, options);
      }
      if (isPreviousElementTag) {
        xmlStr += indentation;
      }
      xmlStr += tagText;
      isPreviousElementTag = false;
      matcher.pop();
      continue;
    } else if (tagName === options.cdataPropName) {
      if (isPreviousElementTag) {
        xmlStr += indentation;
      }
      const val2 = tagObj[rawTagName][0][options.textNodeName];
      const safeVal = safeCdata(val2);
      xmlStr += `<![CDATA[${safeVal}]]>`;
      isPreviousElementTag = false;
      matcher.pop();
      continue;
    } else if (tagName === options.commentPropName) {
      const val2 = tagObj[rawTagName][0][options.textNodeName];
      const safeVal = safeComment(val2);
      xmlStr += indentation + `<!--${safeVal}-->`;
      isPreviousElementTag = true;
      matcher.pop();
      continue;
    } else if (tagName[0] === "?") {
      const attStr2 = attr_to_str(tagObj[":@"], options, isStopNode, matcher, xmlVersion);
      const tempInd = tagName === "?xml" ? "" : indentation;
      xmlStr += tempInd + `<${tagName}${attStr2}?>`;
      isPreviousElementTag = true;
      matcher.pop();
      continue;
    }
    let newIdentation = indentation;
    if (newIdentation !== "") {
      newIdentation += options.indentBy;
    }
    const attStr = attr_to_str(tagObj[":@"], options, isStopNode, matcher, xmlVersion);
    const tagStart = indentation + `<${tagName}${attStr}`;
    let tagValue;
    if (isStopNode) {
      tagValue = getRawContent(tagObj[rawTagName], options);
    } else {
      tagValue = arrToStr(tagObj[rawTagName], options, newIdentation, matcher, stopNodeExpressions, xmlVersion);
    }
    if (options.unpairedTags.indexOf(tagName) !== -1) {
      if (options.suppressUnpairedNode) xmlStr += tagStart + ">";
      else xmlStr += tagStart + "/>";
    } else if ((!tagValue || tagValue.length === 0) && options.suppressEmptyNode) {
      xmlStr += tagStart + "/>";
    } else if (tagValue && tagValue.endsWith(">")) {
      xmlStr += tagStart + `>${tagValue}${indentation}</${tagName}>`;
    } else {
      xmlStr += tagStart + ">";
      if (tagValue && indentation !== "" && (tagValue.includes("/>") || tagValue.includes("</"))) {
        xmlStr += indentation + options.indentBy + tagValue + indentation;
      } else {
        xmlStr += tagValue;
      }
      xmlStr += `</${tagName}>`;
    }
    isPreviousElementTag = true;
    matcher.pop();
  }
  return xmlStr;
}
function extractAttributeValues(attrMap, options) {
  if (!attrMap || options.ignoreAttributes) return null;
  const attrValues = {};
  let hasAttrs = false;
  for (let attr2 in attrMap) {
    if (!Object.prototype.hasOwnProperty.call(attrMap, attr2)) continue;
    const cleanAttrName = attr2.startsWith(options.attributeNamePrefix) ? attr2.substr(options.attributeNamePrefix.length) : attr2;
    attrValues[cleanAttrName] = escapeAttribute(attrMap[attr2]);
    hasAttrs = true;
  }
  return hasAttrs ? attrValues : null;
}
function getRawContent(arr, options) {
  if (!Array.isArray(arr)) {
    if (arr !== void 0 && arr !== null) {
      return arr.toString();
    }
    return "";
  }
  let content = "";
  for (let i = 0; i < arr.length; i++) {
    const item = arr[i];
    const tagName = propName2(item);
    if (tagName === options.textNodeName) {
      content += item[tagName];
    } else if (tagName === options.cdataPropName) {
      content += item[tagName][0][options.textNodeName];
    } else if (tagName === options.commentPropName) {
      content += item[tagName][0][options.textNodeName];
    } else if (tagName && tagName[0] === "?") {
      continue;
    } else if (tagName) {
      const attStr = attr_to_str_raw(item[":@"], options);
      const nestedContent = getRawContent(item[tagName], options);
      if (!nestedContent || nestedContent.length === 0) {
        content += `<${tagName}${attStr}/>`;
      } else {
        content += `<${tagName}${attStr}>${nestedContent}</${tagName}>`;
      }
    }
  }
  return content;
}
function attr_to_str_raw(attrMap, options) {
  let attrStr = "";
  if (attrMap && !options.ignoreAttributes) {
    for (let attr2 in attrMap) {
      if (!Object.prototype.hasOwnProperty.call(attrMap, attr2)) continue;
      let attrVal = attrMap[attr2];
      if (attrVal === true && options.suppressBooleanAttributes) {
        attrStr += ` ${attr2.substr(options.attributeNamePrefix.length)}`;
      } else {
        attrStr += ` ${attr2.substr(options.attributeNamePrefix.length)}="${escapeAttribute(attrVal)}"`;
      }
    }
  }
  return attrStr;
}
function propName2(obj) {
  const keys = Object.keys(obj);
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    if (!Object.prototype.hasOwnProperty.call(obj, key)) continue;
    if (key !== ":@") return key;
  }
}
function attr_to_str(attrMap, options, isStopNode, matcher, xmlVersion) {
  let attrStr = "";
  if (attrMap && !options.ignoreAttributes) {
    for (let attr2 in attrMap) {
      if (!Object.prototype.hasOwnProperty.call(attrMap, attr2)) continue;
      const cleanAttrName = attr2.substr(options.attributeNamePrefix.length);
      const resolvedAttrName = isStopNode ? cleanAttrName : resolveTagName(cleanAttrName, true, options, matcher, xmlVersion);
      let attrVal;
      if (isStopNode) {
        attrVal = attrMap[attr2];
      } else {
        attrVal = options.attributeValueProcessor(attr2, attrMap[attr2]);
        attrVal = replaceEntitiesValue2(attrVal, options);
      }
      if (attrVal === true && options.suppressBooleanAttributes) {
        attrStr += ` ${resolvedAttrName}`;
      } else {
        attrStr += ` ${resolvedAttrName}="${escapeAttribute(attrVal)}"`;
      }
    }
  }
  return attrStr;
}
function checkStopNode(matcher, stopNodeExpressions) {
  if (!stopNodeExpressions || stopNodeExpressions.length === 0) return false;
  for (let i = 0; i < stopNodeExpressions.length; i++) {
    if (matcher.matches(stopNodeExpressions[i])) {
      return true;
    }
  }
  return false;
}
function replaceEntitiesValue2(textValue, options) {
  if (textValue && textValue.length > 0 && options.processEntities) {
    for (let i = 0; i < options.entities.length; i++) {
      const entity = options.entities[i];
      textValue = textValue.replace(entity.regex, entity.val);
    }
  }
  return textValue;
}
function getIgnoreAttributesFn2(ignoreAttributes) {
  if (typeof ignoreAttributes === "function") {
    return ignoreAttributes;
  }
  if (Array.isArray(ignoreAttributes)) {
    return (attrName) => {
      for (const pattern of ignoreAttributes) {
        if (typeof pattern === "string" && attrName === pattern) {
          return true;
        }
        if (pattern instanceof RegExp && pattern.test(attrName)) {
          return true;
        }
      }
    };
  }
  return () => false;
}
var defaultOptions3 = {
  attributeNamePrefix: "@_",
  attributesGroupName: false,
  textNodeName: "#text",
  ignoreAttributes: true,
  cdataPropName: false,
  format: false,
  indentBy: "  ",
  suppressEmptyNode: false,
  suppressUnpairedNode: true,
  suppressBooleanAttributes: true,
  tagValueProcessor: function(key, a) {
    return a;
  },
  attributeValueProcessor: function(attrName, a) {
    return a;
  },
  preserveOrder: false,
  commentPropName: false,
  unpairedTags: [],
  entities: [
    { regex: new RegExp("&", "g"), val: "&amp;" },
    //it must be on top
    { regex: new RegExp(">", "g"), val: "&gt;" },
    { regex: new RegExp("<", "g"), val: "&lt;" },
    { regex: new RegExp("'", "g"), val: "&apos;" },
    { regex: new RegExp('"', "g"), val: "&quot;" }
  ],
  processEntities: true,
  stopNodes: [],
  // transformTagName: false,
  // transformAttributeName: false,
  oneListGroup: false,
  maxNestedTags: 100,
  jPath: true,
  // When true, callbacks receive string jPath; when false, receive Matcher instance
  sanitizeName: false
  // false = allow all names as-is (default, backward-compatible).
  // Set to a function (name, { isAttribute, matcher }) => string to
  // validate/sanitize tag and attribute names. Throw inside the function
  // to reject an invalid name.
};
function Builder(options) {
  this.options = Object.assign({}, defaultOptions3, options);
  if (this.options.stopNodes && Array.isArray(this.options.stopNodes)) {
    this.options.stopNodes = this.options.stopNodes.map((node) => {
      if (typeof node === "string" && node.startsWith("*.")) {
        return ".." + node.substring(2);
      }
      return node;
    });
  }
  this.stopNodeExpressions = [];
  if (this.options.stopNodes && Array.isArray(this.options.stopNodes)) {
    for (let i = 0; i < this.options.stopNodes.length; i++) {
      const node = this.options.stopNodes[i];
      if (typeof node === "string") {
        this.stopNodeExpressions.push(new Expression(node));
      } else if (node instanceof Expression) {
        this.stopNodeExpressions.push(node);
      }
    }
  }
  if (this.options.ignoreAttributes === true || this.options.attributesGroupName) {
    this.isAttribute = function() {
      return false;
    };
  } else {
    this.ignoreAttributesFn = getIgnoreAttributesFn2(this.options.ignoreAttributes);
    this.attrPrefixLen = this.options.attributeNamePrefix.length;
    this.isAttribute = isAttribute;
  }
  this.processTextOrObjNode = processTextOrObjNode;
  if (this.options.format) {
    this.indentate = indentate;
    this.tagEndChar = ">\n";
    this.newLine = "\n";
  } else {
    this.indentate = function() {
      return "";
    };
    this.tagEndChar = ">";
    this.newLine = "";
  }
}
function detectXmlVersionFromObj(jObj, options) {
  const decl = jObj["?xml"];
  if (decl && typeof decl === "object") {
    if (options.attributesGroupName && decl[options.attributesGroupName]) {
      const v2 = decl[options.attributesGroupName][options.attributeNamePrefix + "version"];
      if (v2) return v2;
    }
    const v = decl[options.attributeNamePrefix + "version"];
    if (v) return v;
  }
  return "1.0";
}
function resolveTagName2(name, isAttribute2, options, matcher, xmlVersion) {
  if (!options.sanitizeName) return name;
  if (qName(name, { xmlVersion })) return name;
  return options.sanitizeName(name, { isAttribute: isAttribute2, matcher: matcher.readOnly() });
}
Builder.prototype.build = function(jObj) {
  if (this.options.preserveOrder) {
    return toXml(jObj, this.options);
  } else {
    if (Array.isArray(jObj) && this.options.arrayNodeName && this.options.arrayNodeName.length > 1) {
      jObj = {
        [this.options.arrayNodeName]: jObj
      };
    }
    const matcher = new Matcher();
    const xmlVersion = detectXmlVersionFromObj(jObj, this.options);
    return this.j2x(jObj, 0, matcher, xmlVersion).val;
  }
};
Builder.prototype.j2x = function(jObj, level, matcher, xmlVersion) {
  let attrStr = "";
  let val2 = "";
  if (this.options.maxNestedTags && matcher.getDepth() >= this.options.maxNestedTags) {
    throw new Error("Maximum nested tags exceeded");
  }
  const jPath = this.options.jPath ? matcher.toString() : matcher;
  const isCurrentStopNode = this.checkStopNode(matcher);
  for (let key in jObj) {
    if (!Object.prototype.hasOwnProperty.call(jObj, key)) continue;
    const isSpecialKey = key === this.options.textNodeName || key === this.options.cdataPropName || key === this.options.commentPropName || this.options.attributesGroupName && key === this.options.attributesGroupName || this.isAttribute(key) || key[0] === "?";
    const resolvedKey = isSpecialKey ? key : resolveTagName2(key, false, this.options, matcher, xmlVersion);
    if (typeof jObj[key] === "undefined") {
      if (this.isAttribute(key)) {
        val2 += "";
      }
    } else if (jObj[key] === null) {
      if (this.isAttribute(key)) {
        val2 += "";
      } else if (resolvedKey === this.options.cdataPropName || resolvedKey === this.options.commentPropName) {
        val2 += "";
      } else if (resolvedKey[0] === "?") {
        val2 += this.indentate(level) + "<" + resolvedKey + "?" + this.tagEndChar;
      } else {
        val2 += this.indentate(level) + "<" + resolvedKey + "/" + this.tagEndChar;
      }
    } else if (jObj[key] instanceof Date) {
      val2 += this.buildTextValNode(jObj[key], resolvedKey, "", level, matcher);
    } else if (typeof jObj[key] !== "object") {
      const attr2 = this.isAttribute(key);
      if (attr2 && !this.ignoreAttributesFn(attr2, jPath)) {
        const resolvedAttr = resolveTagName2(attr2, true, this.options, matcher, xmlVersion);
        attrStr += this.buildAttrPairStr(resolvedAttr, "" + jObj[key], isCurrentStopNode);
      } else if (!attr2) {
        if (key === this.options.textNodeName) {
          let newval = this.options.tagValueProcessor(key, "" + jObj[key]);
          val2 += this.replaceEntitiesValue(newval);
        } else {
          matcher.push(resolvedKey);
          const isStopNode = this.checkStopNode(matcher);
          matcher.pop();
          if (isStopNode) {
            const textValue = "" + jObj[key];
            if (textValue === "") {
              val2 += this.indentate(level) + "<" + resolvedKey + this.closeTag(resolvedKey) + this.tagEndChar;
            } else {
              val2 += this.indentate(level) + "<" + resolvedKey + ">" + textValue + "</" + resolvedKey + this.tagEndChar;
            }
          } else {
            val2 += this.buildTextValNode(jObj[key], resolvedKey, "", level, matcher);
          }
        }
      }
    } else if (Array.isArray(jObj[key])) {
      const arrLen = jObj[key].length;
      let listTagVal = "";
      let listTagAttr = "";
      for (let j = 0; j < arrLen; j++) {
        const item = jObj[key][j];
        if (typeof item === "undefined") {
        } else if (item === null) {
          if (resolvedKey[0] === "?") val2 += this.indentate(level) + "<" + resolvedKey + "?" + this.tagEndChar;
          else val2 += this.indentate(level) + "<" + resolvedKey + "/" + this.tagEndChar;
        } else if (typeof item === "object") {
          if (this.options.oneListGroup) {
            matcher.push(resolvedKey);
            const result = this.j2x(item, level + 1, matcher, xmlVersion);
            matcher.pop();
            listTagVal += result.val;
            if (this.options.attributesGroupName && item.hasOwnProperty(this.options.attributesGroupName)) {
              listTagAttr += result.attrStr;
            }
          } else {
            listTagVal += this.processTextOrObjNode(item, resolvedKey, level, matcher, xmlVersion);
          }
        } else {
          if (this.options.oneListGroup) {
            let textValue = this.options.tagValueProcessor(resolvedKey, item);
            textValue = this.replaceEntitiesValue(textValue);
            listTagVal += textValue;
          } else {
            matcher.push(resolvedKey);
            const isStopNode = this.checkStopNode(matcher);
            matcher.pop();
            if (isStopNode) {
              const textValue = "" + item;
              if (textValue === "") {
                listTagVal += this.indentate(level) + "<" + resolvedKey + this.closeTag(resolvedKey) + this.tagEndChar;
              } else {
                listTagVal += this.indentate(level) + "<" + resolvedKey + ">" + textValue + "</" + resolvedKey + this.tagEndChar;
              }
            } else {
              listTagVal += this.buildTextValNode(item, resolvedKey, "", level, matcher);
            }
          }
        }
      }
      if (this.options.oneListGroup) {
        listTagVal = this.buildObjectNode(listTagVal, resolvedKey, listTagAttr, level);
      }
      val2 += listTagVal;
    } else {
      if (this.options.attributesGroupName && key === this.options.attributesGroupName) {
        const Ks = Object.keys(jObj[key]);
        const L = Ks.length;
        for (let j = 0; j < L; j++) {
          const resolvedAttr = resolveTagName2(Ks[j], true, this.options, matcher, xmlVersion);
          attrStr += this.buildAttrPairStr(resolvedAttr, "" + jObj[key][Ks[j]], isCurrentStopNode);
        }
      } else {
        val2 += this.processTextOrObjNode(jObj[key], resolvedKey, level, matcher, xmlVersion);
      }
    }
  }
  return { attrStr, val: val2 };
};
Builder.prototype.buildAttrPairStr = function(attrName, val2, isStopNode) {
  if (!isStopNode) {
    val2 = this.options.attributeValueProcessor(attrName, "" + val2);
    val2 = this.replaceEntitiesValue(val2);
  }
  if (this.options.suppressBooleanAttributes && val2 === "true") {
    return " " + attrName;
  } else return " " + attrName + '="' + escapeAttribute(val2) + '"';
};
function processTextOrObjNode(object, key, level, matcher, xmlVersion) {
  const attrValues = this.extractAttributes(object);
  matcher.push(key, attrValues);
  const isStopNode = this.checkStopNode(matcher);
  if (isStopNode) {
    const rawContent = this.buildRawContent(object);
    const attrStr = this.buildAttributesForStopNode(object);
    matcher.pop();
    return this.buildObjectNode(rawContent, key, attrStr, level);
  }
  const result = this.j2x(object, level + 1, matcher, xmlVersion);
  matcher.pop();
  if (key[0] === "?") {
    return this.buildTextValNode("", key, result.attrStr, level, matcher);
  } else if (object[this.options.textNodeName] !== void 0 && Object.keys(object).length === 1) {
    return this.buildTextValNode(object[this.options.textNodeName], key, result.attrStr, level, matcher);
  } else {
    return this.buildObjectNode(result.val, key, result.attrStr, level);
  }
}
Builder.prototype.extractAttributes = function(obj) {
  if (!obj || typeof obj !== "object") return null;
  const attrValues = {};
  let hasAttrs = false;
  if (this.options.attributesGroupName && obj[this.options.attributesGroupName]) {
    const attrGroup = obj[this.options.attributesGroupName];
    for (let attrKey in attrGroup) {
      if (!Object.prototype.hasOwnProperty.call(attrGroup, attrKey)) continue;
      const cleanKey = attrKey.startsWith(this.options.attributeNamePrefix) ? attrKey.substring(this.options.attributeNamePrefix.length) : attrKey;
      attrValues[cleanKey] = escapeAttribute(attrGroup[attrKey]);
      hasAttrs = true;
    }
  } else {
    for (let key in obj) {
      if (!Object.prototype.hasOwnProperty.call(obj, key)) continue;
      const attr2 = this.isAttribute(key);
      if (attr2) {
        attrValues[attr2] = escapeAttribute(obj[key]);
        hasAttrs = true;
      }
    }
  }
  return hasAttrs ? attrValues : null;
};
Builder.prototype.buildRawContent = function(obj) {
  if (typeof obj === "string") {
    return obj;
  }
  if (typeof obj !== "object" || obj === null) {
    return String(obj);
  }
  if (obj[this.options.textNodeName] !== void 0) {
    return obj[this.options.textNodeName];
  }
  let content = "";
  for (let key in obj) {
    if (!Object.prototype.hasOwnProperty.call(obj, key)) continue;
    if (this.isAttribute(key)) continue;
    if (this.options.attributesGroupName && key === this.options.attributesGroupName) continue;
    const value = obj[key];
    if (key === this.options.textNodeName) {
      content += value;
    } else if (Array.isArray(value)) {
      for (let item of value) {
        if (typeof item === "string" || typeof item === "number") {
          content += `<${key}>${item}</${key}>`;
        } else if (typeof item === "object" && item !== null) {
          const nestedContent = this.buildRawContent(item);
          const nestedAttrs = this.buildAttributesForStopNode(item);
          if (nestedContent === "") {
            content += `<${key}${nestedAttrs}/>`;
          } else {
            content += `<${key}${nestedAttrs}>${nestedContent}</${key}>`;
          }
        }
      }
    } else if (typeof value === "object" && value !== null) {
      const nestedContent = this.buildRawContent(value);
      const nestedAttrs = this.buildAttributesForStopNode(value);
      if (nestedContent === "") {
        content += `<${key}${nestedAttrs}/>`;
      } else {
        content += `<${key}${nestedAttrs}>${nestedContent}</${key}>`;
      }
    } else {
      content += `<${key}>${value}</${key}>`;
    }
  }
  return content;
};
Builder.prototype.buildAttributesForStopNode = function(obj) {
  if (!obj || typeof obj !== "object") return "";
  let attrStr = "";
  if (this.options.attributesGroupName && obj[this.options.attributesGroupName]) {
    const attrGroup = obj[this.options.attributesGroupName];
    for (let attrKey in attrGroup) {
      if (!Object.prototype.hasOwnProperty.call(attrGroup, attrKey)) continue;
      const cleanKey = attrKey.startsWith(this.options.attributeNamePrefix) ? attrKey.substring(this.options.attributeNamePrefix.length) : attrKey;
      const val2 = attrGroup[attrKey];
      if (val2 === true && this.options.suppressBooleanAttributes) {
        attrStr += " " + cleanKey;
      } else {
        attrStr += " " + cleanKey + '="' + val2 + '"';
      }
    }
  } else {
    for (let key in obj) {
      if (!Object.prototype.hasOwnProperty.call(obj, key)) continue;
      const attr2 = this.isAttribute(key);
      if (attr2) {
        const val2 = obj[key];
        if (val2 === true && this.options.suppressBooleanAttributes) {
          attrStr += " " + attr2;
        } else {
          attrStr += " " + attr2 + '="' + val2 + '"';
        }
      }
    }
  }
  return attrStr;
};
Builder.prototype.buildObjectNode = function(val2, key, attrStr, level) {
  if (val2 === "") {
    if (key[0] === "?") return this.indentate(level) + "<" + key + attrStr + "?" + this.tagEndChar;
    else {
      return this.indentate(level) + "<" + key + attrStr + this.closeTag(key) + this.tagEndChar;
    }
  } else if (key[0] === "?") {
    return this.indentate(level) + "<" + key + attrStr + "?" + this.tagEndChar;
  } else {
    let tagEndExp = "</" + key + this.tagEndChar;
    let piClosingChar = "";
    if (key[0] === "?") {
      piClosingChar = "?";
      tagEndExp = "";
    }
    if ((attrStr || attrStr === "") && val2.indexOf("<") === -1) {
      return this.indentate(level) + "<" + key + attrStr + piClosingChar + ">" + val2 + tagEndExp;
    } else if (this.options.commentPropName !== false && key === this.options.commentPropName && piClosingChar.length === 0) {
      return this.indentate(level) + `<!--${val2}-->` + this.newLine;
    } else {
      return this.indentate(level) + "<" + key + attrStr + piClosingChar + this.tagEndChar + val2 + this.indentate(level) + tagEndExp;
    }
  }
};
Builder.prototype.closeTag = function(key) {
  let closeTag = "";
  if (this.options.unpairedTags.indexOf(key) !== -1) {
    if (!this.options.suppressUnpairedNode) closeTag = "/";
  } else if (this.options.suppressEmptyNode) {
    closeTag = "/";
  } else {
    closeTag = `></${key}`;
  }
  return closeTag;
};
Builder.prototype.checkStopNode = function(matcher) {
  if (!this.stopNodeExpressions || this.stopNodeExpressions.length === 0) return false;
  for (let i = 0; i < this.stopNodeExpressions.length; i++) {
    if (matcher.matches(this.stopNodeExpressions[i])) {
      return true;
    }
  }
  return false;
};
Builder.prototype.buildTextValNode = function(val2, key, attrStr, level, matcher) {
  if (this.options.cdataPropName !== false && key === this.options.cdataPropName) {
    const safeVal = safeCdata(val2);
    return this.indentate(level) + `<![CDATA[${safeVal}]]>` + this.newLine;
  } else if (this.options.commentPropName !== false && key === this.options.commentPropName) {
    const safeVal = safeComment(val2);
    return this.indentate(level) + `<!--${safeVal}-->` + this.newLine;
  } else if (key[0] === "?") {
    return this.indentate(level) + "<" + key + attrStr + "?" + this.tagEndChar;
  } else {
    let textValue = this.options.tagValueProcessor(key, val2);
    textValue = this.replaceEntitiesValue(textValue);
    if (textValue === "") {
      return this.indentate(level) + "<" + key + attrStr + this.closeTag(key) + this.tagEndChar;
    } else {
      return this.indentate(level) + "<" + key + attrStr + ">" + textValue + "</" + key + this.tagEndChar;
    }
  }
};
Builder.prototype.replaceEntitiesValue = function(textValue) {
  if (textValue && textValue.length > 0 && this.options.processEntities) {
    for (let i = 0; i < this.options.entities.length; i++) {
      const entity = this.options.entities[i];
      textValue = textValue.replace(entity.regex, entity.val);
    }
  }
  return textValue;
};
function indentate(level) {
  return this.options.indentBy.repeat(level);
}
function isAttribute(name) {
  if (name.startsWith(this.options.attributeNamePrefix) && name !== this.options.textNodeName) {
    return name.substr(this.attrPrefixLen);
  } else {
    return false;
  }
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/xml.ts
var parser = new XMLParser({
  preserveOrder: true,
  ignoreAttributes: false,
  attributeNamePrefix: "",
  textNodeName: "#text",
  commentPropName: "#comment",
  parseTagValue: false,
  parseAttributeValue: false,
  trimValues: false
});
function parseWordPortXml(xml) {
  const parsed = parser.parse(xml);
  const document = {};
  for (const entry of parsed) {
    if ("?xml" in entry) {
      document.declaration = normalizeAttributes(entry[":@"] ?? {});
      continue;
    }
    const node = convertEntry(entry);
    if (node) {
      document.root = node;
      break;
    }
  }
  return document;
}
function serializeWordPortXml(document) {
  const parts = [];
  const declaration = document.declaration ?? { version: "1.0", encoding: "UTF-8", standalone: "yes" };
  parts.push(`<?xml${serializeAttributes(declaration)}?>`);
  if (document.root) parts.push(serializeNode(document.root));
  return parts.join("");
}
function cloneWordPortXmlNode(node) {
  if (!node) return node;
  return {
    name: node.name,
    attributes: node.attributes ? { ...node.attributes } : void 0,
    text: node.text,
    children: node.children?.map((child2) => cloneWordPortXmlNode(child2))
  };
}
function getChildren(node, name) {
  const children = node?.children ?? [];
  if (!name) return children;
  return children.filter((child2) => child2.name === name);
}
function firstChild(node, name) {
  return getChildren(node, name)[0];
}
function textContent(node) {
  if (!node) return "";
  if (node.text != null) return node.text;
  return (node.children ?? []).map((child2) => textContent(child2)).join("");
}
function findDescendants(node, predicate) {
  const result = [];
  const visit = (current) => {
    if (predicate(current)) result.push(current);
    for (const child2 of current.children ?? []) visit(child2);
  };
  if (node) visit(node);
  return result;
}
function createXmlNode(name, attributes, children) {
  const normalizedAttributes = {};
  for (const [key, value] of Object.entries(attributes ?? {})) {
    if (value != null) normalizedAttributes[key] = String(value);
  }
  return {
    name,
    ...Object.keys(normalizedAttributes).length ? { attributes: normalizedAttributes } : {},
    ...children?.length ? { children } : {}
  };
}
function createTextNode(name, text, attributes) {
  return {
    name,
    attributes: normalizeAttributes(attributes ?? {}),
    text
  };
}
function convertEntry(entry) {
  for (const [key, value] of Object.entries(entry)) {
    if (key === ":@" || key === "#text" || key === "#comment" || key === "?xml") continue;
    const children = Array.isArray(value) ? value.map((child2) => convertEntry(child2)).filter(Boolean) : [];
    const node = { name: key };
    const attributes = normalizeAttributes(entry[":@"] ?? {});
    if (Object.keys(attributes).length) node.attributes = attributes;
    if (children.length) node.children = children;
    return node;
  }
  if ("#text" in entry) {
    return { name: "#text", text: String(entry["#text"] ?? "") };
  }
  if ("#comment" in entry) {
    return { name: "#comment", text: String(entry["#comment"] ?? "") };
  }
  return void 0;
}
function normalizeAttributes(attributes) {
  const normalized = {};
  for (const [key, value] of Object.entries(attributes)) {
    if (value != null) normalized[key] = String(value);
  }
  return normalized;
}
function serializeNode(node) {
  if (node.name === "#text") return escapeText(node.text ?? "");
  if (node.name === "#comment") return `<!--${node.text ?? ""}-->`;
  const attributes = serializeAttributes(node.attributes ?? {});
  const children = node.children?.map((child2) => serializeNode(child2)).join("") ?? "";
  const text = node.text != null ? escapeText(node.text) : "";
  if (!children && !text) return `<${node.name}${attributes}/>`;
  return `<${node.name}${attributes}>${text}${children}</${node.name}>`;
}
function serializeAttributes(attributes) {
  const entries = Object.entries(attributes);
  if (!entries.length) return "";
  return ` ${entries.map(([key, value]) => `${key}="${escapeAttribute2(value)}"`).join(" ")}`;
}
function escapeText(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function escapeAttribute2(value) {
  return escapeText(value).replace(/"/g, "&quot;");
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/content-types.ts
var DEFAULT_CONTENT_TYPES_XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>';
function parseContentTypesXml(xml, diagnostics) {
  const contentTypes = {
    defaults: /* @__PURE__ */ new Map([
      ["rels", "application/vnd.openxmlformats-package.relationships+xml"],
      ["xml", "application/xml"]
    ]),
    overrides: /* @__PURE__ */ new Map(),
    xml
  };
  if (!xml) {
    diagnostics?.warning("opc.invalid-content-types", "DOCX package is missing [Content_Types].xml; defaults will be synthesized.");
    return contentTypes;
  }
  try {
    const document = parseWordPortXml(xml);
    const root = document.root;
    if (!root || localName(root.name) !== "Types") throw new Error("Missing Types root");
    for (const child2 of root.children ?? []) {
      if (child2.name === "Default" && child2.attributes?.Extension && child2.attributes.ContentType) {
        contentTypes.defaults.set(child2.attributes.Extension.toLowerCase(), child2.attributes.ContentType);
      }
      if (child2.name === "Override" && child2.attributes?.PartName && child2.attributes.ContentType) {
        contentTypes.overrides.set(normalizePartName(child2.attributes.PartName), child2.attributes.ContentType);
      }
    }
  } catch (error) {
    diagnostics?.error("opc.invalid-content-types", "Unable to parse [Content_Types].xml.", {
      path: "[Content_Types].xml",
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
  }
  return contentTypes;
}
function getContentTypeForPart(contentTypes, partPath) {
  const normalized = normalizeOpcPath(partPath);
  const override = contentTypes.overrides.get(normalized);
  if (override) return override;
  const extension = extensionForPath(normalized);
  return extension ? contentTypes.defaults.get(extension) : void 0;
}
function ensureContentTypeOverride(contentTypes, partPath, contentType) {
  contentTypes.overrides.set(normalizeOpcPath(partPath), contentType);
}
function ensureContentTypeDefault(contentTypes, extension, contentType) {
  contentTypes.defaults.set(extension.toLowerCase().replace(/^\./, ""), contentType);
}
function ensureContentTypeForPart(contentTypes, partPath, contentType) {
  const normalized = normalizeOpcPath(partPath);
  const extension = extensionForPath(normalized);
  if (extension && contentTypes.defaults.get(extension) === contentType) {
    removeContentTypeOverride(contentTypes, normalized);
    return;
  }
  ensureContentTypeOverride(contentTypes, normalized, contentType);
}
function removeContentTypeOverride(contentTypes, partPath) {
  contentTypes.overrides.delete(normalizeOpcPath(partPath));
}
function serializeContentTypesXml(contentTypes, packagePaths) {
  const packagePathSet = new Set([...packagePaths].map((path) => normalizeOpcPath(path)));
  const root = {
    name: "Types",
    attributes: { xmlns: "http://schemas.openxmlformats.org/package/2006/content-types" },
    children: []
  };
  const defaults = new Map(contentTypes.defaults);
  for (const path of packagePaths) {
    const extension = extensionForPath(path);
    if (extension && !defaults.has(extension)) defaults.set(extension, defaultContentTypeForExtension(extension));
  }
  for (const [extension, contentType] of [...defaults.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    root.children?.push({ name: "Default", attributes: { Extension: extension, ContentType: contentType } });
  }
  for (const [partName, contentType] of [...contentTypes.overrides.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    if (!packagePathSet.has(normalizeOpcPath(partName))) continue;
    root.children?.push({ name: "Override", attributes: { PartName: `/${partName}`, ContentType: contentType } });
  }
  return serializeWordPortXml({ root });
}
function reconcileContentTypesForParts(contentTypes, parts) {
  for (const part of parts) {
    const normalized = normalizeOpcPath(part.path);
    const extension = extensionForPath(normalized);
    if (!extension) continue;
    if (extension === "xml" || extension === "rels") continue;
    const reconciledContentType = part.contentType ?? defaultContentTypeForExtension(extension);
    const currentDefault = contentTypes.defaults.get(extension);
    if (!currentDefault || currentDefault === defaultContentTypeForExtension(extension)) {
      ensureContentTypeDefault(contentTypes, extension, reconciledContentType);
    }
    if (part.contentType && contentTypes.defaults.get(extension) === part.contentType) {
      removeContentTypeOverride(contentTypes, normalized);
    }
  }
}
function defaultContentTypeForExtension(extension) {
  switch (extension) {
    case "rels":
      return "application/vnd.openxmlformats-package.relationships+xml";
    case "xml":
      return "application/xml";
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "bmp":
      return "image/bmp";
    case "svg":
      return "image/svg+xml";
    default:
      return "application/octet-stream";
  }
}
function normalizePartName(partName) {
  return normalizeOpcPath(partName.replace(/^\/+/, ""));
}
function localName(name) {
  return name.includes(":") ? name.split(":").pop() : name;
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/relationships.ts
var RELATIONSHIPS_NS = "http://schemas.openxmlformats.org/package/2006/relationships";
var WORD_PORT_EXTERNAL_TARGET_MODE = "External";
function parseRelationshipsXml(xml, path, sourcePart, diagnostics) {
  const set = { path, sourcePart, relationships: [], xml };
  if (!xml) return set;
  try {
    const document = parseWordPortXml(xml);
    const root = document.root;
    if (!root || root.name !== "Relationships") throw new Error("Missing Relationships root");
    for (const child2 of root.children ?? []) {
      if (child2.name !== "Relationship") continue;
      const id = child2.attributes?.Id;
      const type = child2.attributes?.Type;
      const target = child2.attributes?.Target;
      if (!id || !type || !target) continue;
      set.relationships.push({
        id,
        type,
        target,
        targetMode: child2.attributes?.TargetMode,
        sourcePart,
        resolvedTarget: resolveWordPortRelationshipTarget({ sourcePart }, { target, targetMode: child2.attributes?.TargetMode })
      });
    }
  } catch (error) {
    diagnostics?.error("opc.invalid-relationships", "Unable to parse OPC relationships.", {
      path,
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
  }
  return set;
}
function serializeRelationshipsXml(relationships) {
  const root = {
    name: "Relationships",
    attributes: { xmlns: RELATIONSHIPS_NS },
    children: relationships.relationships.map((relationship) => ({
      name: "Relationship",
      attributes: {
        Id: relationship.id,
        Type: relationship.type,
        Target: relationship.target,
        ...relationship.targetMode ? { TargetMode: relationship.targetMode } : {}
      }
    }))
  };
  return serializeWordPortXml({ root });
}
function getRelationshipById(set, id) {
  if (!set || !id) return void 0;
  return set.relationships.find((relationship) => relationship.id === id);
}
function getRelationshipsPathForPart(partPath) {
  return relationshipsPathForPart(normalizeOpcPath(partPath));
}
function relationshipTargetForPart(sourcePart, targetPart) {
  const sourceSegments = normalizeOpcPath(sourcePart).split("/");
  sourceSegments.pop();
  const targetSegments = normalizeOpcPath(targetPart).split("/");
  while (sourceSegments.length && targetSegments.length && sourceSegments[0] === targetSegments[0]) {
    sourceSegments.shift();
    targetSegments.shift();
  }
  return `${"../".repeat(sourceSegments.length)}${targetSegments.join("/")}`;
}
function isExternalWordPortRelationship(relationship) {
  return relationship.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE;
}
function resolveWordPortRelationshipTarget(set, relationship) {
  if (!relationship.target || isExternalWordPortRelationship(relationship)) return void 0;
  return set.sourcePart ? resolveOpcTargetPath(set.sourcePart, relationship.target) : normalizeOpcPath(relationship.target);
}
function refreshWordPortRelationshipResolution(set, relationship) {
  relationship.sourcePart = set.sourcePart;
  relationship.resolvedTarget = resolveWordPortRelationshipTarget(set, relationship);
}
function nextRelationshipId(relationships) {
  let max = 0;
  for (const relationship of relationships) {
    const match = relationship.id.match(/^rId(\d+)$/);
    if (match) max = Math.max(max, Number(match[1]));
  }
  return `rId${max + 1}`;
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/managed-parts.ts
var WORD_PORT_RELATIONSHIP_TYPES = {
  officeDocument: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
  coreProperties: "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
  extendedProperties: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
  customProperties: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties",
  comments: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
  commentsExtended: "http://schemas.microsoft.com/office/2011/relationships/commentsExtended",
  commentsIds: "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds",
  footnotes: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
  endnotes: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes",
  footer: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer",
  header: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
  hyperlink: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
  image: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
  numbering: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
  settings: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings",
  styles: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles"
};
var WORD_PORT_CONTENT_TYPES = {
  appProperties: "application/vnd.openxmlformats-officedocument.extended-properties+xml",
  comments: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
  commentsExtended: "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml",
  commentsIds: "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsIds+xml",
  coreProperties: "application/vnd.openxmlformats-package.core-properties+xml",
  customProperties: "application/vnd.openxmlformats-officedocument.custom-properties+xml",
  endnotes: "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
  footer: "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
  footnotes: "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
  header: "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
  mainDocument: "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
  numbering: "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
  settings: "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
  styles: "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
};
var WORD_PORT_MANAGED_RELATED_PARTS = {
  comments: {
    path: "word/comments.xml",
    contentType: WORD_PORT_CONTENT_TYPES.comments,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.comments,
    sourcePart: "word/document.xml"
  },
  commentsExtended: {
    path: "word/commentsExtended.xml",
    contentType: WORD_PORT_CONTENT_TYPES.commentsExtended,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.commentsExtended,
    sourcePart: "word/document.xml"
  },
  commentsIds: {
    path: "word/commentsIds.xml",
    contentType: WORD_PORT_CONTENT_TYPES.commentsIds,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.commentsIds,
    sourcePart: "word/document.xml"
  },
  endnotes: {
    path: "word/endnotes.xml",
    contentType: WORD_PORT_CONTENT_TYPES.endnotes,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.endnotes,
    sourcePart: "word/document.xml"
  },
  footnotes: {
    path: "word/footnotes.xml",
    contentType: WORD_PORT_CONTENT_TYPES.footnotes,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.footnotes,
    sourcePart: "word/document.xml"
  },
  numbering: {
    path: "word/numbering.xml",
    contentType: WORD_PORT_CONTENT_TYPES.numbering,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.numbering,
    sourcePart: "word/document.xml"
  },
  settings: {
    path: "word/settings.xml",
    contentType: WORD_PORT_CONTENT_TYPES.settings,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.settings,
    sourcePart: "word/document.xml"
  },
  styles: {
    path: "word/styles.xml",
    contentType: WORD_PORT_CONTENT_TYPES.styles,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.styles,
    sourcePart: "word/document.xml"
  }
};
var WORD_PORT_MANAGED_PACKAGE_PARTS = [
  {
    path: "word/document.xml",
    contentType: WORD_PORT_CONTENT_TYPES.mainDocument,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.officeDocument
  },
  {
    path: "docProps/core.xml",
    contentType: WORD_PORT_CONTENT_TYPES.coreProperties,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.coreProperties
  },
  {
    path: "docProps/app.xml",
    contentType: WORD_PORT_CONTENT_TYPES.appProperties,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.extendedProperties
  },
  {
    path: "docProps/custom.xml",
    contentType: WORD_PORT_CONTENT_TYPES.customProperties,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.customProperties
  }
];
var WORD_PORT_MANAGED_PART_REGISTRY = [
  ...WORD_PORT_MANAGED_PACKAGE_PARTS,
  ...Object.values(WORD_PORT_MANAGED_RELATED_PARTS)
];
function deleteWordPortManagedPart(wordPackage, options) {
  const path = normalizeOpcPath(options.path);
  wordPackage.deletePart(path);
  removeContentTypeOverride(wordPackage.contentTypes, path);
  deleteWordPortRelationships(wordPackage, { ...options, targetPart: path });
  const relsPath = relationshipsPathForPart(path);
  if (wordPackage.relationships.delete(relsPath)) {
    wordPackage.deletePart(relsPath);
    pushPackageWarning(wordPackage, "Removed relationships for deleted part.", {
      path: relsPath,
      source: options.diagnosticSource ?? "opc.delete-part",
      detail: { deletedPart: path }
    });
  }
}
function reconcileWordPortPackageParts(wordPackage) {
  ensureContentTypeDefault(wordPackage.contentTypes, "rels", "application/vnd.openxmlformats-package.relationships+xml");
  ensureContentTypeDefault(wordPackage.contentTypes, "xml", "application/xml");
  for (const entry of WORD_PORT_MANAGED_PACKAGE_PARTS) reconcileManagedEntry(wordPackage, entry);
  for (const entry of Object.values(WORD_PORT_MANAGED_RELATED_PARTS)) reconcileManagedEntry(wordPackage, entry);
  reconcileContentTypesForParts(wordPackage.contentTypes, wordPackage.parts.values());
  for (const path of [...wordPackage.contentTypes.overrides.keys()]) {
    const normalized = normalizeOpcPath(path);
    if (wordPackage.parts.has(normalized)) continue;
    removeContentTypeOverride(wordPackage.contentTypes, normalized);
    pushPackageWarning(wordPackage, "Content type override pointed at a missing part and was removed.", {
      path: "[Content_Types].xml",
      source: "opc.content-types",
      detail: { missingPart: normalized }
    });
  }
  deleteStaleWordPortRelationships(wordPackage);
  dedupeRelationshipIds(wordPackage);
}
function detectDanglingInternalRelationships(wordPackage) {
  const dangling = [];
  for (const set of wordPackage.relationships.values()) {
    for (const relationship of set.relationships) {
      if (isExternalWordPortRelationship(relationship)) continue;
      const resolvedTarget = resolveRelationshipTarget(set, relationship);
      if (!resolvedTarget || wordPackage.parts.has(resolvedTarget)) continue;
      dangling.push({
        relationshipsPath: set.path,
        sourcePart: set.sourcePart,
        relationshipId: relationship.id,
        relationshipType: relationship.type,
        target: relationship.target,
        resolvedTarget
      });
    }
  }
  return dangling;
}
function deleteStaleWordPortRelationships(wordPackage) {
  for (const [relsPath, set] of [...wordPackage.relationships.entries()]) {
    if (!set.sourcePart || wordPackage.parts.has(set.sourcePart)) continue;
    wordPackage.relationships.delete(relsPath);
    wordPackage.deletePart(relsPath);
    pushPackageWarning(wordPackage, "Relationships for a missing source part could not be preserved and were removed.", {
      path: relsPath,
      source: "opc.relationships",
      detail: { sourcePart: set.sourcePart }
    });
  }
  const dangling = detectDanglingInternalRelationships(wordPackage);
  for (const relationship of dangling) {
    const set = wordPackage.relationships.get(relationship.relationshipsPath);
    if (!set) continue;
    set.relationships = set.relationships.filter((candidate) => candidate.id !== relationship.relationshipId);
    pushPackageWarning(wordPackage, "Internal relationship target was missing and the relationship was removed.", {
      path: relationship.relationshipsPath,
      source: "opc.relationships",
      detail: relationship
    });
  }
  return dangling;
}
function ensureWordPortRelationship(wordPackage, options) {
  const targetPart = normalizeOpcPath(options.targetPart);
  const sourcePart = options.sourcePart ? normalizeOpcPath(options.sourcePart) : void 0;
  const set = ensureRelationshipSet(wordPackage, sourcePart);
  const target = options.target ?? (sourcePart ? relationshipTargetForPart(sourcePart, targetPart) : targetPart);
  const resolvedTarget = options.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE ? void 0 : sourcePart ? resolveOpcTargetPath(sourcePart, target) : normalizeOpcPath(target);
  const existing = set.relationships.find(
    (relationship) => relationship.type === options.relationshipType && (relationship.id === options.relationshipId || relationship.target === target || !isExternalWordPortRelationship(relationship) && resolveRelationshipTarget(set, relationship) === resolvedTarget)
  );
  if (existing) {
    existing.target = target;
    existing.resolvedTarget = resolvedTarget;
    existing.sourcePart = sourcePart;
    if (options.targetMode) existing.targetMode = options.targetMode;
    else delete existing.targetMode;
    return existing.id;
  }
  const id = options.relationshipId && !set.relationships.some((relationship) => relationship.id === options.relationshipId) ? options.relationshipId : nextRelationshipId(set.relationships);
  set.relationships.push({
    id,
    type: options.relationshipType,
    target,
    ...options.targetMode ? { targetMode: options.targetMode } : {},
    sourcePart,
    resolvedTarget
  });
  return id;
}
function reconcileManagedEntry(wordPackage, entry) {
  const path = normalizeOpcPath(entry.path);
  if (!wordPackage.parts.has(path)) {
    removeContentTypeOverride(wordPackage.contentTypes, path);
    if (entry.relationshipType) deleteWordPortRelationships(wordPackage, { ...entry, targetPart: path });
    return;
  }
  const contentType = entry.contentType ?? inferManagedContentType(path);
  if (contentType) ensureContentTypeForPart(wordPackage.contentTypes, path, contentType);
  if (entry.relationshipType) reconcileManagedRelationship(wordPackage, { ...entry, path });
}
function ensureRelationshipSet(wordPackage, sourcePart) {
  if (!sourcePart) {
    let root = wordPackage.relationships.get("_rels/.rels") ?? wordPackage.rootRelationships;
    if (!root) root = { path: "_rels/.rels", relationships: [] };
    root.path = "_rels/.rels";
    root.sourcePart = void 0;
    wordPackage.rootRelationships = root;
    wordPackage.relationships.set("_rels/.rels", root);
    return root;
  }
  const relsPath = relationshipsPathForPart(sourcePart);
  let set = wordPackage.relationships.get(relsPath);
  if (!set) {
    set = { path: relsPath, sourcePart, relationships: [] };
    wordPackage.relationships.set(relsPath, set);
  }
  return set;
}
function deleteWordPortRelationships(wordPackage, options) {
  const normalizedTarget = options.targetPart ? normalizeOpcPath(options.targetPart) : void 0;
  for (const set of wordPackage.relationships.values()) {
    if (options.sourcePart && set.sourcePart !== normalizeOpcPath(options.sourcePart)) continue;
    set.relationships = set.relationships.filter((relationship) => {
      if (options.relationshipId && relationship.id !== options.relationshipId) return true;
      if (options.relationshipType && relationship.type !== options.relationshipType) return true;
      if (!normalizedTarget) return false;
      if (isExternalWordPortRelationship(relationship)) return true;
      return resolveRelationshipTarget(set, relationship) !== normalizedTarget;
    });
  }
}
function resolveRelationshipTarget(set, relationship) {
  return resolveWordPortRelationshipTarget(set, relationship);
}
function inferManagedContentType(path) {
  if (/^word\/header\d+\.xml$/i.test(path)) return WORD_PORT_CONTENT_TYPES.header;
  if (/^word\/footer\d+\.xml$/i.test(path)) return WORD_PORT_CONTENT_TYPES.footer;
  for (const entry of WORD_PORT_MANAGED_PART_REGISTRY) {
    if (normalizeOpcPath(entry.path) === path) return entry.contentType;
  }
  return void 0;
}
function reconcileManagedRelationship(wordPackage, entry) {
  if (!entry.relationshipType) throw new Error("reconcileManagedRelationship requires a relationship type.");
  const sourcePart = entry.sourcePart ? normalizeOpcPath(entry.sourcePart) : void 0;
  const targetPart = normalizeOpcPath(entry.path);
  const set = ensureRelationshipSet(wordPackage, sourcePart);
  const target = entry.target ?? (sourcePart ? relationshipTargetForPart(sourcePart, targetPart) : targetPart);
  const resolvedTarget = entry.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE ? void 0 : sourcePart ? resolveOpcTargetPath(sourcePart, target) : normalizeOpcPath(target);
  const existing = set.relationships.filter((relationship) => relationship.type === entry.relationshipType);
  if (existing.length) {
    const keep = existing[0];
    keep.target = target;
    keep.sourcePart = sourcePart;
    keep.resolvedTarget = resolvedTarget;
    if (entry.targetMode) keep.targetMode = entry.targetMode;
    else delete keep.targetMode;
    set.relationships = set.relationships.filter(
      (relationship) => relationship === keep || relationship.type !== entry.relationshipType
    );
    return keep.id;
  }
  return ensureWordPortRelationship(wordPackage, {
    sourcePart,
    targetPart,
    relationshipType: entry.relationshipType,
    target,
    targetMode: entry.targetMode
  });
}
function dedupeRelationshipIds(wordPackage) {
  for (const set of wordPackage.relationships.values()) {
    const seen = /* @__PURE__ */ new Set();
    for (const relationship of set.relationships) {
      if (!seen.has(relationship.id)) {
        seen.add(relationship.id);
        refreshWordPortRelationshipResolution(set, relationship);
        continue;
      }
      const oldId = relationship.id;
      relationship.id = nextRelationshipId(set.relationships);
      seen.add(relationship.id);
      refreshWordPortRelationshipResolution(set, relationship);
      pushPackageWarning(wordPackage, "Duplicate relationship id was replaced with a new id.", {
        path: set.path,
        source: "opc.relationships",
        detail: { oldRelationshipId: oldId, newRelationshipId: relationship.id, relationshipType: relationship.type }
      });
    }
  }
}
function pushPackageWarning(wordPackage, message, detail) {
  wordPackage.diagnostics.push({ severity: "warning", code: "opc.missing-part", message, ...detail });
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/validation.ts
function validateWordPortPackage(wordPackage) {
  return [
    ...validateRelationshipSets(wordPackage),
    ...validateContentTypes(wordPackage)
  ];
}
function validateRelationshipSets(wordPackage) {
  const diagnostics = [];
  for (const set of wordPackage.relationships.values()) {
    const seenIds = /* @__PURE__ */ new Map();
    for (const relationship of set.relationships) {
      const previousType = seenIds.get(relationship.id);
      if (previousType) {
        diagnostics.push({
          severity: "error",
          code: "opc.duplicate-relationship-id",
          message: "Relationship id is duplicated within a relationships part.",
          path: set.path,
          source: "opc.validation",
          detail: { relationshipId: relationship.id, previousType, relationshipType: relationship.type }
        });
      } else {
        seenIds.set(relationship.id, relationship.type);
      }
      if (!relationship.target) {
        diagnostics.push({
          severity: "error",
          code: "opc.invalid-relationships",
          message: "Relationship is missing a Target value.",
          path: set.path,
          source: "opc.validation",
          detail: { relationshipId: relationship.id, relationshipType: relationship.type }
        });
        continue;
      }
      if (relationship.targetMode !== void 0 && relationship.targetMode !== WORD_PORT_EXTERNAL_TARGET_MODE) {
        diagnostics.push({
          severity: "error",
          code: "opc.invalid-target-mode",
          message: "Relationship TargetMode must be External when present.",
          path: set.path,
          source: "opc.validation",
          detail: {
            relationshipId: relationship.id,
            relationshipType: relationship.type,
            targetMode: relationship.targetMode
          }
        });
      }
      if (isExternalWordPortRelationship(relationship)) continue;
      const resolvedTarget = resolveWordPortRelationshipTarget(set, relationship);
      if (!resolvedTarget || wordPackage.parts.has(resolvedTarget)) continue;
      diagnostics.push({
        severity: "error",
        code: "opc.dangling-relationship",
        message: "Internal relationship target is missing from the package.",
        path: set.path,
        source: "opc.validation",
        detail: {
          relationshipId: relationship.id,
          relationshipType: relationship.type,
          target: relationship.target,
          resolvedTarget
        }
      });
    }
  }
  return diagnostics;
}
function validateContentTypes(wordPackage) {
  const diagnostics = [];
  const managedContentTypes = new Map(
    WORD_PORT_MANAGED_PART_REGISTRY.filter((entry) => entry.contentType).map((entry) => [normalizeOpcPath(entry.path), entry.contentType])
  );
  for (const part of wordPackage.parts.values()) {
    if (part.path === "[Content_Types].xml") continue;
    const contentType = getContentTypeForPart(wordPackage.contentTypes, part.path);
    const managedContentType = managedContentTypes.get(normalizeOpcPath(part.path));
    if (managedContentType && contentType !== managedContentType) {
      diagnostics.push({
        severity: "error",
        code: "opc.missing-content-type",
        message: "Managed part is missing its required content type override.",
        path: "[Content_Types].xml",
        source: "opc.validation",
        detail: { partPath: part.path, expectedContentType: managedContentType, actualContentType: contentType }
      });
      continue;
    }
    if (!contentType) {
      diagnostics.push({
        severity: "error",
        code: "opc.missing-content-type",
        message: "Package part has no matching content type override or default.",
        path: "[Content_Types].xml",
        source: "opc.validation",
        detail: { partPath: part.path }
      });
    }
  }
  return diagnostics;
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/package/package.ts
var TEXT_DECODER = new TextDecoder();
var TEXT_ENCODER = new TextEncoder();
async function openWordPortPackage(input, options = {}) {
  const diagnostics = new WordPortDiagnostics();
  diagnostics.merge(options.diagnostics);
  const zip = await jszip_default.loadAsync(input);
  const parts = /* @__PURE__ */ new Map();
  const entries = Object.values(zip.files).filter((entry) => !entry.dir);
  for (const entry of entries) {
    const path = normalizeOpcPath(entry.name);
    const data = await entry.async("uint8array");
    const part = { path, data };
    if (isXmlLike(path)) part.text = decodeXml(data);
    parts.set(path, part);
  }
  const contentTypesXml = parts.get("[Content_Types].xml")?.text ?? DEFAULT_CONTENT_TYPES_XML;
  const contentTypes = parseContentTypesXml(contentTypesXml, diagnostics);
  for (const part of parts.values()) part.contentType = getContentTypeForPart(contentTypes, part.path);
  const relationships = /* @__PURE__ */ new Map();
  const rootRels = parseRelationshipsXml(parts.get("_rels/.rels")?.text, "_rels/.rels", void 0, diagnostics);
  relationships.set("_rels/.rels", rootRels);
  for (const part of parts.values()) {
    if (!part.path.endsWith(".rels") || part.path === "_rels/.rels") continue;
    const sourcePart = sourcePartPathForRelationships(part.path);
    const relationshipSet = parseRelationshipsXml(part.text, part.path, sourcePart, diagnostics);
    relationships.set(part.path, relationshipSet);
  }
  for (const part of parts.values()) {
    if (part.path === "[Content_Types].xml" || part.path.endsWith(".rels")) continue;
    diagnostics.info("opc.preserved-part", "Package part loaded for preservation.", {
      path: part.path,
      detail: { contentType: part.contentType, bytes: part.data.byteLength }
    });
  }
  return createWordPortPackage(parts, contentTypes, relationships, rootRels, diagnostics.items);
}
async function writeWordPortPackage(wordPackage, options = {}) {
  const zip = new jszip_default();
  reconcileWordPortPackageParts(wordPackage);
  for (const relationshipSet of wordPackage.relationships.values()) {
    wordPackage.setText(
      relationshipSet.path,
      serializeRelationshipsXml(relationshipSet)
    );
  }
  const packagePaths = new Set(wordPackage.parts.keys());
  const contentTypesXml = serializeContentTypesXml(wordPackage.contentTypes, packagePaths);
  wordPackage.setText("[Content_Types].xml", contentTypesXml);
  wordPackage.diagnostics.push(...validateWordPortPackage(wordPackage));
  for (const part of wordPackage.parts.values()) {
    const data = part.changed && part.text != null ? TEXT_ENCODER.encode(part.text) : part.data;
    zip.file(part.path, data);
  }
  return zip.generateAsync({
    type: "uint8array",
    compression: options.compression ?? "DEFLATE"
  });
}
function createWordPortPackage(parts, contentTypes, relationships, rootRelationships, diagnostics = []) {
  const wordPackage = {
    parts,
    contentTypes,
    relationships,
    rootRelationships,
    diagnostics,
    getPart(path) {
      return parts.get(normalizeOpcPath(path));
    },
    getText(path) {
      return parts.get(normalizeOpcPath(path))?.text;
    },
    setText(path, text, contentType) {
      const normalized = normalizeOpcPath(path);
      const data = TEXT_ENCODER.encode(text);
      const existing = parts.get(normalized);
      const part = existing ? { ...existing, text, data, changed: true, contentType: contentType ?? existing.contentType } : { path: normalized, text, data, changed: true, contentType };
      parts.set(normalized, part);
      if (contentType) ensureContentTypeForPart(contentTypes, normalized, contentType);
    },
    setBinary(path, data, contentType) {
      const normalized = normalizeOpcPath(path);
      const existing = parts.get(normalized);
      const part = existing ? { ...existing, data, text: void 0, changed: true, contentType: contentType ?? existing.contentType } : { path: normalized, data, changed: true, contentType };
      parts.set(normalized, part);
      if (contentType) ensureContentTypeForPart(contentTypes, normalized, contentType);
    },
    deletePart(path) {
      parts.delete(normalizeOpcPath(path));
    },
    getRelationships(path) {
      const normalized = normalizeOpcPath(path);
      const relsPath = normalized.endsWith(".rels") ? normalized : getRelationshipsPathForPart(normalized);
      return relationships.get(relsPath);
    }
  };
  return wordPackage;
}
function isXmlLike(path) {
  return path.endsWith(".xml") || path.endsWith(".rels");
}
function decodeXml(data) {
  if (data.length >= 2) {
    if (data[0] === 255 && data[1] === 254) return new TextDecoder("utf-16le").decode(data.subarray(2));
    if (data[0] === 254 && data[1] === 255) return new TextDecoder("utf-16be").decode(data.subarray(2));
  }
  return TEXT_DECODER.decode(data);
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/part-mutation.ts
var descriptors = /* @__PURE__ */ new Map();
function getWordPortPartDescriptor(partId) {
  return descriptors.get(normalizeOpcPath(partId));
}
function getWordPortPartMutationRevision(wordPackage) {
  return wordPackage._wordPortMutationRevision ?? 0;
}
function mutateWordPortParts(request) {
  const dryRun = request.dryRun ?? false;
  const diagnostics = [];
  if (!request.operations.length) return { changed: false, degraded: false, parts: [], diagnostics };
  assertRevision(request.wordPackage, request.expectedRevision);
  const rollbacks = [];
  const outcomes = [];
  try {
    for (const operation of request.operations) {
      const op = { ...operation, dryRun };
      if (op.operation === "mutate") outcomes.push(executeMutate(op, rollbacks));
      else if (op.operation === "create") outcomes.push(executeCreate(op, rollbacks));
      else outcomes.push(executeDelete(op, rollbacks));
    }
  } catch (error) {
    rollback(request.wordPackage, rollbacks);
    throw error;
  }
  const changed = outcomes.some((outcome) => outcome.changed);
  if (dryRun || !changed) rollback(request.wordPackage, rollbacks);
  let degraded = false;
  if (!dryRun && changed) {
    incrementRevision(request.wordPackage);
    degraded = runAfterCommitHooks(request, outcomes, diagnostics);
  }
  return {
    changed,
    degraded,
    parts: outcomes.map(({ partId, operation, changed: changed2, changedPaths }) => ({ partId, operation, changed: changed2, changedPaths })),
    diagnostics
  };
}
function mutateWordPortPart(request) {
  let callbackResult;
  const operation = request.operation === "mutate" ? {
    ...request,
    mutate(context) {
      callbackResult = request.mutate(context);
      return callbackResult;
    }
  } : request;
  const result = mutateWordPortParts({
    wordPackage: request.wordPackage,
    source: request.source,
    dryRun: request.dryRun,
    operations: [operation]
  });
  const part = result.parts[0];
  return {
    changed: part?.changed ?? false,
    changedPaths: part?.changedPaths ?? [],
    degraded: result.degraded,
    result: callbackResult,
    diagnostics: result.diagnostics
  };
}
function executeMutate(op, rollbacks) {
  const partId = normalizeOpcPath(op.partId);
  const descriptor = getWordPortPartDescriptor(partId);
  const existedBefore = op.wordPackage.parts.has(partId);
  if (!existedBefore) {
    if (!descriptor?.ensurePart) throw new Error(`mutateWordPortPart: part "${partId}" does not exist.`);
    op.wordPackage.parts.set(partId, clonePart(descriptor.ensurePart(lifecycleContext(op, partId))));
  }
  const part = op.wordPackage.getPart(partId);
  if (!part) throw new Error(`mutateWordPortPart: part "${partId}" could not be loaded.`);
  const snapshot = clonePart(part);
  rollbacks.push({ partId, operation: existedBefore ? "mutate" : "create", snapshot: existedBefore ? snapshot : void 0 });
  const callbackResult = op.mutate({ part, dryRun: op.dryRun ?? false });
  normalizeAndValidatePart(op.wordPackage, partId, descriptor);
  const changedPaths = diffWordPortPartPaths(snapshot, op.wordPackage.getPart(partId));
  return { partId, operation: "mutate", changed: changedPaths.length > 0, changedPaths, callbackResult };
}
function executeCreate(op, rollbacks) {
  const partId = normalizeOpcPath(op.partId);
  if (op.wordPackage.parts.has(partId)) throw new Error(`mutateWordPortPart: part "${partId}" already exists.`);
  rollbacks.push({ partId, operation: "create" });
  const descriptor = getWordPortPartDescriptor(partId);
  op.wordPackage.parts.set(partId, clonePart(op.initial));
  normalizeAndValidatePart(op.wordPackage, partId, descriptor);
  return { partId, operation: "create", changed: true, changedPaths: [] };
}
function executeDelete(op, rollbacks) {
  const partId = normalizeOpcPath(op.partId);
  const part = op.wordPackage.getPart(partId);
  if (!part) throw new Error(`mutateWordPortPart: part "${partId}" does not exist.`);
  rollbacks.push({ partId, operation: "delete", snapshot: clonePart(part) });
  const descriptor = getWordPortPartDescriptor(partId);
  descriptor?.onDelete?.({ ...lifecycleContext(op, partId), part });
  op.wordPackage.deletePart(partId);
  return { partId, operation: "delete", changed: true, changedPaths: [] };
}
function normalizeAndValidatePart(wordPackage, partId, descriptor) {
  const part = wordPackage.getPart(partId);
  if (!part) return;
  const normalized = descriptor?.normalizePart?.(part);
  if (normalized && normalized !== part) wordPackage.parts.set(partId, normalized);
  descriptor?.validatePart?.(wordPackage.getPart(partId) ?? part);
}
function rollback(wordPackage, entries) {
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index];
    if (entry.operation === "create") wordPackage.deletePart(entry.partId);
    else if (entry.snapshot) wordPackage.parts.set(entry.partId, clonePart(entry.snapshot));
  }
}
function runAfterCommitHooks(request, outcomes, diagnostics) {
  let degraded = false;
  for (let index = 0; index < outcomes.length; index++) {
    const outcome = outcomes[index];
    if (!outcome.changed || outcome.operation === "delete") continue;
    const descriptor = getWordPortPartDescriptor(outcome.partId);
    const part = request.wordPackage.getPart(outcome.partId);
    if (!descriptor?.afterCommit || !part) continue;
    try {
      descriptor.afterCommit({ ...lifecycleContext(request.operations[index], outcome.partId), part });
    } catch (error) {
      degraded = true;
      diagnostics.push({
        severity: "warning",
        code: "opc.missing-part",
        message: "Part afterCommit hook failed; mutation was committed but marked degraded.",
        path: outcome.partId,
        source: request.source,
        detail: { error: error instanceof Error ? error.message : String(error) }
      });
    }
  }
  return degraded;
}
function assertRevision(wordPackage, expectedRevision) {
  if (expectedRevision == null) return;
  const actual = getWordPortPartMutationRevision(wordPackage);
  if (actual !== expectedRevision) {
    throw new Error(`mutateWordPortParts: expected revision ${expectedRevision}, got ${actual}.`);
  }
}
function incrementRevision(wordPackage) {
  const state = wordPackage;
  state._wordPortMutationRevision = (state._wordPortMutationRevision ?? 0) + 1;
}
function lifecycleContext(operation, partId) {
  return {
    wordPackage: operation.wordPackage,
    partId,
    source: operation.source,
    sectionId: operation.sectionId
  };
}
function clonePart(part) {
  return {
    ...part,
    data: new Uint8Array(part.data)
  };
}
function diffWordPortPartPaths(before, after) {
  if (!before && !after) return [];
  if (!before || !after) return [""];
  const changed = [];
  if (before.path !== after.path) changed.push("path");
  if (before.contentType !== after.contentType) changed.push("contentType");
  if (before.text !== after.text) changed.push("text");
  if (before.changed !== after.changed) changed.push("changed");
  if (!equalBytes(before.data, after.data)) changed.push("data");
  return changed;
}
function equalBytes(left, right) {
  if (left.byteLength !== right.byteLength) return false;
  for (let index = 0; index < left.byteLength; index++) {
    if (left[index] !== right[index]) return false;
  }
  return true;
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/relationship-mutation.ts
function findOrCreateWordPortRelationship(wordPackage, options) {
  const diagnostics = [];
  const sourcePart = options.sourcePart ? normalizeOpcPath(options.sourcePart) : void 0;
  const set = ensureWordPortRelationshipSet(wordPackage, sourcePart);
  const target = resolveRelationshipTargetForMutation(sourcePart, options);
  const targetPart = options.targetPart ? normalizeOpcPath(options.targetPart) : void 0;
  const resolvedTarget = options.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE ? void 0 : sourcePart ? resolveOpcTargetPath(sourcePart, target) : normalizeOpcPath(target);
  const existing = findReusableRelationship(set, {
    relationshipType: options.relationshipType,
    target,
    targetPart,
    resolvedTarget,
    relationshipId: options.relationshipId
  });
  if (existing) {
    const changed = reconcileExistingRelationship(set, existing, {
      sourcePart,
      target,
      targetMode: options.targetMode,
      resolvedTarget
    });
    return { relationshipId: existing.id, relationship: existing, changed, diagnostics };
  }
  const relationshipId = allocateWordPortRelationshipId(set, options.relationshipId);
  const relationship = {
    id: relationshipId,
    type: options.relationshipType,
    target,
    ...options.targetMode ? { targetMode: options.targetMode } : {},
    sourcePart,
    resolvedTarget
  };
  set.relationships.push(relationship);
  syncRelationshipPart(wordPackage, set);
  return { relationshipId, relationship, changed: true, diagnostics };
}
function ensureWordPortRelationshipSet(wordPackage, sourcePart) {
  if (!sourcePart) {
    let set2 = wordPackage.relationships.get("_rels/.rels") ?? wordPackage.rootRelationships;
    if (!set2) set2 = { path: "_rels/.rels", relationships: [] };
    set2.path = "_rels/.rels";
    set2.sourcePart = void 0;
    wordPackage.rootRelationships = set2;
    wordPackage.relationships.set("_rels/.rels", set2);
    return set2;
  }
  const normalizedSource = normalizeOpcPath(sourcePart);
  const relsPath = relationshipsPathForPart(normalizedSource);
  let set = wordPackage.relationships.get(relsPath);
  if (!set) {
    set = { path: relsPath, sourcePart: normalizedSource, relationships: [] };
    wordPackage.relationships.set(relsPath, set);
  }
  return set;
}
function allocateWordPortRelationshipId(set, preferredId) {
  if (preferredId && !set.relationships.some((relationship) => relationship.id === preferredId)) return preferredId;
  return nextRelationshipId(set.relationships);
}
function normalizeWordPortRelationshipTarget(sourcePart, target) {
  if (!sourcePart) return normalizeOpcPath(target);
  const normalizedTarget = normalizeOpcPath(target);
  return normalizedTarget.startsWith("word/") ? relationshipTargetForPart(sourcePart, normalizedTarget) : target;
}
function resolveRelationshipTargetForMutation(sourcePart, options) {
  if (options.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE) return options.target ?? options.targetPart ?? "";
  if (options.target) return normalizeWordPortRelationshipTarget(sourcePart, options.target);
  if (!options.targetPart) return "";
  const targetPart = normalizeOpcPath(options.targetPart);
  return sourcePart ? relationshipTargetForPart(sourcePart, targetPart) : targetPart;
}
function findReusableRelationship(set, input) {
  return set.relationships.find((relationship) => {
    if (relationship.type !== input.relationshipType) return false;
    if (input.relationshipId && relationship.id === input.relationshipId) return true;
    if (relationship.target === input.target) return true;
    if (input.targetPart && relationship.target === input.targetPart) return true;
    return Boolean(input.resolvedTarget && relationship.resolvedTarget === input.resolvedTarget);
  });
}
function reconcileExistingRelationship(set, relationship, input) {
  const before = JSON.stringify(relationship);
  relationship.sourcePart = input.sourcePart;
  relationship.target = input.target;
  relationship.resolvedTarget = input.resolvedTarget;
  if (input.targetMode) relationship.targetMode = input.targetMode;
  else delete relationship.targetMode;
  refreshWordPortRelationshipResolution(set, relationship);
  return before !== JSON.stringify(relationship);
}
function syncRelationshipPart(wordPackage, set) {
  const existing = wordPackage.getPart(set.path);
  if (existing) return;
  wordPackage.setText(set.path, "", "application/vnd.openxmlformats-package.relationships+xml");
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/context.ts
var TEXT_ENCODER2 = new TextEncoder();
function resolveMediaReference(context, relationshipId) {
  const relationship = getRelationshipById(context.relationships, relationshipId);
  const resolvedPath = relationship?.resolvedTarget;
  return {
    relationshipId,
    target: relationship?.target,
    resolvedPath,
    relationshipType: relationship?.type,
    contentType: resolvedPath ? getContentTypeForPart(context.package.contentTypes, resolvedPath) : void 0
  };
}
function upsertWordPortExportManagedPart(context, options) {
  if (!context.package) {
    context.diagnostics.warning("ooxml.export-partial", "Cannot upsert related part without an export package.", {
      path: options.path,
      source: options.diagnosticSource ?? "opc.managed-part"
    });
    return void 0;
  }
  mutateWordPortExportManagedPart(context.package, options);
  if (!options.relationshipType) return void 0;
  const relationship = findOrCreateWordPortRelationship(context.package, {
    sourcePart: options.sourcePart,
    targetPart: options.path,
    target: options.target,
    relationshipType: options.relationshipType,
    targetMode: options.targetMode,
    relationshipId: options.relationshipId,
    diagnosticSource: options.diagnosticSource ?? "ooxml.export"
  });
  context.diagnostics.merge(relationship.diagnostics);
  return relationship.relationshipId;
}
function ensureWordPortExportExternalRelationship(context, options) {
  if (!context.package) {
    context.diagnostics.warning("ooxml.export-partial", "Cannot create external relationship without an export package.", {
      path: context.partPath,
      source: options.diagnosticSource ?? "opc.relationship-mutation",
      detail: { target: options.target }
    });
    return void 0;
  }
  const relationship = findOrCreateWordPortRelationship(context.package, {
    sourcePart: context.partPath,
    target: options.target,
    targetMode: "External",
    relationshipType: options.relationshipType,
    relationshipId: options.relationshipId,
    diagnosticSource: options.diagnosticSource ?? "ooxml.export"
  });
  context.diagnostics.merge(relationship.diagnostics);
  return relationship.relationshipId;
}
function mutateWordPortExportManagedPart(wordPackage, options) {
  const existing = wordPackage.getPart(options.path);
  const nextText = options.text;
  const nextData = options.data ?? (nextText != null ? TEXT_ENCODER2.encode(nextText) : void 0);
  if (!existing && nextData == null && options.contentType) {
    return;
  }
  if (!existing) {
    mutateWordPortPart({
      operation: "create",
      wordPackage,
      partId: options.path,
      source: options.diagnosticSource ?? "ooxml.export-managed-part",
      initial: {
        path: options.path,
        data: nextData ?? new Uint8Array(),
        ...nextText != null ? { text: nextText } : {},
        contentType: options.contentType,
        changed: true
      }
    });
    return;
  }
  mutateWordPortPart({
    operation: "mutate",
    wordPackage,
    partId: options.path,
    source: options.diagnosticSource ?? "ooxml.export-managed-part",
    mutate({ part }) {
      if (nextText != null) {
        part.text = nextText;
        part.data = nextData ?? TEXT_ENCODER2.encode(nextText);
        part.changed = true;
      } else if (nextData != null) {
        part.data = nextData;
        delete part.text;
        part.changed = true;
      }
      if (options.contentType) part.contentType = options.contentType;
    }
  });
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/document-relationships.ts
var WORD_PORT_MANAGED_DOCUMENT_PARTS = [
  {
    path: WORD_PORT_MANAGED_RELATED_PARTS.numbering.path,
    contentType: WORD_PORT_CONTENT_TYPES.numbering,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.numbering,
    relationshipTarget: "numbering.xml"
  },
  {
    path: WORD_PORT_MANAGED_RELATED_PARTS.styles.path,
    contentType: WORD_PORT_CONTENT_TYPES.styles,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.styles,
    relationshipTarget: "styles.xml"
  },
  {
    path: WORD_PORT_MANAGED_RELATED_PARTS.settings.path,
    contentType: WORD_PORT_CONTENT_TYPES.settings,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.settings,
    relationshipTarget: "settings.xml"
  }
];
function reconcileWordPortDocumentRelationshipsInPackage(wordPackage, managedParts = WORD_PORT_MANAGED_DOCUMENT_PARTS) {
  const addedRelationshipIds = [];
  const diagnostics = [];
  let changed = false;
  for (const part of managedParts) {
    if (!wordPackage.parts.has(normalizeOpcPath(part.path))) continue;
    const result = findOrCreateWordPortRelationship(wordPackage, {
      sourcePart: "word/document.xml",
      targetPart: part.path,
      target: part.relationshipTarget,
      relationshipType: part.relationshipType,
      diagnosticSource: "opc.document-relationships"
    });
    diagnostics.push(...result.diagnostics);
    changed ||= result.changed;
    if (result.changed) addedRelationshipIds.push(result.relationshipId);
  }
  const set = wordPackage.getRelationships("word/document.xml");
  return {
    relationshipsXml: set ? serializeRelationshipsXml(set) : "",
    changed,
    addedRelationshipIds,
    diagnostics
  };
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/package-metadata-sync.ts
function syncWordPortPackageMetadata(source) {
  const diagnostics = [];
  const managedParts = source.managedPackageParts ?? WORD_PORT_MANAGED_PACKAGE_PARTS;
  const presentParts = /* @__PURE__ */ new Set();
  for (const entry of managedParts) {
    const path = normalizeOpcPath(entry.path);
    if (partExists(path, source)) presentParts.add(path);
  }
  const rawContentTypes = readLayeredEntry("[Content_Types].xml", source);
  if (rawContentTypes == null) {
    return failOrFallback(source, diagnostics, "opc.invalid-content-types", "[Content_Types].xml is missing from package metadata.");
  }
  const contentTypes = parseContentTypes(rawContentTypes, diagnostics, source.strict !== false);
  reconcileManagedContentTypes(contentTypes, managedParts, presentParts);
  const rawRels = readLayeredEntry("_rels/.rels", source);
  const rootRelationships = rawRels == null ? { path: "_rels/.rels", relationships: [] } : parseRelationshipsXml(toText(rawRels), "_rels/.rels", void 0);
  reconcileManagedRootRelationships(rootRelationships, managedParts, presentParts);
  const packagePaths = collectPackagePaths(source);
  packagePaths.add("[Content_Types].xml");
  packagePaths.add("_rels/.rels");
  for (const path of presentParts) packagePaths.add(path);
  const contentTypesXml = serializeContentTypesXml(contentTypes, packagePaths);
  const rootRelationshipsXml = serializeRelationshipsXml(rootRelationships);
  return {
    contentTypesXml,
    rootRelationshipsXml,
    changed: contentTypesXml !== toText(rawContentTypes) || rootRelationshipsXml !== (rawRels == null ? "" : toText(rawRels)),
    diagnostics
  };
}
function reconcileManagedContentTypes(contentTypes, managedParts, presentParts) {
  for (const entry of managedParts) {
    if (!entry.contentType) continue;
    const path = normalizeOpcPath(entry.path);
    if (presentParts.has(path)) contentTypes.overrides.set(path, entry.contentType);
    else contentTypes.overrides.delete(path);
  }
}
function reconcileManagedRootRelationships(rootRelationships, managedParts, presentParts) {
  const managedRelationshipTypes = new Set(managedParts.map((entry) => entry.relationshipType).filter(Boolean));
  const keepByType = /* @__PURE__ */ new Map();
  const remove = /* @__PURE__ */ new Set();
  let maxId = 0;
  for (let index = 0; index < rootRelationships.relationships.length; index++) {
    const relationship = rootRelationships.relationships[index];
    const match = relationship.id.match(/^rId(\d+)$/);
    if (match) maxId = Math.max(maxId, Number(match[1]));
    if (!managedRelationshipTypes.has(relationship.type)) continue;
    if (keepByType.has(relationship.type)) remove.add(index);
    else keepByType.set(relationship.type, index);
  }
  for (const entry of managedParts) {
    if (!entry.relationshipType) continue;
    const path = normalizeOpcPath(entry.path);
    const existingIndex = keepByType.get(entry.relationshipType);
    if (!presentParts.has(path)) {
      if (existingIndex != null) remove.add(existingIndex);
      continue;
    }
    if (existingIndex == null) {
      maxId++;
      rootRelationships.relationships.push({
        id: `rId${maxId}`,
        type: entry.relationshipType,
        target: path,
        resolvedTarget: path
      });
      continue;
    }
    const relationship = rootRelationships.relationships[existingIndex];
    relationship.target = path;
    relationship.resolvedTarget = path;
    relationship.sourcePart = void 0;
    delete relationship.targetMode;
  }
  for (const index of [...remove].sort((a, b) => b - a)) {
    rootRelationships.relationships.splice(index, 1);
  }
}
function parseContentTypes(rawContentTypes, diagnostics, strict) {
  const text = toText(rawContentTypes);
  const before = diagnostics.length;
  const parsed = parseContentTypesXml(text, {
    error(code, message, detail) {
      diagnostics.push({ severity: "error", code, message, ...detail ?? {} });
    },
    warning(code, message, detail) {
      diagnostics.push({ severity: "warning", code, message, ...detail ?? {} });
    }
  });
  if (strict && diagnostics.length > before && diagnostics.slice(before).some((diagnostic) => diagnostic.severity === "error")) {
    throw new Error("[Content_Types].xml could not be parsed.");
  }
  return parsed;
}
function failOrFallback(source, diagnostics, code, message) {
  const diagnostic = {
    severity: "error",
    code,
    message,
    path: "[Content_Types].xml",
    source: "opc.package-metadata-sync"
  };
  diagnostics.push(diagnostic);
  if (source.strict !== false) throw new Error(message);
  return {
    contentTypesXml: DEFAULT_CONTENT_TYPES_XML,
    rootRelationshipsXml: serializeRelationshipsXml({ path: "_rels/.rels", relationships: [] }),
    changed: true,
    diagnostics
  };
}
function partExists(path, source) {
  const value = readLayeredEntry(path, source);
  return value != null;
}
function readLayeredEntry(path, source) {
  const normalized = normalizeOpcPath(path);
  const updated = readUpdatedEntry(normalized, source.updatedParts);
  if (updated.found) return updated.value;
  const base = source.baseFiles;
  if (!base) return void 0;
  if (isWordPortPackage(base)) return base.getPart(normalized)?.text ?? base.getPart(normalized)?.data;
  if (Array.isArray(base)) return base.find((entry) => normalizeOpcPath(entry.name) === normalized)?.content;
  return base[normalized];
}
function readUpdatedEntry(path, updatedParts) {
  if (!updatedParts) return { found: false, value: void 0 };
  if (updatedParts instanceof Map) {
    if (!updatedParts.has(path)) return { found: false, value: void 0 };
    return { found: true, value: updatedParts.get(path) };
  }
  if (!Object.prototype.hasOwnProperty.call(updatedParts, path)) return { found: false, value: void 0 };
  return { found: true, value: updatedParts[path] };
}
function collectPackagePaths(source) {
  const paths = /* @__PURE__ */ new Set();
  const base = source.baseFiles;
  if (isWordPortPackage(base)) {
    for (const path of base.parts.keys()) paths.add(path);
  } else if (Array.isArray(base)) {
    for (const entry of base) paths.add(normalizeOpcPath(entry.name));
  } else if (base) {
    for (const path of Object.keys(base)) paths.add(normalizeOpcPath(path));
  }
  const updated = source.updatedParts;
  const entries = updated instanceof Map ? updated.entries() : Object.entries(updated ?? {});
  for (const [path, value] of entries) {
    const normalized = normalizeOpcPath(path);
    if (value == null) paths.delete(normalized);
    else paths.add(normalized);
  }
  return paths;
}
function isWordPortPackage(value) {
  return Boolean(value && typeof value === "object" && "parts" in value && "getPart" in value);
}
function toText(value) {
  return typeof value === "string" ? value : new TextDecoder().decode(value);
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/ir.ts
var WORD_PORT_FIELD_REFERENCE_NAME = "w:noBreakHyphen";
var WORD_PORT_FIELD_ATTRIBUTE = "data-word-port-field";

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/comments.ts
function liveWordPortComments(comments) {
  return (comments ?? []).filter((comment) => !comment.deleted);
}
function ensureWordPortCommentParaId(comment) {
  if (comment.paraId) return comment.paraId;
  const rawParaId = comment.attributes?.["w15:paraId"] ?? comment.attributes?.["w14:paraId"];
  if (rawParaId) return rawParaId;
  return stableWordPortCommentParaId(comment.id);
}
function exportWordPortCommentsExtendedXml(comments) {
  const liveComments = liveWordPortComments(comments);
  const paraIdByCommentId = new Map(liveComments.map((comment) => [comment.id, ensureWordPortCommentParaId(comment)]));
  return createXmlNode("w15:commentsEx", {
    "xmlns:w15": "http://schemas.microsoft.com/office/word/2012/wordml"
  }, liveComments.map((comment) => {
    const parentParaId = comment.parentCommentId ? paraIdByCommentId.get(comment.parentCommentId) : void 0;
    return createXmlNode("w15:commentEx", {
      "w15:paraId": ensureWordPortCommentParaId(comment),
      "w15:done": comment.done ? "1" : "0",
      ...parentParaId ? { "w15:paraIdParent": parentParaId } : {}
    });
  }));
}
function exportWordPortCommentsIdsXml(comments) {
  return createXmlNode("w16cid:commentsIds", {
    "xmlns:w16cid": "http://schemas.microsoft.com/office/word/2016/wordml/cid"
  }, liveWordPortComments(comments).map((comment, index) => createXmlNode("w16cid:commentId", {
    "w16cid:paraId": ensureWordPortCommentParaId(comment),
    "w16cid:durableId": stableWordPortDurableCommentId(comment.id, index)
  })));
}
function stableWordPortCommentParaId(id) {
  return stableHex(`para:${id}`, 8).toUpperCase();
}
function stableWordPortDurableCommentId(id, index) {
  return String(Number.parseInt(stableHex(`durable:${id}:${index}`, 7), 16));
}
function stableHex(value, length) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(length, "0").slice(0, length);
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/notes.ts
function liveWordPortNotes(notes) {
  return Object.values(notes ?? {});
}
function hasEditableWordPortNoteStories(notes) {
  return liveWordPortNotes(notes).some((note) => !note.raw);
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/export.ts
async function exportWordPortDocumentToDocx(document, options = {}) {
  const diagnostics = new WordPortDiagnostics();
  const wordPackage = options.package ?? createMinimalPackage();
  const mainDocumentPart = document.package?.mainDocumentPart ?? "word/document.xml";
  reconcileDocumentHyperlinkRelationships(document, wordPackage, diagnostics, mainDocumentPart);
  reconcileNumberingPart(document, wordPackage, mainDocumentPart, diagnostics);
  const root = exportDocumentXml(document, diagnostics, { package: wordPackage, partPath: mainDocumentPart });
  upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: mainDocumentPart }, {
    path: mainDocumentPart,
    text: serializeWordPortXml({ root }),
    contentType: WORD_PORT_CONTENT_TYPES.mainDocument,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.officeDocument
  });
  exportAnnotationParts(document, wordPackage, diagnostics, mainDocumentPart);
  exportHeaderFooterParts(document, wordPackage, diagnostics, mainDocumentPart);
  finalizeExportOpcMetadata(wordPackage, diagnostics);
  wordPackage.diagnostics.push(...diagnostics.items);
  return writeWordPortPackage(wordPackage);
}
function exportDocumentXml(document, diagnostics = new WordPortDiagnostics(), contextOptions = {}) {
  const context = {
    diagnostics,
    partPath: contextOptions.partPath ?? document.package?.mainDocumentPart ?? "word/document.xml",
    package: contextOptions.package
  };
  const attributes = {
    "xmlns:wpc": "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
    "xmlns:mc": "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "xmlns:o": "urn:schemas-microsoft-com:office:office",
    "xmlns:r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "xmlns:m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
    "xmlns:v": "urn:schemas-microsoft-com:vml",
    "xmlns:wp14": "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
    "xmlns:wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "xmlns:w10": "urn:schemas-microsoft-com:office:word",
    "xmlns:w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "xmlns:w14": "http://schemas.microsoft.com/office/word/2010/wordml",
    "xmlns:w15": "http://schemas.microsoft.com/office/word/2012/wordml",
    "xmlns:wpg": "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    "xmlns:wpi": "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
    "xmlns:wne": "http://schemas.microsoft.com/office/word/2006/wordml",
    "xmlns:wps": "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    "mc:Ignorable": "w14 w15 wp14",
    ...document.source?.documentAttributes ?? {}
  };
  return createXmlNode("w:document", attributes, [exportBody(document.body, document.source?.bodyProperties, context)]);
}
function exportBody(body, bodyProperties, context) {
  const children = [];
  for (const block of body.blocks) children.push(exportBlock(block, context));
  for (const raw of body.raw ?? []) {
    children.push(cloneWordPortXmlNode(raw));
    context.diagnostics.warning("ooxml.export-passthrough", `Raw body child ${raw.name} was preserved during export.`, { source: raw.name });
  }
  if (bodyProperties) children.push(cloneWordPortXmlNode(bodyProperties));
  return createXmlNode("w:body", void 0, children);
}
function exportBlock(block, context) {
  if (block.type === "paragraph") return exportParagraph(block, context);
  if (block.type === "table") return exportTable(block, context);
  if (block.type === "blockWrapper") return exportBlockWrapper(block, context);
  context.diagnostics.warning("ooxml.export-passthrough", `Unsupported block ${block.name} was preserved during export.`, { source: block.name });
  return cloneWordPortXmlNode(block.raw);
}
function exportBlockWrapper(block, context) {
  const preservedChildren = (block.raw?.children ?? []).filter((child2) => child2.name !== block.contentName).map((child2) => cloneWordPortXmlNode(child2));
  return createXmlNode(block.name, block.attributes, [
    ...preservedChildren,
    createXmlNode(block.contentName, void 0, block.blocks.map((child2) => exportBlock(child2, context)))
  ]);
}
function exportParagraph(paragraph, context) {
  const children = [];
  if (paragraph.properties) children.push(cloneWordPortXmlNode(paragraph.properties));
  for (const inline of paragraph.runs) children.push(...exportInline(inline, context));
  for (const raw of paragraph.raw ?? []) {
    children.push(cloneWordPortXmlNode(raw));
    context.diagnostics.warning("ooxml.export-passthrough", `Raw paragraph child ${raw.name} was preserved during export.`, { source: raw.name });
  }
  return createXmlNode("w:p", sourceAnchorAttributes(paragraph.source), children);
}
function exportInline(inline, context) {
  switch (inline.type) {
    case "run":
      return [exportRun(inline, context)];
    case "text":
      return [exportText(inline)];
    case "break":
      return [exportBreak(inline)];
    case "tab":
      return [exportTab(inline)];
    case "hyperlink":
      return [exportHyperlink(inline, context)];
    case "inlineWrapper":
      return [exportInlineWrapper(inline, context)];
    case "reference":
      return exportReference(inline, context.diagnostics);
    case "drawing":
      return [exportDrawing(inline, context)];
    case "unsupportedInline":
      context.diagnostics.warning("ooxml.export-passthrough", `Unsupported inline ${inline.name} was preserved during export.`, {
        source: inline.name
      });
      return [cloneWordPortXmlNode(inline.raw)];
  }
}
function exportRun(run, context) {
  const children = [];
  if (run.properties) children.push(cloneWordPortXmlNode(run.properties));
  for (const inline of run.content) children.push(...exportInline(inline, context));
  for (const raw of run.raw ?? []) {
    children.push(cloneWordPortXmlNode(raw));
    context.diagnostics.warning("ooxml.export-passthrough", `Raw run child ${raw.name} was preserved during export.`, { source: raw.name });
  }
  return createXmlNode("w:r", void 0, children);
}
function exportText(text) {
  return createTextNode("w:t", text.text, text.space === "preserve" || /^\s|\s$/.test(text.text) ? { "xml:space": "preserve" } : void 0);
}
function exportBreak(lineBreak) {
  return createXmlNode("w:br", lineBreak.breakType ? { "w:type": lineBreak.breakType } : void 0);
}
function exportTab(_tab) {
  return createXmlNode("w:tab");
}
function exportHyperlink(hyperlink, context) {
  const attrs = {};
  if (hyperlink.relationshipId) attrs["r:id"] = hyperlink.relationshipId;
  if (hyperlink.anchor) attrs["w:anchor"] = hyperlink.anchor;
  if (!hyperlink.relationshipId && !hyperlink.anchor && hyperlink.target) {
    context.diagnostics.warning("ooxml.export-partial", "External hyperlink export requires a relationship; hyperlink text was preserved without target.", {
      detail: { target: hyperlink.target }
    });
  }
  return createXmlNode("w:hyperlink", attrs, hyperlink.content.flatMap((inline) => exportInline(inline, context)));
}
function exportInlineWrapper(wrapper, context) {
  if (wrapper.name === "w:sdt") {
    const preservedChildren = (wrapper.raw?.children ?? []).filter((child2) => child2.name !== "w:sdtContent").map((child2) => cloneWordPortXmlNode(child2));
    return createXmlNode("w:sdt", wrapper.attributes, [
      ...preservedChildren,
      createXmlNode("w:sdtContent", void 0, wrapper.content.flatMap((inline) => exportInline(inline, context)))
    ]);
  }
  return createXmlNode(
    wrapper.name,
    wrapper.attributes,
    wrapper.content.flatMap((inline) => exportInline(inline, context))
  );
}
function exportReference(reference, diagnostics) {
  const field = reference.field ?? decodeFieldReference(reference.attributes?.[WORD_PORT_FIELD_ATTRIBUTE]);
  if (field) return exportFieldReference(field, diagnostics);
  if (reference.attributes?.[WORD_PORT_FIELD_ATTRIBUTE]) {
    diagnostics.warning("ooxml.export-partial", "Field placeholder metadata could not be decoded; preserving the visible reference marker only.", {
      source: reference.name
    });
  }
  return [createXmlNode(reference.name, reference.attributes)];
}
function exportFieldReference(field, diagnostics) {
  if (field.kind === "complex" && field.rawNodes?.length) return field.rawNodes.map((node) => cloneWordPortXmlNode(node));
  if (field.kind === "simple" && field.rawNode) return [cloneWordPortXmlNode(field.rawNode)];
  if (!field.supported) {
    diagnostics.warning("ooxml.export-partial", "Edited unsupported field cannot be rebuilt safely; exporting its display text with a diagnostic instead of synthesizing field XML.", {
      source: "w:fldSimple",
      detail: { instructionType: field.instructionType, instruction: field.instruction }
    });
    return [createXmlNode("w:r", void 0, [createTextNode("w:t", field.displayText)])];
  }
  if (field.complete === false) {
    diagnostics.warning("ooxml.malformed-field", "Incomplete field metadata cannot be rebuilt safely; exporting display text instead of synthesizing field XML.", {
      source: "w:fldChar",
      detail: { instructionType: field.instructionType, instruction: field.instruction }
    });
    return [createXmlNode("w:r", void 0, [createTextNode("w:t", field.displayText)])];
  }
  diagnostics.warning("ooxml.export-partial", "Field XML was rebuilt from structured WordPort metadata because original field XML was unavailable.", {
    source: field.kind === "complex" ? "w:fldChar" : "w:fldSimple",
    detail: { instructionType: field.instructionType, instruction: field.instruction }
  });
  if (field.kind === "complex") return exportSynthesizedComplexField(field);
  return exportSynthesizedSimpleField(field);
}
function exportSynthesizedSimpleField(field) {
  return [createXmlNode("w:fldSimple", { "w:instr": field.instruction }, [
    ...textRunsFromFieldResult(fieldResultTextForExport(field))
  ])];
}
function exportSynthesizedComplexField(field) {
  return [
    createXmlNode("w:r", void 0, [createXmlNode("w:fldChar", { "w:fldCharType": "begin" })]),
    createXmlNode("w:r", void 0, instructionNodesForComplexField(field)),
    createXmlNode("w:r", void 0, [createXmlNode("w:fldChar", { "w:fldCharType": "separate" })]),
    ...textRunsFromFieldResult(fieldResultTextForExport(field)),
    createXmlNode("w:r", void 0, [createXmlNode("w:fldChar", { "w:fldCharType": "end" })])
  ];
}
function instructionNodesForComplexField(field) {
  const tokens = field.instructionTokens?.length ? field.instructionTokens : [{ type: "text", text: field.instruction }];
  const nodes = [];
  for (const token of tokens) {
    if (token.type === "tab") nodes.push(createXmlNode("w:tab"));
    else if (token.text) nodes.push(createTextNode("w:instrText", token.text, { "xml:space": "preserve" }));
  }
  return nodes.length ? nodes : [createTextNode("w:instrText", field.instruction, { "xml:space": "preserve" })];
}
function textRunsFromFieldResult(result) {
  if (!result) return [];
  const runs = [];
  const pieces = result.split("	");
  pieces.forEach((piece, index) => {
    if (index > 0) runs.push(createXmlNode("w:r", void 0, [createXmlNode("w:tab")]));
    if (piece) runs.push(createXmlNode("w:r", void 0, [createTextNode("w:t", piece, /^\s|\s$/.test(piece) ? { "xml:space": "preserve" } : void 0)]));
  });
  return runs;
}
function fieldResultTextForExport(field) {
  if (field.resultText != null) return field.resultText;
  if (field.instructionType === "XE" || field.instructionType === "TC") return "";
  return field.displayText;
}
function decodeFieldReference(value) {
  if (!value) return void 0;
  try {
    const parsed = JSON.parse(value);
    if ((parsed.kind === "complex" || parsed.kind === "simple") && typeof parsed.instruction === "string" && typeof parsed.displayText === "string") {
      return parsed;
    }
  } catch {
    return void 0;
  }
  return void 0;
}
function exportDrawing(drawing, context) {
  if (shouldPreserveRawDrawing(drawing)) {
    context.diagnostics.warning("ooxml.export-passthrough", "Unchanged drawing XML was preserved during export.", {
      path: context.partPath,
      source: drawing.raw?.name ?? "w:drawing",
      detail: { embeds: drawing.embeds.map((embed) => embed.relationshipId).filter(Boolean) }
    });
    return cloneWordPortXmlNode(drawing.raw);
  }
  const media = ensureDrawingMediaRelationship(drawing, context);
  if (!media.relationshipId) {
    context.diagnostics.warning("ooxml.export-partial", "DrawingML image export requires an image relationship; empty drawing was emitted.", {
      path: context.partPath,
      source: "w:drawing",
      detail: { embeds: drawing.embeds }
    });
    return drawing.raw ? cloneWordPortXmlNode(drawing.raw) : createXmlNode("w:drawing");
  }
  return createXmlNode("w:drawing", void 0, [
    drawing.drawingKind === "anchor" ? exportAnchorDrawing(drawing, media, context) : exportInlineDrawing(drawing, media, context)
  ]);
}
function shouldPreserveRawDrawing(drawing) {
  if (!drawing.raw) return false;
  if (drawing.media) return false;
  const firstEmbed = drawing.embeds[0];
  if (firstEmbed?.data || firstEmbed?.bytesBase64 || firstEmbed?.dataUri || firstEmbed?.src) return false;
  if (drawingSizeChangedFromRaw(drawing)) return false;
  if (drawingDocPrChangedFromRaw(drawing)) return false;
  if (drawingWrapChangedFromRaw(drawing)) return false;
  if (drawing.transform || drawing.crop || drawing.hyperlink) return false;
  return true;
}
function drawingSizeChangedFromRaw(drawing) {
  if (!drawing.size) return false;
  const extent = firstXmlDescendant(drawing.raw, "extent", (node) => node.name.startsWith("wp:"));
  const rawCx = positiveInteger(extent?.attributes?.cx);
  const rawCy = positiveInteger(extent?.attributes?.cy);
  const nextCx = positiveInteger(drawing.size.cx) ?? positiveInteger(pixelToEmu(drawing.size.width));
  const nextCy = positiveInteger(drawing.size.cy) ?? positiveInteger(pixelToEmu(drawing.size.height));
  return nextCx != null && rawCx != null && nextCx !== rawCx || nextCy != null && rawCy != null && nextCy !== rawCy;
}
function drawingDocPrChangedFromRaw(drawing) {
  if (!drawing.docPr && drawing.altText == null && drawing.title == null && drawing.decorative == null) return false;
  const rawDocPr = firstXmlDescendant(drawing.raw, "docPr", (node) => node.name.startsWith("wp:"));
  const nextName = drawing.docPr?.name ?? drawing.altText;
  const nextDescr = drawing.decorative ? "" : drawing.docPr?.descr ?? drawing.title ?? drawing.altText;
  if (nextName != null && rawDocPr?.attributes?.name != null && nextName !== rawDocPr.attributes.name) return true;
  if (nextDescr != null && rawDocPr?.attributes?.descr != null && nextDescr !== rawDocPr.attributes.descr) return true;
  return false;
}
function drawingWrapChangedFromRaw(drawing) {
  if (drawing.drawingKind !== "anchor" || !drawing.anchor?.wrap?.type) return false;
  const rawAnchor = firstXmlDescendant(drawing.raw, "anchor", (node) => node.name.startsWith("wp:"));
  const rawWrap = (rawAnchor?.children ?? []).find((child2) => localName2(child2.name).startsWith("wrap"));
  if (!rawWrap) return drawing.anchor.wrap.type !== "None";
  return normalizeWrapType(rawWrap.name.replace(/^.*?:wrap/, "")) !== normalizeWrapType(drawing.anchor.wrap.type);
}
function exportInlineDrawing(drawing, media, context) {
  const size = resolveDrawingSize(drawing, media);
  return createXmlNode("wp:inline", inlineDistanceAttributes(drawing), [
    createXmlNode("wp:extent", { cx: size.cx, cy: size.cy }),
    createXmlNode("wp:effectExtent", { l: 0, t: 0, r: 0, b: 0 }),
    exportDocPr(drawing, media),
    exportCNvGraphicFramePr(drawing),
    exportPictureGraphic(drawing, media, size, context)
  ]);
}
function exportAnchorDrawing(drawing, media, context) {
  const size = resolveDrawingSize(drawing, media);
  const anchor = drawing.anchor;
  const attributes = {
    distT: emuFromCssPixels(anchor?.distance?.top) ?? 0,
    distB: emuFromCssPixels(anchor?.distance?.bottom) ?? 0,
    distL: emuFromCssPixels(anchor?.distance?.left) ?? 0,
    distR: emuFromCssPixels(anchor?.distance?.right) ?? 0,
    simplePos: anchor?.simplePos?.enabled ? "1" : "0",
    relativeHeight: anchor?.relativeHeight ?? 1,
    behindDoc: anchor?.behindDoc ? "1" : "0",
    locked: "0",
    layoutInCell: anchor?.layoutInCell === false ? "0" : "1",
    allowOverlap: anchor?.allowOverlap === false ? "0" : "1",
    ...anchor?.originalAttributes ?? {}
  };
  return createXmlNode("wp:anchor", attributes, [
    createXmlNode("wp:simplePos", {
      x: anchor?.simplePos?.x ?? emuFromCssPixels(anchor?.simplePos?.xPx) ?? 0,
      y: anchor?.simplePos?.y ?? emuFromCssPixels(anchor?.simplePos?.yPx) ?? 0
    }),
    exportAnchorPosition("wp:positionH", anchor?.positionH, "column"),
    exportAnchorPosition("wp:positionV", anchor?.positionV, "paragraph"),
    createXmlNode("wp:extent", { cx: size.cx, cy: size.cy }),
    createXmlNode("wp:effectExtent", {
      l: emuFromCssPixels(anchor?.effectExtent?.left) ?? 0,
      t: emuFromCssPixels(anchor?.effectExtent?.top) ?? 0,
      r: emuFromCssPixels(anchor?.effectExtent?.right) ?? 0,
      b: emuFromCssPixels(anchor?.effectExtent?.bottom) ?? 0
    }),
    exportAnchorWrap(anchor),
    exportDocPr(drawing, media),
    exportCNvGraphicFramePr(drawing),
    exportPictureGraphic(drawing, media, size, context)
  ]);
}
function exportAnchorPosition(name, position, fallback) {
  const children = [];
  if (position?.align) children.push(createTextNode("wp:align", position.align));
  else children.push(createTextNode("wp:posOffset", String(position?.offsetEmu ?? emuFromCssPixels(position?.offset) ?? 0)));
  return createXmlNode(name, { relativeFrom: position?.relativeFrom ?? fallback }, children);
}
function exportAnchorWrap(anchor) {
  const wrap = anchor?.wrap;
  const type = normalizeWrapType(wrap?.type ?? "None");
  if (type === "Square") {
    return createXmlNode("wp:wrapSquare", {
      wrapText: stringAttr(wrap?.attrs?.wrapText) ?? "bothSides",
      distT: emuFromCssPixels(numberAttr(wrap?.attrs?.distTop)),
      distR: emuFromCssPixels(numberAttr(wrap?.attrs?.distRight)),
      distB: emuFromCssPixels(numberAttr(wrap?.attrs?.distBottom)),
      distL: emuFromCssPixels(numberAttr(wrap?.attrs?.distLeft))
    });
  }
  if (type === "TopAndBottom") {
    return createXmlNode("wp:wrapTopAndBottom", {
      distT: emuFromCssPixels(numberAttr(wrap?.attrs?.distTop)),
      distB: emuFromCssPixels(numberAttr(wrap?.attrs?.distBottom))
    });
  }
  return createXmlNode("wp:wrapNone");
}
function exportDocPr(drawing, media) {
  const id = positiveInteger(drawing.docPr?.id) ?? stablePositiveId(media.relationshipId);
  const name = drawing.docPr?.name ?? drawing.altText ?? media.name ?? `Picture ${id}`;
  const descr = drawing.decorative ? void 0 : drawing.docPr?.descr ?? drawing.title ?? drawing.altText;
  const children = drawing.decorative ? [createXmlNode("a:extLst", void 0, [
    createXmlNode("a:ext", { uri: "{C183D7F6-B498-43B3-948B-1728B52AA6E4}" }, [
      createXmlNode("adec:decorative", {
        "xmlns:adec": "http://schemas.microsoft.com/office/drawing/2017/decorative",
        val: "1"
      })
    ])
  ])] : void 0;
  return createXmlNode("wp:docPr", { id, name, descr }, children);
}
function exportCNvGraphicFramePr(drawing) {
  return createXmlNode("wp:cNvGraphicFramePr", void 0, [
    createXmlNode("a:graphicFrameLocks", {
      "xmlns:a": "http://schemas.openxmlformats.org/drawingml/2006/main",
      ...drawing.lockAspectRatio !== false ? { noChangeAspect: 1 } : {}
    })
  ]);
}
function exportPictureGraphic(drawing, media, size, context) {
  const id = positiveInteger(drawing.docPr?.id) ?? stablePositiveId(media.relationshipId);
  const name = drawing.docPr?.name ?? drawing.altText ?? media.name ?? `Picture ${id}`;
  const hyperlinkRelationshipId = ensureDrawingHyperlinkRelationship(drawing, context);
  return createXmlNode("a:graphic", { "xmlns:a": "http://schemas.openxmlformats.org/drawingml/2006/main" }, [
    createXmlNode("a:graphicData", { uri: "http://schemas.openxmlformats.org/drawingml/2006/picture" }, [
      createXmlNode("pic:pic", { "xmlns:pic": "http://schemas.openxmlformats.org/drawingml/2006/picture" }, [
        createXmlNode("pic:nvPicPr", void 0, [
          createXmlNode("pic:cNvPr", { id, name }, hyperlinkRelationshipId ? [
            createXmlNode("a:hlinkClick", {
              "r:id": hyperlinkRelationshipId,
              tooltip: drawing.hyperlink?.tooltip
            })
          ] : void 0),
          createXmlNode("pic:cNvPicPr", void 0, [
            createXmlNode("a:picLocks", {
              noChangeArrowheads: 1,
              ...drawing.lockAspectRatio !== false ? { noChangeAspect: 1 } : {}
            })
          ])
        ]),
        createXmlNode("pic:blipFill", void 0, [
          createXmlNode("a:blip", { "r:embed": media.relationshipId }),
          ...drawing.crop ? [createXmlNode("a:srcRect", cropAttributes(drawing.crop))] : [],
          createXmlNode("a:stretch", void 0, [createXmlNode("a:fillRect")])
        ]),
        createXmlNode("pic:spPr", { bwMode: "auto" }, [
          createXmlNode("a:xfrm", transformAttributes(drawing), [
            createXmlNode("a:ext", { cx: size.cx, cy: size.cy }),
            createXmlNode("a:off", { x: 0, y: 0 })
          ]),
          createXmlNode("a:prstGeom", { prst: "rect" }, [createXmlNode("a:avLst")]),
          createXmlNode("a:noFill")
        ])
      ])
    ])
  ]);
}
function ensureDrawingHyperlinkRelationship(drawing, context) {
  const url = drawing.hyperlink?.url;
  if (!url || !context.package) return drawing.hyperlink?.relationshipId;
  return ensureWordPortExportExternalRelationship(context, {
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.hyperlink,
    target: url,
    relationshipId: drawing.hyperlink.relationshipId,
    diagnosticSource: "ooxml.drawing-hyperlink"
  });
}
function transformAttributes(drawing) {
  const transform = drawing.transform;
  if (!transform) return void 0;
  return {
    rot: typeof transform.rotation === "number" ? Math.round(transform.rotation * 6e4) : void 0,
    flipH: transform.horizontalFlip ? 1 : void 0,
    flipV: transform.verticalFlip ? 1 : void 0
  };
}
function cropAttributes(crop) {
  return {
    l: cropPercentToOoxml(crop.left),
    t: cropPercentToOoxml(crop.top),
    r: cropPercentToOoxml(crop.right),
    b: cropPercentToOoxml(crop.bottom)
  };
}
function cropPercentToOoxml(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) return void 0;
  return Math.max(0, Math.min(99e3, Math.round(parsed * 1e3)));
}
function ensureDrawingMediaRelationship(drawing, context) {
  const source = drawing.media ?? drawing.embeds.find((embed) => embed.data || embed.bytesBase64 || embed.dataUri || embed.src) ?? {};
  const existing = drawing.embeds[0];
  const parsedDataUri = parseDataUri(source.dataUri ?? source.src ?? existing?.dataUri ?? existing?.src);
  const data = source.data ?? existing?.data ?? bytesFromBase64(source.bytesBase64 ?? existing?.bytesBase64) ?? parsedDataUri?.data;
  const contentType = source.contentType ?? existing?.contentType ?? parsedDataUri?.contentType ?? contentTypeFromPath(source.partPath ?? existing?.resolvedPath ?? existing?.target);
  const partPath = source.partPath ?? existing?.resolvedPath ?? (contentType ? nextMediaPartPath(context.package, sanitizeMediaFileName(source.name ?? existing?.name ?? source.src ?? existing?.target, contentType)) : void 0);
  const name = sanitizeMediaFileName(source.name ?? existing?.name ?? source.partPath ?? existing?.resolvedPath ?? existing?.target ?? "image", contentType).split("/").pop();
  if (!partPath) {
    return {
      relationshipId: existing?.relationshipId ?? "",
      contentType,
      name
    };
  }
  const relationshipId = upsertWordPortExportManagedPart(context, {
    path: partPath,
    ...data ? { data } : {},
    contentType,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.image,
    relationshipId: existing?.relationshipId,
    sourcePart: context.partPath,
    diagnosticSource: "ooxml.drawing-media"
  }) ?? existing?.relationshipId ?? "";
  return {
    relationshipId,
    partPath,
    contentType,
    name,
    width: parsedDataUri?.width,
    height: parsedDataUri?.height
  };
}
function resolveDrawingSize(drawing, media) {
  const cx = positiveInteger(drawing.size?.cx) ?? positiveInteger(pixelToEmu(drawing.size?.width)) ?? positiveInteger(pixelToEmu(media.width));
  const cy = positiveInteger(drawing.size?.cy) ?? positiveInteger(pixelToEmu(drawing.size?.height)) ?? positiveInteger(pixelToEmu(media.height));
  return {
    cx: cx ?? 1,
    cy: cy ?? 1
  };
}
function inlineDistanceAttributes(drawing) {
  return {
    distT: emuFromCssPixels(drawing.anchor?.distance?.top) ?? 0,
    distB: emuFromCssPixels(drawing.anchor?.distance?.bottom) ?? 0,
    distL: emuFromCssPixels(drawing.anchor?.distance?.left) ?? 0,
    distR: emuFromCssPixels(drawing.anchor?.distance?.right) ?? 0
  };
}
function nextMediaPartPath(wordPackage, filename) {
  const parts = wordPackage?.parts;
  const normalized = sanitizeMediaFileName(filename);
  const dot = normalized.lastIndexOf(".");
  const basename = dot >= 0 ? normalized.slice(0, dot) : normalized;
  const extension = dot >= 0 ? normalized.slice(dot) : "";
  let path = `word/media/${normalized}`;
  let index = 1;
  while (parts?.has(path)) {
    path = `word/media/${basename}-${index}${extension}`;
    index += 1;
  }
  return path;
}
function sanitizeMediaFileName(value, contentType) {
  const raw = (value ?? "image").split(/[\\/]/).pop() ?? "image";
  const withoutDataPrefix = raw.startsWith("data:") ? `image.${extensionFromContentType(contentType ?? raw.slice(5, raw.indexOf(";")))}` : raw;
  const dot = withoutDataPrefix.lastIndexOf(".");
  const rawName = dot > 0 ? withoutDataPrefix.slice(0, dot) : withoutDataPrefix;
  const rawExtension = dot > 0 ? withoutDataPrefix.slice(dot + 1) : extensionFromContentType(contentType);
  const name = rawName.replace(/[^a-zA-Z0-9_-]/g, "_") || "image";
  const extension = sanitizeExtension(rawExtension);
  return extension ? `${name}.${extension}` : name;
}
function sanitizeExtension(value) {
  const normalized = value?.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (!normalized) return void 0;
  if (normalized === "jpeg") return "jpg";
  return normalized;
}
function parseDataUri(value) {
  if (!value) return void 0;
  const match = value.match(/^data:([^;,]+);base64,([a-z0-9+/=\s]+)$/i);
  if (!match) return void 0;
  const data = bytesFromBase64(match[2]);
  if (!data) return void 0;
  return {
    contentType: match[1],
    data,
    ...readImageDimensions(data)
  };
}
function bytesFromBase64(value) {
  if (!value) return void 0;
  try {
    const binary = globalThis.atob(value.replace(/\s+/g, ""));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    return bytes;
  } catch {
    return void 0;
  }
}
function readImageDimensions(bytes) {
  if (bytes.length >= 24 && bytes[0] === 137 && bytes[1] === 80 && bytes[2] === 78 && bytes[3] === 71) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const width = view.getInt32(16);
    const height = view.getInt32(20);
    if (width > 0 && height > 0) return { width, height };
  }
  if (bytes.length >= 10 && bytes[0] === 71 && bytes[1] === 73 && bytes[2] === 70) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const width = view.getUint16(6, true);
    const height = view.getUint16(8, true);
    if (width > 0 && height > 0) return { width, height };
  }
  return {};
}
function contentTypeFromPath(path) {
  const extension = sanitizeExtension(path?.split(".").pop());
  if (!extension) return void 0;
  if (extension === "png") return "image/png";
  if (extension === "jpg" || extension === "jpeg") return "image/jpeg";
  if (extension === "gif") return "image/gif";
  if (extension === "bmp") return "image/bmp";
  if (extension === "svg") return "image/svg+xml";
  return void 0;
}
function extensionFromContentType(contentType) {
  const normalized = contentType?.toLowerCase();
  if (normalized === "image/png") return "png";
  if (normalized === "image/jpeg" || normalized === "image/jpg") return "jpg";
  if (normalized === "image/gif") return "gif";
  if (normalized === "image/bmp") return "bmp";
  if (normalized === "image/svg+xml") return "svg";
  return void 0;
}
function firstXmlDescendant(node, childLocalName, predicate) {
  if (!node) return void 0;
  const local = localName2(node.name);
  if (local === childLocalName && (!predicate || predicate(node))) return node;
  for (const child2 of node.children ?? []) {
    const match = firstXmlDescendant(child2, childLocalName, predicate);
    if (match) return match;
  }
  return void 0;
}
function normalizeWrapType(value) {
  const normalized = value.replace(/^wp:wrap/i, "").toLowerCase();
  if (normalized === "square") return "Square";
  if (normalized === "topandbottom") return "TopAndBottom";
  if (normalized === "tight") return "Tight";
  if (normalized === "through") return "Through";
  return "None";
}
function positiveInteger(value) {
  if (value == null || value === "") return void 0;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? Math.round(parsed) : void 0;
}
function pixelToEmu(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? Math.round(parsed * 9525) : void 0;
}
function emuFromCssPixels(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? Math.round(parsed * 9525) : void 0;
}
function numberAttr(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : void 0;
}
function stringAttr(value) {
  return typeof value === "string" && value ? value : void 0;
}
function stablePositiveId(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) % 2e9 + 1;
}
function exportTable(table, context) {
  const children = [];
  const rows = normalizeTableVerticalMerges(table);
  const properties = exportTableProperties(table);
  const grid = exportTableGrid(table, rows);
  if (properties) children.push(properties);
  if (grid) children.push(grid);
  for (const row of rows) children.push(exportTableRow(row, context));
  for (const raw of table.raw ?? []) children.push(cloneWordPortXmlNode(raw));
  return createXmlNode("w:tbl", sourceAnchorAttributes(table.source), children);
}
function exportTableRow(row, context) {
  const children = [];
  if (row.propertyExceptions) children.push(cloneWordPortXmlNode(row.propertyExceptions));
  const properties = exportTableRowProperties(row);
  if (properties) children.push(properties);
  for (const cell of row.cells) children.push(exportTableCell(cell, context));
  for (const raw of row.raw ?? []) children.push(cloneWordPortXmlNode(raw));
  return createXmlNode("w:tr", sourceAnchorAttributes(row.source), children);
}
function exportTableCell(cell, context) {
  const children = [];
  const properties = exportTableCellProperties(cell);
  if (properties) children.push(properties);
  for (const block of cell.blocks) children.push(exportBlock(block, context));
  for (const raw of cell.raw ?? []) children.push(cloneWordPortXmlNode(raw));
  if (!cell.blocks.length) children.push(createXmlNode("w:p"));
  return createXmlNode("w:tc", sourceAnchorAttributes(cell.source), children);
}
function sourceAnchorAttributes(source) {
  const attributes = source?.attributes;
  if (!attributes) return void 0;
  const preserved = Object.fromEntries(Object.entries(attributes).filter(
    ([key, value]) => typeof value === "string" && value.length > 0 && (key === "w14:paraId" || key === "w15:paraId" || key === "paraId" || key === "w14:textId" || key === "w15:textId" || key === "textId")
  ));
  return Object.keys(preserved).length ? preserved : void 0;
}
function exportTableProperties(table) {
  const properties = cloneWordPortXmlNode(table.properties);
  if (!table.width && !table.indent && !table.layout && !table.justification && !table.styleId && !table.caption && !table.description && !table.overlap && !table.cellSpacing && table.rightToLeft == null && !table.floatingTableProperties && !table.look && !table.borders && !table.shading && !table.cellMargins) return properties;
  const next = properties ?? createXmlNode("w:tblPr");
  if (table.styleId) upsertTableChild(next, createXmlNode("w:tblStyle", { "w:val": table.styleId }));
  if (table.caption) upsertTableChild(next, createXmlNode("w:tblCaption", { "w:val": table.caption }));
  if (table.description) upsertTableChild(next, createXmlNode("w:tblDescription", { "w:val": table.description }));
  if (table.overlap) upsertTableChild(next, createXmlNode("w:tblOverlap", { "w:val": table.overlap }));
  if (table.width) upsertTableMeasurement(next, "w:tblW", table.width);
  if (table.indent) upsertTableMeasurement(next, "w:tblInd", table.indent);
  if (table.cellSpacing) upsertTableMeasurement(next, "w:tblCellSpacing", table.cellSpacing);
  if (table.justification) upsertTableChild(next, createXmlNode("w:jc", { "w:val": table.justification }));
  if (table.rightToLeft != null) upsertTableChild(next, createXmlNode("w:bidiVisual", table.rightToLeft ? void 0 : { "w:val": "0" }));
  if (table.floatingTableProperties) upsertTableChild(next, exportFloatingTableProperties(table.floatingTableProperties));
  if (table.layout) upsertTableChild(next, createXmlNode("w:tblLayout", { "w:type": table.layout }));
  if (table.look) upsertTableChild(next, exportTableLook(table.look));
  if (table.borders) upsertTableChild(next, exportTableBorders("w:tblBorders", table.borders));
  if (table.shading) upsertTableChild(next, exportTableShading(table.shading));
  if (table.cellMargins) upsertTableChild(next, exportTableCellMargins("w:tblCellMar", table.cellMargins));
  return next;
}
function exportTableGrid(table, rows) {
  if (table.grid && !tableGridColumnsChanged(table)) return cloneWordPortXmlNode(table.grid);
  const columns = table.gridColumns?.length ? table.gridColumns : buildExportFallbackGridColumns(table, rows);
  if (!columns.length) return void 0;
  return createXmlNode("w:tblGrid", void 0, columns.map(
    (column) => createXmlNode("w:gridCol", column.widthTwips ? { "w:w": String(column.widthTwips) } : void 0)
  ));
}
function exportTableRowProperties(row) {
  const properties = cloneWordPortXmlNode(row.properties);
  if (row.gridBefore == null && row.gridAfter == null && !row.wBefore && !row.wAfter && !row.cantSplit && !row.repeatHeader && row.heightTwips == null && !row.justification && !row.cellSpacing && !row.conditionalStyle) return properties;
  const next = properties ?? createXmlNode("w:trPr");
  if (row.gridBefore != null) upsertTableChild(next, createXmlNode("w:gridBefore", { "w:val": String(Math.max(0, Math.round(row.gridBefore))) }));
  if (row.gridAfter != null) upsertTableChild(next, createXmlNode("w:gridAfter", { "w:val": String(Math.max(0, Math.round(row.gridAfter))) }));
  if (row.wBefore) upsertTableMeasurement(next, "w:wBefore", row.wBefore);
  if (row.wAfter) upsertTableMeasurement(next, "w:wAfter", row.wAfter);
  if (row.cantSplit) upsertTableChild(next, createXmlNode("w:cantSplit"));
  if (row.repeatHeader) upsertTableChild(next, createXmlNode("w:tblHeader"));
  if (row.justification) upsertTableChild(next, createXmlNode("w:jc", { "w:val": row.justification }));
  if (row.cellSpacing) upsertTableMeasurement(next, "w:tblCellSpacing", row.cellSpacing);
  if (row.conditionalStyle) upsertTableChild(next, exportTableConditionalStyle(row.conditionalStyle));
  if (row.heightTwips != null) {
    upsertTableChild(next, createXmlNode("w:trHeight", {
      "w:val": String(Math.max(0, Math.round(row.heightTwips))),
      ...row.heightRule ? { "w:hRule": row.heightRule } : {}
    }));
  }
  return next;
}
function exportTableCellProperties(cell) {
  const properties = cloneWordPortXmlNode(cell.properties);
  if (cell.gridSpan == null && !cell.vMerge && !cell.hMerge && !cell.width && !cell.borders && !cell.shading && !cell.cellMargins && !cell.verticalAlign && cell.noWrap == null && !cell.textDirection && cell.fitText == null && cell.hideMark == null && !cell.conditionalStyle) return properties;
  const next = properties ?? createXmlNode("w:tcPr");
  if (cell.gridSpan != null) {
    const span = positiveInteger(cell.gridSpan) ?? 1;
    if (span > 1) upsertTableChild(next, createXmlNode("w:gridSpan", { "w:val": String(span) }));
    else removeTableChild(next, "gridSpan");
  }
  if (cell.width) upsertTableMeasurement(next, "w:tcW", cell.width);
  if (cell.vMerge) {
    upsertTableChild(next, createXmlNode("w:vMerge", cell.vMerge === "restart" ? { "w:val": "restart" } : void 0));
  }
  if (cell.hMerge) {
    upsertTableChild(next, createXmlNode("w:hMerge", cell.hMerge === "restart" ? { "w:val": "restart" } : void 0));
  }
  if (cell.borders) upsertTableChild(next, exportTableBorders("w:tcBorders", cell.borders));
  if (cell.shading) upsertTableChild(next, exportTableShading(cell.shading));
  if (cell.cellMargins) upsertTableChild(next, exportTableCellMargins("w:tcMar", cell.cellMargins));
  if (cell.verticalAlign) upsertTableChild(next, createXmlNode("w:vAlign", { "w:val": cell.verticalAlign }));
  if (cell.noWrap != null) upsertTableChild(next, createXmlNode("w:noWrap", cell.noWrap ? void 0 : { "w:val": "0" }));
  if (cell.textDirection) upsertTableChild(next, createXmlNode("w:textDirection", { "w:val": cell.textDirection }));
  if (cell.fitText != null) upsertTableChild(next, createXmlNode("w:tcFitText", cell.fitText ? void 0 : { "w:val": "0" }));
  if (cell.hideMark != null) upsertTableChild(next, createXmlNode("w:hideMark", cell.hideMark ? void 0 : { "w:val": "0" }));
  if (cell.conditionalStyle) upsertTableChild(next, exportTableConditionalStyle(cell.conditionalStyle));
  return next;
}
function normalizeTableVerticalMerges(table) {
  const rows = table.rows.map((row) => ({ ...row, cells: row.cells.map((cell) => ({ ...cell })) }));
  const activeMerges = /* @__PURE__ */ new Map();
  for (let rowIndex = 0; rowIndex < rows.length; rowIndex += 1) {
    const row = rows[rowIndex];
    let columnIndex = row.gridBefore ?? 0;
    const slots = row.cells.map((cell, index) => {
      const startColumn = columnIndex;
      const span = Math.max(1, positiveInteger(cell.gridSpan) ?? 1);
      columnIndex += span;
      return { cell, index, startColumn, span };
    });
    for (const [startColumn, merge] of [...activeMerges]) {
      if (merge.remainingRows <= 0) {
        activeMerges.delete(startColumn);
        continue;
      }
      const existing = slots.find((slot) => slot.startColumn === startColumn && slot.cell.vMerge === "continue");
      if (existing) {
        existing.cell = row.cells[existing.index] = { ...existing.cell, vMerge: "continue", gridSpan: merge.gridSpan };
      } else {
        const insertIndex = slots.findIndex((slot) => slot.startColumn >= startColumn);
        const placeholder = {
          type: "tableCell",
          blocks: [],
          gridSpan: merge.gridSpan,
          vMerge: "continue",
          ...merge.width ? { width: merge.width } : {}
        };
        if (insertIndex >= 0) row.cells.splice(insertIndex, 0, placeholder);
        else row.cells.push(placeholder);
      }
      merge.remainingRows -= 1;
      if (merge.remainingRows <= 0) activeMerges.delete(startColumn);
    }
    for (const slot of slots) {
      const rowSpan = positiveInteger(slot.cell.rowSpan) ?? positiveInteger(slot.cell.rowspan) ?? 1;
      if (rowSpan > 1 || slot.cell.vMerge === "restart") {
        slot.cell.vMerge = "restart";
        slot.cell.gridSpan = Math.max(1, positiveInteger(slot.cell.gridSpan) ?? slot.span);
        activeMerges.set(slot.startColumn, {
          remainingRows: Math.max(0, Math.min(rowSpan, rows.length - rowIndex) - 1),
          gridSpan: slot.cell.gridSpan,
          ...slot.cell.width ? { width: slot.cell.width } : {}
        });
      }
    }
  }
  return rows;
}
function buildExportFallbackGridColumns(table, rows) {
  const columnCount = rows.reduce((max, row) => Math.max(max, (row.gridBefore ?? 0) + row.cells.reduce((sum, cell) => sum + Math.max(1, positiveInteger(cell.gridSpan) ?? 1), 0) + (row.gridAfter ?? 0)), 0);
  if (columnCount <= 0) return [];
  const total = table.width?.type === "dxa" && table.width.value > 0 ? table.width.value : 9360;
  const base = Math.max(150, Math.floor(total / columnCount));
  const remainder = total - base * columnCount;
  return Array.from({ length: columnCount }, (_, index) => ({ widthTwips: base + (index < remainder ? 1 : 0) }));
}
function tableGridColumnsChanged(table) {
  if (!table.grid || !table.gridColumns?.length) return false;
  const rawColumns = (table.grid.children ?? []).filter((child2) => localName2(child2.name) === "gridCol").map((child2) => positiveInteger(xmlAttr(child2, "w")) ?? positiveInteger(xmlAttr(child2, "w:w")));
  if (rawColumns.length !== table.gridColumns.length) return true;
  return table.gridColumns.some((column, index) => {
    const next = positiveInteger(column.widthTwips);
    const raw = rawColumns[index];
    return next != null && raw != null && next !== raw;
  });
}
function exportTableLook(look) {
  return createXmlNode("w:tblLook", {
    ...look.attributes ?? {},
    ...look.value ? { "w:val": look.value } : {},
    ...look.firstRow != null ? { "w:firstRow": look.firstRow ? "1" : "0" } : {},
    ...look.lastRow != null ? { "w:lastRow": look.lastRow ? "1" : "0" } : {},
    ...look.firstColumn != null ? { "w:firstColumn": look.firstColumn ? "1" : "0" } : {},
    ...look.lastColumn != null ? { "w:lastColumn": look.lastColumn ? "1" : "0" } : {},
    ...look.noHorizontalBand != null ? { "w:noHBand": look.noHorizontalBand ? "1" : "0" } : {},
    ...look.noVerticalBand != null ? { "w:noVBand": look.noVerticalBand ? "1" : "0" } : {}
  });
}
function exportTableBorders(name, borders) {
  return createXmlNode(name, void 0, Object.entries(borders).map(
    ([side, border]) => createXmlNode(`w:${side}`, {
      ...border.attributes ?? {},
      ...border.val ? { "w:val": border.val } : {},
      ...border.size != null ? { "w:sz": String(Math.max(0, Math.round(border.size))) } : {},
      ...border.space != null ? { "w:space": String(Math.max(0, Math.round(border.space))) } : {},
      ...border.color ? { "w:color": border.color } : {}
    })
  ));
}
function exportTableShading(shading) {
  return createXmlNode("w:shd", {
    ...shading.attributes ?? {},
    ...shading.val ? { "w:val": shading.val } : {},
    ...shading.color ? { "w:color": shading.color } : {},
    ...shading.fill ? { "w:fill": shading.fill } : {}
  });
}
function exportTableConditionalStyle(style) {
  return createXmlNode("w:cnfStyle", {
    ...style.attributes ?? {},
    ...style.evenHBand != null ? { "w:evenHBand": style.evenHBand ? "1" : "0" } : {},
    ...style.evenVBand != null ? { "w:evenVBand": style.evenVBand ? "1" : "0" } : {},
    ...style.firstColumn != null ? { "w:firstColumn": style.firstColumn ? "1" : "0" } : {},
    ...style.firstRow != null ? { "w:firstRow": style.firstRow ? "1" : "0" } : {},
    ...style.firstRowFirstColumn != null ? { "w:firstRowFirstColumn": style.firstRowFirstColumn ? "1" : "0" } : {},
    ...style.firstRowLastColumn != null ? { "w:firstRowLastColumn": style.firstRowLastColumn ? "1" : "0" } : {},
    ...style.lastColumn != null ? { "w:lastColumn": style.lastColumn ? "1" : "0" } : {},
    ...style.lastRow != null ? { "w:lastRow": style.lastRow ? "1" : "0" } : {},
    ...style.lastRowFirstColumn != null ? { "w:lastRowFirstColumn": style.lastRowFirstColumn ? "1" : "0" } : {},
    ...style.lastRowLastColumn != null ? { "w:lastRowLastColumn": style.lastRowLastColumn ? "1" : "0" } : {},
    ...style.oddHBand != null ? { "w:oddHBand": style.oddHBand ? "1" : "0" } : {},
    ...style.oddVBand != null ? { "w:oddVBand": style.oddVBand ? "1" : "0" } : {},
    ...style.value ? { "w:val": style.value } : {}
  });
}
function exportFloatingTableProperties(floating) {
  return createXmlNode("w:tblpPr", {
    ...floating.attributes ?? {},
    ...floating.leftFromText != null ? { "w:leftFromText": String(Math.round(floating.leftFromText)) } : {},
    ...floating.rightFromText != null ? { "w:rightFromText": String(Math.round(floating.rightFromText)) } : {},
    ...floating.topFromText != null ? { "w:topFromText": String(Math.round(floating.topFromText)) } : {},
    ...floating.bottomFromText != null ? { "w:bottomFromText": String(Math.round(floating.bottomFromText)) } : {},
    ...floating.x != null ? { "w:tblpX": String(Math.round(floating.x)) } : {},
    ...floating.y != null ? { "w:tblpY": String(Math.round(floating.y)) } : {},
    ...floating.horizontalAnchor ? { "w:horzAnchor": floating.horizontalAnchor } : {},
    ...floating.verticalAnchor ? { "w:vertAnchor": floating.verticalAnchor } : {},
    ...floating.xSpec ? { "w:tblpXSpec": floating.xSpec } : {},
    ...floating.ySpec ? { "w:tblpYSpec": floating.ySpec } : {}
  });
}
function exportTableCellMargins(name, margins) {
  return createXmlNode(name, void 0, Object.entries(margins).map(
    ([side, margin]) => createXmlNode(`w:${side}`, {
      ...margin.attributes ?? {},
      "w:w": String(Math.max(0, Math.round(margin.value))),
      ...margin.type ? { "w:type": margin.type } : {}
    })
  ));
}
function upsertTableMeasurement(parent, name, measurement) {
  upsertTableChild(parent, createXmlNode(name, {
    "w:w": String(Math.max(0, Math.round(measurement.value))),
    ...measurement.type ? { "w:type": measurement.type } : {}
  }));
}
function upsertTableChild(parent, child2) {
  parent.children ??= [];
  removeTableChild(parent, localName2(child2.name));
  parent.children.push(child2);
}
function removeTableChild(parent, childLocalName) {
  parent.children = (parent.children ?? []).filter((child2) => localName2(child2.name) !== childLocalName);
}
function createMinimalPackage() {
  const parts = /* @__PURE__ */ new Map();
  const contentTypes = parseContentTypesXml(void 0);
  const rootRelationships = {
    path: "_rels/.rels",
    relationships: [
      {
        id: "rId1",
        type: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
        target: "word/document.xml",
        resolvedTarget: "word/document.xml"
      }
    ]
  };
  return createWordPortPackage(parts, contentTypes, /* @__PURE__ */ new Map([["_rels/.rels", rootRelationships]]), rootRelationships);
}
function finalizeExportOpcMetadata(wordPackage, diagnostics) {
  const documentRelationships = reconcileWordPortDocumentRelationshipsInPackage(wordPackage);
  diagnostics.merge(documentRelationships.diagnostics);
  const metadata = syncWordPortPackageMetadata({
    baseFiles: wordPackage,
    strict: false
  });
  diagnostics.merge(metadata.diagnostics);
  wordPackage.setText("[Content_Types].xml", metadata.contentTypesXml);
  wordPackage.setText("_rels/.rels", metadata.rootRelationshipsXml);
  diagnostics.merge(validateWordPortPackage(wordPackage));
}
function exportHeaderFooterParts(document, wordPackage, diagnostics, mainDocumentPart) {
  for (const headerFooter of [
    ...Object.values(document.headers ?? {}),
    ...Object.values(document.footers ?? {})
  ]) {
    const root = exportHeaderFooterXml(headerFooter, {
      package: wordPackage,
      diagnostics,
      partPath: headerFooter.partPath
    });
    upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: headerFooter.partPath }, {
      path: headerFooter.partPath,
      text: serializeWordPortXml({ root }),
      contentType: headerFooter.kind === "header" ? WORD_PORT_CONTENT_TYPES.header : WORD_PORT_CONTENT_TYPES.footer,
      relationshipType: headerFooter.kind === "header" ? WORD_PORT_RELATIONSHIP_TYPES.header : WORD_PORT_RELATIONSHIP_TYPES.footer,
      relationshipId: headerFooter.relationshipId,
      sourcePart: mainDocumentPart
    });
  }
}
function exportHeaderFooterXml(headerFooter, context) {
  const rootName = headerFooter.kind === "header" ? "w:hdr" : "w:ftr";
  const preservedChildren = (headerFooter.raw?.children ?? []).filter((child2) => child2.name !== "w:p" && child2.name !== "w:tbl" && child2.name !== "w:sdt").map((child2) => cloneWordPortXmlNode(child2));
  return createXmlNode(
    rootName,
    headerFooter.attributes ?? headerFooter.raw?.attributes,
    [
      ...preservedChildren,
      ...headerFooter.blocks.map((block) => exportBlock(block, context))
    ]
  );
}
function exportAnnotationParts(document, wordPackage, diagnostics, mainDocumentPart) {
  exportCommentsParts(document, wordPackage, diagnostics, mainDocumentPart);
  exportNotesPart(document.footnotes, "footnote", wordPackage, diagnostics, mainDocumentPart);
  exportNotesPart(document.endnotes, "endnote", wordPackage, diagnostics, mainDocumentPart);
}
function exportCommentsParts(document, wordPackage, diagnostics, mainDocumentPart) {
  if (!document.comments) return;
  const comments = liveWordPortComments(document.comments);
  if (!comments.length) {
    deleteWordPortManagedPart(wordPackage, {
      ...WORD_PORT_MANAGED_RELATED_PARTS.comments,
      sourcePart: mainDocumentPart
    });
    deleteWordPortManagedPart(wordPackage, {
      ...WORD_PORT_MANAGED_RELATED_PARTS.commentsExtended,
      sourcePart: mainDocumentPart
    });
    deleteWordPortManagedPart(wordPackage, {
      ...WORD_PORT_MANAGED_RELATED_PARTS.commentsIds,
      sourcePart: mainDocumentPart
    });
    return;
  }
  upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: WORD_PORT_MANAGED_RELATED_PARTS.comments.path }, {
    ...WORD_PORT_MANAGED_RELATED_PARTS.comments,
    sourcePart: mainDocumentPart,
    text: serializeWordPortXml({ root: exportCommentsXml(comments, {
      package: wordPackage,
      diagnostics,
      partPath: WORD_PORT_MANAGED_RELATED_PARTS.comments.path
    }) })
  });
  upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: WORD_PORT_MANAGED_RELATED_PARTS.commentsExtended.path }, {
    ...WORD_PORT_MANAGED_RELATED_PARTS.commentsExtended,
    sourcePart: mainDocumentPart,
    text: serializeWordPortXml({ root: exportWordPortCommentsExtendedXml(comments) })
  });
  if (wordPackage.getText(WORD_PORT_MANAGED_RELATED_PARTS.commentsIds.path)) {
    upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: WORD_PORT_MANAGED_RELATED_PARTS.commentsIds.path }, {
      ...WORD_PORT_MANAGED_RELATED_PARTS.commentsIds,
      sourcePart: mainDocumentPart,
      text: serializeWordPortXml({ root: exportWordPortCommentsIdsXml(comments) })
    });
  }
}
function exportCommentsXml(comments, context) {
  return createXmlNode("w:comments", {
    "xmlns:w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "xmlns:w14": "http://schemas.microsoft.com/office/word/2010/wordml",
    "xmlns:w15": "http://schemas.microsoft.com/office/word/2012/wordml"
  }, (comments ?? []).map((comment) => {
    const paraId = ensureWordPortCommentParaId(comment);
    const attributes = {
      ...filterCommentAttributes(comment.attributes),
      "w:id": comment.id,
      ...comment.author ? { "w:author": comment.author } : {},
      ...comment.initials ? { "w:initials": comment.initials } : {},
      ...comment.date ? { "w:date": comment.date } : {}
    };
    const blocks = comment.blocks.length ? comment.blocks : [{ type: "paragraph", runs: [] }];
    return createXmlNode("w:comment", attributes, blocks.map((block, index) => {
      const exported = exportBlock(block, context);
      if (index === 0 && exported.name === "w:p") {
        exported.attributes = {
          ...exported.attributes ?? {},
          "w14:paraId": paraId
        };
      }
      return exported;
    }));
  }));
}
function exportNotesPart(notes, kind, wordPackage, diagnostics, mainDocumentPart) {
  if (!notes) return;
  const descriptor = kind === "footnote" ? WORD_PORT_MANAGED_RELATED_PARTS.footnotes : WORD_PORT_MANAGED_RELATED_PARTS.endnotes;
  if (wordPackage.getText(descriptor.path) && !hasEditableWordPortNoteStories(notes)) return;
  const rootName = kind === "footnote" ? "w:footnotes" : "w:endnotes";
  const noteName = kind === "footnote" ? "w:footnote" : "w:endnote";
  const noteContext = {
    package: wordPackage,
    diagnostics,
    partPath: descriptor.path
  };
  const root = createXmlNode(rootName, {
    "xmlns:w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  }, Object.values(notes).map((note) => createXmlNode(noteName, {
    ...note.attributes ?? {},
    "w:id": note.id,
    ...note.noteType ? { "w:type": note.noteType } : {}
  }, note.blocks.map((block) => exportBlock(block, noteContext)))));
  upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: descriptor.path }, {
    ...descriptor,
    sourcePart: mainDocumentPart,
    text: serializeWordPortXml({ root })
  });
}
function filterCommentAttributes(attributes) {
  if (!attributes) return {};
  const allowed = /* @__PURE__ */ new Set(["w:id", "id", "w:author", "author", "w:initials", "initials", "w:date", "date"]);
  return Object.fromEntries(Object.entries(attributes).filter(([name]) => allowed.has(name)));
}
function reconcileDocumentHyperlinkRelationships(document, wordPackage, diagnostics, mainDocumentPart) {
  reconcileHyperlinksInBlocks(document.body.blocks, wordPackage, diagnostics, mainDocumentPart);
  for (const headerFooter of [
    ...Object.values(document.headers ?? {}),
    ...Object.values(document.footers ?? {})
  ]) {
    reconcileHyperlinksInBlocks(headerFooter.blocks, wordPackage, diagnostics, headerFooter.partPath);
  }
}
function reconcileNumberingPart(document, wordPackage, mainDocumentPart, diagnostics) {
  const usedNumbering = collectNumberingReferences(document.body.blocks);
  if (!usedNumbering.size && !document.numbering) return;
  const numberingPart = "word/numbering.xml";
  const root = numberingRootFromCatalog(document.numbering) ?? numberingRootFromPackage(wordPackage, numberingPart, diagnostics) ?? createDefaultNumberingRoot();
  ensureNumberingDefinitions(root, usedNumbering);
  upsertWordPortExportManagedPart({ package: wordPackage, diagnostics, partPath: numberingPart }, {
    path: numberingPart,
    text: serializeWordPortXml({ root }),
    contentType: WORD_PORT_CONTENT_TYPES.numbering,
    relationshipType: WORD_PORT_RELATIONSHIP_TYPES.numbering,
    sourcePart: mainDocumentPart
  });
}
function numberingRootFromCatalog(numbering) {
  return cloneWordPortXmlNode(numbering?.raw);
}
function numberingRootFromPackage(wordPackage, numberingPart, diagnostics) {
  const xml = wordPackage.getText(numberingPart);
  if (!xml) return void 0;
  try {
    const root = parseWordPortXml(xml).root;
    return root && localName2(root.name) === "numbering" ? root : void 0;
  } catch (error) {
    diagnostics.warning("ooxml.parse-error", "Unable to parse existing numbering.xml; a minimal numbering part was synthesized.", {
      path: numberingPart,
      source: "w:numbering",
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
    return void 0;
  }
}
function createDefaultNumberingRoot() {
  return createXmlNode("w:numbering", {
    "xmlns:w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  });
}
function collectNumberingReferences(blocks, target = /* @__PURE__ */ new Map()) {
  for (const block of blocks) {
    if (block.type === "paragraph") {
      const ref = readParagraphNumbering(block.properties);
      if (ref?.numId) {
        const levels = target.get(ref.numId) ?? /* @__PURE__ */ new Set();
        levels.add(ref.level);
        target.set(ref.numId, levels);
      }
    } else if (block.type === "table") {
      for (const row of block.rows) {
        for (const cell of row.cells) collectNumberingReferences(cell.blocks, target);
      }
    } else if (block.type === "blockWrapper") {
      collectNumberingReferences(block.blocks, target);
    }
  }
  return target;
}
function readParagraphNumbering(properties) {
  const numPr = firstXmlChild(properties, "numPr");
  const numId = xmlVal(firstXmlChild(numPr, "numId"));
  if (!numId) return void 0;
  return {
    numId,
    level: readInteger(xmlVal(firstXmlChild(numPr, "ilvl"))) ?? 0
  };
}
function ensureNumberingDefinitions(root, usedNumbering) {
  root.children ??= [];
  for (const [numId, levels] of usedNumbering) {
    const existingNum = root.children.find((child2) => localName2(child2.name) === "num" && xmlAttr(child2, "numId") === numId);
    const abstractId = xmlVal(firstXmlChild(existingNum, "abstractNumId")) ?? defaultAbstractIdForNumId(numId);
    let abstract = root.children.find((child2) => localName2(child2.name) === "abstractNum" && xmlAttr(child2, "abstractNumId") === abstractId);
    if (!abstract) {
      abstract = createDefaultAbstractNumbering(abstractId, numId === "900002" ? "bullet" : "decimal");
      const firstNumIndex = root.children.findIndex((child2) => localName2(child2.name) === "num");
      if (firstNumIndex >= 0) root.children.splice(firstNumIndex, 0, abstract);
      else root.children.push(abstract);
    }
    ensureAbstractLevels(abstract, levels, numId === "900002" ? "bullet" : "decimal");
    if (!existingNum) {
      root.children.push(createXmlNode("w:num", { "w:numId": numId }, [
        createXmlNode("w:abstractNumId", { "w:val": abstractId })
      ]));
    }
  }
}
function createDefaultAbstractNumbering(abstractId, kind) {
  const abstract = createXmlNode("w:abstractNum", { "w:abstractNumId": abstractId }, [
    createXmlNode("w:multiLevelType", { "w:val": "hybridMultilevel" })
  ]);
  ensureAbstractLevels(abstract, /* @__PURE__ */ new Set([0]), kind);
  return abstract;
}
function ensureAbstractLevels(abstract, levels, kind) {
  abstract.children ??= [];
  for (const level of levels) {
    if (abstract.children.some((child2) => localName2(child2.name) === "lvl" && xmlAttr(child2, "ilvl") === String(level))) continue;
    abstract.children.push(createDefaultLevel(level, kind));
  }
}
function createDefaultLevel(level, kind) {
  const left = 720 + level * 360;
  return createXmlNode("w:lvl", { "w:ilvl": level }, [
    createXmlNode("w:start", { "w:val": "1" }),
    createXmlNode("w:numFmt", { "w:val": kind === "bullet" ? "bullet" : defaultOrderedFormat(level) }),
    createXmlNode("w:lvlText", { "w:val": kind === "bullet" ? "\u2022" : `%${level + 1}.` }),
    createXmlNode("w:suff", { "w:val": "tab" }),
    createXmlNode("w:lvlJc", { "w:val": "left" }),
    createXmlNode("w:pPr", void 0, [
      createXmlNode("w:ind", { "w:left": left, "w:hanging": "360" })
    ])
  ]);
}
function defaultOrderedFormat(level) {
  if (level % 3 === 1) return "lowerLetter";
  if (level % 3 === 2) return "lowerRoman";
  return "decimal";
}
function firstXmlChild(node, childLocalName) {
  return node?.children?.find((child2) => localName2(child2.name) === childLocalName);
}
function xmlAttr(node, attrLocalName) {
  if (!node?.attributes) return void 0;
  return node.attributes[attrLocalName] ?? node.attributes[`w:${attrLocalName}`] ?? Object.entries(node.attributes).find(([name]) => localName2(name) === attrLocalName)?.[1];
}
function xmlVal(node) {
  return xmlAttr(node, "val");
}
function localName2(name) {
  if (!name) return "";
  const index = name.indexOf(":");
  return index >= 0 ? name.slice(index + 1) : name;
}
function readInteger(value) {
  if (typeof value === "number" && Number.isInteger(value)) return value;
  if (typeof value !== "string" || !value.trim()) return void 0;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) ? parsed : void 0;
}
function defaultAbstractIdForNumId(numId) {
  if (numId === "900001" || numId === "900002") return numId;
  return `900${numId.replace(/\D/g, "") || "0"}`;
}
function reconcileHyperlinksInBlocks(blocks, wordPackage, diagnostics, sourcePart) {
  for (const block of blocks) {
    if (block.type === "paragraph") reconcileHyperlinksInInlines(block.runs, wordPackage, diagnostics, sourcePart);
    else if (block.type === "table") {
      for (const row of block.rows) {
        for (const cell of row.cells) reconcileHyperlinksInBlocks(cell.blocks, wordPackage, diagnostics, sourcePart);
      }
    } else if (block.type === "blockWrapper") {
      reconcileHyperlinksInBlocks(block.blocks, wordPackage, diagnostics, sourcePart);
    }
  }
}
function reconcileHyperlinksInInlines(inlines, wordPackage, diagnostics, sourcePart) {
  for (const inline of inlines) {
    if (inline.type === "run") reconcileHyperlinksInInlines(inline.content, wordPackage, diagnostics, sourcePart);
    else if (inline.type === "inlineWrapper") reconcileHyperlinksInInlines(inline.content, wordPackage, diagnostics, sourcePart);
    else if (inline.type === "hyperlink") {
      reconcileHyperlinksInInlines(inline.content, wordPackage, diagnostics, sourcePart);
      if (inline.relationshipId || inline.anchor || !inline.target || inline.target.startsWith("#")) continue;
      inline.relationshipId = ensureWordPortExportExternalRelationship({
        package: wordPackage,
        diagnostics,
        partPath: sourcePart
      }, {
        target: inline.target,
        relationshipType: WORD_PORT_RELATIONSHIP_TYPES.hyperlink,
        diagnosticSource: "ooxml.hyperlink"
      });
    }
  }
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/handlers.ts
var WordPortHandlerRegistry = class {
  handlers = /* @__PURE__ */ new Map();
  register(handler) {
    this.handlers.set(handler.name, handler);
    return this;
  }
  get(name) {
    return this.handlers.get(name);
  }
  importNode(node, context) {
    const handler = this.get(node.name);
    if (!handler) {
      context.diagnostics.warning("ooxml.unsupported-node", `No OOXML handler registered for ${node.name}; preserving raw node.`, {
        path: context.partPath,
        source: node.name
      });
      return void 0;
    }
    return handler.importNode(node, context);
  }
};

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/properties.ts
var propertyMetadata = /* @__PURE__ */ new WeakMap();
var RUN_PROPERTY_MAPPED_NODES = /* @__PURE__ */ new Set([
  "w:b",
  "w:i",
  "w:u",
  "w:strike",
  "w:dstrike",
  "w:color",
  "w:highlight",
  "w:rFonts",
  "w:sz",
  "w:shd"
]);
var PARAGRAPH_PROPERTY_MAPPED_NODES = /* @__PURE__ */ new Set([
  "w:pStyle",
  "w:jc",
  "w:numPr",
  "w:spacing",
  "w:tabs",
  "w:keepNext",
  "w:keepLines",
  "w:pageBreakBefore",
  "w:widowControl",
  "w:contextualSpacing"
]);
var SUPPORTED_NUMBERING_FORMATS = /* @__PURE__ */ new Set([
  "bullet",
  "decimal",
  "decimalZero",
  "custom",
  "japaneseCounting",
  "lowerLetter",
  "upperLetter",
  "lowerRoman",
  "upperRoman",
  "ordinal",
  "none"
]);
function paragraphPropertiesToPmAttrs(properties) {
  const source = effectiveParagraphProperties(properties) ?? properties;
  const metadata = properties ? propertyMetadata.get(properties) : void 0;
  if (!source) return metadata?.listRendering ? { listRendering: metadata.listRendering } : {};
  const directStyleId = val(child(properties, "w:pStyle"));
  const styleId = directStyleId ?? metadata?.styleId ?? val(child(source, "w:pStyle"));
  const alignment = val(child(source, "w:jc"));
  const numbering = readNumbering(source);
  const metadataNumbering = metadata?.listRendering ? { numId: metadata.listRendering.numberingId, level: String(metadata.listRendering.level) } : void 0;
  const spacing = readSpacing(source);
  const tabs = readTabs(source);
  const keepNext = readOnOffChild(source, "w:keepNext");
  const keepLines = readOnOffChild(source, "w:keepLines");
  const pageBreakBefore = readOnOffChild(source, "w:pageBreakBefore");
  const widowControl = readOnOffChild(source, "w:widowControl");
  const contextualSpacing = readOnOffChild(source, "w:contextualSpacing");
  return {
    ...styleId ? { styleId, pStyle: styleId } : {},
    ...alignment ? { alignment, align: alignment, textAlign: alignment } : {},
    ...numbering || metadataNumbering ? {
      numbering: numbering ?? metadataNumbering,
      numId: numbering?.numId ?? metadataNumbering?.numId,
      numberingLevel: numbering?.level ?? metadataNumbering?.level
    } : {},
    ...spacing ? {
      spacing,
      spacingBefore: spacing.before,
      spacingAfter: spacing.after,
      lineSpacing: spacing.line,
      lineHeight: spacing.line,
      lineRule: spacing.lineRule
    } : {},
    ...tabs?.length ? { tabs } : {},
    ...keepNext != null ? { keepNext } : {},
    ...keepLines != null ? { keepLines } : {},
    ...pageBreakBefore != null ? { pageBreakBefore } : {},
    ...widowControl != null ? { widowControl } : {},
    ...contextualSpacing != null ? { contextualSpacing } : {},
    ...metadata?.listRendering ? { listRendering: metadata.listRendering } : {}
  };
}
function diagnoseRunProperties(properties, diagnostics, path) {
  diagnoseUnmapped(properties, RUN_PROPERTY_MAPPED_NODES, diagnostics, "run", path);
}
function diagnoseParagraphProperties(properties, diagnostics, path) {
  diagnoseUnmapped(properties, PARAGRAPH_PROPERTY_MAPPED_NODES, diagnostics, "paragraph", path);
}
function readWordPortStyleCatalog(xml) {
  if (!xml) return void 0;
  const parsed = parseCatalogXml(xml, "w:styles");
  const root = parsed?.root;
  if (!root) return void 0;
  const catalog = {
    styles: {},
    raw: cloneWordPortXmlNode(root)
  };
  const docDefaults = firstChildByLocalName(root, "docDefaults");
  const runDefaults = firstChildByLocalName(firstChildByLocalName(docDefaults, "rPrDefault"), "rPr");
  const paragraphDefaults = firstChildByLocalName(firstChildByLocalName(docDefaults, "pPrDefault"), "pPr");
  if (runDefaults || paragraphDefaults) {
    catalog.docDefaults = {
      ...paragraphDefaults ? { paragraph: cloneWordPortXmlNode(paragraphDefaults) } : {},
      ...runDefaults ? { run: cloneWordPortXmlNode(runDefaults) } : {}
    };
  }
  for (const style of childrenByLocalName(root, "style")) {
    const id = attr(style, "w:styleId");
    if (!id) continue;
    const type = normalizeStyleType(attr(style, "w:type"));
    const definition = {
      id,
      type,
      name: attr(firstChildByLocalName(style, "name"), "w:val") ?? id,
      basedOn: attr(firstChildByLocalName(style, "basedOn"), "w:val"),
      next: attr(firstChildByLocalName(style, "next"), "w:val"),
      linkedStyleId: attr(firstChildByLocalName(style, "link"), "w:val"),
      isDefault: ["1", "true", "on"].includes((attr(style, "w:default") ?? "").toLowerCase()),
      paragraph: cloneWordPortXmlNode(firstChildByLocalName(style, "pPr")),
      run: cloneWordPortXmlNode(firstChildByLocalName(style, "rPr")),
      table: cloneWordPortXmlNode(firstChildByLocalName(style, "tblPr")),
      raw: cloneWordPortXmlNode(style)
    };
    catalog.styles[id] = definition;
    if (definition.isDefault && definition.type === "paragraph") catalog.defaultParagraphStyleId = id;
    if (definition.isDefault && definition.type === "character") catalog.defaultCharacterStyleId = id;
    if (definition.isDefault && definition.type === "table") catalog.defaultTableStyleId = id;
  }
  return catalog;
}
function readWordPortNumberingCatalog(xml, diagnostics, path = "word/numbering.xml") {
  if (!xml) return void 0;
  const parsed = parseCatalogXml(xml, "w:numbering");
  const root = parsed?.root;
  if (!root) return void 0;
  const catalog = {
    abstractNums: {},
    nums: {},
    raw: cloneWordPortXmlNode(root)
  };
  for (const pictureBullet of childrenByLocalName(root, "numPicBullet")) {
    diagnostics?.warning("ooxml.unsupported-node", "Picture bullet definitions are preserved but render with fallback bullet glyphs.", {
      path,
      source: pictureBullet.name,
      detail: { numPicBulletId: attr(pictureBullet, "w:numPicBulletId") }
    });
  }
  for (const abstractNode of childrenByLocalName(root, "abstractNum")) {
    const id = attr(abstractNode, "w:abstractNumId");
    if (!id) continue;
    catalog.abstractNums[id] = {
      id,
      name: attr(firstChildByLocalName(abstractNode, "name"), "w:val"),
      nsid: attr(firstChildByLocalName(abstractNode, "nsid"), "w:val"),
      tmpl: attr(firstChildByLocalName(abstractNode, "tmpl"), "w:val"),
      multiLevelType: attr(firstChildByLocalName(abstractNode, "multiLevelType"), "w:val"),
      styleLink: attr(firstChildByLocalName(abstractNode, "styleLink"), "w:val"),
      numStyleLink: attr(firstChildByLocalName(abstractNode, "numStyleLink"), "w:val"),
      levels: childrenByLocalName(abstractNode, "lvl").map((level) => readNumberingLevel(level, diagnostics, path)),
      raw: cloneWordPortXmlNode(abstractNode)
    };
  }
  for (const numNode of childrenByLocalName(root, "num")) {
    const id = attr(numNode, "w:numId");
    if (!id) continue;
    const overrides = Object.fromEntries(childrenByLocalName(numNode, "lvlOverride").map((override) => {
      const level = readInteger2(attr(override, "w:ilvl")) ?? 0;
      const start = readInteger2(attr(firstChildByLocalName(override, "startOverride"), "w:val"));
      const levelOverrideNode = firstChildByLocalName(override, "lvl");
      return [String(level), {
        level,
        ...start != null ? { start } : {},
        ...levelOverrideNode ? { levelOverride: readNumberingLevel(levelOverrideNode, diagnostics, path) } : {},
        raw: cloneWordPortXmlNode(override)
      }];
    }));
    catalog.nums[id] = {
      id,
      abstractId: attr(firstChildByLocalName(numNode, "abstractNumId"), "w:val") ?? "",
      ...Object.keys(overrides).length ? { overrides } : {},
      raw: cloneWordPortXmlNode(numNode)
    };
  }
  return catalog;
}
function annotateParagraphProperties(properties, metadata) {
  if (!properties) return void 0;
  propertyMetadata.set(properties, metadata);
  return properties;
}
function annotateRunProperties(properties, metadata) {
  if (!properties) return void 0;
  propertyMetadata.set(properties, metadata);
  return properties;
}
function resolveCascadedParagraphProperties(properties, styles, numbering) {
  const directStyleId = val(child(properties, "w:pStyle"));
  const styleId = resolveStyleIdForType(styles, directStyleId ?? styles?.defaultParagraphStyleId, "paragraph");
  let paragraph = cloneWordPortXmlNode(styles?.docDefaults?.paragraph);
  let run = cloneWordPortXmlNode(styles?.docDefaults?.run);
  for (const style of styleChain(styles, styleId, "paragraph")) {
    paragraph = mergePropertyNodes(paragraph, style.paragraph, "w:pPr");
    run = mergePropertyNodes(run, style.run, "w:rPr");
  }
  paragraph = mergePropertyNodes(paragraph, properties, "w:pPr");
  const resolvedNumbering = resolveNumberingReference(paragraph, numbering, styles);
  const level = resolvedNumbering ? resolveNumberingLevel(numbering, resolvedNumbering.numberingId, resolvedNumbering.level, styles) : void 0;
  if (level?.level.paragraph) paragraph = mergePropertyNodes(paragraph, level.level.paragraph, "w:pPr");
  if (level?.level.run) run = mergePropertyNodes(run, level.level.run, "w:rPr");
  return {
    ...paragraph ? { paragraph } : {},
    ...run ? { run } : {},
    ...styleId ? { styleId } : {}
  };
}
function resolveCascadedRunProperties(properties, paragraphMetadata, styles) {
  const characterStyleId = resolveStyleIdForType(styles, val(child(properties, "w:rStyle")) ?? styles?.defaultCharacterStyleId, "character");
  let run = cloneWordPortXmlNode(paragraphMetadata?.run ?? styles?.docDefaults?.run);
  for (const style of styleChain(styles, characterStyleId, "character")) {
    run = mergePropertyNodes(run, style.run, "w:rPr");
  }
  run = mergePropertyNodes(run, properties, "w:rPr");
  return {
    ...run ? { run } : {},
    ...characterStyleId ? { characterStyleId } : {}
  };
}
function createWordPortNumberingState(numbering, styles) {
  return { numbering, styles, scopes: {}, counters: {}, nextPosition: 0 };
}
function nextWordPortListRendering(state, paragraphAttrs) {
  const numbering = state?.numbering;
  if (!numbering) return void 0;
  const ref = numberingReferenceFromAttrs(paragraphAttrs) ?? resolveStyleNumberingReference(numbering, stringAttr2(paragraphAttrs?.styleId) ?? stringAttr2(paragraphAttrs?.pStyle), state.styles);
  if (!ref) return void 0;
  const resolved = resolveNumberingLevel(numbering, ref.numberingId, ref.level, state.styles);
  if (!resolved) return void 0;
  const pos = state.nextPosition++;
  const scopeKey = resolved.startOverridden ? `num:${resolved.instance.id}` : `abstract:${resolved.abstract.id}`;
  const count = calculateNumberingCounter(state, scopeKey, resolved, pos);
  recordNumberingCounter(state, scopeKey, resolved.level.level, pos, count);
  const path = calculateNumberingPath(state, scopeKey, resolved, pos, count);
  const markerText = resolved.level.format === "bullet" ? normalizeBulletMarker(resolved.level.text) : formatMarkerTemplate(resolved.level.text || "%1.", resolved.level, path, resolved.abstract, resolved.instance, numbering);
  return {
    markerText,
    markerSuffixText: markerSuffixText(resolved.level.suffix),
    numberingId: resolved.instance.id,
    abstractId: resolved.abstract.id,
    level: resolved.level.level,
    path,
    numberingType: resolved.level.formatValue ?? resolved.level.format,
    suffix: resolved.level.suffix
  };
}
function diagnoseUnmapped(properties, mapped, diagnostics, kind, path) {
  const seen = /* @__PURE__ */ new Set();
  for (const property of getChildren(properties)) {
    if (mapped.has(property.name) || seen.has(property.name)) continue;
    seen.add(property.name);
    diagnostics.info("ooxml.unsupported-node", `Unmapped ${kind} property ${property.name} was preserved raw.`, {
      path,
      source: property.name
    });
  }
}
function parseCatalogXml(xml, rootName) {
  const parsed = parseWordPortXml(xml);
  if (parsed.root?.name === rootName || localName3(parsed.root?.name) === localName3(rootName)) return parsed;
  const root = firstChildByLocalName(parsed.root, localName3(rootName));
  return root ? { ...parsed, root } : parsed;
}
function readNumberingLevel(level, diagnostics, path = "word/numbering.xml") {
  const formatNode = firstChildByLocalName(level, "numFmt") ?? readAlternateContentNumberFormat(level);
  const rawFormat = attr(formatNode, "w:val") ?? "decimal";
  const format = SUPPORTED_NUMBERING_FORMATS.has(rawFormat) ? rawFormat : "decimal";
  if (!SUPPORTED_NUMBERING_FORMATS.has(rawFormat)) {
    diagnostics?.warning("ooxml.unsupported-node", "Unsupported numbering format is preserved and rendered as decimal.", {
      path,
      source: "w:numFmt",
      detail: {
        format: rawFormat,
        level: attr(level, "w:ilvl")
      }
    });
  }
  const pictureBullet = firstChildByLocalName(level, "lvlPicBulletId");
  if (pictureBullet) {
    diagnostics?.warning("ooxml.unsupported-node", "Picture bullet level is preserved but rendered with fallback bullet glyphs.", {
      path,
      source: "w:lvlPicBulletId",
      detail: {
        level: attr(level, "w:ilvl"),
        id: attr(pictureBullet, "w:val")
      }
    });
  }
  return {
    level: readInteger2(attr(level, "w:ilvl")) ?? 0,
    start: readInteger2(attr(firstChildByLocalName(level, "start"), "w:val")),
    restart: readInteger2(attr(firstChildByLocalName(level, "lvlRestart"), "w:val")),
    format,
    formatValue: rawFormat,
    text: attr(firstChildByLocalName(level, "lvlText"), "w:val") ?? "%1.",
    suffix: normalizeSuffix(attr(firstChildByLocalName(level, "suff"), "w:val")),
    justification: normalizeJustification(attr(firstChildByLocalName(level, "lvlJc"), "w:val")),
    styleId: attr(firstChildByLocalName(level, "pStyle"), "w:val"),
    isLegal: Boolean(firstChildByLocalName(level, "isLgl")),
    paragraph: cloneWordPortXmlNode(firstChildByLocalName(level, "pPr")),
    run: cloneWordPortXmlNode(firstChildByLocalName(level, "rPr")),
    raw: cloneWordPortXmlNode(level)
  };
}
function readAlternateContentNumberFormat(level) {
  for (const alternate of childrenByLocalName(level, "AlternateContent")) {
    for (const choice of childrenByLocalName(alternate, "Choice")) {
      const formatNode = firstChildByLocalName(choice, "numFmt");
      if (formatNode) return formatNode;
    }
  }
  return void 0;
}
function normalizeStyleType(value) {
  if (value === "character" || value === "table" || value === "numbering") return value;
  return "paragraph";
}
function normalizeSuffix(value) {
  if (value === "space" || value === "nothing" || value === "tab") return value;
  return void 0;
}
function normalizeJustification(value) {
  if (value === "center" || value === "right" || value === "left") return value;
  return void 0;
}
function styleChain(styles, styleId, type) {
  if (!styles || !styleId) return [];
  const chain = [];
  const seen = /* @__PURE__ */ new Set();
  let currentId = resolveStyleIdForType(styles, styleId, type);
  while (currentId && !seen.has(currentId)) {
    seen.add(currentId);
    const current = styles.styles[currentId];
    if (!current) break;
    if (current.type === type) chain.push(current);
    currentId = current.basedOn;
  }
  return chain.reverse();
}
function resolveStyleIdForType(styles, styleId, type) {
  if (!styles || !styleId) return styleId;
  const direct = styles.styles[styleId];
  if (!direct) return styleId;
  if (direct.type === type) return direct.id;
  const linked = direct.linkedStyleId ? styles.styles[direct.linkedStyleId] : void 0;
  if (linked?.type === type) return linked.id;
  const reverseLinked = Object.values(styles.styles).find((candidate) => candidate.type === type && candidate.linkedStyleId === direct.id);
  return reverseLinked?.id ?? styleId;
}
function mergePropertyNodes(base, override, fallbackName) {
  if (!base && !override) return void 0;
  const next = cloneWordPortXmlNode(base) ?? createXmlNode(fallbackName);
  const children = [...next.children ?? []];
  for (const childNode of getChildren(override)) {
    const replaceableName = childNode.name;
    for (let index = children.length - 1; index >= 0; index -= 1) {
      if (children[index]?.name === replaceableName) children.splice(index, 1);
    }
    children.push(cloneWordPortXmlNode(childNode));
  }
  next.children = children;
  return next.children.length ? next : void 0;
}
function effectiveParagraphProperties(properties) {
  return properties ? propertyMetadata.get(properties)?.paragraph : void 0;
}
function resolveNumberingReference(paragraph, numbering, styles) {
  const direct = readNumbering(paragraph ?? createXmlNode("w:pPr"));
  if (direct?.numId) return { numberingId: direct.numId, level: readInteger2(direct.level) ?? 0 };
  return resolveStyleNumberingReference(numbering, val(child(paragraph, "w:pStyle")), styles);
}
function resolveStyleNumberingReference(numbering, styleId, styles) {
  if (!numbering || !styleId) return void 0;
  const paragraphStyleId = resolveStyleIdForType(styles, styleId, "paragraph") ?? styleId;
  const styleNumbering = readNumbering(styles?.styles[paragraphStyleId]?.paragraph ?? createXmlNode("w:pPr"));
  if (styleNumbering?.numId) return { numberingId: styleNumbering.numId, level: readInteger2(styleNumbering.level) ?? 0 };
  const candidates = [];
  for (const instance of Object.values(numbering.nums)) {
    const abstract = numbering.abstractNums[instance.abstractId];
    const level = abstract?.levels.find((candidate) => candidate.styleId === paragraphStyleId);
    if (level) candidates.push({ numberingId: instance.id, level: level.level });
    if (abstract?.numStyleLink === paragraphStyleId) {
      const firstLevel = abstract.levels.find((candidate) => candidate.level === 0) ?? abstract.levels[0];
      candidates.push({ numberingId: instance.id, level: firstLevel?.level ?? 0 });
    }
  }
  return candidates.sort((left, right) => compareIds(left.numberingId, right.numberingId) || left.level - right.level)[0];
}
function resolveNumberingLevel(numbering, numberingId, levelIndex, styles, tries = 0) {
  const instance = numbering?.nums[numberingId];
  const abstract = instance ? numbering?.abstractNums[instance.abstractId] : void 0;
  if (numbering && abstract?.numStyleLink && tries < 1) {
    const linkedRef = resolveStyleNumberingReference(numbering, abstract.numStyleLink, styles);
    if (linkedRef && linkedRef.numberingId !== numberingId) {
      const linked = resolveNumberingLevel(numbering, linkedRef.numberingId, levelIndex, styles, tries + 1);
      if (instance && abstract && linked) {
        const override2 = instance.overrides?.[String(levelIndex)];
        return {
          instance,
          abstract,
          level: {
            ...linked.level,
            ...override2?.levelOverride ?? {},
            level: levelIndex,
            start: override2?.start ?? override2?.levelOverride?.start ?? linked.level.start
          },
          startOverridden: override2?.start != null
        };
      }
    }
  }
  const baseLevel = abstract?.levels.find((candidate) => candidate.level === levelIndex);
  if (!instance || !abstract || !baseLevel) return void 0;
  const override = instance.overrides?.[String(levelIndex)];
  return {
    instance,
    abstract,
    level: {
      ...baseLevel,
      ...override?.levelOverride ?? {},
      level: levelIndex,
      start: override?.start ?? override?.levelOverride?.start ?? baseLevel.start
    },
    startOverridden: override?.start != null
  };
}
function calculateNumberingCounter(state, scopeKey, resolved, pos) {
  const level = resolved.level.level;
  const start = resolved.level.start ?? 1;
  const previous = previousCounter(state, scopeKey, level, pos);
  const previousCount = previous ? previous.count : start - 1;
  const restart = resolved.level.restart;
  if (restart === 0) return previousCount + 1;
  if (!previous) return start;
  const usedLowerLevels = usedLowerNumberingLevels(state, scopeKey, level, previous.pos, pos);
  if (!usedLowerLevels.length) return previousCount + 1;
  if (restart == null) return start;
  return usedLowerLevels.some((usedLevel) => usedLevel <= restart) ? start : previousCount + 1;
}
function recordNumberingCounter(state, scopeKey, level, pos, count) {
  state.scopes[scopeKey] ??= {};
  state.scopes[scopeKey][level] = count;
  state.counters[scopeKey] ??= {};
  state.counters[scopeKey][level] ??= [];
  state.counters[scopeKey][level].push({ pos, count });
}
function calculateNumberingPath(state, scopeKey, resolved, pos, currentCount) {
  const path = [];
  for (let level = 0; level < resolved.level.level; level += 1) {
    const ancestor = previousCounter(state, scopeKey, level, pos);
    const ancestorLevel = resolveNumberingLevel(state.numbering, resolved.instance.id, level, state.styles)?.level;
    path.push(ancestor?.count ?? ancestorLevel?.start ?? 1);
  }
  path.push(currentCount);
  return path;
}
function previousCounter(state, scopeKey, level, pos) {
  const entries = state.counters[scopeKey]?.[level] ?? [];
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (entry.pos < pos) return entry;
  }
  return void 0;
}
function usedLowerNumberingLevels(state, scopeKey, level, previousPos, pos) {
  const used = [];
  const scope = state.counters[scopeKey] ?? {};
  for (let lower = 0; lower < level; lower += 1) {
    const wasUsed = (scope[lower] ?? []).some((entry) => entry.pos > previousPos && entry.pos < pos);
    if (wasUsed) used.push(lower);
  }
  return used;
}
function numberingReferenceFromAttrs(attrs) {
  const numbering = attrs?.numbering && typeof attrs.numbering === "object" ? attrs.numbering : void 0;
  const numberingId = stringAttr2(numbering?.numberingId) ?? stringAttr2(numbering?.numId) ?? stringAttr2(attrs?.numberingId) ?? stringAttr2(attrs?.numId);
  if (!numberingId) return void 0;
  return {
    numberingId,
    level: readInteger2(numbering?.level) ?? readInteger2(attrs?.numberingLevel) ?? 0
  };
}
function formatMarkerTemplate(template, currentLevel, path, abstract, instance, numbering) {
  return template.replace(/%(\d+)/g, (_match, rawIndex) => {
    const index = Math.max(0, Number(rawIndex) - 1);
    const level = resolveNumberingLevel(numbering, instance.id, index)?.level ?? abstract.levels[index] ?? currentLevel;
    return formatNumber(path[index] ?? 1, level, Boolean(currentLevel.isLegal));
  });
}
function formatNumber(value, level, forceDecimal) {
  const normalized = Math.max(1, Math.floor(value));
  if (forceDecimal) return String(normalized);
  switch (level.format) {
    case "none":
      return "";
    case "ordinal":
      return toOrdinal(normalized);
    case "japaneseCounting":
      return toJapaneseCounting(normalized);
    case "upperRoman":
      return toRoman(normalized).toUpperCase();
    case "lowerRoman":
      return toRoman(normalized).toLowerCase();
    case "upperLetter":
      return toLetters(normalized).toUpperCase();
    case "lowerLetter":
      return toLetters(normalized).toLowerCase();
    case "decimalZero":
      return String(normalized).padStart(2, "0");
    default:
      return String(normalized);
  }
}
function toLetters(value) {
  const letter = String.fromCharCode(97 + (value - 1) % 26);
  return letter.repeat(Math.floor((value - 1) / 26) + 1);
}
function toRoman(value) {
  let current = Math.max(1, Math.min(3999, value));
  const numerals = [[1e3, "M"], [900, "CM"], [500, "D"], [400, "CD"], [100, "C"], [90, "XC"], [50, "L"], [40, "XL"], [10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"]];
  let output = "";
  for (const [amount, numeral] of numerals) {
    while (current >= amount) {
      output += numeral;
      current -= amount;
    }
  }
  return output;
}
function normalizeBulletMarker(text) {
  if (!text || text === "%1.") return "\u2022";
  if (text === "\uF0B7") return "\u2022";
  if (text === "\uF0A7") return "\u25AA";
  if (text === "\u25CB" || text === "o") return "\u25E6";
  if (text === "\u25A0") return "\u25AA";
  if (text === "\u25A1") return "\u25EF";
  return text;
}
function toOrdinal(value) {
  const suffixes = ["th", "st", "nd", "rd"];
  const lastTwo = value % 100;
  const suffix = suffixes[(lastTwo - 20) % 10] || suffixes[lastTwo] || suffixes[0];
  return `${value}${suffix}`;
}
function toJapaneseCounting(value) {
  const digits = ["", "\u4E00", "\u4E8C", "\u4E09", "\u56DB", "\u4E94", "\u516D", "\u4E03", "\u516B", "\u4E5D"];
  const units = ["", "\u5341", "\u767E", "\u5343"];
  if (value === 0) return "\u96F6";
  if (value < 10) return digits[value] ?? "";
  let output = "";
  let current = value;
  let unitIndex = 0;
  while (current > 0 && unitIndex < units.length) {
    const digit = current % 10;
    if (digit !== 0) {
      output = `${digit === 1 && unitIndex > 0 ? "" : digits[digit]}${units[unitIndex]}${output}`;
    } else if (output && current % 100 !== 0 && !output.startsWith("\u96F6")) {
      output = `\u96F6${output}`;
    }
    current = Math.floor(current / 10);
    unitIndex += 1;
  }
  return value >= 10 && value < 20 ? output.replace(/^一十/, "\u5341") : output;
}
function markerSuffixText(suffix) {
  if (suffix === "space") return " ";
  if (suffix === "nothing") return "";
  return "	";
}
function compareIds(left, right) {
  const leftNumber = Number(left);
  const rightNumber = Number(right);
  if (Number.isFinite(leftNumber) && Number.isFinite(rightNumber)) return leftNumber - rightNumber;
  return left.localeCompare(right);
}
function attr(node, name) {
  if (!node?.attributes) return void 0;
  return node.attributes[name] ?? node.attributes[localName3(name)];
}
function childrenByLocalName(node, name) {
  return getChildren(node).filter((candidate) => localName3(candidate.name) === name);
}
function firstChildByLocalName(node, name) {
  return childrenByLocalName(node, name)[0];
}
function localName3(name) {
  return name?.includes(":") ? name.split(":").pop() ?? name : name ?? "";
}
function readInteger2(value) {
  if (typeof value === "number" && Number.isFinite(value)) return Math.floor(value);
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number.parseInt(value, 10);
    if (Number.isFinite(parsed)) return parsed;
  }
  return void 0;
}
function child(node, name) {
  return getChildren(node, name)[0];
}
function val(node) {
  return node?.attributes?.["w:val"] ?? node?.attributes?.val;
}
function stringAttr2(value) {
  return typeof value === "string" && value.trim() ? value.trim() : void 0;
}
function booleanAttr(value) {
  if (typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value !== 0;
  if (typeof value !== "string") return void 0;
  const normalized = value.trim().toLowerCase();
  if (normalized === "true" || normalized === "1" || normalized === "on") return true;
  if (normalized === "false" || normalized === "0" || normalized === "off") return false;
  return void 0;
}
function readNumbering(properties) {
  const numPr = child(properties, "w:numPr");
  if (!numPr) return void 0;
  const numId = val(child(numPr, "w:numId"));
  const level = val(child(numPr, "w:ilvl"));
  return numId || level ? { numId, level } : void 0;
}
function readSpacing(properties) {
  const spacing = child(properties, "w:spacing");
  if (!spacing?.attributes) return void 0;
  const result = {};
  for (const key of ["before", "after", "line", "lineRule"]) {
    const value = spacing.attributes[`w:${key}`] ?? spacing.attributes[key];
    if (value != null) result[key] = value;
  }
  return Object.keys(result).length ? result : void 0;
}
function readTabs(properties) {
  const tabs = child(properties, "w:tabs");
  if (!tabs?.children?.length) return void 0;
  const result = [];
  for (const tab of tabs.children) {
    if (tab.name !== "w:tab") continue;
    const rawVal = tab.attributes?.["w:val"] ?? tab.attributes?.val;
    const rawPos = tab.attributes?.["w:pos"] ?? tab.attributes?.pos;
    const pos = Number(rawPos);
    if (!Number.isFinite(pos)) continue;
    result.push({
      val: normalizeTabVal(rawVal),
      pos,
      originalPos: pos,
      ...tab.attributes?.["w:leader"] || tab.attributes?.leader ? { leader: normalizeTabLeader(tab.attributes?.["w:leader"] ?? tab.attributes?.leader) } : {}
    });
  }
  return result.length ? result : void 0;
}
function readOnOffChild(properties, name) {
  const node = child(properties, name);
  if (!node) return void 0;
  const raw = node.attributes?.["w:val"] ?? node.attributes?.val;
  if (raw == null) return true;
  return booleanAttr(raw) ?? true;
}
function normalizeTabVal(value) {
  const normalized = value?.trim();
  if (normalized === "right") return "end";
  if (normalized === "left") return "start";
  if (normalized === "start" || normalized === "end" || normalized === "center" || normalized === "decimal" || normalized === "bar" || normalized === "clear" || normalized === "num") return normalized;
  return "start";
}
function normalizeTabLeader(value) {
  const normalized = value?.trim();
  if (normalized === "dot" || normalized === "heavy" || normalized === "hyphen" || normalized === "middleDot" || normalized === "underscore" || normalized === "none") return normalized;
  return "none";
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/import.ts
async function importDocxToWordPortDocument(wordPackage, options = {}) {
  const diagnostics = new WordPortDiagnostics();
  diagnostics.merge(wordPackage.diagnostics);
  const importDiagnosticStart = diagnostics.items.length;
  const mainDocumentPart = options.mainDocumentPart ?? resolveMainDocumentPart(wordPackage) ?? "word/document.xml";
  const documentXml = wordPackage.getText(mainDocumentPart);
  if (!documentXml) {
    diagnostics.error("opc.missing-part", "Main document part is missing.", { path: mainDocumentPart });
    wordPackage.diagnostics.push(...diagnostics.items.slice(importDiagnosticStart));
    return {
      type: "document",
      package: { mainDocumentPart },
      body: { type: "body", blocks: [] },
      source: {}
    };
  }
  let parsed;
  try {
    parsed = parseWordPortXml(documentXml);
  } catch (error) {
    diagnostics.error("ooxml.parse-error", "Unable to parse word/document.xml.", {
      path: mainDocumentPart,
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
    wordPackage.diagnostics.push(...diagnostics.items.slice(importDiagnosticStart));
    return { type: "document", package: { mainDocumentPart }, body: { type: "body", blocks: [] } };
  }
  const documentRoot = parsed.root;
  const bodyNode = firstChild(documentRoot, "w:body");
  const context = {
    package: wordPackage,
    diagnostics,
    partPath: mainDocumentPart,
    relationships: wordPackage.relationships.get(getRelationshipsPathForPart(mainDocumentPart))
  };
  const styles = readWordPortStyleCatalog(wordPackage.getText(resolveRelatedPart(context, "/styles", "word/styles.xml")));
  const numberingPath = resolveRelatedPart(context, "/numbering", "word/numbering.xml");
  const numbering = readWordPortNumberingCatalog(wordPackage.getText(numberingPath), diagnostics, numberingPath);
  const richContext = {
    ...context,
    styles,
    numbering,
    numberingState: createWordPortNumberingState(numbering, styles),
    story: { kind: "body", partPath: mainDocumentPart },
    sourceOrdinalByKey: {}
  };
  const body = bodyNode ? importBody(bodyNode, richContext) : { type: "body", blocks: [] };
  const headerFooters = bodyNode ? importHeaderFooters(bodyNode, richContext) : { headers: {}, footers: {} };
  const annotations = importAnnotations(richContext);
  const bookmarks = collectDocumentBookmarks(body, headerFooters, annotations, { kind: "body", partPath: mainDocumentPart });
  if (!bodyNode) diagnostics.error("ooxml.parse-error", "Document root does not contain w:body.", { path: mainDocumentPart });
  wordPackage.diagnostics.push(...diagnostics.items.slice(importDiagnosticStart));
  return {
    type: "document",
    package: { mainDocumentPart },
    ...Object.keys(headerFooters.headers).length ? { headers: headerFooters.headers } : {},
    ...Object.keys(headerFooters.footers).length ? { footers: headerFooters.footers } : {},
    ...annotations.comments.length ? { comments: annotations.comments } : {},
    ...Object.keys(annotations.footnotes).length ? { footnotes: annotations.footnotes } : {},
    ...Object.keys(annotations.endnotes).length ? { endnotes: annotations.endnotes } : {},
    ...bookmarks.length ? { bookmarks } : {},
    ...styles ? { styles } : {},
    ...numbering ? { numbering } : {},
    body,
    source: {
      documentAttributes: documentRoot?.attributes,
      bodyProperties: bodyNode ? cloneWordPortXmlNode(firstChild(bodyNode, "w:sectPr")) : void 0,
      rawDocument: documentRoot ? cloneWordPortXmlNode(documentRoot) : void 0
    }
  };
}
function importAnnotations(context) {
  const commentsPath = resolveRelatedPart(context, "/comments", "word/comments.xml");
  const comments = importCommentsPart(context, commentsPath);
  const commentMetadata = importCommentExtendedMetadata(context, resolveRelatedPart(context, "/commentsExtended", "word/commentsExtended.xml"));
  const commentIdByParaId = new Map(comments.flatMap((comment) => comment.paraId ? [[comment.paraId, comment.id]] : []));
  const commentsWithMetadata = comments.map((comment) => {
    const meta = comment.paraId ? commentMetadata[comment.paraId] : void 0;
    if (!meta) return comment;
    const parentCommentId = meta.paraIdParent ? commentIdByParaId.get(meta.paraIdParent) : void 0;
    return {
      ...comment,
      done: meta.done ?? comment.done,
      ...parentCommentId ? { parentCommentId } : {}
    };
  });
  return {
    comments: commentsWithMetadata,
    footnotes: importNotesPart(context, resolveRelatedPart(context, "/footnotes", "word/footnotes.xml"), "footnote"),
    endnotes: importNotesPart(context, resolveRelatedPart(context, "/endnotes", "word/endnotes.xml"), "endnote")
  };
}
function importCommentsPart(context, partPath) {
  const xml = context.package.getText(partPath);
  if (!xml) return [];
  try {
    const root = parseWordPortXml(xml).root;
    if (!root) return [];
    return (root.children ?? []).filter((node) => node.name === "w:comment").map((node) => {
      const paraId = readFirstParaId(node);
      const id = node.attributes?.["w:id"] ?? node.attributes?.id ?? "";
      return {
        type: "comment",
        id,
        author: node.attributes?.["w:author"] ?? node.attributes?.author,
        initials: node.attributes?.["w:initials"] ?? node.attributes?.initials,
        date: node.attributes?.["w:date"] ?? node.attributes?.date,
        ...paraId ? { paraId } : {},
        attributes: node.attributes ? { ...node.attributes } : void 0,
        raw: cloneWordPortXmlNode(node),
        blocks: importStoryBlocks(node, {
          ...context,
          partPath,
          relationships: context.package.relationships.get(getRelationshipsPathForPart(partPath)),
          story: { kind: "comment", partPath, id }
        })
      };
    }).filter((comment) => comment.id);
  } catch (error) {
    context.diagnostics.warning("ooxml.parse-error", "Unable to parse comments part.", {
      path: partPath,
      source: "word/comments.xml",
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
    return [];
  }
}
function importCommentExtendedMetadata(context, partPath) {
  const xml = context.package.getText(partPath);
  if (!xml) return {};
  try {
    const root = parseWordPortXml(xml).root;
    if (!root) return {};
    return Object.fromEntries((root.children ?? []).flatMap((node) => {
      const paraId = node.attributes?.["w15:paraId"] ?? node.attributes?.paraId;
      if (!paraId) return [];
      return [[paraId, {
        done: readOoxmlBoolean(node.attributes?.["w15:done"] ?? node.attributes?.done),
        paraIdParent: node.attributes?.["w15:paraIdParent"] ?? node.attributes?.paraIdParent
      }]];
    }));
  } catch {
    return {};
  }
}
function importNotesPart(context, partPath, kind) {
  const xml = context.package.getText(partPath);
  if (!xml) return {};
  const nodeName = kind === "footnote" ? "w:footnote" : "w:endnote";
  try {
    const root = parseWordPortXml(xml).root;
    if (!root) return {};
    return Object.fromEntries((root.children ?? []).filter((node) => node.name === nodeName).map((node) => {
      const id = node.attributes?.["w:id"] ?? node.attributes?.id ?? "";
      return [id, {
        type: "note",
        kind,
        id,
        noteType: node.attributes?.["w:type"] ?? node.attributes?.type,
        attributes: node.attributes ? { ...node.attributes } : void 0,
        raw: cloneWordPortXmlNode(node),
        blocks: importStoryBlocks(node, {
          ...context,
          partPath,
          relationships: context.package.relationships.get(getRelationshipsPathForPart(partPath)),
          story: { kind, partPath, id }
        })
      }];
    }).filter(([id]) => Boolean(id)));
  } catch (error) {
    context.diagnostics.warning("ooxml.parse-error", `Unable to parse ${kind} part.`, {
      path: partPath,
      source: partPath,
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
    return {};
  }
}
function importStoryBlocks(node, context) {
  const blocks = [];
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:sdt") {
      blocks.push(importBlockWrapper(child2, context, "w:sdtContent"));
      continue;
    }
    const imported = registry.importNode(child2, context);
    if (imported && isBlock(imported)) blocks.push(imported);
  }
  return blocks;
}
function readFirstParaId(node) {
  const paragraph = (node.children ?? []).find((child2) => child2.name === "w:p");
  return paragraph?.attributes?.["w14:paraId"] ?? paragraph?.attributes?.paraId;
}
function createDefaultWordPortHandlerRegistry() {
  return new WordPortHandlerRegistry().register({ name: "w:p", importNode: (node, context) => importParagraph(node, context) }).register({ name: "w:r", importNode: (node, context) => importRun(node, context) }).register({ name: "w:t", importNode: (node) => importText(node) }).register({ name: "w:delText", importNode: (node) => importText(node) }).register({ name: "w:br", importNode: (node) => importBreak(node) }).register({ name: "w:tab", importNode: (node) => importTab(node) }).register({ name: "w:commentRangeStart", importNode: (node) => importReference(node) }).register({ name: "w:commentRangeEnd", importNode: (node) => importReference(node) }).register({ name: "w:commentReference", importNode: (node) => importReference(node) }).register({ name: "w:footnoteReference", importNode: (node) => importReference(node) }).register({ name: "w:endnoteReference", importNode: (node) => importReference(node) }).register({ name: "w:bookmarkStart", importNode: (node) => importReference(node) }).register({ name: "w:bookmarkEnd", importNode: (node) => importReference(node) }).register({ name: "w:noBreakHyphen", importNode: (node) => importReference(node) }).register({ name: "w:fldSimple", importNode: (node, context) => importSimpleField(node, context) }).register({ name: "w:hyperlink", importNode: (node, context) => importHyperlink(node, context) }).register({ name: "w:sdt", importNode: (node, context) => importInlineSdt(node, context) }).register({ name: "mc:AlternateContent", importNode: (node, context) => importAlternateContent(node, context) }).register({ name: "w:drawing", importNode: (node, context) => importDrawing(node, context) }).register({ name: "w:pict", importNode: (node, context) => importPictDrawing(node, context) }).register({ name: "w:tbl", importNode: (node, context) => importTable(node, context) });
}
var registry = createDefaultWordPortHandlerRegistry();
function importBody(node, context) {
  const blocks = [];
  const raw = [];
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:sectPr") continue;
    if (child2.name === "w:sdt") {
      blocks.push(importBlockWrapper(child2, context, "w:sdtContent"));
      continue;
    }
    const imported = registry.importNode(child2, context);
    if (imported && isBlock(imported)) blocks.push(imported);
    else raw.push(cloneWordPortXmlNode(child2));
  }
  return { type: "body", blocks, ...raw.length ? { raw } : {} };
}
function collectDocumentBookmarks(body, headerFooters, annotations, bodyStory) {
  return [
    ...collectBookmarksFromBlocks(body.blocks, bodyStory),
    ...Object.values(headerFooters.headers).flatMap(
      (header) => collectBookmarksFromBlocks(header.blocks, {
        kind: "header",
        partPath: header.partPath,
        relationshipId: header.relationshipId,
        ...header.variant ? { variant: header.variant } : {}
      })
    ),
    ...Object.values(headerFooters.footers).flatMap(
      (footer) => collectBookmarksFromBlocks(footer.blocks, {
        kind: "footer",
        partPath: footer.partPath,
        relationshipId: footer.relationshipId,
        ...footer.variant ? { variant: footer.variant } : {}
      })
    ),
    ...annotations.comments.flatMap(
      (comment) => collectBookmarksFromBlocks(comment.blocks, { kind: "comment", partPath: "word/comments.xml", id: comment.id })
    ),
    ...Object.values(annotations.footnotes).flatMap(
      (note) => collectBookmarksFromBlocks(note.blocks, { kind: "footnote", partPath: "word/footnotes.xml", id: note.id })
    ),
    ...Object.values(annotations.endnotes).flatMap(
      (note) => collectBookmarksFromBlocks(note.blocks, { kind: "endnote", partPath: "word/endnotes.xml", id: note.id })
    )
  ];
}
function collectBookmarksFromBlocks(blocks, fallbackStory) {
  const starts = /* @__PURE__ */ new Map();
  const ranges = [];
  const markers = collectBookmarkMarkersFromBlocks(blocks);
  for (const marker of markers) {
    const story = marker.story ?? fallbackStory;
    const markerWithStory = { ...marker, story };
    if (marker.role === "start") {
      starts.set(bookmarkRangeKey(story, marker.id), markerWithStory);
      continue;
    }
    const key = bookmarkRangeKey(story, marker.id);
    const start = starts.get(key);
    if (start) starts.delete(key);
    ranges.push({
      id: marker.id,
      ...start?.name ? { name: start.name } : {},
      ...start?.colFirst ? { colFirst: start.colFirst } : {},
      ...start?.colLast ? { colLast: start.colLast } : {},
      ...start?.displacedByCustomXml ?? marker.displacedByCustomXml ? { displacedByCustomXml: start?.displacedByCustomXml ?? marker.displacedByCustomXml } : {},
      story,
      source: story.partPath,
      complete: Boolean(start),
      ...start ? { start } : {},
      end: markerWithStory
    });
  }
  for (const start of starts.values()) {
    const story = start.story ?? fallbackStory;
    ranges.push({
      id: start.id,
      ...start.name ? { name: start.name } : {},
      ...start.colFirst ? { colFirst: start.colFirst } : {},
      ...start.colLast ? { colLast: start.colLast } : {},
      ...start.displacedByCustomXml ? { displacedByCustomXml: start.displacedByCustomXml } : {},
      story,
      source: story.partPath,
      complete: false,
      start
    });
  }
  return ranges;
}
function collectBookmarkMarkersFromBlocks(blocks) {
  const markers = [];
  for (const block of blocks) {
    if (block.type === "paragraph") collectBookmarkMarkersFromInlines(block.runs, markers);
    else if (block.type === "table") {
      for (const row of block.rows) {
        for (const cell of row.cells) markers.push(...collectBookmarkMarkersFromBlocks(cell.blocks));
      }
    } else if (block.type === "blockWrapper") {
      markers.push(...collectBookmarkMarkersFromBlocks(block.blocks));
    }
  }
  return markers;
}
function collectBookmarkMarkersFromInlines(inlines, markers) {
  for (const inline of inlines) {
    if (inline.type === "reference" && inline.bookmark) markers.push(inline.bookmark);
    else if (inline.type === "run") collectBookmarkMarkersFromInlines(inline.content, markers);
    else if (inline.type === "inlineWrapper") collectBookmarkMarkersFromInlines(inline.content, markers);
    else if (inline.type === "hyperlink") collectBookmarkMarkersFromInlines(inline.content, markers);
  }
}
function bookmarkRangeKey(story, id) {
  return `${story.kind}:${story.partPath}:${id}`;
}
var SOURCE_ANCHOR_PREFIX = {
  paragraph: "para",
  table: "table",
  tableRow: "row",
  tableCell: "cell"
};
function createBlockSourceAnchor(node, context, kind, parentPath) {
  const explicit = readOoxmlIdentity(node);
  const bookmark = kind === "paragraph" ? readFirstBookmarkIdentity(node) : void 0;
  const occurrencePath = parentPath ?? `${storySourceKey(context.story)}/${node.name}[${nextSourceOrdinal(context, node.name)}]`;
  const sourcePart = context.partPath;
  const generatedSourceId = `${SOURCE_ANCHOR_PREFIX[kind]}-${stableHexHash(`${sourcePart}:${occurrencePath}`)}`;
  const sourceId = explicit.paraId ?? bookmark?.sourceId ?? generatedSourceId;
  return {
    sourceId,
    idKind: explicit.paraId ? "ooxml" : bookmark ? "bookmark" : "generated",
    sourcePart,
    nodeName: node.name,
    occurrencePath,
    ...explicit.paraId ? { explicitId: explicit.paraId } : {},
    ...explicit.textId ? { textId: explicit.textId } : {},
    ...bookmark?.id ? { bookmarkId: bookmark.id } : {},
    ...bookmark?.name ? { bookmarkName: bookmark.name } : {},
    ...!explicit.paraId && !bookmark ? { generated: true } : {},
    ...Object.keys(explicit.attributes).length ? { attributes: explicit.attributes } : {}
  };
}
function readOoxmlIdentity(node) {
  const attributes = {};
  for (const key of ["w14:paraId", "w15:paraId", "paraId", "w14:textId", "w15:textId", "textId"]) {
    const value = node.attributes?.[key];
    if (typeof value === "string" && value.length) attributes[key] = value;
  }
  return {
    paraId: attributes["w14:paraId"] ?? attributes["w15:paraId"] ?? attributes.paraId,
    textId: attributes["w14:textId"] ?? attributes["w15:textId"] ?? attributes.textId,
    attributes
  };
}
function readFirstBookmarkIdentity(node) {
  const stack = [...node.children ?? []];
  while (stack.length) {
    const current = stack.shift();
    if (!current) continue;
    if (current.name === "w:bookmarkStart") {
      const id = current.attributes?.["w:id"] ?? current.attributes?.id;
      const name = current.attributes?.["w:name"] ?? current.attributes?.name;
      const sourceId = name ? `bookmark:${name}` : id ? `bookmark-id:${id}` : void 0;
      return sourceId ? { sourceId, ...id ? { id } : {}, ...name ? { name } : {} } : void 0;
    }
    stack.unshift(...current.children ?? []);
  }
  return void 0;
}
function nextSourceOrdinal(context, key) {
  context.sourceOrdinalByKey ??= {};
  const storyKey = `${storySourceKey(context.story)}:${key}`;
  const next = context.sourceOrdinalByKey[storyKey] ?? 0;
  context.sourceOrdinalByKey[storyKey] = next + 1;
  return next;
}
function storySourceKey(story) {
  switch (story.kind) {
    case "body":
      return `body:${story.partPath}`;
    case "header":
    case "footer":
      return `${story.kind}:${story.partPath}:${story.relationshipId}`;
    case "comment":
    case "footnote":
    case "endnote":
      return `${story.kind}:${story.partPath}:${story.id}`;
  }
}
function stableHexHash(input) {
  let hash = 2166136261;
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0").toUpperCase();
}
function importHeaderFooters(bodyNode, context) {
  const headers = {};
  const footers = {};
  const sectionNodes = findDescendants(bodyNode, (node) => node.name === "w:sectPr");
  for (const sectionNode of sectionNodes) {
    for (const reference of sectionNode.children ?? []) {
      if (reference.name !== "w:headerReference" && reference.name !== "w:footerReference") continue;
      const relationshipId = reference.attributes?.["r:id"];
      if (!relationshipId) continue;
      const relationship = context.relationships?.relationships.find((item) => item.id === relationshipId);
      const partPath = relationship?.resolvedTarget;
      if (!partPath) {
        context.diagnostics.warning("ooxml.unsupported-node", "Header/footer relationship target could not be resolved.", {
          path: context.partPath,
          source: reference.name,
          detail: { relationshipId }
        });
        continue;
      }
      const imported = importHeaderFooterPart({
        context,
        kind: reference.name === "w:headerReference" ? "header" : "footer",
        variant: readHeaderFooterVariant(reference.attributes?.["w:type"]),
        relationshipId,
        partPath
      });
      if (!imported) continue;
      if (imported.kind === "header") headers[relationshipId] = imported;
      else footers[relationshipId] = imported;
    }
  }
  return { headers, footers };
}
function importHeaderFooterPart(input) {
  const xml = input.context.package.getText(input.partPath);
  if (!xml) {
    input.context.diagnostics.warning("opc.missing-part", "Referenced header/footer part is missing.", {
      path: input.partPath,
      source: input.kind,
      detail: { relationshipId: input.relationshipId }
    });
    return void 0;
  }
  try {
    const parsed = parseWordPortXml(xml);
    const root = parsed.root;
    const headerFooterContext = {
      ...input.context,
      partPath: input.partPath,
      relationships: input.context.package.relationships.get(getRelationshipsPathForPart(input.partPath)),
      story: {
        kind: input.kind,
        partPath: input.partPath,
        relationshipId: input.relationshipId,
        ...input.variant ? { variant: input.variant } : {}
      }
    };
    return {
      type: "headerFooter",
      kind: input.kind,
      ...input.variant ? { variant: input.variant } : {},
      relationshipId: input.relationshipId,
      partPath: input.partPath,
      attributes: root.attributes ? { ...root.attributes } : void 0,
      raw: cloneWordPortXmlNode(root),
      blocks: importHeaderFooterBlocks(root, headerFooterContext)
    };
  } catch (error) {
    input.context.diagnostics.warning("ooxml.parse-error", "Unable to parse referenced header/footer part.", {
      path: input.partPath,
      source: input.kind,
      detail: { error: error instanceof Error ? error.message : String(error) }
    });
    return void 0;
  }
}
function importHeaderFooterBlocks(root, context) {
  const blocks = [];
  for (const child2 of root.children ?? []) {
    if (child2.name === "w:sdt") {
      blocks.push(importBlockWrapper(child2, context, "w:sdtContent"));
      continue;
    }
    const imported = registry.importNode(child2, context);
    if (imported && isBlock(imported)) blocks.push(imported);
  }
  return blocks;
}
function importParagraph(node, context) {
  const runs = [];
  const raw = [];
  const properties = cloneWordPortXmlNode(firstChild(node, "w:pPr"));
  const source = createBlockSourceAnchor(node, context, "paragraph");
  const effectiveProperties = resolveCascadedParagraphProperties(properties, context.styles, context.numbering);
  const listRendering = nextWordPortListRendering(context.numberingState, paragraphPropertiesToPmAttrs(
    annotateParagraphProperties(properties, effectiveProperties)
  ));
  if (listRendering) effectiveProperties.listRendering = listRendering;
  annotateParagraphProperties(properties, effectiveProperties);
  diagnoseParagraphProperties(properties, context.diagnostics, context.partPath);
  const previousParagraphProperties = context.paragraphProperties;
  context.paragraphProperties = effectiveProperties;
  const children = expandNodesForFieldProcessing((node.children ?? []).filter((child2) => child2.name !== "w:pPr"));
  for (let index = 0; index < children.length; index += 1) {
    const child2 = children[index];
    const nodeForImport = child2.node;
    const complexField = collectComplexField(children, index);
    if (complexField) {
      runs.push(importComplexField(complexField.nodes, context, complexField.complete));
      index = complexField.endIndex;
      continue;
    }
    if (fieldCharType(nodeForImport) === "end") {
      context.diagnostics.warning("ooxml.malformed-field", "Field end marker was found without a matching begin marker.", {
        path: context.partPath,
        source: "w:fldChar"
      });
    }
    if (nodeForImport.name === "w:fldSimple") {
      runs.push(importSimpleField(nodeForImport, context));
      continue;
    }
    if (nodeForImport.name === "w:ins" || nodeForImport.name === "w:del") {
      runs.push(importInlineWrapper(nodeForImport, context));
      continue;
    }
    const imported = registry.importNode(nodeForImport, context);
    if (imported && isInline(imported)) runs.push(imported);
    else raw.push(cloneWordPortXmlNode(nodeForImport));
  }
  context.paragraphProperties = previousParagraphProperties;
  return { type: "paragraph", runs, ...properties ? { properties } : {}, effectiveProperties, source, ...raw.length ? { raw } : {} };
}
function importRun(node, context) {
  const content = [];
  const raw = [];
  let properties = cloneWordPortXmlNode(firstChild(node, "w:rPr"));
  const effectiveProperties = resolveCascadedRunProperties(properties, context.paragraphProperties, context.styles);
  if (!properties && effectiveProperties.run) properties = createXmlNode("w:rPr");
  annotateRunProperties(properties, effectiveProperties);
  diagnoseRunProperties(properties, context.diagnostics, context.partPath);
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:rPr") continue;
    const imported = registry.importNode(child2, context);
    if (imported && isInline(imported)) content.push(imported);
    else raw.push(cloneWordPortXmlNode(child2));
  }
  return { type: "run", content, ...properties ? { properties } : {}, effectiveProperties, ...raw.length ? { raw } : {} };
}
function importText(node) {
  return {
    type: "text",
    text: textContent(node),
    space: node.attributes?.["xml:space"] === "preserve" ? "preserve" : void 0,
    raw: cloneWordPortXmlNode(node)
  };
}
function importBreak(node) {
  return { type: "break", breakType: node.attributes?.["w:type"], raw: cloneWordPortXmlNode(node) };
}
function importTab(node) {
  return { type: "tab", raw: cloneWordPortXmlNode(node) };
}
function importReference(node, context) {
  const bookmark = importBookmarkMarker(node, context?.story);
  return {
    type: "reference",
    name: node.name,
    attributes: node.attributes ? { ...node.attributes } : void 0,
    ...bookmark ? { bookmark } : {},
    raw: cloneWordPortXmlNode(node)
  };
}
function importSimpleField(node, context) {
  const instruction = node.attributes?.["w:instr"] ?? node.attributes?.instr ?? "";
  const field = createFieldReference({
    kind: "simple",
    instruction,
    instructionTokens: [{ type: "text", text: instruction }],
    resultText: visibleTextFromNodes(node.children ?? []),
    rawNode: cloneWordPortXmlNode(node),
    complete: true
  });
  diagnoseImportedField(field, context, "w:fldSimple");
  return fieldReferenceToInline(field);
}
function importComplexField(nodes, context, complete) {
  const logicalNodes = nodes.map((node) => node.node);
  const instructionTokens = instructionTokensFromComplexField(logicalNodes);
  const instruction = instructionTextFromTokens(instructionTokens);
  const field = createFieldReference({
    kind: "complex",
    instruction,
    instructionTokens,
    resultText: resultTextFromComplexField(logicalNodes),
    rawNodes: uniqueRawNodes(nodes).map((node) => cloneWordPortXmlNode(node)),
    complete
  });
  if (!complete) {
    context.diagnostics.warning("ooxml.malformed-field", "Unclosed complex field was preserved as read-only content.", {
      path: context.partPath,
      source: "w:fldChar",
      detail: { instruction }
    });
  }
  diagnoseImportedField(field, context, "w:fldChar");
  return fieldReferenceToInline(field);
}
function diagnoseImportedField(field, context, source) {
  const message = field.supported ? "Word field was imported as read-only structured content." : "Unsupported Word field was preserved as read-only structured content.";
  context.diagnostics.warning(field.supported ? "ooxml.export-partial" : "ooxml.unsupported-node", message, {
    path: context.partPath,
    source,
    detail: {
      instructionType: field.instructionType,
      instruction: field.instruction,
      kind: field.kind,
      complete: field.complete,
      ...field.supported ? {} : { reason: field.structure.nodeName === "sd:unsupportedField" ? field.structure.reason : void 0 }
    }
  });
}
function fieldReferenceToInline(field) {
  return {
    type: "reference",
    name: WORD_PORT_FIELD_REFERENCE_NAME,
    attributes: {
      [WORD_PORT_FIELD_ATTRIBUTE]: JSON.stringify(field),
      "data-word-port-field-type": field.instructionType,
      "data-word-port-field-kind": field.kind
    },
    field
  };
}
function collectComplexField(children, startIndex) {
  const first = children[startIndex];
  if (fieldCharType(first?.node) !== "begin") return void 0;
  const nodes = [];
  let depth = 0;
  for (let index = startIndex; index < children.length; index += 1) {
    const child2 = children[index];
    nodes.push(child2);
    const type = fieldCharType(child2.node);
    if (type === "begin") depth += 1;
    if (type === "end") {
      depth -= 1;
      if (depth <= 0) return { nodes, endIndex: index, complete: true };
    }
  }
  return { nodes, endIndex: children.length - 1, complete: false };
}
function expandNodesForFieldProcessing(nodes) {
  return nodes.flatMap(
    (node, childIndex) => expandNodeForFieldProcessing(node).map((logicalNode) => ({
      node: logicalNode,
      rawNode: node,
      childIndex,
      splitFromRun: logicalNode !== node
    }))
  );
}
function expandNodeForFieldProcessing(node) {
  const elements = node.name === "w:r" ? node.children ?? [] : [];
  if (!elements.length) return [node];
  const runProperties = elements.filter((child2) => child2.name === "w:rPr");
  const contentElements = elements.filter((child2) => child2.name !== "w:rPr");
  const logicalNodes = [];
  let currentKind = null;
  let currentElements = [];
  const flush = () => {
    if (!currentElements.length) return;
    logicalNodes.push({ ...cloneWordPortXmlNode(node), children: [...runProperties.map(cloneWordPortXmlNode), ...currentElements.map(cloneWordPortXmlNode)] });
    currentElements = [];
    currentKind = null;
  };
  for (const element of contentElements) {
    if (element.name === "w:fldChar") {
      flush();
      logicalNodes.push({ ...cloneWordPortXmlNode(node), children: [...runProperties.map(cloneWordPortXmlNode), cloneWordPortXmlNode(element)] });
      continue;
    }
    if (element.name === "w:instrText" || element.name === "w:tab") {
      if (currentKind !== "instruction") {
        flush();
        currentKind = "instruction";
      }
      currentElements.push(element);
      continue;
    }
    if (currentKind !== "content") {
      flush();
      currentKind = "content";
    }
    currentElements.push(element);
  }
  flush();
  return logicalNodes.length > 1 ? logicalNodes : [node];
}
function uniqueRawNodes(nodes) {
  const rawNodes = [];
  let previousChildIndex = -1;
  for (const node of nodes) {
    if (node.childIndex === previousChildIndex) continue;
    rawNodes.push(node.rawNode);
    previousChildIndex = node.childIndex;
  }
  return rawNodes;
}
function fieldCharType(node) {
  if (!node) return void 0;
  const fieldChar = node.name === "w:fldChar" ? node : firstChild(node, "w:fldChar");
  return fieldChar?.attributes?.["w:fldCharType"] ?? fieldChar?.attributes?.fldCharType;
}
function instructionTokensFromComplexField(nodes) {
  const tokens = [];
  for (const node of nodes) {
    if (fieldCharType(node) === "separate") break;
    for (const child2 of node.children ?? []) {
      if (child2.name === "w:instrText") tokens.push({ type: "text", text: textContent(child2) });
      if (child2.name === "w:tab") tokens.push({ type: "tab" });
    }
  }
  return tokens;
}
function instructionTextFromTokens(tokens) {
  return normalizeInstructionText(tokens.map((token) => token.type === "tab" ? "	" : token.text).join(" "));
}
function resultTextFromComplexField(nodes) {
  const resultNodes = [];
  let afterSeparate = false;
  let depth = 0;
  for (const node of nodes) {
    const type = fieldCharType(node);
    if (type === "begin") depth += 1;
    if (type === "separate" && depth === 1) {
      afterSeparate = true;
      continue;
    }
    if (type === "end") {
      if (depth === 1) break;
      depth -= 1;
    }
    if (afterSeparate) resultNodes.push(node);
  }
  return visibleTextFromNodes(resultNodes);
}
function visibleTextFromNodes(nodes) {
  return nodes.map(visibleTextFromNode).join("");
}
function visibleTextFromNode(node) {
  if (node.name === "w:t" || node.name === "w:delText") return textContent(node);
  if (node.name === "w:tab") return "	";
  return (node.children ?? []).map(visibleTextFromNode).join("");
}
function createFieldReference(input) {
  const instruction = normalizeInstructionText(input.instruction);
  const instructionType = fieldInstructionType(instruction);
  const resultText = input.resultText?.trim() ? input.resultText : void 0;
  const supported = instructionType !== "UNKNOWN";
  const instructionSwitches = parseFieldInstructionSwitches(instruction);
  return {
    kind: input.kind,
    instruction,
    instructionType,
    ...input.instructionTokens?.length ? { instructionTokens: input.instructionTokens } : {},
    ...instructionSwitches.length ? { instructionSwitches } : {},
    ...resultText ? { resultText } : {},
    displayText: fieldDisplayText(instructionType, instruction, resultText),
    supported,
    complete: input.complete ?? true,
    structure: fieldStructure(instructionType, instruction, resultText),
    ...input.rawNodes ? { rawNodes: input.rawNodes } : {},
    ...input.rawNode ? { rawNode: input.rawNode } : {}
  };
}
function normalizeInstructionText(value) {
  return value.replace(/\s+/g, " ").trim();
}
function parseFieldInstructionSwitches(instruction) {
  const tokens = tokenizeFieldInstruction(instruction);
  const switches = [];
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (!token?.startsWith("\\")) continue;
    const next = tokens[index + 1];
    const hasValue = Boolean(next && !next.startsWith("\\") && switchUsuallyCarriesValue(token));
    const raw = hasValue ? `${token} ${next}` : token;
    switches.push({
      name: token,
      ...hasValue ? { value: unquoteFieldToken(next) } : {},
      raw
    });
    if (hasValue) index += 1;
  }
  return switches;
}
function tokenizeFieldInstruction(instruction) {
  return instruction.match(/"[^"]*"|\\\*|\\[A-Za-z]+|[^\s]+/g) ?? [];
}
function switchUsuallyCarriesValue(token) {
  const normalized = token.toLowerCase();
  return (/* @__PURE__ */ new Set(["\\*", "\\a", "\\b", "\\c", "\\d", "\\f", "\\l", "\\n", "\\o", "\\p", "\\r", "\\s", "\\t"])).has(normalized);
}
function unquoteFieldToken(token) {
  return token.startsWith('"') && token.endsWith('"') ? token.slice(1, -1) : token;
}
function fieldInstructionType(instruction) {
  const token = instruction.match(/^=?\s*([A-Za-z]+)/)?.[1]?.toUpperCase();
  if (token === "PAGE" || token === "NUMPAGES" || token === "HYPERLINK" || token === "REF" || token === "PAGEREF" || token === "STYLEREF" || token === "SEQ" || token === "TOC" || token === "TOA" || token === "XE" || token === "TC" || token === "NOTEREF") {
    return token;
  }
  return "UNKNOWN";
}
function fieldDisplayText(type, instruction, resultText) {
  if ((type === "PAGE" || type === "NUMPAGES") && resultText) return resultText;
  if (type === "PAGE" || type === "NUMPAGES") return "1";
  if (resultText) return resultText;
  if (type === "TOC") return "Table of contents";
  if (type === "TOA") return "Table of authorities";
  if (type === "XE") return "Index entry";
  if (type === "TC") return "Table of contents entry";
  if (type === "SEQ") return "1";
  if (type === "HYPERLINK") return fieldTargetToken(instruction) || resultText || "Hyperlink";
  if (type === "REF" || type === "PAGEREF" || type === "STYLEREF" || type === "NOTEREF") return `[${type} ${fieldTargetToken(instruction)}]`;
  return instruction ? `[FIELD ${instruction}]` : "[FIELD]";
}
function fieldTargetToken(instruction) {
  const quoted = instruction.match(/^[A-Za-z]+\s+"([^"]+)"/)?.[1];
  if (quoted) return quoted;
  return instruction.split(/\s+/).slice(1).find((token) => token && !token.startsWith("\\"))?.replace(/^"|"$/g, "") ?? "";
}
function fieldStructure(type, instruction, resultText) {
  switch (type) {
    case "HYPERLINK": {
      const hyperlink = parseHyperlinkInstruction(instruction);
      return {
        nodeName: "w:hyperlink",
        fieldType: "HYPERLINK",
        ...hyperlink.target ? { target: hyperlink.target } : {},
        ...hyperlink.anchor ? { anchor: hyperlink.anchor } : {},
        ...hyperlink.tooltip ? { tooltip: hyperlink.tooltip } : {},
        ...hyperlink.targetFrame ? { targetFrame: hyperlink.targetFrame } : {}
      };
    }
    case "PAGE":
      return { nodeName: "sd:autoPageNumber", fieldType: "PAGE" };
    case "NUMPAGES":
      return { nodeName: "sd:totalPageNumber", fieldType: "NUMPAGES", ...resultText ? { importedCachedText: resultText } : {} };
    case "REF":
    case "STYLEREF":
    case "NOTEREF":
      return { nodeName: "sd:crossReference", fieldType: type, target: fieldTargetToken(instruction) };
    case "PAGEREF":
      return { nodeName: "sd:pageReference", fieldType: "PAGEREF", target: fieldTargetToken(instruction) };
    case "SEQ":
      return { nodeName: "sd:sequenceField", fieldType: "SEQ", identifier: fieldTargetToken(instruction) };
    case "TOC":
      return { nodeName: "sd:tableOfContents", fieldType: "TOC" };
    case "TOA":
      return { nodeName: "sd:tableOfAuthorities", fieldType: "TOA" };
    case "XE":
      return { nodeName: "sd:indexEntry", fieldType: "XE" };
    case "TC":
      return { nodeName: "sd:tableOfContentsEntry", fieldType: "TC" };
    case "UNKNOWN":
      return { nodeName: "sd:unsupportedField", fieldType: "UNKNOWN", reason: "No WordPort field preprocessor is registered for this instruction." };
  }
}
function parseHyperlinkInstruction(instruction) {
  const target = instruction.match(/HYPERLINK\s+"([^"]+)"/i)?.[1];
  const anchor = instruction.match(/(?:\\)?l\s+"([^"]+)"/i)?.[1];
  const tooltip = instruction.match(/(?:\\)?o\s+"([^"]+)"/i)?.[1];
  const explicitFrame = instruction.match(/(?:\\t|\t)\s+"([^"]+)"/i)?.[1];
  const newWindow = /(?:\\n|\n)/i.test(instruction);
  return {
    ...target ? { target } : {},
    ...anchor ? { anchor } : {},
    ...tooltip ? { tooltip } : {},
    ...explicitFrame ? { targetFrame: explicitFrame } : newWindow ? { targetFrame: "_blank" } : {}
  };
}
function importBookmarkMarker(node, story) {
  if (node.name !== "w:bookmarkStart" && node.name !== "w:bookmarkEnd") return void 0;
  const attrs = node.attributes ?? {};
  const marker = {
    role: node.name === "w:bookmarkStart" ? "start" : "end",
    id: attrs["w:id"] ?? attrs.id ?? "",
    ...attrs["w:name"] ?? attrs.name ? { name: attrs["w:name"] ?? attrs.name } : {},
    ...attrs["w:colFirst"] ?? attrs.colFirst ? { colFirst: attrs["w:colFirst"] ?? attrs.colFirst } : {},
    ...attrs["w:colLast"] ?? attrs.colLast ? { colLast: attrs["w:colLast"] ?? attrs.colLast } : {},
    ...attrs["w:displacedByCustomXml"] ?? attrs.displacedByCustomXml ? { displacedByCustomXml: attrs["w:displacedByCustomXml"] ?? attrs.displacedByCustomXml } : {},
    ...story ? { story } : {},
    attributes: { ...attrs },
    raw: cloneWordPortXmlNode(node)
  };
  return marker;
}
function importHyperlink(node, context) {
  const content = [];
  const raw = [];
  for (const child2 of node.children ?? []) {
    const imported = registry.importNode(child2, context);
    if (imported && isInline(imported)) content.push(imported);
    else raw.push(cloneWordPortXmlNode(child2));
  }
  const relationshipId = node.attributes?.["r:id"];
  const relationship = relationshipId ? context.relationships?.relationships.find((item) => item.id === relationshipId) : void 0;
  return {
    type: "hyperlink",
    content,
    relationshipId,
    anchor: node.attributes?.["w:anchor"],
    target: relationship?.target,
    raw: raw.length ? { ...cloneWordPortXmlNode(node), children: raw } : cloneWordPortXmlNode(node)
  };
}
function importInlineWrapper(node, context) {
  const content = [];
  for (const child2 of node.children ?? []) {
    const imported = registry.importNode(child2, context);
    if (imported && isInline(imported)) content.push(imported);
  }
  return {
    type: "inlineWrapper",
    name: node.name,
    attributes: node.attributes ? { ...node.attributes } : void 0,
    content,
    raw: cloneWordPortXmlNode(node)
  };
}
function importInlineSdt(node, context) {
  const contentNode = firstChild(node, "w:sdtContent");
  const hasBlockContent = Boolean(contentNode?.children?.some((child2) => child2.name === "w:p" || child2.name === "w:tbl"));
  if (hasBlockContent) return importBlockWrapper(node, context, "w:sdtContent");
  const content = [];
  for (const child2 of contentNode?.children ?? []) {
    const imported = registry.importNode(child2, context);
    if (imported && isInline(imported)) content.push(imported);
  }
  return {
    type: "inlineWrapper",
    name: node.name,
    attributes: node.attributes ? { ...node.attributes } : void 0,
    content,
    raw: cloneWordPortXmlNode(node)
  };
}
function importBlockWrapper(node, context, contentName) {
  const content = firstChild(node, contentName);
  const blocks = [];
  for (const child2 of content?.children ?? []) {
    const imported = registry.importNode(child2, context);
    if (imported && isBlock(imported)) blocks.push(imported);
  }
  return {
    type: "blockWrapper",
    name: node.name,
    contentName,
    attributes: node.attributes ? { ...node.attributes } : void 0,
    blocks,
    raw: cloneWordPortXmlNode(node)
  };
}
function importAlternateContent(node, context) {
  const branches = [
    ...(node.children ?? []).filter((child2) => localName4(child2.name) === "Choice"),
    ...(node.children ?? []).filter((child2) => localName4(child2.name) === "Fallback")
  ];
  for (const branch of branches) {
    for (const child2 of branch.children ?? []) {
      const imported = registry.importNode(child2, context);
      if (imported) return withAlternateContentRaw(imported, node);
    }
  }
  context.diagnostics.warning("ooxml.unsupported-node", "AlternateContent did not contain a readable OOXML branch.", {
    path: context.partPath,
    source: node.name
  });
  return void 0;
}
function withAlternateContentRaw(imported, alternateContent) {
  if (!("raw" in imported)) return imported;
  return {
    ...imported,
    raw: cloneWordPortXmlNode(alternateContent)
  };
}
function importDrawing(node, context) {
  const drawingNode = firstChild(node, "wp:inline") ?? firstChild(node, "wp:anchor");
  const drawingKind = drawingNode?.name === "wp:anchor" ? "anchor" : drawingNode?.name === "wp:inline" ? "inline" : "unknown";
  const anchor = drawingKind === "anchor" ? readAnchorMetadata(drawingNode) : void 0;
  const embedIds = findDescendants(node, (child2) => child2.name === "a:blip").map((child2) => child2.attributes?.["r:embed"] ?? child2.attributes?.["r:link"]).filter(Boolean);
  const extent = firstChild(drawingNode, "wp:extent");
  const size = extent?.attributes ? {
    width: emuToPixels(extent.attributes.cx),
    height: emuToPixels(extent.attributes.cy),
    cx: readFiniteNumber(extent.attributes.cx),
    cy: readFiniteNumber(extent.attributes.cy)
  } : void 0;
  const docPr = firstChild(drawingNode, "wp:docPr");
  const graphicData = findDescendants(node, (child2) => child2.name === "a:graphicData")[0];
  const transform = readPictureTransform(node);
  const crop = readPictureCrop(node);
  const hyperlink = readPictureHyperlink(node, context);
  const textboxBlocks = importTextboxBlocks(node, context);
  if (drawingKind === "anchor") {
    context.diagnostics.warning("ooxml.export-partial", "Floating DrawingML anchor was imported with best-effort display positioning.", {
      path: context.partPath,
      source: "wp:anchor",
      detail: {
        embeds: embedIds,
        positionH: anchor?.positionH,
        positionV: anchor?.positionV,
        wrap: anchor?.wrap?.type,
        behindDoc: anchor?.behindDoc,
        layoutInCell: anchor?.layoutInCell
      }
    });
  } else if (drawingKind === "unknown") {
    context.diagnostics.warning("ooxml.unsupported-node", "DrawingML without wp:inline/wp:anchor was preserved but cannot be positioned yet.", {
      path: context.partPath,
      source: "w:drawing"
    });
  }
  if (embedIds.length === 0 && textboxBlocks.length === 0) {
    context.diagnostics.warning("ooxml.unsupported-node", "DrawingML without an a:blip relationship was preserved but cannot render as an image.", {
      path: context.partPath,
      source: "w:drawing",
      detail: { graphicDataUri: graphicData?.attributes?.uri }
    });
  } else if (embedIds.length > 1) {
    context.diagnostics.warning("ooxml.export-partial", "DrawingML with multiple image relationships renders the first image only.", {
      path: context.partPath,
      source: "w:drawing",
      detail: { embeds: embedIds }
    });
  }
  const drawing = {
    type: "drawing",
    embeds: embedIds.map((id) => resolveMediaReference(context, id)),
    raw: cloneWordPortXmlNode(node),
    ...textboxBlocks.length ? { textboxBlocks } : {},
    drawingKind,
    sourcePartPath: context.partPath,
    ...docPr?.attributes ? { docPr: { ...docPr.attributes } } : {},
    ...docPr?.attributes?.descr ? { altText: docPr.attributes.descr, title: docPr.attributes.descr } : {},
    ...transform ? { transform } : {},
    ...crop ? { crop } : {},
    ...hyperlink ? { hyperlink } : {},
    ...graphicData?.attributes?.uri ? { graphicDataUri: graphicData.attributes.uri } : {},
    ...size ? { size } : {},
    ...anchor ? { anchor } : {}
  };
  return drawing;
}
function importPictDrawing(node, context) {
  const textboxBlocks = importTextboxBlocks(node, context);
  const shape = findDescendants(node, (child2) => child2.name === "v:shape")[0];
  const textbox = findDescendants(node, (child2) => child2.name === "v:textbox")[0];
  const shapeMetadata = readVmlShapeMetadata(shape, textbox);
  if (!textboxBlocks.length) {
    context.diagnostics.warning("ooxml.unsupported-node", "VML picture without readable textbox content was preserved but cannot render.", {
      path: context.partPath,
      source: "w:pict"
    });
  }
  return {
    type: "drawing",
    embeds: [],
    raw: cloneWordPortXmlNode(node),
    drawingKind: shapeMetadata?.anchor ? "anchor" : "inline",
    sourcePartPath: context.partPath,
    ...shapeMetadata?.size ? { size: shapeMetadata.size } : {},
    ...shapeMetadata?.anchor ? { anchor: shapeMetadata.anchor } : {},
    ...shapeMetadata ? { shape: shapeMetadata } : {},
    ...textboxBlocks.length ? { textboxBlocks } : {}
  };
}
function importTextboxBlocks(node, context) {
  const blocks = [];
  const textboxContentNodes = findDescendants(node, (child2) => child2.name === "w:txbxContent");
  for (const contentNode of textboxContentNodes) {
    for (const child2 of contentNode.children ?? []) {
      const imported = registry.importNode(child2, context);
      if (imported && isBlock(imported)) blocks.push(imported);
    }
  }
  return blocks;
}
function readAnchorMetadata(node) {
  if (!node || node.name !== "wp:anchor") return void 0;
  const positionH = readAnchorPosition(firstChild(node, "wp:positionH"));
  const positionV = readAnchorPosition(firstChild(node, "wp:positionV"));
  const extent = readDrawingExtent(firstChild(node, "wp:extent"));
  const effectExtent = readEffectExtent(firstChild(node, "wp:effectExtent"));
  const simplePosNode = firstChild(node, "wp:simplePos");
  const simplePosEnabled = readOoxmlBoolean(node.attributes?.simplePos);
  const simplePos = simplePosNode ? {
    enabled: simplePosEnabled,
    x: readFiniteNumber(simplePosNode.attributes?.x),
    y: readFiniteNumber(simplePosNode.attributes?.y),
    xPx: emuToCssPixels(simplePosNode.attributes?.x),
    yPx: emuToCssPixels(simplePosNode.attributes?.y)
  } : void 0;
  const wrap = readAnchorWrap(node);
  const behindDoc = readOoxmlBoolean(node.attributes?.behindDoc);
  const relativeHeight = readFiniteNumber(node.attributes?.relativeHeight);
  return {
    ...extent ? { extent } : {},
    ...effectExtent ? { effectExtent } : {},
    ...positionH ? { positionH } : {},
    ...positionV ? { positionV } : {},
    ...positionH?.offset != null || positionV?.offset != null ? { marginOffset: { horizontal: positionH?.offset, top: positionV?.offset } } : {},
    ...wrap ? { wrap } : {},
    behindDoc,
    layoutInCell: readOoxmlBoolean(node.attributes?.layoutInCell),
    allowOverlap: readOoxmlBoolean(node.attributes?.allowOverlap),
    ...simplePos ? { simplePos } : {},
    distance: {
      top: emuToCssPixels(node.attributes?.distT),
      right: emuToCssPixels(node.attributes?.distR),
      bottom: emuToCssPixels(node.attributes?.distB),
      left: emuToCssPixels(node.attributes?.distL)
    },
    ...relativeHeight != null ? { relativeHeight, zIndex: ooxmlRelativeHeightToZIndex(relativeHeight) } : {},
    originalAttributes: node.attributes ? { ...node.attributes } : void 0
  };
}
function readDrawingExtent(node) {
  if (!node?.attributes) return void 0;
  const width = emuToPixels(node.attributes.cx);
  const height = emuToPixels(node.attributes.cy);
  const cx = readFiniteNumber(node.attributes.cx);
  const cy = readFiniteNumber(node.attributes.cy);
  return width != null || height != null || cx != null || cy != null ? { width, height, cx, cy } : void 0;
}
function readEffectExtent(node) {
  if (!node?.attributes) return void 0;
  const effectExtent = {
    left: emuToCssPixels(node.attributes.l),
    top: emuToCssPixels(node.attributes.t),
    right: emuToCssPixels(node.attributes.r),
    bottom: emuToCssPixels(node.attributes.b)
  };
  return Object.values(effectExtent).some((value) => value != null) ? effectExtent : void 0;
}
function readAnchorPosition(node) {
  if (!node) return void 0;
  const posOffset = firstChild(node, "wp:posOffset");
  const offsetEmu = readFiniteNumber(textContent(posOffset));
  const align = textContent(firstChild(node, "wp:align"));
  return {
    relativeFrom: node.attributes?.relativeFrom,
    ...offsetEmu != null ? { offsetEmu, offset: emuToCssPixels(offsetEmu) } : {},
    ...align ? { align } : {}
  };
}
function readAnchorWrap(node) {
  const wrapNode = (node.children ?? []).find(
    (child2) => child2.name === "wp:wrapNone" || child2.name === "wp:wrapSquare" || child2.name === "wp:wrapThrough" || child2.name === "wp:wrapTight" || child2.name === "wp:wrapTopAndBottom"
  );
  if (!wrapNode) return void 0;
  const type = wrapNode.name.replace("wp:wrap", "") || "None";
  const attrs = {};
  if (wrapNode.attributes?.wrapText) attrs.wrapText = wrapNode.attributes.wrapText;
  if (wrapNode.attributes?.distT != null) attrs.distTop = emuToCssPixels(wrapNode.attributes.distT);
  if (wrapNode.attributes?.distR != null) attrs.distRight = emuToCssPixels(wrapNode.attributes.distR);
  if (wrapNode.attributes?.distB != null) attrs.distBottom = emuToCssPixels(wrapNode.attributes.distB);
  if (wrapNode.attributes?.distL != null) attrs.distLeft = emuToCssPixels(wrapNode.attributes.distL);
  if (type === "None") attrs.behindDoc = readOoxmlBoolean(node.attributes?.behindDoc);
  const polygon = readWrapPolygon(firstChild(wrapNode, "wp:wrapPolygon"));
  return { type, ...Object.keys(attrs).length ? { attrs } : {}, ...polygon ? { polygon } : {} };
}
function readPictureTransform(node) {
  const transformNode = findDescendants(node, (child2) => child2.name === "a:xfrm")[0];
  if (!transformNode?.attributes) return void 0;
  const rotationRaw = readFiniteNumber(transformNode.attributes.rot);
  const transform = {};
  if (rotationRaw != null) transform.rotation = rotationRaw / 6e4;
  if (transformNode.attributes.flipH != null) transform.horizontalFlip = readOoxmlBoolean(transformNode.attributes.flipH);
  if (transformNode.attributes.flipV != null) transform.verticalFlip = readOoxmlBoolean(transformNode.attributes.flipV);
  return Object.keys(transform).length ? transform : void 0;
}
function readPictureCrop(node) {
  const cropNode = findDescendants(node, (child2) => child2.name === "a:srcRect")[0];
  if (!cropNode?.attributes) return void 0;
  const crop = {};
  const left = ooxmlCropToPercent(cropNode.attributes.l);
  const top = ooxmlCropToPercent(cropNode.attributes.t);
  const right = ooxmlCropToPercent(cropNode.attributes.r);
  const bottom = ooxmlCropToPercent(cropNode.attributes.b);
  if (left != null) crop.left = left;
  if (top != null) crop.top = top;
  if (right != null) crop.right = right;
  if (bottom != null) crop.bottom = bottom;
  return Object.keys(crop).length ? crop : void 0;
}
function readPictureHyperlink(node, context) {
  const hyperlinkNode = findDescendants(node, (child2) => child2.name === "a:hlinkClick")[0];
  if (!hyperlinkNode?.attributes) return void 0;
  const relationshipId = hyperlinkNode.attributes["r:id"];
  const relationship = relationshipId ? context.relationships?.relationships.find((item) => item.id === relationshipId) : void 0;
  const url = relationship?.target ?? hyperlinkNode.attributes.href;
  if (!url && !relationshipId) return void 0;
  return {
    url: url ?? relationshipId ?? "",
    ...hyperlinkNode.attributes.tooltip ? { tooltip: hyperlinkNode.attributes.tooltip } : {},
    ...relationshipId ? { relationshipId } : {}
  };
}
function ooxmlCropToPercent(value) {
  const raw = readFiniteNumber(value);
  if (raw == null) return void 0;
  return raw / 1e3;
}
function readWrapPolygon(node) {
  if (!node) return void 0;
  const startNode = firstChild(node, "wp:start");
  const lineNodes = (node.children ?? []).filter((child2) => child2.name === "wp:lineTo");
  const point = (child2) => child2 ? {
    x: readFiniteNumber(child2.attributes?.x),
    y: readFiniteNumber(child2.attributes?.y),
    xPx: emuToCssPixels(child2.attributes?.x),
    yPx: emuToCssPixels(child2.attributes?.y)
  } : void 0;
  const start = point(startNode);
  const points = lineNodes.map((child2) => point(child2)).filter(Boolean);
  if (!start && points.length === 0) return void 0;
  return {
    ...node.attributes?.edited != null ? { edited: readOoxmlBoolean(node.attributes.edited) } : {},
    ...start ? { start } : {},
    points
  };
}
function readVmlShapeMetadata(shape, textbox) {
  if (!shape) return void 0;
  const style = parseVmlStyle(shape.attributes?.style);
  const size = readVmlSize(style);
  const anchor = readVmlAnchorMetadata(style, shape);
  return {
    kind: "vml",
    ...Object.keys(style).length ? { style } : {},
    ...size ? { size } : {},
    ...anchor ? { anchor } : {},
    ...textbox?.attributes ? { textboxAttributes: { ...textbox.attributes } } : {},
    ...shape.attributes ? { attributes: { ...shape.attributes } } : {}
  };
}
function readVmlSize(style) {
  const width = cssLengthToPixels(style.width);
  const height = cssLengthToPixels(style.height);
  return width != null || height != null ? { width, height } : void 0;
}
function readVmlAnchorMetadata(style, shape) {
  const position = style.position?.toLowerCase();
  const horizontalOffset = cssLengthToPixels(style["margin-left"] ?? style.left);
  const verticalOffset = cssLengthToPixels(style["margin-top"] ?? style.top);
  const alignH = normalizeVmlAlign(style["mso-position-horizontal"]);
  const alignV = normalizeVmlAlign(style["mso-position-vertical"]);
  const hRelativeFrom = normalizeVmlRelativeFrom(style["mso-position-horizontal-relative"]);
  const vRelativeFrom = normalizeVmlRelativeFrom(style["mso-position-vertical-relative"]);
  const zIndex = readFiniteNumber(style["z-index"]);
  const isAnchored = position === "absolute" || hRelativeFrom != null || vRelativeFrom != null || horizontalOffset != null || verticalOffset != null;
  if (!isAnchored) return void 0;
  const positionH = readVmlAnchorPosition(hRelativeFrom, alignH, horizontalOffset);
  const positionV = readVmlAnchorPosition(vRelativeFrom, alignV, verticalOffset);
  const wrap = readVmlWrap(shape, zIndex);
  return {
    ...positionH ? { positionH } : {},
    ...positionV ? { positionV } : {},
    ...horizontalOffset != null || verticalOffset != null ? { marginOffset: { horizontal: horizontalOffset, top: verticalOffset } } : {},
    ...wrap ? { wrap } : {},
    behindDoc: zIndex != null && zIndex < 0,
    ...zIndex != null ? { zIndex } : {},
    originalAttributes: shape.attributes ? { ...shape.attributes } : void 0
  };
}
function readVmlAnchorPosition(relativeFrom, align, offset) {
  if (relativeFrom == null && align == null && offset == null) return void 0;
  return {
    ...relativeFrom ? { relativeFrom } : {},
    ...offset != null ? { offset } : {},
    ...align ? { align } : {}
  };
}
function readVmlWrap(shape, zIndex) {
  const wrapNode = findDescendants(shape, (child2) => child2.name === "w10:wrap")[0];
  const rawType = wrapNode?.attributes?.type?.toLowerCase();
  if (zIndex != null && zIndex < 0) return { type: "None", attrs: { behindDoc: true } };
  if (rawType === "square") return { type: "Square", attrs: { wrapText: wrapNode?.attributes?.side } };
  if (rawType === "tight") return { type: "Tight" };
  if (rawType === "through") return { type: "Through" };
  if (rawType === "topandbottom" || rawType === "topAndBottom".toLowerCase()) return { type: "TopAndBottom" };
  return { type: "None" };
}
function parseVmlStyle(style) {
  const result = {};
  for (const part of style?.split(";") ?? []) {
    const index = part.indexOf(":");
    if (index <= 0) continue;
    const key = part.slice(0, index).trim().toLowerCase();
    const value = part.slice(index + 1).trim();
    if (key && value) result[key] = value;
  }
  return result;
}
function cssLengthToPixels(value) {
  if (!value) return void 0;
  const match = value.trim().match(/^(-?\d+(?:\.\d+)?)(px|pt|in|cm|mm)?$/i);
  if (!match) return void 0;
  const amount = Number(match[1]);
  if (!Number.isFinite(amount)) return void 0;
  const unit = match[2]?.toLowerCase() ?? "px";
  if (unit === "pt") return amount * 4 / 3;
  if (unit === "in") return amount * 96;
  if (unit === "cm") return amount * 96 / 2.54;
  if (unit === "mm") return amount * 96 / 25.4;
  return amount;
}
function normalizeVmlAlign(value) {
  if (!value) return void 0;
  const normalized = value.trim().toLowerCase();
  if (normalized === "middle") return "center";
  if (["left", "center", "right", "top", "bottom", "inside", "outside"].includes(normalized)) return normalized;
  return void 0;
}
function normalizeVmlRelativeFrom(value) {
  if (!value) return void 0;
  const normalized = value.trim().toLowerCase();
  if (["page", "margin", "column", "paragraph", "line"].includes(normalized)) return normalized;
  return void 0;
}
function emuToPixels(value) {
  const emu = readFiniteNumber(value);
  if (emu == null) return void 0;
  return Math.max(1, Math.round(emu / 9525));
}
function emuToCssPixels(value) {
  const emu = readFiniteNumber(value);
  if (emu == null) return void 0;
  return Math.round(emu / 9525);
}
function ooxmlRelativeHeightToZIndex(relativeHeight) {
  return Math.max(0, relativeHeight - 251658240);
}
function readFiniteNumber(value) {
  if (value == null || value === "") return void 0;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : void 0;
}
function readOoxmlBoolean(value) {
  return value === "1" || value === "true" || value === "on";
}
function readOptionalOoxmlBoolean(value) {
  if (value == null) return void 0;
  return value !== "0" && value !== "false" && value !== "off";
}
function readHeaderFooterVariant(value) {
  if (value === "first" || value === "even") return value;
  return "default";
}
function importTable(node, context) {
  const rows = [];
  const raw = [];
  const properties = cloneWordPortXmlNode(firstChild(node, "w:tblPr"));
  const grid = cloneWordPortXmlNode(firstChild(node, "w:tblGrid"));
  const source = createBlockSourceAnchor(node, context, "table");
  let rowIndex = 0;
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:tblPr" || child2.name === "w:tblGrid") continue;
    if (child2.name === "w:tr") {
      rows.push(importTableRow(child2, context, `${source.occurrencePath}/w:tr[${rowIndex}]`));
      rowIndex += 1;
    } else raw.push(cloneWordPortXmlNode(child2));
  }
  normalizeImportedVerticalMergeRowSpans(rows);
  const gridColumns = readTableGridColumns(grid);
  const directTableProperties = readTableProperties(properties);
  const tableProperties = mergeTablePropertyPartials(
    resolveCascadedTableStyleProperties(directTableProperties.styleId, context.styles),
    directTableProperties
  );
  applyTableStyleCellProperties(rows, tableProperties, context.styles);
  const fallbackGridColumns = gridColumns.length ? gridColumns : buildFallbackTableGridColumns(rows, tableProperties.width);
  return {
    type: "table",
    rows,
    source,
    ...properties ? { properties } : {},
    ...grid ? { grid } : {},
    ...fallbackGridColumns.length ? { gridColumns: fallbackGridColumns } : {},
    ...tableProperties,
    ...raw.length ? { raw } : {}
  };
}
function importTableRow(node, context, occurrencePath) {
  const properties = cloneWordPortXmlNode(firstChild(node, "w:trPr"));
  const propertyExceptions = cloneWordPortXmlNode(firstChild(node, "w:tblPrEx"));
  const rowProperties = readTableRowProperties(properties);
  const source = createBlockSourceAnchor(node, context, "tableRow", occurrencePath);
  const cells = [];
  const raw = [];
  let cellIndex = 0;
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:trPr" || child2.name === "w:tblPrEx") continue;
    if (child2.name === "w:tc") {
      cells.push(importTableCell(child2, context, `${source.occurrencePath}/w:tc[${cellIndex}]`));
      cellIndex += 1;
    } else raw.push(cloneWordPortXmlNode(child2));
  }
  normalizeImportedHorizontalMerges(cells);
  return { type: "tableRow", cells, source, ...properties ? { properties } : {}, ...propertyExceptions ? { propertyExceptions } : {}, ...rowProperties, ...raw.length ? { raw } : {} };
}
function importTableCell(node, context, occurrencePath) {
  const properties = cloneWordPortXmlNode(firstChild(node, "w:tcPr"));
  const cellProperties = readTableCellProperties(properties);
  const source = createBlockSourceAnchor(node, context, "tableCell", occurrencePath);
  const blocks = [];
  const raw = [];
  for (const child2 of node.children ?? []) {
    if (child2.name === "w:tcPr") continue;
    const imported = registry.importNode(child2, context);
    if (imported && isBlock(imported)) blocks.push(imported);
    else raw.push(cloneWordPortXmlNode(child2));
  }
  return { type: "tableCell", blocks, source, ...properties ? { properties } : {}, ...cellProperties, ...raw.length ? { raw } : {} };
}
function readTableGridColumns(grid) {
  return (grid?.children ?? []).filter((child2) => child2.name === "w:gridCol").map((child2) => {
    const widthTwips = readPositiveInteger(tableXmlAttr(child2, "w:w"));
    return widthTwips ? { widthTwips } : {};
  });
}
function buildFallbackTableGridColumns(rows, tableWidth) {
  const columnCount = rows.reduce((max, row) => Math.max(max, countTableRowColumns(row)), 0);
  if (columnCount <= 0) return [];
  const seeded = Array.from({ length: columnCount });
  for (const row of rows) {
    seedSkippedColumnWidths(seeded, 0, row.gridBefore ?? 0, row.wBefore);
    seedSkippedColumnWidths(seeded, columnCount - (row.gridAfter ?? 0), row.gridAfter ?? 0, row.wAfter);
  }
  const totalWidth = tableWidth?.type === "dxa" && tableWidth.value > 0 ? tableWidth.value : 9360;
  const seededTotal = seeded.reduce((sum, width) => sum + (width ?? 0), 0);
  const missing = seeded.filter((width) => width == null).length;
  const fallback = Math.max(150, Math.round((Math.max(totalWidth, seededTotal + missing * 150) - seededTotal) / Math.max(1, missing)));
  return seeded.map((widthTwips) => ({ widthTwips: Math.max(150, Math.round(widthTwips ?? fallback)) }));
}
function seedSkippedColumnWidths(target, start, span, measurement) {
  if (span <= 0 || !measurement || measurement.type !== "dxa" || measurement.value <= 0) return;
  const base = Math.floor(measurement.value / span);
  const remainder = measurement.value - base * span;
  for (let offset = 0; offset < span; offset += 1) {
    const index = start + offset;
    if (index < 0 || index >= target.length) continue;
    target[index] = Math.max(target[index] ?? 0, base + (offset < remainder ? 1 : 0));
  }
}
function countTableRowColumns(row) {
  return (row.gridBefore ?? 0) + row.cells.reduce((sum, cell) => sum + Math.max(1, cell.gridSpan ?? 1), 0) + (row.gridAfter ?? 0);
}
function normalizeImportedVerticalMergeRowSpans(rows) {
  const rowSlots = rows.map((row) => {
    let column = row.gridBefore ?? 0;
    return row.cells.map((cell) => {
      const slot = { cell, startColumn: column, span: Math.max(1, cell.gridSpan ?? 1) };
      column += slot.span;
      return slot;
    });
  });
  rowSlots.forEach((slots, rowIndex) => {
    for (const slot of slots) {
      if (slot.cell.vMerge !== "restart") continue;
      let rowSpan = 1;
      for (let nextRowIndex = rowIndex + 1; nextRowIndex < rowSlots.length; nextRowIndex += 1) {
        const continuation = rowSlots[nextRowIndex]?.find(
          (candidate) => candidate.startColumn === slot.startColumn && candidate.span === slot.span && candidate.cell.vMerge === "continue"
        );
        if (!continuation) break;
        rowSpan += 1;
      }
      if (rowSpan > 1) slot.cell.rowSpan = rowSpan;
    }
  });
}
function normalizeImportedHorizontalMerges(cells) {
  for (let index = 0; index < cells.length; index += 1) {
    const cell = cells[index];
    if (cell?.hMerge !== "restart") continue;
    let span = Math.max(1, cell.gridSpan ?? 1);
    for (let nextIndex = index + 1; nextIndex < cells.length; nextIndex += 1) {
      const next = cells[nextIndex];
      if (next?.hMerge !== "continue") break;
      span += Math.max(1, next.gridSpan ?? 1);
    }
    if (span > 1 && !cell.gridSpan) {
      cell.gridSpan = span;
      cell.colSpan = span;
    }
  }
}
function readTableProperties(properties) {
  if (!properties) return {};
  const width = readTableWidth(properties);
  const indent = readTableMeasurement(firstChild(properties, "w:tblInd"));
  const layout = readTableLayout(properties);
  const justification = tableXmlVal(firstChild(properties, "w:jc"));
  const styleId = tableXmlVal(firstChild(properties, "w:tblStyle"));
  const look = readTableLook(firstChild(properties, "w:tblLook"));
  const borders = readTableBorders(firstChild(properties, "w:tblBorders"));
  const shading = readTableShading(firstChild(properties, "w:shd"));
  const cellMargins = readTableCellMargins(firstChild(properties, "w:tblCellMar"));
  const cellSpacing = readTableMeasurement(firstChild(properties, "w:tblCellSpacing"));
  const caption = tableXmlVal(firstChild(properties, "w:tblCaption"));
  const description = tableXmlVal(firstChild(properties, "w:tblDescription"));
  const overlap = tableXmlVal(firstChild(properties, "w:tblOverlap"));
  const bidiVisual = firstChild(properties, "w:bidiVisual");
  const floatingTableProperties = readFloatingTableProperties(firstChild(properties, "w:tblpPr"));
  return {
    ...width ? { width } : {},
    ...indent ? { indent } : {},
    ...layout ? { layout } : {},
    ...justification ? { justification } : {},
    ...styleId ? { styleId } : {},
    ...caption ? { caption } : {},
    ...description ? { description } : {},
    ...overlap ? { overlap } : {},
    ...cellSpacing ? { cellSpacing } : {},
    ...bidiVisual ? { rightToLeft: readOptionalOoxmlBoolean(tableXmlVal(bidiVisual)) ?? true } : {},
    ...floatingTableProperties ? { floatingTableProperties } : {},
    ...look ? { look } : {},
    ...borders ? { borders } : {},
    ...shading ? { shading } : {},
    ...cellMargins ? { cellMargins } : {}
  };
}
function readTableWidth(properties) {
  const tblW = firstChild(properties, "w:tblW");
  return readTableMeasurement(tblW);
}
function readTableLayout(properties) {
  const value = tableXmlAttr(firstChild(properties, "w:tblLayout"), "w:type") ?? tableXmlAttr(firstChild(properties, "w:tblLayout"), "w:val");
  if (value === "fixed") return "fixed";
  if (value === "autofit") return "autofit";
  return void 0;
}
function readTableRowProperties(properties) {
  if (!properties) return {};
  const height = firstChild(properties, "w:trHeight");
  const cellSpacing = readTableMeasurement(firstChild(properties, "w:tblCellSpacing"));
  const conditionalStyle = readTableConditionalStyle(firstChild(properties, "w:cnfStyle"));
  const justification = tableXmlVal(firstChild(properties, "w:jc"));
  return {
    ...readPositiveInteger(tableXmlVal(firstChild(properties, "w:gridBefore"))) ? { gridBefore: readPositiveInteger(tableXmlVal(firstChild(properties, "w:gridBefore"))) } : {},
    ...readPositiveInteger(tableXmlVal(firstChild(properties, "w:gridAfter"))) ? { gridAfter: readPositiveInteger(tableXmlVal(firstChild(properties, "w:gridAfter"))) } : {},
    ...readTableMeasurement(firstChild(properties, "w:wBefore")) ? { wBefore: readTableMeasurement(firstChild(properties, "w:wBefore")) } : {},
    ...readTableMeasurement(firstChild(properties, "w:wAfter")) ? { wAfter: readTableMeasurement(firstChild(properties, "w:wAfter")) } : {},
    ...firstChild(properties, "w:cantSplit") ? { cantSplit: true } : {},
    ...firstChild(properties, "w:tblHeader") ? { repeatHeader: true } : {},
    ...readPositiveInteger(tableXmlAttr(height, "w:val")) ? { heightTwips: readPositiveInteger(tableXmlAttr(height, "w:val")) } : {},
    ...tableXmlAttr(height, "w:hRule") ? { heightRule: tableXmlAttr(height, "w:hRule") } : {},
    ...justification ? { justification } : {},
    ...cellSpacing ? { cellSpacing } : {},
    ...conditionalStyle ? { conditionalStyle } : {}
  };
}
function readTableCellProperties(properties) {
  if (!properties) return {};
  const gridSpan = readPositiveInteger(tableXmlVal(firstChild(properties, "w:gridSpan")));
  const vMergeNode = firstChild(properties, "w:vMerge");
  const vMergeValue = tableXmlVal(vMergeNode);
  const hMergeNode = firstChild(properties, "w:hMerge");
  const hMergeValue = tableXmlVal(hMergeNode);
  const width = readTableMeasurement(firstChild(properties, "w:tcW"));
  const borders = readTableBorders(firstChild(properties, "w:tcBorders"));
  const shading = readTableShading(firstChild(properties, "w:shd"));
  const cellMargins = readTableCellMargins(firstChild(properties, "w:tcMar"));
  const verticalAlign = tableXmlVal(firstChild(properties, "w:vAlign"));
  const noWrapNode = firstChild(properties, "w:noWrap");
  const textDirection = tableXmlVal(firstChild(properties, "w:textDirection"));
  const fitTextNode = firstChild(properties, "w:tcFitText");
  const hideMarkNode = firstChild(properties, "w:hideMark");
  const conditionalStyle = readTableConditionalStyle(firstChild(properties, "w:cnfStyle"));
  return {
    ...gridSpan ? { gridSpan } : {},
    ...gridSpan ? { colSpan: gridSpan } : {},
    ...vMergeNode ? { vMerge: vMergeValue === "restart" ? "restart" : "continue" } : {},
    ...hMergeNode ? { hMerge: hMergeValue === "restart" ? "restart" : "continue" } : {},
    ...width ? { width } : {},
    ...borders ? { borders } : {},
    ...shading ? { shading } : {},
    ...cellMargins ? { cellMargins } : {},
    ...verticalAlign ? { verticalAlign } : {},
    ...noWrapNode ? { noWrap: readOptionalOoxmlBoolean(tableXmlVal(noWrapNode)) ?? true } : {},
    ...textDirection ? { textDirection } : {},
    ...fitTextNode ? { fitText: readOptionalOoxmlBoolean(tableXmlVal(fitTextNode)) ?? true } : {},
    ...hideMarkNode ? { hideMark: readOptionalOoxmlBoolean(tableXmlVal(hideMarkNode)) ?? true } : {},
    ...conditionalStyle ? { conditionalStyle } : {}
  };
}
var TABLE_STYLE_PRECEDENCE = [
  "wholeTable",
  "band1Horz",
  "band2Horz",
  "band1Vert",
  "band2Vert",
  "firstCol",
  "lastCol",
  "firstRow",
  "lastRow",
  "nwCell",
  "neCell",
  "swCell",
  "seCell"
];
var CNF_STYLE_TO_TABLE_STYLE = [
  ["oddHBand", "band1Horz"],
  ["evenHBand", "band2Horz"],
  ["oddVBand", "band1Vert"],
  ["evenVBand", "band2Vert"],
  ["firstRow", "firstRow"],
  ["firstColumn", "firstCol"],
  ["lastRow", "lastRow"],
  ["lastColumn", "lastCol"],
  ["firstRowFirstColumn", "nwCell"],
  ["firstRowLastColumn", "neCell"],
  ["lastRowFirstColumn", "swCell"],
  ["lastRowLastColumn", "seCell"]
];
function resolveCascadedTableStyleProperties(styleId, styles) {
  const chain = resolveTableStyleChain(styleId, styles);
  return mergeTablePropertyPartials(...chain.map((style) => readTableProperties(style.table)));
}
function applyTableStyleCellProperties(rows, table, styles) {
  if (!table.styleId || !styles?.styles?.[table.styleId]) return;
  const rowBandSize = resolveTableStyleBandSize(table.styleId, styles, "w:tblStyleRowBandSize");
  const colBandSize = resolveTableStyleBandSize(table.styleId, styles, "w:tblStyleColBandSize");
  const numRows = rows.length;
  const numCells = rows.reduce((max, row) => Math.max(max, countTableRowColumns(row)), 0);
  rows.forEach((row, rowIndex) => {
    let columnIndex = row.gridBefore ?? 0;
    row.cells.forEach((cell) => {
      const styleTypes = determineTableCellStyleTypes(
        table.look,
        rowIndex,
        columnIndex,
        numRows,
        numCells,
        rowBandSize,
        colBandSize,
        row.conditionalStyle,
        cell.conditionalStyle
      );
      const styledCell = mergeTableCellPropertyPartials(
        ...styleTypes.map((type) => resolveConditionalTableCellStyle(table.styleId, styles, type))
      );
      applyTableCellStyleDefaults(cell, styledCell);
      columnIndex += Math.max(1, cell.gridSpan ?? 1);
    });
  });
}
function resolveConditionalTableCellStyle(styleId, styles, styleType) {
  const chain = resolveTableStyleChain(styleId, styles);
  return mergeTableCellPropertyPartials(...chain.map((style) => {
    const styleProperties = getChildren(style.raw).find((child2) => localName4(child2.name) === "tblStylePr" && tableXmlAttr(child2, "w:type") === styleType);
    return readTableCellProperties(firstChildByLocalName2(styleProperties, "tcPr"));
  }));
}
function resolveTableStyleChain(styleId, styles) {
  const chain = [];
  const seen = /* @__PURE__ */ new Set();
  let currentId = styleId;
  while (currentId && !seen.has(currentId)) {
    seen.add(currentId);
    const style = styles?.styles?.[currentId];
    if (!style || style.type !== "table") break;
    chain.push(style);
    currentId = style.basedOn;
  }
  return chain.reverse();
}
function resolveTableStyleBandSize(styleId, styles, nodeName) {
  for (const style of [...resolveTableStyleChain(styleId, styles)].reverse()) {
    const value = readPositiveInteger(tableXmlVal(firstChildByLocalName2(style.table, localName4(nodeName))));
    if (value != null) return value;
  }
  return 1;
}
function determineTableCellStyleTypes(look, rowIndex, cellIndex, numRows, numCells, rowBandSize, colBandSize, rowCnfStyle, cellCnfStyle) {
  const effectiveLook = {
    firstRow: look?.firstRow ?? true,
    lastRow: look?.lastRow ?? false,
    firstColumn: look?.firstColumn ?? true,
    lastColumn: look?.lastColumn ?? false,
    noHorizontalBand: look?.noHorizontalBand ?? false,
    noVerticalBand: look?.noVerticalBand ?? true
  };
  const applicable = /* @__PURE__ */ new Set(["wholeTable"]);
  const normalizedRowBandSize = rowBandSize > 0 ? rowBandSize : 1;
  const normalizedColBandSize = colBandSize > 0 ? colBandSize : 1;
  const bandRowIndex = Math.max(0, rowIndex - (effectiveLook.firstRow ? 1 : 0));
  const bandColIndex = Math.max(0, cellIndex - (effectiveLook.firstColumn ? 1 : 0));
  if (!effectiveLook.noHorizontalBand) {
    applicable.add(Math.floor(bandRowIndex / normalizedRowBandSize) % 2 === 0 ? "band1Horz" : "band2Horz");
  }
  if (!effectiveLook.noVerticalBand) {
    applicable.add(Math.floor(bandColIndex / normalizedColBandSize) % 2 === 0 ? "band1Vert" : "band2Vert");
  }
  const isFirstRow = effectiveLook.firstRow && rowIndex === 0;
  const isLastRow = effectiveLook.lastRow && numRows > 0 && rowIndex === numRows - 1;
  const isFirstCol = effectiveLook.firstColumn && cellIndex === 0;
  const isLastCol = effectiveLook.lastColumn && numCells > 0 && cellIndex === numCells - 1;
  if (isFirstRow) applicable.add("firstRow");
  if (isLastRow) applicable.add("lastRow");
  if (isFirstCol) applicable.add("firstCol");
  if (isLastCol) applicable.add("lastCol");
  if (isFirstRow && isFirstCol) applicable.add("nwCell");
  if (isFirstRow && isLastCol) applicable.add("neCell");
  if (isLastRow && isFirstCol) applicable.add("swCell");
  if (isLastRow && isLastCol) applicable.add("seCell");
  for (const [flag, styleType] of CNF_STYLE_TO_TABLE_STYLE) {
    if (rowCnfStyle?.[flag] === true || cellCnfStyle?.[flag] === true) applicable.add(styleType);
  }
  return TABLE_STYLE_PRECEDENCE.filter((styleType) => applicable.has(styleType));
}
function mergeTablePropertyPartials(...parts) {
  const merged = {};
  for (const part of parts) {
    if (!part) continue;
    Object.assign(merged, part);
    if (part.look) merged.look = { ...merged.look ?? {}, ...part.look };
    if (part.borders) merged.borders = { ...merged.borders ?? {}, ...part.borders };
    if (part.cellMargins) merged.cellMargins = { ...merged.cellMargins ?? {}, ...part.cellMargins };
    if (part.floatingTableProperties) merged.floatingTableProperties = { ...merged.floatingTableProperties ?? {}, ...part.floatingTableProperties };
  }
  return merged;
}
function mergeTableCellPropertyPartials(...parts) {
  const merged = {};
  for (const part of parts) {
    if (!part) continue;
    Object.assign(merged, part);
    if (part.borders) merged.borders = { ...merged.borders ?? {}, ...part.borders };
    if (part.cellMargins) merged.cellMargins = { ...merged.cellMargins ?? {}, ...part.cellMargins };
    if (part.shading) merged.shading = { ...part.shading };
  }
  return merged;
}
function applyTableCellStyleDefaults(cell, defaults) {
  if (defaults.borders) cell.borders = { ...defaults.borders ?? {}, ...cell.borders ?? {} };
  if (defaults.cellMargins) cell.cellMargins = { ...defaults.cellMargins ?? {}, ...cell.cellMargins ?? {} };
  if (!cell.shading && defaults.shading) cell.shading = defaults.shading;
  if (cell.verticalAlign == null && defaults.verticalAlign != null) cell.verticalAlign = defaults.verticalAlign;
  if (cell.noWrap == null && defaults.noWrap != null) cell.noWrap = defaults.noWrap;
  if (cell.textDirection == null && defaults.textDirection != null) cell.textDirection = defaults.textDirection;
  if (cell.fitText == null && defaults.fitText != null) cell.fitText = defaults.fitText;
  if (cell.hideMark == null && defaults.hideMark != null) cell.hideMark = defaults.hideMark;
}
function readTableMeasurement(node) {
  const value = readNonNegativeInteger(tableXmlAttr(node, "w:w"));
  if (value == null) return void 0;
  return { value, ...tableXmlAttr(node, "w:type") ? { type: tableXmlAttr(node, "w:type") } : {} };
}
function readTableLook(node) {
  if (!node) return void 0;
  return {
    ...tableXmlAttr(node, "w:val") ? { value: tableXmlAttr(node, "w:val") } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRow")) != null ? { firstRow: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRow")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRow")) != null ? { lastRow: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRow")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstColumn")) != null ? { firstColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastColumn")) != null ? { lastColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:noHBand")) != null ? { noHorizontalBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:noHBand")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:noVBand")) != null ? { noVerticalBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:noVBand")) } : {},
    ...node.attributes ? { attributes: { ...node.attributes } } : {}
  };
}
function readTableBorders(node) {
  const entries = (node?.children ?? []).flatMap((child2) => {
    if (child2.name === "#text") return [];
    const size = readNonNegativeInteger(tableXmlAttr(child2, "w:sz"));
    const space = readNonNegativeInteger(tableXmlAttr(child2, "w:space"));
    return [[localName4(child2.name), {
      ...tableXmlVal(child2) ? { val: tableXmlVal(child2) } : {},
      ...size != null ? { size } : {},
      ...space != null ? { space } : {},
      ...tableXmlAttr(child2, "w:color") ? { color: tableXmlAttr(child2, "w:color") } : {},
      ...child2.attributes ? { attributes: { ...child2.attributes } } : {}
    }]];
  });
  return entries.length ? Object.fromEntries(entries) : void 0;
}
function readTableShading(node) {
  if (!node) return void 0;
  return {
    ...tableXmlVal(node) ? { val: tableXmlVal(node) } : {},
    ...tableXmlAttr(node, "w:color") ? { color: tableXmlAttr(node, "w:color") } : {},
    ...tableXmlAttr(node, "w:fill") ? { fill: tableXmlAttr(node, "w:fill") } : {},
    ...node.attributes ? { attributes: { ...node.attributes } } : {}
  };
}
function readTableCellMargins(node) {
  const entries = (node?.children ?? []).flatMap((child2) => {
    if (child2.name === "#text") return [];
    const measurement = readTableMeasurement(child2);
    if (!measurement) return [];
    return [[localName4(child2.name), { ...measurement, ...child2.attributes ? { attributes: { ...child2.attributes } } : {} }]];
  });
  return entries.length ? Object.fromEntries(entries) : void 0;
}
function readTableConditionalStyle(node) {
  if (!node?.attributes) return void 0;
  const style = {
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:evenHBand")) != null ? { evenHBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:evenHBand")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:evenVBand")) != null ? { evenVBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:evenVBand")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstColumn")) != null ? { firstColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRow")) != null ? { firstRow: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRow")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRowFirstColumn")) != null ? { firstRowFirstColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRowFirstColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRowLastColumn")) != null ? { firstRowLastColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:firstRowLastColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastColumn")) != null ? { lastColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRow")) != null ? { lastRow: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRow")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRowFirstColumn")) != null ? { lastRowFirstColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRowFirstColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRowLastColumn")) != null ? { lastRowLastColumn: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:lastRowLastColumn")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:oddHBand")) != null ? { oddHBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:oddHBand")) } : {},
    ...readOptionalOoxmlBoolean(tableXmlAttr(node, "w:oddVBand")) != null ? { oddVBand: readOptionalOoxmlBoolean(tableXmlAttr(node, "w:oddVBand")) } : {},
    ...tableXmlAttr(node, "w:val") ? { value: tableXmlAttr(node, "w:val") } : {},
    attributes: { ...node.attributes }
  };
  return Object.keys(style).length > 1 ? style : void 0;
}
function readFloatingTableProperties(node) {
  if (!node?.attributes) return void 0;
  const floating = {
    ...readNonNegativeInteger(tableXmlAttr(node, "w:leftFromText")) != null ? { leftFromText: readNonNegativeInteger(tableXmlAttr(node, "w:leftFromText")) } : {},
    ...readNonNegativeInteger(tableXmlAttr(node, "w:rightFromText")) != null ? { rightFromText: readNonNegativeInteger(tableXmlAttr(node, "w:rightFromText")) } : {},
    ...readNonNegativeInteger(tableXmlAttr(node, "w:topFromText")) != null ? { topFromText: readNonNegativeInteger(tableXmlAttr(node, "w:topFromText")) } : {},
    ...readNonNegativeInteger(tableXmlAttr(node, "w:bottomFromText")) != null ? { bottomFromText: readNonNegativeInteger(tableXmlAttr(node, "w:bottomFromText")) } : {},
    ...readFiniteNumber(tableXmlAttr(node, "w:tblpX")) != null ? { x: readFiniteNumber(tableXmlAttr(node, "w:tblpX")) } : {},
    ...readFiniteNumber(tableXmlAttr(node, "w:tblpY")) != null ? { y: readFiniteNumber(tableXmlAttr(node, "w:tblpY")) } : {},
    ...tableXmlAttr(node, "w:horzAnchor") ? { horizontalAnchor: tableXmlAttr(node, "w:horzAnchor") } : {},
    ...tableXmlAttr(node, "w:vertAnchor") ? { verticalAnchor: tableXmlAttr(node, "w:vertAnchor") } : {},
    ...tableXmlAttr(node, "w:tblpXSpec") ? { xSpec: tableXmlAttr(node, "w:tblpXSpec") } : {},
    ...tableXmlAttr(node, "w:tblpYSpec") ? { ySpec: tableXmlAttr(node, "w:tblpYSpec") } : {},
    attributes: { ...node.attributes }
  };
  return Object.keys(floating).length > 1 ? floating : void 0;
}
function tableXmlVal(node) {
  return tableXmlAttr(node, "w:val");
}
function tableXmlAttr(node, name) {
  if (!node?.attributes) return void 0;
  const local = name.includes(":") ? name.slice(name.indexOf(":") + 1) : name;
  return node.attributes[name] ?? node.attributes[local] ?? node.attributes[`w:${local}`] ?? Object.entries(node.attributes).find(([key]) => (key.includes(":") ? key.slice(key.indexOf(":") + 1) : key) === local)?.[1];
}
function firstChildByLocalName2(node, name) {
  return getChildren(node).find((child2) => localName4(child2.name) === name);
}
function readPositiveInteger(value) {
  if (typeof value === "number" && Number.isInteger(value) && value > 0) return value;
  if (typeof value !== "string" || value.trim() === "") return void 0;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : void 0;
}
function readNonNegativeInteger(value) {
  if (typeof value === "number" && Number.isInteger(value) && value >= 0) return value;
  if (typeof value !== "string" || value.trim() === "") return void 0;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : void 0;
}
function localName4(name) {
  return name?.includes(":") ? name.slice(name.indexOf(":") + 1) : name ?? "";
}
function resolveMainDocumentPart(wordPackage) {
  return wordPackage.rootRelationships.relationships.find(
    (relationship) => relationship.type.endsWith("/officeDocument")
  )?.resolvedTarget;
}
function resolveRelatedPart(context, relationshipSuffix, fallbackPath) {
  return context.relationships?.relationships.find(
    (relationship) => relationship.type.endsWith(relationshipSuffix) && relationship.resolvedTarget
  )?.resolvedTarget ?? fallbackPath;
}
function isBlock(node) {
  return node.type === "paragraph" || node.type === "table" || node.type === "blockWrapper" || node.type === "unsupportedBlock";
}
function isInline(node) {
  return !isBlock(node);
}

// src/apps/business-os/modules/documents/document-format/src/index.ts
var DOCUMENT_FORMAT_ESM_VERSION = "0.2.0-wordport-format";
async function importDocx(input) {
  const pkg = await openWordPortPackage(input);
  const document = await importDocxToWordPortDocument(pkg);
  return {
    document,
    diagnostics: pkg.diagnostics ?? []
  };
}
async function mergeDocxFields(input, values, options = {}) {
  const pkg = await openWordPortPackage(input);
  const document = await importDocxToWordPortDocument(pkg);
  const report = materializeDocxMergeFields(document, values);
  const textReport = materializeDocxTextReplacements(document, options.textReplacements ?? {});
  if (options.strict !== false && (report.missingFields.length || textReport.missingTextReplacements.length)) {
    const missing = [...report.missingFields, ...textReport.missingTextReplacements];
    throw new Error(`DOCX merge values are missing for: ${missing.join(", ")}`);
  }
  return {
    bytes: await exportWordPortDocumentToDocx(document, { package: pkg }),
    ...report,
    ...textReport
  };
}
function materializeDocxMergeFields(document, values) {
  const normalizedValues = new Map(
    Object.entries(values ?? {}).map(([key, value]) => [key.trim().toLowerCase(), value == null ? "" : String(value)])
  );
  const merged = /* @__PURE__ */ new Set();
  const missing = /* @__PURE__ */ new Set();
  visitDocumentValue(document, (reference) => {
    const field = reference.field;
    const name = mergeFieldName(field?.instruction);
    if (!field || !name) return;
    const value = normalizedValues.get(name.toLowerCase());
    if (value === void 0) {
      missing.add(name);
      return;
    }
    materializeFieldReference(field, value);
    merged.add(name);
  });
  materializePlainMergePlaceholders(document, normalizedValues, merged, missing);
  return {
    mergedFields: [...merged].sort((left, right) => left.localeCompare(right)),
    missingFields: [...missing].sort((left, right) => left.localeCompare(right))
  };
}
function materializeDocxTextReplacements(document, replacements) {
  const replaced = /* @__PURE__ */ new Set();
  const missing = /* @__PURE__ */ new Set();
  for (const [search, rawReplacement] of Object.entries(replacements ?? {})) {
    const replacement = rawReplacement == null ? "" : String(rawReplacement);
    const count = replaceDocumentText(document, search, replacement);
    if (count) replaced.add(search);
    else missing.add(search);
  }
  return {
    replacedText: [...replaced].sort((left, right) => left.localeCompare(right)),
    missingTextReplacements: [...missing].sort((left, right) => left.localeCompare(right))
  };
}
function replaceDocumentText(value, search, replacement) {
  if (!search) return 0;
  if (Array.isArray(value)) {
    return value.reduce((count2, item) => count2 + replaceDocumentText(item, search, replacement), 0);
  }
  if (!value || typeof value !== "object") return 0;
  const candidate = value;
  let count = 0;
  for (const [key, item] of Object.entries(candidate)) {
    if (key === "text" && typeof item === "string" && item.includes(search)) {
      candidate[key] = item.split(search).join(replacement);
      count += item.split(search).length - 1;
      continue;
    }
    count += replaceDocumentText(item, search, replacement);
  }
  return count;
}
function materializePlainMergePlaceholders(value, values, merged, missing) {
  if (Array.isArray(value)) {
    value.forEach((item) => materializePlainMergePlaceholders(item, values, merged, missing));
    return;
  }
  if (!value || typeof value !== "object") return;
  const candidate = value;
  for (const [key, item] of Object.entries(candidate)) {
    if (key === "text" && typeof item === "string") {
      candidate[key] = item.replace(/«\s*([^»]+?)\s*»/g, (token, rawName) => {
        const name = rawName.trim();
        const replacement = values.get(name.toLowerCase());
        if (replacement === void 0) {
          missing.add(name);
          return token;
        }
        merged.add(name);
        return replacement;
      });
      continue;
    }
    materializePlainMergePlaceholders(item, values, merged, missing);
  }
}
function visitDocumentValue(value, visit) {
  if (Array.isArray(value)) {
    value.forEach((item) => visitDocumentValue(item, visit));
    return;
  }
  if (!value || typeof value !== "object") return;
  const candidate = value;
  if (candidate.type === "reference") visit(value);
  Object.values(candidate).forEach((item) => visitDocumentValue(item, visit));
}
function mergeFieldName(instruction) {
  const match = String(instruction ?? "").match(/^\s*MERGEFIELD\s+(?:"([^"]+)"|([^\\\s]+))/i);
  return (match?.[1] ?? match?.[2])?.trim() || void 0;
}
function materializeFieldReference(field, value) {
  const resultRuns = field.kind === "complex" ? complexResultRuns(field.rawNodes ?? []) : simpleResultRuns(field.rawNode);
  if (resultRuns.length) {
    setResultRunText(resultRuns, value);
    field.kind = "complex";
    field.rawNodes = resultRuns;
    delete field.rawNode;
  } else {
    delete field.rawNodes;
    delete field.rawNode;
    field.supported = false;
  }
  field.resultText = value;
  field.displayText = value;
}
function complexResultRuns(nodes) {
  let collecting = false;
  const result = [];
  for (const node of nodes) {
    const fieldType = descendantAttribute(node, "w:fldChar", "w:fldCharType");
    if (fieldType === "separate") {
      collecting = true;
      continue;
    }
    if (fieldType === "end") break;
    if (collecting) result.push(cloneXmlNode(node));
  }
  return result;
}
function simpleResultRuns(node) {
  return (node?.children ?? []).map(cloneXmlNode);
}
function descendantAttribute(node, name, attribute) {
  if (node.name === name) return node.attributes?.[attribute];
  for (const child2 of node.children ?? []) {
    const value = descendantAttribute(child2, name, attribute);
    if (value !== void 0) return value;
  }
  return void 0;
}
function setResultRunText(nodes, value) {
  const textNodes = [];
  nodes.forEach((node) => collectXmlNodes(node, "w:t", textNodes));
  if (!textNodes.length) {
    const run = nodes.find((node) => node.name === "w:r") ?? nodes[0];
    run.children = [...run.children ?? [], wordTextNode(value)];
    return;
  }
  textNodes.forEach((node, index) => {
    node.children = [{ name: "#text", text: index === 0 ? value : "" }];
    if (index === 0 && /^\s|\s$/.test(value)) {
      node.attributes = { ...node.attributes ?? {}, "xml:space": "preserve" };
    }
  });
}
function collectXmlNodes(node, name, result) {
  if (node.name === name) result.push(node);
  (node.children ?? []).forEach((child2) => collectXmlNodes(child2, name, result));
}
function wordTextNode(value) {
  return {
    name: "w:t",
    .../^\s|\s$/.test(value) ? { attributes: { "xml:space": "preserve" } } : {},
    children: [{ name: "#text", text: value }]
  };
}
function cloneXmlNode(node) {
  return JSON.parse(JSON.stringify(node));
}
function importMarkdown(markdown) {
  return {
    document: markdownToWordPortDocument(markdown),
    diagnostics: []
  };
}
function exportMarkdown(document) {
  return (document.body?.blocks ?? []).map(blockToMarkdown).join("\n\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}
function getDocumentText(document) {
  return (document.body?.blocks ?? []).map(blockText).filter(Boolean).join("\n");
}
function paragraphText(paragraph) {
  return paragraph.runs.map(inlineText).join("");
}
function blockText(block) {
  if (block.type === "paragraph") return paragraphText(block);
  if (block.type === "table") {
    return block.rows.map((row) => row.cells.map(cellText).join("	")).join("\n");
  }
  if (block.type === "blockWrapper") return block.blocks.map(blockText).join("\n");
  return "";
}
function cellText(cell) {
  return cell.blocks.map(blockText).join("\n");
}
function inlineText(inline) {
  if (inline.type === "text") return inline.text;
  if (inline.type === "run") return inline.content.map(inlineText).join("");
  if (inline.type === "tab") return "	";
  if (inline.type === "break") return "\n";
  if (inline.type === "hyperlink") return inline.content.map(inlineText).join("");
  if (inline.type === "inlineWrapper") return inline.content.map(inlineText).join("");
  if (inline.type === "reference") return inline.field?.displayText ?? "";
  if (inline.type === "drawing") return inline.altText ? `[${inline.altText}]` : "";
  return "";
}
function markdownToWordPortDocument(markdown) {
  const blocks = [];
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  let paragraph = [];
  let tableRows = [];
  const flushParagraph = () => {
    if (!paragraph.length) return;
    blocks.push(markdownParagraph(paragraph.join("\n")));
    paragraph = [];
  };
  const flushTable = () => {
    if (!tableRows.length) return;
    blocks.push({
      type: "table",
      rows: tableRows.map((cells) => ({
        type: "tableRow",
        cells: cells.map((cell) => ({
          type: "tableCell",
          blocks: [markdownParagraph(cell.trim())]
        }))
      }))
    });
    tableRows = [];
  };
  for (const line of lines) {
    if (!line.trim()) {
      flushParagraph();
      flushTable();
      continue;
    }
    if (/^\s*\|.*\|\s*$/.test(line)) {
      flushParagraph();
      const cells = line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
      if (!cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()))) tableRows.push(cells);
      continue;
    }
    flushTable();
    paragraph.push(line);
  }
  flushParagraph();
  flushTable();
  return {
    type: "document",
    body: { type: "body", blocks },
    source: { documentAttributes: { "data-document-mode": "markdown" } }
  };
}
function markdownParagraph(text) {
  const heading = text.match(/^(#{1,6})\s+(.*)$/);
  const quote = text.match(/^>\s?(.*)$/);
  const bullet = text.match(/^[-*+]\s+(.*)$/);
  const ordered = text.match(/^\d+[.)]\s+(.*)$/);
  const cleanText = heading?.[2] ?? quote?.[1] ?? bullet?.[1] ?? ordered?.[1] ?? text;
  const paragraph = {
    type: "paragraph",
    runs: [{ type: "text", text: cleanText }]
  };
  if (heading) {
    paragraph.effectiveProperties = { styleId: `Heading${heading[1].length}` };
  }
  if (quote) {
    paragraph.effectiveProperties = { ...paragraph.effectiveProperties ?? {}, styleId: "Quote" };
  }
  if (bullet || ordered) {
    paragraph.effectiveProperties = {
      ...paragraph.effectiveProperties ?? {},
      listRendering: {
        markerText: bullet ? "-" : "1.",
        markerSuffixText: " ",
        level: 0,
        numberingId: ordered ? "markdown-ordered" : "markdown-bullet",
        abstractId: ordered ? "markdown-ordered" : "markdown-bullet",
        numberingType: ordered ? "decimal" : "bullet",
        path: [1],
        suffix: "space"
      }
    };
  }
  return paragraph;
}
function blockToMarkdown(block) {
  if (block.type === "paragraph") return paragraphToMarkdown(block);
  if (block.type === "table") return tableToMarkdown(block);
  if (block.type === "blockWrapper") return block.blocks.map(blockToMarkdown).join("\n\n");
  return `<!-- Unsupported block: ${block.name} -->`;
}
function paragraphToMarkdown(paragraph) {
  const text = paragraphText(paragraph);
  const styleId = paragraph.effectiveProperties?.styleId ?? "";
  const heading = styleId.match(/^Heading([1-6])$/i);
  if (heading) return `${"#".repeat(Number(heading[1]))} ${text}`;
  const list = paragraph.effectiveProperties?.listRendering;
  if (list?.numberingType === "bullet") return `- ${text}`;
  if (list?.numberingType === "decimal") return `1. ${text}`;
  if (/quote/i.test(styleId)) return `> ${text}`;
  return text;
}
function tableToMarkdown(table) {
  const rows = table.rows.map((row) => row.cells.map((cell) => cellText(cell).replace(/\s+/g, " ").trim()));
  if (!rows.length) return "";
  const columnCount = Math.max(...rows.map((row) => row.length));
  const normalize = (row) => Array.from({ length: columnCount }, (_, index) => row[index] ?? "");
  const [head, ...body] = rows.map(normalize);
  return [
    `| ${head.join(" | ")} |`,
    `| ${head.map(() => "---").join(" | ")} |`,
    ...body.map((row) => `| ${row.join(" | ")} |`)
  ].join("\n");
}
export {
  DOCUMENT_FORMAT_ESM_VERSION,
  exportMarkdown,
  getDocumentText,
  importDocx,
  importMarkdown,
  materializeDocxMergeFields,
  materializeDocxTextReplacements,
  mergeDocxFields
};
