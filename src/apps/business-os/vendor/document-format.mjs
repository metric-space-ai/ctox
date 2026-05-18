var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __require = /* @__PURE__ */ ((x) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(x, {
  get: (a, b) => (typeof require !== "undefined" ? require : a)[b]
}) : x)(function(x) {
  if (typeof require !== "undefined") return require.apply(this, arguments);
  throw Error('Dynamic require of "' + x + '" is not supported');
});
var __commonJS = (cb, mod) => function __require2() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));

// archive/2026-05-18-cleanup/generated/templates/business-basic/node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/dist/jszip.min.js
var require_jszip_min = __commonJS({
  "archive/2026-05-18-cleanup/generated/templates/business-basic/node_modules/.pnpm/jszip@3.10.1/node_modules/jszip/dist/jszip.min.js"(exports, module) {
    !(function(e) {
      if ("object" == typeof exports && "undefined" != typeof module) module.exports = e();
      else if ("function" == typeof define && define.amd) define([], e);
      else {
        ("undefined" != typeof window ? window : "undefined" != typeof global ? global : "undefined" != typeof self ? self : this).JSZip = e();
      }
    })(function() {
      return (function s(a, o, h) {
        function u(r, e2) {
          if (!o[r]) {
            if (!a[r]) {
              var t = "function" == typeof __require && __require;
              if (!e2 && t) return t(r, true);
              if (l) return l(r, true);
              var n = new Error("Cannot find module '" + r + "'");
              throw n.code = "MODULE_NOT_FOUND", n;
            }
            var i = o[r] = { exports: {} };
            a[r][0].call(i.exports, function(e3) {
              var t2 = a[r][1][e3];
              return u(t2 || e3);
            }, i, i.exports, s, a, o, h);
          }
          return o[r].exports;
        }
        for (var l = "function" == typeof __require && __require, e = 0; e < h.length; e++) u(h[e]);
        return u;
      })({ 1: [function(e, t, r) {
        "use strict";
        var d = e("./utils"), c = e("./support"), p = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
        r.encode = function(e2) {
          for (var t2, r2, n, i, s, a, o, h = [], u = 0, l = e2.length, f = l, c2 = "string" !== d.getTypeOf(e2); u < e2.length; ) f = l - u, n = c2 ? (t2 = e2[u++], r2 = u < l ? e2[u++] : 0, u < l ? e2[u++] : 0) : (t2 = e2.charCodeAt(u++), r2 = u < l ? e2.charCodeAt(u++) : 0, u < l ? e2.charCodeAt(u++) : 0), i = t2 >> 2, s = (3 & t2) << 4 | r2 >> 4, a = 1 < f ? (15 & r2) << 2 | n >> 6 : 64, o = 2 < f ? 63 & n : 64, h.push(p.charAt(i) + p.charAt(s) + p.charAt(a) + p.charAt(o));
          return h.join("");
        }, r.decode = function(e2) {
          var t2, r2, n, i, s, a, o = 0, h = 0, u = "data:";
          if (e2.substr(0, u.length) === u) throw new Error("Invalid base64 input, it looks like a data url.");
          var l, f = 3 * (e2 = e2.replace(/[^A-Za-z0-9+/=]/g, "")).length / 4;
          if (e2.charAt(e2.length - 1) === p.charAt(64) && f--, e2.charAt(e2.length - 2) === p.charAt(64) && f--, f % 1 != 0) throw new Error("Invalid base64 input, bad content length.");
          for (l = c.uint8array ? new Uint8Array(0 | f) : new Array(0 | f); o < e2.length; ) t2 = p.indexOf(e2.charAt(o++)) << 2 | (i = p.indexOf(e2.charAt(o++))) >> 4, r2 = (15 & i) << 4 | (s = p.indexOf(e2.charAt(o++))) >> 2, n = (3 & s) << 6 | (a = p.indexOf(e2.charAt(o++))), l[h++] = t2, 64 !== s && (l[h++] = r2), 64 !== a && (l[h++] = n);
          return l;
        };
      }, { "./support": 30, "./utils": 32 }], 2: [function(e, t, r) {
        "use strict";
        var n = e("./external"), i = e("./stream/DataWorker"), s = e("./stream/Crc32Probe"), a = e("./stream/DataLengthProbe");
        function o(e2, t2, r2, n2, i2) {
          this.compressedSize = e2, this.uncompressedSize = t2, this.crc32 = r2, this.compression = n2, this.compressedContent = i2;
        }
        o.prototype = { getContentWorker: function() {
          var e2 = new i(n.Promise.resolve(this.compressedContent)).pipe(this.compression.uncompressWorker()).pipe(new a("data_length")), t2 = this;
          return e2.on("end", function() {
            if (this.streamInfo.data_length !== t2.uncompressedSize) throw new Error("Bug : uncompressed data size mismatch");
          }), e2;
        }, getCompressedWorker: function() {
          return new i(n.Promise.resolve(this.compressedContent)).withStreamInfo("compressedSize", this.compressedSize).withStreamInfo("uncompressedSize", this.uncompressedSize).withStreamInfo("crc32", this.crc32).withStreamInfo("compression", this.compression);
        } }, o.createWorkerFrom = function(e2, t2, r2) {
          return e2.pipe(new s()).pipe(new a("uncompressedSize")).pipe(t2.compressWorker(r2)).pipe(new a("compressedSize")).withStreamInfo("compression", t2);
        }, t.exports = o;
      }, { "./external": 6, "./stream/Crc32Probe": 25, "./stream/DataLengthProbe": 26, "./stream/DataWorker": 27 }], 3: [function(e, t, r) {
        "use strict";
        var n = e("./stream/GenericWorker");
        r.STORE = { magic: "\0\0", compressWorker: function() {
          return new n("STORE compression");
        }, uncompressWorker: function() {
          return new n("STORE decompression");
        } }, r.DEFLATE = e("./flate");
      }, { "./flate": 7, "./stream/GenericWorker": 28 }], 4: [function(e, t, r) {
        "use strict";
        var n = e("./utils");
        var o = (function() {
          for (var e2, t2 = [], r2 = 0; r2 < 256; r2++) {
            e2 = r2;
            for (var n2 = 0; n2 < 8; n2++) e2 = 1 & e2 ? 3988292384 ^ e2 >>> 1 : e2 >>> 1;
            t2[r2] = e2;
          }
          return t2;
        })();
        t.exports = function(e2, t2) {
          return void 0 !== e2 && e2.length ? "string" !== n.getTypeOf(e2) ? (function(e3, t3, r2, n2) {
            var i = o, s = n2 + r2;
            e3 ^= -1;
            for (var a = n2; a < s; a++) e3 = e3 >>> 8 ^ i[255 & (e3 ^ t3[a])];
            return -1 ^ e3;
          })(0 | t2, e2, e2.length, 0) : (function(e3, t3, r2, n2) {
            var i = o, s = n2 + r2;
            e3 ^= -1;
            for (var a = n2; a < s; a++) e3 = e3 >>> 8 ^ i[255 & (e3 ^ t3.charCodeAt(a))];
            return -1 ^ e3;
          })(0 | t2, e2, e2.length, 0) : 0;
        };
      }, { "./utils": 32 }], 5: [function(e, t, r) {
        "use strict";
        r.base64 = false, r.binary = false, r.dir = false, r.createFolders = true, r.date = null, r.compression = null, r.compressionOptions = null, r.comment = null, r.unixPermissions = null, r.dosPermissions = null;
      }, {}], 6: [function(e, t, r) {
        "use strict";
        var n = null;
        n = "undefined" != typeof Promise ? Promise : e("lie"), t.exports = { Promise: n };
      }, { lie: 37 }], 7: [function(e, t, r) {
        "use strict";
        var n = "undefined" != typeof Uint8Array && "undefined" != typeof Uint16Array && "undefined" != typeof Uint32Array, i = e("pako"), s = e("./utils"), a = e("./stream/GenericWorker"), o = n ? "uint8array" : "array";
        function h(e2, t2) {
          a.call(this, "FlateWorker/" + e2), this._pako = null, this._pakoAction = e2, this._pakoOptions = t2, this.meta = {};
        }
        r.magic = "\b\0", s.inherits(h, a), h.prototype.processChunk = function(e2) {
          this.meta = e2.meta, null === this._pako && this._createPako(), this._pako.push(s.transformTo(o, e2.data), false);
        }, h.prototype.flush = function() {
          a.prototype.flush.call(this), null === this._pako && this._createPako(), this._pako.push([], true);
        }, h.prototype.cleanUp = function() {
          a.prototype.cleanUp.call(this), this._pako = null;
        }, h.prototype._createPako = function() {
          this._pako = new i[this._pakoAction]({ raw: true, level: this._pakoOptions.level || -1 });
          var t2 = this;
          this._pako.onData = function(e2) {
            t2.push({ data: e2, meta: t2.meta });
          };
        }, r.compressWorker = function(e2) {
          return new h("Deflate", e2);
        }, r.uncompressWorker = function() {
          return new h("Inflate", {});
        };
      }, { "./stream/GenericWorker": 28, "./utils": 32, pako: 38 }], 8: [function(e, t, r) {
        "use strict";
        function A(e2, t2) {
          var r2, n2 = "";
          for (r2 = 0; r2 < t2; r2++) n2 += String.fromCharCode(255 & e2), e2 >>>= 8;
          return n2;
        }
        function n(e2, t2, r2, n2, i2, s2) {
          var a, o, h = e2.file, u = e2.compression, l = s2 !== O.utf8encode, f = I.transformTo("string", s2(h.name)), c = I.transformTo("string", O.utf8encode(h.name)), d = h.comment, p = I.transformTo("string", s2(d)), m = I.transformTo("string", O.utf8encode(d)), _ = c.length !== h.name.length, g = m.length !== d.length, b = "", v = "", y = "", w = h.dir, k = h.date, x = { crc32: 0, compressedSize: 0, uncompressedSize: 0 };
          t2 && !r2 || (x.crc32 = e2.crc32, x.compressedSize = e2.compressedSize, x.uncompressedSize = e2.uncompressedSize);
          var S = 0;
          t2 && (S |= 8), l || !_ && !g || (S |= 2048);
          var z = 0, C = 0;
          w && (z |= 16), "UNIX" === i2 ? (C = 798, z |= (function(e3, t3) {
            var r3 = e3;
            return e3 || (r3 = t3 ? 16893 : 33204), (65535 & r3) << 16;
          })(h.unixPermissions, w)) : (C = 20, z |= (function(e3) {
            return 63 & (e3 || 0);
          })(h.dosPermissions)), a = k.getUTCHours(), a <<= 6, a |= k.getUTCMinutes(), a <<= 5, a |= k.getUTCSeconds() / 2, o = k.getUTCFullYear() - 1980, o <<= 4, o |= k.getUTCMonth() + 1, o <<= 5, o |= k.getUTCDate(), _ && (v = A(1, 1) + A(B(f), 4) + c, b += "up" + A(v.length, 2) + v), g && (y = A(1, 1) + A(B(p), 4) + m, b += "uc" + A(y.length, 2) + y);
          var E = "";
          return E += "\n\0", E += A(S, 2), E += u.magic, E += A(a, 2), E += A(o, 2), E += A(x.crc32, 4), E += A(x.compressedSize, 4), E += A(x.uncompressedSize, 4), E += A(f.length, 2), E += A(b.length, 2), { fileRecord: R.LOCAL_FILE_HEADER + E + f + b, dirRecord: R.CENTRAL_FILE_HEADER + A(C, 2) + E + A(p.length, 2) + "\0\0\0\0" + A(z, 4) + A(n2, 4) + f + b + p };
        }
        var I = e("../utils"), i = e("../stream/GenericWorker"), O = e("../utf8"), B = e("../crc32"), R = e("../signature");
        function s(e2, t2, r2, n2) {
          i.call(this, "ZipFileWorker"), this.bytesWritten = 0, this.zipComment = t2, this.zipPlatform = r2, this.encodeFileName = n2, this.streamFiles = e2, this.accumulate = false, this.contentBuffer = [], this.dirRecords = [], this.currentSourceOffset = 0, this.entriesCount = 0, this.currentFile = null, this._sources = [];
        }
        I.inherits(s, i), s.prototype.push = function(e2) {
          var t2 = e2.meta.percent || 0, r2 = this.entriesCount, n2 = this._sources.length;
          this.accumulate ? this.contentBuffer.push(e2) : (this.bytesWritten += e2.data.length, i.prototype.push.call(this, { data: e2.data, meta: { currentFile: this.currentFile, percent: r2 ? (t2 + 100 * (r2 - n2 - 1)) / r2 : 100 } }));
        }, s.prototype.openedSource = function(e2) {
          this.currentSourceOffset = this.bytesWritten, this.currentFile = e2.file.name;
          var t2 = this.streamFiles && !e2.file.dir;
          if (t2) {
            var r2 = n(e2, t2, false, this.currentSourceOffset, this.zipPlatform, this.encodeFileName);
            this.push({ data: r2.fileRecord, meta: { percent: 0 } });
          } else this.accumulate = true;
        }, s.prototype.closedSource = function(e2) {
          this.accumulate = false;
          var t2 = this.streamFiles && !e2.file.dir, r2 = n(e2, t2, true, this.currentSourceOffset, this.zipPlatform, this.encodeFileName);
          if (this.dirRecords.push(r2.dirRecord), t2) this.push({ data: (function(e3) {
            return R.DATA_DESCRIPTOR + A(e3.crc32, 4) + A(e3.compressedSize, 4) + A(e3.uncompressedSize, 4);
          })(e2), meta: { percent: 100 } });
          else for (this.push({ data: r2.fileRecord, meta: { percent: 0 } }); this.contentBuffer.length; ) this.push(this.contentBuffer.shift());
          this.currentFile = null;
        }, s.prototype.flush = function() {
          for (var e2 = this.bytesWritten, t2 = 0; t2 < this.dirRecords.length; t2++) this.push({ data: this.dirRecords[t2], meta: { percent: 100 } });
          var r2 = this.bytesWritten - e2, n2 = (function(e3, t3, r3, n3, i2) {
            var s2 = I.transformTo("string", i2(n3));
            return R.CENTRAL_DIRECTORY_END + "\0\0\0\0" + A(e3, 2) + A(e3, 2) + A(t3, 4) + A(r3, 4) + A(s2.length, 2) + s2;
          })(this.dirRecords.length, r2, e2, this.zipComment, this.encodeFileName);
          this.push({ data: n2, meta: { percent: 100 } });
        }, s.prototype.prepareNextSource = function() {
          this.previous = this._sources.shift(), this.openedSource(this.previous.streamInfo), this.isPaused ? this.previous.pause() : this.previous.resume();
        }, s.prototype.registerPrevious = function(e2) {
          this._sources.push(e2);
          var t2 = this;
          return e2.on("data", function(e3) {
            t2.processChunk(e3);
          }), e2.on("end", function() {
            t2.closedSource(t2.previous.streamInfo), t2._sources.length ? t2.prepareNextSource() : t2.end();
          }), e2.on("error", function(e3) {
            t2.error(e3);
          }), this;
        }, s.prototype.resume = function() {
          return !!i.prototype.resume.call(this) && (!this.previous && this._sources.length ? (this.prepareNextSource(), true) : this.previous || this._sources.length || this.generatedError ? void 0 : (this.end(), true));
        }, s.prototype.error = function(e2) {
          var t2 = this._sources;
          if (!i.prototype.error.call(this, e2)) return false;
          for (var r2 = 0; r2 < t2.length; r2++) try {
            t2[r2].error(e2);
          } catch (e3) {
          }
          return true;
        }, s.prototype.lock = function() {
          i.prototype.lock.call(this);
          for (var e2 = this._sources, t2 = 0; t2 < e2.length; t2++) e2[t2].lock();
        }, t.exports = s;
      }, { "../crc32": 4, "../signature": 23, "../stream/GenericWorker": 28, "../utf8": 31, "../utils": 32 }], 9: [function(e, t, r) {
        "use strict";
        var u = e("../compressions"), n = e("./ZipFileWorker");
        r.generateWorker = function(e2, a, t2) {
          var o = new n(a.streamFiles, t2, a.platform, a.encodeFileName), h = 0;
          try {
            e2.forEach(function(e3, t3) {
              h++;
              var r2 = (function(e4, t4) {
                var r3 = e4 || t4, n3 = u[r3];
                if (!n3) throw new Error(r3 + " is not a valid compression method !");
                return n3;
              })(t3.options.compression, a.compression), n2 = t3.options.compressionOptions || a.compressionOptions || {}, i = t3.dir, s = t3.date;
              t3._compressWorker(r2, n2).withStreamInfo("file", { name: e3, dir: i, date: s, comment: t3.comment || "", unixPermissions: t3.unixPermissions, dosPermissions: t3.dosPermissions }).pipe(o);
            }), o.entriesCount = h;
          } catch (e3) {
            o.error(e3);
          }
          return o;
        };
      }, { "../compressions": 3, "./ZipFileWorker": 8 }], 10: [function(e, t, r) {
        "use strict";
        function n() {
          if (!(this instanceof n)) return new n();
          if (arguments.length) throw new Error("The constructor with parameters has been removed in JSZip 3.0, please check the upgrade guide.");
          this.files = /* @__PURE__ */ Object.create(null), this.comment = null, this.root = "", this.clone = function() {
            var e2 = new n();
            for (var t2 in this) "function" != typeof this[t2] && (e2[t2] = this[t2]);
            return e2;
          };
        }
        (n.prototype = e("./object")).loadAsync = e("./load"), n.support = e("./support"), n.defaults = e("./defaults"), n.version = "3.10.1", n.loadAsync = function(e2, t2) {
          return new n().loadAsync(e2, t2);
        }, n.external = e("./external"), t.exports = n;
      }, { "./defaults": 5, "./external": 6, "./load": 11, "./object": 15, "./support": 30 }], 11: [function(e, t, r) {
        "use strict";
        var u = e("./utils"), i = e("./external"), n = e("./utf8"), s = e("./zipEntries"), a = e("./stream/Crc32Probe"), l = e("./nodejsUtils");
        function f(n2) {
          return new i.Promise(function(e2, t2) {
            var r2 = n2.decompressed.getContentWorker().pipe(new a());
            r2.on("error", function(e3) {
              t2(e3);
            }).on("end", function() {
              r2.streamInfo.crc32 !== n2.decompressed.crc32 ? t2(new Error("Corrupted zip : CRC32 mismatch")) : e2();
            }).resume();
          });
        }
        t.exports = function(e2, o) {
          var h = this;
          return o = u.extend(o || {}, { base64: false, checkCRC32: false, optimizedBinaryString: false, createFolders: false, decodeFileName: n.utf8decode }), l.isNode && l.isStream(e2) ? i.Promise.reject(new Error("JSZip can't accept a stream when loading a zip file.")) : u.prepareContent("the loaded zip file", e2, true, o.optimizedBinaryString, o.base64).then(function(e3) {
            var t2 = new s(o);
            return t2.load(e3), t2;
          }).then(function(e3) {
            var t2 = [i.Promise.resolve(e3)], r2 = e3.files;
            if (o.checkCRC32) for (var n2 = 0; n2 < r2.length; n2++) t2.push(f(r2[n2]));
            return i.Promise.all(t2);
          }).then(function(e3) {
            for (var t2 = e3.shift(), r2 = t2.files, n2 = 0; n2 < r2.length; n2++) {
              var i2 = r2[n2], s2 = i2.fileNameStr, a2 = u.resolve(i2.fileNameStr);
              h.file(a2, i2.decompressed, { binary: true, optimizedBinaryString: true, date: i2.date, dir: i2.dir, comment: i2.fileCommentStr.length ? i2.fileCommentStr : null, unixPermissions: i2.unixPermissions, dosPermissions: i2.dosPermissions, createFolders: o.createFolders }), i2.dir || (h.file(a2).unsafeOriginalName = s2);
            }
            return t2.zipComment.length && (h.comment = t2.zipComment), h;
          });
        };
      }, { "./external": 6, "./nodejsUtils": 14, "./stream/Crc32Probe": 25, "./utf8": 31, "./utils": 32, "./zipEntries": 33 }], 12: [function(e, t, r) {
        "use strict";
        var n = e("../utils"), i = e("../stream/GenericWorker");
        function s(e2, t2) {
          i.call(this, "Nodejs stream input adapter for " + e2), this._upstreamEnded = false, this._bindStream(t2);
        }
        n.inherits(s, i), s.prototype._bindStream = function(e2) {
          var t2 = this;
          (this._stream = e2).pause(), e2.on("data", function(e3) {
            t2.push({ data: e3, meta: { percent: 0 } });
          }).on("error", function(e3) {
            t2.isPaused ? this.generatedError = e3 : t2.error(e3);
          }).on("end", function() {
            t2.isPaused ? t2._upstreamEnded = true : t2.end();
          });
        }, s.prototype.pause = function() {
          return !!i.prototype.pause.call(this) && (this._stream.pause(), true);
        }, s.prototype.resume = function() {
          return !!i.prototype.resume.call(this) && (this._upstreamEnded ? this.end() : this._stream.resume(), true);
        }, t.exports = s;
      }, { "../stream/GenericWorker": 28, "../utils": 32 }], 13: [function(e, t, r) {
        "use strict";
        var i = e("readable-stream").Readable;
        function n(e2, t2, r2) {
          i.call(this, t2), this._helper = e2;
          var n2 = this;
          e2.on("data", function(e3, t3) {
            n2.push(e3) || n2._helper.pause(), r2 && r2(t3);
          }).on("error", function(e3) {
            n2.emit("error", e3);
          }).on("end", function() {
            n2.push(null);
          });
        }
        e("../utils").inherits(n, i), n.prototype._read = function() {
          this._helper.resume();
        }, t.exports = n;
      }, { "../utils": 32, "readable-stream": 16 }], 14: [function(e, t, r) {
        "use strict";
        t.exports = { isNode: "undefined" != typeof Buffer, newBufferFrom: function(e2, t2) {
          if (Buffer.from && Buffer.from !== Uint8Array.from) return Buffer.from(e2, t2);
          if ("number" == typeof e2) throw new Error('The "data" argument must not be a number');
          return new Buffer(e2, t2);
        }, allocBuffer: function(e2) {
          if (Buffer.alloc) return Buffer.alloc(e2);
          var t2 = new Buffer(e2);
          return t2.fill(0), t2;
        }, isBuffer: function(e2) {
          return Buffer.isBuffer(e2);
        }, isStream: function(e2) {
          return e2 && "function" == typeof e2.on && "function" == typeof e2.pause && "function" == typeof e2.resume;
        } };
      }, {}], 15: [function(e, t, r) {
        "use strict";
        function s(e2, t2, r2) {
          var n2, i2 = u.getTypeOf(t2), s2 = u.extend(r2 || {}, f);
          s2.date = s2.date || /* @__PURE__ */ new Date(), null !== s2.compression && (s2.compression = s2.compression.toUpperCase()), "string" == typeof s2.unixPermissions && (s2.unixPermissions = parseInt(s2.unixPermissions, 8)), s2.unixPermissions && 16384 & s2.unixPermissions && (s2.dir = true), s2.dosPermissions && 16 & s2.dosPermissions && (s2.dir = true), s2.dir && (e2 = g(e2)), s2.createFolders && (n2 = _(e2)) && b.call(this, n2, true);
          var a2 = "string" === i2 && false === s2.binary && false === s2.base64;
          r2 && void 0 !== r2.binary || (s2.binary = !a2), (t2 instanceof c && 0 === t2.uncompressedSize || s2.dir || !t2 || 0 === t2.length) && (s2.base64 = false, s2.binary = true, t2 = "", s2.compression = "STORE", i2 = "string");
          var o2 = null;
          o2 = t2 instanceof c || t2 instanceof l ? t2 : p.isNode && p.isStream(t2) ? new m(e2, t2) : u.prepareContent(e2, t2, s2.binary, s2.optimizedBinaryString, s2.base64);
          var h2 = new d(e2, o2, s2);
          this.files[e2] = h2;
        }
        var i = e("./utf8"), u = e("./utils"), l = e("./stream/GenericWorker"), a = e("./stream/StreamHelper"), f = e("./defaults"), c = e("./compressedObject"), d = e("./zipObject"), o = e("./generate"), p = e("./nodejsUtils"), m = e("./nodejs/NodejsStreamInputAdapter"), _ = function(e2) {
          "/" === e2.slice(-1) && (e2 = e2.substring(0, e2.length - 1));
          var t2 = e2.lastIndexOf("/");
          return 0 < t2 ? e2.substring(0, t2) : "";
        }, g = function(e2) {
          return "/" !== e2.slice(-1) && (e2 += "/"), e2;
        }, b = function(e2, t2) {
          return t2 = void 0 !== t2 ? t2 : f.createFolders, e2 = g(e2), this.files[e2] || s.call(this, e2, null, { dir: true, createFolders: t2 }), this.files[e2];
        };
        function h(e2) {
          return "[object RegExp]" === Object.prototype.toString.call(e2);
        }
        var n = { load: function() {
          throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
        }, forEach: function(e2) {
          var t2, r2, n2;
          for (t2 in this.files) n2 = this.files[t2], (r2 = t2.slice(this.root.length, t2.length)) && t2.slice(0, this.root.length) === this.root && e2(r2, n2);
        }, filter: function(r2) {
          var n2 = [];
          return this.forEach(function(e2, t2) {
            r2(e2, t2) && n2.push(t2);
          }), n2;
        }, file: function(e2, t2, r2) {
          if (1 !== arguments.length) return e2 = this.root + e2, s.call(this, e2, t2, r2), this;
          if (h(e2)) {
            var n2 = e2;
            return this.filter(function(e3, t3) {
              return !t3.dir && n2.test(e3);
            });
          }
          var i2 = this.files[this.root + e2];
          return i2 && !i2.dir ? i2 : null;
        }, folder: function(r2) {
          if (!r2) return this;
          if (h(r2)) return this.filter(function(e3, t3) {
            return t3.dir && r2.test(e3);
          });
          var e2 = this.root + r2, t2 = b.call(this, e2), n2 = this.clone();
          return n2.root = t2.name, n2;
        }, remove: function(r2) {
          r2 = this.root + r2;
          var e2 = this.files[r2];
          if (e2 || ("/" !== r2.slice(-1) && (r2 += "/"), e2 = this.files[r2]), e2 && !e2.dir) delete this.files[r2];
          else for (var t2 = this.filter(function(e3, t3) {
            return t3.name.slice(0, r2.length) === r2;
          }), n2 = 0; n2 < t2.length; n2++) delete this.files[t2[n2].name];
          return this;
        }, generate: function() {
          throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
        }, generateInternalStream: function(e2) {
          var t2, r2 = {};
          try {
            if ((r2 = u.extend(e2 || {}, { streamFiles: false, compression: "STORE", compressionOptions: null, type: "", platform: "DOS", comment: null, mimeType: "application/zip", encodeFileName: i.utf8encode })).type = r2.type.toLowerCase(), r2.compression = r2.compression.toUpperCase(), "binarystring" === r2.type && (r2.type = "string"), !r2.type) throw new Error("No output type specified.");
            u.checkSupport(r2.type), "darwin" !== r2.platform && "freebsd" !== r2.platform && "linux" !== r2.platform && "sunos" !== r2.platform || (r2.platform = "UNIX"), "win32" === r2.platform && (r2.platform = "DOS");
            var n2 = r2.comment || this.comment || "";
            t2 = o.generateWorker(this, r2, n2);
          } catch (e3) {
            (t2 = new l("error")).error(e3);
          }
          return new a(t2, r2.type || "string", r2.mimeType);
        }, generateAsync: function(e2, t2) {
          return this.generateInternalStream(e2).accumulate(t2);
        }, generateNodeStream: function(e2, t2) {
          return (e2 = e2 || {}).type || (e2.type = "nodebuffer"), this.generateInternalStream(e2).toNodejsStream(t2);
        } };
        t.exports = n;
      }, { "./compressedObject": 2, "./defaults": 5, "./generate": 9, "./nodejs/NodejsStreamInputAdapter": 12, "./nodejsUtils": 14, "./stream/GenericWorker": 28, "./stream/StreamHelper": 29, "./utf8": 31, "./utils": 32, "./zipObject": 35 }], 16: [function(e, t, r) {
        "use strict";
        t.exports = e("stream");
      }, { stream: void 0 }], 17: [function(e, t, r) {
        "use strict";
        var n = e("./DataReader");
        function i(e2) {
          n.call(this, e2);
          for (var t2 = 0; t2 < this.data.length; t2++) e2[t2] = 255 & e2[t2];
        }
        e("../utils").inherits(i, n), i.prototype.byteAt = function(e2) {
          return this.data[this.zero + e2];
        }, i.prototype.lastIndexOfSignature = function(e2) {
          for (var t2 = e2.charCodeAt(0), r2 = e2.charCodeAt(1), n2 = e2.charCodeAt(2), i2 = e2.charCodeAt(3), s = this.length - 4; 0 <= s; --s) if (this.data[s] === t2 && this.data[s + 1] === r2 && this.data[s + 2] === n2 && this.data[s + 3] === i2) return s - this.zero;
          return -1;
        }, i.prototype.readAndCheckSignature = function(e2) {
          var t2 = e2.charCodeAt(0), r2 = e2.charCodeAt(1), n2 = e2.charCodeAt(2), i2 = e2.charCodeAt(3), s = this.readData(4);
          return t2 === s[0] && r2 === s[1] && n2 === s[2] && i2 === s[3];
        }, i.prototype.readData = function(e2) {
          if (this.checkOffset(e2), 0 === e2) return [];
          var t2 = this.data.slice(this.zero + this.index, this.zero + this.index + e2);
          return this.index += e2, t2;
        }, t.exports = i;
      }, { "../utils": 32, "./DataReader": 18 }], 18: [function(e, t, r) {
        "use strict";
        var n = e("../utils");
        function i(e2) {
          this.data = e2, this.length = e2.length, this.index = 0, this.zero = 0;
        }
        i.prototype = { checkOffset: function(e2) {
          this.checkIndex(this.index + e2);
        }, checkIndex: function(e2) {
          if (this.length < this.zero + e2 || e2 < 0) throw new Error("End of data reached (data length = " + this.length + ", asked index = " + e2 + "). Corrupted zip ?");
        }, setIndex: function(e2) {
          this.checkIndex(e2), this.index = e2;
        }, skip: function(e2) {
          this.setIndex(this.index + e2);
        }, byteAt: function() {
        }, readInt: function(e2) {
          var t2, r2 = 0;
          for (this.checkOffset(e2), t2 = this.index + e2 - 1; t2 >= this.index; t2--) r2 = (r2 << 8) + this.byteAt(t2);
          return this.index += e2, r2;
        }, readString: function(e2) {
          return n.transformTo("string", this.readData(e2));
        }, readData: function() {
        }, lastIndexOfSignature: function() {
        }, readAndCheckSignature: function() {
        }, readDate: function() {
          var e2 = this.readInt(4);
          return new Date(Date.UTC(1980 + (e2 >> 25 & 127), (e2 >> 21 & 15) - 1, e2 >> 16 & 31, e2 >> 11 & 31, e2 >> 5 & 63, (31 & e2) << 1));
        } }, t.exports = i;
      }, { "../utils": 32 }], 19: [function(e, t, r) {
        "use strict";
        var n = e("./Uint8ArrayReader");
        function i(e2) {
          n.call(this, e2);
        }
        e("../utils").inherits(i, n), i.prototype.readData = function(e2) {
          this.checkOffset(e2);
          var t2 = this.data.slice(this.zero + this.index, this.zero + this.index + e2);
          return this.index += e2, t2;
        }, t.exports = i;
      }, { "../utils": 32, "./Uint8ArrayReader": 21 }], 20: [function(e, t, r) {
        "use strict";
        var n = e("./DataReader");
        function i(e2) {
          n.call(this, e2);
        }
        e("../utils").inherits(i, n), i.prototype.byteAt = function(e2) {
          return this.data.charCodeAt(this.zero + e2);
        }, i.prototype.lastIndexOfSignature = function(e2) {
          return this.data.lastIndexOf(e2) - this.zero;
        }, i.prototype.readAndCheckSignature = function(e2) {
          return e2 === this.readData(4);
        }, i.prototype.readData = function(e2) {
          this.checkOffset(e2);
          var t2 = this.data.slice(this.zero + this.index, this.zero + this.index + e2);
          return this.index += e2, t2;
        }, t.exports = i;
      }, { "../utils": 32, "./DataReader": 18 }], 21: [function(e, t, r) {
        "use strict";
        var n = e("./ArrayReader");
        function i(e2) {
          n.call(this, e2);
        }
        e("../utils").inherits(i, n), i.prototype.readData = function(e2) {
          if (this.checkOffset(e2), 0 === e2) return new Uint8Array(0);
          var t2 = this.data.subarray(this.zero + this.index, this.zero + this.index + e2);
          return this.index += e2, t2;
        }, t.exports = i;
      }, { "../utils": 32, "./ArrayReader": 17 }], 22: [function(e, t, r) {
        "use strict";
        var n = e("../utils"), i = e("../support"), s = e("./ArrayReader"), a = e("./StringReader"), o = e("./NodeBufferReader"), h = e("./Uint8ArrayReader");
        t.exports = function(e2) {
          var t2 = n.getTypeOf(e2);
          return n.checkSupport(t2), "string" !== t2 || i.uint8array ? "nodebuffer" === t2 ? new o(e2) : i.uint8array ? new h(n.transformTo("uint8array", e2)) : new s(n.transformTo("array", e2)) : new a(e2);
        };
      }, { "../support": 30, "../utils": 32, "./ArrayReader": 17, "./NodeBufferReader": 19, "./StringReader": 20, "./Uint8ArrayReader": 21 }], 23: [function(e, t, r) {
        "use strict";
        r.LOCAL_FILE_HEADER = "PK", r.CENTRAL_FILE_HEADER = "PK", r.CENTRAL_DIRECTORY_END = "PK", r.ZIP64_CENTRAL_DIRECTORY_LOCATOR = "PK\x07", r.ZIP64_CENTRAL_DIRECTORY_END = "PK", r.DATA_DESCRIPTOR = "PK\x07\b";
      }, {}], 24: [function(e, t, r) {
        "use strict";
        var n = e("./GenericWorker"), i = e("../utils");
        function s(e2) {
          n.call(this, "ConvertWorker to " + e2), this.destType = e2;
        }
        i.inherits(s, n), s.prototype.processChunk = function(e2) {
          this.push({ data: i.transformTo(this.destType, e2.data), meta: e2.meta });
        }, t.exports = s;
      }, { "../utils": 32, "./GenericWorker": 28 }], 25: [function(e, t, r) {
        "use strict";
        var n = e("./GenericWorker"), i = e("../crc32");
        function s() {
          n.call(this, "Crc32Probe"), this.withStreamInfo("crc32", 0);
        }
        e("../utils").inherits(s, n), s.prototype.processChunk = function(e2) {
          this.streamInfo.crc32 = i(e2.data, this.streamInfo.crc32 || 0), this.push(e2);
        }, t.exports = s;
      }, { "../crc32": 4, "../utils": 32, "./GenericWorker": 28 }], 26: [function(e, t, r) {
        "use strict";
        var n = e("../utils"), i = e("./GenericWorker");
        function s(e2) {
          i.call(this, "DataLengthProbe for " + e2), this.propName = e2, this.withStreamInfo(e2, 0);
        }
        n.inherits(s, i), s.prototype.processChunk = function(e2) {
          if (e2) {
            var t2 = this.streamInfo[this.propName] || 0;
            this.streamInfo[this.propName] = t2 + e2.data.length;
          }
          i.prototype.processChunk.call(this, e2);
        }, t.exports = s;
      }, { "../utils": 32, "./GenericWorker": 28 }], 27: [function(e, t, r) {
        "use strict";
        var n = e("../utils"), i = e("./GenericWorker");
        function s(e2) {
          i.call(this, "DataWorker");
          var t2 = this;
          this.dataIsReady = false, this.index = 0, this.max = 0, this.data = null, this.type = "", this._tickScheduled = false, e2.then(function(e3) {
            t2.dataIsReady = true, t2.data = e3, t2.max = e3 && e3.length || 0, t2.type = n.getTypeOf(e3), t2.isPaused || t2._tickAndRepeat();
          }, function(e3) {
            t2.error(e3);
          });
        }
        n.inherits(s, i), s.prototype.cleanUp = function() {
          i.prototype.cleanUp.call(this), this.data = null;
        }, s.prototype.resume = function() {
          return !!i.prototype.resume.call(this) && (!this._tickScheduled && this.dataIsReady && (this._tickScheduled = true, n.delay(this._tickAndRepeat, [], this)), true);
        }, s.prototype._tickAndRepeat = function() {
          this._tickScheduled = false, this.isPaused || this.isFinished || (this._tick(), this.isFinished || (n.delay(this._tickAndRepeat, [], this), this._tickScheduled = true));
        }, s.prototype._tick = function() {
          if (this.isPaused || this.isFinished) return false;
          var e2 = null, t2 = Math.min(this.max, this.index + 16384);
          if (this.index >= this.max) return this.end();
          switch (this.type) {
            case "string":
              e2 = this.data.substring(this.index, t2);
              break;
            case "uint8array":
              e2 = this.data.subarray(this.index, t2);
              break;
            case "array":
            case "nodebuffer":
              e2 = this.data.slice(this.index, t2);
          }
          return this.index = t2, this.push({ data: e2, meta: { percent: this.max ? this.index / this.max * 100 : 0 } });
        }, t.exports = s;
      }, { "../utils": 32, "./GenericWorker": 28 }], 28: [function(e, t, r) {
        "use strict";
        function n(e2) {
          this.name = e2 || "default", this.streamInfo = {}, this.generatedError = null, this.extraStreamInfo = {}, this.isPaused = true, this.isFinished = false, this.isLocked = false, this._listeners = { data: [], end: [], error: [] }, this.previous = null;
        }
        n.prototype = { push: function(e2) {
          this.emit("data", e2);
        }, end: function() {
          if (this.isFinished) return false;
          this.flush();
          try {
            this.emit("end"), this.cleanUp(), this.isFinished = true;
          } catch (e2) {
            this.emit("error", e2);
          }
          return true;
        }, error: function(e2) {
          return !this.isFinished && (this.isPaused ? this.generatedError = e2 : (this.isFinished = true, this.emit("error", e2), this.previous && this.previous.error(e2), this.cleanUp()), true);
        }, on: function(e2, t2) {
          return this._listeners[e2].push(t2), this;
        }, cleanUp: function() {
          this.streamInfo = this.generatedError = this.extraStreamInfo = null, this._listeners = [];
        }, emit: function(e2, t2) {
          if (this._listeners[e2]) for (var r2 = 0; r2 < this._listeners[e2].length; r2++) this._listeners[e2][r2].call(this, t2);
        }, pipe: function(e2) {
          return e2.registerPrevious(this);
        }, registerPrevious: function(e2) {
          if (this.isLocked) throw new Error("The stream '" + this + "' has already been used.");
          this.streamInfo = e2.streamInfo, this.mergeStreamInfo(), this.previous = e2;
          var t2 = this;
          return e2.on("data", function(e3) {
            t2.processChunk(e3);
          }), e2.on("end", function() {
            t2.end();
          }), e2.on("error", function(e3) {
            t2.error(e3);
          }), this;
        }, pause: function() {
          return !this.isPaused && !this.isFinished && (this.isPaused = true, this.previous && this.previous.pause(), true);
        }, resume: function() {
          if (!this.isPaused || this.isFinished) return false;
          var e2 = this.isPaused = false;
          return this.generatedError && (this.error(this.generatedError), e2 = true), this.previous && this.previous.resume(), !e2;
        }, flush: function() {
        }, processChunk: function(e2) {
          this.push(e2);
        }, withStreamInfo: function(e2, t2) {
          return this.extraStreamInfo[e2] = t2, this.mergeStreamInfo(), this;
        }, mergeStreamInfo: function() {
          for (var e2 in this.extraStreamInfo) Object.prototype.hasOwnProperty.call(this.extraStreamInfo, e2) && (this.streamInfo[e2] = this.extraStreamInfo[e2]);
        }, lock: function() {
          if (this.isLocked) throw new Error("The stream '" + this + "' has already been used.");
          this.isLocked = true, this.previous && this.previous.lock();
        }, toString: function() {
          var e2 = "Worker " + this.name;
          return this.previous ? this.previous + " -> " + e2 : e2;
        } }, t.exports = n;
      }, {}], 29: [function(e, t, r) {
        "use strict";
        var h = e("../utils"), i = e("./ConvertWorker"), s = e("./GenericWorker"), u = e("../base64"), n = e("../support"), a = e("../external"), o = null;
        if (n.nodestream) try {
          o = e("../nodejs/NodejsStreamOutputAdapter");
        } catch (e2) {
        }
        function l(e2, o2) {
          return new a.Promise(function(t2, r2) {
            var n2 = [], i2 = e2._internalType, s2 = e2._outputType, a2 = e2._mimeType;
            e2.on("data", function(e3, t3) {
              n2.push(e3), o2 && o2(t3);
            }).on("error", function(e3) {
              n2 = [], r2(e3);
            }).on("end", function() {
              try {
                var e3 = (function(e4, t3, r3) {
                  switch (e4) {
                    case "blob":
                      return h.newBlob(h.transformTo("arraybuffer", t3), r3);
                    case "base64":
                      return u.encode(t3);
                    default:
                      return h.transformTo(e4, t3);
                  }
                })(s2, (function(e4, t3) {
                  var r3, n3 = 0, i3 = null, s3 = 0;
                  for (r3 = 0; r3 < t3.length; r3++) s3 += t3[r3].length;
                  switch (e4) {
                    case "string":
                      return t3.join("");
                    case "array":
                      return Array.prototype.concat.apply([], t3);
                    case "uint8array":
                      for (i3 = new Uint8Array(s3), r3 = 0; r3 < t3.length; r3++) i3.set(t3[r3], n3), n3 += t3[r3].length;
                      return i3;
                    case "nodebuffer":
                      return Buffer.concat(t3);
                    default:
                      throw new Error("concat : unsupported type '" + e4 + "'");
                  }
                })(i2, n2), a2);
                t2(e3);
              } catch (e4) {
                r2(e4);
              }
              n2 = [];
            }).resume();
          });
        }
        function f(e2, t2, r2) {
          var n2 = t2;
          switch (t2) {
            case "blob":
            case "arraybuffer":
              n2 = "uint8array";
              break;
            case "base64":
              n2 = "string";
          }
          try {
            this._internalType = n2, this._outputType = t2, this._mimeType = r2, h.checkSupport(n2), this._worker = e2.pipe(new i(n2)), e2.lock();
          } catch (e3) {
            this._worker = new s("error"), this._worker.error(e3);
          }
        }
        f.prototype = { accumulate: function(e2) {
          return l(this, e2);
        }, on: function(e2, t2) {
          var r2 = this;
          return "data" === e2 ? this._worker.on(e2, function(e3) {
            t2.call(r2, e3.data, e3.meta);
          }) : this._worker.on(e2, function() {
            h.delay(t2, arguments, r2);
          }), this;
        }, resume: function() {
          return h.delay(this._worker.resume, [], this._worker), this;
        }, pause: function() {
          return this._worker.pause(), this;
        }, toNodejsStream: function(e2) {
          if (h.checkSupport("nodestream"), "nodebuffer" !== this._outputType) throw new Error(this._outputType + " is not supported by this method");
          return new o(this, { objectMode: "nodebuffer" !== this._outputType }, e2);
        } }, t.exports = f;
      }, { "../base64": 1, "../external": 6, "../nodejs/NodejsStreamOutputAdapter": 13, "../support": 30, "../utils": 32, "./ConvertWorker": 24, "./GenericWorker": 28 }], 30: [function(e, t, r) {
        "use strict";
        if (r.base64 = true, r.array = true, r.string = true, r.arraybuffer = "undefined" != typeof ArrayBuffer && "undefined" != typeof Uint8Array, r.nodebuffer = "undefined" != typeof Buffer, r.uint8array = "undefined" != typeof Uint8Array, "undefined" == typeof ArrayBuffer) r.blob = false;
        else {
          var n = new ArrayBuffer(0);
          try {
            r.blob = 0 === new Blob([n], { type: "application/zip" }).size;
          } catch (e2) {
            try {
              var i = new (self.BlobBuilder || self.WebKitBlobBuilder || self.MozBlobBuilder || self.MSBlobBuilder)();
              i.append(n), r.blob = 0 === i.getBlob("application/zip").size;
            } catch (e3) {
              r.blob = false;
            }
          }
        }
        try {
          r.nodestream = !!e("readable-stream").Readable;
        } catch (e2) {
          r.nodestream = false;
        }
      }, { "readable-stream": 16 }], 31: [function(e, t, s) {
        "use strict";
        for (var o = e("./utils"), h = e("./support"), r = e("./nodejsUtils"), n = e("./stream/GenericWorker"), u = new Array(256), i = 0; i < 256; i++) u[i] = 252 <= i ? 6 : 248 <= i ? 5 : 240 <= i ? 4 : 224 <= i ? 3 : 192 <= i ? 2 : 1;
        u[254] = u[254] = 1;
        function a() {
          n.call(this, "utf-8 decode"), this.leftOver = null;
        }
        function l() {
          n.call(this, "utf-8 encode");
        }
        s.utf8encode = function(e2) {
          return h.nodebuffer ? r.newBufferFrom(e2, "utf-8") : (function(e3) {
            var t2, r2, n2, i2, s2, a2 = e3.length, o2 = 0;
            for (i2 = 0; i2 < a2; i2++) 55296 == (64512 & (r2 = e3.charCodeAt(i2))) && i2 + 1 < a2 && 56320 == (64512 & (n2 = e3.charCodeAt(i2 + 1))) && (r2 = 65536 + (r2 - 55296 << 10) + (n2 - 56320), i2++), o2 += r2 < 128 ? 1 : r2 < 2048 ? 2 : r2 < 65536 ? 3 : 4;
            for (t2 = h.uint8array ? new Uint8Array(o2) : new Array(o2), i2 = s2 = 0; s2 < o2; i2++) 55296 == (64512 & (r2 = e3.charCodeAt(i2))) && i2 + 1 < a2 && 56320 == (64512 & (n2 = e3.charCodeAt(i2 + 1))) && (r2 = 65536 + (r2 - 55296 << 10) + (n2 - 56320), i2++), r2 < 128 ? t2[s2++] = r2 : (r2 < 2048 ? t2[s2++] = 192 | r2 >>> 6 : (r2 < 65536 ? t2[s2++] = 224 | r2 >>> 12 : (t2[s2++] = 240 | r2 >>> 18, t2[s2++] = 128 | r2 >>> 12 & 63), t2[s2++] = 128 | r2 >>> 6 & 63), t2[s2++] = 128 | 63 & r2);
            return t2;
          })(e2);
        }, s.utf8decode = function(e2) {
          return h.nodebuffer ? o.transformTo("nodebuffer", e2).toString("utf-8") : (function(e3) {
            var t2, r2, n2, i2, s2 = e3.length, a2 = new Array(2 * s2);
            for (t2 = r2 = 0; t2 < s2; ) if ((n2 = e3[t2++]) < 128) a2[r2++] = n2;
            else if (4 < (i2 = u[n2])) a2[r2++] = 65533, t2 += i2 - 1;
            else {
              for (n2 &= 2 === i2 ? 31 : 3 === i2 ? 15 : 7; 1 < i2 && t2 < s2; ) n2 = n2 << 6 | 63 & e3[t2++], i2--;
              1 < i2 ? a2[r2++] = 65533 : n2 < 65536 ? a2[r2++] = n2 : (n2 -= 65536, a2[r2++] = 55296 | n2 >> 10 & 1023, a2[r2++] = 56320 | 1023 & n2);
            }
            return a2.length !== r2 && (a2.subarray ? a2 = a2.subarray(0, r2) : a2.length = r2), o.applyFromCharCode(a2);
          })(e2 = o.transformTo(h.uint8array ? "uint8array" : "array", e2));
        }, o.inherits(a, n), a.prototype.processChunk = function(e2) {
          var t2 = o.transformTo(h.uint8array ? "uint8array" : "array", e2.data);
          if (this.leftOver && this.leftOver.length) {
            if (h.uint8array) {
              var r2 = t2;
              (t2 = new Uint8Array(r2.length + this.leftOver.length)).set(this.leftOver, 0), t2.set(r2, this.leftOver.length);
            } else t2 = this.leftOver.concat(t2);
            this.leftOver = null;
          }
          var n2 = (function(e3, t3) {
            var r3;
            for ((t3 = t3 || e3.length) > e3.length && (t3 = e3.length), r3 = t3 - 1; 0 <= r3 && 128 == (192 & e3[r3]); ) r3--;
            return r3 < 0 ? t3 : 0 === r3 ? t3 : r3 + u[e3[r3]] > t3 ? r3 : t3;
          })(t2), i2 = t2;
          n2 !== t2.length && (h.uint8array ? (i2 = t2.subarray(0, n2), this.leftOver = t2.subarray(n2, t2.length)) : (i2 = t2.slice(0, n2), this.leftOver = t2.slice(n2, t2.length))), this.push({ data: s.utf8decode(i2), meta: e2.meta });
        }, a.prototype.flush = function() {
          this.leftOver && this.leftOver.length && (this.push({ data: s.utf8decode(this.leftOver), meta: {} }), this.leftOver = null);
        }, s.Utf8DecodeWorker = a, o.inherits(l, n), l.prototype.processChunk = function(e2) {
          this.push({ data: s.utf8encode(e2.data), meta: e2.meta });
        }, s.Utf8EncodeWorker = l;
      }, { "./nodejsUtils": 14, "./stream/GenericWorker": 28, "./support": 30, "./utils": 32 }], 32: [function(e, t, a) {
        "use strict";
        var o = e("./support"), h = e("./base64"), r = e("./nodejsUtils"), u = e("./external");
        function n(e2) {
          return e2;
        }
        function l(e2, t2) {
          for (var r2 = 0; r2 < e2.length; ++r2) t2[r2] = 255 & e2.charCodeAt(r2);
          return t2;
        }
        e("setimmediate"), a.newBlob = function(t2, r2) {
          a.checkSupport("blob");
          try {
            return new Blob([t2], { type: r2 });
          } catch (e2) {
            try {
              var n2 = new (self.BlobBuilder || self.WebKitBlobBuilder || self.MozBlobBuilder || self.MSBlobBuilder)();
              return n2.append(t2), n2.getBlob(r2);
            } catch (e3) {
              throw new Error("Bug : can't construct the Blob.");
            }
          }
        };
        var i = { stringifyByChunk: function(e2, t2, r2) {
          var n2 = [], i2 = 0, s2 = e2.length;
          if (s2 <= r2) return String.fromCharCode.apply(null, e2);
          for (; i2 < s2; ) "array" === t2 || "nodebuffer" === t2 ? n2.push(String.fromCharCode.apply(null, e2.slice(i2, Math.min(i2 + r2, s2)))) : n2.push(String.fromCharCode.apply(null, e2.subarray(i2, Math.min(i2 + r2, s2)))), i2 += r2;
          return n2.join("");
        }, stringifyByChar: function(e2) {
          for (var t2 = "", r2 = 0; r2 < e2.length; r2++) t2 += String.fromCharCode(e2[r2]);
          return t2;
        }, applyCanBeUsed: { uint8array: (function() {
          try {
            return o.uint8array && 1 === String.fromCharCode.apply(null, new Uint8Array(1)).length;
          } catch (e2) {
            return false;
          }
        })(), nodebuffer: (function() {
          try {
            return o.nodebuffer && 1 === String.fromCharCode.apply(null, r.allocBuffer(1)).length;
          } catch (e2) {
            return false;
          }
        })() } };
        function s(e2) {
          var t2 = 65536, r2 = a.getTypeOf(e2), n2 = true;
          if ("uint8array" === r2 ? n2 = i.applyCanBeUsed.uint8array : "nodebuffer" === r2 && (n2 = i.applyCanBeUsed.nodebuffer), n2) for (; 1 < t2; ) try {
            return i.stringifyByChunk(e2, r2, t2);
          } catch (e3) {
            t2 = Math.floor(t2 / 2);
          }
          return i.stringifyByChar(e2);
        }
        function f(e2, t2) {
          for (var r2 = 0; r2 < e2.length; r2++) t2[r2] = e2[r2];
          return t2;
        }
        a.applyFromCharCode = s;
        var c = {};
        c.string = { string: n, array: function(e2) {
          return l(e2, new Array(e2.length));
        }, arraybuffer: function(e2) {
          return c.string.uint8array(e2).buffer;
        }, uint8array: function(e2) {
          return l(e2, new Uint8Array(e2.length));
        }, nodebuffer: function(e2) {
          return l(e2, r.allocBuffer(e2.length));
        } }, c.array = { string: s, array: n, arraybuffer: function(e2) {
          return new Uint8Array(e2).buffer;
        }, uint8array: function(e2) {
          return new Uint8Array(e2);
        }, nodebuffer: function(e2) {
          return r.newBufferFrom(e2);
        } }, c.arraybuffer = { string: function(e2) {
          return s(new Uint8Array(e2));
        }, array: function(e2) {
          return f(new Uint8Array(e2), new Array(e2.byteLength));
        }, arraybuffer: n, uint8array: function(e2) {
          return new Uint8Array(e2);
        }, nodebuffer: function(e2) {
          return r.newBufferFrom(new Uint8Array(e2));
        } }, c.uint8array = { string: s, array: function(e2) {
          return f(e2, new Array(e2.length));
        }, arraybuffer: function(e2) {
          return e2.buffer;
        }, uint8array: n, nodebuffer: function(e2) {
          return r.newBufferFrom(e2);
        } }, c.nodebuffer = { string: s, array: function(e2) {
          return f(e2, new Array(e2.length));
        }, arraybuffer: function(e2) {
          return c.nodebuffer.uint8array(e2).buffer;
        }, uint8array: function(e2) {
          return f(e2, new Uint8Array(e2.length));
        }, nodebuffer: n }, a.transformTo = function(e2, t2) {
          if (t2 = t2 || "", !e2) return t2;
          a.checkSupport(e2);
          var r2 = a.getTypeOf(t2);
          return c[r2][e2](t2);
        }, a.resolve = function(e2) {
          for (var t2 = e2.split("/"), r2 = [], n2 = 0; n2 < t2.length; n2++) {
            var i2 = t2[n2];
            "." === i2 || "" === i2 && 0 !== n2 && n2 !== t2.length - 1 || (".." === i2 ? r2.pop() : r2.push(i2));
          }
          return r2.join("/");
        }, a.getTypeOf = function(e2) {
          return "string" == typeof e2 ? "string" : "[object Array]" === Object.prototype.toString.call(e2) ? "array" : o.nodebuffer && r.isBuffer(e2) ? "nodebuffer" : o.uint8array && e2 instanceof Uint8Array ? "uint8array" : o.arraybuffer && e2 instanceof ArrayBuffer ? "arraybuffer" : void 0;
        }, a.checkSupport = function(e2) {
          if (!o[e2.toLowerCase()]) throw new Error(e2 + " is not supported by this platform");
        }, a.MAX_VALUE_16BITS = 65535, a.MAX_VALUE_32BITS = -1, a.pretty = function(e2) {
          var t2, r2, n2 = "";
          for (r2 = 0; r2 < (e2 || "").length; r2++) n2 += "\\x" + ((t2 = e2.charCodeAt(r2)) < 16 ? "0" : "") + t2.toString(16).toUpperCase();
          return n2;
        }, a.delay = function(e2, t2, r2) {
          setImmediate(function() {
            e2.apply(r2 || null, t2 || []);
          });
        }, a.inherits = function(e2, t2) {
          function r2() {
          }
          r2.prototype = t2.prototype, e2.prototype = new r2();
        }, a.extend = function() {
          var e2, t2, r2 = {};
          for (e2 = 0; e2 < arguments.length; e2++) for (t2 in arguments[e2]) Object.prototype.hasOwnProperty.call(arguments[e2], t2) && void 0 === r2[t2] && (r2[t2] = arguments[e2][t2]);
          return r2;
        }, a.prepareContent = function(r2, e2, n2, i2, s2) {
          return u.Promise.resolve(e2).then(function(n3) {
            return o.blob && (n3 instanceof Blob || -1 !== ["[object File]", "[object Blob]"].indexOf(Object.prototype.toString.call(n3))) && "undefined" != typeof FileReader ? new u.Promise(function(t2, r3) {
              var e3 = new FileReader();
              e3.onload = function(e4) {
                t2(e4.target.result);
              }, e3.onerror = function(e4) {
                r3(e4.target.error);
              }, e3.readAsArrayBuffer(n3);
            }) : n3;
          }).then(function(e3) {
            var t2 = a.getTypeOf(e3);
            return t2 ? ("arraybuffer" === t2 ? e3 = a.transformTo("uint8array", e3) : "string" === t2 && (s2 ? e3 = h.decode(e3) : n2 && true !== i2 && (e3 = (function(e4) {
              return l(e4, o.uint8array ? new Uint8Array(e4.length) : new Array(e4.length));
            })(e3))), e3) : u.Promise.reject(new Error("Can't read the data of '" + r2 + "'. Is it in a supported JavaScript type (String, Blob, ArrayBuffer, etc) ?"));
          });
        };
      }, { "./base64": 1, "./external": 6, "./nodejsUtils": 14, "./support": 30, setimmediate: 54 }], 33: [function(e, t, r) {
        "use strict";
        var n = e("./reader/readerFor"), i = e("./utils"), s = e("./signature"), a = e("./zipEntry"), o = e("./support");
        function h(e2) {
          this.files = [], this.loadOptions = e2;
        }
        h.prototype = { checkSignature: function(e2) {
          if (!this.reader.readAndCheckSignature(e2)) {
            this.reader.index -= 4;
            var t2 = this.reader.readString(4);
            throw new Error("Corrupted zip or bug: unexpected signature (" + i.pretty(t2) + ", expected " + i.pretty(e2) + ")");
          }
        }, isSignature: function(e2, t2) {
          var r2 = this.reader.index;
          this.reader.setIndex(e2);
          var n2 = this.reader.readString(4) === t2;
          return this.reader.setIndex(r2), n2;
        }, readBlockEndOfCentral: function() {
          this.diskNumber = this.reader.readInt(2), this.diskWithCentralDirStart = this.reader.readInt(2), this.centralDirRecordsOnThisDisk = this.reader.readInt(2), this.centralDirRecords = this.reader.readInt(2), this.centralDirSize = this.reader.readInt(4), this.centralDirOffset = this.reader.readInt(4), this.zipCommentLength = this.reader.readInt(2);
          var e2 = this.reader.readData(this.zipCommentLength), t2 = o.uint8array ? "uint8array" : "array", r2 = i.transformTo(t2, e2);
          this.zipComment = this.loadOptions.decodeFileName(r2);
        }, readBlockZip64EndOfCentral: function() {
          this.zip64EndOfCentralSize = this.reader.readInt(8), this.reader.skip(4), this.diskNumber = this.reader.readInt(4), this.diskWithCentralDirStart = this.reader.readInt(4), this.centralDirRecordsOnThisDisk = this.reader.readInt(8), this.centralDirRecords = this.reader.readInt(8), this.centralDirSize = this.reader.readInt(8), this.centralDirOffset = this.reader.readInt(8), this.zip64ExtensibleData = {};
          for (var e2, t2, r2, n2 = this.zip64EndOfCentralSize - 44; 0 < n2; ) e2 = this.reader.readInt(2), t2 = this.reader.readInt(4), r2 = this.reader.readData(t2), this.zip64ExtensibleData[e2] = { id: e2, length: t2, value: r2 };
        }, readBlockZip64EndOfCentralLocator: function() {
          if (this.diskWithZip64CentralDirStart = this.reader.readInt(4), this.relativeOffsetEndOfZip64CentralDir = this.reader.readInt(8), this.disksCount = this.reader.readInt(4), 1 < this.disksCount) throw new Error("Multi-volumes zip are not supported");
        }, readLocalFiles: function() {
          var e2, t2;
          for (e2 = 0; e2 < this.files.length; e2++) t2 = this.files[e2], this.reader.setIndex(t2.localHeaderOffset), this.checkSignature(s.LOCAL_FILE_HEADER), t2.readLocalPart(this.reader), t2.handleUTF8(), t2.processAttributes();
        }, readCentralDir: function() {
          var e2;
          for (this.reader.setIndex(this.centralDirOffset); this.reader.readAndCheckSignature(s.CENTRAL_FILE_HEADER); ) (e2 = new a({ zip64: this.zip64 }, this.loadOptions)).readCentralPart(this.reader), this.files.push(e2);
          if (this.centralDirRecords !== this.files.length && 0 !== this.centralDirRecords && 0 === this.files.length) throw new Error("Corrupted zip or bug: expected " + this.centralDirRecords + " records in central dir, got " + this.files.length);
        }, readEndOfCentral: function() {
          var e2 = this.reader.lastIndexOfSignature(s.CENTRAL_DIRECTORY_END);
          if (e2 < 0) throw !this.isSignature(0, s.LOCAL_FILE_HEADER) ? new Error("Can't find end of central directory : is this a zip file ? If it is, see https://stuk.github.io/jszip/documentation/howto/read_zip.html") : new Error("Corrupted zip: can't find end of central directory");
          this.reader.setIndex(e2);
          var t2 = e2;
          if (this.checkSignature(s.CENTRAL_DIRECTORY_END), this.readBlockEndOfCentral(), this.diskNumber === i.MAX_VALUE_16BITS || this.diskWithCentralDirStart === i.MAX_VALUE_16BITS || this.centralDirRecordsOnThisDisk === i.MAX_VALUE_16BITS || this.centralDirRecords === i.MAX_VALUE_16BITS || this.centralDirSize === i.MAX_VALUE_32BITS || this.centralDirOffset === i.MAX_VALUE_32BITS) {
            if (this.zip64 = true, (e2 = this.reader.lastIndexOfSignature(s.ZIP64_CENTRAL_DIRECTORY_LOCATOR)) < 0) throw new Error("Corrupted zip: can't find the ZIP64 end of central directory locator");
            if (this.reader.setIndex(e2), this.checkSignature(s.ZIP64_CENTRAL_DIRECTORY_LOCATOR), this.readBlockZip64EndOfCentralLocator(), !this.isSignature(this.relativeOffsetEndOfZip64CentralDir, s.ZIP64_CENTRAL_DIRECTORY_END) && (this.relativeOffsetEndOfZip64CentralDir = this.reader.lastIndexOfSignature(s.ZIP64_CENTRAL_DIRECTORY_END), this.relativeOffsetEndOfZip64CentralDir < 0)) throw new Error("Corrupted zip: can't find the ZIP64 end of central directory");
            this.reader.setIndex(this.relativeOffsetEndOfZip64CentralDir), this.checkSignature(s.ZIP64_CENTRAL_DIRECTORY_END), this.readBlockZip64EndOfCentral();
          }
          var r2 = this.centralDirOffset + this.centralDirSize;
          this.zip64 && (r2 += 20, r2 += 12 + this.zip64EndOfCentralSize);
          var n2 = t2 - r2;
          if (0 < n2) this.isSignature(t2, s.CENTRAL_FILE_HEADER) || (this.reader.zero = n2);
          else if (n2 < 0) throw new Error("Corrupted zip: missing " + Math.abs(n2) + " bytes.");
        }, prepareReader: function(e2) {
          this.reader = n(e2);
        }, load: function(e2) {
          this.prepareReader(e2), this.readEndOfCentral(), this.readCentralDir(), this.readLocalFiles();
        } }, t.exports = h;
      }, { "./reader/readerFor": 22, "./signature": 23, "./support": 30, "./utils": 32, "./zipEntry": 34 }], 34: [function(e, t, r) {
        "use strict";
        var n = e("./reader/readerFor"), s = e("./utils"), i = e("./compressedObject"), a = e("./crc32"), o = e("./utf8"), h = e("./compressions"), u = e("./support");
        function l(e2, t2) {
          this.options = e2, this.loadOptions = t2;
        }
        l.prototype = { isEncrypted: function() {
          return 1 == (1 & this.bitFlag);
        }, useUTF8: function() {
          return 2048 == (2048 & this.bitFlag);
        }, readLocalPart: function(e2) {
          var t2, r2;
          if (e2.skip(22), this.fileNameLength = e2.readInt(2), r2 = e2.readInt(2), this.fileName = e2.readData(this.fileNameLength), e2.skip(r2), -1 === this.compressedSize || -1 === this.uncompressedSize) throw new Error("Bug or corrupted zip : didn't get enough information from the central directory (compressedSize === -1 || uncompressedSize === -1)");
          if (null === (t2 = (function(e3) {
            for (var t3 in h) if (Object.prototype.hasOwnProperty.call(h, t3) && h[t3].magic === e3) return h[t3];
            return null;
          })(this.compressionMethod))) throw new Error("Corrupted zip : compression " + s.pretty(this.compressionMethod) + " unknown (inner file : " + s.transformTo("string", this.fileName) + ")");
          this.decompressed = new i(this.compressedSize, this.uncompressedSize, this.crc32, t2, e2.readData(this.compressedSize));
        }, readCentralPart: function(e2) {
          this.versionMadeBy = e2.readInt(2), e2.skip(2), this.bitFlag = e2.readInt(2), this.compressionMethod = e2.readString(2), this.date = e2.readDate(), this.crc32 = e2.readInt(4), this.compressedSize = e2.readInt(4), this.uncompressedSize = e2.readInt(4);
          var t2 = e2.readInt(2);
          if (this.extraFieldsLength = e2.readInt(2), this.fileCommentLength = e2.readInt(2), this.diskNumberStart = e2.readInt(2), this.internalFileAttributes = e2.readInt(2), this.externalFileAttributes = e2.readInt(4), this.localHeaderOffset = e2.readInt(4), this.isEncrypted()) throw new Error("Encrypted zip are not supported");
          e2.skip(t2), this.readExtraFields(e2), this.parseZIP64ExtraField(e2), this.fileComment = e2.readData(this.fileCommentLength);
        }, processAttributes: function() {
          this.unixPermissions = null, this.dosPermissions = null;
          var e2 = this.versionMadeBy >> 8;
          this.dir = !!(16 & this.externalFileAttributes), 0 == e2 && (this.dosPermissions = 63 & this.externalFileAttributes), 3 == e2 && (this.unixPermissions = this.externalFileAttributes >> 16 & 65535), this.dir || "/" !== this.fileNameStr.slice(-1) || (this.dir = true);
        }, parseZIP64ExtraField: function() {
          if (this.extraFields[1]) {
            var e2 = n(this.extraFields[1].value);
            this.uncompressedSize === s.MAX_VALUE_32BITS && (this.uncompressedSize = e2.readInt(8)), this.compressedSize === s.MAX_VALUE_32BITS && (this.compressedSize = e2.readInt(8)), this.localHeaderOffset === s.MAX_VALUE_32BITS && (this.localHeaderOffset = e2.readInt(8)), this.diskNumberStart === s.MAX_VALUE_32BITS && (this.diskNumberStart = e2.readInt(4));
          }
        }, readExtraFields: function(e2) {
          var t2, r2, n2, i2 = e2.index + this.extraFieldsLength;
          for (this.extraFields || (this.extraFields = {}); e2.index + 4 < i2; ) t2 = e2.readInt(2), r2 = e2.readInt(2), n2 = e2.readData(r2), this.extraFields[t2] = { id: t2, length: r2, value: n2 };
          e2.setIndex(i2);
        }, handleUTF8: function() {
          var e2 = u.uint8array ? "uint8array" : "array";
          if (this.useUTF8()) this.fileNameStr = o.utf8decode(this.fileName), this.fileCommentStr = o.utf8decode(this.fileComment);
          else {
            var t2 = this.findExtraFieldUnicodePath();
            if (null !== t2) this.fileNameStr = t2;
            else {
              var r2 = s.transformTo(e2, this.fileName);
              this.fileNameStr = this.loadOptions.decodeFileName(r2);
            }
            var n2 = this.findExtraFieldUnicodeComment();
            if (null !== n2) this.fileCommentStr = n2;
            else {
              var i2 = s.transformTo(e2, this.fileComment);
              this.fileCommentStr = this.loadOptions.decodeFileName(i2);
            }
          }
        }, findExtraFieldUnicodePath: function() {
          var e2 = this.extraFields[28789];
          if (e2) {
            var t2 = n(e2.value);
            return 1 !== t2.readInt(1) ? null : a(this.fileName) !== t2.readInt(4) ? null : o.utf8decode(t2.readData(e2.length - 5));
          }
          return null;
        }, findExtraFieldUnicodeComment: function() {
          var e2 = this.extraFields[25461];
          if (e2) {
            var t2 = n(e2.value);
            return 1 !== t2.readInt(1) ? null : a(this.fileComment) !== t2.readInt(4) ? null : o.utf8decode(t2.readData(e2.length - 5));
          }
          return null;
        } }, t.exports = l;
      }, { "./compressedObject": 2, "./compressions": 3, "./crc32": 4, "./reader/readerFor": 22, "./support": 30, "./utf8": 31, "./utils": 32 }], 35: [function(e, t, r) {
        "use strict";
        function n(e2, t2, r2) {
          this.name = e2, this.dir = r2.dir, this.date = r2.date, this.comment = r2.comment, this.unixPermissions = r2.unixPermissions, this.dosPermissions = r2.dosPermissions, this._data = t2, this._dataBinary = r2.binary, this.options = { compression: r2.compression, compressionOptions: r2.compressionOptions };
        }
        var s = e("./stream/StreamHelper"), i = e("./stream/DataWorker"), a = e("./utf8"), o = e("./compressedObject"), h = e("./stream/GenericWorker");
        n.prototype = { internalStream: function(e2) {
          var t2 = null, r2 = "string";
          try {
            if (!e2) throw new Error("No output type specified.");
            var n2 = "string" === (r2 = e2.toLowerCase()) || "text" === r2;
            "binarystring" !== r2 && "text" !== r2 || (r2 = "string"), t2 = this._decompressWorker();
            var i2 = !this._dataBinary;
            i2 && !n2 && (t2 = t2.pipe(new a.Utf8EncodeWorker())), !i2 && n2 && (t2 = t2.pipe(new a.Utf8DecodeWorker()));
          } catch (e3) {
            (t2 = new h("error")).error(e3);
          }
          return new s(t2, r2, "");
        }, async: function(e2, t2) {
          return this.internalStream(e2).accumulate(t2);
        }, nodeStream: function(e2, t2) {
          return this.internalStream(e2 || "nodebuffer").toNodejsStream(t2);
        }, _compressWorker: function(e2, t2) {
          if (this._data instanceof o && this._data.compression.magic === e2.magic) return this._data.getCompressedWorker();
          var r2 = this._decompressWorker();
          return this._dataBinary || (r2 = r2.pipe(new a.Utf8EncodeWorker())), o.createWorkerFrom(r2, e2, t2);
        }, _decompressWorker: function() {
          return this._data instanceof o ? this._data.getContentWorker() : this._data instanceof h ? this._data : new i(this._data);
        } };
        for (var u = ["asText", "asBinary", "asNodeBuffer", "asUint8Array", "asArrayBuffer"], l = function() {
          throw new Error("This method has been removed in JSZip 3.0, please check the upgrade guide.");
        }, f = 0; f < u.length; f++) n.prototype[u[f]] = l;
        t.exports = n;
      }, { "./compressedObject": 2, "./stream/DataWorker": 27, "./stream/GenericWorker": 28, "./stream/StreamHelper": 29, "./utf8": 31 }], 36: [function(e, l, t) {
        (function(t2) {
          "use strict";
          var r, n, e2 = t2.MutationObserver || t2.WebKitMutationObserver;
          if (e2) {
            var i = 0, s = new e2(u), a = t2.document.createTextNode("");
            s.observe(a, { characterData: true }), r = function() {
              a.data = i = ++i % 2;
            };
          } else if (t2.setImmediate || void 0 === t2.MessageChannel) r = "document" in t2 && "onreadystatechange" in t2.document.createElement("script") ? function() {
            var e3 = t2.document.createElement("script");
            e3.onreadystatechange = function() {
              u(), e3.onreadystatechange = null, e3.parentNode.removeChild(e3), e3 = null;
            }, t2.document.documentElement.appendChild(e3);
          } : function() {
            setTimeout(u, 0);
          };
          else {
            var o = new t2.MessageChannel();
            o.port1.onmessage = u, r = function() {
              o.port2.postMessage(0);
            };
          }
          var h = [];
          function u() {
            var e3, t3;
            n = true;
            for (var r2 = h.length; r2; ) {
              for (t3 = h, h = [], e3 = -1; ++e3 < r2; ) t3[e3]();
              r2 = h.length;
            }
            n = false;
          }
          l.exports = function(e3) {
            1 !== h.push(e3) || n || r();
          };
        }).call(this, "undefined" != typeof global ? global : "undefined" != typeof self ? self : "undefined" != typeof window ? window : {});
      }, {}], 37: [function(e, t, r) {
        "use strict";
        var i = e("immediate");
        function u() {
        }
        var l = {}, s = ["REJECTED"], a = ["FULFILLED"], n = ["PENDING"];
        function o(e2) {
          if ("function" != typeof e2) throw new TypeError("resolver must be a function");
          this.state = n, this.queue = [], this.outcome = void 0, e2 !== u && d(this, e2);
        }
        function h(e2, t2, r2) {
          this.promise = e2, "function" == typeof t2 && (this.onFulfilled = t2, this.callFulfilled = this.otherCallFulfilled), "function" == typeof r2 && (this.onRejected = r2, this.callRejected = this.otherCallRejected);
        }
        function f(t2, r2, n2) {
          i(function() {
            var e2;
            try {
              e2 = r2(n2);
            } catch (e3) {
              return l.reject(t2, e3);
            }
            e2 === t2 ? l.reject(t2, new TypeError("Cannot resolve promise with itself")) : l.resolve(t2, e2);
          });
        }
        function c(e2) {
          var t2 = e2 && e2.then;
          if (e2 && ("object" == typeof e2 || "function" == typeof e2) && "function" == typeof t2) return function() {
            t2.apply(e2, arguments);
          };
        }
        function d(t2, e2) {
          var r2 = false;
          function n2(e3) {
            r2 || (r2 = true, l.reject(t2, e3));
          }
          function i2(e3) {
            r2 || (r2 = true, l.resolve(t2, e3));
          }
          var s2 = p(function() {
            e2(i2, n2);
          });
          "error" === s2.status && n2(s2.value);
        }
        function p(e2, t2) {
          var r2 = {};
          try {
            r2.value = e2(t2), r2.status = "success";
          } catch (e3) {
            r2.status = "error", r2.value = e3;
          }
          return r2;
        }
        (t.exports = o).prototype.finally = function(t2) {
          if ("function" != typeof t2) return this;
          var r2 = this.constructor;
          return this.then(function(e2) {
            return r2.resolve(t2()).then(function() {
              return e2;
            });
          }, function(e2) {
            return r2.resolve(t2()).then(function() {
              throw e2;
            });
          });
        }, o.prototype.catch = function(e2) {
          return this.then(null, e2);
        }, o.prototype.then = function(e2, t2) {
          if ("function" != typeof e2 && this.state === a || "function" != typeof t2 && this.state === s) return this;
          var r2 = new this.constructor(u);
          this.state !== n ? f(r2, this.state === a ? e2 : t2, this.outcome) : this.queue.push(new h(r2, e2, t2));
          return r2;
        }, h.prototype.callFulfilled = function(e2) {
          l.resolve(this.promise, e2);
        }, h.prototype.otherCallFulfilled = function(e2) {
          f(this.promise, this.onFulfilled, e2);
        }, h.prototype.callRejected = function(e2) {
          l.reject(this.promise, e2);
        }, h.prototype.otherCallRejected = function(e2) {
          f(this.promise, this.onRejected, e2);
        }, l.resolve = function(e2, t2) {
          var r2 = p(c, t2);
          if ("error" === r2.status) return l.reject(e2, r2.value);
          var n2 = r2.value;
          if (n2) d(e2, n2);
          else {
            e2.state = a, e2.outcome = t2;
            for (var i2 = -1, s2 = e2.queue.length; ++i2 < s2; ) e2.queue[i2].callFulfilled(t2);
          }
          return e2;
        }, l.reject = function(e2, t2) {
          e2.state = s, e2.outcome = t2;
          for (var r2 = -1, n2 = e2.queue.length; ++r2 < n2; ) e2.queue[r2].callRejected(t2);
          return e2;
        }, o.resolve = function(e2) {
          if (e2 instanceof this) return e2;
          return l.resolve(new this(u), e2);
        }, o.reject = function(e2) {
          var t2 = new this(u);
          return l.reject(t2, e2);
        }, o.all = function(e2) {
          var r2 = this;
          if ("[object Array]" !== Object.prototype.toString.call(e2)) return this.reject(new TypeError("must be an array"));
          var n2 = e2.length, i2 = false;
          if (!n2) return this.resolve([]);
          var s2 = new Array(n2), a2 = 0, t2 = -1, o2 = new this(u);
          for (; ++t2 < n2; ) h2(e2[t2], t2);
          return o2;
          function h2(e3, t3) {
            r2.resolve(e3).then(function(e4) {
              s2[t3] = e4, ++a2 !== n2 || i2 || (i2 = true, l.resolve(o2, s2));
            }, function(e4) {
              i2 || (i2 = true, l.reject(o2, e4));
            });
          }
        }, o.race = function(e2) {
          var t2 = this;
          if ("[object Array]" !== Object.prototype.toString.call(e2)) return this.reject(new TypeError("must be an array"));
          var r2 = e2.length, n2 = false;
          if (!r2) return this.resolve([]);
          var i2 = -1, s2 = new this(u);
          for (; ++i2 < r2; ) a2 = e2[i2], t2.resolve(a2).then(function(e3) {
            n2 || (n2 = true, l.resolve(s2, e3));
          }, function(e3) {
            n2 || (n2 = true, l.reject(s2, e3));
          });
          var a2;
          return s2;
        };
      }, { immediate: 36 }], 38: [function(e, t, r) {
        "use strict";
        var n = {};
        (0, e("./lib/utils/common").assign)(n, e("./lib/deflate"), e("./lib/inflate"), e("./lib/zlib/constants")), t.exports = n;
      }, { "./lib/deflate": 39, "./lib/inflate": 40, "./lib/utils/common": 41, "./lib/zlib/constants": 44 }], 39: [function(e, t, r) {
        "use strict";
        var a = e("./zlib/deflate"), o = e("./utils/common"), h = e("./utils/strings"), i = e("./zlib/messages"), s = e("./zlib/zstream"), u = Object.prototype.toString, l = 0, f = -1, c = 0, d = 8;
        function p(e2) {
          if (!(this instanceof p)) return new p(e2);
          this.options = o.assign({ level: f, method: d, chunkSize: 16384, windowBits: 15, memLevel: 8, strategy: c, to: "" }, e2 || {});
          var t2 = this.options;
          t2.raw && 0 < t2.windowBits ? t2.windowBits = -t2.windowBits : t2.gzip && 0 < t2.windowBits && t2.windowBits < 16 && (t2.windowBits += 16), this.err = 0, this.msg = "", this.ended = false, this.chunks = [], this.strm = new s(), this.strm.avail_out = 0;
          var r2 = a.deflateInit2(this.strm, t2.level, t2.method, t2.windowBits, t2.memLevel, t2.strategy);
          if (r2 !== l) throw new Error(i[r2]);
          if (t2.header && a.deflateSetHeader(this.strm, t2.header), t2.dictionary) {
            var n2;
            if (n2 = "string" == typeof t2.dictionary ? h.string2buf(t2.dictionary) : "[object ArrayBuffer]" === u.call(t2.dictionary) ? new Uint8Array(t2.dictionary) : t2.dictionary, (r2 = a.deflateSetDictionary(this.strm, n2)) !== l) throw new Error(i[r2]);
            this._dict_set = true;
          }
        }
        function n(e2, t2) {
          var r2 = new p(t2);
          if (r2.push(e2, true), r2.err) throw r2.msg || i[r2.err];
          return r2.result;
        }
        p.prototype.push = function(e2, t2) {
          var r2, n2, i2 = this.strm, s2 = this.options.chunkSize;
          if (this.ended) return false;
          n2 = t2 === ~~t2 ? t2 : true === t2 ? 4 : 0, "string" == typeof e2 ? i2.input = h.string2buf(e2) : "[object ArrayBuffer]" === u.call(e2) ? i2.input = new Uint8Array(e2) : i2.input = e2, i2.next_in = 0, i2.avail_in = i2.input.length;
          do {
            if (0 === i2.avail_out && (i2.output = new o.Buf8(s2), i2.next_out = 0, i2.avail_out = s2), 1 !== (r2 = a.deflate(i2, n2)) && r2 !== l) return this.onEnd(r2), !(this.ended = true);
            0 !== i2.avail_out && (0 !== i2.avail_in || 4 !== n2 && 2 !== n2) || ("string" === this.options.to ? this.onData(h.buf2binstring(o.shrinkBuf(i2.output, i2.next_out))) : this.onData(o.shrinkBuf(i2.output, i2.next_out)));
          } while ((0 < i2.avail_in || 0 === i2.avail_out) && 1 !== r2);
          return 4 === n2 ? (r2 = a.deflateEnd(this.strm), this.onEnd(r2), this.ended = true, r2 === l) : 2 !== n2 || (this.onEnd(l), !(i2.avail_out = 0));
        }, p.prototype.onData = function(e2) {
          this.chunks.push(e2);
        }, p.prototype.onEnd = function(e2) {
          e2 === l && ("string" === this.options.to ? this.result = this.chunks.join("") : this.result = o.flattenChunks(this.chunks)), this.chunks = [], this.err = e2, this.msg = this.strm.msg;
        }, r.Deflate = p, r.deflate = n, r.deflateRaw = function(e2, t2) {
          return (t2 = t2 || {}).raw = true, n(e2, t2);
        }, r.gzip = function(e2, t2) {
          return (t2 = t2 || {}).gzip = true, n(e2, t2);
        };
      }, { "./utils/common": 41, "./utils/strings": 42, "./zlib/deflate": 46, "./zlib/messages": 51, "./zlib/zstream": 53 }], 40: [function(e, t, r) {
        "use strict";
        var c = e("./zlib/inflate"), d = e("./utils/common"), p = e("./utils/strings"), m = e("./zlib/constants"), n = e("./zlib/messages"), i = e("./zlib/zstream"), s = e("./zlib/gzheader"), _ = Object.prototype.toString;
        function a(e2) {
          if (!(this instanceof a)) return new a(e2);
          this.options = d.assign({ chunkSize: 16384, windowBits: 0, to: "" }, e2 || {});
          var t2 = this.options;
          t2.raw && 0 <= t2.windowBits && t2.windowBits < 16 && (t2.windowBits = -t2.windowBits, 0 === t2.windowBits && (t2.windowBits = -15)), !(0 <= t2.windowBits && t2.windowBits < 16) || e2 && e2.windowBits || (t2.windowBits += 32), 15 < t2.windowBits && t2.windowBits < 48 && 0 == (15 & t2.windowBits) && (t2.windowBits |= 15), this.err = 0, this.msg = "", this.ended = false, this.chunks = [], this.strm = new i(), this.strm.avail_out = 0;
          var r2 = c.inflateInit2(this.strm, t2.windowBits);
          if (r2 !== m.Z_OK) throw new Error(n[r2]);
          this.header = new s(), c.inflateGetHeader(this.strm, this.header);
        }
        function o(e2, t2) {
          var r2 = new a(t2);
          if (r2.push(e2, true), r2.err) throw r2.msg || n[r2.err];
          return r2.result;
        }
        a.prototype.push = function(e2, t2) {
          var r2, n2, i2, s2, a2, o2, h = this.strm, u = this.options.chunkSize, l = this.options.dictionary, f = false;
          if (this.ended) return false;
          n2 = t2 === ~~t2 ? t2 : true === t2 ? m.Z_FINISH : m.Z_NO_FLUSH, "string" == typeof e2 ? h.input = p.binstring2buf(e2) : "[object ArrayBuffer]" === _.call(e2) ? h.input = new Uint8Array(e2) : h.input = e2, h.next_in = 0, h.avail_in = h.input.length;
          do {
            if (0 === h.avail_out && (h.output = new d.Buf8(u), h.next_out = 0, h.avail_out = u), (r2 = c.inflate(h, m.Z_NO_FLUSH)) === m.Z_NEED_DICT && l && (o2 = "string" == typeof l ? p.string2buf(l) : "[object ArrayBuffer]" === _.call(l) ? new Uint8Array(l) : l, r2 = c.inflateSetDictionary(this.strm, o2)), r2 === m.Z_BUF_ERROR && true === f && (r2 = m.Z_OK, f = false), r2 !== m.Z_STREAM_END && r2 !== m.Z_OK) return this.onEnd(r2), !(this.ended = true);
            h.next_out && (0 !== h.avail_out && r2 !== m.Z_STREAM_END && (0 !== h.avail_in || n2 !== m.Z_FINISH && n2 !== m.Z_SYNC_FLUSH) || ("string" === this.options.to ? (i2 = p.utf8border(h.output, h.next_out), s2 = h.next_out - i2, a2 = p.buf2string(h.output, i2), h.next_out = s2, h.avail_out = u - s2, s2 && d.arraySet(h.output, h.output, i2, s2, 0), this.onData(a2)) : this.onData(d.shrinkBuf(h.output, h.next_out)))), 0 === h.avail_in && 0 === h.avail_out && (f = true);
          } while ((0 < h.avail_in || 0 === h.avail_out) && r2 !== m.Z_STREAM_END);
          return r2 === m.Z_STREAM_END && (n2 = m.Z_FINISH), n2 === m.Z_FINISH ? (r2 = c.inflateEnd(this.strm), this.onEnd(r2), this.ended = true, r2 === m.Z_OK) : n2 !== m.Z_SYNC_FLUSH || (this.onEnd(m.Z_OK), !(h.avail_out = 0));
        }, a.prototype.onData = function(e2) {
          this.chunks.push(e2);
        }, a.prototype.onEnd = function(e2) {
          e2 === m.Z_OK && ("string" === this.options.to ? this.result = this.chunks.join("") : this.result = d.flattenChunks(this.chunks)), this.chunks = [], this.err = e2, this.msg = this.strm.msg;
        }, r.Inflate = a, r.inflate = o, r.inflateRaw = function(e2, t2) {
          return (t2 = t2 || {}).raw = true, o(e2, t2);
        }, r.ungzip = o;
      }, { "./utils/common": 41, "./utils/strings": 42, "./zlib/constants": 44, "./zlib/gzheader": 47, "./zlib/inflate": 49, "./zlib/messages": 51, "./zlib/zstream": 53 }], 41: [function(e, t, r) {
        "use strict";
        var n = "undefined" != typeof Uint8Array && "undefined" != typeof Uint16Array && "undefined" != typeof Int32Array;
        r.assign = function(e2) {
          for (var t2 = Array.prototype.slice.call(arguments, 1); t2.length; ) {
            var r2 = t2.shift();
            if (r2) {
              if ("object" != typeof r2) throw new TypeError(r2 + "must be non-object");
              for (var n2 in r2) r2.hasOwnProperty(n2) && (e2[n2] = r2[n2]);
            }
          }
          return e2;
        }, r.shrinkBuf = function(e2, t2) {
          return e2.length === t2 ? e2 : e2.subarray ? e2.subarray(0, t2) : (e2.length = t2, e2);
        };
        var i = { arraySet: function(e2, t2, r2, n2, i2) {
          if (t2.subarray && e2.subarray) e2.set(t2.subarray(r2, r2 + n2), i2);
          else for (var s2 = 0; s2 < n2; s2++) e2[i2 + s2] = t2[r2 + s2];
        }, flattenChunks: function(e2) {
          var t2, r2, n2, i2, s2, a;
          for (t2 = n2 = 0, r2 = e2.length; t2 < r2; t2++) n2 += e2[t2].length;
          for (a = new Uint8Array(n2), t2 = i2 = 0, r2 = e2.length; t2 < r2; t2++) s2 = e2[t2], a.set(s2, i2), i2 += s2.length;
          return a;
        } }, s = { arraySet: function(e2, t2, r2, n2, i2) {
          for (var s2 = 0; s2 < n2; s2++) e2[i2 + s2] = t2[r2 + s2];
        }, flattenChunks: function(e2) {
          return [].concat.apply([], e2);
        } };
        r.setTyped = function(e2) {
          e2 ? (r.Buf8 = Uint8Array, r.Buf16 = Uint16Array, r.Buf32 = Int32Array, r.assign(r, i)) : (r.Buf8 = Array, r.Buf16 = Array, r.Buf32 = Array, r.assign(r, s));
        }, r.setTyped(n);
      }, {}], 42: [function(e, t, r) {
        "use strict";
        var h = e("./common"), i = true, s = true;
        try {
          String.fromCharCode.apply(null, [0]);
        } catch (e2) {
          i = false;
        }
        try {
          String.fromCharCode.apply(null, new Uint8Array(1));
        } catch (e2) {
          s = false;
        }
        for (var u = new h.Buf8(256), n = 0; n < 256; n++) u[n] = 252 <= n ? 6 : 248 <= n ? 5 : 240 <= n ? 4 : 224 <= n ? 3 : 192 <= n ? 2 : 1;
        function l(e2, t2) {
          if (t2 < 65537 && (e2.subarray && s || !e2.subarray && i)) return String.fromCharCode.apply(null, h.shrinkBuf(e2, t2));
          for (var r2 = "", n2 = 0; n2 < t2; n2++) r2 += String.fromCharCode(e2[n2]);
          return r2;
        }
        u[254] = u[254] = 1, r.string2buf = function(e2) {
          var t2, r2, n2, i2, s2, a = e2.length, o = 0;
          for (i2 = 0; i2 < a; i2++) 55296 == (64512 & (r2 = e2.charCodeAt(i2))) && i2 + 1 < a && 56320 == (64512 & (n2 = e2.charCodeAt(i2 + 1))) && (r2 = 65536 + (r2 - 55296 << 10) + (n2 - 56320), i2++), o += r2 < 128 ? 1 : r2 < 2048 ? 2 : r2 < 65536 ? 3 : 4;
          for (t2 = new h.Buf8(o), i2 = s2 = 0; s2 < o; i2++) 55296 == (64512 & (r2 = e2.charCodeAt(i2))) && i2 + 1 < a && 56320 == (64512 & (n2 = e2.charCodeAt(i2 + 1))) && (r2 = 65536 + (r2 - 55296 << 10) + (n2 - 56320), i2++), r2 < 128 ? t2[s2++] = r2 : (r2 < 2048 ? t2[s2++] = 192 | r2 >>> 6 : (r2 < 65536 ? t2[s2++] = 224 | r2 >>> 12 : (t2[s2++] = 240 | r2 >>> 18, t2[s2++] = 128 | r2 >>> 12 & 63), t2[s2++] = 128 | r2 >>> 6 & 63), t2[s2++] = 128 | 63 & r2);
          return t2;
        }, r.buf2binstring = function(e2) {
          return l(e2, e2.length);
        }, r.binstring2buf = function(e2) {
          for (var t2 = new h.Buf8(e2.length), r2 = 0, n2 = t2.length; r2 < n2; r2++) t2[r2] = e2.charCodeAt(r2);
          return t2;
        }, r.buf2string = function(e2, t2) {
          var r2, n2, i2, s2, a = t2 || e2.length, o = new Array(2 * a);
          for (r2 = n2 = 0; r2 < a; ) if ((i2 = e2[r2++]) < 128) o[n2++] = i2;
          else if (4 < (s2 = u[i2])) o[n2++] = 65533, r2 += s2 - 1;
          else {
            for (i2 &= 2 === s2 ? 31 : 3 === s2 ? 15 : 7; 1 < s2 && r2 < a; ) i2 = i2 << 6 | 63 & e2[r2++], s2--;
            1 < s2 ? o[n2++] = 65533 : i2 < 65536 ? o[n2++] = i2 : (i2 -= 65536, o[n2++] = 55296 | i2 >> 10 & 1023, o[n2++] = 56320 | 1023 & i2);
          }
          return l(o, n2);
        }, r.utf8border = function(e2, t2) {
          var r2;
          for ((t2 = t2 || e2.length) > e2.length && (t2 = e2.length), r2 = t2 - 1; 0 <= r2 && 128 == (192 & e2[r2]); ) r2--;
          return r2 < 0 ? t2 : 0 === r2 ? t2 : r2 + u[e2[r2]] > t2 ? r2 : t2;
        };
      }, { "./common": 41 }], 43: [function(e, t, r) {
        "use strict";
        t.exports = function(e2, t2, r2, n) {
          for (var i = 65535 & e2 | 0, s = e2 >>> 16 & 65535 | 0, a = 0; 0 !== r2; ) {
            for (r2 -= a = 2e3 < r2 ? 2e3 : r2; s = s + (i = i + t2[n++] | 0) | 0, --a; ) ;
            i %= 65521, s %= 65521;
          }
          return i | s << 16 | 0;
        };
      }, {}], 44: [function(e, t, r) {
        "use strict";
        t.exports = { Z_NO_FLUSH: 0, Z_PARTIAL_FLUSH: 1, Z_SYNC_FLUSH: 2, Z_FULL_FLUSH: 3, Z_FINISH: 4, Z_BLOCK: 5, Z_TREES: 6, Z_OK: 0, Z_STREAM_END: 1, Z_NEED_DICT: 2, Z_ERRNO: -1, Z_STREAM_ERROR: -2, Z_DATA_ERROR: -3, Z_BUF_ERROR: -5, Z_NO_COMPRESSION: 0, Z_BEST_SPEED: 1, Z_BEST_COMPRESSION: 9, Z_DEFAULT_COMPRESSION: -1, Z_FILTERED: 1, Z_HUFFMAN_ONLY: 2, Z_RLE: 3, Z_FIXED: 4, Z_DEFAULT_STRATEGY: 0, Z_BINARY: 0, Z_TEXT: 1, Z_UNKNOWN: 2, Z_DEFLATED: 8 };
      }, {}], 45: [function(e, t, r) {
        "use strict";
        var o = (function() {
          for (var e2, t2 = [], r2 = 0; r2 < 256; r2++) {
            e2 = r2;
            for (var n = 0; n < 8; n++) e2 = 1 & e2 ? 3988292384 ^ e2 >>> 1 : e2 >>> 1;
            t2[r2] = e2;
          }
          return t2;
        })();
        t.exports = function(e2, t2, r2, n) {
          var i = o, s = n + r2;
          e2 ^= -1;
          for (var a = n; a < s; a++) e2 = e2 >>> 8 ^ i[255 & (e2 ^ t2[a])];
          return -1 ^ e2;
        };
      }, {}], 46: [function(e, t, r) {
        "use strict";
        var h, c = e("../utils/common"), u = e("./trees"), d = e("./adler32"), p = e("./crc32"), n = e("./messages"), l = 0, f = 4, m = 0, _ = -2, g = -1, b = 4, i = 2, v = 8, y = 9, s = 286, a = 30, o = 19, w = 2 * s + 1, k = 15, x = 3, S = 258, z = S + x + 1, C = 42, E = 113, A = 1, I = 2, O = 3, B = 4;
        function R(e2, t2) {
          return e2.msg = n[t2], t2;
        }
        function T(e2) {
          return (e2 << 1) - (4 < e2 ? 9 : 0);
        }
        function D(e2) {
          for (var t2 = e2.length; 0 <= --t2; ) e2[t2] = 0;
        }
        function F(e2) {
          var t2 = e2.state, r2 = t2.pending;
          r2 > e2.avail_out && (r2 = e2.avail_out), 0 !== r2 && (c.arraySet(e2.output, t2.pending_buf, t2.pending_out, r2, e2.next_out), e2.next_out += r2, t2.pending_out += r2, e2.total_out += r2, e2.avail_out -= r2, t2.pending -= r2, 0 === t2.pending && (t2.pending_out = 0));
        }
        function N(e2, t2) {
          u._tr_flush_block(e2, 0 <= e2.block_start ? e2.block_start : -1, e2.strstart - e2.block_start, t2), e2.block_start = e2.strstart, F(e2.strm);
        }
        function U(e2, t2) {
          e2.pending_buf[e2.pending++] = t2;
        }
        function P(e2, t2) {
          e2.pending_buf[e2.pending++] = t2 >>> 8 & 255, e2.pending_buf[e2.pending++] = 255 & t2;
        }
        function L(e2, t2) {
          var r2, n2, i2 = e2.max_chain_length, s2 = e2.strstart, a2 = e2.prev_length, o2 = e2.nice_match, h2 = e2.strstart > e2.w_size - z ? e2.strstart - (e2.w_size - z) : 0, u2 = e2.window, l2 = e2.w_mask, f2 = e2.prev, c2 = e2.strstart + S, d2 = u2[s2 + a2 - 1], p2 = u2[s2 + a2];
          e2.prev_length >= e2.good_match && (i2 >>= 2), o2 > e2.lookahead && (o2 = e2.lookahead);
          do {
            if (u2[(r2 = t2) + a2] === p2 && u2[r2 + a2 - 1] === d2 && u2[r2] === u2[s2] && u2[++r2] === u2[s2 + 1]) {
              s2 += 2, r2++;
              do {
              } while (u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && u2[++s2] === u2[++r2] && s2 < c2);
              if (n2 = S - (c2 - s2), s2 = c2 - S, a2 < n2) {
                if (e2.match_start = t2, o2 <= (a2 = n2)) break;
                d2 = u2[s2 + a2 - 1], p2 = u2[s2 + a2];
              }
            }
          } while ((t2 = f2[t2 & l2]) > h2 && 0 != --i2);
          return a2 <= e2.lookahead ? a2 : e2.lookahead;
        }
        function j(e2) {
          var t2, r2, n2, i2, s2, a2, o2, h2, u2, l2, f2 = e2.w_size;
          do {
            if (i2 = e2.window_size - e2.lookahead - e2.strstart, e2.strstart >= f2 + (f2 - z)) {
              for (c.arraySet(e2.window, e2.window, f2, f2, 0), e2.match_start -= f2, e2.strstart -= f2, e2.block_start -= f2, t2 = r2 = e2.hash_size; n2 = e2.head[--t2], e2.head[t2] = f2 <= n2 ? n2 - f2 : 0, --r2; ) ;
              for (t2 = r2 = f2; n2 = e2.prev[--t2], e2.prev[t2] = f2 <= n2 ? n2 - f2 : 0, --r2; ) ;
              i2 += f2;
            }
            if (0 === e2.strm.avail_in) break;
            if (a2 = e2.strm, o2 = e2.window, h2 = e2.strstart + e2.lookahead, u2 = i2, l2 = void 0, l2 = a2.avail_in, u2 < l2 && (l2 = u2), r2 = 0 === l2 ? 0 : (a2.avail_in -= l2, c.arraySet(o2, a2.input, a2.next_in, l2, h2), 1 === a2.state.wrap ? a2.adler = d(a2.adler, o2, l2, h2) : 2 === a2.state.wrap && (a2.adler = p(a2.adler, o2, l2, h2)), a2.next_in += l2, a2.total_in += l2, l2), e2.lookahead += r2, e2.lookahead + e2.insert >= x) for (s2 = e2.strstart - e2.insert, e2.ins_h = e2.window[s2], e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[s2 + 1]) & e2.hash_mask; e2.insert && (e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[s2 + x - 1]) & e2.hash_mask, e2.prev[s2 & e2.w_mask] = e2.head[e2.ins_h], e2.head[e2.ins_h] = s2, s2++, e2.insert--, !(e2.lookahead + e2.insert < x)); ) ;
          } while (e2.lookahead < z && 0 !== e2.strm.avail_in);
        }
        function Z(e2, t2) {
          for (var r2, n2; ; ) {
            if (e2.lookahead < z) {
              if (j(e2), e2.lookahead < z && t2 === l) return A;
              if (0 === e2.lookahead) break;
            }
            if (r2 = 0, e2.lookahead >= x && (e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[e2.strstart + x - 1]) & e2.hash_mask, r2 = e2.prev[e2.strstart & e2.w_mask] = e2.head[e2.ins_h], e2.head[e2.ins_h] = e2.strstart), 0 !== r2 && e2.strstart - r2 <= e2.w_size - z && (e2.match_length = L(e2, r2)), e2.match_length >= x) if (n2 = u._tr_tally(e2, e2.strstart - e2.match_start, e2.match_length - x), e2.lookahead -= e2.match_length, e2.match_length <= e2.max_lazy_match && e2.lookahead >= x) {
              for (e2.match_length--; e2.strstart++, e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[e2.strstart + x - 1]) & e2.hash_mask, r2 = e2.prev[e2.strstart & e2.w_mask] = e2.head[e2.ins_h], e2.head[e2.ins_h] = e2.strstart, 0 != --e2.match_length; ) ;
              e2.strstart++;
            } else e2.strstart += e2.match_length, e2.match_length = 0, e2.ins_h = e2.window[e2.strstart], e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[e2.strstart + 1]) & e2.hash_mask;
            else n2 = u._tr_tally(e2, 0, e2.window[e2.strstart]), e2.lookahead--, e2.strstart++;
            if (n2 && (N(e2, false), 0 === e2.strm.avail_out)) return A;
          }
          return e2.insert = e2.strstart < x - 1 ? e2.strstart : x - 1, t2 === f ? (N(e2, true), 0 === e2.strm.avail_out ? O : B) : e2.last_lit && (N(e2, false), 0 === e2.strm.avail_out) ? A : I;
        }
        function W(e2, t2) {
          for (var r2, n2, i2; ; ) {
            if (e2.lookahead < z) {
              if (j(e2), e2.lookahead < z && t2 === l) return A;
              if (0 === e2.lookahead) break;
            }
            if (r2 = 0, e2.lookahead >= x && (e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[e2.strstart + x - 1]) & e2.hash_mask, r2 = e2.prev[e2.strstart & e2.w_mask] = e2.head[e2.ins_h], e2.head[e2.ins_h] = e2.strstart), e2.prev_length = e2.match_length, e2.prev_match = e2.match_start, e2.match_length = x - 1, 0 !== r2 && e2.prev_length < e2.max_lazy_match && e2.strstart - r2 <= e2.w_size - z && (e2.match_length = L(e2, r2), e2.match_length <= 5 && (1 === e2.strategy || e2.match_length === x && 4096 < e2.strstart - e2.match_start) && (e2.match_length = x - 1)), e2.prev_length >= x && e2.match_length <= e2.prev_length) {
              for (i2 = e2.strstart + e2.lookahead - x, n2 = u._tr_tally(e2, e2.strstart - 1 - e2.prev_match, e2.prev_length - x), e2.lookahead -= e2.prev_length - 1, e2.prev_length -= 2; ++e2.strstart <= i2 && (e2.ins_h = (e2.ins_h << e2.hash_shift ^ e2.window[e2.strstart + x - 1]) & e2.hash_mask, r2 = e2.prev[e2.strstart & e2.w_mask] = e2.head[e2.ins_h], e2.head[e2.ins_h] = e2.strstart), 0 != --e2.prev_length; ) ;
              if (e2.match_available = 0, e2.match_length = x - 1, e2.strstart++, n2 && (N(e2, false), 0 === e2.strm.avail_out)) return A;
            } else if (e2.match_available) {
              if ((n2 = u._tr_tally(e2, 0, e2.window[e2.strstart - 1])) && N(e2, false), e2.strstart++, e2.lookahead--, 0 === e2.strm.avail_out) return A;
            } else e2.match_available = 1, e2.strstart++, e2.lookahead--;
          }
          return e2.match_available && (n2 = u._tr_tally(e2, 0, e2.window[e2.strstart - 1]), e2.match_available = 0), e2.insert = e2.strstart < x - 1 ? e2.strstart : x - 1, t2 === f ? (N(e2, true), 0 === e2.strm.avail_out ? O : B) : e2.last_lit && (N(e2, false), 0 === e2.strm.avail_out) ? A : I;
        }
        function M(e2, t2, r2, n2, i2) {
          this.good_length = e2, this.max_lazy = t2, this.nice_length = r2, this.max_chain = n2, this.func = i2;
        }
        function H() {
          this.strm = null, this.status = 0, this.pending_buf = null, this.pending_buf_size = 0, this.pending_out = 0, this.pending = 0, this.wrap = 0, this.gzhead = null, this.gzindex = 0, this.method = v, this.last_flush = -1, this.w_size = 0, this.w_bits = 0, this.w_mask = 0, this.window = null, this.window_size = 0, this.prev = null, this.head = null, this.ins_h = 0, this.hash_size = 0, this.hash_bits = 0, this.hash_mask = 0, this.hash_shift = 0, this.block_start = 0, this.match_length = 0, this.prev_match = 0, this.match_available = 0, this.strstart = 0, this.match_start = 0, this.lookahead = 0, this.prev_length = 0, this.max_chain_length = 0, this.max_lazy_match = 0, this.level = 0, this.strategy = 0, this.good_match = 0, this.nice_match = 0, this.dyn_ltree = new c.Buf16(2 * w), this.dyn_dtree = new c.Buf16(2 * (2 * a + 1)), this.bl_tree = new c.Buf16(2 * (2 * o + 1)), D(this.dyn_ltree), D(this.dyn_dtree), D(this.bl_tree), this.l_desc = null, this.d_desc = null, this.bl_desc = null, this.bl_count = new c.Buf16(k + 1), this.heap = new c.Buf16(2 * s + 1), D(this.heap), this.heap_len = 0, this.heap_max = 0, this.depth = new c.Buf16(2 * s + 1), D(this.depth), this.l_buf = 0, this.lit_bufsize = 0, this.last_lit = 0, this.d_buf = 0, this.opt_len = 0, this.static_len = 0, this.matches = 0, this.insert = 0, this.bi_buf = 0, this.bi_valid = 0;
        }
        function G(e2) {
          var t2;
          return e2 && e2.state ? (e2.total_in = e2.total_out = 0, e2.data_type = i, (t2 = e2.state).pending = 0, t2.pending_out = 0, t2.wrap < 0 && (t2.wrap = -t2.wrap), t2.status = t2.wrap ? C : E, e2.adler = 2 === t2.wrap ? 0 : 1, t2.last_flush = l, u._tr_init(t2), m) : R(e2, _);
        }
        function K(e2) {
          var t2 = G(e2);
          return t2 === m && (function(e3) {
            e3.window_size = 2 * e3.w_size, D(e3.head), e3.max_lazy_match = h[e3.level].max_lazy, e3.good_match = h[e3.level].good_length, e3.nice_match = h[e3.level].nice_length, e3.max_chain_length = h[e3.level].max_chain, e3.strstart = 0, e3.block_start = 0, e3.lookahead = 0, e3.insert = 0, e3.match_length = e3.prev_length = x - 1, e3.match_available = 0, e3.ins_h = 0;
          })(e2.state), t2;
        }
        function Y(e2, t2, r2, n2, i2, s2) {
          if (!e2) return _;
          var a2 = 1;
          if (t2 === g && (t2 = 6), n2 < 0 ? (a2 = 0, n2 = -n2) : 15 < n2 && (a2 = 2, n2 -= 16), i2 < 1 || y < i2 || r2 !== v || n2 < 8 || 15 < n2 || t2 < 0 || 9 < t2 || s2 < 0 || b < s2) return R(e2, _);
          8 === n2 && (n2 = 9);
          var o2 = new H();
          return (e2.state = o2).strm = e2, o2.wrap = a2, o2.gzhead = null, o2.w_bits = n2, o2.w_size = 1 << o2.w_bits, o2.w_mask = o2.w_size - 1, o2.hash_bits = i2 + 7, o2.hash_size = 1 << o2.hash_bits, o2.hash_mask = o2.hash_size - 1, o2.hash_shift = ~~((o2.hash_bits + x - 1) / x), o2.window = new c.Buf8(2 * o2.w_size), o2.head = new c.Buf16(o2.hash_size), o2.prev = new c.Buf16(o2.w_size), o2.lit_bufsize = 1 << i2 + 6, o2.pending_buf_size = 4 * o2.lit_bufsize, o2.pending_buf = new c.Buf8(o2.pending_buf_size), o2.d_buf = 1 * o2.lit_bufsize, o2.l_buf = 3 * o2.lit_bufsize, o2.level = t2, o2.strategy = s2, o2.method = r2, K(e2);
        }
        h = [new M(0, 0, 0, 0, function(e2, t2) {
          var r2 = 65535;
          for (r2 > e2.pending_buf_size - 5 && (r2 = e2.pending_buf_size - 5); ; ) {
            if (e2.lookahead <= 1) {
              if (j(e2), 0 === e2.lookahead && t2 === l) return A;
              if (0 === e2.lookahead) break;
            }
            e2.strstart += e2.lookahead, e2.lookahead = 0;
            var n2 = e2.block_start + r2;
            if ((0 === e2.strstart || e2.strstart >= n2) && (e2.lookahead = e2.strstart - n2, e2.strstart = n2, N(e2, false), 0 === e2.strm.avail_out)) return A;
            if (e2.strstart - e2.block_start >= e2.w_size - z && (N(e2, false), 0 === e2.strm.avail_out)) return A;
          }
          return e2.insert = 0, t2 === f ? (N(e2, true), 0 === e2.strm.avail_out ? O : B) : (e2.strstart > e2.block_start && (N(e2, false), e2.strm.avail_out), A);
        }), new M(4, 4, 8, 4, Z), new M(4, 5, 16, 8, Z), new M(4, 6, 32, 32, Z), new M(4, 4, 16, 16, W), new M(8, 16, 32, 32, W), new M(8, 16, 128, 128, W), new M(8, 32, 128, 256, W), new M(32, 128, 258, 1024, W), new M(32, 258, 258, 4096, W)], r.deflateInit = function(e2, t2) {
          return Y(e2, t2, v, 15, 8, 0);
        }, r.deflateInit2 = Y, r.deflateReset = K, r.deflateResetKeep = G, r.deflateSetHeader = function(e2, t2) {
          return e2 && e2.state ? 2 !== e2.state.wrap ? _ : (e2.state.gzhead = t2, m) : _;
        }, r.deflate = function(e2, t2) {
          var r2, n2, i2, s2;
          if (!e2 || !e2.state || 5 < t2 || t2 < 0) return e2 ? R(e2, _) : _;
          if (n2 = e2.state, !e2.output || !e2.input && 0 !== e2.avail_in || 666 === n2.status && t2 !== f) return R(e2, 0 === e2.avail_out ? -5 : _);
          if (n2.strm = e2, r2 = n2.last_flush, n2.last_flush = t2, n2.status === C) if (2 === n2.wrap) e2.adler = 0, U(n2, 31), U(n2, 139), U(n2, 8), n2.gzhead ? (U(n2, (n2.gzhead.text ? 1 : 0) + (n2.gzhead.hcrc ? 2 : 0) + (n2.gzhead.extra ? 4 : 0) + (n2.gzhead.name ? 8 : 0) + (n2.gzhead.comment ? 16 : 0)), U(n2, 255 & n2.gzhead.time), U(n2, n2.gzhead.time >> 8 & 255), U(n2, n2.gzhead.time >> 16 & 255), U(n2, n2.gzhead.time >> 24 & 255), U(n2, 9 === n2.level ? 2 : 2 <= n2.strategy || n2.level < 2 ? 4 : 0), U(n2, 255 & n2.gzhead.os), n2.gzhead.extra && n2.gzhead.extra.length && (U(n2, 255 & n2.gzhead.extra.length), U(n2, n2.gzhead.extra.length >> 8 & 255)), n2.gzhead.hcrc && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending, 0)), n2.gzindex = 0, n2.status = 69) : (U(n2, 0), U(n2, 0), U(n2, 0), U(n2, 0), U(n2, 0), U(n2, 9 === n2.level ? 2 : 2 <= n2.strategy || n2.level < 2 ? 4 : 0), U(n2, 3), n2.status = E);
          else {
            var a2 = v + (n2.w_bits - 8 << 4) << 8;
            a2 |= (2 <= n2.strategy || n2.level < 2 ? 0 : n2.level < 6 ? 1 : 6 === n2.level ? 2 : 3) << 6, 0 !== n2.strstart && (a2 |= 32), a2 += 31 - a2 % 31, n2.status = E, P(n2, a2), 0 !== n2.strstart && (P(n2, e2.adler >>> 16), P(n2, 65535 & e2.adler)), e2.adler = 1;
          }
          if (69 === n2.status) if (n2.gzhead.extra) {
            for (i2 = n2.pending; n2.gzindex < (65535 & n2.gzhead.extra.length) && (n2.pending !== n2.pending_buf_size || (n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), F(e2), i2 = n2.pending, n2.pending !== n2.pending_buf_size)); ) U(n2, 255 & n2.gzhead.extra[n2.gzindex]), n2.gzindex++;
            n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), n2.gzindex === n2.gzhead.extra.length && (n2.gzindex = 0, n2.status = 73);
          } else n2.status = 73;
          if (73 === n2.status) if (n2.gzhead.name) {
            i2 = n2.pending;
            do {
              if (n2.pending === n2.pending_buf_size && (n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), F(e2), i2 = n2.pending, n2.pending === n2.pending_buf_size)) {
                s2 = 1;
                break;
              }
              s2 = n2.gzindex < n2.gzhead.name.length ? 255 & n2.gzhead.name.charCodeAt(n2.gzindex++) : 0, U(n2, s2);
            } while (0 !== s2);
            n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), 0 === s2 && (n2.gzindex = 0, n2.status = 91);
          } else n2.status = 91;
          if (91 === n2.status) if (n2.gzhead.comment) {
            i2 = n2.pending;
            do {
              if (n2.pending === n2.pending_buf_size && (n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), F(e2), i2 = n2.pending, n2.pending === n2.pending_buf_size)) {
                s2 = 1;
                break;
              }
              s2 = n2.gzindex < n2.gzhead.comment.length ? 255 & n2.gzhead.comment.charCodeAt(n2.gzindex++) : 0, U(n2, s2);
            } while (0 !== s2);
            n2.gzhead.hcrc && n2.pending > i2 && (e2.adler = p(e2.adler, n2.pending_buf, n2.pending - i2, i2)), 0 === s2 && (n2.status = 103);
          } else n2.status = 103;
          if (103 === n2.status && (n2.gzhead.hcrc ? (n2.pending + 2 > n2.pending_buf_size && F(e2), n2.pending + 2 <= n2.pending_buf_size && (U(n2, 255 & e2.adler), U(n2, e2.adler >> 8 & 255), e2.adler = 0, n2.status = E)) : n2.status = E), 0 !== n2.pending) {
            if (F(e2), 0 === e2.avail_out) return n2.last_flush = -1, m;
          } else if (0 === e2.avail_in && T(t2) <= T(r2) && t2 !== f) return R(e2, -5);
          if (666 === n2.status && 0 !== e2.avail_in) return R(e2, -5);
          if (0 !== e2.avail_in || 0 !== n2.lookahead || t2 !== l && 666 !== n2.status) {
            var o2 = 2 === n2.strategy ? (function(e3, t3) {
              for (var r3; ; ) {
                if (0 === e3.lookahead && (j(e3), 0 === e3.lookahead)) {
                  if (t3 === l) return A;
                  break;
                }
                if (e3.match_length = 0, r3 = u._tr_tally(e3, 0, e3.window[e3.strstart]), e3.lookahead--, e3.strstart++, r3 && (N(e3, false), 0 === e3.strm.avail_out)) return A;
              }
              return e3.insert = 0, t3 === f ? (N(e3, true), 0 === e3.strm.avail_out ? O : B) : e3.last_lit && (N(e3, false), 0 === e3.strm.avail_out) ? A : I;
            })(n2, t2) : 3 === n2.strategy ? (function(e3, t3) {
              for (var r3, n3, i3, s3, a3 = e3.window; ; ) {
                if (e3.lookahead <= S) {
                  if (j(e3), e3.lookahead <= S && t3 === l) return A;
                  if (0 === e3.lookahead) break;
                }
                if (e3.match_length = 0, e3.lookahead >= x && 0 < e3.strstart && (n3 = a3[i3 = e3.strstart - 1]) === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3]) {
                  s3 = e3.strstart + S;
                  do {
                  } while (n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && n3 === a3[++i3] && i3 < s3);
                  e3.match_length = S - (s3 - i3), e3.match_length > e3.lookahead && (e3.match_length = e3.lookahead);
                }
                if (e3.match_length >= x ? (r3 = u._tr_tally(e3, 1, e3.match_length - x), e3.lookahead -= e3.match_length, e3.strstart += e3.match_length, e3.match_length = 0) : (r3 = u._tr_tally(e3, 0, e3.window[e3.strstart]), e3.lookahead--, e3.strstart++), r3 && (N(e3, false), 0 === e3.strm.avail_out)) return A;
              }
              return e3.insert = 0, t3 === f ? (N(e3, true), 0 === e3.strm.avail_out ? O : B) : e3.last_lit && (N(e3, false), 0 === e3.strm.avail_out) ? A : I;
            })(n2, t2) : h[n2.level].func(n2, t2);
            if (o2 !== O && o2 !== B || (n2.status = 666), o2 === A || o2 === O) return 0 === e2.avail_out && (n2.last_flush = -1), m;
            if (o2 === I && (1 === t2 ? u._tr_align(n2) : 5 !== t2 && (u._tr_stored_block(n2, 0, 0, false), 3 === t2 && (D(n2.head), 0 === n2.lookahead && (n2.strstart = 0, n2.block_start = 0, n2.insert = 0))), F(e2), 0 === e2.avail_out)) return n2.last_flush = -1, m;
          }
          return t2 !== f ? m : n2.wrap <= 0 ? 1 : (2 === n2.wrap ? (U(n2, 255 & e2.adler), U(n2, e2.adler >> 8 & 255), U(n2, e2.adler >> 16 & 255), U(n2, e2.adler >> 24 & 255), U(n2, 255 & e2.total_in), U(n2, e2.total_in >> 8 & 255), U(n2, e2.total_in >> 16 & 255), U(n2, e2.total_in >> 24 & 255)) : (P(n2, e2.adler >>> 16), P(n2, 65535 & e2.adler)), F(e2), 0 < n2.wrap && (n2.wrap = -n2.wrap), 0 !== n2.pending ? m : 1);
        }, r.deflateEnd = function(e2) {
          var t2;
          return e2 && e2.state ? (t2 = e2.state.status) !== C && 69 !== t2 && 73 !== t2 && 91 !== t2 && 103 !== t2 && t2 !== E && 666 !== t2 ? R(e2, _) : (e2.state = null, t2 === E ? R(e2, -3) : m) : _;
        }, r.deflateSetDictionary = function(e2, t2) {
          var r2, n2, i2, s2, a2, o2, h2, u2, l2 = t2.length;
          if (!e2 || !e2.state) return _;
          if (2 === (s2 = (r2 = e2.state).wrap) || 1 === s2 && r2.status !== C || r2.lookahead) return _;
          for (1 === s2 && (e2.adler = d(e2.adler, t2, l2, 0)), r2.wrap = 0, l2 >= r2.w_size && (0 === s2 && (D(r2.head), r2.strstart = 0, r2.block_start = 0, r2.insert = 0), u2 = new c.Buf8(r2.w_size), c.arraySet(u2, t2, l2 - r2.w_size, r2.w_size, 0), t2 = u2, l2 = r2.w_size), a2 = e2.avail_in, o2 = e2.next_in, h2 = e2.input, e2.avail_in = l2, e2.next_in = 0, e2.input = t2, j(r2); r2.lookahead >= x; ) {
            for (n2 = r2.strstart, i2 = r2.lookahead - (x - 1); r2.ins_h = (r2.ins_h << r2.hash_shift ^ r2.window[n2 + x - 1]) & r2.hash_mask, r2.prev[n2 & r2.w_mask] = r2.head[r2.ins_h], r2.head[r2.ins_h] = n2, n2++, --i2; ) ;
            r2.strstart = n2, r2.lookahead = x - 1, j(r2);
          }
          return r2.strstart += r2.lookahead, r2.block_start = r2.strstart, r2.insert = r2.lookahead, r2.lookahead = 0, r2.match_length = r2.prev_length = x - 1, r2.match_available = 0, e2.next_in = o2, e2.input = h2, e2.avail_in = a2, r2.wrap = s2, m;
        }, r.deflateInfo = "pako deflate (from Nodeca project)";
      }, { "../utils/common": 41, "./adler32": 43, "./crc32": 45, "./messages": 51, "./trees": 52 }], 47: [function(e, t, r) {
        "use strict";
        t.exports = function() {
          this.text = 0, this.time = 0, this.xflags = 0, this.os = 0, this.extra = null, this.extra_len = 0, this.name = "", this.comment = "", this.hcrc = 0, this.done = false;
        };
      }, {}], 48: [function(e, t, r) {
        "use strict";
        t.exports = function(e2, t2) {
          var r2, n, i, s, a, o, h, u, l, f, c, d, p, m, _, g, b, v, y, w, k, x, S, z, C;
          r2 = e2.state, n = e2.next_in, z = e2.input, i = n + (e2.avail_in - 5), s = e2.next_out, C = e2.output, a = s - (t2 - e2.avail_out), o = s + (e2.avail_out - 257), h = r2.dmax, u = r2.wsize, l = r2.whave, f = r2.wnext, c = r2.window, d = r2.hold, p = r2.bits, m = r2.lencode, _ = r2.distcode, g = (1 << r2.lenbits) - 1, b = (1 << r2.distbits) - 1;
          e: do {
            p < 15 && (d += z[n++] << p, p += 8, d += z[n++] << p, p += 8), v = m[d & g];
            t: for (; ; ) {
              if (d >>>= y = v >>> 24, p -= y, 0 === (y = v >>> 16 & 255)) C[s++] = 65535 & v;
              else {
                if (!(16 & y)) {
                  if (0 == (64 & y)) {
                    v = m[(65535 & v) + (d & (1 << y) - 1)];
                    continue t;
                  }
                  if (32 & y) {
                    r2.mode = 12;
                    break e;
                  }
                  e2.msg = "invalid literal/length code", r2.mode = 30;
                  break e;
                }
                w = 65535 & v, (y &= 15) && (p < y && (d += z[n++] << p, p += 8), w += d & (1 << y) - 1, d >>>= y, p -= y), p < 15 && (d += z[n++] << p, p += 8, d += z[n++] << p, p += 8), v = _[d & b];
                r: for (; ; ) {
                  if (d >>>= y = v >>> 24, p -= y, !(16 & (y = v >>> 16 & 255))) {
                    if (0 == (64 & y)) {
                      v = _[(65535 & v) + (d & (1 << y) - 1)];
                      continue r;
                    }
                    e2.msg = "invalid distance code", r2.mode = 30;
                    break e;
                  }
                  if (k = 65535 & v, p < (y &= 15) && (d += z[n++] << p, (p += 8) < y && (d += z[n++] << p, p += 8)), h < (k += d & (1 << y) - 1)) {
                    e2.msg = "invalid distance too far back", r2.mode = 30;
                    break e;
                  }
                  if (d >>>= y, p -= y, (y = s - a) < k) {
                    if (l < (y = k - y) && r2.sane) {
                      e2.msg = "invalid distance too far back", r2.mode = 30;
                      break e;
                    }
                    if (S = c, (x = 0) === f) {
                      if (x += u - y, y < w) {
                        for (w -= y; C[s++] = c[x++], --y; ) ;
                        x = s - k, S = C;
                      }
                    } else if (f < y) {
                      if (x += u + f - y, (y -= f) < w) {
                        for (w -= y; C[s++] = c[x++], --y; ) ;
                        if (x = 0, f < w) {
                          for (w -= y = f; C[s++] = c[x++], --y; ) ;
                          x = s - k, S = C;
                        }
                      }
                    } else if (x += f - y, y < w) {
                      for (w -= y; C[s++] = c[x++], --y; ) ;
                      x = s - k, S = C;
                    }
                    for (; 2 < w; ) C[s++] = S[x++], C[s++] = S[x++], C[s++] = S[x++], w -= 3;
                    w && (C[s++] = S[x++], 1 < w && (C[s++] = S[x++]));
                  } else {
                    for (x = s - k; C[s++] = C[x++], C[s++] = C[x++], C[s++] = C[x++], 2 < (w -= 3); ) ;
                    w && (C[s++] = C[x++], 1 < w && (C[s++] = C[x++]));
                  }
                  break;
                }
              }
              break;
            }
          } while (n < i && s < o);
          n -= w = p >> 3, d &= (1 << (p -= w << 3)) - 1, e2.next_in = n, e2.next_out = s, e2.avail_in = n < i ? i - n + 5 : 5 - (n - i), e2.avail_out = s < o ? o - s + 257 : 257 - (s - o), r2.hold = d, r2.bits = p;
        };
      }, {}], 49: [function(e, t, r) {
        "use strict";
        var I = e("../utils/common"), O = e("./adler32"), B = e("./crc32"), R = e("./inffast"), T = e("./inftrees"), D = 1, F = 2, N = 0, U = -2, P = 1, n = 852, i = 592;
        function L(e2) {
          return (e2 >>> 24 & 255) + (e2 >>> 8 & 65280) + ((65280 & e2) << 8) + ((255 & e2) << 24);
        }
        function s() {
          this.mode = 0, this.last = false, this.wrap = 0, this.havedict = false, this.flags = 0, this.dmax = 0, this.check = 0, this.total = 0, this.head = null, this.wbits = 0, this.wsize = 0, this.whave = 0, this.wnext = 0, this.window = null, this.hold = 0, this.bits = 0, this.length = 0, this.offset = 0, this.extra = 0, this.lencode = null, this.distcode = null, this.lenbits = 0, this.distbits = 0, this.ncode = 0, this.nlen = 0, this.ndist = 0, this.have = 0, this.next = null, this.lens = new I.Buf16(320), this.work = new I.Buf16(288), this.lendyn = null, this.distdyn = null, this.sane = 0, this.back = 0, this.was = 0;
        }
        function a(e2) {
          var t2;
          return e2 && e2.state ? (t2 = e2.state, e2.total_in = e2.total_out = t2.total = 0, e2.msg = "", t2.wrap && (e2.adler = 1 & t2.wrap), t2.mode = P, t2.last = 0, t2.havedict = 0, t2.dmax = 32768, t2.head = null, t2.hold = 0, t2.bits = 0, t2.lencode = t2.lendyn = new I.Buf32(n), t2.distcode = t2.distdyn = new I.Buf32(i), t2.sane = 1, t2.back = -1, N) : U;
        }
        function o(e2) {
          var t2;
          return e2 && e2.state ? ((t2 = e2.state).wsize = 0, t2.whave = 0, t2.wnext = 0, a(e2)) : U;
        }
        function h(e2, t2) {
          var r2, n2;
          return e2 && e2.state ? (n2 = e2.state, t2 < 0 ? (r2 = 0, t2 = -t2) : (r2 = 1 + (t2 >> 4), t2 < 48 && (t2 &= 15)), t2 && (t2 < 8 || 15 < t2) ? U : (null !== n2.window && n2.wbits !== t2 && (n2.window = null), n2.wrap = r2, n2.wbits = t2, o(e2))) : U;
        }
        function u(e2, t2) {
          var r2, n2;
          return e2 ? (n2 = new s(), (e2.state = n2).window = null, (r2 = h(e2, t2)) !== N && (e2.state = null), r2) : U;
        }
        var l, f, c = true;
        function j(e2) {
          if (c) {
            var t2;
            for (l = new I.Buf32(512), f = new I.Buf32(32), t2 = 0; t2 < 144; ) e2.lens[t2++] = 8;
            for (; t2 < 256; ) e2.lens[t2++] = 9;
            for (; t2 < 280; ) e2.lens[t2++] = 7;
            for (; t2 < 288; ) e2.lens[t2++] = 8;
            for (T(D, e2.lens, 0, 288, l, 0, e2.work, { bits: 9 }), t2 = 0; t2 < 32; ) e2.lens[t2++] = 5;
            T(F, e2.lens, 0, 32, f, 0, e2.work, { bits: 5 }), c = false;
          }
          e2.lencode = l, e2.lenbits = 9, e2.distcode = f, e2.distbits = 5;
        }
        function Z(e2, t2, r2, n2) {
          var i2, s2 = e2.state;
          return null === s2.window && (s2.wsize = 1 << s2.wbits, s2.wnext = 0, s2.whave = 0, s2.window = new I.Buf8(s2.wsize)), n2 >= s2.wsize ? (I.arraySet(s2.window, t2, r2 - s2.wsize, s2.wsize, 0), s2.wnext = 0, s2.whave = s2.wsize) : (n2 < (i2 = s2.wsize - s2.wnext) && (i2 = n2), I.arraySet(s2.window, t2, r2 - n2, i2, s2.wnext), (n2 -= i2) ? (I.arraySet(s2.window, t2, r2 - n2, n2, 0), s2.wnext = n2, s2.whave = s2.wsize) : (s2.wnext += i2, s2.wnext === s2.wsize && (s2.wnext = 0), s2.whave < s2.wsize && (s2.whave += i2))), 0;
        }
        r.inflateReset = o, r.inflateReset2 = h, r.inflateResetKeep = a, r.inflateInit = function(e2) {
          return u(e2, 15);
        }, r.inflateInit2 = u, r.inflate = function(e2, t2) {
          var r2, n2, i2, s2, a2, o2, h2, u2, l2, f2, c2, d, p, m, _, g, b, v, y, w, k, x, S, z, C = 0, E = new I.Buf8(4), A = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
          if (!e2 || !e2.state || !e2.output || !e2.input && 0 !== e2.avail_in) return U;
          12 === (r2 = e2.state).mode && (r2.mode = 13), a2 = e2.next_out, i2 = e2.output, h2 = e2.avail_out, s2 = e2.next_in, n2 = e2.input, o2 = e2.avail_in, u2 = r2.hold, l2 = r2.bits, f2 = o2, c2 = h2, x = N;
          e: for (; ; ) switch (r2.mode) {
            case P:
              if (0 === r2.wrap) {
                r2.mode = 13;
                break;
              }
              for (; l2 < 16; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if (2 & r2.wrap && 35615 === u2) {
                E[r2.check = 0] = 255 & u2, E[1] = u2 >>> 8 & 255, r2.check = B(r2.check, E, 2, 0), l2 = u2 = 0, r2.mode = 2;
                break;
              }
              if (r2.flags = 0, r2.head && (r2.head.done = false), !(1 & r2.wrap) || (((255 & u2) << 8) + (u2 >> 8)) % 31) {
                e2.msg = "incorrect header check", r2.mode = 30;
                break;
              }
              if (8 != (15 & u2)) {
                e2.msg = "unknown compression method", r2.mode = 30;
                break;
              }
              if (l2 -= 4, k = 8 + (15 & (u2 >>>= 4)), 0 === r2.wbits) r2.wbits = k;
              else if (k > r2.wbits) {
                e2.msg = "invalid window size", r2.mode = 30;
                break;
              }
              r2.dmax = 1 << k, e2.adler = r2.check = 1, r2.mode = 512 & u2 ? 10 : 12, l2 = u2 = 0;
              break;
            case 2:
              for (; l2 < 16; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if (r2.flags = u2, 8 != (255 & r2.flags)) {
                e2.msg = "unknown compression method", r2.mode = 30;
                break;
              }
              if (57344 & r2.flags) {
                e2.msg = "unknown header flags set", r2.mode = 30;
                break;
              }
              r2.head && (r2.head.text = u2 >> 8 & 1), 512 & r2.flags && (E[0] = 255 & u2, E[1] = u2 >>> 8 & 255, r2.check = B(r2.check, E, 2, 0)), l2 = u2 = 0, r2.mode = 3;
            case 3:
              for (; l2 < 32; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              r2.head && (r2.head.time = u2), 512 & r2.flags && (E[0] = 255 & u2, E[1] = u2 >>> 8 & 255, E[2] = u2 >>> 16 & 255, E[3] = u2 >>> 24 & 255, r2.check = B(r2.check, E, 4, 0)), l2 = u2 = 0, r2.mode = 4;
            case 4:
              for (; l2 < 16; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              r2.head && (r2.head.xflags = 255 & u2, r2.head.os = u2 >> 8), 512 & r2.flags && (E[0] = 255 & u2, E[1] = u2 >>> 8 & 255, r2.check = B(r2.check, E, 2, 0)), l2 = u2 = 0, r2.mode = 5;
            case 5:
              if (1024 & r2.flags) {
                for (; l2 < 16; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                r2.length = u2, r2.head && (r2.head.extra_len = u2), 512 & r2.flags && (E[0] = 255 & u2, E[1] = u2 >>> 8 & 255, r2.check = B(r2.check, E, 2, 0)), l2 = u2 = 0;
              } else r2.head && (r2.head.extra = null);
              r2.mode = 6;
            case 6:
              if (1024 & r2.flags && (o2 < (d = r2.length) && (d = o2), d && (r2.head && (k = r2.head.extra_len - r2.length, r2.head.extra || (r2.head.extra = new Array(r2.head.extra_len)), I.arraySet(r2.head.extra, n2, s2, d, k)), 512 & r2.flags && (r2.check = B(r2.check, n2, d, s2)), o2 -= d, s2 += d, r2.length -= d), r2.length)) break e;
              r2.length = 0, r2.mode = 7;
            case 7:
              if (2048 & r2.flags) {
                if (0 === o2) break e;
                for (d = 0; k = n2[s2 + d++], r2.head && k && r2.length < 65536 && (r2.head.name += String.fromCharCode(k)), k && d < o2; ) ;
                if (512 & r2.flags && (r2.check = B(r2.check, n2, d, s2)), o2 -= d, s2 += d, k) break e;
              } else r2.head && (r2.head.name = null);
              r2.length = 0, r2.mode = 8;
            case 8:
              if (4096 & r2.flags) {
                if (0 === o2) break e;
                for (d = 0; k = n2[s2 + d++], r2.head && k && r2.length < 65536 && (r2.head.comment += String.fromCharCode(k)), k && d < o2; ) ;
                if (512 & r2.flags && (r2.check = B(r2.check, n2, d, s2)), o2 -= d, s2 += d, k) break e;
              } else r2.head && (r2.head.comment = null);
              r2.mode = 9;
            case 9:
              if (512 & r2.flags) {
                for (; l2 < 16; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                if (u2 !== (65535 & r2.check)) {
                  e2.msg = "header crc mismatch", r2.mode = 30;
                  break;
                }
                l2 = u2 = 0;
              }
              r2.head && (r2.head.hcrc = r2.flags >> 9 & 1, r2.head.done = true), e2.adler = r2.check = 0, r2.mode = 12;
              break;
            case 10:
              for (; l2 < 32; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              e2.adler = r2.check = L(u2), l2 = u2 = 0, r2.mode = 11;
            case 11:
              if (0 === r2.havedict) return e2.next_out = a2, e2.avail_out = h2, e2.next_in = s2, e2.avail_in = o2, r2.hold = u2, r2.bits = l2, 2;
              e2.adler = r2.check = 1, r2.mode = 12;
            case 12:
              if (5 === t2 || 6 === t2) break e;
            case 13:
              if (r2.last) {
                u2 >>>= 7 & l2, l2 -= 7 & l2, r2.mode = 27;
                break;
              }
              for (; l2 < 3; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              switch (r2.last = 1 & u2, l2 -= 1, 3 & (u2 >>>= 1)) {
                case 0:
                  r2.mode = 14;
                  break;
                case 1:
                  if (j(r2), r2.mode = 20, 6 !== t2) break;
                  u2 >>>= 2, l2 -= 2;
                  break e;
                case 2:
                  r2.mode = 17;
                  break;
                case 3:
                  e2.msg = "invalid block type", r2.mode = 30;
              }
              u2 >>>= 2, l2 -= 2;
              break;
            case 14:
              for (u2 >>>= 7 & l2, l2 -= 7 & l2; l2 < 32; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if ((65535 & u2) != (u2 >>> 16 ^ 65535)) {
                e2.msg = "invalid stored block lengths", r2.mode = 30;
                break;
              }
              if (r2.length = 65535 & u2, l2 = u2 = 0, r2.mode = 15, 6 === t2) break e;
            case 15:
              r2.mode = 16;
            case 16:
              if (d = r2.length) {
                if (o2 < d && (d = o2), h2 < d && (d = h2), 0 === d) break e;
                I.arraySet(i2, n2, s2, d, a2), o2 -= d, s2 += d, h2 -= d, a2 += d, r2.length -= d;
                break;
              }
              r2.mode = 12;
              break;
            case 17:
              for (; l2 < 14; ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if (r2.nlen = 257 + (31 & u2), u2 >>>= 5, l2 -= 5, r2.ndist = 1 + (31 & u2), u2 >>>= 5, l2 -= 5, r2.ncode = 4 + (15 & u2), u2 >>>= 4, l2 -= 4, 286 < r2.nlen || 30 < r2.ndist) {
                e2.msg = "too many length or distance symbols", r2.mode = 30;
                break;
              }
              r2.have = 0, r2.mode = 18;
            case 18:
              for (; r2.have < r2.ncode; ) {
                for (; l2 < 3; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                r2.lens[A[r2.have++]] = 7 & u2, u2 >>>= 3, l2 -= 3;
              }
              for (; r2.have < 19; ) r2.lens[A[r2.have++]] = 0;
              if (r2.lencode = r2.lendyn, r2.lenbits = 7, S = { bits: r2.lenbits }, x = T(0, r2.lens, 0, 19, r2.lencode, 0, r2.work, S), r2.lenbits = S.bits, x) {
                e2.msg = "invalid code lengths set", r2.mode = 30;
                break;
              }
              r2.have = 0, r2.mode = 19;
            case 19:
              for (; r2.have < r2.nlen + r2.ndist; ) {
                for (; g = (C = r2.lencode[u2 & (1 << r2.lenbits) - 1]) >>> 16 & 255, b = 65535 & C, !((_ = C >>> 24) <= l2); ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                if (b < 16) u2 >>>= _, l2 -= _, r2.lens[r2.have++] = b;
                else {
                  if (16 === b) {
                    for (z = _ + 2; l2 < z; ) {
                      if (0 === o2) break e;
                      o2--, u2 += n2[s2++] << l2, l2 += 8;
                    }
                    if (u2 >>>= _, l2 -= _, 0 === r2.have) {
                      e2.msg = "invalid bit length repeat", r2.mode = 30;
                      break;
                    }
                    k = r2.lens[r2.have - 1], d = 3 + (3 & u2), u2 >>>= 2, l2 -= 2;
                  } else if (17 === b) {
                    for (z = _ + 3; l2 < z; ) {
                      if (0 === o2) break e;
                      o2--, u2 += n2[s2++] << l2, l2 += 8;
                    }
                    l2 -= _, k = 0, d = 3 + (7 & (u2 >>>= _)), u2 >>>= 3, l2 -= 3;
                  } else {
                    for (z = _ + 7; l2 < z; ) {
                      if (0 === o2) break e;
                      o2--, u2 += n2[s2++] << l2, l2 += 8;
                    }
                    l2 -= _, k = 0, d = 11 + (127 & (u2 >>>= _)), u2 >>>= 7, l2 -= 7;
                  }
                  if (r2.have + d > r2.nlen + r2.ndist) {
                    e2.msg = "invalid bit length repeat", r2.mode = 30;
                    break;
                  }
                  for (; d--; ) r2.lens[r2.have++] = k;
                }
              }
              if (30 === r2.mode) break;
              if (0 === r2.lens[256]) {
                e2.msg = "invalid code -- missing end-of-block", r2.mode = 30;
                break;
              }
              if (r2.lenbits = 9, S = { bits: r2.lenbits }, x = T(D, r2.lens, 0, r2.nlen, r2.lencode, 0, r2.work, S), r2.lenbits = S.bits, x) {
                e2.msg = "invalid literal/lengths set", r2.mode = 30;
                break;
              }
              if (r2.distbits = 6, r2.distcode = r2.distdyn, S = { bits: r2.distbits }, x = T(F, r2.lens, r2.nlen, r2.ndist, r2.distcode, 0, r2.work, S), r2.distbits = S.bits, x) {
                e2.msg = "invalid distances set", r2.mode = 30;
                break;
              }
              if (r2.mode = 20, 6 === t2) break e;
            case 20:
              r2.mode = 21;
            case 21:
              if (6 <= o2 && 258 <= h2) {
                e2.next_out = a2, e2.avail_out = h2, e2.next_in = s2, e2.avail_in = o2, r2.hold = u2, r2.bits = l2, R(e2, c2), a2 = e2.next_out, i2 = e2.output, h2 = e2.avail_out, s2 = e2.next_in, n2 = e2.input, o2 = e2.avail_in, u2 = r2.hold, l2 = r2.bits, 12 === r2.mode && (r2.back = -1);
                break;
              }
              for (r2.back = 0; g = (C = r2.lencode[u2 & (1 << r2.lenbits) - 1]) >>> 16 & 255, b = 65535 & C, !((_ = C >>> 24) <= l2); ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if (g && 0 == (240 & g)) {
                for (v = _, y = g, w = b; g = (C = r2.lencode[w + ((u2 & (1 << v + y) - 1) >> v)]) >>> 16 & 255, b = 65535 & C, !(v + (_ = C >>> 24) <= l2); ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                u2 >>>= v, l2 -= v, r2.back += v;
              }
              if (u2 >>>= _, l2 -= _, r2.back += _, r2.length = b, 0 === g) {
                r2.mode = 26;
                break;
              }
              if (32 & g) {
                r2.back = -1, r2.mode = 12;
                break;
              }
              if (64 & g) {
                e2.msg = "invalid literal/length code", r2.mode = 30;
                break;
              }
              r2.extra = 15 & g, r2.mode = 22;
            case 22:
              if (r2.extra) {
                for (z = r2.extra; l2 < z; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                r2.length += u2 & (1 << r2.extra) - 1, u2 >>>= r2.extra, l2 -= r2.extra, r2.back += r2.extra;
              }
              r2.was = r2.length, r2.mode = 23;
            case 23:
              for (; g = (C = r2.distcode[u2 & (1 << r2.distbits) - 1]) >>> 16 & 255, b = 65535 & C, !((_ = C >>> 24) <= l2); ) {
                if (0 === o2) break e;
                o2--, u2 += n2[s2++] << l2, l2 += 8;
              }
              if (0 == (240 & g)) {
                for (v = _, y = g, w = b; g = (C = r2.distcode[w + ((u2 & (1 << v + y) - 1) >> v)]) >>> 16 & 255, b = 65535 & C, !(v + (_ = C >>> 24) <= l2); ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                u2 >>>= v, l2 -= v, r2.back += v;
              }
              if (u2 >>>= _, l2 -= _, r2.back += _, 64 & g) {
                e2.msg = "invalid distance code", r2.mode = 30;
                break;
              }
              r2.offset = b, r2.extra = 15 & g, r2.mode = 24;
            case 24:
              if (r2.extra) {
                for (z = r2.extra; l2 < z; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                r2.offset += u2 & (1 << r2.extra) - 1, u2 >>>= r2.extra, l2 -= r2.extra, r2.back += r2.extra;
              }
              if (r2.offset > r2.dmax) {
                e2.msg = "invalid distance too far back", r2.mode = 30;
                break;
              }
              r2.mode = 25;
            case 25:
              if (0 === h2) break e;
              if (d = c2 - h2, r2.offset > d) {
                if ((d = r2.offset - d) > r2.whave && r2.sane) {
                  e2.msg = "invalid distance too far back", r2.mode = 30;
                  break;
                }
                p = d > r2.wnext ? (d -= r2.wnext, r2.wsize - d) : r2.wnext - d, d > r2.length && (d = r2.length), m = r2.window;
              } else m = i2, p = a2 - r2.offset, d = r2.length;
              for (h2 < d && (d = h2), h2 -= d, r2.length -= d; i2[a2++] = m[p++], --d; ) ;
              0 === r2.length && (r2.mode = 21);
              break;
            case 26:
              if (0 === h2) break e;
              i2[a2++] = r2.length, h2--, r2.mode = 21;
              break;
            case 27:
              if (r2.wrap) {
                for (; l2 < 32; ) {
                  if (0 === o2) break e;
                  o2--, u2 |= n2[s2++] << l2, l2 += 8;
                }
                if (c2 -= h2, e2.total_out += c2, r2.total += c2, c2 && (e2.adler = r2.check = r2.flags ? B(r2.check, i2, c2, a2 - c2) : O(r2.check, i2, c2, a2 - c2)), c2 = h2, (r2.flags ? u2 : L(u2)) !== r2.check) {
                  e2.msg = "incorrect data check", r2.mode = 30;
                  break;
                }
                l2 = u2 = 0;
              }
              r2.mode = 28;
            case 28:
              if (r2.wrap && r2.flags) {
                for (; l2 < 32; ) {
                  if (0 === o2) break e;
                  o2--, u2 += n2[s2++] << l2, l2 += 8;
                }
                if (u2 !== (4294967295 & r2.total)) {
                  e2.msg = "incorrect length check", r2.mode = 30;
                  break;
                }
                l2 = u2 = 0;
              }
              r2.mode = 29;
            case 29:
              x = 1;
              break e;
            case 30:
              x = -3;
              break e;
            case 31:
              return -4;
            case 32:
            default:
              return U;
          }
          return e2.next_out = a2, e2.avail_out = h2, e2.next_in = s2, e2.avail_in = o2, r2.hold = u2, r2.bits = l2, (r2.wsize || c2 !== e2.avail_out && r2.mode < 30 && (r2.mode < 27 || 4 !== t2)) && Z(e2, e2.output, e2.next_out, c2 - e2.avail_out) ? (r2.mode = 31, -4) : (f2 -= e2.avail_in, c2 -= e2.avail_out, e2.total_in += f2, e2.total_out += c2, r2.total += c2, r2.wrap && c2 && (e2.adler = r2.check = r2.flags ? B(r2.check, i2, c2, e2.next_out - c2) : O(r2.check, i2, c2, e2.next_out - c2)), e2.data_type = r2.bits + (r2.last ? 64 : 0) + (12 === r2.mode ? 128 : 0) + (20 === r2.mode || 15 === r2.mode ? 256 : 0), (0 == f2 && 0 === c2 || 4 === t2) && x === N && (x = -5), x);
        }, r.inflateEnd = function(e2) {
          if (!e2 || !e2.state) return U;
          var t2 = e2.state;
          return t2.window && (t2.window = null), e2.state = null, N;
        }, r.inflateGetHeader = function(e2, t2) {
          var r2;
          return e2 && e2.state ? 0 == (2 & (r2 = e2.state).wrap) ? U : ((r2.head = t2).done = false, N) : U;
        }, r.inflateSetDictionary = function(e2, t2) {
          var r2, n2 = t2.length;
          return e2 && e2.state ? 0 !== (r2 = e2.state).wrap && 11 !== r2.mode ? U : 11 === r2.mode && O(1, t2, n2, 0) !== r2.check ? -3 : Z(e2, t2, n2, n2) ? (r2.mode = 31, -4) : (r2.havedict = 1, N) : U;
        }, r.inflateInfo = "pako inflate (from Nodeca project)";
      }, { "../utils/common": 41, "./adler32": 43, "./crc32": 45, "./inffast": 48, "./inftrees": 50 }], 50: [function(e, t, r) {
        "use strict";
        var D = e("../utils/common"), F = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258, 0, 0], N = [16, 16, 16, 16, 16, 16, 16, 16, 17, 17, 17, 17, 18, 18, 18, 18, 19, 19, 19, 19, 20, 20, 20, 20, 21, 21, 21, 21, 16, 72, 78], U = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 0, 0], P = [16, 16, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 21, 22, 22, 23, 23, 24, 24, 25, 25, 26, 26, 27, 27, 28, 28, 29, 29, 64, 64];
        t.exports = function(e2, t2, r2, n, i, s, a, o) {
          var h, u, l, f, c, d, p, m, _, g = o.bits, b = 0, v = 0, y = 0, w = 0, k = 0, x = 0, S = 0, z = 0, C = 0, E = 0, A = null, I = 0, O = new D.Buf16(16), B = new D.Buf16(16), R = null, T = 0;
          for (b = 0; b <= 15; b++) O[b] = 0;
          for (v = 0; v < n; v++) O[t2[r2 + v]]++;
          for (k = g, w = 15; 1 <= w && 0 === O[w]; w--) ;
          if (w < k && (k = w), 0 === w) return i[s++] = 20971520, i[s++] = 20971520, o.bits = 1, 0;
          for (y = 1; y < w && 0 === O[y]; y++) ;
          for (k < y && (k = y), b = z = 1; b <= 15; b++) if (z <<= 1, (z -= O[b]) < 0) return -1;
          if (0 < z && (0 === e2 || 1 !== w)) return -1;
          for (B[1] = 0, b = 1; b < 15; b++) B[b + 1] = B[b] + O[b];
          for (v = 0; v < n; v++) 0 !== t2[r2 + v] && (a[B[t2[r2 + v]]++] = v);
          if (d = 0 === e2 ? (A = R = a, 19) : 1 === e2 ? (A = F, I -= 257, R = N, T -= 257, 256) : (A = U, R = P, -1), b = y, c = s, S = v = E = 0, l = -1, f = (C = 1 << (x = k)) - 1, 1 === e2 && 852 < C || 2 === e2 && 592 < C) return 1;
          for (; ; ) {
            for (p = b - S, _ = a[v] < d ? (m = 0, a[v]) : a[v] > d ? (m = R[T + a[v]], A[I + a[v]]) : (m = 96, 0), h = 1 << b - S, y = u = 1 << x; i[c + (E >> S) + (u -= h)] = p << 24 | m << 16 | _ | 0, 0 !== u; ) ;
            for (h = 1 << b - 1; E & h; ) h >>= 1;
            if (0 !== h ? (E &= h - 1, E += h) : E = 0, v++, 0 == --O[b]) {
              if (b === w) break;
              b = t2[r2 + a[v]];
            }
            if (k < b && (E & f) !== l) {
              for (0 === S && (S = k), c += y, z = 1 << (x = b - S); x + S < w && !((z -= O[x + S]) <= 0); ) x++, z <<= 1;
              if (C += 1 << x, 1 === e2 && 852 < C || 2 === e2 && 592 < C) return 1;
              i[l = E & f] = k << 24 | x << 16 | c - s | 0;
            }
          }
          return 0 !== E && (i[c + E] = b - S << 24 | 64 << 16 | 0), o.bits = k, 0;
        };
      }, { "../utils/common": 41 }], 51: [function(e, t, r) {
        "use strict";
        t.exports = { 2: "need dictionary", 1: "stream end", 0: "", "-1": "file error", "-2": "stream error", "-3": "data error", "-4": "insufficient memory", "-5": "buffer error", "-6": "incompatible version" };
      }, {}], 52: [function(e, t, r) {
        "use strict";
        var i = e("../utils/common"), o = 0, h = 1;
        function n(e2) {
          for (var t2 = e2.length; 0 <= --t2; ) e2[t2] = 0;
        }
        var s = 0, a = 29, u = 256, l = u + 1 + a, f = 30, c = 19, _ = 2 * l + 1, g = 15, d = 16, p = 7, m = 256, b = 16, v = 17, y = 18, w = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0], k = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13], x = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7], S = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15], z = new Array(2 * (l + 2));
        n(z);
        var C = new Array(2 * f);
        n(C);
        var E = new Array(512);
        n(E);
        var A = new Array(256);
        n(A);
        var I = new Array(a);
        n(I);
        var O, B, R, T = new Array(f);
        function D(e2, t2, r2, n2, i2) {
          this.static_tree = e2, this.extra_bits = t2, this.extra_base = r2, this.elems = n2, this.max_length = i2, this.has_stree = e2 && e2.length;
        }
        function F(e2, t2) {
          this.dyn_tree = e2, this.max_code = 0, this.stat_desc = t2;
        }
        function N(e2) {
          return e2 < 256 ? E[e2] : E[256 + (e2 >>> 7)];
        }
        function U(e2, t2) {
          e2.pending_buf[e2.pending++] = 255 & t2, e2.pending_buf[e2.pending++] = t2 >>> 8 & 255;
        }
        function P(e2, t2, r2) {
          e2.bi_valid > d - r2 ? (e2.bi_buf |= t2 << e2.bi_valid & 65535, U(e2, e2.bi_buf), e2.bi_buf = t2 >> d - e2.bi_valid, e2.bi_valid += r2 - d) : (e2.bi_buf |= t2 << e2.bi_valid & 65535, e2.bi_valid += r2);
        }
        function L(e2, t2, r2) {
          P(e2, r2[2 * t2], r2[2 * t2 + 1]);
        }
        function j(e2, t2) {
          for (var r2 = 0; r2 |= 1 & e2, e2 >>>= 1, r2 <<= 1, 0 < --t2; ) ;
          return r2 >>> 1;
        }
        function Z(e2, t2, r2) {
          var n2, i2, s2 = new Array(g + 1), a2 = 0;
          for (n2 = 1; n2 <= g; n2++) s2[n2] = a2 = a2 + r2[n2 - 1] << 1;
          for (i2 = 0; i2 <= t2; i2++) {
            var o2 = e2[2 * i2 + 1];
            0 !== o2 && (e2[2 * i2] = j(s2[o2]++, o2));
          }
        }
        function W(e2) {
          var t2;
          for (t2 = 0; t2 < l; t2++) e2.dyn_ltree[2 * t2] = 0;
          for (t2 = 0; t2 < f; t2++) e2.dyn_dtree[2 * t2] = 0;
          for (t2 = 0; t2 < c; t2++) e2.bl_tree[2 * t2] = 0;
          e2.dyn_ltree[2 * m] = 1, e2.opt_len = e2.static_len = 0, e2.last_lit = e2.matches = 0;
        }
        function M(e2) {
          8 < e2.bi_valid ? U(e2, e2.bi_buf) : 0 < e2.bi_valid && (e2.pending_buf[e2.pending++] = e2.bi_buf), e2.bi_buf = 0, e2.bi_valid = 0;
        }
        function H(e2, t2, r2, n2) {
          var i2 = 2 * t2, s2 = 2 * r2;
          return e2[i2] < e2[s2] || e2[i2] === e2[s2] && n2[t2] <= n2[r2];
        }
        function G(e2, t2, r2) {
          for (var n2 = e2.heap[r2], i2 = r2 << 1; i2 <= e2.heap_len && (i2 < e2.heap_len && H(t2, e2.heap[i2 + 1], e2.heap[i2], e2.depth) && i2++, !H(t2, n2, e2.heap[i2], e2.depth)); ) e2.heap[r2] = e2.heap[i2], r2 = i2, i2 <<= 1;
          e2.heap[r2] = n2;
        }
        function K(e2, t2, r2) {
          var n2, i2, s2, a2, o2 = 0;
          if (0 !== e2.last_lit) for (; n2 = e2.pending_buf[e2.d_buf + 2 * o2] << 8 | e2.pending_buf[e2.d_buf + 2 * o2 + 1], i2 = e2.pending_buf[e2.l_buf + o2], o2++, 0 === n2 ? L(e2, i2, t2) : (L(e2, (s2 = A[i2]) + u + 1, t2), 0 !== (a2 = w[s2]) && P(e2, i2 -= I[s2], a2), L(e2, s2 = N(--n2), r2), 0 !== (a2 = k[s2]) && P(e2, n2 -= T[s2], a2)), o2 < e2.last_lit; ) ;
          L(e2, m, t2);
        }
        function Y(e2, t2) {
          var r2, n2, i2, s2 = t2.dyn_tree, a2 = t2.stat_desc.static_tree, o2 = t2.stat_desc.has_stree, h2 = t2.stat_desc.elems, u2 = -1;
          for (e2.heap_len = 0, e2.heap_max = _, r2 = 0; r2 < h2; r2++) 0 !== s2[2 * r2] ? (e2.heap[++e2.heap_len] = u2 = r2, e2.depth[r2] = 0) : s2[2 * r2 + 1] = 0;
          for (; e2.heap_len < 2; ) s2[2 * (i2 = e2.heap[++e2.heap_len] = u2 < 2 ? ++u2 : 0)] = 1, e2.depth[i2] = 0, e2.opt_len--, o2 && (e2.static_len -= a2[2 * i2 + 1]);
          for (t2.max_code = u2, r2 = e2.heap_len >> 1; 1 <= r2; r2--) G(e2, s2, r2);
          for (i2 = h2; r2 = e2.heap[1], e2.heap[1] = e2.heap[e2.heap_len--], G(e2, s2, 1), n2 = e2.heap[1], e2.heap[--e2.heap_max] = r2, e2.heap[--e2.heap_max] = n2, s2[2 * i2] = s2[2 * r2] + s2[2 * n2], e2.depth[i2] = (e2.depth[r2] >= e2.depth[n2] ? e2.depth[r2] : e2.depth[n2]) + 1, s2[2 * r2 + 1] = s2[2 * n2 + 1] = i2, e2.heap[1] = i2++, G(e2, s2, 1), 2 <= e2.heap_len; ) ;
          e2.heap[--e2.heap_max] = e2.heap[1], (function(e3, t3) {
            var r3, n3, i3, s3, a3, o3, h3 = t3.dyn_tree, u3 = t3.max_code, l2 = t3.stat_desc.static_tree, f2 = t3.stat_desc.has_stree, c2 = t3.stat_desc.extra_bits, d2 = t3.stat_desc.extra_base, p2 = t3.stat_desc.max_length, m2 = 0;
            for (s3 = 0; s3 <= g; s3++) e3.bl_count[s3] = 0;
            for (h3[2 * e3.heap[e3.heap_max] + 1] = 0, r3 = e3.heap_max + 1; r3 < _; r3++) p2 < (s3 = h3[2 * h3[2 * (n3 = e3.heap[r3]) + 1] + 1] + 1) && (s3 = p2, m2++), h3[2 * n3 + 1] = s3, u3 < n3 || (e3.bl_count[s3]++, a3 = 0, d2 <= n3 && (a3 = c2[n3 - d2]), o3 = h3[2 * n3], e3.opt_len += o3 * (s3 + a3), f2 && (e3.static_len += o3 * (l2[2 * n3 + 1] + a3)));
            if (0 !== m2) {
              do {
                for (s3 = p2 - 1; 0 === e3.bl_count[s3]; ) s3--;
                e3.bl_count[s3]--, e3.bl_count[s3 + 1] += 2, e3.bl_count[p2]--, m2 -= 2;
              } while (0 < m2);
              for (s3 = p2; 0 !== s3; s3--) for (n3 = e3.bl_count[s3]; 0 !== n3; ) u3 < (i3 = e3.heap[--r3]) || (h3[2 * i3 + 1] !== s3 && (e3.opt_len += (s3 - h3[2 * i3 + 1]) * h3[2 * i3], h3[2 * i3 + 1] = s3), n3--);
            }
          })(e2, t2), Z(s2, u2, e2.bl_count);
        }
        function X(e2, t2, r2) {
          var n2, i2, s2 = -1, a2 = t2[1], o2 = 0, h2 = 7, u2 = 4;
          for (0 === a2 && (h2 = 138, u2 = 3), t2[2 * (r2 + 1) + 1] = 65535, n2 = 0; n2 <= r2; n2++) i2 = a2, a2 = t2[2 * (n2 + 1) + 1], ++o2 < h2 && i2 === a2 || (o2 < u2 ? e2.bl_tree[2 * i2] += o2 : 0 !== i2 ? (i2 !== s2 && e2.bl_tree[2 * i2]++, e2.bl_tree[2 * b]++) : o2 <= 10 ? e2.bl_tree[2 * v]++ : e2.bl_tree[2 * y]++, s2 = i2, u2 = (o2 = 0) === a2 ? (h2 = 138, 3) : i2 === a2 ? (h2 = 6, 3) : (h2 = 7, 4));
        }
        function V(e2, t2, r2) {
          var n2, i2, s2 = -1, a2 = t2[1], o2 = 0, h2 = 7, u2 = 4;
          for (0 === a2 && (h2 = 138, u2 = 3), n2 = 0; n2 <= r2; n2++) if (i2 = a2, a2 = t2[2 * (n2 + 1) + 1], !(++o2 < h2 && i2 === a2)) {
            if (o2 < u2) for (; L(e2, i2, e2.bl_tree), 0 != --o2; ) ;
            else 0 !== i2 ? (i2 !== s2 && (L(e2, i2, e2.bl_tree), o2--), L(e2, b, e2.bl_tree), P(e2, o2 - 3, 2)) : o2 <= 10 ? (L(e2, v, e2.bl_tree), P(e2, o2 - 3, 3)) : (L(e2, y, e2.bl_tree), P(e2, o2 - 11, 7));
            s2 = i2, u2 = (o2 = 0) === a2 ? (h2 = 138, 3) : i2 === a2 ? (h2 = 6, 3) : (h2 = 7, 4);
          }
        }
        n(T);
        var q = false;
        function J(e2, t2, r2, n2) {
          P(e2, (s << 1) + (n2 ? 1 : 0), 3), (function(e3, t3, r3, n3) {
            M(e3), n3 && (U(e3, r3), U(e3, ~r3)), i.arraySet(e3.pending_buf, e3.window, t3, r3, e3.pending), e3.pending += r3;
          })(e2, t2, r2, true);
        }
        r._tr_init = function(e2) {
          q || ((function() {
            var e3, t2, r2, n2, i2, s2 = new Array(g + 1);
            for (n2 = r2 = 0; n2 < a - 1; n2++) for (I[n2] = r2, e3 = 0; e3 < 1 << w[n2]; e3++) A[r2++] = n2;
            for (A[r2 - 1] = n2, n2 = i2 = 0; n2 < 16; n2++) for (T[n2] = i2, e3 = 0; e3 < 1 << k[n2]; e3++) E[i2++] = n2;
            for (i2 >>= 7; n2 < f; n2++) for (T[n2] = i2 << 7, e3 = 0; e3 < 1 << k[n2] - 7; e3++) E[256 + i2++] = n2;
            for (t2 = 0; t2 <= g; t2++) s2[t2] = 0;
            for (e3 = 0; e3 <= 143; ) z[2 * e3 + 1] = 8, e3++, s2[8]++;
            for (; e3 <= 255; ) z[2 * e3 + 1] = 9, e3++, s2[9]++;
            for (; e3 <= 279; ) z[2 * e3 + 1] = 7, e3++, s2[7]++;
            for (; e3 <= 287; ) z[2 * e3 + 1] = 8, e3++, s2[8]++;
            for (Z(z, l + 1, s2), e3 = 0; e3 < f; e3++) C[2 * e3 + 1] = 5, C[2 * e3] = j(e3, 5);
            O = new D(z, w, u + 1, l, g), B = new D(C, k, 0, f, g), R = new D(new Array(0), x, 0, c, p);
          })(), q = true), e2.l_desc = new F(e2.dyn_ltree, O), e2.d_desc = new F(e2.dyn_dtree, B), e2.bl_desc = new F(e2.bl_tree, R), e2.bi_buf = 0, e2.bi_valid = 0, W(e2);
        }, r._tr_stored_block = J, r._tr_flush_block = function(e2, t2, r2, n2) {
          var i2, s2, a2 = 0;
          0 < e2.level ? (2 === e2.strm.data_type && (e2.strm.data_type = (function(e3) {
            var t3, r3 = 4093624447;
            for (t3 = 0; t3 <= 31; t3++, r3 >>>= 1) if (1 & r3 && 0 !== e3.dyn_ltree[2 * t3]) return o;
            if (0 !== e3.dyn_ltree[18] || 0 !== e3.dyn_ltree[20] || 0 !== e3.dyn_ltree[26]) return h;
            for (t3 = 32; t3 < u; t3++) if (0 !== e3.dyn_ltree[2 * t3]) return h;
            return o;
          })(e2)), Y(e2, e2.l_desc), Y(e2, e2.d_desc), a2 = (function(e3) {
            var t3;
            for (X(e3, e3.dyn_ltree, e3.l_desc.max_code), X(e3, e3.dyn_dtree, e3.d_desc.max_code), Y(e3, e3.bl_desc), t3 = c - 1; 3 <= t3 && 0 === e3.bl_tree[2 * S[t3] + 1]; t3--) ;
            return e3.opt_len += 3 * (t3 + 1) + 5 + 5 + 4, t3;
          })(e2), i2 = e2.opt_len + 3 + 7 >>> 3, (s2 = e2.static_len + 3 + 7 >>> 3) <= i2 && (i2 = s2)) : i2 = s2 = r2 + 5, r2 + 4 <= i2 && -1 !== t2 ? J(e2, t2, r2, n2) : 4 === e2.strategy || s2 === i2 ? (P(e2, 2 + (n2 ? 1 : 0), 3), K(e2, z, C)) : (P(e2, 4 + (n2 ? 1 : 0), 3), (function(e3, t3, r3, n3) {
            var i3;
            for (P(e3, t3 - 257, 5), P(e3, r3 - 1, 5), P(e3, n3 - 4, 4), i3 = 0; i3 < n3; i3++) P(e3, e3.bl_tree[2 * S[i3] + 1], 3);
            V(e3, e3.dyn_ltree, t3 - 1), V(e3, e3.dyn_dtree, r3 - 1);
          })(e2, e2.l_desc.max_code + 1, e2.d_desc.max_code + 1, a2 + 1), K(e2, e2.dyn_ltree, e2.dyn_dtree)), W(e2), n2 && M(e2);
        }, r._tr_tally = function(e2, t2, r2) {
          return e2.pending_buf[e2.d_buf + 2 * e2.last_lit] = t2 >>> 8 & 255, e2.pending_buf[e2.d_buf + 2 * e2.last_lit + 1] = 255 & t2, e2.pending_buf[e2.l_buf + e2.last_lit] = 255 & r2, e2.last_lit++, 0 === t2 ? e2.dyn_ltree[2 * r2]++ : (e2.matches++, t2--, e2.dyn_ltree[2 * (A[r2] + u + 1)]++, e2.dyn_dtree[2 * N(t2)]++), e2.last_lit === e2.lit_bufsize - 1;
        }, r._tr_align = function(e2) {
          P(e2, 2, 3), L(e2, m, z), (function(e3) {
            16 === e3.bi_valid ? (U(e3, e3.bi_buf), e3.bi_buf = 0, e3.bi_valid = 0) : 8 <= e3.bi_valid && (e3.pending_buf[e3.pending++] = 255 & e3.bi_buf, e3.bi_buf >>= 8, e3.bi_valid -= 8);
          })(e2);
        };
      }, { "../utils/common": 41 }], 53: [function(e, t, r) {
        "use strict";
        t.exports = function() {
          this.input = null, this.next_in = 0, this.avail_in = 0, this.total_in = 0, this.output = null, this.next_out = 0, this.avail_out = 0, this.total_out = 0, this.msg = "", this.state = null, this.data_type = 2, this.adler = 0;
        };
      }, {}], 54: [function(e, t, r) {
        (function(e2) {
          !(function(r2, n) {
            "use strict";
            if (!r2.setImmediate) {
              var i, s, t2, a, o = 1, h = {}, u = false, l = r2.document, e3 = Object.getPrototypeOf && Object.getPrototypeOf(r2);
              e3 = e3 && e3.setTimeout ? e3 : r2, i = "[object process]" === {}.toString.call(r2.process) ? function(e4) {
                process.nextTick(function() {
                  c(e4);
                });
              } : (function() {
                if (r2.postMessage && !r2.importScripts) {
                  var e4 = true, t3 = r2.onmessage;
                  return r2.onmessage = function() {
                    e4 = false;
                  }, r2.postMessage("", "*"), r2.onmessage = t3, e4;
                }
              })() ? (a = "setImmediate$" + Math.random() + "$", r2.addEventListener ? r2.addEventListener("message", d, false) : r2.attachEvent("onmessage", d), function(e4) {
                r2.postMessage(a + e4, "*");
              }) : r2.MessageChannel ? ((t2 = new MessageChannel()).port1.onmessage = function(e4) {
                c(e4.data);
              }, function(e4) {
                t2.port2.postMessage(e4);
              }) : l && "onreadystatechange" in l.createElement("script") ? (s = l.documentElement, function(e4) {
                var t3 = l.createElement("script");
                t3.onreadystatechange = function() {
                  c(e4), t3.onreadystatechange = null, s.removeChild(t3), t3 = null;
                }, s.appendChild(t3);
              }) : function(e4) {
                setTimeout(c, 0, e4);
              }, e3.setImmediate = function(e4) {
                "function" != typeof e4 && (e4 = new Function("" + e4));
                for (var t3 = new Array(arguments.length - 1), r3 = 0; r3 < t3.length; r3++) t3[r3] = arguments[r3 + 1];
                var n2 = { callback: e4, args: t3 };
                return h[o] = n2, i(o), o++;
              }, e3.clearImmediate = f;
            }
            function f(e4) {
              delete h[e4];
            }
            function c(e4) {
              if (u) setTimeout(c, 0, e4);
              else {
                var t3 = h[e4];
                if (t3) {
                  u = true;
                  try {
                    !(function(e5) {
                      var t4 = e5.callback, r3 = e5.args;
                      switch (r3.length) {
                        case 0:
                          t4();
                          break;
                        case 1:
                          t4(r3[0]);
                          break;
                        case 2:
                          t4(r3[0], r3[1]);
                          break;
                        case 3:
                          t4(r3[0], r3[1], r3[2]);
                          break;
                        default:
                          t4.apply(n, r3);
                      }
                    })(t3);
                  } finally {
                    f(e4), u = false;
                  }
                }
              }
            }
            function d(e4) {
              e4.source === r2 && "string" == typeof e4.data && 0 === e4.data.indexOf(a) && c(+e4.data.slice(a.length));
            }
          })("undefined" == typeof self ? void 0 === e2 ? this : e2 : self);
        }).call(this, "undefined" != typeof global ? global : "undefined" != typeof self ? self : "undefined" != typeof window ? window : {});
      }, {}] }, {}, [10])(10);
    });
  }
});

// archive/2026-05-18-cleanup/generated/templates/business-basic/node_modules/.pnpm/fast-xml-parser@5.8.0/node_modules/fast-xml-parser/lib/fxp.cjs
var require_fxp = __commonJS({
  "archive/2026-05-18-cleanup/generated/templates/business-basic/node_modules/.pnpm/fast-xml-parser@5.8.0/node_modules/fast-xml-parser/lib/fxp.cjs"(exports, module) {
    (() => {
      "use strict";
      var t = { d: (e2, n2) => {
        for (var i2 in n2) t.o(n2, i2) && !t.o(e2, i2) && Object.defineProperty(e2, i2, { enumerable: true, get: n2[i2] });
      }, o: (t2, e2) => Object.prototype.hasOwnProperty.call(t2, e2), r: (t2) => {
        "undefined" != typeof Symbol && Symbol.toStringTag && Object.defineProperty(t2, Symbol.toStringTag, { value: "Module" }), Object.defineProperty(t2, "__esModule", { value: true });
      } }, e = {};
      t.r(e), t.d(e, { XMLBuilder: () => ie, XMLParser: () => Lt, XMLValidator: () => se });
      const n = ":A-Za-z_\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD", i = new RegExp("^[" + n + "][" + n + "\\-.\\d\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$");
      function s(t2, e2) {
        const n2 = [];
        let i2 = e2.exec(t2);
        for (; i2; ) {
          const s2 = [];
          s2.startIndex = e2.lastIndex - i2[0].length;
          const r2 = i2.length;
          for (let t3 = 0; t3 < r2; t3++) s2.push(i2[t3]);
          n2.push(s2), i2 = e2.exec(t2);
        }
        return n2;
      }
      const r = function(t2) {
        return !(null == i.exec(t2));
      }, o = ["hasOwnProperty", "toString", "valueOf", "__defineGetter__", "__defineSetter__", "__lookupGetter__", "__lookupSetter__"], a = ["__proto__", "constructor", "prototype"], h = { allowBooleanAttributes: false, unpairedTags: [] };
      function l(t2, e2) {
        e2 = Object.assign({}, h, e2);
        const n2 = [];
        let i2 = false, s2 = false;
        "\uFEFF" === t2[0] && (t2 = t2.substr(1));
        for (let r2 = 0; r2 < t2.length; r2++) if ("<" === t2[r2] && "?" === t2[r2 + 1]) {
          if (r2 += 2, r2 = p(t2, r2), r2.err) return r2;
        } else {
          if ("<" !== t2[r2]) {
            if (u(t2[r2])) continue;
            return b("InvalidChar", "char '" + t2[r2] + "' is not expected.", w(t2, r2));
          }
          {
            let o2 = r2;
            if (r2++, "!" === t2[r2]) {
              r2 = c(t2, r2);
              continue;
            }
            {
              let a2 = false;
              "/" === t2[r2] && (a2 = true, r2++);
              let h2 = "";
              for (; r2 < t2.length && ">" !== t2[r2] && " " !== t2[r2] && "	" !== t2[r2] && "\n" !== t2[r2] && "\r" !== t2[r2]; r2++) h2 += t2[r2];
              if (h2 = h2.trim(), "/" === h2[h2.length - 1] && (h2 = h2.substring(0, h2.length - 1), r2--), !E(h2)) {
                let e3;
                return e3 = 0 === h2.trim().length ? "Invalid space after '<'." : "Tag '" + h2 + "' is an invalid name.", b("InvalidTag", e3, w(t2, r2));
              }
              const l2 = g(t2, r2);
              if (false === l2) return b("InvalidAttr", "Attributes for '" + h2 + "' have open quote.", w(t2, r2));
              let d2 = l2.value;
              if (r2 = l2.index, "/" === d2[d2.length - 1]) {
                const n3 = r2 - d2.length;
                d2 = d2.substring(0, d2.length - 1);
                const s3 = x(d2, e2);
                if (true !== s3) return b(s3.err.code, s3.err.msg, w(t2, n3 + s3.err.line));
                i2 = true;
              } else if (a2) {
                if (!l2.tagClosed) return b("InvalidTag", "Closing tag '" + h2 + "' doesn't have proper closing.", w(t2, r2));
                if (d2.trim().length > 0) return b("InvalidTag", "Closing tag '" + h2 + "' can't have attributes or invalid starting.", w(t2, o2));
                if (0 === n2.length) return b("InvalidTag", "Closing tag '" + h2 + "' has not been opened.", w(t2, o2));
                {
                  const e3 = n2.pop();
                  if (h2 !== e3.tagName) {
                    let n3 = w(t2, e3.tagStartPos);
                    return b("InvalidTag", "Expected closing tag '" + e3.tagName + "' (opened in line " + n3.line + ", col " + n3.col + ") instead of closing tag '" + h2 + "'.", w(t2, o2));
                  }
                  0 == n2.length && (s2 = true);
                }
              } else {
                const a3 = x(d2, e2);
                if (true !== a3) return b(a3.err.code, a3.err.msg, w(t2, r2 - d2.length + a3.err.line));
                if (true === s2) return b("InvalidXml", "Multiple possible root nodes found.", w(t2, r2));
                -1 !== e2.unpairedTags.indexOf(h2) || n2.push({ tagName: h2, tagStartPos: o2 }), i2 = true;
              }
              for (r2++; r2 < t2.length; r2++) if ("<" === t2[r2]) {
                if ("!" === t2[r2 + 1]) {
                  r2++, r2 = c(t2, r2);
                  continue;
                }
                if ("?" !== t2[r2 + 1]) break;
                if (r2 = p(t2, ++r2), r2.err) return r2;
              } else if ("&" === t2[r2]) {
                const e3 = N(t2, r2);
                if (-1 == e3) return b("InvalidChar", "char '&' is not expected.", w(t2, r2));
                r2 = e3;
              } else if (true === s2 && !u(t2[r2])) return b("InvalidXml", "Extra text at the end", w(t2, r2));
              "<" === t2[r2] && r2--;
            }
          }
        }
        return i2 ? 1 == n2.length ? b("InvalidTag", "Unclosed tag '" + n2[0].tagName + "'.", w(t2, n2[0].tagStartPos)) : !(n2.length > 0) || b("InvalidXml", "Invalid '" + JSON.stringify(n2.map((t3) => t3.tagName), null, 4).replace(/\r?\n/g, "") + "' found.", { line: 1, col: 1 }) : b("InvalidXml", "Start tag expected.", 1);
      }
      function u(t2) {
        return " " === t2 || "	" === t2 || "\n" === t2 || "\r" === t2;
      }
      function p(t2, e2) {
        const n2 = e2;
        for (; e2 < t2.length; e2++) if ("?" == t2[e2] || " " == t2[e2]) {
          const i2 = t2.substr(n2, e2 - n2);
          if (e2 > 5 && "xml" === i2) return b("InvalidXml", "XML declaration allowed only at the start of the document.", w(t2, e2));
          if ("?" == t2[e2] && ">" == t2[e2 + 1]) {
            e2++;
            break;
          }
          continue;
        }
        return e2;
      }
      function c(t2, e2) {
        if (t2.length > e2 + 5 && "-" === t2[e2 + 1] && "-" === t2[e2 + 2]) {
          for (e2 += 3; e2 < t2.length; e2++) if ("-" === t2[e2] && "-" === t2[e2 + 1] && ">" === t2[e2 + 2]) {
            e2 += 2;
            break;
          }
        } else if (t2.length > e2 + 8 && "D" === t2[e2 + 1] && "O" === t2[e2 + 2] && "C" === t2[e2 + 3] && "T" === t2[e2 + 4] && "Y" === t2[e2 + 5] && "P" === t2[e2 + 6] && "E" === t2[e2 + 7]) {
          let n2 = 1;
          for (e2 += 8; e2 < t2.length; e2++) if ("<" === t2[e2]) n2++;
          else if (">" === t2[e2] && (n2--, 0 === n2)) break;
        } else if (t2.length > e2 + 9 && "[" === t2[e2 + 1] && "C" === t2[e2 + 2] && "D" === t2[e2 + 3] && "A" === t2[e2 + 4] && "T" === t2[e2 + 5] && "A" === t2[e2 + 6] && "[" === t2[e2 + 7]) {
          for (e2 += 8; e2 < t2.length; e2++) if ("]" === t2[e2] && "]" === t2[e2 + 1] && ">" === t2[e2 + 2]) {
            e2 += 2;
            break;
          }
        }
        return e2;
      }
      const d = '"', f = "'";
      function g(t2, e2) {
        let n2 = "", i2 = "", s2 = false;
        for (; e2 < t2.length; e2++) {
          if (t2[e2] === d || t2[e2] === f) "" === i2 ? i2 = t2[e2] : i2 !== t2[e2] || (i2 = "");
          else if (">" === t2[e2] && "" === i2) {
            s2 = true;
            break;
          }
          n2 += t2[e2];
        }
        return "" === i2 && { value: n2, index: e2, tagClosed: s2 };
      }
      const m = new RegExp(`(\\s*)([^\\s=]+)(\\s*=)?(\\s*(['"])(([\\s\\S])*?)\\5)?`, "g");
      function x(t2, e2) {
        const n2 = s(t2, m), i2 = {};
        for (let t3 = 0; t3 < n2.length; t3++) {
          if (0 === n2[t3][1].length) return b("InvalidAttr", "Attribute '" + n2[t3][2] + "' has no space in starting.", v(n2[t3]));
          if (void 0 !== n2[t3][3] && void 0 === n2[t3][4]) return b("InvalidAttr", "Attribute '" + n2[t3][2] + "' is without value.", v(n2[t3]));
          if (void 0 === n2[t3][3] && !e2.allowBooleanAttributes) return b("InvalidAttr", "boolean attribute '" + n2[t3][2] + "' is not allowed.", v(n2[t3]));
          const s2 = n2[t3][2];
          if (!y(s2)) return b("InvalidAttr", "Attribute '" + s2 + "' is an invalid name.", v(n2[t3]));
          if (Object.prototype.hasOwnProperty.call(i2, s2)) return b("InvalidAttr", "Attribute '" + s2 + "' is repeated.", v(n2[t3]));
          i2[s2] = 1;
        }
        return true;
      }
      function N(t2, e2) {
        if (";" === t2[++e2]) return -1;
        if ("#" === t2[e2]) return (function(t3, e3) {
          let n3 = /\d/;
          for ("x" === t3[e3] && (e3++, n3 = /[\da-fA-F]/); e3 < t3.length; e3++) {
            if (";" === t3[e3]) return e3;
            if (!t3[e3].match(n3)) break;
          }
          return -1;
        })(t2, ++e2);
        let n2 = 0;
        for (; e2 < t2.length; e2++, n2++) if (!(t2[e2].match(/\w/) && n2 < 20)) {
          if (";" === t2[e2]) break;
          return -1;
        }
        return e2;
      }
      function b(t2, e2, n2) {
        return { err: { code: t2, msg: e2, line: n2.line || n2, col: n2.col } };
      }
      function y(t2) {
        return r(t2);
      }
      function E(t2) {
        return r(t2);
      }
      function w(t2, e2) {
        const n2 = t2.substring(0, e2).split(/\r?\n/);
        return { line: n2.length, col: n2[n2.length - 1].length + 1 };
      }
      function v(t2) {
        return t2.startIndex + t2[1].length;
      }
      const S = (t2) => o.includes(t2) ? "__" + t2 : t2, _ = { preserveOrder: false, attributeNamePrefix: "@_", attributesGroupName: false, textNodeName: "#text", ignoreAttributes: true, removeNSPrefix: false, allowBooleanAttributes: false, parseTagValue: true, parseAttributeValue: false, trimValues: true, cdataPropName: false, numberParseOptions: { hex: true, leadingZeros: true, eNotation: true }, tagValueProcessor: function(t2, e2) {
        return e2;
      }, attributeValueProcessor: function(t2, e2) {
        return e2;
      }, stopNodes: [], alwaysCreateTextNode: false, isArray: () => false, commentPropName: false, unpairedTags: [], processEntities: true, htmlEntities: false, entityDecoder: null, ignoreDeclaration: false, ignorePiTags: false, transformTagName: false, transformAttributeName: false, updateTag: function(t2, e2, n2) {
        return t2;
      }, captureMetaData: false, maxNestedTags: 100, strictReservedNames: true, jPath: true, onDangerousProperty: S };
      function A(t2, e2) {
        if ("string" != typeof t2) return;
        const n2 = t2.toLowerCase();
        if (o.some((t3) => n2 === t3.toLowerCase())) throw new Error(`[SECURITY] Invalid ${e2}: "${t2}" is a reserved JavaScript keyword that could cause prototype pollution`);
        if (a.some((t3) => n2 === t3.toLowerCase())) throw new Error(`[SECURITY] Invalid ${e2}: "${t2}" is a reserved JavaScript keyword that could cause prototype pollution`);
      }
      function T(t2, e2) {
        return "boolean" == typeof t2 ? { enabled: t2, maxEntitySize: 1e4, maxExpansionDepth: 1e4, maxTotalExpansions: 1 / 0, maxExpandedLength: 1e5, maxEntityCount: 1e3, allowedTags: null, tagFilter: null, appliesTo: "all" } : "object" == typeof t2 && null !== t2 ? { enabled: false !== t2.enabled, maxEntitySize: Math.max(1, t2.maxEntitySize ?? 1e4), maxExpansionDepth: Math.max(1, t2.maxExpansionDepth ?? 1e4), maxTotalExpansions: Math.max(1, t2.maxTotalExpansions ?? 1 / 0), maxExpandedLength: Math.max(1, t2.maxExpandedLength ?? 1e5), maxEntityCount: Math.max(1, t2.maxEntityCount ?? 1e3), allowedTags: t2.allowedTags ?? null, tagFilter: t2.tagFilter ?? null, appliesTo: t2.appliesTo ?? "all" } : T(true);
      }
      const C = function(t2) {
        const e2 = Object.assign({}, _, t2), n2 = [{ value: e2.attributeNamePrefix, name: "attributeNamePrefix" }, { value: e2.attributesGroupName, name: "attributesGroupName" }, { value: e2.textNodeName, name: "textNodeName" }, { value: e2.cdataPropName, name: "cdataPropName" }, { value: e2.commentPropName, name: "commentPropName" }];
        for (const { value: t3, name: e3 } of n2) t3 && A(t3, e3);
        return null === e2.onDangerousProperty && (e2.onDangerousProperty = S), e2.processEntities = T(e2.processEntities, e2.htmlEntities), e2.unpairedTagsSet = new Set(e2.unpairedTags), e2.stopNodes && Array.isArray(e2.stopNodes) && (e2.stopNodes = e2.stopNodes.map((t3) => "string" == typeof t3 && t3.startsWith("*.") ? ".." + t3.substring(2) : t3)), e2;
      };
      let P;
      P = "function" != typeof Symbol ? "@@xmlMetadata" : /* @__PURE__ */ Symbol("XML Node Metadata");
      class $ {
        constructor(t2) {
          this.tagname = t2, this.child = [], this[":@"] = /* @__PURE__ */ Object.create(null);
        }
        add(t2, e2) {
          "__proto__" === t2 && (t2 = "#__proto__"), this.child.push({ [t2]: e2 });
        }
        addChild(t2, e2) {
          "__proto__" === t2.tagname && (t2.tagname = "#__proto__"), t2[":@"] && Object.keys(t2[":@"]).length > 0 ? this.child.push({ [t2.tagname]: t2.child, ":@": t2[":@"] }) : this.child.push({ [t2.tagname]: t2.child }), void 0 !== e2 && (this.child[this.child.length - 1][P] = { startIndex: e2 });
        }
        static getMetaDataSymbol() {
          return P;
        }
      }
      const O = ":A-Za-z_\xC0-\xD6\xD8-\xF6\xF8-\u02FF\u0370-\u037D\u037F-\u0486\u0488-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD", I = ":A-Za-z_\xC0-\u02FF\u0370-\u037D\u037F-\u0486\u0488-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD\u{10000}-\u{EFFFF}", V = I + "\\-\\.\\d\xB7\u0300-\u036F\u0487\u203F-\u2040", D = (t2, e2, n2 = "") => {
        const i2 = `[${t2.replace(":", "")}][${e2.replace(":", "")}]*`;
        return { name: new RegExp(`^[${t2}][${e2}]*$`, n2), ncName: new RegExp(`^${i2}$`, n2), qName: new RegExp(`^${i2}(?::${i2})?$`, n2), nmToken: new RegExp(`^[${e2}]+$`, n2), nmTokens: new RegExp(`^[${e2}]+(?:\\s+[${e2}]+)*$`, n2) };
      }, M = D(O, O + "\\-\\.\\d\xB7\u0300-\u036F\u203F-\u2040"), j = D(I, V, "u"), L = (t2, { xmlVersion: e2 = "1.0" } = {}) => (/* @__PURE__ */ ((t3 = "1.0") => "1.1" === t3 ? j : M)(e2)).qName.test(t2);
      class k {
        constructor(t2, e2) {
          this.suppressValidationErr = !t2, this.options = t2, this.xmlVersion = e2 || 1;
        }
        setXmlVersion(t2 = 1) {
          this.xmlVersion = t2;
        }
        readDocType(t2, e2) {
          const n2 = /* @__PURE__ */ Object.create(null);
          let i2 = 0;
          if ("O" !== t2[e2 + 3] || "C" !== t2[e2 + 4] || "T" !== t2[e2 + 5] || "Y" !== t2[e2 + 6] || "P" !== t2[e2 + 7] || "E" !== t2[e2 + 8]) throw new Error("Invalid Tag instead of DOCTYPE");
          {
            e2 += 9;
            let s2 = 1, r2 = false, o2 = false, a2 = "";
            for (; e2 < t2.length; e2++) if ("<" !== t2[e2] || o2) if (">" === t2[e2]) {
              if (o2 ? "-" === t2[e2 - 1] && "-" === t2[e2 - 2] && (o2 = false, s2--) : s2--, 0 === s2) break;
            } else "[" === t2[e2] ? r2 = true : a2 += t2[e2];
            else {
              if (r2 && F(t2, "!ENTITY", e2)) {
                let s3, r3;
                if (e2 += 7, [s3, r3, e2] = this.readEntityExp(t2, e2 + 1, this.suppressValidationErr), -1 === r3.indexOf("&")) {
                  if (false !== this.options.enabled && null != this.options.maxEntityCount && i2 >= this.options.maxEntityCount) throw new Error(`Entity count (${i2 + 1}) exceeds maximum allowed (${this.options.maxEntityCount})`);
                  n2[s3] = r3, i2++;
                }
              } else if (r2 && F(t2, "!ELEMENT", e2)) {
                e2 += 8;
                const { index: n3 } = this.readElementExp(t2, e2 + 1);
                e2 = n3;
              } else if (r2 && F(t2, "!ATTLIST", e2)) e2 += 8;
              else if (r2 && F(t2, "!NOTATION", e2)) {
                e2 += 9;
                const { index: n3 } = this.readNotationExp(t2, e2 + 1, this.suppressValidationErr);
                e2 = n3;
              } else {
                if (!F(t2, "!--", e2)) throw new Error("Invalid DOCTYPE");
                o2 = true;
              }
              s2++, a2 = "";
            }
            if (0 !== s2) throw new Error("Unclosed DOCTYPE");
          }
          return { entities: n2, i: e2 };
        }
        readEntityExp(t2, e2) {
          const n2 = e2 = R(t2, e2);
          for (; e2 < t2.length && !/\s/.test(t2[e2]) && '"' !== t2[e2] && "'" !== t2[e2]; ) e2++;
          let i2 = t2.substring(n2, e2);
          if (G(i2, { xmlVersion: this.xmlVersion }), e2 = R(t2, e2), !this.suppressValidationErr) {
            if ("SYSTEM" === t2.substring(e2, e2 + 6).toUpperCase()) throw new Error("External entities are not supported");
            if ("%" === t2[e2]) throw new Error("Parameter entities are not supported");
          }
          let s2 = "";
          if ([e2, s2] = this.readIdentifierVal(t2, e2, "entity"), false !== this.options.enabled && null != this.options.maxEntitySize && s2.length > this.options.maxEntitySize) throw new Error(`Entity "${i2}" size (${s2.length}) exceeds maximum allowed size (${this.options.maxEntitySize})`);
          return [i2, s2, --e2];
        }
        readNotationExp(t2, e2) {
          const n2 = e2 = R(t2, e2);
          for (; e2 < t2.length && !/\s/.test(t2[e2]); ) e2++;
          let i2 = t2.substring(n2, e2);
          !this.suppressValidationErr && G(i2, { xmlVersion: this.xmlVersion }), e2 = R(t2, e2);
          const s2 = t2.substring(e2, e2 + 6).toUpperCase();
          if (!this.suppressValidationErr && "SYSTEM" !== s2 && "PUBLIC" !== s2) throw new Error(`Expected SYSTEM or PUBLIC, found "${s2}"`);
          e2 += s2.length, e2 = R(t2, e2);
          let r2 = null, o2 = null;
          if ("PUBLIC" === s2) [e2, r2] = this.readIdentifierVal(t2, e2, "publicIdentifier"), '"' !== t2[e2 = R(t2, e2)] && "'" !== t2[e2] || ([e2, o2] = this.readIdentifierVal(t2, e2, "systemIdentifier"));
          else if ("SYSTEM" === s2 && ([e2, o2] = this.readIdentifierVal(t2, e2, "systemIdentifier"), !this.suppressValidationErr && !o2)) throw new Error("Missing mandatory system identifier for SYSTEM notation");
          return { notationName: i2, publicIdentifier: r2, systemIdentifier: o2, index: --e2 };
        }
        readIdentifierVal(t2, e2, n2) {
          let i2 = "";
          const s2 = t2[e2];
          if ('"' !== s2 && "'" !== s2) throw new Error(`Expected quoted string, found "${s2}"`);
          const r2 = ++e2;
          for (; e2 < t2.length && t2[e2] !== s2; ) e2++;
          if (i2 = t2.substring(r2, e2), t2[e2] !== s2) throw new Error(`Unterminated ${n2} value`);
          return [++e2, i2];
        }
        readElementExp(t2, e2) {
          const n2 = e2 = R(t2, e2);
          for (; e2 < t2.length && !/\s/.test(t2[e2]); ) e2++;
          let i2 = t2.substring(n2, e2);
          if (!this.suppressValidationErr && !L(i2, { xmlVersion: this.xmlVersion })) throw new Error(`Invalid element name: "${i2}"`);
          let s2 = "";
          if ("E" === t2[e2 = R(t2, e2)] && F(t2, "MPTY", e2)) e2 += 4;
          else if ("A" === t2[e2] && F(t2, "NY", e2)) e2 += 2;
          else if ("(" === t2[e2]) {
            const n3 = ++e2;
            for (; e2 < t2.length && ")" !== t2[e2]; ) e2++;
            if (s2 = t2.substring(n3, e2), ")" !== t2[e2]) throw new Error("Unterminated content model");
          } else if (!this.suppressValidationErr) throw new Error(`Invalid Element Expression, found "${t2[e2]}"`);
          return { elementName: i2, contentModel: s2.trim(), index: e2 };
        }
        readAttlistExp(t2, e2) {
          let n2 = e2 = R(t2, e2);
          for (; e2 < t2.length && !/\s/.test(t2[e2]); ) e2++;
          let i2 = t2.substring(n2, e2);
          for (G(i2, { xmlVersion: this.xmlVersion }), n2 = e2 = R(t2, e2); e2 < t2.length && !/\s/.test(t2[e2]); ) e2++;
          let s2 = t2.substring(n2, e2);
          if (!G(s2, { xmlVersion: this.xmlVersion })) throw new Error(`Invalid attribute name: "${s2}"`);
          e2 = R(t2, e2);
          let r2 = "";
          if ("NOTATION" === t2.substring(e2, e2 + 8).toUpperCase()) {
            if (r2 = "NOTATION", "(" !== t2[e2 = R(t2, e2 += 8)]) throw new Error(`Expected '(', found "${t2[e2]}"`);
            e2++;
            let n3 = [];
            for (; e2 < t2.length && ")" !== t2[e2]; ) {
              const i3 = e2;
              for (; e2 < t2.length && "|" !== t2[e2] && ")" !== t2[e2]; ) e2++;
              let s3 = t2.substring(i3, e2);
              if (s3 = s3.trim(), !G(s3, { xmlVersion: this.xmlVersion })) throw new Error(`Invalid notation name: "${s3}"`);
              n3.push(s3), "|" === t2[e2] && (e2++, e2 = R(t2, e2));
            }
            if (")" !== t2[e2]) throw new Error("Unterminated list of notations");
            e2++, r2 += " (" + n3.join("|") + ")";
          } else {
            const n3 = e2;
            for (; e2 < t2.length && !/\s/.test(t2[e2]); ) e2++;
            r2 += t2.substring(n3, e2);
            const i3 = ["CDATA", "ID", "IDREF", "IDREFS", "ENTITY", "ENTITIES", "NMTOKEN", "NMTOKENS"];
            if (!this.suppressValidationErr && !i3.includes(r2.toUpperCase())) throw new Error(`Invalid attribute type: "${r2}"`);
          }
          e2 = R(t2, e2);
          let o2 = "";
          return "#REQUIRED" === t2.substring(e2, e2 + 8).toUpperCase() ? (o2 = "#REQUIRED", e2 += 8) : "#IMPLIED" === t2.substring(e2, e2 + 7).toUpperCase() ? (o2 = "#IMPLIED", e2 += 7) : [e2, o2] = this.readIdentifierVal(t2, e2, "ATTLIST"), { elementName: i2, attributeName: s2, attributeType: r2, defaultValue: o2, index: e2 };
        }
      }
      const R = (t2, e2) => {
        for (; e2 < t2.length && /\s/.test(t2[e2]); ) e2++;
        return e2;
      };
      function F(t2, e2, n2) {
        for (let i2 = 0; i2 < e2.length; i2++) if (e2[i2] !== t2[n2 + i2 + 1]) return false;
        return true;
      }
      function G(t2, e2) {
        if (L(t2, { xmlVersion: e2 })) return t2;
        throw new Error(`Invalid entity name ${t2}`);
      }
      const U = /^[-+]?0x[a-fA-F0-9]+$/, B = /^0b[01]+$/, W = /^0o[0-7]+$/, z = /^([\-\+])?(0*)([0-9]*(\.[0-9]*)?)$/, X = { hex: true, binary: false, octal: false, leadingZeros: true, decimalPoint: ".", eNotation: true, infinity: "original" };
      const Y = /^([-+])?(0*)(\d*(\.\d*)?[eE][-\+]?\d+)$/;
      function q(t2, e2) {
        const n2 = t2.trim();
        if (2 !== e2 && 8 !== e2 || (t2 = n2.substring(2)), parseInt) return parseInt(t2, e2);
        if (Number.parseInt) return Number.parseInt(t2, e2);
        if (window && window.parseInt) return window.parseInt(t2, e2);
        throw new Error("parseInt, Number.parseInt, window.parseInt are not supported");
      }
      class Z {
        constructor(t2) {
          this._matcher = t2;
        }
        get separator() {
          return this._matcher.separator;
        }
        getCurrentTag() {
          const t2 = this._matcher.path;
          return t2.length > 0 ? t2[t2.length - 1].tag : void 0;
        }
        getCurrentNamespace() {
          const t2 = this._matcher.path;
          return t2.length > 0 ? t2[t2.length - 1].namespace : void 0;
        }
        getAttrValue(t2) {
          const e2 = this._matcher.path;
          if (0 !== e2.length) return e2[e2.length - 1].values?.[t2];
        }
        hasAttr(t2) {
          const e2 = this._matcher.path;
          if (0 === e2.length) return false;
          const n2 = e2[e2.length - 1];
          return void 0 !== n2.values && t2 in n2.values;
        }
        getPosition() {
          const t2 = this._matcher.path;
          return 0 === t2.length ? -1 : t2[t2.length - 1].position ?? 0;
        }
        getCounter() {
          const t2 = this._matcher.path;
          return 0 === t2.length ? -1 : t2[t2.length - 1].counter ?? 0;
        }
        getIndex() {
          return this.getPosition();
        }
        getDepth() {
          return this._matcher.path.length;
        }
        toString(t2, e2 = true) {
          return this._matcher.toString(t2, e2);
        }
        toArray() {
          return this._matcher.path.map((t2) => t2.tag);
        }
        matches(t2) {
          return this._matcher.matches(t2);
        }
        matchesAny(t2) {
          return t2.matchesAny(this._matcher);
        }
      }
      class J {
        constructor(t2 = {}) {
          this.separator = t2.separator || ".", this.path = [], this.siblingStacks = [], this._pathStringCache = null, this._view = new Z(this);
        }
        push(t2, e2 = null, n2 = null) {
          this._pathStringCache = null, this.path.length > 0 && (this.path[this.path.length - 1].values = void 0);
          const i2 = this.path.length;
          this.siblingStacks[i2] || (this.siblingStacks[i2] = /* @__PURE__ */ new Map());
          const s2 = this.siblingStacks[i2], r2 = n2 ? `${n2}:${t2}` : t2, o2 = s2.get(r2) || 0;
          let a2 = 0;
          for (const t3 of s2.values()) a2 += t3;
          s2.set(r2, o2 + 1);
          const h2 = { tag: t2, position: a2, counter: o2 };
          null != n2 && (h2.namespace = n2), null != e2 && (h2.values = e2), this.path.push(h2);
        }
        pop() {
          if (0 === this.path.length) return;
          this._pathStringCache = null;
          const t2 = this.path.pop();
          return this.siblingStacks.length > this.path.length + 1 && (this.siblingStacks.length = this.path.length + 1), t2;
        }
        updateCurrent(t2) {
          if (this.path.length > 0) {
            const e2 = this.path[this.path.length - 1];
            null != t2 && (e2.values = t2);
          }
        }
        getCurrentTag() {
          return this.path.length > 0 ? this.path[this.path.length - 1].tag : void 0;
        }
        getCurrentNamespace() {
          return this.path.length > 0 ? this.path[this.path.length - 1].namespace : void 0;
        }
        getAttrValue(t2) {
          if (0 !== this.path.length) return this.path[this.path.length - 1].values?.[t2];
        }
        hasAttr(t2) {
          if (0 === this.path.length) return false;
          const e2 = this.path[this.path.length - 1];
          return void 0 !== e2.values && t2 in e2.values;
        }
        getPosition() {
          return 0 === this.path.length ? -1 : this.path[this.path.length - 1].position ?? 0;
        }
        getCounter() {
          return 0 === this.path.length ? -1 : this.path[this.path.length - 1].counter ?? 0;
        }
        getIndex() {
          return this.getPosition();
        }
        getDepth() {
          return this.path.length;
        }
        toString(t2, e2 = true) {
          const n2 = t2 || this.separator;
          if (n2 === this.separator && true === e2) {
            if (null !== this._pathStringCache) return this._pathStringCache;
            const t3 = this.path.map((t4) => t4.namespace ? `${t4.namespace}:${t4.tag}` : t4.tag).join(n2);
            return this._pathStringCache = t3, t3;
          }
          return this.path.map((t3) => e2 && t3.namespace ? `${t3.namespace}:${t3.tag}` : t3.tag).join(n2);
        }
        toArray() {
          return this.path.map((t2) => t2.tag);
        }
        reset() {
          this._pathStringCache = null, this.path = [], this.siblingStacks = [];
        }
        matches(t2) {
          const e2 = t2.segments;
          return 0 !== e2.length && (t2.hasDeepWildcard() ? this._matchWithDeepWildcard(e2) : this._matchSimple(e2));
        }
        _matchSimple(t2) {
          if (this.path.length !== t2.length) return false;
          for (let e2 = 0; e2 < t2.length; e2++) if (!this._matchSegment(t2[e2], this.path[e2], e2 === this.path.length - 1)) return false;
          return true;
        }
        _matchWithDeepWildcard(t2) {
          let e2 = this.path.length - 1, n2 = t2.length - 1;
          for (; n2 >= 0 && e2 >= 0; ) {
            const i2 = t2[n2];
            if ("deep-wildcard" === i2.type) {
              if (n2--, n2 < 0) return true;
              const i3 = t2[n2];
              let s2 = false;
              for (let t3 = e2; t3 >= 0; t3--) if (this._matchSegment(i3, this.path[t3], t3 === this.path.length - 1)) {
                e2 = t3 - 1, n2--, s2 = true;
                break;
              }
              if (!s2) return false;
            } else {
              if (!this._matchSegment(i2, this.path[e2], e2 === this.path.length - 1)) return false;
              e2--, n2--;
            }
          }
          return n2 < 0;
        }
        _matchSegment(t2, e2, n2) {
          if ("*" !== t2.tag && t2.tag !== e2.tag) return false;
          if (void 0 !== t2.namespace && "*" !== t2.namespace && t2.namespace !== e2.namespace) return false;
          if (void 0 !== t2.attrName) {
            if (!n2) return false;
            if (!e2.values || !(t2.attrName in e2.values)) return false;
            if (void 0 !== t2.attrValue && String(e2.values[t2.attrName]) !== String(t2.attrValue)) return false;
          }
          if (void 0 !== t2.position) {
            if (!n2) return false;
            const i2 = e2.counter ?? 0;
            if ("first" === t2.position && 0 !== i2) return false;
            if ("odd" === t2.position && i2 % 2 != 1) return false;
            if ("even" === t2.position && i2 % 2 != 0) return false;
            if ("nth" === t2.position && i2 !== t2.positionValue) return false;
          }
          return true;
        }
        matchesAny(t2) {
          return t2.matchesAny(this);
        }
        snapshot() {
          return { path: this.path.map((t2) => ({ ...t2 })), siblingStacks: this.siblingStacks.map((t2) => new Map(t2)) };
        }
        restore(t2) {
          this._pathStringCache = null, this.path = t2.path.map((t3) => ({ ...t3 })), this.siblingStacks = t2.siblingStacks.map((t3) => new Map(t3));
        }
        readOnly() {
          return this._view;
        }
      }
      class K {
        constructor(t2, e2 = {}, n2) {
          this.pattern = t2, this.separator = e2.separator || ".", this.segments = this._parse(t2), this.data = n2, this._hasDeepWildcard = this.segments.some((t3) => "deep-wildcard" === t3.type), this._hasAttributeCondition = this.segments.some((t3) => void 0 !== t3.attrName), this._hasPositionSelector = this.segments.some((t3) => void 0 !== t3.position);
        }
        _parse(t2) {
          const e2 = [];
          let n2 = 0, i2 = "";
          for (; n2 < t2.length; ) t2[n2] === this.separator ? n2 + 1 < t2.length && t2[n2 + 1] === this.separator ? (i2.trim() && (e2.push(this._parseSegment(i2.trim())), i2 = ""), e2.push({ type: "deep-wildcard" }), n2 += 2) : (i2.trim() && e2.push(this._parseSegment(i2.trim())), i2 = "", n2++) : (i2 += t2[n2], n2++);
          return i2.trim() && e2.push(this._parseSegment(i2.trim())), e2;
        }
        _parseSegment(t2) {
          const e2 = { type: "tag" };
          let n2 = null, i2 = t2;
          const s2 = t2.match(/^([^\[]+)(\[[^\]]*\])(.*)$/);
          if (s2 && (i2 = s2[1] + s2[3], s2[2])) {
            const t3 = s2[2].slice(1, -1);
            t3 && (n2 = t3);
          }
          let r2, o2, a2 = i2;
          if (i2.includes("::")) {
            const e3 = i2.indexOf("::");
            if (r2 = i2.substring(0, e3).trim(), a2 = i2.substring(e3 + 2).trim(), !r2) throw new Error(`Invalid namespace in pattern: ${t2}`);
          }
          let h2 = null;
          if (a2.includes(":")) {
            const t3 = a2.lastIndexOf(":"), e3 = a2.substring(0, t3).trim(), n3 = a2.substring(t3 + 1).trim();
            ["first", "last", "odd", "even"].includes(n3) || /^nth\(\d+\)$/.test(n3) ? (o2 = e3, h2 = n3) : o2 = a2;
          } else o2 = a2;
          if (!o2) throw new Error(`Invalid segment pattern: ${t2}`);
          if (e2.tag = o2, r2 && (e2.namespace = r2), n2) if (n2.includes("=")) {
            const t3 = n2.indexOf("=");
            e2.attrName = n2.substring(0, t3).trim(), e2.attrValue = n2.substring(t3 + 1).trim();
          } else e2.attrName = n2.trim();
          if (h2) {
            const t3 = h2.match(/^nth\((\d+)\)$/);
            t3 ? (e2.position = "nth", e2.positionValue = parseInt(t3[1], 10)) : e2.position = h2;
          }
          return e2;
        }
        get length() {
          return this.segments.length;
        }
        hasDeepWildcard() {
          return this._hasDeepWildcard;
        }
        hasAttributeCondition() {
          return this._hasAttributeCondition;
        }
        hasPositionSelector() {
          return this._hasPositionSelector;
        }
        toString() {
          return this.pattern;
        }
      }
      class Q {
        constructor() {
          this._byDepthAndTag = /* @__PURE__ */ new Map(), this._wildcardByDepth = /* @__PURE__ */ new Map(), this._deepWildcards = [], this._patterns = /* @__PURE__ */ new Set(), this._sealed = false;
        }
        add(t2) {
          if (this._sealed) throw new TypeError("ExpressionSet is sealed. Create a new ExpressionSet to add more expressions.");
          if (this._patterns.has(t2.pattern)) return this;
          if (this._patterns.add(t2.pattern), t2.hasDeepWildcard()) return this._deepWildcards.push(t2), this;
          const e2 = t2.length, n2 = t2.segments[t2.segments.length - 1], i2 = n2?.tag;
          if (i2 && "*" !== i2) {
            const n3 = `${e2}:${i2}`;
            this._byDepthAndTag.has(n3) || this._byDepthAndTag.set(n3, []), this._byDepthAndTag.get(n3).push(t2);
          } else this._wildcardByDepth.has(e2) || this._wildcardByDepth.set(e2, []), this._wildcardByDepth.get(e2).push(t2);
          return this;
        }
        addAll(t2) {
          for (const e2 of t2) this.add(e2);
          return this;
        }
        has(t2) {
          return this._patterns.has(t2.pattern);
        }
        get size() {
          return this._patterns.size;
        }
        seal() {
          return this._sealed = true, this;
        }
        get isSealed() {
          return this._sealed;
        }
        matchesAny(t2) {
          return null !== this.findMatch(t2);
        }
        findMatch(t2) {
          const e2 = t2.getDepth(), n2 = `${e2}:${t2.getCurrentTag()}`, i2 = this._byDepthAndTag.get(n2);
          if (i2) {
            for (let e3 = 0; e3 < i2.length; e3++) if (t2.matches(i2[e3])) return i2[e3];
          }
          const s2 = this._wildcardByDepth.get(e2);
          if (s2) {
            for (let e3 = 0; e3 < s2.length; e3++) if (t2.matches(s2[e3])) return s2[e3];
          }
          for (let e3 = 0; e3 < this._deepWildcards.length; e3++) if (t2.matches(this._deepWildcards[e3])) return this._deepWildcards[e3];
          return null;
        }
      }
      const H = { cent: "\xA2", pound: "\xA3", curren: "\xA4", yen: "\xA5", euro: "\u20AC", dollar: "$", euro: "\u20AC", fnof: "\u0192", inr: "\u20B9", af: "\u060B", birr: "\u1265\u122D", peso: "\u20B1", rub: "\u20BD", won: "\u20A9", yuan: "\xA5", cedil: "\xB8" }, tt = { amp: "&", apos: "'", gt: ">", lt: "<", quot: '"' }, et = { nbsp: "\xA0", copy: "\xA9", reg: "\xAE", trade: "\u2122", mdash: "\u2014", ndash: "\u2013", hellip: "\u2026", laquo: "\xAB", raquo: "\xBB", lsquo: "\u2018", rsquo: "\u2019", ldquo: "\u201C", rdquo: "\u201D", bull: "\u2022", para: "\xB6", sect: "\xA7", deg: "\xB0", frac12: "\xBD", frac14: "\xBC", frac34: "\xBE" }, nt = new Set("!?\\\\/[]$%{}^&*()<>|+");
      function it(t2) {
        if ("#" === t2[0]) throw new Error(`[EntityReplacer] Invalid character '#' in entity name: "${t2}"`);
        for (const e2 of t2) if (nt.has(e2)) throw new Error(`[EntityReplacer] Invalid character '${e2}' in entity name: "${t2}"`);
        return t2;
      }
      function st(...t2) {
        const e2 = /* @__PURE__ */ Object.create(null);
        for (const n2 of t2) if (n2) for (const t3 of Object.keys(n2)) {
          const i2 = n2[t3];
          if ("string" == typeof i2) e2[t3] = i2;
          else if (i2 && "object" == typeof i2 && void 0 !== i2.val) {
            const n3 = i2.val;
            "string" == typeof n3 && (e2[t3] = n3);
          }
        }
        return e2;
      }
      const rt = "external", ot = "base", at = "all", ht = Object.freeze({ allow: 0, leave: 1, remove: 2, throw: 3 }), lt = /* @__PURE__ */ new Set([9, 10, 13]);
      class ut {
        constructor(t2 = {}) {
          var e2;
          this._limit = t2.limit || {}, this._maxTotalExpansions = this._limit.maxTotalExpansions || 0, this._maxExpandedLength = this._limit.maxExpandedLength || 0, this._postCheck = "function" == typeof t2.postCheck ? t2.postCheck : (t3) => t3, this._limitTiers = (e2 = this._limit.applyLimitsTo ?? rt) && e2 !== rt ? e2 === at ? /* @__PURE__ */ new Set([at]) : e2 === ot ? /* @__PURE__ */ new Set([ot]) : Array.isArray(e2) ? new Set(e2) : /* @__PURE__ */ new Set([rt]) : /* @__PURE__ */ new Set([rt]), this._numericAllowed = t2.numericAllowed ?? true, this._baseMap = st(tt, t2.namedEntities || null), this._externalMap = /* @__PURE__ */ Object.create(null), this._inputMap = /* @__PURE__ */ Object.create(null), this._totalExpansions = 0, this._expandedLength = 0, this._removeSet = new Set(t2.remove && Array.isArray(t2.remove) ? t2.remove : []), this._leaveSet = new Set(t2.leave && Array.isArray(t2.leave) ? t2.leave : []);
          const n2 = (function(t3) {
            if (!t3) return { xmlVersion: 1, onLevel: ht.allow, nullLevel: ht.remove };
            const e3 = 1.1 === t3.xmlVersion ? 1.1 : 1, n3 = ht[t3.onNCR] ?? ht.allow, i2 = ht[t3.nullNCR] ?? ht.remove;
            return { xmlVersion: e3, onLevel: n3, nullLevel: Math.max(i2, ht.remove) };
          })(t2.ncr);
          this._ncrXmlVersion = n2.xmlVersion, this._ncrOnLevel = n2.onLevel, this._ncrNullLevel = n2.nullLevel;
        }
        setExternalEntities(t2) {
          if (t2) for (const e2 of Object.keys(t2)) it(e2);
          this._externalMap = st(t2);
        }
        addExternalEntity(t2, e2) {
          it(t2), "string" == typeof e2 && -1 === e2.indexOf("&") && (this._externalMap[t2] = e2);
        }
        addInputEntities(t2) {
          this._totalExpansions = 0, this._expandedLength = 0, this._inputMap = st(t2);
        }
        reset() {
          return this._inputMap = /* @__PURE__ */ Object.create(null), this._totalExpansions = 0, this._expandedLength = 0, this;
        }
        setXmlVersion(t2) {
          this._ncrXmlVersion = 1.1 === t2 ? 1.1 : 1;
        }
        decode(t2) {
          if ("string" != typeof t2 || 0 === t2.length) return t2;
          const e2 = t2, n2 = [], i2 = t2.length;
          let s2 = 0, r2 = 0;
          const o2 = this._maxTotalExpansions > 0, a2 = this._maxExpandedLength > 0, h2 = o2 || a2;
          for (; r2 < i2; ) {
            if (38 !== t2.charCodeAt(r2)) {
              r2++;
              continue;
            }
            let e3 = r2 + 1;
            for (; e3 < i2 && 59 !== t2.charCodeAt(e3) && e3 - r2 <= 32; ) e3++;
            if (e3 >= i2 || 59 !== t2.charCodeAt(e3)) {
              r2++;
              continue;
            }
            const l3 = t2.slice(r2 + 1, e3);
            if (0 === l3.length) {
              r2++;
              continue;
            }
            let u2, p2;
            if (this._removeSet.has(l3)) u2 = "", void 0 === p2 && (p2 = rt);
            else {
              if (this._leaveSet.has(l3)) {
                r2++;
                continue;
              }
              if (35 === l3.charCodeAt(0)) {
                const t3 = this._resolveNCR(l3);
                if (void 0 === t3) {
                  r2++;
                  continue;
                }
                u2 = t3, p2 = ot;
              } else {
                const t3 = this._resolveName(l3);
                u2 = t3?.value, p2 = t3?.tier;
              }
            }
            if (void 0 !== u2) {
              if (r2 > s2 && n2.push(t2.slice(s2, r2)), n2.push(u2), s2 = e3 + 1, r2 = s2, h2 && this._tierCounts(p2)) {
                if (o2 && (this._totalExpansions++, this._totalExpansions > this._maxTotalExpansions)) throw new Error(`[EntityReplacer] Entity expansion count limit exceeded: ${this._totalExpansions} > ${this._maxTotalExpansions}`);
                if (a2) {
                  const t3 = u2.length - (l3.length + 2);
                  if (t3 > 0 && (this._expandedLength += t3, this._expandedLength > this._maxExpandedLength)) throw new Error(`[EntityReplacer] Expanded content length limit exceeded: ${this._expandedLength} > ${this._maxExpandedLength}`);
                }
              }
            } else r2++;
          }
          s2 < i2 && n2.push(t2.slice(s2));
          const l2 = 0 === n2.length ? t2 : n2.join("");
          return this._postCheck(l2, e2);
        }
        _tierCounts(t2) {
          return !!this._limitTiers.has(at) || this._limitTiers.has(t2);
        }
        _resolveName(t2) {
          return t2 in this._inputMap ? { value: this._inputMap[t2], tier: rt } : t2 in this._externalMap ? { value: this._externalMap[t2], tier: rt } : t2 in this._baseMap ? { value: this._baseMap[t2], tier: ot } : void 0;
        }
        _classifyNCR(t2) {
          return 0 === t2 ? this._ncrNullLevel : t2 >= 55296 && t2 <= 57343 || 1 === this._ncrXmlVersion && t2 >= 1 && t2 <= 31 && !lt.has(t2) ? ht.remove : -1;
        }
        _applyNCRAction(t2, e2, n2) {
          switch (t2) {
            case ht.allow:
              return String.fromCodePoint(n2);
            case ht.remove:
              return "";
            case ht.leave:
              return;
            case ht.throw:
              throw new Error(`[EntityDecoder] Prohibited numeric character reference &${e2}; (U+${n2.toString(16).toUpperCase().padStart(4, "0")})`);
            default:
              return String.fromCodePoint(n2);
          }
        }
        _resolveNCR(t2) {
          const e2 = t2.charCodeAt(1);
          let n2;
          if (n2 = 120 === e2 || 88 === e2 ? parseInt(t2.slice(2), 16) : parseInt(t2.slice(1), 10), Number.isNaN(n2) || n2 < 0 || n2 > 1114111) return;
          const i2 = this._classifyNCR(n2);
          if (!this._numericAllowed && i2 < ht.remove) return;
          const s2 = -1 === i2 ? this._ncrOnLevel : Math.max(this._ncrOnLevel, i2);
          return this._applyNCRAction(s2, t2, n2);
        }
      }
      function pt(t2, e2) {
        if (!t2) return {};
        const n2 = e2.attributesGroupName ? t2[e2.attributesGroupName] : t2;
        if (!n2) return {};
        const i2 = {};
        for (const t3 in n2) t3.startsWith(e2.attributeNamePrefix) ? i2[t3.substring(e2.attributeNamePrefix.length)] = n2[t3] : i2[t3] = n2[t3];
        return i2;
      }
      function ct(t2) {
        if (!t2 || "string" != typeof t2) return;
        const e2 = t2.indexOf(":");
        if (-1 !== e2 && e2 > 0) {
          const n2 = t2.substring(0, e2);
          if ("xmlns" !== n2) return n2;
        }
      }
      class dt {
        constructor(t2, e2) {
          var n2;
          this.options = t2, this.currentNode = null, this.tagsNodeStack = [], this.parseXml = Nt, this.parseTextData = ft, this.resolveNameSpace = gt, this.buildAttributesMap = xt, this.isItStopNode = wt, this.replaceEntitiesValue = yt, this.readStopNodeData = At, this.saveTextToParentTag = Et, this.addChild = bt, this.ignoreAttributesFn = "function" == typeof (n2 = this.options.ignoreAttributes) ? n2 : Array.isArray(n2) ? (t3) => {
            for (const e3 of n2) {
              if ("string" == typeof e3 && t3 === e3) return true;
              if (e3 instanceof RegExp && e3.test(t3)) return true;
            }
          } : () => false, this.entityExpansionCount = 0, this.currentExpandedLength = 0;
          let i2 = { ...tt };
          this.options.entityDecoder ? this.entityDecoder = this.options.entityDecoder : ("object" == typeof this.options.htmlEntities ? i2 = this.options.htmlEntities : true === this.options.htmlEntities && (i2 = { ...et, ...H }), this.entityDecoder = new ut({ namedEntities: { ...i2, ...e2 }, numericAllowed: this.options.htmlEntities, limit: { maxTotalExpansions: this.options.processEntities.maxTotalExpansions, maxExpandedLength: this.options.processEntities.maxExpandedLength, applyLimitsTo: this.options.processEntities.appliesTo } })), this.matcher = new J(), this.readonlyMatcher = this.matcher.readOnly(), this.isCurrentNodeStopNode = false, this.stopNodeExpressionsSet = new Q();
          const s2 = this.options.stopNodes;
          if (s2 && s2.length > 0) {
            for (let t3 = 0; t3 < s2.length; t3++) {
              const e3 = s2[t3];
              "string" == typeof e3 ? this.stopNodeExpressionsSet.add(new K(e3)) : e3 instanceof K && this.stopNodeExpressionsSet.add(e3);
            }
            this.stopNodeExpressionsSet.seal();
          }
        }
      }
      function ft(t2, e2, n2, i2, s2, r2, o2) {
        const a2 = this.options;
        if (void 0 !== t2 && (a2.trimValues && !i2 && (t2 = t2.trim()), t2.length > 0)) {
          o2 || (t2 = this.replaceEntitiesValue(t2, e2, n2));
          const i3 = a2.jPath ? n2.toString() : n2, h2 = a2.tagValueProcessor(e2, t2, i3, s2, r2);
          return null == h2 ? t2 : typeof h2 != typeof t2 || h2 !== t2 ? h2 : a2.trimValues || t2.trim() === t2 ? Tt(t2, a2.parseTagValue, a2.numberParseOptions) : t2;
        }
      }
      function gt(t2) {
        if (this.options.removeNSPrefix) {
          const e2 = t2.split(":"), n2 = "/" === t2.charAt(0) ? "/" : "";
          if ("xmlns" === e2[0]) return "";
          2 === e2.length && (t2 = n2 + e2[1]);
        }
        return t2;
      }
      const mt = new RegExp(`([^\\s=]+)\\s*(=\\s*(['"])([\\s\\S]*?)\\3)?`, "gm");
      function xt(t2, e2, n2, i2 = false) {
        const r2 = this.options;
        if (true === i2 || true !== r2.ignoreAttributes && "string" == typeof t2) {
          const i3 = s(t2, mt), o2 = i3.length, a2 = {}, h2 = new Array(o2);
          let l2 = false;
          const u2 = {};
          for (let t3 = 0; t3 < o2; t3++) {
            const e3 = this.resolveNameSpace(i3[t3][1]), s2 = i3[t3][4];
            if (e3.length && void 0 !== s2) {
              let i4 = s2;
              r2.trimValues && (i4 = i4.trim()), i4 = this.replaceEntitiesValue(i4, n2, this.readonlyMatcher), h2[t3] = i4, u2[e3] = i4, l2 = true;
            }
          }
          l2 && "object" == typeof e2 && e2.updateCurrent && e2.updateCurrent(u2);
          const p2 = r2.jPath ? e2.toString() : this.readonlyMatcher;
          let c2 = false;
          for (let t3 = 0; t3 < o2; t3++) {
            const e3 = this.resolveNameSpace(i3[t3][1]);
            if (this.ignoreAttributesFn(e3, p2)) continue;
            let n3 = r2.attributeNamePrefix + e3;
            if (e3.length) if (r2.transformAttributeName && (n3 = r2.transformAttributeName(n3)), n3 = Pt(n3, r2), void 0 !== i3[t3][4]) {
              const i4 = h2[t3], s2 = r2.attributeValueProcessor(e3, i4, p2);
              a2[n3] = null == s2 ? i4 : typeof s2 != typeof i4 || s2 !== i4 ? s2 : Tt(i4, r2.parseAttributeValue, r2.numberParseOptions), c2 = true;
            } else r2.allowBooleanAttributes && (a2[n3] = true, c2 = true);
          }
          if (!c2) return;
          if (r2.attributesGroupName && !r2.preserveOrder) {
            const t3 = {};
            return t3[r2.attributesGroupName] = a2, t3;
          }
          return a2;
        }
      }
      const Nt = function(t2) {
        t2 = t2.replace(/\r\n?/g, "\n");
        const e2 = new $("!xml");
        let n2 = e2, i2 = "";
        this.matcher.reset(), this.entityDecoder.reset(), this.entityExpansionCount = 0, this.currentExpandedLength = 0;
        const s2 = this.options, r2 = new k(s2.processEntities), o2 = t2.length;
        for (let a2 = 0; a2 < o2; a2++) if ("<" === t2[a2]) {
          const h2 = t2.charCodeAt(a2 + 1);
          if (47 === h2) {
            const e3 = vt(t2, ">", a2, "Closing Tag is not closed.");
            let r3 = t2.substring(a2 + 2, e3).trim();
            if (s2.removeNSPrefix) {
              const t3 = r3.indexOf(":");
              -1 !== t3 && (r3 = r3.substr(t3 + 1));
            }
            r3 = Ct(s2.transformTagName, r3, "", s2).tagName, n2 && (i2 = this.saveTextToParentTag(i2, n2, this.readonlyMatcher));
            const o3 = this.matcher.getCurrentTag();
            if (r3 && s2.unpairedTagsSet.has(r3)) throw new Error(`Unpaired tag can not be used as closing tag: </${r3}>`);
            o3 && s2.unpairedTagsSet.has(o3) && (this.matcher.pop(), this.tagsNodeStack.pop()), this.matcher.pop(), this.isCurrentNodeStopNode = false, n2 = this.tagsNodeStack.pop(), i2 = "", a2 = e3;
          } else if (63 === h2) {
            let e3 = _t(t2, a2, false, "?>");
            if (!e3) throw new Error("Pi Tag is not closed.");
            i2 = this.saveTextToParentTag(i2, n2, this.readonlyMatcher);
            const o3 = this.buildAttributesMap(e3.tagExp, this.matcher, e3.tagName, true);
            if (o3) {
              const t3 = o3[this.options.attributeNamePrefix + "version"];
              this.entityDecoder.setXmlVersion(Number(t3) || 1), r2.setXmlVersion(Number(t3) || 1);
            }
            if (s2.ignoreDeclaration && "?xml" === e3.tagName || s2.ignorePiTags) ;
            else {
              const t3 = new $(e3.tagName);
              t3.add(s2.textNodeName, ""), e3.tagName !== e3.tagExp && e3.attrExpPresent && true !== s2.ignoreAttributes && (t3[":@"] = o3), this.addChild(n2, t3, this.readonlyMatcher, a2);
            }
            a2 = e3.closeIndex + 1;
          } else if (33 === h2 && 45 === t2.charCodeAt(a2 + 2) && 45 === t2.charCodeAt(a2 + 3)) {
            const e3 = vt(t2, "-->", a2 + 4, "Comment is not closed.");
            if (s2.commentPropName) {
              const r3 = t2.substring(a2 + 4, e3 - 2);
              i2 = this.saveTextToParentTag(i2, n2, this.readonlyMatcher), n2.add(s2.commentPropName, [{ [s2.textNodeName]: r3 }]);
            }
            a2 = e3;
          } else if (33 === h2 && 68 === t2.charCodeAt(a2 + 2)) {
            const e3 = r2.readDocType(t2, a2);
            this.entityDecoder.addInputEntities(e3.entities), a2 = e3.i;
          } else if (33 === h2 && 91 === t2.charCodeAt(a2 + 2)) {
            const e3 = vt(t2, "]]>", a2, "CDATA is not closed.") - 2, r3 = t2.substring(a2 + 9, e3);
            i2 = this.saveTextToParentTag(i2, n2, this.readonlyMatcher);
            let o3 = this.parseTextData(r3, n2.tagname, this.readonlyMatcher, true, false, true, true);
            null == o3 && (o3 = ""), s2.cdataPropName ? n2.add(s2.cdataPropName, [{ [s2.textNodeName]: r3 }]) : n2.add(s2.textNodeName, o3), a2 = e3 + 2;
          } else {
            let r3 = _t(t2, a2, s2.removeNSPrefix);
            if (!r3) {
              const e3 = t2.substring(Math.max(0, a2 - 50), Math.min(o2, a2 + 50));
              throw new Error(`readTagExp returned undefined at position ${a2}. Context: "${e3}"`);
            }
            let h3 = r3.tagName;
            const l2 = r3.rawTagName;
            let u2 = r3.tagExp, p2 = r3.attrExpPresent, c2 = r3.closeIndex;
            if ({ tagName: h3, tagExp: u2 } = Ct(s2.transformTagName, h3, u2, s2), s2.strictReservedNames && (h3 === s2.commentPropName || h3 === s2.cdataPropName || h3 === s2.textNodeName || h3 === s2.attributesGroupName)) throw new Error(`Invalid tag name: ${h3}`);
            n2 && i2 && "!xml" !== n2.tagname && (i2 = this.saveTextToParentTag(i2, n2, this.readonlyMatcher, false));
            const d2 = n2;
            d2 && s2.unpairedTagsSet.has(d2.tagname) && (n2 = this.tagsNodeStack.pop(), this.matcher.pop());
            let f2 = false;
            u2.length > 0 && u2.lastIndexOf("/") === u2.length - 1 && (f2 = true, "/" === h3[h3.length - 1] ? (h3 = h3.substr(0, h3.length - 1), u2 = h3) : u2 = u2.substr(0, u2.length - 1), p2 = h3 !== u2);
            let g2, m2 = null, x2 = {};
            g2 = ct(l2), h3 !== e2.tagname && this.matcher.push(h3, {}, g2), h3 !== u2 && p2 && (m2 = this.buildAttributesMap(u2, this.matcher, h3), m2 && (x2 = pt(m2, s2))), h3 !== e2.tagname && (this.isCurrentNodeStopNode = this.isItStopNode());
            const N2 = a2;
            if (this.isCurrentNodeStopNode) {
              let e3 = "";
              if (f2) a2 = r3.closeIndex;
              else if (s2.unpairedTagsSet.has(h3)) a2 = r3.closeIndex;
              else {
                const n3 = this.readStopNodeData(t2, l2, c2 + 1);
                if (!n3) throw new Error(`Unexpected end of ${l2}`);
                a2 = n3.i, e3 = n3.tagContent;
              }
              const i3 = new $(h3);
              m2 && (i3[":@"] = m2), i3.add(s2.textNodeName, e3), this.matcher.pop(), this.isCurrentNodeStopNode = false, this.addChild(n2, i3, this.readonlyMatcher, N2);
            } else {
              if (f2) {
                ({ tagName: h3, tagExp: u2 } = Ct(s2.transformTagName, h3, u2, s2));
                const t3 = new $(h3);
                m2 && (t3[":@"] = m2), this.addChild(n2, t3, this.readonlyMatcher, N2), this.matcher.pop(), this.isCurrentNodeStopNode = false;
              } else {
                if (s2.unpairedTagsSet.has(h3)) {
                  const t3 = new $(h3);
                  m2 && (t3[":@"] = m2), this.addChild(n2, t3, this.readonlyMatcher, N2), this.matcher.pop(), this.isCurrentNodeStopNode = false, a2 = r3.closeIndex;
                  continue;
                }
                {
                  const t3 = new $(h3);
                  if (this.tagsNodeStack.length > s2.maxNestedTags) throw new Error("Maximum nested tags exceeded");
                  this.tagsNodeStack.push(n2), m2 && (t3[":@"] = m2), this.addChild(n2, t3, this.readonlyMatcher, N2), n2 = t3;
                }
              }
              i2 = "", a2 = c2;
            }
          }
        } else i2 += t2[a2];
        return e2.child;
      };
      function bt(t2, e2, n2, i2) {
        this.options.captureMetaData || (i2 = void 0);
        const s2 = this.options.jPath ? n2.toString() : n2, r2 = this.options.updateTag(e2.tagname, s2, e2[":@"]);
        false === r2 || ("string" == typeof r2 ? (e2.tagname = r2, t2.addChild(e2, i2)) : t2.addChild(e2, i2));
      }
      function yt(t2, e2, n2) {
        const i2 = this.options.processEntities;
        if (!i2 || !i2.enabled) return t2;
        if (i2.allowedTags) {
          const s2 = this.options.jPath ? n2.toString() : n2;
          if (!(Array.isArray(i2.allowedTags) ? i2.allowedTags.includes(e2) : i2.allowedTags(e2, s2))) return t2;
        }
        if (i2.tagFilter) {
          const s2 = this.options.jPath ? n2.toString() : n2;
          if (!i2.tagFilter(e2, s2)) return t2;
        }
        return this.entityDecoder.decode(t2);
      }
      function Et(t2, e2, n2, i2) {
        return t2 && (void 0 === i2 && (i2 = 0 === e2.child.length), void 0 !== (t2 = this.parseTextData(t2, e2.tagname, n2, false, !!e2[":@"] && 0 !== Object.keys(e2[":@"]).length, i2)) && "" !== t2 && e2.add(this.options.textNodeName, t2), t2 = ""), t2;
      }
      function wt() {
        return 0 !== this.stopNodeExpressionsSet.size && this.matcher.matchesAny(this.stopNodeExpressionsSet);
      }
      function vt(t2, e2, n2, i2) {
        const s2 = t2.indexOf(e2, n2);
        if (-1 === s2) throw new Error(i2);
        return s2 + e2.length - 1;
      }
      function St(t2, e2, n2, i2) {
        const s2 = t2.indexOf(e2, n2);
        if (-1 === s2) throw new Error(i2);
        return s2;
      }
      function _t(t2, e2, n2, i2 = ">") {
        const s2 = (function(t3, e3, n3 = ">") {
          let i3 = 0;
          const s3 = t3.length, r3 = n3.charCodeAt(0), o3 = n3.length > 1 ? n3.charCodeAt(1) : -1;
          let a3 = "", h3 = e3;
          for (let n4 = e3; n4 < s3; n4++) {
            const e4 = t3.charCodeAt(n4);
            if (i3) e4 === i3 && (i3 = 0);
            else if (34 === e4 || 39 === e4) i3 = e4;
            else if (e4 === r3) {
              if (-1 === o3) return a3 += t3.substring(h3, n4), { data: a3, index: n4 };
              if (t3.charCodeAt(n4 + 1) === o3) return a3 += t3.substring(h3, n4), { data: a3, index: n4 };
            } else 9 !== e4 || i3 || (a3 += t3.substring(h3, n4) + " ", h3 = n4 + 1);
          }
        })(t2, e2 + 1, i2);
        if (!s2) return;
        let r2 = s2.data;
        const o2 = s2.index, a2 = r2.search(/\s/);
        let h2 = r2, l2 = true;
        -1 !== a2 && (h2 = r2.substring(0, a2), r2 = r2.substring(a2 + 1).trimStart());
        const u2 = h2;
        if (n2) {
          const t3 = h2.indexOf(":");
          -1 !== t3 && (h2 = h2.substr(t3 + 1), l2 = h2 !== s2.data.substr(t3 + 1));
        }
        return { tagName: h2, tagExp: r2, closeIndex: o2, attrExpPresent: l2, rawTagName: u2 };
      }
      function At(t2, e2, n2) {
        const i2 = n2;
        let s2 = 1;
        const r2 = t2.length;
        for (; n2 < r2; n2++) if ("<" === t2[n2]) {
          const r3 = t2.charCodeAt(n2 + 1);
          if (47 === r3) {
            const r4 = St(t2, ">", n2, `${e2} is not closed`);
            if (t2.substring(n2 + 2, r4).trim() === e2 && (s2--, 0 === s2)) return { tagContent: t2.substring(i2, n2), i: r4 };
            n2 = r4;
          } else if (63 === r3) n2 = vt(t2, "?>", n2 + 1, "StopNode is not closed.");
          else if (33 === r3 && 45 === t2.charCodeAt(n2 + 2) && 45 === t2.charCodeAt(n2 + 3)) n2 = vt(t2, "-->", n2 + 3, "StopNode is not closed.");
          else if (33 === r3 && 91 === t2.charCodeAt(n2 + 2)) n2 = vt(t2, "]]>", n2, "StopNode is not closed.") - 2;
          else {
            const i3 = _t(t2, n2, false);
            i3 && ((i3 && i3.tagName) === e2 && "/" !== i3.tagExp[i3.tagExp.length - 1] && s2++, n2 = i3.closeIndex);
          }
        }
      }
      function Tt(t2, e2, n2) {
        if (e2 && "string" == typeof t2) {
          const e3 = t2.trim();
          return "true" === e3 || "false" !== e3 && (function(t3, e4 = {}) {
            if (e4 = Object.assign({}, X, e4), !t3 || "string" != typeof t3) return t3;
            let n3 = t3.trim();
            if (0 === n3.length) return t3;
            if (void 0 !== e4.skipLike && e4.skipLike.test(n3)) return t3;
            if ("0" === n3) return 0;
            if (e4.hex && U.test(n3)) return q(n3, 16);
            if (e4.binary && B.test(n3)) return q(n3, 2);
            if (e4.octal && W.test(n3)) return q(n3, 8);
            if (isFinite(n3)) {
              if (n3.includes("e") || n3.includes("E")) return (function(t4, e5, n4) {
                if (!n4.eNotation) return t4;
                const i3 = e5.match(Y);
                if (i3) {
                  let s2 = i3[1] || "";
                  const r2 = -1 === i3[3].indexOf("e") ? "E" : "e", o2 = i3[2], a2 = s2 ? t4[o2.length + 1] === r2 : t4[o2.length] === r2;
                  return o2.length > 1 && a2 ? t4 : (1 !== o2.length || !i3[3].startsWith(`.${r2}`) && i3[3][0] !== r2) && o2.length > 0 ? n4.leadingZeros && !a2 ? (e5 = (i3[1] || "") + i3[3], Number(e5)) : t4 : Number(e5);
                }
                return t4;
              })(t3, n3, e4);
              {
                const s2 = z.exec(n3);
                if (s2) {
                  const r2 = s2[1] || "", o2 = s2[2];
                  let a2 = (i2 = s2[3]) && -1 !== i2.indexOf(".") ? ("." === (i2 = i2.replace(/0+$/, "")) ? i2 = "0" : "." === i2[0] ? i2 = "0" + i2 : "." === i2[i2.length - 1] && (i2 = i2.substring(0, i2.length - 1)), i2) : i2;
                  const h2 = r2 ? "." === t3[o2.length + 1] : "." === t3[o2.length];
                  if (!e4.leadingZeros && (o2.length > 1 || 1 === o2.length && !h2)) return t3;
                  {
                    const i3 = Number(n3), s3 = String(i3);
                    if (0 === i3) return i3;
                    if (-1 !== s3.search(/[eE]/)) return e4.eNotation ? i3 : t3;
                    if (-1 !== n3.indexOf(".")) return "0" === s3 || s3 === a2 || s3 === `${r2}${a2}` ? i3 : t3;
                    let h3 = o2 ? a2 : n3;
                    return o2 ? h3 === s3 || r2 + h3 === s3 ? i3 : t3 : h3 === s3 || h3 === r2 + s3 ? i3 : t3;
                  }
                }
                return t3;
              }
            }
            var i2;
            return (function(t4, e5, n4) {
              const i3 = e5 === 1 / 0;
              switch (n4.infinity.toLowerCase()) {
                case "null":
                  return null;
                case "infinity":
                  return e5;
                case "string":
                  return i3 ? "Infinity" : "-Infinity";
                default:
                  return t4;
              }
            })(t3, Number(n3), e4);
          })(t2, n2);
        }
        return void 0 !== t2 ? t2 : "";
      }
      function Ct(t2, e2, n2, i2) {
        if (t2) {
          const i3 = t2(e2);
          n2 === e2 && (n2 = i3), e2 = i3;
        }
        return { tagName: e2 = Pt(e2, i2), tagExp: n2 };
      }
      function Pt(t2, e2) {
        if (a.includes(t2)) throw new Error(`[SECURITY] Invalid name: "${t2}" is a reserved JavaScript keyword that could cause prototype pollution`);
        return o.includes(t2) ? e2.onDangerousProperty(t2) : t2;
      }
      const $t = $.getMetaDataSymbol();
      function Ot(t2, e2) {
        if (!t2 || "object" != typeof t2) return {};
        if (!e2) return t2;
        const n2 = {};
        for (const i2 in t2) i2.startsWith(e2) ? n2[i2.substring(e2.length)] = t2[i2] : n2[i2] = t2[i2];
        return n2;
      }
      function It(t2, e2, n2, i2) {
        return Vt(t2, e2, n2, i2);
      }
      function Vt(t2, e2, n2, i2) {
        let s2;
        const r2 = {};
        for (let o2 = 0; o2 < t2.length; o2++) {
          const a2 = t2[o2], h2 = Dt(a2);
          if (void 0 !== h2 && h2 !== e2.textNodeName) {
            const t3 = Ot(a2[":@"] || {}, e2.attributeNamePrefix);
            n2.push(h2, t3);
          }
          if (h2 === e2.textNodeName) void 0 === s2 ? s2 = a2[h2] : s2 += "" + a2[h2];
          else {
            if (void 0 === h2) continue;
            if (a2[h2]) {
              let t3 = Vt(a2[h2], e2, n2, i2);
              const s3 = jt(t3, e2);
              if (0 === Object.keys(t3).length && e2.alwaysCreateTextNode && (t3[e2.textNodeName] = ""), a2[":@"] ? Mt(t3, a2[":@"], i2, e2) : 1 !== Object.keys(t3).length || void 0 === t3[e2.textNodeName] || e2.alwaysCreateTextNode ? 0 === Object.keys(t3).length && (e2.alwaysCreateTextNode ? t3[e2.textNodeName] = "" : t3 = "") : t3 = t3[e2.textNodeName], void 0 !== a2[$t] && "object" == typeof t3 && null !== t3 && (t3[$t] = a2[$t]), void 0 !== r2[h2] && Object.prototype.hasOwnProperty.call(r2, h2)) Array.isArray(r2[h2]) || (r2[h2] = [r2[h2]]), r2[h2].push(t3);
              else {
                const n3 = e2.jPath ? i2.toString() : i2;
                e2.isArray(h2, n3, s3) ? r2[h2] = [t3] : r2[h2] = t3;
              }
              void 0 !== h2 && h2 !== e2.textNodeName && n2.pop();
            }
          }
        }
        return "string" == typeof s2 ? s2.length > 0 && (r2[e2.textNodeName] = s2) : void 0 !== s2 && (r2[e2.textNodeName] = s2), r2;
      }
      function Dt(t2) {
        const e2 = Object.keys(t2);
        for (let t3 = 0; t3 < e2.length; t3++) {
          const n2 = e2[t3];
          if (":@" !== n2) return n2;
        }
      }
      function Mt(t2, e2, n2, i2) {
        if (e2) {
          const s2 = Object.keys(e2), r2 = s2.length;
          for (let o2 = 0; o2 < r2; o2++) {
            const r3 = s2[o2], a2 = r3.startsWith(i2.attributeNamePrefix) ? r3.substring(i2.attributeNamePrefix.length) : r3, h2 = i2.jPath ? n2.toString() + "." + a2 : n2;
            i2.isArray(r3, h2, true, true) ? t2[r3] = [e2[r3]] : t2[r3] = e2[r3];
          }
        }
      }
      function jt(t2, e2) {
        const { textNodeName: n2 } = e2, i2 = Object.keys(t2).length;
        return 0 === i2 || !(1 !== i2 || !t2[n2] && "boolean" != typeof t2[n2] && 0 !== t2[n2]);
      }
      class Lt {
        constructor(t2) {
          this.externalEntities = {}, this.options = C(t2);
        }
        parse(t2, e2) {
          if ("string" != typeof t2 && t2.toString) t2 = t2.toString();
          else if ("string" != typeof t2) throw new Error("XML data is accepted in String or Bytes[] form.");
          if (e2) {
            true === e2 && (e2 = {});
            const n3 = l(t2, e2);
            if (true !== n3) throw Error(`${n3.err.msg}:${n3.err.line}:${n3.err.col}`);
          }
          const n2 = new dt(this.options, this.externalEntities), i2 = n2.parseXml(t2);
          return this.options.preserveOrder || void 0 === i2 ? i2 : It(i2, this.options, n2.matcher, n2.readonlyMatcher);
        }
        addEntity(t2, e2) {
          if (-1 !== e2.indexOf("&")) throw new Error("Entity value can't have '&'");
          if (-1 !== t2.indexOf("&") || -1 !== t2.indexOf(";")) throw new Error("An entity must be set without '&' and ';'. Eg. use '#xD' for '&#xD;'");
          if ("&" === e2) throw new Error("An entity with value '&' is not permitted");
          this.externalEntities[t2] = e2;
        }
        static getMetaDataSymbol() {
          return $.getMetaDataSymbol();
        }
      }
      function kt(t2) {
        return String(t2).replace(/--/g, "- -").replace(/--/g, "- -").replace(/-$/, "- ");
      }
      function Rt(t2) {
        return String(t2).replace(/\]\]>/g, "]]]]><![CDATA[>");
      }
      function Ft(t2) {
        return String(t2).replace(/"/g, "&quot;").replace(/'/g, "&apos;");
      }
      function Gt(t2, e2, n2, i2, s2) {
        return n2.sanitizeName ? L(t2, { xmlVersion: s2 }) ? t2 : n2.sanitizeName(t2, { isAttribute: e2, matcher: i2.readOnly() }) : t2;
      }
      function Ut(t2, e2) {
        let n2 = "";
        e2.format && (n2 = "\n");
        const i2 = [];
        if (e2.stopNodes && Array.isArray(e2.stopNodes)) for (let t3 = 0; t3 < e2.stopNodes.length; t3++) {
          const n3 = e2.stopNodes[t3];
          "string" == typeof n3 ? i2.push(new K(n3)) : n3 instanceof K && i2.push(n3);
        }
        const s2 = (function(t3, e3) {
          if (!Array.isArray(t3) || 0 === t3.length) return "1.0";
          const n3 = t3[0];
          if ("?xml" === Yt(n3)) {
            const t4 = n3[":@"];
            if (t4) {
              const n4 = e3.attributeNamePrefix + "version";
              if (t4[n4]) return t4[n4];
            }
          }
          return "1.0";
        })(t2, e2);
        return Bt(t2, e2, n2, new J(), i2, s2);
      }
      function Bt(t2, e2, n2, i2, s2, r2) {
        let o2 = "", a2 = false;
        if (e2.maxNestedTags && i2.getDepth() > e2.maxNestedTags) throw new Error("Maximum nested tags exceeded");
        if (!Array.isArray(t2)) {
          if (null != t2) {
            let n3 = t2.toString();
            return n3 = Jt(n3, e2), n3;
          }
          return "";
        }
        for (let h2 = 0; h2 < t2.length; h2++) {
          const l2 = t2[h2], u2 = Yt(l2);
          if (void 0 === u2) continue;
          const p2 = u2 === e2.textNodeName || u2 === e2.cdataPropName || u2 === e2.commentPropName || "?" === u2[0] ? u2 : Gt(u2, false, e2, i2, r2), c2 = Wt(l2[":@"], e2);
          i2.push(p2, c2);
          const d2 = Zt(i2, s2);
          if (p2 === e2.textNodeName) {
            let t3 = l2[u2];
            d2 || (t3 = e2.tagValueProcessor(p2, t3), t3 = Jt(t3, e2)), a2 && (o2 += n2), o2 += t3, a2 = false, i2.pop();
            continue;
          }
          if (p2 === e2.cdataPropName) {
            a2 && (o2 += n2), o2 += `<![CDATA[${Rt(l2[u2][0][e2.textNodeName])}]]>`, a2 = false, i2.pop();
            continue;
          }
          if (p2 === e2.commentPropName) {
            o2 += n2 + `<!--${kt(l2[u2][0][e2.textNodeName])}-->`, a2 = true, i2.pop();
            continue;
          }
          if ("?" === p2[0]) {
            o2 += ("?xml" === p2 ? "" : n2) + `<${p2}${qt(l2[":@"], e2, d2, i2, r2)}?>`, a2 = true, i2.pop();
            continue;
          }
          let f2 = n2;
          "" !== f2 && (f2 += e2.indentBy);
          const g2 = n2 + `<${p2}${qt(l2[":@"], e2, d2, i2, r2)}`;
          let m2;
          m2 = d2 ? zt(l2[u2], e2) : Bt(l2[u2], e2, f2, i2, s2, r2), -1 !== e2.unpairedTags.indexOf(p2) ? e2.suppressUnpairedNode ? o2 += g2 + ">" : o2 += g2 + "/>" : m2 && 0 !== m2.length || !e2.suppressEmptyNode ? m2 && m2.endsWith(">") ? o2 += g2 + `>${m2}${n2}</${p2}>` : (o2 += g2 + ">", m2 && "" !== n2 && (m2.includes("/>") || m2.includes("</")) ? o2 += n2 + e2.indentBy + m2 + n2 : o2 += m2, o2 += `</${p2}>`) : o2 += g2 + "/>", a2 = true, i2.pop();
        }
        return o2;
      }
      function Wt(t2, e2) {
        if (!t2 || e2.ignoreAttributes) return null;
        const n2 = {};
        let i2 = false;
        for (let s2 in t2) Object.prototype.hasOwnProperty.call(t2, s2) && (n2[s2.startsWith(e2.attributeNamePrefix) ? s2.substr(e2.attributeNamePrefix.length) : s2] = Ft(t2[s2]), i2 = true);
        return i2 ? n2 : null;
      }
      function zt(t2, e2) {
        if (!Array.isArray(t2)) return null != t2 ? t2.toString() : "";
        let n2 = "";
        for (let i2 = 0; i2 < t2.length; i2++) {
          const s2 = t2[i2], r2 = Yt(s2);
          if (r2 === e2.textNodeName) n2 += s2[r2];
          else if (r2 === e2.cdataPropName) n2 += s2[r2][0][e2.textNodeName];
          else if (r2 === e2.commentPropName) n2 += s2[r2][0][e2.textNodeName];
          else {
            if (r2 && "?" === r2[0]) continue;
            if (r2) {
              const t3 = Xt(s2[":@"], e2), i3 = zt(s2[r2], e2);
              i3 && 0 !== i3.length ? n2 += `<${r2}${t3}>${i3}</${r2}>` : n2 += `<${r2}${t3}/>`;
            }
          }
        }
        return n2;
      }
      function Xt(t2, e2) {
        let n2 = "";
        if (t2 && !e2.ignoreAttributes) for (let i2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, i2)) continue;
          let s2 = t2[i2];
          true === s2 && e2.suppressBooleanAttributes ? n2 += ` ${i2.substr(e2.attributeNamePrefix.length)}` : n2 += ` ${i2.substr(e2.attributeNamePrefix.length)}="${Ft(s2)}"`;
        }
        return n2;
      }
      function Yt(t2) {
        const e2 = Object.keys(t2);
        for (let n2 = 0; n2 < e2.length; n2++) {
          const i2 = e2[n2];
          if (Object.prototype.hasOwnProperty.call(t2, i2) && ":@" !== i2) return i2;
        }
      }
      function qt(t2, e2, n2, i2, s2) {
        let r2 = "";
        if (t2 && !e2.ignoreAttributes) for (let o2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, o2)) continue;
          const a2 = o2.substr(e2.attributeNamePrefix.length), h2 = n2 ? a2 : Gt(a2, true, e2, i2, s2);
          let l2;
          n2 ? l2 = t2[o2] : (l2 = e2.attributeValueProcessor(o2, t2[o2]), l2 = Jt(l2, e2)), true === l2 && e2.suppressBooleanAttributes ? r2 += ` ${h2}` : r2 += ` ${h2}="${Ft(l2)}"`;
        }
        return r2;
      }
      function Zt(t2, e2) {
        if (!e2 || 0 === e2.length) return false;
        for (let n2 = 0; n2 < e2.length; n2++) if (t2.matches(e2[n2])) return true;
        return false;
      }
      function Jt(t2, e2) {
        if (t2 && t2.length > 0 && e2.processEntities) for (let n2 = 0; n2 < e2.entities.length; n2++) {
          const i2 = e2.entities[n2];
          t2 = t2.replace(i2.regex, i2.val);
        }
        return t2;
      }
      const Kt = { attributeNamePrefix: "@_", attributesGroupName: false, textNodeName: "#text", ignoreAttributes: true, cdataPropName: false, format: false, indentBy: "  ", suppressEmptyNode: false, suppressUnpairedNode: true, suppressBooleanAttributes: true, tagValueProcessor: function(t2, e2) {
        return e2;
      }, attributeValueProcessor: function(t2, e2) {
        return e2;
      }, preserveOrder: false, commentPropName: false, unpairedTags: [], entities: [{ regex: new RegExp("&", "g"), val: "&amp;" }, { regex: new RegExp(">", "g"), val: "&gt;" }, { regex: new RegExp("<", "g"), val: "&lt;" }, { regex: new RegExp("'", "g"), val: "&apos;" }, { regex: new RegExp('"', "g"), val: "&quot;" }], processEntities: true, stopNodes: [], oneListGroup: false, maxNestedTags: 100, jPath: true, sanitizeName: false };
      function Qt(t2) {
        if (this.options = Object.assign({}, Kt, t2), this.options.stopNodes && Array.isArray(this.options.stopNodes) && (this.options.stopNodes = this.options.stopNodes.map((t3) => "string" == typeof t3 && t3.startsWith("*.") ? ".." + t3.substring(2) : t3)), this.stopNodeExpressions = [], this.options.stopNodes && Array.isArray(this.options.stopNodes)) for (let t3 = 0; t3 < this.options.stopNodes.length; t3++) {
          const e3 = this.options.stopNodes[t3];
          "string" == typeof e3 ? this.stopNodeExpressions.push(new K(e3)) : e3 instanceof K && this.stopNodeExpressions.push(e3);
        }
        var e2;
        true === this.options.ignoreAttributes || this.options.attributesGroupName ? this.isAttribute = function() {
          return false;
        } : (this.ignoreAttributesFn = "function" == typeof (e2 = this.options.ignoreAttributes) ? e2 : Array.isArray(e2) ? (t3) => {
          for (const n2 of e2) {
            if ("string" == typeof n2 && t3 === n2) return true;
            if (n2 instanceof RegExp && n2.test(t3)) return true;
          }
        } : () => false, this.attrPrefixLen = this.options.attributeNamePrefix.length, this.isAttribute = ne), this.processTextOrObjNode = te, this.options.format ? (this.indentate = ee, this.tagEndChar = ">\n", this.newLine = "\n") : (this.indentate = function() {
          return "";
        }, this.tagEndChar = ">", this.newLine = "");
      }
      function Ht(t2, e2, n2, i2, s2) {
        return n2.sanitizeName ? L(t2, { xmlVersion: s2 }) ? t2 : n2.sanitizeName(t2, { isAttribute: e2, matcher: i2.readOnly() }) : t2;
      }
      function te(t2, e2, n2, i2, s2) {
        const r2 = this.extractAttributes(t2);
        if (i2.push(e2, r2), this.checkStopNode(i2)) {
          const s3 = this.buildRawContent(t2), r3 = this.buildAttributesForStopNode(t2);
          return i2.pop(), this.buildObjectNode(s3, e2, r3, n2);
        }
        const o2 = this.j2x(t2, n2 + 1, i2, s2);
        return i2.pop(), "?" === e2[0] ? this.buildTextValNode("", e2, o2.attrStr, n2, i2) : void 0 !== t2[this.options.textNodeName] && 1 === Object.keys(t2).length ? this.buildTextValNode(t2[this.options.textNodeName], e2, o2.attrStr, n2, i2) : this.buildObjectNode(o2.val, e2, o2.attrStr, n2);
      }
      function ee(t2) {
        return this.options.indentBy.repeat(t2);
      }
      function ne(t2) {
        return !(!t2.startsWith(this.options.attributeNamePrefix) || t2 === this.options.textNodeName) && t2.substr(this.attrPrefixLen);
      }
      Qt.prototype.build = function(t2) {
        if (this.options.preserveOrder) return Ut(t2, this.options);
        {
          Array.isArray(t2) && this.options.arrayNodeName && this.options.arrayNodeName.length > 1 && (t2 = { [this.options.arrayNodeName]: t2 });
          const e2 = new J(), n2 = (function(t3, e3) {
            const n3 = t3["?xml"];
            if (n3 && "object" == typeof n3) {
              if (e3.attributesGroupName && n3[e3.attributesGroupName]) {
                const t5 = n3[e3.attributesGroupName][e3.attributeNamePrefix + "version"];
                if (t5) return t5;
              }
              const t4 = n3[e3.attributeNamePrefix + "version"];
              if (t4) return t4;
            }
            return "1.0";
          })(t2, this.options);
          return this.j2x(t2, 0, e2, n2).val;
        }
      }, Qt.prototype.j2x = function(t2, e2, n2, i2) {
        let s2 = "", r2 = "";
        if (this.options.maxNestedTags && n2.getDepth() >= this.options.maxNestedTags) throw new Error("Maximum nested tags exceeded");
        const o2 = this.options.jPath ? n2.toString() : n2, a2 = this.checkStopNode(n2);
        for (let h2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, h2)) continue;
          const l2 = h2 === this.options.textNodeName || h2 === this.options.cdataPropName || h2 === this.options.commentPropName || this.options.attributesGroupName && h2 === this.options.attributesGroupName || this.isAttribute(h2) || "?" === h2[0] ? h2 : Ht(h2, false, this.options, n2, i2);
          if (void 0 === t2[h2]) this.isAttribute(h2) && (r2 += "");
          else if (null === t2[h2]) this.isAttribute(h2) || l2 === this.options.cdataPropName || l2 === this.options.commentPropName ? r2 += "" : "?" === l2[0] ? r2 += this.indentate(e2) + "<" + l2 + "?" + this.tagEndChar : r2 += this.indentate(e2) + "<" + l2 + "/" + this.tagEndChar;
          else if (t2[h2] instanceof Date) r2 += this.buildTextValNode(t2[h2], l2, "", e2, n2);
          else if ("object" != typeof t2[h2]) {
            const u2 = this.isAttribute(h2);
            if (u2 && !this.ignoreAttributesFn(u2, o2)) {
              const e3 = Ht(u2, true, this.options, n2, i2);
              s2 += this.buildAttrPairStr(e3, "" + t2[h2], a2);
            } else if (!u2) if (h2 === this.options.textNodeName) {
              let e3 = this.options.tagValueProcessor(h2, "" + t2[h2]);
              r2 += this.replaceEntitiesValue(e3);
            } else {
              n2.push(l2);
              const i3 = this.checkStopNode(n2);
              if (n2.pop(), i3) {
                const n3 = "" + t2[h2];
                r2 += "" === n3 ? this.indentate(e2) + "<" + l2 + this.closeTag(l2) + this.tagEndChar : this.indentate(e2) + "<" + l2 + ">" + n3 + "</" + l2 + this.tagEndChar;
              } else r2 += this.buildTextValNode(t2[h2], l2, "", e2, n2);
            }
          } else if (Array.isArray(t2[h2])) {
            const s3 = t2[h2].length;
            let o3 = "", a3 = "";
            for (let u2 = 0; u2 < s3; u2++) {
              const s4 = t2[h2][u2];
              if (void 0 === s4) ;
              else if (null === s4) "?" === l2[0] ? r2 += this.indentate(e2) + "<" + l2 + "?" + this.tagEndChar : r2 += this.indentate(e2) + "<" + l2 + "/" + this.tagEndChar;
              else if ("object" == typeof s4) if (this.options.oneListGroup) {
                n2.push(l2);
                const t3 = this.j2x(s4, e2 + 1, n2, i2);
                n2.pop(), o3 += t3.val, this.options.attributesGroupName && s4.hasOwnProperty(this.options.attributesGroupName) && (a3 += t3.attrStr);
              } else o3 += this.processTextOrObjNode(s4, l2, e2, n2, i2);
              else if (this.options.oneListGroup) {
                let t3 = this.options.tagValueProcessor(l2, s4);
                t3 = this.replaceEntitiesValue(t3), o3 += t3;
              } else {
                n2.push(l2);
                const t3 = this.checkStopNode(n2);
                if (n2.pop(), t3) {
                  const t4 = "" + s4;
                  o3 += "" === t4 ? this.indentate(e2) + "<" + l2 + this.closeTag(l2) + this.tagEndChar : this.indentate(e2) + "<" + l2 + ">" + t4 + "</" + l2 + this.tagEndChar;
                } else o3 += this.buildTextValNode(s4, l2, "", e2, n2);
              }
            }
            this.options.oneListGroup && (o3 = this.buildObjectNode(o3, l2, a3, e2)), r2 += o3;
          } else if (this.options.attributesGroupName && h2 === this.options.attributesGroupName) {
            const e3 = Object.keys(t2[h2]), r3 = e3.length;
            for (let o3 = 0; o3 < r3; o3++) {
              const r4 = Ht(e3[o3], true, this.options, n2, i2);
              s2 += this.buildAttrPairStr(r4, "" + t2[h2][e3[o3]], a2);
            }
          } else r2 += this.processTextOrObjNode(t2[h2], l2, e2, n2, i2);
        }
        return { attrStr: s2, val: r2 };
      }, Qt.prototype.buildAttrPairStr = function(t2, e2, n2) {
        return n2 || (e2 = this.options.attributeValueProcessor(t2, "" + e2), e2 = this.replaceEntitiesValue(e2)), this.options.suppressBooleanAttributes && "true" === e2 ? " " + t2 : " " + t2 + '="' + Ft(e2) + '"';
      }, Qt.prototype.extractAttributes = function(t2) {
        if (!t2 || "object" != typeof t2) return null;
        const e2 = {};
        let n2 = false;
        if (this.options.attributesGroupName && t2[this.options.attributesGroupName]) {
          const i2 = t2[this.options.attributesGroupName];
          for (let t3 in i2) Object.prototype.hasOwnProperty.call(i2, t3) && (e2[t3.startsWith(this.options.attributeNamePrefix) ? t3.substring(this.options.attributeNamePrefix.length) : t3] = Ft(i2[t3]), n2 = true);
        } else for (let i2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, i2)) continue;
          const s2 = this.isAttribute(i2);
          s2 && (e2[s2] = Ft(t2[i2]), n2 = true);
        }
        return n2 ? e2 : null;
      }, Qt.prototype.buildRawContent = function(t2) {
        if ("string" == typeof t2) return t2;
        if ("object" != typeof t2 || null === t2) return String(t2);
        if (void 0 !== t2[this.options.textNodeName]) return t2[this.options.textNodeName];
        let e2 = "";
        for (let n2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, n2)) continue;
          if (this.isAttribute(n2)) continue;
          if (this.options.attributesGroupName && n2 === this.options.attributesGroupName) continue;
          const i2 = t2[n2];
          if (n2 === this.options.textNodeName) e2 += i2;
          else if (Array.isArray(i2)) {
            for (let t3 of i2) if ("string" == typeof t3 || "number" == typeof t3) e2 += `<${n2}>${t3}</${n2}>`;
            else if ("object" == typeof t3 && null !== t3) {
              const i3 = this.buildRawContent(t3), s2 = this.buildAttributesForStopNode(t3);
              e2 += "" === i3 ? `<${n2}${s2}/>` : `<${n2}${s2}>${i3}</${n2}>`;
            }
          } else if ("object" == typeof i2 && null !== i2) {
            const t3 = this.buildRawContent(i2), s2 = this.buildAttributesForStopNode(i2);
            e2 += "" === t3 ? `<${n2}${s2}/>` : `<${n2}${s2}>${t3}</${n2}>`;
          } else e2 += `<${n2}>${i2}</${n2}>`;
        }
        return e2;
      }, Qt.prototype.buildAttributesForStopNode = function(t2) {
        if (!t2 || "object" != typeof t2) return "";
        let e2 = "";
        if (this.options.attributesGroupName && t2[this.options.attributesGroupName]) {
          const n2 = t2[this.options.attributesGroupName];
          for (let t3 in n2) {
            if (!Object.prototype.hasOwnProperty.call(n2, t3)) continue;
            const i2 = t3.startsWith(this.options.attributeNamePrefix) ? t3.substring(this.options.attributeNamePrefix.length) : t3, s2 = n2[t3];
            true === s2 && this.options.suppressBooleanAttributes ? e2 += " " + i2 : e2 += " " + i2 + '="' + s2 + '"';
          }
        } else for (let n2 in t2) {
          if (!Object.prototype.hasOwnProperty.call(t2, n2)) continue;
          const i2 = this.isAttribute(n2);
          if (i2) {
            const s2 = t2[n2];
            true === s2 && this.options.suppressBooleanAttributes ? e2 += " " + i2 : e2 += " " + i2 + '="' + s2 + '"';
          }
        }
        return e2;
      }, Qt.prototype.buildObjectNode = function(t2, e2, n2, i2) {
        if ("" === t2) return "?" === e2[0] ? this.indentate(i2) + "<" + e2 + n2 + "?" + this.tagEndChar : this.indentate(i2) + "<" + e2 + n2 + this.closeTag(e2) + this.tagEndChar;
        if ("?" === e2[0]) return this.indentate(i2) + "<" + e2 + n2 + "?" + this.tagEndChar;
        {
          let s2 = "</" + e2 + this.tagEndChar, r2 = "";
          return "?" === e2[0] && (r2 = "?", s2 = ""), !n2 && "" !== n2 || -1 !== t2.indexOf("<") ? false !== this.options.commentPropName && e2 === this.options.commentPropName && 0 === r2.length ? this.indentate(i2) + `<!--${t2}-->` + this.newLine : this.indentate(i2) + "<" + e2 + n2 + r2 + this.tagEndChar + t2 + this.indentate(i2) + s2 : this.indentate(i2) + "<" + e2 + n2 + r2 + ">" + t2 + s2;
        }
      }, Qt.prototype.closeTag = function(t2) {
        let e2 = "";
        return -1 !== this.options.unpairedTags.indexOf(t2) ? this.options.suppressUnpairedNode || (e2 = "/") : e2 = this.options.suppressEmptyNode ? "/" : `></${t2}`, e2;
      }, Qt.prototype.checkStopNode = function(t2) {
        if (!this.stopNodeExpressions || 0 === this.stopNodeExpressions.length) return false;
        for (let e2 = 0; e2 < this.stopNodeExpressions.length; e2++) if (t2.matches(this.stopNodeExpressions[e2])) return true;
        return false;
      }, Qt.prototype.buildTextValNode = function(t2, e2, n2, i2, s2) {
        if (false !== this.options.cdataPropName && e2 === this.options.cdataPropName) {
          const e3 = Rt(t2);
          return this.indentate(i2) + `<![CDATA[${e3}]]>` + this.newLine;
        }
        if (false !== this.options.commentPropName && e2 === this.options.commentPropName) {
          const e3 = kt(t2);
          return this.indentate(i2) + `<!--${e3}-->` + this.newLine;
        }
        if ("?" === e2[0]) return this.indentate(i2) + "<" + e2 + n2 + "?" + this.tagEndChar;
        {
          let s3 = this.options.tagValueProcessor(e2, t2);
          return s3 = this.replaceEntitiesValue(s3), "" === s3 ? this.indentate(i2) + "<" + e2 + n2 + this.closeTag(e2) + this.tagEndChar : this.indentate(i2) + "<" + e2 + n2 + ">" + s3 + "</" + e2 + this.tagEndChar;
        }
      }, Qt.prototype.replaceEntitiesValue = function(t2) {
        if (t2 && t2.length > 0 && this.options.processEntities) for (let e2 = 0; e2 < this.options.entities.length; e2++) {
          const n2 = this.options.entities[e2];
          t2 = t2.replace(n2.regex, n2.val);
        }
        return t2;
      };
      const ie = Qt, se = { validate: l };
      module.exports = e;
    })();
  }
});

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/package/package.ts
var import_jszip = __toESM(require_jszip_min(), 1);

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

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/xml.ts
var import_fast_xml_parser = __toESM(require_fxp(), 1);
var parser = new import_fast_xml_parser.XMLParser({
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
function normalizePartName(partName) {
  return normalizeOpcPath(partName.replace(/^\/+/, ""));
}
function localName(name) {
  return name.includes(":") ? name.split(":").pop() : name;
}

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/opc/relationships.ts
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
function getRelationshipById(set, id) {
  if (!set || !id) return void 0;
  return set.relationships.find((relationship) => relationship.id === id);
}
function getRelationshipsPathForPart(partPath) {
  return relationshipsPathForPart(normalizeOpcPath(partPath));
}
function isExternalWordPortRelationship(relationship) {
  return relationship.targetMode === WORD_PORT_EXTERNAL_TARGET_MODE;
}
function resolveWordPortRelationshipTarget(set, relationship) {
  if (!relationship.target || isExternalWordPortRelationship(relationship)) return void 0;
  return set.sourcePart ? resolveOpcTargetPath(set.sourcePart, relationship.target) : normalizeOpcPath(relationship.target);
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

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/package/package.ts
var TEXT_DECODER = new TextDecoder();
var TEXT_ENCODER = new TextEncoder();
async function openWordPortPackage(input, options = {}) {
  const diagnostics = new WordPortDiagnostics();
  diagnostics.merge(options.diagnostics);
  const zip = await import_jszip.default.loadAsync(input);
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

// archive/reorg-review/templates/business-basic/apps/web/lib/word-port/ooxml/ir.ts
var WORD_PORT_FIELD_REFERENCE_NAME = "w:noBreakHyphen";
var WORD_PORT_FIELD_ATTRIBUTE = "data-word-port-field";

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
      const level = readInteger(attr(override, "w:ilvl")) ?? 0;
      const start = readInteger(attr(firstChildByLocalName(override, "startOverride"), "w:val"));
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
  const ref = numberingReferenceFromAttrs(paragraphAttrs) ?? resolveStyleNumberingReference(numbering, stringAttr(paragraphAttrs?.styleId) ?? stringAttr(paragraphAttrs?.pStyle), state.styles);
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
  if (parsed.root?.name === rootName || localName2(parsed.root?.name) === localName2(rootName)) return parsed;
  const root = firstChildByLocalName(parsed.root, localName2(rootName));
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
    level: readInteger(attr(level, "w:ilvl")) ?? 0,
    start: readInteger(attr(firstChildByLocalName(level, "start"), "w:val")),
    restart: readInteger(attr(firstChildByLocalName(level, "lvlRestart"), "w:val")),
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
  if (direct?.numId) return { numberingId: direct.numId, level: readInteger(direct.level) ?? 0 };
  return resolveStyleNumberingReference(numbering, val(child(paragraph, "w:pStyle")), styles);
}
function resolveStyleNumberingReference(numbering, styleId, styles) {
  if (!numbering || !styleId) return void 0;
  const paragraphStyleId = resolveStyleIdForType(styles, styleId, "paragraph") ?? styleId;
  const styleNumbering = readNumbering(styles?.styles[paragraphStyleId]?.paragraph ?? createXmlNode("w:pPr"));
  if (styleNumbering?.numId) return { numberingId: styleNumbering.numId, level: readInteger(styleNumbering.level) ?? 0 };
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
  const numberingId = stringAttr(numbering?.numberingId) ?? stringAttr(numbering?.numId) ?? stringAttr(attrs?.numberingId) ?? stringAttr(attrs?.numId);
  if (!numberingId) return void 0;
  return {
    numberingId,
    level: readInteger(numbering?.level) ?? readInteger(attrs?.numberingLevel) ?? 0
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
  return node.attributes[name] ?? node.attributes[localName2(name)];
}
function childrenByLocalName(node, name) {
  return getChildren(node).filter((candidate) => localName2(candidate.name) === name);
}
function firstChildByLocalName(node, name) {
  return childrenByLocalName(node, name)[0];
}
function localName2(name) {
  return name?.includes(":") ? name.split(":").pop() ?? name : name ?? "";
}
function readInteger(value) {
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
function stringAttr(value) {
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
    ...(node.children ?? []).filter((child2) => localName3(child2.name) === "Choice"),
    ...(node.children ?? []).filter((child2) => localName3(child2.name) === "Fallback")
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
    const styleProperties = getChildren(style.raw).find((child2) => localName3(child2.name) === "tblStylePr" && tableXmlAttr(child2, "w:type") === styleType);
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
    const value = readPositiveInteger(tableXmlVal(firstChildByLocalName2(style.table, localName3(nodeName))));
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
    return [[localName3(child2.name), {
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
    return [[localName3(child2.name), { ...measurement, ...child2.attributes ? { attributes: { ...child2.attributes } } : {} }]];
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
  return getChildren(node).find((child2) => localName3(child2.name) === name);
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
function localName3(name) {
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
  importMarkdown
};
/*! Bundled license information:

jszip/dist/jszip.min.js:
  (*!
  
  JSZip v3.10.1 - A JavaScript class for generating and reading zip files
  <http://stuartk.com/jszip>
  
  (c) 2009-2016 Stuart Knightley <stuart [at] stuartk.com>
  Dual licenced under the MIT license or GPLv3. See https://raw.github.com/Stuk/jszip/main/LICENSE.markdown.
  
  JSZip uses the library pako released under the MIT license :
  https://github.com/nodeca/pako/blob/main/LICENSE
  *)
*/
