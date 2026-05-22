/**
 * CTOX Business OS - JSpreadsheet CE & jSuites unminified ESM Bundle
 * Generated dynamically to support full, offline-first ESM development.
 */

(function() {
  // Bind top-level global context so UMD wrapper registers correctly in ESM scope
  const window = this;
  const global = this;

  // --- START jSuites unminified code ---
  
;(function (global, factory) {
    typeof exports === 'object' && typeof module !== 'undefined' ? module.exports = factory() :
    typeof define === 'function' && define.amd ? define(factory) :
    global.jSuites = factory();
}(this, (function () {

var jSuites;
/******/ (function() { // webpackBootstrap
/******/ 	var __webpack_modules__ = ({

/***/ 195:
/***/ (function(module) {

/**
 * (c) jSuites Javascript Plugins and Web Components (v4)
 *
 * Website: https://jsuites.net
 * Description: Create amazing web based applications.
 * Plugin: Organogram
 *
 * MIT License
 */

;(function (global, factory) {
     true ? module.exports = factory() :
    0;
}(this, (function () {

    return (function(str) {
        function int64(msint_32, lsint_32) {
            this.highOrder = msint_32;
            this.lowOrder = lsint_32;
        }

        var H = [new int64(0x6a09e667, 0xf3bcc908), new int64(0xbb67ae85, 0x84caa73b),
            new int64(0x3c6ef372, 0xfe94f82b), new int64(0xa54ff53a, 0x5f1d36f1),
            new int64(0x510e527f, 0xade682d1), new int64(0x9b05688c, 0x2b3e6c1f),
            new int64(0x1f83d9ab, 0xfb41bd6b), new int64(0x5be0cd19, 0x137e2179)];

        var K = [new int64(0x428a2f98, 0xd728ae22), new int64(0x71374491, 0x23ef65cd),
            new int64(0xb5c0fbcf, 0xec4d3b2f), new int64(0xe9b5dba5, 0x8189dbbc),
            new int64(0x3956c25b, 0xf348b538), new int64(0x59f111f1, 0xb605d019),
            new int64(0x923f82a4, 0xaf194f9b), new int64(0xab1c5ed5, 0xda6d8118),
            new int64(0xd807aa98, 0xa3030242), new int64(0x12835b01, 0x45706fbe),
            new int64(0x243185be, 0x4ee4b28c), new int64(0x550c7dc3, 0xd5ffb4e2),
            new int64(0x72be5d74, 0xf27b896f), new int64(0x80deb1fe, 0x3b1696b1),
            new int64(0x9bdc06a7, 0x25c71235), new int64(0xc19bf174, 0xcf692694),
            new int64(0xe49b69c1, 0x9ef14ad2), new int64(0xefbe4786, 0x384f25e3),
            new int64(0x0fc19dc6, 0x8b8cd5b5), new int64(0x240ca1cc, 0x77ac9c65),
            new int64(0x2de92c6f, 0x592b0275), new int64(0x4a7484aa, 0x6ea6e483),
            new int64(0x5cb0a9dc, 0xbd41fbd4), new int64(0x76f988da, 0x831153b5),
            new int64(0x983e5152, 0xee66dfab), new int64(0xa831c66d, 0x2db43210),
            new int64(0xb00327c8, 0x98fb213f), new int64(0xbf597fc7, 0xbeef0ee4),
            new int64(0xc6e00bf3, 0x3da88fc2), new int64(0xd5a79147, 0x930aa725),
            new int64(0x06ca6351, 0xe003826f), new int64(0x14292967, 0x0a0e6e70),
            new int64(0x27b70a85, 0x46d22ffc), new int64(0x2e1b2138, 0x5c26c926),
            new int64(0x4d2c6dfc, 0x5ac42aed), new int64(0x53380d13, 0x9d95b3df),
            new int64(0x650a7354, 0x8baf63de), new int64(0x766a0abb, 0x3c77b2a8),
            new int64(0x81c2c92e, 0x47edaee6), new int64(0x92722c85, 0x1482353b),
            new int64(0xa2bfe8a1, 0x4cf10364), new int64(0xa81a664b, 0xbc423001),
            new int64(0xc24b8b70, 0xd0f89791), new int64(0xc76c51a3, 0x0654be30),
            new int64(0xd192e819, 0xd6ef5218), new int64(0xd6990624, 0x5565a910),
            new int64(0xf40e3585, 0x5771202a), new int64(0x106aa070, 0x32bbd1b8),
            new int64(0x19a4c116, 0xb8d2d0c8), new int64(0x1e376c08, 0x5141ab53),
            new int64(0x2748774c, 0xdf8eeb99), new int64(0x34b0bcb5, 0xe19b48a8),
            new int64(0x391c0cb3, 0xc5c95a63), new int64(0x4ed8aa4a, 0xe3418acb),
            new int64(0x5b9cca4f, 0x7763e373), new int64(0x682e6ff3, 0xd6b2b8a3),
            new int64(0x748f82ee, 0x5defb2fc), new int64(0x78a5636f, 0x43172f60),
            new int64(0x84c87814, 0xa1f0ab72), new int64(0x8cc70208, 0x1a6439ec),
            new int64(0x90befffa, 0x23631e28), new int64(0xa4506ceb, 0xde82bde9),
            new int64(0xbef9a3f7, 0xb2c67915), new int64(0xc67178f2, 0xe372532b),
            new int64(0xca273ece, 0xea26619c), new int64(0xd186b8c7, 0x21c0c207),
            new int64(0xeada7dd6, 0xcde0eb1e), new int64(0xf57d4f7f, 0xee6ed178),
            new int64(0x06f067aa, 0x72176fba), new int64(0x0a637dc5, 0xa2c898a6),
            new int64(0x113f9804, 0xbef90dae), new int64(0x1b710b35, 0x131c471b),
            new int64(0x28db77f5, 0x23047d84), new int64(0x32caab7b, 0x40c72493),
            new int64(0x3c9ebe0a, 0x15c9bebc), new int64(0x431d67c4, 0x9c100d4c),
            new int64(0x4cc5d4be, 0xcb3e42b6), new int64(0x597f299c, 0xfc657e2a),
            new int64(0x5fcb6fab, 0x3ad6faec), new int64(0x6c44198c, 0x4a475817)];

        var W = new Array(64);
        var a, b, c, d, e, f, g, h, i, j;
        var T1, T2;
        var charsize = 8;

        function utf8_encode(str) {
            return unescape(encodeURIComponent(str));
        }

        function str2binb(str) {
            var bin = [];
            var mask = (1 << charsize) - 1;
            var len = str.length * charsize;

            for (var i = 0; i < len; i += charsize) {
                bin[i >> 5] |= (str.charCodeAt(i / charsize) & mask) << (32 - charsize - (i % 32));
            }

            return bin;
        }

        function binb2hex(binarray) {
            var hex_tab = "0123456789abcdef";
            var str = "";
            var length = binarray.length * 4;
            var srcByte;

            for (var i = 0; i < length; i += 1) {
                srcByte = binarray[i >> 2] >> ((3 - (i % 4)) * 8);
                str += hex_tab.charAt((srcByte >> 4) & 0xF) + hex_tab.charAt(srcByte & 0xF);
            }

            return str;
        }

        function safe_add_2(x, y) {
            var lsw, msw, lowOrder, highOrder;

            lsw = (x.lowOrder & 0xFFFF) + (y.lowOrder & 0xFFFF);
            msw = (x.lowOrder >>> 16) + (y.lowOrder >>> 16) + (lsw >>> 16);
            lowOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            lsw = (x.highOrder & 0xFFFF) + (y.highOrder & 0xFFFF) + (msw >>> 16);
            msw = (x.highOrder >>> 16) + (y.highOrder >>> 16) + (lsw >>> 16);
            highOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            return new int64(highOrder, lowOrder);
        }

        function safe_add_4(a, b, c, d) {
            var lsw, msw, lowOrder, highOrder;

            lsw = (a.lowOrder & 0xFFFF) + (b.lowOrder & 0xFFFF) + (c.lowOrder & 0xFFFF) + (d.lowOrder & 0xFFFF);
            msw = (a.lowOrder >>> 16) + (b.lowOrder >>> 16) + (c.lowOrder >>> 16) + (d.lowOrder >>> 16) + (lsw >>> 16);
            lowOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            lsw = (a.highOrder & 0xFFFF) + (b.highOrder & 0xFFFF) + (c.highOrder & 0xFFFF) + (d.highOrder & 0xFFFF) + (msw >>> 16);
            msw = (a.highOrder >>> 16) + (b.highOrder >>> 16) + (c.highOrder >>> 16) + (d.highOrder >>> 16) + (lsw >>> 16);
            highOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            return new int64(highOrder, lowOrder);
        }

        function safe_add_5(a, b, c, d, e) {
            var lsw, msw, lowOrder, highOrder;

            lsw = (a.lowOrder & 0xFFFF) + (b.lowOrder & 0xFFFF) + (c.lowOrder & 0xFFFF) + (d.lowOrder & 0xFFFF) + (e.lowOrder & 0xFFFF);
            msw = (a.lowOrder >>> 16) + (b.lowOrder >>> 16) + (c.lowOrder >>> 16) + (d.lowOrder >>> 16) + (e.lowOrder >>> 16) + (lsw >>> 16);
            lowOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            lsw = (a.highOrder & 0xFFFF) + (b.highOrder & 0xFFFF) + (c.highOrder & 0xFFFF) + (d.highOrder & 0xFFFF) + (e.highOrder & 0xFFFF) + (msw >>> 16);
            msw = (a.highOrder >>> 16) + (b.highOrder >>> 16) + (c.highOrder >>> 16) + (d.highOrder >>> 16) + (e.highOrder >>> 16) + (lsw >>> 16);
            highOrder = ((msw & 0xFFFF) << 16) | (lsw & 0xFFFF);

            return new int64(highOrder, lowOrder);
        }

        function maj(x, y, z) {
            return new int64(
                (x.highOrder & y.highOrder) ^ (x.highOrder & z.highOrder) ^ (y.highOrder & z.highOrder),
                (x.lowOrder & y.lowOrder) ^ (x.lowOrder & z.lowOrder) ^ (y.lowOrder & z.lowOrder)
            );
        }

        function ch(x, y, z) {
            return new int64(
                (x.highOrder & y.highOrder) ^ (~x.highOrder & z.highOrder),
                (x.lowOrder & y.lowOrder) ^ (~x.lowOrder & z.lowOrder)
            );
        }

        function rotr(x, n) {
            if (n <= 32) {
                return new int64(
                    (x.highOrder >>> n) | (x.lowOrder << (32 - n)),
                    (x.lowOrder >>> n) | (x.highOrder << (32 - n))
                );
            } else {
                return new int64(
                    (x.lowOrder >>> n) | (x.highOrder << (32 - n)),
                    (x.highOrder >>> n) | (x.lowOrder << (32 - n))
                );
            }
        }

        function sigma0(x) {
            var rotr28 = rotr(x, 28);
            var rotr34 = rotr(x, 34);
            var rotr39 = rotr(x, 39);

            return new int64(
                rotr28.highOrder ^ rotr34.highOrder ^ rotr39.highOrder,
                rotr28.lowOrder ^ rotr34.lowOrder ^ rotr39.lowOrder
            );
        }

        function sigma1(x) {
            var rotr14 = rotr(x, 14);
            var rotr18 = rotr(x, 18);
            var rotr41 = rotr(x, 41);

            return new int64(
                rotr14.highOrder ^ rotr18.highOrder ^ rotr41.highOrder,
                rotr14.lowOrder ^ rotr18.lowOrder ^ rotr41.lowOrder
            );
        }

        function gamma0(x) {
            var rotr1 = rotr(x, 1), rotr8 = rotr(x, 8), shr7 = shr(x, 7);

            return new int64(
                rotr1.highOrder ^ rotr8.highOrder ^ shr7.highOrder,
                rotr1.lowOrder ^ rotr8.lowOrder ^ shr7.lowOrder
            );
        }

        function gamma1(x) {
            var rotr19 = rotr(x, 19);
            var rotr61 = rotr(x, 61);
            var shr6 = shr(x, 6);

            return new int64(
                rotr19.highOrder ^ rotr61.highOrder ^ shr6.highOrder,
                rotr19.lowOrder ^ rotr61.lowOrder ^ shr6.lowOrder
            );
        }

        function shr(x, n) {
            if (n <= 32) {
                return new int64(
                    x.highOrder >>> n,
                    x.lowOrder >>> n | (x.highOrder << (32 - n))
                );
            } else {
                return new int64(
                    0,
                    x.highOrder << (32 - n)
                );
            }
        }

        var str = utf8_encode(str);
        var strlen = str.length*charsize;
        str = str2binb(str);

        str[strlen >> 5] |= 0x80 << (24 - strlen % 32);
        str[(((strlen + 128) >> 10) << 5) + 31] = strlen;

        for (var i = 0; i < str.length; i += 32) {
            a = H[0];
            b = H[1];
            c = H[2];
            d = H[3];
            e = H[4];
            f = H[5];
            g = H[6];
            h = H[7];

            for (var j = 0; j < 80; j++) {
                if (j < 16) {
                    W[j] = new int64(str[j*2 + i], str[j*2 + i + 1]);
                } else {
                    W[j] = safe_add_4(gamma1(W[j - 2]), W[j - 7], gamma0(W[j - 15]), W[j - 16]);
                }

                T1 = safe_add_5(h, sigma1(e), ch(e, f, g), K[j], W[j]);
                T2 = safe_add_2(sigma0(a), maj(a, b, c));
                h = g;
                g = f;
                f = e;
                e = safe_add_2(d, T1);
                d = c;
                c = b;
                b = a;
                a = safe_add_2(T1, T2);
            }

            H[0] = safe_add_2(a, H[0]);
            H[1] = safe_add_2(b, H[1]);
            H[2] = safe_add_2(c, H[2]);
            H[3] = safe_add_2(d, H[3]);
            H[4] = safe_add_2(e, H[4]);
            H[5] = safe_add_2(f, H[5]);
            H[6] = safe_add_2(g, H[6]);
            H[7] = safe_add_2(h, H[7]);
        }

        var binarray = [];
        for (var i = 0; i < H.length; i++) {
            binarray.push(H[i].highOrder);
            binarray.push(H[i].lowOrder);
        }

        return binb2hex(binarray);
    });

})));


/***/ })

/******/ 	});
/************************************************************************/
/******/ 	// The module cache
/******/ 	var __webpack_module_cache__ = {};
/******/ 	
/******/ 	// The require function
/******/ 	function __webpack_require__(moduleId) {
/******/ 		// Check if module is in cache
/******/ 		var cachedModule = __webpack_module_cache__[moduleId];
/******/ 		if (cachedModule !== undefined) {
/******/ 			return cachedModule.exports;
/******/ 		}
/******/ 		// Create a new module (and put it into the cache)
/******/ 		var module = __webpack_module_cache__[moduleId] = {
/******/ 			// no module.id needed
/******/ 			// no module.loaded needed
/******/ 			exports: {}
/******/ 		};
/******/ 	
/******/ 		// Execute the module function
/******/ 		__webpack_modules__[moduleId].call(module.exports, module, module.exports, __webpack_require__);
/******/ 	
/******/ 		// Return the exports of the module
/******/ 		return module.exports;
/******/ 	}
/******/ 	
/************************************************************************/
/******/ 	/* webpack/runtime/compat get default export */
/******/ 	!function() {
/******/ 		// getDefaultExport function for compatibility with non-harmony modules
/******/ 		__webpack_require__.n = function(module) {
/******/ 			var getter = module && module.__esModule ?
/******/ 				function() { return module['default']; } :
/******/ 				function() { return module; };
/******/ 			__webpack_require__.d(getter, { a: getter });
/******/ 			return getter;
/******/ 		};
/******/ 	}();
/******/ 	
/******/ 	/* webpack/runtime/define property getters */
/******/ 	!function() {
/******/ 		// define getter functions for harmony exports
/******/ 		__webpack_require__.d = function(exports, definition) {
/******/ 			for(var key in definition) {
/******/ 				if(__webpack_require__.o(definition, key) && !__webpack_require__.o(exports, key)) {
/******/ 					Object.defineProperty(exports, key, { enumerable: true, get: definition[key] });
/******/ 				}
/******/ 			}
/******/ 		};
/******/ 	}();
/******/ 	
/******/ 	/* webpack/runtime/hasOwnProperty shorthand */
/******/ 	!function() {
/******/ 		__webpack_require__.o = function(obj, prop) { return Object.prototype.hasOwnProperty.call(obj, prop); }
/******/ 	}();
/******/ 	
/************************************************************************/
var __webpack_exports__ = {};
// This entry need to be wrapped in an IIFE because it need to be in strict mode.
!function() {
"use strict";

// EXPORTS
__webpack_require__.d(__webpack_exports__, {
  "default": function() { return /* binding */ jsuites; }
});

;// CONCATENATED MODULE: ./src/utils/dictionary.js
// Update dictionary
var setDictionary = function(d) {
    if (! document.dictionary) {
        document.dictionary = {}
    }
    // Replace the key into the dictionary and append the new ones
    var t = null;
    var i = null;
    var k = Object.keys(d);
    for (i = 0; i < k.length; i++) {
        document.dictionary[k[i]] = d[k[i]];
    }
}

// Translate
var translate = function(t) {
    if (typeof(document) !== "undefined" && document.dictionary) {
        return document.dictionary[t] || t;
    } else {
        return t;
    }
}


/* harmony default export */ var dictionary = ({ setDictionary, translate });
;// CONCATENATED MODULE: ./src/utils/tracking.js
 const Tracking = function(component, state) {
    if (state === true) {
        window['jSuitesStateControl'] = window['jSuitesStateControl'].filter(function(v) {
            return v !== null;
        });

        // Start after all events
        setTimeout(function() {
            window['jSuitesStateControl'].push(component);
        }, 0);

    } else {
        var index = window['jSuitesStateControl'].indexOf(component);
        if (index >= 0) {
            window['jSuitesStateControl'].splice(index, 1);
        }
    }
}

/* harmony default export */ var tracking = (Tracking);
;// CONCATENATED MODULE: ./src/utils/helpers.js
var Helpers = {};

// Two digits
Helpers.two = function(value) {
    value = '' + value;
    if (value.length == 1) {
        value = '0' + value;
    }
    return value;
}

Helpers.focus = function(el) {
    if (el.textContent.length) {
        // Handle contenteditable elements
        const range = document.createRange();
        const sel = window.getSelection();

        let node = el;
        // Go as deep as possible to the last text node
        while (node.lastChild) node = node.lastChild;
        // Ensure it's a text node
        if (node.nodeType === Node.TEXT_NODE) {
            range.setStart(node, node.length);
        } else {
            range.setStart(node, node.childNodes.length);
        }
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);

        el.scrollLeft = el.scrollWidth;
    }
}

Helpers.isNumeric = (function (num) {
    if (typeof(num) === 'string') {
        num = num.trim();
    }
    return !isNaN(num) && num !== null && num !== '';
});

Helpers.guid = function() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
        var r = Math.random() * 16 | 0, v = c == 'x' ? r : (r & 0x3 | 0x8);
        return v.toString(16);
    });
}

Helpers.getNode = function() {
    var node = document.getSelection().anchorNode;
    if (node) {
        return (node.nodeType == 3 ? node.parentNode : node);
    } else {
        return null;
    }
}
/**
 * Generate hash from a string
 */
Helpers.hash = function(str) {
    var hash = 0, i, chr;

    if (str.length === 0) {
        return hash;
    } else {
        for (i = 0; i < str.length; i++) {
            chr = str.charCodeAt(i);
            if (chr > 32) {
                hash = ((hash << 5) - hash) + chr;
                hash |= 0;
            }
        }
    }
    return hash;
}

/**
 * Generate a random color
 */
Helpers.randomColor = function(h) {
    var lum = -0.25;
    var hex = String('#' + Math.random().toString(16).slice(2, 8).toUpperCase()).replace(/[^0-9a-f]/gi, '');
    if (hex.length < 6) {
        hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
    }
    var rgb = [], c, i;
    for (i = 0; i < 3; i++) {
        c = parseInt(hex.substr(i * 2, 2), 16);
        c = Math.round(Math.min(Math.max(0, c + (c * lum)), 255)).toString(16);
        rgb.push(("00" + c).substr(c.length));
    }

    // Return hex
    if (h == true) {
        return '#' + Helpers.two(rgb[0].toString(16)) + Helpers.two(rgb[1].toString(16)) + Helpers.two(rgb[2].toString(16));
    }

    return rgb;
}

Helpers.getWindowWidth = function() {
    var w = window,
    d = document,
    e = d.documentElement,
    g = d.getElementsByTagName('body')[0],
    x = w.innerWidth || e.clientWidth || g.clientWidth;
    return x;
}

Helpers.getWindowHeight = function() {
    var w = window,
    d = document,
    e = d.documentElement,
    g = d.getElementsByTagName('body')[0],
    y = w.innerHeight|| e.clientHeight|| g.clientHeight;
    return  y;
}

Helpers.getPosition = function(e) {
    if (e.changedTouches && e.changedTouches[0]) {
        var x = e.changedTouches[0].pageX;
        var y = e.changedTouches[0].pageY;
    } else {
        var x = (window.Event) ? e.pageX : e.clientX + (document.documentElement.scrollLeft ? document.documentElement.scrollLeft : document.body.scrollLeft);
        var y = (window.Event) ? e.pageY : e.clientY + (document.documentElement.scrollTop ? document.documentElement.scrollTop : document.body.scrollTop);
    }

    return [ x, y ];
}

Helpers.click = function(el) {
    if (el.click) {
        el.click();
    } else {
        var evt = new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
            view: window
        });
        el.dispatchEvent(evt);
    }
}

Helpers.findElement = function(element, condition) {
    var foundElement = false;

    function path (element) {
        if (element && ! foundElement) {
            if (typeof(condition) == 'function') {
                foundElement = condition(element)
            } else if (typeof(condition) == 'string') {
                if (element.classList && element.classList.contains(condition)) {
                    foundElement = element;
                }
            }
        }

        if (element.parentNode && ! foundElement) {
            path(element.parentNode);
        }
    }

    path(element);

    return foundElement;
}

/* harmony default export */ var helpers = (Helpers);
;// CONCATENATED MODULE: ./src/utils/path.js
const isValidPathObj = function(o) {
    return typeof o === 'object' || typeof o === 'function';
}

function Path(pathString, value, remove) {
    // Ensure the path is a valid, non-empty string
    if (typeof pathString !== 'string' || pathString.length === 0) {
        return undefined;
    }

    // Split the path into individual keys and filter out empty keys
    const keys = pathString.split('.');
    if (keys.length === 0) {
        return undefined;
    }

    // Start with the root object
    let currentObject = this;

    // Read mode: retrieve a value
    if (typeof value === 'undefined' && typeof remove === 'undefined') {
        // Traverse all keys
        for (let i = 0; i < keys.length; i++) {
            const key = keys[i];
            // Check if the current object is valid and has the key
            if (
                currentObject != null &&
                isValidPathObj(currentObject) &&
                Object.prototype.hasOwnProperty.call(currentObject, key)
            ) {
                currentObject = currentObject[key];
            } else {
                // Return undefined if the path is invalid or currentObject is null/undefined
                return undefined;
            }
        }
        // Return the final value
        return currentObject;
    }

    // Write mode: set or delete a value
    // Traverse all keys except the last one
    for (let i = 0; i < keys.length - 1; i++) {
        const key = keys[i];

        // Check if the current object is invalid (null/undefined or non-object)
        if (currentObject == null || ! isValidPathObj(currentObject)) {
            console.warn(`Cannot set value: path '${pathString}' blocked by invalid object at '${key}'`);
            return false;
        }

        // If the key exists but is null/undefined or a non-object, replace it with an empty object
        if (
            Object.prototype.hasOwnProperty.call(currentObject, key) &&
            (currentObject[key] == null || ! isValidPathObj(currentObject[key]))
        ) {
            currentObject[key] = {};
        } else if (!Object.prototype.hasOwnProperty.call(currentObject, key)) {
            // If the key doesn't exist, create an empty object
            currentObject[key] = {};
        }

        // Move to the next level
        currentObject = currentObject[key];
    }

    // Handle the final key
    const finalKey = keys[keys.length - 1];

    // Check if the current object is valid for setting/deleting
    if (currentObject == null || ! isValidPathObj(currentObject)) {
        return false;
    }

    // Delete the property if remove is true
    if (remove === true) {
        if (Object.prototype.hasOwnProperty.call(currentObject, finalKey)) {
            delete currentObject[finalKey];
            return true;
        }
        return false; // Nothing to delete
    }

    // Set the value
    currentObject[finalKey] = value;
    return true;
}
;// CONCATENATED MODULE: ./src/utils/sorting.js
function Sorting(el, options) {
    var obj = {};
    obj.options = {};

    var defaults = {
        pointer: null,
        direction: null,
        ondragstart: null,
        ondragend: null,
        ondrop: null,
    }

    var dragElement = null;

    // Loop through the initial configuration
    for (var property in defaults) {
        if (options && options.hasOwnProperty(property)) {
            obj.options[property] = options[property];
        } else {
            obj.options[property] = defaults[property];
        }
    }

    el.classList.add('jsorting');

    el.addEventListener('dragstart', function(e) {
        let target = e.target;
        if (target.nodeType === 3) {
            if (target.parentNode.getAttribute('draggable') === 'true') {
                target = target.parentNode;
            } else {
                e.preventDefault();
                e.stopPropagation();
                return;
            }
        }

        if (target.getAttribute('draggable') === 'true') {
            let position = Array.prototype.indexOf.call(target.parentNode.children, target);
            dragElement = {
                element: target,
                o: position,
                d: position
            }
            target.style.opacity = '0.25';

            if (typeof (obj.options.ondragstart) == 'function') {
                obj.options.ondragstart(el, target, e);
            }

            e.dataTransfer.setDragImage(target,0,0);
        }
    });

    el.addEventListener('dragover', function(e) {
        e.preventDefault();

        if (dragElement) {
            if (getElement(e.target)) {
                if (e.target.getAttribute('draggable') == 'true' && dragElement.element != e.target) {
                    if (!obj.options.direction) {
                        var condition = e.target.clientHeight / 2 > e.offsetY;
                    } else {
                        var condition = e.target.clientWidth / 2 > e.offsetX;
                    }

                    if (condition) {
                        e.target.parentNode.insertBefore(dragElement.element, e.target);
                    } else {
                        e.target.parentNode.insertBefore(dragElement.element, e.target.nextSibling);
                    }

                    dragElement.d = Array.prototype.indexOf.call(e.target.parentNode.children, dragElement.element);
                }
            }
        }
    });

    el.addEventListener('dragleave', function(e) {
        e.preventDefault();
    });

    el.addEventListener('dragend', function(e) {
        e.preventDefault();

        if (dragElement) {
            if (typeof(obj.options.ondragend) == 'function') {
                obj.options.ondragend(el, dragElement.element, e);
            }

            // Cancelled put element to the original position
            if (dragElement.o < dragElement.d) {
                e.target.parentNode.insertBefore(dragElement.element, e.target.parentNode.children[dragElement.o]);
            } else {
                e.target.parentNode.insertBefore(dragElement.element, e.target.parentNode.children[dragElement.o].nextSibling);
            }

            dragElement.element.style.opacity = '';
            dragElement = null;
        }
    });

    el.addEventListener('drop', function(e) {
        e.preventDefault();

        if (dragElement) {
            if (dragElement.o !== dragElement.d) {
                if (typeof (obj.options.ondrop) == 'function') {
                    obj.options.ondrop(el, dragElement.o, dragElement.d, dragElement.element, e.target, e);
                }
            }

            dragElement.element.style.opacity = '';
            dragElement = null;
        }
    });

    var getElement = function(element) {
        var sorting = false;

        function path (element) {
            if (element.className) {
                if (element.classList.contains('jsorting')) {
                    sorting = true;
                }
            }

            if (! sorting) {
                path(element.parentNode);
            }
        }

        path(element);

        return sorting;
    }

    for (var i = 0; i < el.children.length; i++) {
        if (! el.children[i].hasAttribute('draggable')) {
            el.children[i].setAttribute('draggable', 'true');
        }
    }

    el.val = function() {
        var id = null;
        var data = [];
        for (var i = 0; i < el.children.length; i++) {
            if (id = el.children[i].getAttribute('data-id')) {
                data.push(id);
            }
        }
        return data;
    }

    return el;
}
;// CONCATENATED MODULE: ./src/utils/lazyloading.js
function LazyLoading(el, options) {
    var obj = {}

    // Mandatory options
    if (! options.loadUp || typeof(options.loadUp) != 'function') {
        options.loadUp = function() {
            return false;
        }
    }
    if (! options.loadDown || typeof(options.loadDown) != 'function') {
        options.loadDown = function() {
            return false;
        }
    }
    // Timer ms
    if (! options.timer) {
        options.timer = 100;
    }

    // Timer
    var timeControlLoading = null;

    // Controls
    var scrollControls = function(e) {
        if (timeControlLoading == null) {
            var event = false;
            var scrollTop = el.scrollTop;
            if (el.scrollTop + (el.clientHeight * 2) >= el.scrollHeight) {
                if (options.loadDown()) {
                    if (scrollTop == el.scrollTop) {
                        el.scrollTop = el.scrollTop - (el.clientHeight);
                    }
                    event = true;
                }
            } else if (el.scrollTop <= el.clientHeight) {
                if (options.loadUp()) {
                    if (scrollTop == el.scrollTop) {
                        el.scrollTop = el.scrollTop + (el.clientHeight);
                    }
                    event = true;
                }
            }

            timeControlLoading = setTimeout(function() {
                timeControlLoading = null;
            }, options.timer);

            if (event) {
                if (typeof(options.onupdate) == 'function') {
                    options.onupdate();
                }
            }
        }
    }

    // Onscroll
    el.onscroll = function(e) {
        scrollControls(e);
    }

    el.onwheel = function(e) {
        scrollControls(e);
    }

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/ajax.js
function Ajax() {
    var Component = (function(options, complete) {
        if (Array.isArray(options)) {
            // Create multiple request controller
            var multiple = {
                instance: [],
                complete: complete,
            }

            if (options.length > 0) {
                for (var i = 0; i < options.length; i++) {
                    options[i].multiple = multiple;
                    multiple.instance.push(Component(options[i]));
                }
            }

            return multiple;
        }

        if (! options.data) {
            options.data = {};
        }

        if (options.type) {
            options.method = options.type;
        }

        // Default method
        if (! options.method) {
            options.method = 'GET';
        }

        // Default type
        if (! options.dataType) {
            options.dataType = 'json';
        }

        if (options.data) {
            // Parse object to variables format
            var parseData = function (value, key) {
                var vars = [];
                if (value) {
                    var keys = Object.keys(value);
                    if (keys.length) {
                        for (var i = 0; i < keys.length; i++) {
                            if (key) {
                                var k = key + '[' + keys[i] + ']';
                            } else {
                                var k = keys[i];
                            }

                            if (value[k] instanceof FileList) {
                                vars[k] = value[keys[i]];
                            } else if (value[keys[i]] === null || value[keys[i]] === undefined) {
                                vars[k] = '';
                            } else if (typeof(value[keys[i]]) == 'object') {
                                var r = parseData(value[keys[i]], k);
                                var o = Object.keys(r);
                                for (var j = 0; j < o.length; j++) {
                                    vars[o[j]] = r[o[j]];
                                }
                            } else {
                                vars[k] = value[keys[i]];
                            }
                        }
                    }
                }

                return vars;
            }

            var d = parseData(options.data);
            var k = Object.keys(d);

            // Data form
            if (options.method == 'GET') {
                if (k.length) {
                    var data = [];
                    for (var i = 0; i < k.length; i++) {
                        data.push(k[i] + '=' + encodeURIComponent(d[k[i]]));
                    }

                    if (options.url.indexOf('?') < 0) {
                        options.url += '?';
                    }
                    options.url += data.join('&');
                }
            } else {
                var data = new FormData();
                for (var i = 0; i < k.length; i++) {
                    if (d[k[i]] instanceof FileList) {
                        if (d[k[i]].length) {
                            for (var j = 0; j < d[k[i]].length; j++) {
                                data.append(k[i], d[k[i]][j], d[k[i]][j].name);
                            }
                        }
                    } else {
                        data.append(k[i], d[k[i]]);
                    }
                }
            }
        }

        var httpRequest = new XMLHttpRequest();
        httpRequest.open(options.method, options.url, true);

        if (options.requestedWith) {
            httpRequest.setRequestHeader('X-Requested-With', options.requestedWith);
        } else {
            if (options.requestedWith !== false) {
                httpRequest.setRequestHeader('X-Requested-With', 'XMLHttpRequest');
            }
        }

        // Content type
        if (options.contentType) {
            httpRequest.setRequestHeader('Content-Type', options.contentType);
        }

        // Headers
        if (options.method === 'POST') {
            httpRequest.setRequestHeader('Accept', 'application/json');
        } else {
            if (options.dataType === 'blob') {
                httpRequest.responseType = "blob";
            } else {
                if (! options.contentType) {
                    if (options.dataType === 'json') {
                        httpRequest.setRequestHeader('Content-Type', 'text/json');
                    } else if (options.dataType === 'html') {
                        httpRequest.setRequestHeader('Content-Type', 'text/html');
                    }
                }
            }
        }

        // No cache
        if (options.cache !== true) {
            httpRequest.setRequestHeader('pragma', 'no-cache');
            httpRequest.setRequestHeader('cache-control', 'no-cache');
        }

        // Authentication
        if (options.withCredentials === true) {
            httpRequest.withCredentials = true
        }

        // Before send
        if (typeof(options.beforeSend) == 'function') {
            options.beforeSend(httpRequest);
        }

        // Before send
        if (typeof(Component.beforeSend) == 'function') {
            Component.beforeSend(httpRequest);
        }

        if (document.ajax && typeof(document.ajax.beforeSend) == 'function') {
            document.ajax.beforeSend(httpRequest);
        }

        httpRequest.onerror = function() {
            if (options.error && typeof(options.error) == 'function') {
                options.error({
                    message: 'Network error: Unable to reach the server.',
                    status: 0
                });
            }
        }

        httpRequest.ontimeout = function() {
            if (options.error && typeof(options.error) == 'function') {
                options.error({
                    message: 'Request timed out after ' + httpRequest.timeout + 'ms.',
                    status: 0
                });
            }
        }

        httpRequest.onload = function() {
            if (httpRequest.status >= 200 && httpRequest.status < 300) {
                if (options.dataType === 'json') {
                    try {
                        var result = JSON.parse(httpRequest.responseText);

                        if (options.success && typeof(options.success) == 'function') {
                            options.success(result);
                        }
                    } catch(err) {
                        if (options.error && typeof(options.error) == 'function') {
                            options.error(err, result);
                        }
                    }
                } else {
                    if (options.dataType === 'blob') {
                        var result = httpRequest.response;
                    } else {
                        var result = httpRequest.responseText;
                    }

                    if (options.success && typeof(options.success) == 'function') {
                        options.success(result);
                    }
                }
            } else {
                if (options.error && typeof(options.error) == 'function') {
                    options.error(httpRequest.responseText, httpRequest.status);
                }
            }

            // Global queue
            if (Component.queue && Component.queue.length > 0) {
                Component.send(Component.queue.shift());
            }

            // Global complete method
            if (Component.requests && Component.requests.length) {
                // Get index of this request in the container
                var index = Component.requests.indexOf(httpRequest);
                // Remove from the ajax requests container
                Component.requests.splice(index, 1);
                // Deprecated: Last one?
                if (! Component.requests.length) {
                    // Object event
                    if (options.complete && typeof(options.complete) == 'function') {
                        options.complete(result);
                    }
                }
                // Group requests
                if (options.group) {
                    if (Component.oncomplete && typeof(Component.oncomplete[options.group]) == 'function') {
                        if (! Component.pending(options.group)) {
                            Component.oncomplete[options.group]();
                            Component.oncomplete[options.group] = null;
                        }
                    }
                }
                // Multiple requests controller
                if (options.multiple && options.multiple.instance) {
                    // Get index of this request in the container
                    var index = options.multiple.instance.indexOf(httpRequest);
                    // Remove from the ajax requests container
                    options.multiple.instance.splice(index, 1);
                    // If this is the last one call method complete
                    if (! options.multiple.instance.length) {
                        if (options.multiple.complete && typeof(options.multiple.complete) == 'function') {
                            options.multiple.complete(result);
                        }
                    }
                }
            }
        }

        // Keep the options
        httpRequest.options = options;
        // Data
        httpRequest.data = data;

        // Queue
        if (options.queue === true && Component.requests.length > 0) {
            Component.queue.push(httpRequest);
        } else {
            Component.send(httpRequest)
        }

        return httpRequest;
    });

    Component.send = function(httpRequest) {
        if (httpRequest.data) {
            if (Array.isArray(httpRequest.data)) {
                httpRequest.send(httpRequest.data.join('&'));
            } else {
                httpRequest.send(httpRequest.data);
            }
        } else {
            httpRequest.send();
        }

        Component.requests.push(httpRequest);
    }

    Component.exists = function(url, __callback) {
        var http = new XMLHttpRequest();
        http.open('HEAD', url, false);
        http.send();
        if (http.status) {
            __callback(http.status);
        }
    }

    Component.pending = function(group) {
        var n = 0;
        var o = Component.requests;
        if (o && o.length) {
            for (var i = 0; i < o.length; i++) {
                if (! group || group == o[i].options.group) {
                    n++
                }
            }
        }
        return n;
    }

    Component.oncomplete = {};
    Component.requests = [];
    Component.queue = [];

    return Component
}

/* harmony default export */ var ajax = (Ajax());
;// CONCATENATED MODULE: ./src/plugins/animation.js
function Animation() {
    const Component = {
        loading: {}
    }
    
    Component.loading.show = function(timeout) {
        if (! Component.loading.element) {
            Component.loading.element = document.createElement('div');
            Component.loading.element.className = 'jloading';
        }
        document.body.appendChild(Component.loading.element);
    
        // Max timeout in seconds
        if (timeout > 0) {
            setTimeout(function() {
                Component.loading.hide();
            }, timeout * 1000)
        }
    }
    
    Component.loading.hide = function() {
        if (Component.loading.element && Component.loading.element.parentNode) {
            document.body.removeChild(Component.loading.element);
        }
    }
    
    Component.slideLeft = function (element, direction, done) {
        if (direction == true) {
            element.classList.add('jslide-left-in');
            setTimeout(function () {
                element.classList.remove('jslide-left-in');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        } else {
            element.classList.add('jslide-left-out');
            setTimeout(function () {
                element.classList.remove('jslide-left-out');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        }
    }
    
    Component.slideRight = function (element, direction, done) {
        if (direction === true) {
            element.classList.add('jslide-right-in');
            setTimeout(function () {
                element.classList.remove('jslide-right-in');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        } else {
            element.classList.add('jslide-right-out');
            setTimeout(function () {
                element.classList.remove('jslide-right-out');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        }
    }
    
    Component.slideTop = function (element, direction, done) {
        if (direction === true) {
            element.classList.add('jslide-top-in');
            setTimeout(function () {
                element.classList.remove('jslide-top-in');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        } else {
            element.classList.add('jslide-top-out');
            setTimeout(function () {
                element.classList.remove('jslide-top-out');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        }
    }
    
    Component.slideBottom = function (element, direction, done) {
        if (direction === true) {
            element.classList.add('jslide-bottom-in');
            setTimeout(function () {
                element.classList.remove('jslide-bottom-in');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 400);
        } else {
            element.classList.add('jslide-bottom-out');
            setTimeout(function () {
                element.classList.remove('jslide-bottom-out');
                if (typeof (done) == 'function') {
                    done();
                }
            }, 100);
        }
    }
    
    Component.fadeIn = function (element, done) {
        element.style.display = '';
        element.classList.add('jfade-in');
        setTimeout(function () {
            element.classList.remove('jfade-in');
            if (typeof (done) == 'function') {
                done();
            }
        }, 2000);
    }
    
    Component.fadeOut = function (element, done) {
        element.classList.add('jfade-out');
        setTimeout(function () {
            element.style.display = 'none';
            element.classList.remove('jfade-out');
            if (typeof (done) == 'function') {
                done();
            }
        }, 1000);
    }

    return Component;
}

/* harmony default export */ var animation = (Animation());
;// CONCATENATED MODULE: ./src/utils/helpers.date.js



function HelpersDate() {
    var Component = {};

    Component.now = function (date, dateOnly) {
        var y = null;
        var m = null;
        var d = null;
        var h = null;
        var i = null;
        var s = null;

        if (Array.isArray(date)) {
            y = date[0];
            m = date[1];
            d = date[2];
            h = date[3];
            i = date[4];
            s = date[5];
        } else {
            if (! date) {
                date = new Date();
            }
            y = date.getFullYear();
            m = date.getMonth() + 1;
            d = date.getDate();
            h = date.getHours();
            i = date.getMinutes();
            s = date.getSeconds();
        }

        if (dateOnly == true) {
            return helpers.two(y) + '-' + helpers.two(m) + '-' + helpers.two(d);
        } else {
            return helpers.two(y) + '-' + helpers.two(m) + '-' + helpers.two(d) + ' ' + helpers.two(h) + ':' + helpers.two(i) + ':' + helpers.two(s);
        }
    }

    Component.toArray = function (value) {
        var date = value.split(((value.indexOf('T') !== -1) ? 'T' : ' '));
        var time = date[1];
        var date = date[0].split('-');
        var y = parseInt(date[0]);
        var m = parseInt(date[1]);
        var d = parseInt(date[2]);
        var h = 0;
        var i = 0;

        if (time) {
            time = time.split(':');
            h = parseInt(time[0]);
            i = parseInt(time[1]);
        }
        return [y, m, d, h, i, 0];
    }

    var excelInitialTime = Date.UTC(1900, 0, 0);
    var excelLeapYearBug = Date.UTC(1900, 1, 29);
    var millisecondsPerDay = 86400000;

    /**
     * Date to number
     */
    Component.dateToNum = function (jsDate) {
        if (typeof (jsDate) === 'string') {
            jsDate = new Date(jsDate + '  GMT+0');
        }
        var jsDateInMilliseconds = jsDate.getTime();
        if (jsDateInMilliseconds >= excelLeapYearBug) {
            jsDateInMilliseconds += millisecondsPerDay;
        }
        jsDateInMilliseconds -= excelInitialTime;

        return jsDateInMilliseconds / millisecondsPerDay;
    }

    /**
     * Number to date
     *
     * IMPORTANT: Excel incorrectly considers 1900 to be a leap year
     */
    Component.numToDate = function (excelSerialNumber) {
        var jsDateInMilliseconds = excelInitialTime + excelSerialNumber * millisecondsPerDay;
        if (jsDateInMilliseconds >= excelLeapYearBug) {
            jsDateInMilliseconds -= millisecondsPerDay;
        }

        const d = new Date(jsDateInMilliseconds);

        var date = [
            d.getUTCFullYear(),
            d.getUTCMonth() + 1,
            d.getUTCDate(),
            d.getUTCHours(),
            d.getUTCMinutes(),
            d.getUTCSeconds(),
        ];

        return Component.now(date);
    }

    let weekdays = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
    let months = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];

    Object.defineProperty(Component, 'weekdays', {
        get: function () {
            return weekdays.map(function(v) {
                return dictionary.translate(v);
            });
        },
    });

    Object.defineProperty(Component, 'weekdaysShort', {
        get: function () {
            return weekdays.map(function(v) {
                return dictionary.translate(v).substring(0,3);
            });
        },
    });

    Object.defineProperty(Component, 'months', {
        get: function () {
            return months.map(function(v) {
                return dictionary.translate(v);
            });
        },
    });

    Object.defineProperty(Component, 'monthsShort', {
        get: function () {
            return months.map(function(v) {
                return dictionary.translate(v).substring(0,3);
            });
        },
    });

    return Component;
}

/* harmony default export */ var helpers_date = (HelpersDate());
;// CONCATENATED MODULE: ./src/plugins/mask.js



function Mask() {
    // Currency
    var tokens = {
        // Text
        text: [ '@' ],
        // Currency tokens
        currency: [ '#(.{1})##0?(.{1}0+)?( ?;(.*)?)?', '#' ],
        // Scientific
        scientific: [ '0{1}(.{1}0+)?E{1}\\+0+' ],
        // Percentage
        percentage: [ '0{1}(.{1}0+)?%' ],
        // Number
        numeric: [ '0{1}(.{1}0+)?' ],
        // Data tokens
        datetime: [ 'YYYY', 'YYY', 'YY', 'MMMMM', 'MMMM', 'MMM', 'MM', 'DDDDD', 'DDDD', 'DDD', 'DD', 'DY', 'DAY', 'WD', 'D', 'Q', 'MONTH', 'MON', 'HH24', 'HH12', 'HH', '\\[H\\]', 'H', 'AM/PM', 'MI', 'SS', 'MS', 'Y', 'M' ],
        // Other
        general: [ 'A', '0', '[0-9a-zA-Z\$]+', '.']
    }

    var getDate = function() {
        if (this.mask.toLowerCase().indexOf('[h]') !== -1) {
            var m = 0;
            if (this.date[4]) {
                m = parseFloat(this.date[4] / 60);
            }
            var v = parseInt(this.date[3]) + m;
            v /= 24;
        } else if (! (this.date[0] && this.date[1] && this.date[2]) && (this.date[3] || this.date[4])) {
            v = helpers.two(this.date[3]) + ':' + helpers.two(this.date[4]) + ':' + helpers.two(this.date[5])
        } else {
            if (this.date[0] && this.date[1] && ! this.date[2]) {
                this.date[2] = 1;
            }
            v = helpers.two(this.date[0]) + '-' + helpers.two(this.date[1]) + '-' + helpers.two(this.date[2]);

            if (this.date[3] || this.date[4] || this.date[5]) {
                v += ' ' + helpers.two(this.date[3]) + ':' + helpers.two(this.date[4]) + ':' + helpers.two(this.date[5]);
            }
        }

        return v;
    }

    var extractDate = function() {
        var v = '';
        if (! (this.date[0] && this.date[1] && this.date[2]) && (this.date[3] || this.date[4])) {
            if (this.mask.toLowerCase().indexOf('[h]') !== -1) {
                v = parseInt(this.date[3]);
            } else {
                let h = parseInt(this.date[3]);
                if (h < 13 && this.values.indexOf('PM') !== -1) {
                    v = (h+12) % 24;
                } else {
                    v = h % 24;
                }
            }
            if (this.date[4]) {
                v += parseFloat(this.date[4] / 60);
            }
            if (this.date[5]) {
                v += parseFloat(this.date[5] / 3600);
            }
            v /= 24;
        } else if (this.date[0] || this.date[1] || this.date[2] || this.date[3] || this.date[4] || this.date[5]) {
            if (this.date[0] && this.date[1] && ! this.date[2]) {
                this.date[2] = 1;
            }
            var t = helpers_date.now(this.date);
            v = helpers_date.dateToNum(t);
        }

        if (isNaN(v)) {
            v = '';
        }

        return v;
    }

    var isBlank = function(v) {
        return v === null || v === '' || v === undefined ? true : false;
    }

    var isFormula = function(value) {
        var v = (''+value)[0];
        return v == '=' ? true : false;
    }

    var isNumeric = function(t) {
        return t === 'currency' || t === 'percentage' || t === 'scientific' || t === 'numeric' ? true : false;
    }

    /**
     * Get the decimal defined in the mask configuration
     */
    var getDecimal = function(v) {
        if (v && Number(v) == v) {
            return '.';
        } else {
            if (this.options.decimal) {
                return this.options.decimal;
            } else {
                if (this.locale) {
                    var t = Intl.NumberFormat(this.locale).format(1.1);
                    return this.options.decimal = t[1];
                } else {
                    if (! v) {
                        v  = this.mask;
                    }
                    var e = new RegExp('0{1}(.{1})0+', 'ig');
                    var t = e.exec(v);
                    if (t && t[1] && t[1].length == 1) {
                        // Save decimal
                        this.options.decimal = t[1];
                        // Return decimal
                        return t[1];
                    } else {
                        // Did not find any decimal last resort the default
                        var e = new RegExp('#{1}(.{1})#+', 'ig');
                        var t = e.exec(v);
                        if (t && t[1] && t[1].length == 1) {
                            if (t[1] === ',') {
                                this.options.decimal = '.';
                            } else {
                                this.options.decimal = ',';
                            }
                        } else {
                            this.options.decimal = '1.1'.toLocaleString().substring(1,2);
                        }
                    }
                }
            }
        }

        if (this.options.decimal) {
            return this.options.decimal;
        } else {
            return null;
        }
    }

    var ParseValue = function(v, decimal) {
        if (v == '') {
            return '';
        }

        // Get decimal
        if (! decimal) {
            decimal = getDecimal.call(this);
        }

        // New value
        v = (''+v).split(decimal);

        // Signal
        var signal = v[0].match(/[-]+/g);
        if (signal && signal.length) {
            signal = true;
        } else {
            signal = false;
        }

        v[0] = v[0].match(/[0-9]+/g);

        if (v[0]) {
            if (signal) {
                v[0].unshift('-');
            }
            v[0] = v[0].join('');
        } else {
            if (signal) {
                v[0] = '-';
            }
        }

        if (v[0] || v[1]) {
            if (v[1] !== undefined) {
                v[1] = v[1].match(/[0-9]+/g);
                if (v[1]) {
                    v[1] = v[1].join('');
                } else {
                    v[1] = '';
                }
            }
        } else {
            return '';
        }
        return v;
    }

    var FormatValue = function(v, event) {
        if (v === '') {
            return '';
        }
        // Get decimal
        var d = getDecimal.call(this);
        // Convert value
        var o = this.options;
        // Parse value
        v = ParseValue.call(this, v);
        if (v === '') {
            return '';
        }
        var t = null;
        // Temporary value
        if (v[0]) {
            if (o.style === 'percent') {
                t = parseFloat(v[0]) / 100;
            } else {
                t = parseFloat(v[0] + '.1');
            }
        }

        if ((v[0] === '-' || v[0] === '-00') && ! v[1] && (event && event.inputType == "deleteContentBackward")) {
            return '';
        }

        var n = new Intl.NumberFormat(this.locale, o).format(t);
        n = n.split(d);

        if (o.style === 'percent') {
            if (n[0].indexOf('%') !== -1) {
                n[0] = n[0].replace('%', '');
                n[2] = '%';
            }
        }

        if (typeof(n[1]) !== 'undefined') {
            var s = n[1].replace(/[0-9]*/g, '');
            if (s) {
                n[2] = s;
            }
        }

        if (v[1] !== undefined) {
            n[1] = d + v[1];
        } else {
            n[1] = '';
        }

        return n.join('');
    }

    var Format = function(e, event) {
        var v = Value.call(e);
        if (! v) {
            return;
        }

        // Get decimal
        var n = FormatValue.call(this, v, event);
        var t = (n.length) - v.length;
        var index = Caret.call(e) + t;
        // Set value and update caret
        Value.call(e, n, index, true);
    }

    var Extract = function(v) {
        // Keep the raw value
        var current = ParseValue.call(this, v);
        if (current) {
            // Negative values
            if (current[0] === '-') {
                current[0] = '-0';
            }
            return parseFloat(current.join('.'));
        }
        return null;
    }

    /**
     * Caret getter and setter methods
     */
    var Caret = function(index, adjustNumeric) {
        if (index === undefined) {
            if (this.tagName == 'DIV') {
                var pos = 0;
                var s = window.getSelection();
                if (s) {
                    if (s.rangeCount !== 0) {
                        var r = s.getRangeAt(0);
                        var p = r.cloneRange();
                        p.selectNodeContents(this);
                        p.setEnd(r.endContainer, r.endOffset);
                        pos = p.toString().length;
                    }
                }
                return pos;
            } else {
                return this.selectionStart;
            }
        } else {
            // Get the current value
            var n = Value.call(this);

            // Review the position
            if (adjustNumeric) {
                var p = null;
                for (var i = 0; i < n.length; i++) {
                    if (n[i].match(/[\-0-9]/g) || n[i] === '.' || n[i] === ',') {
                        p = i;
                    }
                }

                // If the string has no numbers
                if (p === null) {
                    p = n.indexOf(' ');
                }

                if (index >= p) {
                    index = p + 1;
                }
            }

            // Do not update caret
            if (index > n.length) {
                index = n.length;
            }

            if (index) {
                // Set caret
                if (this.tagName == 'DIV') {
                    var s = window.getSelection();
                    var r = document.createRange();

                    if (this.childNodes[0]) {
                        r.setStart(this.childNodes[0], index);
                        s.removeAllRanges();
                        s.addRange(r);
                    }
                } else {
                    this.selectionStart = index;
                    this.selectionEnd = index;
                }
            }
        }
    }

    /**
     * Value getter and setter method
     */
    var Value = function(v, updateCaret, adjustNumeric) {
        if (this.tagName == 'DIV') {
            if (v === undefined) {
                var v = this.innerText;
                if (this.value && this.value.length > v.length) {
                    v = this.value;
                }
                return v;
            } else {
                if (this.innerText !== v) {
                    this.innerText = v;

                    if (updateCaret) {
                        Caret.call(this, updateCaret, adjustNumeric);
                    }
                }
            }
        } else {
            if (v === undefined) {
                return this.value;
            } else {
                if (this.value !== v) {
                    this.value = v;
                    if (updateCaret) {
                        Caret.call(this, updateCaret, adjustNumeric);
                    }
                }
            }
        }
    }

    // Labels
    var weekDaysFull = helpers_date.weekdays;
    var weekDays = helpers_date.weekdaysShort;
    var monthsFull = helpers_date.months;
    var months = helpers_date.monthsShort;

    var parser = {
        'YEAR': function(v, s) {
            var y = ''+new Date().getFullYear();

            if (typeof(this.values[this.index]) === 'undefined') {
                this.values[this.index] = '';
            }
            if (parseInt(v) >= 0 && parseInt(v) <= 10) {
                if (this.values[this.index].length < s) {
                    this.values[this.index] += v;
                }
            }
            if (this.values[this.index].length == s) {
                if (s == 2) {
                    var y = y.substr(0,2) + this.values[this.index];
                } else if (s == 3) {
                    var y = y.substr(0,1) + this.values[this.index];
                } else if (s == 4) {
                    var y = this.values[this.index];
                }
                this.date[0] = y;
                this.index++;
            }
        },
        'YYYY': function(v) {
            parser.YEAR.call(this, v, 4);
        },
        'YYY': function(v) {
            parser.YEAR.call(this, v, 3);
        },
        'YY': function(v) {
            parser.YEAR.call(this, v, 2);
        },
        'FIND': function(v, a) {
            if (isBlank(this.values[this.index])) {
                this.values[this.index] = '';
            }
            if (this.event && this.event.inputType && this.event.inputType.indexOf('delete') > -1) {
                this.values[this.index] += v;
                return;
            }
            var pos = 0;
            var count = 0;
            var value = (this.values[this.index] + v).toLowerCase();
            for (var i = 0; i < a.length; i++) {
                if (a[i].toLowerCase().indexOf(value) == 0) {
                    pos = i;
                    count++;
                }
            }
            if (count > 1) {
                this.values[this.index] += v;
            } else if (count == 1) {
                // Jump number of chars
                var t = (a[pos].length - this.values[this.index].length) - 1;
                this.position += t;

                this.values[this.index] = a[pos];
                this.index++;
                return pos;
            }
        },
        'MMM': function(v) {
            var ret = parser.FIND.call(this, v, months);
            if (ret !== undefined) {
                this.date[1] = ret + 1;
            }
        },
        'MON': function(v) {
            parser['MMM'].call(this, v);
        },
        'MMMM': function(v) {
            var ret = parser.FIND.call(this, v, monthsFull);
            if (ret !== undefined) {
                this.date[1] = ret + 1;
            }
        },
        'MONTH': function(v) {
            parser['MMMM'].call(this, v);
        },
        'MMMMM': function(v) {
            if (isBlank(this.values[this.index])) {
                this.values[this.index] = '';
            }
            var pos = 0;
            var count = 0;
            var value = (this.values[this.index] + v).toLowerCase();
            for (var i = 0; i < monthsFull.length; i++) {
                if (monthsFull[i][0].toLowerCase().indexOf(value) == 0) {
                    this.values[this.index] = monthsFull[i][0];
                    this.date[1] = i + 1;
                    this.index++;
                    break;
                }
            }
        },
        'MM': function(v) {
            if (isBlank(this.values[this.index])) {
                if (parseInt(v) > 1 && parseInt(v) < 10) {
                    this.date[1] = this.values[this.index] = '0' + v;
                    this.index++;
                } else if (parseInt(v) < 2) {
                    this.values[this.index] = v;
                }
            } else {
                if (this.values[this.index] == 1 && parseInt(v) < 3) {
                    this.date[1] = this.values[this.index] += v;
                    this.index++;
                } else if (this.values[this.index] == 0 && parseInt(v) > 0 && parseInt(v) < 10) {
                    this.date[1] = this.values[this.index] += v;
                    this.index++;
                }
            }
        },
        'M': function(v) {
            var test = false;
            if (parseInt(v) >= 0 && parseInt(v) < 10) {
                if (isBlank(this.values[this.index])) {
                    this.values[this.index] = v;
                    if (v > 1) {
                        this.date[1] = this.values[this.index];
                        this.index++;
                    }
                } else {
                    if (this.values[this.index] == 1 && parseInt(v) < 3) {
                        this.date[1] = this.values[this.index] += v;
                        this.index++;
                    } else if (this.values[this.index] == 0 && parseInt(v) > 0) {
                        this.date[1] = this.values[this.index] += v;
                        this.index++;
                    } else {
                        var test = true;
                    }
                }
            } else {
                var test = true;
            }

            // Re-test
            if (test == true) {
                var t = parseInt(this.values[this.index]);
                if (t > 0 && t < 12) {
                    this.date[1] = this.values[this.index];
                    this.index++;
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'D': function(v) {
            var test = false;
            if (parseInt(v) >= 0 && parseInt(v) < 10) {
                if (isBlank(this.values[this.index])) {
                    this.values[this.index] = v;
                    if (parseInt(v) > 3) {
                        this.date[2] = this.values[this.index];
                        this.index++;
                    }
                } else {
                    if (this.values[this.index] == 3 && parseInt(v) < 2) {
                        this.date[2] = this.values[this.index] += v;
                        this.index++;
                    } else if (this.values[this.index] == 1 || this.values[this.index] == 2) {
                        this.date[2] = this.values[this.index] += v;
                        this.index++;
                    } else if (this.values[this.index] == 0 && parseInt(v) > 0) {
                        this.date[2] = this.values[this.index] += v;
                        this.index++;
                    } else {
                        var test = true;
                    }
                }
            } else {
                var test = true;
            }

            // Re-test
            if (test == true) {
                var t = parseInt(this.values[this.index]);
                if (t > 0 && t < 32) {
                    this.date[2] = this.values[this.index];
                    this.index++;
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'DD': function(v) {
            if (isBlank(this.values[this.index])) {
                if (parseInt(v) > 3 && parseInt(v) < 10) {
                    this.date[2] = this.values[this.index] = '0' + v;
                    this.index++;
                } else if (parseInt(v) < 10) {
                    this.values[this.index] = v;
                }
            } else {
                if (this.values[this.index] == 3 && parseInt(v) < 2) {
                    this.date[2] = this.values[this.index] += v;
                    this.index++;
                } else if ((this.values[this.index] == 1 || this.values[this.index] == 2) && parseInt(v) < 10) {
                    this.date[2] = this.values[this.index] += v;
                    this.index++;
                } else if (this.values[this.index] == 0 && parseInt(v) > 0 && parseInt(v) < 10) {
                    this.date[2] = this.values[this.index] += v;
                    this.index++;
                }
            }
        },
        'DDD': function(v) {
            parser.FIND.call(this, v, weekDays);
        },
        'DY': function(v) {
            parser['DDD'].call(this, v);
        },
        'DDDD': function(v) {
            parser.FIND.call(this, v, weekDaysFull);
        },
        'DAY': function(v) {
            parser['DDDD'].call(this, v);
        },
        'HH12': function(v, two) {
            var test = false;
            if (parseInt(v) >= 0 && parseInt(v) < 10) {
                if (isBlank(this.values[this.index])) {
                    if (parseInt(v) > 1 && parseInt(v) < 10) {
                        if (two) {
                            v = 0 + v;
                        }
                        this.date[3] = this.values[this.index] = v;
                        this.index++;
                    } else if (parseInt(v) < 10) {
                        this.values[this.index] = v;
                    }
                } else {
                    if (this.values[this.index] == 1 && parseInt(v) < 3) {
                        this.date[3] = this.values[this.index] += v;
                        this.index++;
                    } else if (this.values[this.index] < 1 && parseInt(v) < 10) {
                        this.date[3] = this.values[this.index] += v;
                        this.index++;
                    } else {
                        var test = true;
                    }
                }
            } else {
                var test = true;
            }

            // Re-test
            if (test == true) {
                var t = parseInt(this.values[this.index]);
                if (t >= 0 && t <= 12) {
                    this.date[3] = this.values[this.index];
                    this.index++;
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'HH24': function(v, two) {
            var test = false;
            if (parseInt(v) >= 0 && parseInt(v) < 10) {
                if (this.values[this.index] == null || this.values[this.index] == '') {
                    if (parseInt(v) > 2 && parseInt(v) < 10) {
                        if (two) {
                            v = 0 + v;
                        }
                        this.date[3] = this.values[this.index] = v;
                        this.index++;
                    } else if (parseInt(v) < 10) {
                        this.values[this.index] = v;
                    }
                } else {
                    if (this.values[this.index] == 2 && parseInt(v) < 4) {
                        if (! two && this.values[this.index] === '0') {
                            this.values[this.index] = '';
                        }
                        this.date[3] = this.values[this.index] += v;
                        this.index++;
                    } else if (this.values[this.index] < 2 && parseInt(v) < 10) {
                        if (! two && this.values[this.index] === '0') {
                            this.values[this.index] = '';
                        }
                        this.date[3] = this.values[this.index] += v;
                        this.index++;
                    } else {
                        var test = true;
                    }
                }
            } else {
                var test = true;
            }

            // Re-test
            if (test == true) {
                var t = parseInt(this.values[this.index]);
                if (t >= 0 && t < 24) {
                    this.date[3] = this.values[this.index];
                    this.index++;
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'HH': function(v) {
            parser['HH24'].call(this, v, 1);
        },
        'H': function(v) {
            parser['HH24'].call(this, v, 0);
        },
        '\\[H\\]': function(v) {
            if (this.values[this.index] == undefined) {
                this.values[this.index] = '';
            }
            if (v.match(/[0-9]/g)) {
                this.date[3] = this.values[this.index] += v;
            } else {
                if (this.values[this.index].match(/[0-9]/g)) {
                    this.date[3] = this.values[this.index];
                    this.index++;
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'N60': function(v, i) {
            if (this.values[this.index] == null || this.values[this.index] == '') {
                if (parseInt(v) > 5 && parseInt(v) < 10) {
                    this.date[i] = this.values[this.index] = '0' + v;
                    this.index++;
                } else if (parseInt(v) < 10) {
                    this.values[this.index] = v;
                }
            } else {
                if (parseInt(v) < 10) {
                    this.date[i] = this.values[this.index] += v;
                    this.index++;
                 }
            }
        },
        'MI': function(v) {
            parser.N60.call(this, v, 4);
        },
        'SS': function(v) {
            parser.N60.call(this, v, 5);
        },
        'AM/PM': function(v) {
            if (typeof(this.values[this.index]) === 'undefined') {
                this.values[this.index] = '';
            }

            if (this.values[this.index] === '') {
                if (v.match(/a/i) && this.date[3] < 13) {
                    this.values[this.index] += 'A';
                } else if (v.match(/p/i)) {
                    this.values[this.index] += 'P';
                }
            } else if (this.values[this.index] === 'A' || this.values[this.index] === 'P') {
                this.values[this.index] += 'M';
                this.index++;
            }
        },
        'WD': function(v) {
            if (typeof(this.values[this.index]) === 'undefined') {
                this.values[this.index] = '';
            }
            if (parseInt(v) >= 0 && parseInt(v) < 7) {
                this.values[this.index] = v;
            }
            if (this.values[this.index].length == 1) {
                this.index++;
            }
        },
        '0{1}(.{1}0+)?': function(v) {
            // Get decimal
            var decimal = getDecimal.call(this);
            // Negative number
            var neg = false;
            // Create if is blank
            if (isBlank(this.values[this.index])) {
                this.values[this.index] = '';
            } else {
                if (this.values[this.index] == '-') {
                    neg = true;
                }
            }
            var current = ParseValue.call(this, this.values[this.index], decimal);
            if (current) {
                this.values[this.index] = current.join(decimal);
            }
            // New entry
            if (parseInt(v) >= 0 && parseInt(v) < 10) {
                // Replace the zero for a number
                if (this.values[this.index] == '0' && v > 0) {
                    this.values[this.index] = '';
                } else if (this.values[this.index] == '-0' && v > 0) {
                    this.values[this.index] = '-';
                }
                // Don't add up zeros because does not mean anything here
                if ((this.values[this.index] != '0' && this.values[this.index] != '-0') || v == decimal) {
                    this.values[this.index] += v;
                }
            } else if (decimal && v == decimal) {
                if (this.values[this.index].indexOf(decimal) == -1) {
                    if (! this.values[this.index]) {
                        this.values[this.index] = '0';
                    }
                    this.values[this.index] += v;
                }
            } else if (v == '-') {
                // Negative signed
                neg = true;
            }

            if (neg === true && this.values[this.index][0] !== '-') {
                this.values[this.index] = '-' + this.values[this.index];
            }
        },
        '0{1}(.{1}0+)?E{1}\\+0+': function(v) {
            parser['0{1}(.{1}0+)?'].call(this, v);
        },
        '0{1}(.{1}0+)?%': function(v) {
            parser['0{1}(.{1}0+)?'].call(this, v);

            if (this.values[this.index].match(/[\-0-9]/g)) {
                if (this.values[this.index] && this.values[this.index].indexOf('%') == -1) {
                    this.values[this.index] += '%';
                }
            } else {
                this.values[this.index] = '';
            }
        },
        '#(.{1})##0?(.{1}0+)?( ?;(.*)?)?': function(v) {
            // Parse number
            parser['0{1}(.{1}0+)?'].call(this, v);
            // Get decimal
            var decimal = getDecimal.call(this);
            // Get separator
            var separator = this.tokens[this.index].substr(1,1);
            // Negative
            var negative = this.values[this.index][0] === '-' ? true : false;
            // Current value
            var current = ParseValue.call(this, this.values[this.index], decimal);

            // Get main and decimal parts
            if (current !== '') {
                // Format number
                var n = current[0].match(/[0-9]/g);
                if (n) {
                    // Format
                    n = n.join('');
                    var t = [];
                    var s = 0;
                    for (var j = n.length - 1; j >= 0 ; j--) {
                        t.push(n[j]);
                        s++;
                        if (! (s % 3)) {
                            t.push(separator);
                        }
                    }
                    t = t.reverse();
                    current[0] = t.join('');
                    if (current[0].substr(0,1) == separator) {
                        current[0] = current[0].substr(1);
                    }
                } else {
                    current[0] = '';
                }

                // Value
                this.values[this.index] = current.join(decimal);

                // Negative
                if (negative) {
                    this.values[this.index] = '-' + this.values[this.index];
                }
            }
        },
        '0': function(v) {
            if (v.match(/[0-9]/g)) {
                this.values[this.index] = v;
                this.index++;
            }
        },
        '[0-9a-zA-Z$]+': function(v) {
            if (isBlank(this.values[this.index])) {
                this.values[this.index] = '';
            }
            var t = this.tokens[this.index];
            var s = this.values[this.index];
            var i = s.length;

            if (t[i] == v) {
                this.values[this.index] += v;

                if (this.values[this.index] == t) {
                    this.index++;
                }
            } else {
                this.values[this.index] = t;
                this.index++;

                if (v.match(/[\-0-9]/g)) {
                    // Repeat the character
                    this.position--;
                }
            }
        },
        'A': function(v) {
            if (v.match(/[a-zA-Z]/gi)) {
                this.values[this.index] = v;
                this.index++;
            }
        },
        '.': function(v) {
            parser['[0-9a-zA-Z$]+'].call(this, v);
        },
        '@': function(v) {
            if (isBlank(this.values[this.index])) {
                this.values[this.index] = '';
            }
            this.values[this.index] += v;
        }
    }

    /**
     * Get the tokens in the mask string
     */
    var getTokens = function(str) {
        if (this.type == 'general') {
            var t = [].concat(tokens.general);
        } else {
            var t = [].concat(tokens.currency, tokens.datetime, tokens.percentage, tokens.scientific, tokens.numeric, tokens.text, tokens.general);
        }
        // Expression to extract all tokens from the string
        var e = new RegExp(t.join('|'), 'gi');
        // Extract
        return str.match(e);
    }

    /**
     * Get the method of one given token
     */
    var getMethod = function(str) {
        if (! this.type) {
            var types = Object.keys(tokens);
        } else if (this.type == 'text') {
            var types = [ 'text' ];
        } else if (this.type == 'general') {
            var types = [ 'general' ];
        } else if (this.type == 'datetime') {
            var types = [ 'numeric', 'datetime', 'general' ];
        } else {
            var types = [ 'currency', 'percentage', 'scientific', 'numeric', 'general' ];
        }

        // Found
        for (var i = 0; i < types.length; i++) {
            var type = types[i];
            for (var j = 0; j < tokens[type].length; j++) {
                var e = new RegExp(tokens[type][j], 'gi');
                var r = str.match(e);
                if (r) {
                    return { type: type, method: tokens[type][j] }
                }
            }
        }
    }

    /**
     * Identify each method for each token
     */
    var getMethods = function(t) {
        var result = [];
        for (var i = 0; i < t.length; i++) {
            var m = getMethod.call(this, t[i]);
            if (m) {
                result.push(m.method);
            } else {
                result.push(null);
            }
        }

        // Compatibility with excel
        for (var i = 0; i < result.length; i++) {
            if (result[i] == 'MM') {
                // Not a month, correct to minutes
                if (result[i-1] && result[i-1].indexOf('H') >= 0) {
                    result[i] = 'MI';
                } else if (result[i-2] && result[i-2].indexOf('H') >= 0) {
                    result[i] = 'MI';
                } else if (result[i+1] && result[i+1].indexOf('S') >= 0) {
                    result[i] = 'MI';
                } else if (result[i+2] && result[i+2].indexOf('S') >= 0) {
                    result[i] = 'MI';
                }
            }
        }

        return result;
    }

    /**
     * Get the type for one given token
     */
    var getType = function(str) {
        var m = getMethod.call(this, str);
        if (m) {
            var type = m.type;
        }

        if (type) {
            var numeric = 0;
            // Make sure the correct type
            var t = getTokens.call(this, str);
            for (var i = 0; i < t.length; i++) {
                m = getMethod.call(this, t[i]);
                if (m && isNumeric(m.type)) {
                    numeric++;
                }
            }
            if (numeric > 1) {
                type = 'general';
            }
        }

        return type;
    }

    /**
     * Parse character per character using the detected tokens in the mask
     */
    var parse = function() {
        // Parser method for this position
        if (typeof(parser[this.methods[this.index]]) == 'function') {
            parser[this.methods[this.index]].call(this, this.value[this.position]);
            this.position++;
        } else {
            this.values[this.index] = this.tokens[this.index];
            this.index++;
        }
    }

    var toPlainString = function(num) {
        return (''+ +num).replace(/(-?)(\d*)\.?(\d*)e([+-]\d+)/,
          function(a,b,c,d,e) {
            return e < 0
              ? b + '0.' + Array(1-e-c.length).join(0) + c + d
              : b + c + d + Array(e-d.length+1).join(0);
          });
    }

    /**
     * Mask function
     * @param {mixed|string} JS input or a string to be parsed
     * @param {object|string} When the first param is a string, the second is the mask or object with the mask options
     */
    var obj = function(e, config, returnObject) {
        // Options
        var r = null;
        var t = null;
        var o = {
            // Element
            input: null,
            // Current value
            value: null,
            // Mask options
            options: {},
            // New values for each token found
            values: [],
            // Token position
            index: 0,
            // Character position
            position: 0,
            // Date raw values
            date: [0,0,0,0,0,0],
            // Raw number for the numeric values
            number: 0,
        }

        // This is a JavaScript Event
        if (typeof(e) == 'object') {
            // Element
            o.input = e.target;
            // Current value
            o.value = Value.call(e.target);
            // Current caret position
            o.caret = Caret.call(e.target);
            // Mask
            if (t = e.target.getAttribute('data-mask')) {
                o.mask = t;
            }
            // Type
            if (t = e.target.getAttribute('data-type')) {
                o.type = t;
            }
            // Options
            if (e.target.mask) {
                if (e.target.mask.options) {
                    o.options = e.target.mask.options;
                }
                if (e.target.mask.locale) {
                    o.locale = e.target.mask.locale;
                }
            } else {
                // Locale
                if (t = e.target.getAttribute('data-locale')) {
                    o.locale = t;
                    if (o.mask) {
                        o.options.style = o.mask;
                    }
                }
            }
            // Extra configuration
            if (e.target.attributes && e.target.attributes.length) {
                for (var i = 0; i < e.target.attributes.length; i++) {
                    var k = e.target.attributes[i].name;
                    var v = e.target.attributes[i].value;
                    if (k.substr(0,4) == 'data') {
                        o.options[k.substr(5)] = v;
                    }
                }
            }
        } else {
            // Options
            if (typeof(config) == 'string') {
                // Mask
                o.mask = config;
            } else {
                // Mask
                var k = Object.keys(config);
                for (var i = 0; i < k.length; i++) {
                    o[k[i]] = config[k[i]];
                }
            }

            if (typeof(e) === 'number') {
                // Get decimal
                getDecimal.call(o, o.mask);
                // Replace to the correct decimal
                e = (''+e).replace('.', o.options.decimal);
            }

            // Current
            o.value = e;

            if (o.input) {
                // Value
                Value.call(o.input, e);
                // Focus
                helpers.focus(o.input);
                // Caret
                o.caret = Caret.call(o.input);
            }
        }

        // Mask detected start the process
        if (! isFormula(o.value) && (o.mask || o.locale)) {
            // Compatibility fixes
            if (o.mask) {
                // Remove []
                o.mask = o.mask.replace(new RegExp(/\[h]/),'|h|');
                o.mask = o.mask.replace(new RegExp(/\[.*?\]/),'');
                o.mask = o.mask.replace(new RegExp(/\|h\|/),'[h]');
                if (o.mask.indexOf(';') !== -1) {
                    var t = o.mask.split(';');
                    o.mask = t[0];
                }
                // Excel mask TODO: Improve
                if (o.mask.indexOf('##') !== -1) {
                    var d = o.mask.split(';');
                    if (d[0]) {
                        if (typeof(e) == 'object') {
                            d[0] = d[0].replace(new RegExp(/_\)/g), '');
                            d[0] = d[0].replace(new RegExp(/_\(/g), '');
                        }
                        d[0] = d[0].replace('*', '\t');
                        d[0] = d[0].replace(new RegExp(/_-/g), '');
                        d[0] = d[0].replace(new RegExp(/_/g), '');
                        d[0] = d[0].replace(new RegExp(/"/g), '');
                        d[0] = d[0].replace('##0.###','##0.000');
                        d[0] = d[0].replace('##0.##','##0.00');
                        d[0] = d[0].replace('##0.#','##0.0');
                        d[0] = d[0].replace('##0,###','##0,000');
                        d[0] = d[0].replace('##0,##','##0,00');
                        d[0] = d[0].replace('##0,#','##0,0');
                    }
                    o.mask = d[0];
                }
                // Remove back slashes
                if (o.mask.indexOf('\\') !== -1) {
                    var d = o.mask.split(';');
                    d[0] = d[0].replace(new RegExp(/\\/g), '');
                    o.mask = d[0];
                }
                // Get type
                if (! o.type) {
                    o.type = getType.call(o, o.mask);
                }
                // Get tokens
                o.tokens = getTokens.call(o, o.mask);
            }

            // On new input
            if (typeof(e) !== 'object'  || ! e.inputType || ! e.inputType.indexOf('insert') || ! e.inputType.indexOf('delete')) {
                // Start transformation
                if (o.locale) {
                    if (o.input) {
                        Format.call(o, o.input, e);
                    } else {
                        var newValue = FormatValue.call(o, o.value);
                    }
                } else {
                    // Get tokens
                    o.methods = getMethods.call(o, o.tokens);
                    o.event = e;

                    // Go through all tokes
                    while (o.position < o.value.length && typeof(o.tokens[o.index]) !== 'undefined') {
                        // Get the appropriate parser
                        parse.call(o);
                    }

                    // New value
                    var newValue = o.values.join('');

                    // Add tokens to the end of string only if string is not empty
                    if (isNumeric(o.type) && newValue !== '') {
                        // Complement things in the end of the mask
                        while (typeof(o.tokens[o.index]) !== 'undefined') {
                            var t = getMethod.call(o, o.tokens[o.index]);
                            if (t && t.type == 'general') {
                                o.values[o.index] = o.tokens[o.index];
                            }
                            o.index++;
                        }

                        var adjustNumeric = true;
                    } else {
                        var adjustNumeric = false;
                    }

                    // New value
                    newValue = o.values.join('');

                    // Reset value
                    if (o.input) {
                        t = newValue.length - o.value.length;
                        if (t > 0) {
                            var caret = o.caret + t;
                        } else {
                            var caret = o.caret;
                        }
                        Value.call(o.input, newValue, caret, adjustNumeric);
                    }
                }
            }

            // Update raw data
            if (o.input) {
                var label = null;
                if (isNumeric(o.type)) {
                    let v = Value.call(o.input);
                    // Extract the number
                    o.number = Extract.call(o, v);
                    // Keep the raw data as a property of the tag
                    if (o.type == 'percentage' && (''+v).indexOf('%') !== -1) {
                        label = obj.adjustPrecision(o.number / 100);
                    } else {
                        label = o.number;
                    }
                } else if (o.type == 'datetime') {
                    label = getDate.call(o);

                    if (o.date[0] && o.date[1] && o.date[2]) {
                        o.input.setAttribute('data-completed', true);
                    }
                }

                if (label) {
                    o.input.setAttribute('data-value', label);
                }
            }

            if (newValue !== undefined) {
                if (returnObject) {
                    return o;
                } else {
                    return newValue;
                }
            }
        }
    }

    obj.adjustPrecision = function(num) {
        if (typeof num === 'number' && !Number.isInteger(num)) {
            const v = num.toString().split('.');

            if (v[1] && v[1].length > 10) {
                let t0 = 0;
                const t1 = v[1][v[1].length - 2];

                if (t1 == 0 || t1 == 9) {
                    for (let i = v[1].length - 2; i > 0; i--) {
                        if (t0 >= 0 && v[1][i] == t1) {
                            t0++;
                            if (t0 > 6) {
                                break;
                            }
                        } else {
                            t0 = 0;
                            break;
                        }
                    }

                    if (t0) {
                        return parseFloat(parseFloat(num).toFixed(v[1].length - 1));
                    }
                }
            }
        }

        return num;
    }

    // Get the type of the mask
    obj.getType = getType;

    // Extract the tokens from a mask
    obj.prepare = function(str, o) {
        if (! o) {
            o = {};
        }
        return getTokens.call(o, str);
    }

    /**
     * Apply the mask to a element (legacy)
     */
    obj.apply = function(e) {
        var v = Value.call(e.target);
        if (e.key.length == 1) {
            v += e.key;
        }
        Value.call(e.target, obj(v, e.target.getAttribute('data-mask')));
    }

    /**
     * Legacy support
     */
    obj.run = function(value, mask, decimal) {
        return obj(value, { mask: mask, decimal: decimal });
    }

    /**
     * Extract number from masked string
     */
    obj.extract = function(v, options, returnObject) {
        if (isBlank(v)) {
            return v;
        }
        if (typeof(options) != 'object') {
            return v;
        } else {
            options = Object.assign({}, options);

            if (! options.options) {
                options.options = {};
            }
        }

        // Compatibility
        if (! options.mask && options.format) {
            options.mask = options.format;
        }

        // Remove []
        if (options.mask) {
            if (options.mask.indexOf(';') !== -1) {
                var t = options.mask.split(';');
                options.mask = t[0];
            }
            options.mask = options.mask.replace(new RegExp(/\[h]/),'|h|');
            options.mask = options.mask.replace(new RegExp(/\[.*?\]/),'');
            options.mask = options.mask.replace(new RegExp(/\|h\|/),'[h]');
        }

        // Get decimal
        getDecimal.call(options, options.mask);

        var type = null;
        var value = null;

        if (options.type == 'percent' || options.options.style == 'percent') {
            type = 'percentage';
        } else if (options.mask) {
            type = getType.call(options, options.mask);
        }

        if (type === 'text') {
            var o = {};
            value = v;
        } else if (type === 'general') {
            var o = obj(v, options, true);

            value = v;
        } else if (type === 'datetime') {
            if (v instanceof Date) {
                v = obj.getDateString(v, options.mask);
            }

            var o = obj(v, options, true);

            if (helpers.isNumeric(v)) {
                value = v;
            } else {
                value = extractDate.call(o);
            }
        } else if (type === 'scientific') {
            value = v;
            if (typeof(v) === 'string') {
                value = Number(value);
            }
            var o = options;
        } else {
            value = Extract.call(options, v);
            // Percentage
            if (type === 'percentage' && (''+v).indexOf('%') !== -1) {
                value /= 100;
            }
            var o = options;
        }

        o.value = value;

        if (! o.type && type) {
            o.type = type;
        }

        if (returnObject) {
            return o;
        } else {
            return value;
        }
    }

    /**
     * Render
     */
    obj.render = function(value, options, fullMask, strict) {
        if (isBlank(value)) {
            return value;
        }

        if (typeof(options) != 'object') {
            return value;
        } else {
            options = Object.assign({}, options);

            if (! options.options) {
                options.options = {};
            }
        }

        // Compatibility
        if (! options.mask && options.format) {
            options.mask = options.format;
        }

        // Remove []
        if (options.mask) {
            if (options.mask.indexOf(';') !== -1) {
                var t = options.mask.split(';');
                if (! fullMask) {
                    t[0] = t[0].replace(new RegExp(/_\)/g), '');
                    t[0] = t[0].replace(new RegExp(/_\(/g), '');
                }
                options.mask = t[0];
            }
            options.mask = options.mask.replace(new RegExp(/\[h]/),'|h|');
            options.mask = options.mask.replace(new RegExp(/\[.*?\]/),'');
            options.mask = options.mask.replace(new RegExp(/\|h\|/),'[h]');
        }

        var type = null;
        if (options.type == 'percent' || options.options.style == 'percent') {
            type = 'percentage';
        } else if (options.mask) {
            type = getType.call(options, options.mask);
        } else if (value instanceof Date) {
            type = 'datetime';
        }

        // Fill with blanks
        var fillWithBlanks = false;

        if (type =='datetime' || options.type == 'calendar') {
            var t = obj.getDateString(value, options.mask);
            if (t) {
                value = t;
            }
            if (options.mask && fullMask) {
                fillWithBlanks = true;
            }
        } else if (type === 'text') {
            // Parse number
            if (typeof(value) === 'number') {
                value = value.toString();
            }
        } else {
            // Parse number
            if (typeof(value) === 'string' && jSuites.isNumeric(value)) {
                value = Number(value);
            }
            // Percentage
            if (type === 'percentage') {
                value = obj.adjustPrecision(value*100);
            }

            // Number of decimal places
            if (typeof(value) === 'number') {
                var t = null;
                if (options.mask && fullMask) {
                    var d = getDecimal.call(options, options.mask);
                    if (type === 'scientific') {
                        if (options.mask.indexOf(d) !== -1) {
                            let exp = options.mask.split('E');
                            exp = exp[0].split(d);
                            exp = ('' + exp[1].match(/[0-9]+/g))
                            exp = exp.length;
                            t = value.toExponential(exp);
                        } else {
                            t = value.toExponential(0);
                        }
                    } else {
                        if (options.mask.indexOf(d) !== -1) {
                            d = options.mask.split(d);
                            d = (''+d[1].match(/[0-9]+/g))
                            d = d.length;
                            t = value.toFixed(d);
                            let n = value.toString().split('.');
                            let fraction = n[1];
                            if (fraction && fraction.length > d && fraction[fraction.length-1] === '5') {
                                t = parseFloat(n[0] + '.' + fraction + '1').toFixed(d);
                            }
                        } else {
                            t = value.toFixed(0);
                        }

                        // Handle scientific notation
                        if ((''+t).indexOf('e') !== -1) {
                            t = toPlainString(t);
                        }
                    }
                } else if (options.locale && fullMask) {
                    // Append zeros
                    var d = (''+value).split('.');
                    if (options.options) {
                        if (typeof(d[1]) === 'undefined') {
                            d[1] = '';
                        }
                        var len = d[1].length;
                        if (options.options.minimumFractionDigits > len) {
                            for (var i = 0; i < options.options.minimumFractionDigits - len; i++) {
                                d[1] += '0';
                            }
                        }
                    }
                    if (! d[1].length) {
                        t = d[0]
                    } else {
                        t = d.join('.');
                    }
                    var len = d[1].length;
                    if (options.options && options.options.maximumFractionDigits < len) {
                        t = parseFloat(t).toFixed(options.options.maximumFractionDigits);
                    }
                } else {
                    t = toPlainString(value);
                }

                if (t !== null) {
                    value = t;
                    // Get decimal
                    getDecimal.call(options, options.mask);
                    // Replace to the correct decimal
                    if (options.options.decimal) {
                        value = value.replace('.', options.options.decimal);
                    }
                }
            } else {
                if (options.mask && fullMask) {
                    fillWithBlanks = true;
                }
            }
        }

        if (fillWithBlanks) {
            var s = options.mask.length - value.length;
            if (s > 0) {
                for (var i = 0; i < s; i++) {
                    value += ' ';
                }
            }
        }

        if (type === 'scientific') {
            if (! fullMask) {
                value = toPlainString(value);
            } else {
                return value;
            }
        }

        if (type === 'numeric' && strict === false && typeof(value) === 'string') {
            return value;
        }

        value = obj(value, options);

        // Numeric mask, number of zeros
        if (fullMask && type === 'numeric') {
            var maskZeros = options.mask.match(new RegExp(/^[0]+$/gm));
            if (maskZeros && maskZeros.length === 1) {
                var maskLength = maskZeros[0].length;
                if (maskLength > 3) {
                    value = '' + value;
                    while (value.length < maskLength) {
                        value = '0' + value;
                    }
                }
            }
        }

        return value;
    }

    obj.set = function(e, m) {
        if (m) {
            e.setAttribute('data-mask', m);
            // Reset the value
            var event = new Event('input', {
                bubbles: true,
                cancelable: true,
            });
            e.dispatchEvent(event);
        }
    }

    // Helper to extract date from a string
    obj.extractDateFromString = function (date, format) {
        var o = obj(date, { mask: format }, true);

        // Check if in format Excel (Need difference with format date or type detected is numeric)
        if (date > 0 && Number(date) == date && (o.values.join("") !== o.value || o.type == "numeric")) {
            var d = new Date(Math.round((date - 25569) * 86400 * 1000));
            return d.getFullYear() + "-" + helpers.two(d.getMonth()) + "-" + helpers.two(d.getDate()) + ' 00:00:00';
        }

        var complete = false;

        if (o.values && o.values.length === o.tokens.length && o.values[o.values.length - 1].length >= o.tokens[o.tokens.length - 1].length) {
            complete = true;
        }

        if (o.date[0] && o.date[1] && (o.date[2] || complete)) {
            if (!o.date[2]) {
                o.date[2] = 1;
            }

            return o.date[0] + '-' + helpers.two(o.date[1]) + '-' + helpers.two(o.date[2]) + ' ' + helpers.two(o.date[3]) + ':' + helpers.two(o.date[4]) + ':' + helpers.two(o.date[5]);
        }

        return '';
    }

    // Helper to convert date into string
    obj.getDateString = function (value, options) {
        if (!options) {
            var options = {};
        }

        // Labels
        if (options && typeof (options) == 'object') {
            if (options.format) {
                var format = options.format;
            } else if (options.mask) {
                var format = options.mask;
            }
        } else {
            var format = options;
        }

        if (!format) {
            format = 'YYYY-MM-DD';
        }

        // Convert to number of hours
        if (format.indexOf('[h]') >= 0) {
            var result = 0;
            if (value && helpers.isNumeric(value)) {
                result = parseFloat(24 * Number(value));
                if (format.indexOf('mm') >= 0) {
                    var h = ('' + result).split('.');
                    if (h[1]) {
                        var d = 60 * parseFloat('0.' + h[1])
                        d = parseFloat(d.toFixed(2));
                    } else {
                        var d = 0;
                    }
                    result = parseInt(h[0]) + ':' + helpers.two(d);
                }
            }
            return result;
        }

        // Date instance
        if (value instanceof Date) {
            value = helpers_date.now(value);
        } else if (value && helpers.isNumeric(value)) {
            value = helpers_date.numToDate(value);
        }

        // Tokens
        var tokens = ['DAY', 'WD', 'DDDD', 'DDD', 'DD', 'D', 'Q', 'HH24', 'HH12', 'HH', 'H', 'AM/PM', 'MI', 'SS', 'MS', 'YYYY', 'YYY', 'YY', 'Y', 'MONTH', 'MON', 'MMMMM', 'MMMM', 'MMM', 'MM', 'M', '.'];

        // Expression to extract all tokens from the string
        var e = new RegExp(tokens.join('|'), 'gi');
        // Extract
        var t = format.match(e);

        // Compatibility with excel
        for (var i = 0; i < t.length; i++) {
            if (t[i].toUpperCase() == 'MM') {
                // Not a month, correct to minutes
                if (t[i - 1] && t[i - 1].toUpperCase().indexOf('H') >= 0) {
                    t[i] = 'mi';
                } else if (t[i - 2] && t[i - 2].toUpperCase().indexOf('H') >= 0) {
                    t[i] = 'mi';
                } else if (t[i + 1] && t[i + 1].toUpperCase().indexOf('S') >= 0) {
                    t[i] = 'mi';
                } else if (t[i + 2] && t[i + 2].toUpperCase().indexOf('S') >= 0) {
                    t[i] = 'mi';
                }
            }
        }

        // Object
        var o = {
            tokens: t
        }

        // Value
        if (value) {
            var d = '' + value;
            var splitStr = (d.indexOf('T') !== -1) ? 'T' : ' ';
            d = d.split(splitStr);

            var h = 0;
            var m = 0;
            var s = 0;

            if (d[1]) {
                h = d[1].split(':');
                m = h[1] ? h[1] : 0;
                s = h[2] ? h[2] : 0;
                h = h[0] ? h[0] : 0;
            }

            d = d[0].split('-');

            let day = new Date(d[0], d[1], 0).getDate();

            if (d[0] && d[1] && d[2] && d[0] > 0 && d[1] > 0 && d[1] < 13 && d[2] > 0 && d[2] <= day) {

                // Data
                o.data = [d[0], d[1], d[2], h, m, s];

                // Value
                o.value = [];

                // Calendar instance
                var calendar = new Date(o.data[0], o.data[1] - 1, o.data[2], o.data[3], o.data[4], o.data[5]);

                // Get method
                var get = function (i) {
                    // Token
                    var t = this.tokens[i];
                    // Case token
                    var s = t.toUpperCase();
                    var v = null;

                    if (s === 'YYYY') {
                        v = this.data[0];
                    } else if (s === 'YYY') {
                        v = this.data[0].substring(1, 4);
                    } else if (s === 'YY') {
                        v = this.data[0].substring(2, 4);
                    } else if (s === 'Y') {
                        v = this.data[0].substring(3, 4);
                    } else if (t === 'MON') {
                        v = helpers_date.months[calendar.getMonth()].substr(0, 3).toUpperCase();
                    } else if (t === 'mon') {
                        v = helpers_date.months[calendar.getMonth()].substr(0, 3).toLowerCase();
                    } else if (t === 'MONTH') {
                        v = helpers_date.months[calendar.getMonth()].toUpperCase();
                    } else if (t === 'month') {
                        v = helpers_date.months[calendar.getMonth()].toLowerCase();
                    } else if (s === 'MMMMM') {
                        v = helpers_date.months[calendar.getMonth()].substr(0, 1);
                    } else if (s === 'MMMM' || t === 'Month') {
                        v = helpers_date.months[calendar.getMonth()];
                    } else if (s === 'MMM' || t == 'Mon') {
                        v = helpers_date.months[calendar.getMonth()].substr(0, 3);
                    } else if (s === 'MM') {
                        v = helpers.two(this.data[1]);
                    } else if (s === 'M') {
                        v = calendar.getMonth() + 1;
                    } else if (t === 'DAY') {
                        v = helpers_date.weekdays[calendar.getDay()].toUpperCase();
                    } else if (t === 'day') {
                        v = helpers_date.weekdays[calendar.getDay()].toLowerCase();
                    } else if (s === 'DDDD' || t == 'Day') {
                        v = helpers_date.weekdays[calendar.getDay()];
                    } else if (s === 'DDD') {
                        v = helpers_date.weekdays[calendar.getDay()].substr(0, 3);
                    } else if (s === 'DD') {
                        v = helpers.two(this.data[2]);
                    } else if (s === 'D') {
                        v = parseInt(this.data[2]);
                    } else if (s === 'Q') {
                        v = Math.floor((calendar.getMonth() + 3) / 3);
                    } else if (s === 'HH24' || s === 'HH') {
                        v = this.data[3];
                        if (v > 12 && this.tokens.indexOf('am/pm') !== -1) {
                            v -= 12;
                        }
                        v = helpers.two(v);
                    } else if (s === 'HH12') {
                        if (this.data[3] > 12) {
                            v = helpers.two(this.data[3] - 12);
                        } else {
                            v = helpers.two(this.data[3]);
                        }
                    } else if (s === 'H') {
                        v = this.data[3];
                        if (v > 12 && this.tokens.indexOf('am/pm') !== -1) {
                            v -= 12;
                            v = helpers.two(v);
                        }
                    } else if (s === 'MI') {
                        v = helpers.two(this.data[4]);
                    } else if (s === 'SS') {
                        v = helpers.two(this.data[5]);
                    } else if (s === 'MS') {
                        v = calendar.getMilliseconds();
                    } else if (s === 'AM/PM') {
                        if (this.data[3] >= 12) {
                            v = 'PM';
                        } else {
                            v = 'AM';
                        }
                    } else if (s === 'WD') {
                        v = helpers_date.weekdays[calendar.getDay()];
                    }

                    if (v === null) {
                        this.value[i] = this.tokens[i];
                    } else {
                        this.value[i] = v;
                    }
                }

                for (var i = 0; i < o.tokens.length; i++) {
                    get.call(o, i);
                }
                // Put pieces together
                value = o.value.join('');
            } else {
                value = '';
            }
        }

        return value;
    }

    return obj;
}

/* harmony default export */ var mask = (Mask());

;// CONCATENATED MODULE: ./src/plugins/calendar.js







function Calendar() {
    var Component = (function (el, options) {
        // Already created, update options
        if (el.calendar) {
            return el.calendar.setOptions(options, true);
        }

        // New instance
        var obj = {type: 'calendar'};
        obj.options = {};

        // Date
        obj.date = null;

        /**
         * Update options
         */
        obj.setOptions = function (options, reset) {
            // Default configuration
            var defaults = {
                // Render type: [ default | year-month-picker ]
                type: 'default',
                // Restrictions
                validRange: null,
                // Starting weekday - 0 for sunday, 6 for saturday
                startingDay: null,
                // Date format
                format: 'DD/MM/YYYY',
                // Allow keyboard date entry
                readonly: true,
                // Today is default
                today: false,
                // Show timepicker
                time: false,
                // Show the reset button
                resetButton: true,
                // Placeholder
                placeholder: '',
                // Translations can be done here
                months: helpers_date.monthsShort,
                monthsFull: helpers_date.months,
                weekdays: helpers_date.weekdays,
                textDone: dictionary.translate('Done'),
                textReset: dictionary.translate('Reset'),
                textUpdate: dictionary.translate('Update'),
                // Value
                value: null,
                // Fullscreen (this is automatic set for screensize < 800)
                fullscreen: false,
                // Create the calendar closed as default
                opened: false,
                // Events
                onopen: null,
                onclose: null,
                onchange: null,
                onupdate: null,
                // Internal mode controller
                mode: null,
                position: null,
                // Data type
                dataType: null,
                // Controls
                controls: true,
                // Auto select
                autoSelect: true,
            }

            // Loop through our object
            for (var property in defaults) {
                if (options && options.hasOwnProperty(property)) {
                    obj.options[property] = options[property];
                } else {
                    if (typeof (obj.options[property]) == 'undefined' || reset === true) {
                        obj.options[property] = defaults[property];
                    }
                }
            }

            // Reset button
            if (obj.options.resetButton == false) {
                calendarReset.style.display = 'none';
            } else {
                calendarReset.style.display = '';
            }

            // Readonly
            if (obj.options.readonly) {
                el.setAttribute('readonly', 'readonly');
            } else {
                el.removeAttribute('readonly');
            }

            // Placeholder
            if (obj.options.placeholder) {
                el.setAttribute('placeholder', obj.options.placeholder);
            } else {
                el.removeAttribute('placeholder');
            }

            if (helpers.isNumeric(obj.options.value) && obj.options.value > 0) {
                obj.options.value = Component.numToDate(obj.options.value);
                // Data type numeric
                obj.options.dataType = 'numeric';
            }

            // Texts
            calendarReset.innerHTML = obj.options.textReset;
            calendarConfirm.innerHTML = obj.options.textDone;
            calendarControlsUpdateButton.innerHTML = obj.options.textUpdate;

            // Define mask
            if (obj.options.format) {
                el.setAttribute('data-mask', obj.options.format.toLowerCase());
            }

            // Value
            if (!obj.options.value && obj.options.today) {
                var value = Component.now();
            } else {
                var value = obj.options.value;
            }

            // Set internal date
            if (value) {
                // Force the update
                obj.options.value = null;
                // New value
                obj.setValue(value);
            }

            return obj;
        }

        /**
         * Open the calendar
         */
        obj.open = function (value) {
            if (!calendar.classList.contains('jcalendar-focus')) {
                if (!calendar.classList.contains('jcalendar-inline')) {
                    // Current
                    Component.current = obj;
                    // Start tracking
                    tracking(obj, true);
                    // Create the days
                    obj.getDays();
                    // Render months
                    if (obj.options.type == 'year-month-picker') {
                        obj.getMonths();
                    }
                    // Get time
                    if (obj.options.time) {
                        calendarSelectHour.value = obj.date[3];
                        calendarSelectMin.value = obj.date[4];
                    }

                    // Show calendar
                    calendar.classList.add('jcalendar-focus');

                    // Get the position of the corner helper
                    if (helpers.getWindowWidth() < 800 || obj.options.fullscreen) {
                        calendar.classList.add('jcalendar-fullsize');
                        // Animation
                        animation.slideBottom(calendarContent, 1);
                    } else {
                        calendar.classList.remove('jcalendar-fullsize');

                        var rect = el.getBoundingClientRect();
                        var rectContent = calendarContent.getBoundingClientRect();

                        if (obj.options.position) {
                            calendarContainer.style.position = 'fixed';
                            if (window.innerHeight < rect.bottom + rectContent.height) {
                                calendarContainer.style.top = (rect.top - (rectContent.height + 2)) + 'px';
                            } else {
                                calendarContainer.style.top = (rect.top + rect.height + 2) + 'px';
                            }
                            calendarContainer.style.left = rect.left + 'px';
                        } else {
                            if (window.innerHeight < rect.bottom + rectContent.height) {
                                var d = -1 * (rect.height + rectContent.height + 2);
                                if (d + rect.top < 0) {
                                    d = -1 * (rect.top + rect.height);
                                }
                                calendarContainer.style.top = d + 'px';
                            } else {
                                calendarContainer.style.top = 2 + 'px';
                            }

                            if (window.innerWidth < rect.left + rectContent.width) {
                                var d = window.innerWidth - (rect.left + rectContent.width + 20);
                                calendarContainer.style.left = d + 'px';
                            } else {
                                calendarContainer.style.left = '0px';
                            }
                        }
                    }

                    // Events
                    if (typeof (obj.options.onopen) == 'function') {
                        obj.options.onopen(el);
                    }
                }
            }
        }

        obj.close = function (ignoreEvents, update) {
            if (obj.options.autoSelect !== true && typeof(update) === 'undefined') {
                update = false;
            }
            if (calendar.classList.contains('jcalendar-focus')) {
                if (update !== false) {
                    var element = calendar.querySelector('.jcalendar-selected');

                    if (typeof (update) == 'string') {
                        var value = update;
                    } else if (!element || element.classList.contains('jcalendar-disabled')) {
                        var value = obj.options.value
                    } else {
                        var value = obj.getValue();
                    }

                    obj.setValue(value);
                } else {
                    let value = obj.options.value || '';
                    obj.options.value = null;
                    obj.setValue(value)
                }

                // Events
                if (!ignoreEvents && typeof (obj.options.onclose) == 'function') {
                    obj.options.onclose(el);
                }
                // Hide
                calendar.classList.remove('jcalendar-focus');
                // Stop tracking
                tracking(obj, false);
                // Current
                Component.current = null;
            }

            return obj.options.value;
        }

        obj.prev = function () {
            // Check if the visualization is the days picker or years picker
            if (obj.options.mode == 'years') {
                obj.date[0] = obj.date[0] - 12;

                // Update picker table of days
                obj.getYears();
            } else if (obj.options.mode == 'months') {
                obj.date[0] = parseInt(obj.date[0]) - 1;
                // Update picker table of months
                obj.getMonths();
            } else {
                // Go to the previous month
                if (obj.date[1] < 2) {
                    obj.date[0] = obj.date[0] - 1;
                    obj.date[1] = 12;
                } else {
                    obj.date[1] = obj.date[1] - 1;
                }

                // Update picker table of days
                obj.getDays();
            }
        }

        obj.next = function () {
            // Check if the visualization is the days picker or years picker
            if (obj.options.mode == 'years') {
                obj.date[0] = parseInt(obj.date[0]) + 12;

                // Update picker table of days
                obj.getYears();
            } else if (obj.options.mode == 'months') {
                obj.date[0] = parseInt(obj.date[0]) + 1;
                // Update picker table of months
                obj.getMonths();
            } else {
                // Go to the previous month
                if (obj.date[1] > 11) {
                    obj.date[0] = parseInt(obj.date[0]) + 1;
                    obj.date[1] = 1;
                } else {
                    obj.date[1] = parseInt(obj.date[1]) + 1;
                }

                // Update picker table of days
                obj.getDays();
            }
        }

        /**
         * Set today
         */
        obj.setToday = function () {
            // Today
            var value = new Date().toISOString().substr(0, 10);
            // Change value
            obj.setValue(value);
            // Value
            return value;
        }

        obj.setValue = function (val) {
            if (!val) {
                val = '' + val;
            }
            // Values
            var newValue = val;
            var oldValue = obj.options.value;

            if (oldValue != newValue) {
                // Set label
                if (!newValue) {
                    obj.date = null;
                    var val = '';
                    el.classList.remove('jcalendar_warning');
                    el.title = '';
                } else {
                    var value = obj.setLabel(newValue, obj.options);
                    var date = newValue.split(' ');
                    if (!date[1]) {
                        date[1] = '00:00:00';
                    }
                    var time = date[1].split(':')
                    var date = date[0].split('-');
                    var y = parseInt(date[0]);
                    var m = parseInt(date[1]);
                    var d = parseInt(date[2]);
                    var h = parseInt(time[0]);
                    var i = parseInt(time[1]);
                    obj.date = [y, m, d, h, i, 0];
                    var val = obj.setLabel(newValue, obj.options);

                    // Current selection day
                    var current = Component.now(new Date(y, m - 1, d), true);

                    // Available ranges
                    if (obj.options.validRange) {
                        if (!obj.options.validRange[0] || current >= obj.options.validRange[0]) {
                            var test1 = true;
                        } else {
                            var test1 = false;
                        }

                        if (!obj.options.validRange[1] || current <= obj.options.validRange[1]) {
                            var test2 = true;
                        } else {
                            var test2 = false;
                        }

                        if (!(test1 && test2)) {
                            el.classList.add('jcalendar_warning');
                            el.title = dictionary.translate('Date outside the valid range');
                        } else {
                            el.classList.remove('jcalendar_warning');
                            el.title = '';
                        }
                    } else {
                        el.classList.remove('jcalendar_warning');
                        el.title = '';
                    }
                }

                // New value
                obj.options.value = newValue;

                if (typeof (obj.options.onchange) == 'function') {
                    obj.options.onchange(el, newValue, oldValue);
                }

                // Lemonade JS
                if (el.value != val) {
                    el.value = val;
                    if (typeof (el.oninput) == 'function') {
                        el.oninput({
                            type: 'input',
                            target: el,
                            value: el.value
                        });
                    }
                }
            }

            if (obj.date) {
                obj.getDays();
                // Render months
                if (obj.options.type == 'year-month-picker') {
                    obj.getMonths();
                }
            }
        }

        obj.getValue = function () {
            if (obj.date) {
                if (obj.options.time) {
                    return helpers.two(obj.date[0]) + '-' + helpers.two(obj.date[1]) + '-' + helpers.two(obj.date[2]) + ' ' + helpers.two(obj.date[3]) + ':' + helpers.two(obj.date[4]) + ':' + helpers.two(0);
                } else {
                    return helpers.two(obj.date[0]) + '-' + helpers.two(obj.date[1]) + '-' + helpers.two(obj.date[2]) + ' ' + helpers.two(0) + ':' + helpers.two(0) + ':' + helpers.two(0);
                }
            } else {
                return "";
            }
        }

        /**
         *  Calendar
         */
        obj.update = function (element, v) {
            if (element.classList.contains('jcalendar-disabled')) {
                // Do nothing
            } else {
                var elements = calendar.querySelector('.jcalendar-selected');
                if (elements) {
                    elements.classList.remove('jcalendar-selected');
                }
                element.classList.add('jcalendar-selected');

                if (element.classList.contains('jcalendar-set-month')) {
                    obj.date[1] = v;
                    obj.date[2] = 1; // first day of the month
                } else {
                    obj.date[2] = element.innerText;
                }

                if (!obj.options.time) {
                    obj.close(null, true);
                } else {
                    obj.date[3] = calendarSelectHour.value;
                    obj.date[4] = calendarSelectMin.value;
                }
            }

            // Update
            updateActions();
        }

        /**
         * Set to blank
         */
        obj.reset = function () {
            // Close calendar
            obj.setValue('');
            obj.date = null;
            obj.close(false, false);
        }

        /**
         * Get calendar days
         */
        obj.getDays = function () {
            // Mode
            obj.options.mode = 'days';

            // Setting current values in case of NULLs
            var date = new Date();

            // Current selection
            var year = obj.date && helpers.isNumeric(obj.date[0]) ? obj.date[0] : parseInt(date.getFullYear());
            var month = obj.date && helpers.isNumeric(obj.date[1]) ? obj.date[1] : parseInt(date.getMonth()) + 1;
            var day = obj.date && helpers.isNumeric(obj.date[2]) ? obj.date[2] : parseInt(date.getDate());
            var hour = obj.date && helpers.isNumeric(obj.date[3]) ? obj.date[3] : parseInt(date.getHours());
            var min = obj.date && helpers.isNumeric(obj.date[4]) ? obj.date[4] : parseInt(date.getMinutes());

            // Selection container
            obj.date = [year, month, day, hour, min, 0];

            // Update title
            calendarLabelYear.innerHTML = year;
            calendarLabelMonth.innerHTML = obj.options.months[month - 1];

            // Current month and Year
            var isCurrentMonthAndYear = (date.getMonth() == month - 1) && (date.getFullYear() == year) ? true : false;
            var currentDay = date.getDate();

            // Number of days in the month
            var date = new Date(year, month, 0, 0, 0);
            var numberOfDays = date.getDate();

            // First day
            var date = new Date(year, month - 1, 0, 0, 0);
            var firstDay = date.getDay() + 1;

            // Index value
            var index = obj.options.startingDay || 0;

            // First of day relative to the starting calendar weekday
            firstDay = firstDay - index;

            // Reset table
            calendarBody.innerHTML = '';

            // Weekdays Row
            var row = document.createElement('tr');
            row.setAttribute('align', 'center');
            calendarBody.appendChild(row);

            // Create weekdays row
            for (var i = 0; i < 7; i++) {
                var cell = document.createElement('td');
                cell.classList.add('jcalendar-weekday')
                cell.innerHTML = obj.options.weekdays[index].substr(0, 1);
                row.appendChild(cell);
                // Next week day
                index++;
                // Restart index
                if (index > 6) {
                    index = 0;
                }
            }

            // Index of days
            var index = 0;
            var d = 0;

            // Calendar table
            for (var j = 0; j < 6; j++) {
                // Reset cells container
                var row = document.createElement('tr');
                row.setAttribute('align', 'center');
                row.style.height = '34px';

                // Create cells
                for (var i = 0; i < 7; i++) {
                    // Create cell
                    var cell = document.createElement('td');
                    cell.classList.add('jcalendar-set-day');

                    if (index >= firstDay && index < (firstDay + numberOfDays)) {
                        // Day cell
                        d++;
                        cell.innerHTML = d;

                        // Selected
                        if (d == day) {
                            cell.classList.add('jcalendar-selected');
                        }

                        // Current selection day is today
                        if (isCurrentMonthAndYear && currentDay == d) {
                            cell.style.fontWeight = 'bold';
                        }

                        // Current selection day
                        var current = Component.now(new Date(year, month - 1, d), true);

                        // Available ranges
                        if (obj.options.validRange) {
                            if (!obj.options.validRange[0] || current >= obj.options.validRange[0]) {
                                var test1 = true;
                            } else {
                                var test1 = false;
                            }

                            if (!obj.options.validRange[1] || current <= obj.options.validRange[1]) {
                                var test2 = true;
                            } else {
                                var test2 = false;
                            }

                            if (!(test1 && test2)) {
                                cell.classList.add('jcalendar-disabled');
                            }
                        }
                    }
                    // Day cell
                    row.appendChild(cell);
                    // Index
                    index++;
                }

                // Add cell to the calendar body
                calendarBody.appendChild(row);
            }

            // Show time controls
            if (obj.options.time) {
                calendarControlsTime.style.display = '';
            } else {
                calendarControlsTime.style.display = 'none';
            }

            // Update
            updateActions();
        }

        obj.getMonths = function () {
            // Mode
            obj.options.mode = 'months';

            // Loading month labels
            var months = obj.options.months;

            // Value
            var value = obj.options.value;

            // Current date
            var date = new Date();
            var currentYear = parseInt(date.getFullYear());
            var currentMonth = parseInt(date.getMonth()) + 1;
            var selectedYear = obj.date && helpers.isNumeric(obj.date[0]) ? obj.date[0] : currentYear;
            var selectedMonth = obj.date && helpers.isNumeric(obj.date[1]) ? obj.date[1] : currentMonth;

            // Update title
            calendarLabelYear.innerHTML = obj.date[0];
            calendarLabelMonth.innerHTML = months[selectedMonth - 1];

            // Table
            var table = document.createElement('table');
            table.setAttribute('width', '100%');

            // Row
            var row = null;

            // Calendar table
            for (var i = 0; i < 12; i++) {
                if (!(i % 4)) {
                    // Reset cells container
                    var row = document.createElement('tr');
                    row.setAttribute('align', 'center');
                    table.appendChild(row);
                }

                // Create cell
                var cell = document.createElement('td');
                cell.classList.add('jcalendar-set-month');
                cell.setAttribute('data-value', i + 1);
                cell.innerText = months[i];

                if (obj.options.validRange) {
                    var current = selectedYear + '-' + helpers.two(i + 1);
                    if (!obj.options.validRange[0] || current >= obj.options.validRange[0].substr(0, 7)) {
                        var test1 = true;
                    } else {
                        var test1 = false;
                    }

                    if (!obj.options.validRange[1] || current <= obj.options.validRange[1].substr(0, 7)) {
                        var test2 = true;
                    } else {
                        var test2 = false;
                    }

                    if (!(test1 && test2)) {
                        cell.classList.add('jcalendar-disabled');
                    }
                }

                if (i + 1 == selectedMonth) {
                    cell.classList.add('jcalendar-selected');
                }

                if (currentYear == selectedYear && i + 1 == currentMonth) {
                    cell.style.fontWeight = 'bold';
                }

                row.appendChild(cell);
            }

            calendarBody.innerHTML = '<tr><td colspan="7"></td></tr>';
            calendarBody.children[0].children[0].appendChild(table);

            // Update
            updateActions();
        }

        obj.getYears = function () {
            // Mode
            obj.options.mode = 'years';

            // Current date
            var date = new Date();
            var currentYear = date.getFullYear();
            var selectedYear = obj.date && helpers.isNumeric(obj.date[0]) ? obj.date[0] : parseInt(date.getFullYear());

            // Array of years
            var y = [];
            for (var i = 0; i < 25; i++) {
                y[i] = parseInt(obj.date[0]) + (i - 12);
            }

            // Assembling the year tables
            var table = document.createElement('table');
            table.setAttribute('width', '100%');

            for (var i = 0; i < 25; i++) {
                if (!(i % 5)) {
                    // Reset cells container
                    var row = document.createElement('tr');
                    row.setAttribute('align', 'center');
                    table.appendChild(row);
                }

                // Create cell
                var cell = document.createElement('td');
                cell.classList.add('jcalendar-set-year');
                cell.innerText = y[i];

                if (selectedYear == y[i]) {
                    cell.classList.add('jcalendar-selected');
                }

                if (currentYear == y[i]) {
                    cell.style.fontWeight = 'bold';
                }

                row.appendChild(cell);
            }

            calendarBody.innerHTML = '<tr><td colspan="7"></td></tr>';
            calendarBody.firstChild.firstChild.appendChild(table);

            // Update
            updateActions();
        }

        obj.setLabel = function (value, mixed) {
            return Component.getDateString(value, mixed);
        }

        obj.fromFormatted = function (value, format) {
            return Component.extractDateFromString(value, format);
        }

        var mouseUpControls = function (e) {
            var element = helpers.findElement(e.target, 'jcalendar-container');
            if (element) {
                var action = e.target.className;

                // Object id
                if (action == 'jcalendar-prev') {
                    obj.prev();
                } else if (action == 'jcalendar-next') {
                    obj.next();
                } else if (action == 'jcalendar-month') {
                    obj.getMonths();
                } else if (action == 'jcalendar-year') {
                    obj.getYears();
                } else if (action == 'jcalendar-set-year') {
                    obj.date[0] = e.target.innerText;
                    if (obj.options.type == 'year-month-picker') {
                        obj.getMonths();
                    } else {
                        obj.getDays();
                    }
                } else if (e.target.classList.contains('jcalendar-set-month')) {
                    var month = parseInt(e.target.getAttribute('data-value'));
                    if (obj.options.type == 'year-month-picker') {
                        obj.update(e.target, month);
                    } else {
                        obj.date[1] = month;
                        obj.getDays();
                    }
                } else if (action == 'jcalendar-confirm' || action == 'jcalendar-update' || action == 'jcalendar-close') {
                    obj.close(null, true);
                } else if (action == 'jcalendar-backdrop') {
                    obj.close(false, false);
                } else if (action == 'jcalendar-reset') {
                    obj.reset();
                } else if (e.target.classList.contains('jcalendar-set-day') && e.target.innerText) {
                    obj.update(e.target);
                }
            } else {
                obj.close(false, false);
            }
        }

        var keyUpControls = function (e) {
            if (e.target.value && e.target.value.length > 3) {
                var test = Component.extractDateFromString(e.target.value, obj.options.format);
                if (test) {
                    obj.setValue(test);
                }
            }
        }

        // Update actions button
        var updateActions = function () {
            var currentDay = calendar.querySelector('.jcalendar-selected');

            if (currentDay && currentDay.classList.contains('jcalendar-disabled')) {
                calendarControlsUpdateButton.setAttribute('disabled', 'disabled');
                calendarSelectHour.setAttribute('disabled', 'disabled');
                calendarSelectMin.setAttribute('disabled', 'disabled');
            } else {
                calendarControlsUpdateButton.removeAttribute('disabled');
                calendarSelectHour.removeAttribute('disabled');
                calendarSelectMin.removeAttribute('disabled');
            }

            // Event
            if (typeof (obj.options.onupdate) == 'function') {
                obj.options.onupdate(el, obj.getValue());
            }
        }

        var calendar = null;
        var calendarReset = null;
        var calendarConfirm = null;
        var calendarContainer = null;
        var calendarContent = null;
        var calendarLabelYear = null;
        var calendarLabelMonth = null;
        var calendarTable = null;
        var calendarBody = null;

        var calendarControls = null;
        var calendarControlsTime = null;
        var calendarControlsUpdate = null;
        var calendarControlsUpdateButton = null;
        var calendarSelectHour = null;
        var calendarSelectMin = null;

        var init = function () {
            // Get value from initial element if that is an input
            if (el.tagName == 'INPUT' && el.value) {
                options.value = el.value;
            }

            // Calendar DOM elements
            calendarReset = document.createElement('div');
            calendarReset.className = 'jcalendar-reset';

            calendarConfirm = document.createElement('div');
            calendarConfirm.className = 'jcalendar-confirm';

            calendarControls = document.createElement('div');
            calendarControls.className = 'jcalendar-controls'
            calendarControls.style.borderBottom = '1px solid #ddd';
            calendarControls.appendChild(calendarReset);
            calendarControls.appendChild(calendarConfirm);

            calendarContainer = document.createElement('div');
            calendarContainer.className = 'jcalendar-container';
            calendarContent = document.createElement('div');
            calendarContent.className = 'jcalendar-content';
            calendarContainer.appendChild(calendarContent);

            // Main element
            if (el.tagName == 'DIV') {
                calendar = el;
                calendar.classList.add('jcalendar-inline');
            } else {
                // Add controls to the screen
                calendarContent.appendChild(calendarControls);

                calendar = document.createElement('div');
                calendar.className = 'jcalendar';
            }
            calendar.classList.add('jcalendar-container');
            calendar.appendChild(calendarContainer);

            // Table container
            var calendarTableContainer = document.createElement('div');
            calendarTableContainer.className = 'jcalendar-table';
            calendarContent.appendChild(calendarTableContainer);

            // Previous button
            var calendarHeaderPrev = document.createElement('td');
            calendarHeaderPrev.setAttribute('colspan', '2');
            calendarHeaderPrev.className = 'jcalendar-prev';

            // Header with year and month
            calendarLabelYear = document.createElement('span');
            calendarLabelYear.className = 'jcalendar-year';
            calendarLabelMonth = document.createElement('span');
            calendarLabelMonth.className = 'jcalendar-month';

            var calendarHeaderTitle = document.createElement('td');
            calendarHeaderTitle.className = 'jcalendar-header';
            calendarHeaderTitle.setAttribute('colspan', '3');
            calendarHeaderTitle.appendChild(calendarLabelMonth);
            calendarHeaderTitle.appendChild(calendarLabelYear);

            var calendarHeaderNext = document.createElement('td');
            calendarHeaderNext.setAttribute('colspan', '2');
            calendarHeaderNext.className = 'jcalendar-next';

            var calendarHeader = document.createElement('thead');
            var calendarHeaderRow = document.createElement('tr');
            calendarHeaderRow.appendChild(calendarHeaderPrev);
            calendarHeaderRow.appendChild(calendarHeaderTitle);
            calendarHeaderRow.appendChild(calendarHeaderNext);
            calendarHeader.appendChild(calendarHeaderRow);

            calendarTable = document.createElement('table');
            calendarBody = document.createElement('tbody');
            calendarTable.setAttribute('cellpadding', '0');
            calendarTable.setAttribute('cellspacing', '0');
            calendarTable.appendChild(calendarHeader);
            calendarTable.appendChild(calendarBody);
            calendarTableContainer.appendChild(calendarTable);

            calendarSelectHour = document.createElement('select');
            calendarSelectHour.className = 'jcalendar-select';
            calendarSelectHour.onchange = function () {
                obj.date[3] = this.value;

                // Event
                if (typeof (obj.options.onupdate) == 'function') {
                    obj.options.onupdate(el, obj.getValue());
                }
            }

                for (var i = 0; i < 24; i++) {
                    var element = document.createElement('option');
                    element.value = i;
                    element.innerHTML = helpers.two(i);
                    calendarSelectHour.appendChild(element);
                }

            calendarSelectMin = document.createElement('select');
            calendarSelectMin.className = 'jcalendar-select';
            calendarSelectMin.onchange = function () {
                obj.date[4] = this.value;

                // Event
                if (typeof (obj.options.onupdate) == 'function') {
                    obj.options.onupdate(el, obj.getValue());
                }
            }

            for (var i = 0; i < 60; i++) {
                var element = document.createElement('option');
                element.value = i;
                element.innerHTML = helpers.two(i);
                calendarSelectMin.appendChild(element);
            }

            // Footer controls
            var calendarControlsFooter = document.createElement('div');
            calendarControlsFooter.className = 'jcalendar-controls';

            calendarControlsTime = document.createElement('div');
            calendarControlsTime.className = 'jcalendar-time';
            calendarControlsTime.style.maxWidth = '140px';
            calendarControlsTime.appendChild(calendarSelectHour);
            calendarControlsTime.appendChild(calendarSelectMin);

            calendarControlsUpdateButton = document.createElement('button');
            calendarControlsUpdateButton.setAttribute('type', 'button');
            calendarControlsUpdateButton.className = 'jcalendar-update';

            calendarControlsUpdate = document.createElement('div');
            calendarControlsUpdate.style.flexGrow = '10';
            calendarControlsUpdate.appendChild(calendarControlsUpdateButton);
            calendarControlsFooter.appendChild(calendarControlsTime);

            // Only show the update button for input elements
            if (el.tagName == 'INPUT') {
                calendarControlsFooter.appendChild(calendarControlsUpdate);
            }

            calendarContent.appendChild(calendarControlsFooter);

            var calendarBackdrop = document.createElement('div');
            calendarBackdrop.className = 'jcalendar-backdrop';
            calendar.appendChild(calendarBackdrop);

            // Handle events
            el.addEventListener("keyup", keyUpControls);

            // Add global events
            calendar.addEventListener("swipeleft", function (e) {
                animation.slideLeft(calendarTable, 0, function () {
                    obj.next();
                    animation.slideRight(calendarTable, 1);
                });
                e.preventDefault();
                e.stopPropagation();
            });

            calendar.addEventListener("swiperight", function (e) {
                animation.slideRight(calendarTable, 0, function () {
                    obj.prev();
                    animation.slideLeft(calendarTable, 1);
                });
                e.preventDefault();
                e.stopPropagation();
            });

            if ('ontouchend' in document.documentElement === true) {
                calendar.addEventListener("touchend", mouseUpControls);
                el.addEventListener("touchend", obj.open);
            } else {
                calendar.addEventListener("mouseup", mouseUpControls);
                el.addEventListener("mouseup", obj.open);
            }

            // Global controls
            if (!Component.hasEvents) {
                // Execute only one time
                Component.hasEvents = true;
                // Enter and Esc
                document.addEventListener("keydown", Component.keydown);
            }

            // Set configuration
            obj.setOptions(options);

            // Append element to the DOM
            if (el.tagName == 'INPUT') {
                el.parentNode.insertBefore(calendar, el.nextSibling);
                // Add properties
                el.setAttribute('autocomplete', 'off');
                // Element
                el.classList.add('jcalendar-input');
                // Value
                el.value = obj.setLabel(obj.getValue(), obj.options);
            } else {
                // Get days
                obj.getDays();
                // Hour
                if (obj.options.time) {
                    calendarSelectHour.value = obj.date[3];
                    calendarSelectMin.value = obj.date[4];
                }
            }

            // Default opened
            if (obj.options.opened == true) {
                obj.open();
            }

            // Controls
            if (obj.options.controls == false) {
                calendarContainer.classList.add('jcalendar-hide-controls');
            }

            // Change method
            el.change = obj.setValue;

            // Global generic value handler
            el.val = function (val) {
                if (val === undefined) {
                    return obj.getValue();
                } else {
                    obj.setValue(val);
                }
            }

            // Keep object available from the node
            el.calendar = calendar.calendar = obj;
        }

        init();

        return obj;
    });

    Component.keydown = function (e) {
        var calendar = null;
        if (calendar = Component.current) {
            if (e.which == 13) {
                // ENTER
                calendar.close(false, true);
            } else if (e.which == 27) {
                // ESC
                calendar.close(false, false);
            }
        }
    }

    Component.prettify = function (d, texts) {
        if (!texts) {
            var texts = {
                justNow: 'Just now',
                xMinutesAgo: '{0}m ago',
                xHoursAgo: '{0}h ago',
                xDaysAgo: '{0}d ago',
                xWeeksAgo: '{0}w ago',
                xMonthsAgo: '{0} mon ago',
                xYearsAgo: '{0}y ago',
            }
        }

        if (d.indexOf('GMT') === -1 && d.indexOf('Z') === -1) {
            d += ' GMT';
        }

        var d1 = new Date();
        var d2 = new Date(d);
        var total = parseInt((d1 - d2) / 1000 / 60);

        String.prototype.format = function (o) {
            return this.replace('{0}', o);
        }

        if (total == 0) {
            var text = texts.justNow;
        } else if (total < 90) {
            var text = texts.xMinutesAgo.format(total);
        } else if (total < 1440) { // One day
            var text = texts.xHoursAgo.format(Math.round(total / 60));
        } else if (total < 20160) { // 14 days
            var text = texts.xDaysAgo.format(Math.round(total / 1440));
        } else if (total < 43200) { // 30 days
            var text = texts.xWeeksAgo.format(Math.round(total / 10080));
        } else if (total < 1036800) { // 24 months
            var text = texts.xMonthsAgo.format(Math.round(total / 43200));
        } else { // 24 months+
            var text = texts.xYearsAgo.format(Math.round(total / 525600));
        }

        return text;
    }

    Component.prettifyAll = function () {
        var elements = document.querySelectorAll('.prettydate');
        for (var i = 0; i < elements.length; i++) {
            if (elements[i].getAttribute('data-date')) {
                elements[i].innerHTML = Component.prettify(elements[i].getAttribute('data-date'));
            } else {
                if (elements[i].innerHTML) {
                    elements[i].setAttribute('title', elements[i].innerHTML);
                    elements[i].setAttribute('data-date', elements[i].innerHTML);
                    elements[i].innerHTML = Component.prettify(elements[i].innerHTML);
                }
            }
        }
    }

    Component.now = helpers_date.now;
    Component.toArray = helpers_date.toArray;
    Component.dateToNum = helpers_date.dateToNum
    Component.numToDate = helpers_date.numToDate;
    Component.weekdays = helpers_date.weekdays;
    Component.months = helpers_date.months;
    Component.weekdaysShort = helpers_date.weekdaysShort;
    Component.monthsShort = helpers_date.monthsShort;

    // Legacy shortcut
    Component.extractDateFromString = mask.extractDateFromString;
    Component.getDateString = mask.getDateString;

    return Component;
}

/* harmony default export */ var calendar = (Calendar());
;// CONCATENATED MODULE: ./src/plugins/palette.js
// More palettes https://coolors.co/ or https://gka.github.io/palettes/#/10|s|003790,005647,ffffe0|ffffe0,ff005e,93003a|1|1

function Palette() {

    var palette = {
        material: [
            ["#ffebee", "#fce4ec", "#f3e5f5", "#e8eaf6", "#e3f2fd", "#e0f7fa", "#e0f2f1", "#e8f5e9", "#f1f8e9", "#f9fbe7", "#fffde7", "#fff8e1", "#fff3e0", "#fbe9e7", "#efebe9", "#fafafa", "#eceff1"],
            ["#ffcdd2", "#f8bbd0", "#e1bee7", "#c5cae9", "#bbdefb", "#b2ebf2", "#b2dfdb", "#c8e6c9", "#dcedc8", "#f0f4c3", "#fff9c4", "#ffecb3", "#ffe0b2", "#ffccbc", "#d7ccc8", "#f5f5f5", "#cfd8dc"],
            ["#ef9a9a", "#f48fb1", "#ce93d8", "#9fa8da", "#90caf9", "#80deea", "#80cbc4", "#a5d6a7", "#c5e1a5", "#e6ee9c", "#fff59d", "#ffe082", "#ffcc80", "#ffab91", "#bcaaa4", "#eeeeee", "#b0bec5"],
            ["#e57373", "#f06292", "#ba68c8", "#7986cb", "#64b5f6", "#4dd0e1", "#4db6ac", "#81c784", "#aed581", "#dce775", "#fff176", "#ffd54f", "#ffb74d", "#ff8a65", "#a1887f", "#e0e0e0", "#90a4ae"],
            ["#ef5350", "#ec407a", "#ab47bc", "#5c6bc0", "#42a5f5", "#26c6da", "#26a69a", "#66bb6a", "#9ccc65", "#d4e157", "#ffee58", "#ffca28", "#ffa726", "#ff7043", "#8d6e63", "#bdbdbd", "#78909c"],
            ["#f44336", "#e91e63", "#9c27b0", "#3f51b5", "#2196f3", "#00bcd4", "#009688", "#4caf50", "#8bc34a", "#cddc39", "#ffeb3b", "#ffc107", "#ff9800", "#ff5722", "#795548", "#9e9e9e", "#607d8b"],
            ["#e53935", "#d81b60", "#8e24aa", "#3949ab", "#1e88e5", "#00acc1", "#00897b", "#43a047", "#7cb342", "#c0ca33", "#fdd835", "#ffb300", "#fb8c00", "#f4511e", "#6d4c41", "#757575", "#546e7a"],
            ["#d32f2f", "#c2185b", "#7b1fa2", "#303f9f", "#1976d2", "#0097a7", "#00796b", "#388e3c", "#689f38", "#afb42b", "#fbc02d", "#ffa000", "#f57c00", "#e64a19", "#5d4037", "#616161", "#455a64"],
            ["#c62828", "#ad1457", "#6a1b9a", "#283593", "#1565c0", "#00838f", "#00695c", "#2e7d32", "#558b2f", "#9e9d24", "#f9a825", "#ff8f00", "#ef6c00", "#d84315", "#4e342e", "#424242", "#37474f"],
            ["#b71c1c", "#880e4f", "#4a148c", "#1a237e", "#0d47a1", "#006064", "#004d40", "#1b5e20", "#33691e", "#827717", "#f57f17", "#ff6f00", "#e65100", "#bf360c", "#3e2723", "#212121", "#263238"],
        ],
        fire: [
            ["0b1a6d", "840f38", "b60718", "de030b", "ff0c0c", "fd491c", "fc7521", "faa331", "fbb535", "ffc73a"],
            ["071147", "5f0b28", "930513", "be0309", "ef0000", "fa3403", "fb670b", "f9991b", "faad1e", "ffc123"],
            ["03071e", "370617", "6a040f", "9d0208", "d00000", "dc2f02", "e85d04", "f48c06", "faa307", "ffba08"],
            ["020619", "320615", "61040d", "8c0207", "bc0000", "c82a02", "d05203", "db7f06", "e19405", "efab00"],
            ["020515", "2d0513", "58040c", "7f0206", "aa0000", "b62602", "b94903", "c57205", "ca8504", "d89b00"],
        ],
        baby: [
            ["eddcd2", "fff1e6", "fde2e4", "fad2e1", "c5dedd", "dbe7e4", "f0efeb", "d6e2e9", "bcd4e6", "99c1de"],
            ["e1c4b3", "ffd5b5", "fab6ba", "f5a8c4", "aacecd", "bfd5cf", "dbd9d0", "baceda", "9dc0db", "7eb1d5"],
            ["daa990", "ffb787", "f88e95", "f282a9", "8fc4c3", "a3c8be", "cec9b3", "9dbcce", "82acd2", "649dcb"],
            ["d69070", "ff9c5e", "f66770", "f05f8f", "74bbb9", "87bfae", "c5b993", "83aac3", "699bca", "4d89c2"],
            ["c97d5d", "f58443", "eb4d57", "e54a7b", "66a9a7", "78ae9c", "b5a67e", "7599b1", "5c88b7", "4978aa"],
        ],
        chart: [
            ['#C1D37F', '#4C5454', '#FFD275', '#66586F', '#D05D5B', '#C96480', '#95BF8F', '#6EA240', '#0F0F0E', '#EB8258', '#95A3B3', '#995D81'],
        ],
    }

    var Component = function (o) {
        // Otherwise get palette value
        if (palette[o]) {
            return palette[o];
        } else {
            return palette.material;
        }
    }

    Component.get = function (o) {
        // Otherwise get palette value
        if (palette[o]) {
            return palette[o];
        } else {
            return palette;
        }
    }

    Component.set = function (o, v) {
        palette[o] = v;
    }

    return Component;
}

/* harmony default export */ var palette = (Palette());
;// CONCATENATED MODULE: ./src/plugins/tabs.js




function Tabs(el, options) {
    var obj = {};
    obj.options = {};

    // Default configuration
    var defaults = {
        data: [],
        position: null,
        allowCreate: false,
        allowChangePosition: false,
        onclick: null,
        onload: null,
        onchange: null,
        oncreate: null,
        ondelete: null,
        onbeforecreate: null,
        onchangeposition: null,
        animation: false,
        hideHeaders: false,
        padding: null,
        palette: null,
        maxWidth: null,
    }

    // Loop through the initial configuration
    for (var property in defaults) {
        if (options && options.hasOwnProperty(property)) {
            obj.options[property] = options[property];
        } else {
            obj.options[property] = defaults[property];
        }
    }

    // Class
    el.classList.add('jtabs');

    var prev = null;
    var next = null;
    var border = null;

    // Helpers
    const setBorder = function(index) {
        if (obj.options.animation) {
            setTimeout(function() {
                let rect = obj.headers.children[index].getBoundingClientRect();

                if (obj.options.palette === 'modern') {
                    border.style.width = rect.width - 4 + 'px';
                    border.style.left = obj.headers.children[index].offsetLeft + 2 + 'px';
                } else {
                    border.style.width = rect.width + 'px';
                    border.style.left = obj.headers.children[index].offsetLeft + 'px';
                }

                if (obj.options.position === 'bottom') {
                    border.style.top = '0px';
                } else {
                    border.style.bottom = '0px';
                }
            }, 50);
        }
    }

    var updateControls = function(x) {
        if (typeof(obj.headers.scrollTo) == 'function') {
            obj.headers.scrollTo({
                left: x,
                behavior: 'smooth',
            });
        } else {
            obj.headers.scrollLeft = x;
        }

        if (x <= 1) {
            prev.classList.add('disabled');
        } else {
            prev.classList.remove('disabled');
        }

        if (x >= obj.headers.scrollWidth - obj.headers.offsetWidth) {
            next.classList.add('disabled');
        } else {
            next.classList.remove('disabled');
        }

        if (obj.headers.scrollWidth <= obj.headers.offsetWidth) {
            prev.style.display = 'none';
            next.style.display = 'none';
        } else {
            prev.style.display = '';
            next.style.display = '';
        }
    }

    obj.setBorder = setBorder;

    // Set value
    obj.open = function(index) {
        // This is to force safari to update the children
        const items = Array.from(obj.content.children);
        if (! obj.content.children[index]) {
            return;
        }

        var previous = null;
        for (var i = 0; i < obj.headers.children.length; i++) {
            if (obj.headers.children[i].classList.contains('jtabs-selected')) {
                // Current one
                previous = i;
            }
            // Remote selected
            obj.headers.children[i].classList.remove('jtabs-selected');
            obj.headers.children[i].removeAttribute('aria-selected')
            if (obj.content.children[i]) {
                obj.content.children[i].classList.remove('jtabs-selected');
            }
        }

        obj.headers.children[index].classList.add('jtabs-selected');
        obj.headers.children[index].setAttribute('aria-selected', 'true')

        if (obj.content.children[index]) {
            obj.content.children[index].classList.add('jtabs-selected');
        }

        if (previous != index && typeof(obj.options.onchange) == 'function') {
            if (obj.content.children[index]) {
                obj.options.onchange(el, obj, index, obj.headers.children[index], obj.content.children[index]);
            }
        }

        // Hide
        if (obj.options.hideHeaders == true && (obj.headers.children.length < 3 && obj.options.allowCreate == false)) {
            obj.headers.parentNode.style.display = 'none';
        } else {
            obj.headers.parentNode.style.display = '';

            var x1 = obj.headers.children[index].offsetLeft;
            var x2 = x1 + obj.headers.children[index].offsetWidth;
            var r1 = obj.headers.scrollLeft;
            var r2 = r1 + obj.headers.offsetWidth;

            if (! (r1 <= x1 && r2 >= x2)) {
                // Out of the viewport
                updateControls(x1 - 1);
            }

            // Set border
            setBorder(index);
        }
    }

    obj.selectIndex = function(a) {
        var index = Array.prototype.indexOf.call(obj.headers.children, a);
        if (index >= 0) {
            obj.open(index);
        }

        return index;
    }

    obj.rename = function(i, title) {
        if (! title) {
            title = prompt('New title', obj.headers.children[i].innerText);
        }
        obj.headers.children[i].innerText = title;
        setBorder(obj.getActive());
    }

    obj.create = function(title, url) {
        if (typeof(obj.options.onbeforecreate) == 'function') {
            var ret = obj.options.onbeforecreate(el, title);
            if (ret === false) {
                return false;
            } else {
                title = ret;
            }
        }

        var div = obj.appendElement(title);

        if (typeof(obj.options.oncreate) == 'function') {
            obj.options.oncreate(el, div)
        }

        setBorder(obj.getActive());

        return div;
    }

    obj.remove = function(index) {
        return obj.deleteElement(index);
    }

    obj.nextNumber = function() {
        var num = 0;
        for (var i = 0; i < obj.headers.children.length; i++) {
            var tmp = obj.headers.children[i].innerText.match(/[0-9].*/);
            if (tmp > num) {
                num = parseInt(tmp);
            }
        }
        if (! num) {
            num = 1;
        } else {
            num++;
        }

        return num;
    }

    obj.deleteElement = function(index) {
        let current = obj.getActive();

        if (! obj.headers.children[index]) {
            return false;
        } else {
            obj.headers.removeChild(obj.headers.children[index]);
            obj.content.removeChild(obj.content.children[index]);
        }

        if (current === index) {
            obj.open(0);
        } else {
            let current = obj.getActive() || 0;
            setBorder(current);
        }

        if (typeof(obj.options.ondelete) == 'function') {
            obj.options.ondelete(el, index)
        }
    }

    obj.appendElement = function(title, cb, openTab, position) {
        if (! title) {
            var title = prompt('Title?', '');
        }

        if (title) {
            let headerId = helpers.guid();
            let contentId = helpers.guid();
            // Add content
            var div = document.createElement('div');
            div.setAttribute('id', contentId);
            div.setAttribute('role', 'tabpanel');
            div.setAttribute('aria-labelledby', headerId);

            // Add headers
            var h = document.createElement('div');
            h.setAttribute('id', headerId);
            h.setAttribute('role', 'tab');
            h.setAttribute('aria-controls', contentId);

            h.textContent = title;
            h.content = div;

            if (typeof(position) === 'undefined') {
                obj.content.appendChild(div);
                obj.headers.insertBefore(h, obj.headers.lastChild);
            } else {
                let r = obj.content.children[position];
                if (r) {
                    obj.content.insertBefore(div, r);
                } else {
                    obj.content.appendChild(div);
                }
                r = obj.headers.children[position] || obj.headers.lastChild;
                obj.headers.insertBefore(h, r);
            }

            // Sortable
            if (obj.options.allowChangePosition) {
                h.setAttribute('draggable', 'true');
            }

            // Open new tab
            if (openTab !== false) {
                // Open new tab
                obj.selectIndex(h);
            }

            // Callback
            if (typeof(cb) == 'function') {
                cb(div, h);
            }

            // Return element
            return div;
        }
    }

    obj.getActive = function() {
        for (var i = 0; i < obj.headers.children.length; i++) {
            if (obj.headers.children[i].classList.contains('jtabs-selected')) {
                return i;
            }
        }
        return false;
    }

    obj.updateContent = function(position, newContent) {
        if (typeof newContent !== 'string') {
            var contentItem = newContent;
        } else {
            var contentItem = document.createElement('div');
            contentItem.innerHTML = newContent;
        }

        if (obj.content.children[position].classList.contains('jtabs-selected')) {
            newContent.classList.add('jtabs-selected');
        }

        obj.content.replaceChild(newContent, obj.content.children[position]);

        setBorder();
    }

    obj.updatePosition = function(f, t, ignoreEvents, openTab) {
        // Ondrop update position of content
        if (f > t) {
            obj.content.insertBefore(obj.content.children[f], obj.content.children[t]);
        } else {
            obj.content.insertBefore(obj.content.children[f], obj.content.children[t].nextSibling);
        }

        // Open destination tab
        if (openTab !== false) {
            obj.open(t);
        } else {
            const activeIndex = obj.getActive();

            if (t < activeIndex) {
                obj.setBorder(activeIndex);
            }
        }

        // Call event
        if (! ignoreEvents && typeof(obj.options.onchangeposition) == 'function') {
            obj.options.onchangeposition(obj.headers, f, t);
        }
    }

    obj.move = function(f, t, ignoreEvents, openTab) {
        if (f > t) {
            obj.headers.insertBefore(obj.headers.children[f], obj.headers.children[t]);
        } else {
            obj.headers.insertBefore(obj.headers.children[f], obj.headers.children[t].nextSibling);
        }

        obj.updatePosition(f, t, ignoreEvents, openTab);
    }

    obj.setBorder = setBorder;

    obj.init = function() {
        el.textContent = '';

        // Make sure the component is blank
        obj.headers = document.createElement('div');
        obj.content = document.createElement('div');
        obj.headers.classList.add('jtabs-headers');
        obj.headers.setAttribute('role', 'tablist');
        obj.content.classList.add('jtabs-content');
        obj.content.setAttribute('role', 'region');
        obj.content.setAttribute('aria-label', 'Tab Panels');

        if (obj.options.palette) {
            el.classList.add('jtabs-modern');
        } else {
            el.classList.remove('jtabs-modern');
        }

        // Padding
        if (obj.options.padding) {
            obj.content.style.padding = parseInt(obj.options.padding) + 'px';
        }

        // Header
        var header = document.createElement('div');
        header.className = 'jtabs-headers-container';
        header.appendChild(obj.headers);
        if (obj.options.maxWidth) {
            header.style.maxWidth = parseInt(obj.options.maxWidth) + 'px';
        }

        // Controls
        var controls = document.createElement('div');
        controls.className = 'jtabs-controls';
        controls.setAttribute('draggable', 'false');
        header.appendChild(controls);

        // Append DOM elements
        if (obj.options.position == 'bottom') {
            el.appendChild(obj.content);
            el.appendChild(header);
        } else {
            el.appendChild(header);
            el.appendChild(obj.content);
        }

        // New button
        if (obj.options.allowCreate == true) {
            var add = document.createElement('div');
            add.className = 'jtabs-add';
            add.onclick = function() {
                obj.create();
            }
            controls.appendChild(add);
        }

        prev = document.createElement('div');
        prev.className = 'jtabs-prev';
        prev.onclick = function() {
            updateControls(obj.headers.scrollLeft - obj.headers.offsetWidth);
        }
        controls.appendChild(prev);

        next = document.createElement('div');
        next.className = 'jtabs-next';
        next.onclick = function() {
            updateControls(obj.headers.scrollLeft + obj.headers.offsetWidth);
        }
        controls.appendChild(next);

        // Data
        for (var i = 0; i < obj.options.data.length; i++) {
            // Title
            if (obj.options.data[i].titleElement) {
                var headerItem = obj.options.data[i].titleElement;
            } else {
                var headerItem = document.createElement('div');
            }
            // Icon
            if (obj.options.data[i].icon) {
                var iconContainer = document.createElement('div');
                var icon = document.createElement('i');
                icon.classList.add('material-icons');
                icon.textContent = obj.options.data[i].icon;
                iconContainer.appendChild(icon);
                headerItem.appendChild(iconContainer);
            }
            // Title
            if (obj.options.data[i].title) {
                var title = document.createTextNode(obj.options.data[i].title);
                headerItem.appendChild(title);
            }
            // Width
            if (obj.options.data[i].width) {
                headerItem.style.width = obj.options.data[i].width;
            }
            // Content
            if (obj.options.data[i].contentElement) {
                var contentItem = obj.options.data[i].contentElement;
            } else {
                var contentItem = document.createElement('div');
                contentItem.innerHTML = obj.options.data[i].content;
            }
            obj.headers.appendChild(headerItem);
            obj.content.appendChild(contentItem);
        }

        // Animation
        border = document.createElement('div');
        border.className = 'jtabs-border';
        obj.headers.appendChild(border);

        if (obj.options.animation) {
            el.classList.add('jtabs-animation');
        }

        // Events
        obj.headers.addEventListener("click", function(e) {
            if (e.target.parentNode.classList.contains('jtabs-headers')) {
                var target = e.target;
            } else {
                if (e.target.tagName == 'I') {
                    var target = e.target.parentNode.parentNode;
                } else {
                    var target = e.target.parentNode;
                }
            }

            var index = obj.selectIndex(target);

            if (typeof(obj.options.onclick) == 'function') {
                obj.options.onclick(el, obj, index, obj.headers.children[index], obj.content.children[index]);
            }
        });

        obj.headers.addEventListener("contextmenu", function(e) {
            obj.selectIndex(e.target);
        });

        if (obj.headers.children.length) {
            // Open first tab
            obj.open(0);
        }

        // Update controls
        updateControls(0);

        if (obj.options.allowChangePosition == true) {
            Sorting(obj.headers, {
                direction: 1,
                ondrop: function(a,b,c) {
                    obj.updatePosition(b,c);
                },
            });
        }

        if (typeof(obj.options.onload) == 'function') {
            obj.options.onload(el, obj);
        }
    }

    // Loading existing nodes as the data
    if (el.children[0] && el.children[0].children.length) {
        // Create from existing elements
        for (var i = 0; i < el.children[0].children.length; i++) {
            var item = obj.options.data && obj.options.data[i] ? obj.options.data[i] : {};

            if (el.children[1] && el.children[1].children[i]) {
                item.titleElement = el.children[0].children[i];
                item.contentElement = el.children[1].children[i];
            } else {
                item.contentElement = el.children[0].children[i];
            }

            obj.options.data[i] = item;
        }
    }

    // Remote controller flag
    var loadingRemoteData = false;

    // Create from data
    if (obj.options.data) {
        // Append children
        for (var i = 0; i < obj.options.data.length; i++) {
            if (obj.options.data[i].url) {
                ajax({
                    url: obj.options.data[i].url,
                    type: 'GET',
                    dataType: 'text/html',
                    index: i,
                    success: function(result) {
                        obj.options.data[this.index].content = result;
                    },
                    complete: function() {
                        obj.init();
                    }
                });

                // Flag loading
                loadingRemoteData = true;
            }
        }
    }

    if (! loadingRemoteData) {
        obj.init();
    }

    el.tabs = obj;

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/color.js






function Color(el, options) {
    // Already created, update options
    if (el.color) {
        return el.color.setOptions(options, true);
    }

    // New instance
    var obj = { type: 'color' };
    obj.options = {};

    var container = null;
    var backdrop = null;
    var content = null;
    var resetButton = null;
    var closeButton = null;
    var tabs = null;
    var jsuitesTabs = null;

    /**
     * Update options
     */
    obj.setOptions = function(options, reset) {
        /**
         * @typedef {Object} defaults
         * @property {(string|Array)} value - Initial value of the compontent
         * @property {string} placeholder - The default instruction text on the element
         * @property {requestCallback} onchange - Method to be execute after any changes on the element
         * @property {requestCallback} onclose - Method to be execute when the element is closed
         * @property {string} doneLabel - Label for button done
         * @property {string} resetLabel - Label for button reset
         * @property {string} resetValue - Value for button reset
         * @property {Bool} showResetButton - Active or note for button reset - default false
         */
        var defaults = {
            placeholder: '',
            value: null,
            onopen: null,
            onclose: null,
            onchange: null,
            closeOnChange: true,
            palette: null,
            position: null,
            doneLabel: 'Done',
            resetLabel: 'Reset',
            fullscreen: false,
            opened: false,
        }

        if (! options) {
            options = {};
        }

        if (options && ! options.palette) {
            // Default palette
            options.palette = palette();
        }

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                if (typeof(obj.options[property]) == 'undefined' || reset === true) {
                    obj.options[property] = defaults[property];
                }
            }
        }

        // Update the text of the controls, if they have already been created
        if (resetButton) {
            resetButton.innerHTML = obj.options.resetLabel;
        }
        if (closeButton) {
            closeButton.innerHTML = obj.options.doneLabel;
        }

        // Update the pallete
        if (obj.options.palette && jsuitesTabs) {
            jsuitesTabs.updateContent(0, table());
        }

        // Value
        if (typeof obj.options.value === 'string') {
            el.value = obj.options.value;
            if (el.tagName === 'INPUT') {
                el.style.color = el.value;
                el.style.backgroundColor = el.value;
            }
        }

        // Placeholder
        if (obj.options.placeholder) {
            el.setAttribute('placeholder', obj.options.placeholder);
        } else {
            if (el.getAttribute('placeholder')) {
                el.removeAttribute('placeholder');
            }
        }

        return obj;
    }

    obj.select = function(color) {
        // Remove current selected mark
        var selected = container.querySelector('.jcolor-selected');
        if (selected) {
            selected.classList.remove('jcolor-selected');
        }

        // Mark cell as selected
        if (obj.values[color]) {
            obj.values[color].classList.add('jcolor-selected');
        }

        obj.options.value = color;
    }

    /**
     * Open color pallete
     */
    obj.open = function() {
        if (! container.classList.contains('jcolor-focus')) {
            // Start tracking
            tracking(obj, true);

            // Show color picker
            container.classList.add('jcolor-focus');

            // Select current color
            if (obj.options.value) {
                obj.select(obj.options.value);
            }

            // Reset margin
            content.style.marginTop = '';
            content.style.marginLeft = '';

            var rectContent = content.getBoundingClientRect();
            var availableWidth = helpers.getWindowWidth();
            var availableHeight = helpers.getWindowHeight();

            if (availableWidth < 800 || obj.options.fullscreen == true) {
                content.classList.add('jcolor-fullscreen');
                animation.slideBottom(content, 1);
                backdrop.style.display = 'block';
            } else {
                if (content.classList.contains('jcolor-fullscreen')) {
                    content.classList.remove('jcolor-fullscreen');
                    backdrop.style.display = '';
                }

                if (obj.options.position) {
                    content.style.position = 'fixed';
                } else {
                    content.style.position = '';
                }

                if (rectContent.left + rectContent.width > availableWidth) {
                    content.style.marginLeft = -1 * (rectContent.left + rectContent.width - (availableWidth - 20)) + 'px';
                }
                if (rectContent.top + rectContent.height > availableHeight) {
                    content.style.marginTop = -1 * (rectContent.top + rectContent.height - (availableHeight - 20)) + 'px';
                }
            }


            if (typeof(obj.options.onopen) == 'function') {
                obj.options.onopen(el, obj);
            }

            jsuitesTabs.setBorder(jsuitesTabs.getActive());

            // Update sliders
            if (obj.options.value) {
                var rgb = HexToRgb(obj.options.value);

                rgbInputs.forEach(function(rgbInput, index) {
                    rgbInput.value = rgb[index];
                    rgbInput.dispatchEvent(new Event('input'));
                });
            }
        }
    }

    /**
     * Close color pallete
     */
    obj.close = function(ignoreEvents) {
        if (container.classList.contains('jcolor-focus')) {
            // Remove focus
            container.classList.remove('jcolor-focus');
            // Make sure backdrop is hidden
            backdrop.style.display = '';
            // Call related events
            if (! ignoreEvents && typeof(obj.options.onclose) == 'function') {
                obj.options.onclose(el, obj);
            }
            // Stop  the object
            tracking(obj, false);
        }

        return obj.options.value;
    }

    /**
     * Set value
     */
    obj.setValue = function(color) {
        if (! color) {
            color = '';
        }

        if (color != obj.options.value) {
            obj.options.value = color;
            slidersResult = color;

            // Remove current selecded mark
            obj.select(color);

            // Onchange
            if (typeof(obj.options.onchange) == 'function') {
                obj.options.onchange(el, color, obj);
            }

            // Changes
            if (el.value != obj.options.value) {
                // Set input value
                el.value = obj.options.value;
                if (el.tagName === 'INPUT') {
                    el.style.color = el.value;
                    el.style.backgroundColor = el.value;
                }

                // Element onchange native
                if (typeof(el.oninput) == 'function') {
                    el.oninput({
                        type: 'input',
                        target: el,
                        value: el.value
                    });
                }
            }

            if (obj.options.closeOnChange == true) {
                obj.close();
            }
        }
    }

    /**
     * Get value
     */
    obj.getValue = function() {
        return obj.options.value;
    }

    var backdropClickControl = false;

    // Converts a number in decimal to hexadecimal
    var decToHex = function(num) {
        var hex = num.toString(16);
        return hex.length === 1 ? "0" + hex : hex;
    }

    // Converts a color in rgb to hexadecimal
    var rgbToHex = function(r, g, b) {
        return "#" + decToHex(r) + decToHex(g) + decToHex(b);
    }

    // Converts a number in hexadecimal to decimal
    var hexToDec = function(hex) {
        return parseInt('0x' + hex);
    }

    // Converts a color in hexadecimal to rgb
    var HexToRgb = function(hex) {
        return [hexToDec(hex.substr(1, 2)), hexToDec(hex.substr(3, 2)), hexToDec(hex.substr(5, 2))]
    }

    var table = function() {
        // Content of the first tab
        var tableContainer = document.createElement('div');
        tableContainer.className = 'jcolor-grid';

        // Cells
        obj.values = [];

        // Table pallete
        var t = document.createElement('table');
        t.setAttribute('cellpadding', '7');
        t.setAttribute('cellspacing', '0');

        for (var j = 0; j < obj.options.palette.length; j++) {
            var tr = document.createElement('tr');
            for (var i = 0; i < obj.options.palette[j].length; i++) {
                var td = document.createElement('td');
                var color = obj.options.palette[j][i];
                if (color.length < 7 && color.substr(0,1) !== '#') {
                    color = '#' + color;
                }
                td.style.backgroundColor = color;
                td.setAttribute('data-value', color);
                td.innerHTML = '';
                tr.appendChild(td);

                // Selected color
                if (obj.options.value == color) {
                    td.classList.add('jcolor-selected');
                }

                // Possible values
                obj.values[color] = td;
            }
            t.appendChild(tr);
        }

        // Append to the table
        tableContainer.appendChild(t);

        return tableContainer;
    }

    // Canvas where the image will be rendered
    var canvas = document.createElement('canvas');
    canvas.width = 200;
    canvas.height = 160;
    var context = canvas.getContext("2d");

    var resizeCanvas = function() {
        // Specifications necessary to correctly obtain colors later in certain positions
        var m = tabs.firstChild.getBoundingClientRect();
        canvas.width = m.width - 14;
        gradient()
    }

    var gradient = function() {
        var g = context.createLinearGradient(0, 0, canvas.width, 0);
        // Create color gradient
        g.addColorStop(0,    "rgb(255,0,0)");
        g.addColorStop(0.15, "rgb(255,0,255)");
        g.addColorStop(0.33, "rgb(0,0,255)");
        g.addColorStop(0.49, "rgb(0,255,255)");
        g.addColorStop(0.67, "rgb(0,255,0)");
        g.addColorStop(0.84, "rgb(255,255,0)");
        g.addColorStop(1,    "rgb(255,0,0)");
        context.fillStyle = g;
        context.fillRect(0, 0, canvas.width, canvas.height);
        g = context.createLinearGradient(0, 0, 0, canvas.height);
        g.addColorStop(0,   "rgba(255,255,255,1)");
        g.addColorStop(0.5, "rgba(255,255,255,0)");
        g.addColorStop(0.5, "rgba(0,0,0,0)");
        g.addColorStop(1,   "rgba(0,0,0,1)");
        context.fillStyle = g;
        context.fillRect(0, 0, canvas.width, canvas.height);
    }

    var hsl = function() {
        var element = document.createElement('div');
        element.className = "jcolor-hsl";

        var point = document.createElement('div');
        point.className = 'jcolor-point';

        var div = document.createElement('div');
        div.appendChild(canvas);
        div.appendChild(point);
        element.appendChild(div);

        // Moves the marquee point to the specified position
        var update = function(buttons, x, y) {
            if (buttons === 1) {
                var rect = element.getBoundingClientRect();
                var left = x - rect.left;
                var top = y - rect.top;
                if (left < 0) {
                    left = 0;
                }
                if (top < 0) {
                    top = 0;
                }
                if (left > rect.width) {
                    left = rect.width;
                }
                if (top > rect.height) {
                    top = rect.height;
                }
                point.style.left = left + 'px';
                point.style.top = top + 'px';
                var pixel = context.getImageData(left, top, 1, 1).data;
                slidersResult = rgbToHex(pixel[0], pixel[1], pixel[2]);
            }
        }

        // Applies the point's motion function to the div that contains it
        element.addEventListener('mousedown', function(e) {
            update(e.buttons, e.clientX, e.clientY);
        });

        element.addEventListener('mousemove', function(e) {
            update(e.buttons, e.clientX, e.clientY);
        });

        element.addEventListener('touchmove', function(e) {
            update(1, e.changedTouches[0].clientX, e.changedTouches[0].clientY);
        });

        return element;
    }

    var slidersResult = '';

    var rgbInputs = [];

    var changeInputColors = function() {
        if (slidersResult !== '') {
            for (var j = 0; j < rgbInputs.length; j++) {
                var currentColor = HexToRgb(slidersResult);

                currentColor[j] = 0;

                var newGradient = 'linear-gradient(90deg, rgb(';
                newGradient += currentColor.join(', ');
                newGradient += '), rgb(';

                currentColor[j] = 255;

                newGradient += currentColor.join(', ');
                newGradient += '))';

                rgbInputs[j].style.backgroundImage = newGradient;
            }
        }
    }

    var sliders = function() {
        // Content of the third tab
        var slidersElement = document.createElement('div');
        slidersElement.className = 'jcolor-sliders';

        var slidersBody = document.createElement('div');

        // Creates a range-type input with the specified name
        var createSliderInput = function(name) {
            var inputContainer = document.createElement('div');
            inputContainer.className = 'jcolor-sliders-input-container';

            var label = document.createElement('label');
            label.innerText = name;

            var subContainer = document.createElement('div');
            subContainer.className = 'jcolor-sliders-input-subcontainer';

            var input = document.createElement('input');
            input.type = 'range';
            input.min = 0;
            input.max = 255;
            input.value = 0;

            input.setAttribute('aria-label', "Color value");
            input.setAttribute('aria-valuemin', "0");
            input.setAttribute('aria-valuemax', "255");
            input.setAttribute('aria-valuenow', "0");

            inputContainer.appendChild(label);
            subContainer.appendChild(input);

            var value = document.createElement('div');
            value.innerText = input.value;

            input.addEventListener('input', function() {
                value.innerText = input.value;
            });

            subContainer.appendChild(value);
            inputContainer.appendChild(subContainer);

            slidersBody.appendChild(inputContainer);

            return input;
        }

        // Creates red, green and blue inputs
        rgbInputs = [
            createSliderInput('Red'),
            createSliderInput('Green'),
            createSliderInput('Blue'),
        ];

        slidersElement.appendChild(slidersBody);

        // Element that prints the current color
        var slidersResultColor = document.createElement('div');
        slidersResultColor.className = 'jcolor-sliders-final-color';

        var resultElement = document.createElement('div');
        resultElement.style.visibility = 'hidden';
        resultElement.innerText = 'a';
        slidersResultColor.appendChild(resultElement)

        // Update the element that prints the current color
        var updateResult = function() {
            var resultColor = rgbToHex(parseInt(rgbInputs[0].value), parseInt(rgbInputs[1].value), parseInt(rgbInputs[2].value));

            resultElement.innerText = resultColor;
            resultElement.style.color = resultColor;
            resultElement.style.removeProperty('visibility');

            slidersResult = resultColor;
        }

        // Apply the update function to color inputs
        rgbInputs.forEach(function(rgbInput) {
            rgbInput.addEventListener('input', function() {
                updateResult();
                changeInputColors();
            });
        });

        slidersElement.appendChild(slidersResultColor);

        return slidersElement;
    }

    var init = function() {
        // Initial options
        obj.setOptions(options);

        // Add a proper input tag when the element is an input
        if (el.tagName == 'INPUT') {
            el.classList.add('jcolor-input');
            el.readOnly = true;
        }

        // Table container
        container = document.createElement('div');
        container.className = 'jcolor';

        // Table container
        backdrop = document.createElement('div');
        backdrop.className = 'jcolor-backdrop';
        container.appendChild(backdrop);

        // Content
        content = document.createElement('div');
        content.className = 'jcolor-content';

        // Controls
        var controls = document.createElement('div');
        controls.className = 'jcolor-controls';
        content.appendChild(controls);

        // Reset button
        resetButton  = document.createElement('div');
        resetButton.className = 'jcolor-reset';
        resetButton.innerHTML = obj.options.resetLabel;
        controls.appendChild(resetButton);

        // Close button
        closeButton  = document.createElement('div');
        closeButton.className = 'jcolor-close';
        closeButton.innerHTML = obj.options.doneLabel;
        controls.appendChild(closeButton);

        // Element that will be used to create the tabs
        tabs = document.createElement('div');
        content.appendChild(tabs);

        // Starts the jSuites tabs component
        jsuitesTabs = Tabs(tabs, {
            animation: true,
            data: [
                {
                    title: 'Grid',
                    contentElement: table(),
                },
                {
                    title: 'Spectrum',
                    contentElement: hsl(),
                },
                {
                    title: 'Sliders',
                    contentElement: sliders(),
                }
            ],
            onchange: function(element, instance, index) {
                if (index === 1) {
                    resizeCanvas();
                } else {
                    var color = slidersResult !== '' ? slidersResult : obj.getValue();

                    if (index === 2 && color) {
                        var rgb = HexToRgb(color);

                        rgbInputs.forEach(function(rgbInput, index) {
                            rgbInput.value = rgb[index];
                            rgbInput.dispatchEvent(new Event('input'));
                        });
                    }
                }
            },
            palette: 'modern',
        });

        container.appendChild(content);

        // Insert picker after the element
        if (el.tagName == 'INPUT') {
            el.parentNode.insertBefore(container, el.nextSibling);
        } else {
            el.appendChild(container);
        }

        container.addEventListener("click", function(e) {
            if (e.target.tagName == 'TD') {
                var value = e.target.getAttribute('data-value');
                if (value) {
                    obj.setValue(value);
                }
            } else if (e.target.classList.contains('jcolor-reset')) {
                obj.setValue('');
                obj.close();
            } else if (e.target.classList.contains('jcolor-close')) {
                if (jsuitesTabs.getActive() > 0) {
                    obj.setValue(slidersResult);
                }
                obj.close();
            } else if (e.target.classList.contains('jcolor-backdrop')) {
                obj.close();
            } else {
                obj.open();
            }
        });

        /**
         * If element is focus open the picker
         */
        el.addEventListener("mouseup", function(e) {
            obj.open();
        });

        // If the picker is open on the spectrum tab, it changes the canvas size when the window size is changed
        window.addEventListener('resize', function() {
            if (container.classList.contains('jcolor-focus') && jsuitesTabs.getActive() == 1) {
                resizeCanvas();
            }
        });

        // Default opened
        if (obj.options.opened == true) {
            obj.open();
        }

        // Change
        el.change = obj.setValue;

        // Global generic value handler
        el.val = function(val) {
            if (val === undefined) {
                return obj.getValue();
            } else {
                obj.setValue(val);
            }
        }

        // Keep object available from the node
        el.color = obj;

        // Container shortcut
        container.color = obj;
    }

    obj.toHex = function(rgb) {
        var hex = function(x) {
            return ("0" + parseInt(x).toString(16)).slice(-2);
        }
        if (rgb) {
            if (/^#[0-9A-F]{6}$/i.test(rgb)) {
                return rgb;
            } else {
                rgb = rgb.match(/^rgb\((\d+),\s*(\d+),\s*(\d+)\)$/);
                if (rgb && rgb.length) {
                    return "#" + hex(rgb[1]) + hex(rgb[2]) + hex(rgb[3]);
                } else {
                    return "";
                }
            }
        }
    }

    init();

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/contextmenu.js



function Contextmenu() {

    var Component = function(el, options) {
        // New instance
        var obj = {type: 'contextmenu'};
        obj.options = {};

        // Default configuration
        var defaults = {
            items: null,
            onclick: null,
        };

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        // Class definition
        el.classList.add('jcontextmenu');

        /**
         * Open contextmenu
         */
        obj.open = function (e, items) {
            if (items) {
                // Update content
                obj.options.items = items;
                // Create items
                obj.create(items);
            }

            // Close current contextmenu
            if (Component.current) {
                Component.current.close();
            }

            // Add to the opened components monitor
            tracking(obj, true);

            // Show context menu
            el.classList.add('jcontextmenu-focus');

            // Current
            Component.current = obj;

            // Coordinates
            if ((obj.options.items && obj.options.items.length > 0) || el.children.length) {
                if (e.target) {
                    if (e.changedTouches && e.changedTouches[0]) {
                        x = e.changedTouches[0].clientX;
                        y = e.changedTouches[0].clientY;
                    } else {
                        var x = e.clientX;
                        var y = e.clientY;
                    }
                } else {
                    var x = e.x;
                    var y = e.y;
                }

                var rect = el.getBoundingClientRect();

                if (window.innerHeight < y + rect.height) {
                    var h = y - rect.height;
                    if (h < 0) {
                        h = 0;
                    }
                    el.style.top = h + 'px';
                } else {
                    el.style.top = y + 'px';
                }

                if (window.innerWidth < x + rect.width) {
                    if (x - rect.width > 0) {
                        el.style.left = (x - rect.width) + 'px';
                    } else {
                        el.style.left = '10px';
                    }
                } else {
                    el.style.left = x + 'px';
                }
            }
        }

        obj.isOpened = function () {
            return el.classList.contains('jcontextmenu-focus') ? true : false;
        }

        /**
         * Close menu
         */
        obj.close = function () {
            if (el.classList.contains('jcontextmenu-focus')) {
                el.classList.remove('jcontextmenu-focus');
            }
            tracking(obj, false);
        }

        /**
         * Create items based on the declared objectd
         * @param {object} items - List of object
         */
        obj.create = function (items) {
            // Update content
            el.innerHTML = '';

            // Add header contextmenu
            var itemHeader = createHeader();
            el.appendChild(itemHeader);

            // Append items
            for (var i = 0; i < items.length; i++) {
                var itemContainer = createItemElement(items[i]);
                el.appendChild(itemContainer);
            }
        }

        /**
         * createHeader for context menu
         * @private
         * @returns {HTMLElement}
         */
        function createHeader() {
            var header = document.createElement('div');
            header.classList.add("header");
            header.addEventListener("click", function (e) {
                e.preventDefault();
                e.stopPropagation();
            });
            var title = document.createElement('a');
            title.classList.add("title");
            title.innerHTML = dictionary.translate("Menu");

            header.appendChild(title);

            var closeButton = document.createElement('a');
            closeButton.classList.add("close");
            closeButton.innerHTML = dictionary.translate("close");
            closeButton.addEventListener("click", function (e) {
                obj.close();
            });

            header.appendChild(closeButton);

            return header;
        }

        /**
         * Private function for create a new Item element
         * @param {type} item
         * @returns {jsuitesL#15.jSuites.contextmenu.createItemElement.itemContainer}
         */
        function createItemElement(item) {
            if (item.type && (item.type == 'line' || item.type == 'divisor')) {
                var itemContainer = document.createElement('hr');
            } else {
                var itemContainer = document.createElement('div');
                var itemText = document.createElement('a');
                itemText.innerHTML = item.title;

                if (item.tooltip) {
                    itemContainer.setAttribute('title', item.tooltip);
                }

                if (item.icon) {
                    itemContainer.setAttribute('data-icon', item.icon);
                }

                if (item.id) {
                    itemContainer.id = item.id;
                }

                if (item.disabled) {
                    itemContainer.className = 'jcontextmenu-disabled';
                } else if (item.onclick) {
                    let method = item.onclick;
                    itemContainer.addEventListener("mousedown", function (e) {
                        e.preventDefault();
                    });
                    itemContainer.addEventListener("mouseup", function (e) {
                        method(this, e);
                    });
                }
                itemContainer.appendChild(itemText);

                if (item.submenu) {
                    var itemIconSubmenu = document.createElement('span');
                    itemIconSubmenu.innerHTML = "&#9658;";
                    itemContainer.appendChild(itemIconSubmenu);
                    itemContainer.classList.add('jcontexthassubmenu');
                    var el_submenu = document.createElement('div');
                    // Class definition
                    el_submenu.classList.add('jcontextmenu');
                    // Focusable
                    el_submenu.setAttribute('tabindex', '900');

                    // Append items
                    var submenu = item.submenu;
                    for (var i = 0; i < submenu.length; i++) {
                        var itemContainerSubMenu = createItemElement(submenu[i]);
                        el_submenu.appendChild(itemContainerSubMenu);
                    }

                    itemContainer.appendChild(el_submenu);

                    // Submenu positioning logic:
                    // Case 1: Default (enough space to the right) - submenu opens to the right of the parent menu item.
                    // Case 2: Not enough space to the right, but enough to the left - submenu opens to the left of the parent menu item.
                    // Case 3: Not enough space on either side (e.g., very narrow viewport) - submenu opens below the parent menu item.
                    itemContainer.addEventListener('mouseenter', function () {
                        // Reset to default
                        el_submenu.style.left = '';
                        el_submenu.style.right = '';
                        el_submenu.style.minWidth = itemContainer.offsetWidth + 'px';

                        // Temporarily show submenu to measure
                        el_submenu.style.display = 'block';
                        el_submenu.style.opacity = '0';
                        el_submenu.style.pointerEvents = 'none';

                        // Use getBoundingClientRect to determine position
                        var parentRect = itemContainer.getBoundingClientRect();
                        var submenuRect = el_submenu.getBoundingClientRect();
                        var viewportWidth = window.innerWidth || document.documentElement.clientWidth;

                        // Calculate the right edge if rendered to the right
                        var rightEdge = parentRect.right + submenuRect.width;
                        var leftEdge = parentRect.left - submenuRect.width;

                        // If rendering to the right would overflow, render to the left
                        if (rightEdge > viewportWidth && leftEdge >= 0) {
                            el_submenu.style.left = 'auto';
                            el_submenu.style.right = '99%';
                        } 
                        // If both right and left would overflow, render to the right of the left border (worst case)
                        else if (rightEdge > viewportWidth && leftEdge < 0) {
                            el_submenu.style.left = '32px';
                            el_submenu.style.right = 'auto';
                            el_submenu.style.top = '100%';
                        }
                        // Default: render to the right
                        else {
                            el_submenu.style.left = '99%';
                            el_submenu.style.right = 'auto';
                        }

                        // Restore visibility
                        el_submenu.style.opacity = '';
                        el_submenu.style.pointerEvents = '';
                        el_submenu.style.display = '';
                    });

                    // Also reset submenu position on mouseleave to avoid stale styles
                    itemContainer.addEventListener('mouseleave', function () {
                        el_submenu.style.left = '';
                        el_submenu.style.right = '';
                        el_submenu.style.minWidth = '';
                    });
                } else if (item.shortcut) {
                    var itemShortCut = document.createElement('span');
                    itemShortCut.innerHTML = item.shortcut;
                    itemContainer.appendChild(itemShortCut);
                }
            }
            return itemContainer;
        }

        if (typeof (obj.options.onclick) == 'function') {
            el.addEventListener('click', function (e) {
                obj.options.onclick(obj, e);
            });
        }

        // Create items
        if (obj.options.items) {
            obj.create(obj.options.items);
        }

        window.addEventListener("mousewheel", function () {
            obj.close();
        });

        el.contextmenu = obj;

        return obj;
    }

    return Component;
}

/* harmony default export */ var contextmenu = (Contextmenu());
;// CONCATENATED MODULE: ./src/plugins/dropdown.js







function Dropdown() {

    var Component = (function (el, options) {
        // Already created, update options
        if (el.dropdown) {
            return el.dropdown.setOptions(options, true);
        }

        // New instance
        var obj = {type: 'dropdown'};
        obj.options = {};

        // Success
        var success = function (data, val) {
            // Set data
            if (data && data.length) {
                // Sort
                if (obj.options.sortResults !== false) {
                    if (typeof obj.options.sortResults == "function") {
                        data.sort(obj.options.sortResults);
                    } else {
                        data.sort(sortData);
                    }
                }

                obj.setData(data);
            }

            // Onload method
            if (typeof (obj.options.onload) == 'function') {
                obj.options.onload(el, obj, data, val);
            }

            // Set value
            if (val) {
                applyValue(val);
            }

            // Component value
            if (val === undefined || val === null) {
                obj.options.value = '';
            }
            el.value = obj.options.value;

            // Open dropdown
            if (obj.options.opened == true) {
                obj.open();
            }
        }


        // Default sort
        var sortData = function (itemA, itemB) {
            var testA, testB;
            if (typeof itemA == "string") {
                testA = itemA;
            } else {
                if (itemA.text) {
                    testA = itemA.text;
                } else if (itemA.name) {
                    testA = itemA.name;
                }
            }

            if (typeof itemB == "string") {
                testB = itemB;
            } else {
                if (itemB.text) {
                    testB = itemB.text;
                } else if (itemB.name) {
                    testB = itemB.name;
                }
            }

            if (typeof testA == "string" || typeof testB == "string") {
                if (typeof testA != "string") {
                    testA = "" + testA;
                }
                if (typeof testB != "string") {
                    testB = "" + testB;
                }
                return testA.localeCompare(testB);
            } else {
                return testA - testB;
            }
        }

        /**
         * Reset the options for the dropdown
         */
        var resetValue = function () {
            // Reset value container
            obj.value = {};
            // Remove selected
            for (var i = 0; i < obj.items.length; i++) {
                if (obj.items[i].selected == true) {
                    if (obj.items[i].element) {
                        obj.items[i].element.classList.remove('jdropdown-selected')
                    }
                    obj.items[i].selected = null;
                }
            }
            // Reset options
            obj.options.value = '';
            // Reset value
            el.value = '';
        }

        /**
         * Apply values to the dropdown
         */
        var applyValue = function (values) {
            // Reset the current values
            resetValue();

            // Read values
            if (values !== null) {
                if (!values) {
                    if (typeof (obj.value['']) !== 'undefined') {
                        obj.value[''] = '';
                    }
                } else {
                    if (!Array.isArray(values)) {
                        values = ('' + values).split(';');
                    }
                    for (var i = 0; i < values.length; i++) {
                        obj.value[values[i]] = '';
                    }
                }
            }

            // Update the DOM
            for (var i = 0; i < obj.items.length; i++) {
                if (typeof (obj.value[Value(i)]) !== 'undefined') {
                    if (obj.items[i].element) {
                        obj.items[i].element.classList.add('jdropdown-selected')
                    }
                    obj.items[i].selected = true;

                    // Keep label
                    obj.value[Value(i)] = Text(i);
                }
            }

            // Global value
            obj.options.value = Object.keys(obj.value).join(';');

            // Update labels
            obj.header.value = obj.getText();
        }

        // Get the value of one item
        var Value = function (k, v) {
            // Legacy purposes
            if (!obj.options.format) {
                var property = 'value';
            } else {
                var property = 'id';
            }

            if (obj.items[k]) {
                if (v !== undefined) {
                    return obj.items[k].data[property] = v;
                } else {
                    return obj.items[k].data[property];
                }
            }

            return '';
        }

        // Get the label of one item
        var Text = function (k, v) {
            // Legacy purposes
            if (!obj.options.format) {
                var property = 'text';
            } else {
                var property = 'name';
            }

            if (obj.items[k]) {
                if (v !== undefined) {
                    return obj.items[k].data[property] = v;
                } else {
                    return obj.items[k].data[property];
                }
            }

            return '';
        }

        var getValue = function () {
            return Object.keys(obj.value);
        }

        var getText = function () {
            var data = [];
            var k = Object.keys(obj.value);
            for (var i = 0; i < k.length; i++) {
                data.push(obj.value[k[i]]);
            }
            return data;
        }

        obj.setOptions = function (options, reset) {
            if (!options) {
                options = {};
            }

            // Default configuration
            var defaults = {
                url: null,
                data: [],
                format: 0,
                multiple: false,
                autocomplete: false,
                remoteSearch: false,
                lazyLoading: false,
                type: null,
                width: null,
                maxWidth: null,
                opened: false,
                value: null,
                placeholder: '',
                newOptions: false,
                position: false,
                onchange: null,
                onload: null,
                onopen: null,
                onclose: null,
                onfocus: null,
                onblur: null,
                oninsert: null,
                onbeforeinsert: null,
                onsearch: null,
                onbeforesearch: null,
                sortResults: false,
                autofocus: false,
                prompt: null,
                allowEmpty: true,
            }

            // Loop through our object
            for (var property in defaults) {
                if (options && options.hasOwnProperty(property)) {
                    obj.options[property] = options[property];
                } else {
                    if (typeof (obj.options[property]) == 'undefined' || reset === true) {
                        obj.options[property] = defaults[property];
                    }
                }
            }

            // Force autocomplete search
            if (obj.options.remoteSearch == true || obj.options.type === 'searchbar') {
                obj.options.autocomplete = true;
            }

            // New options
            if (obj.options.newOptions == true) {
                obj.header.classList.add('jdropdown-add');
            } else {
                obj.header.classList.remove('jdropdown-add');
            }

            // Autocomplete
            if (obj.options.autocomplete == true) {
                obj.header.removeAttribute('readonly');
            } else {
                obj.header.setAttribute('readonly', 'readonly');
            }

            // Place holder
            if (obj.options.placeholder) {
                obj.header.setAttribute('placeholder', obj.options.placeholder);
            } else {
                obj.header.removeAttribute('placeholder');
            }

            // Remove specific dropdown typing to add again
            el.classList.remove('jdropdown-searchbar');
            el.classList.remove('jdropdown-picker');
            el.classList.remove('jdropdown-list');

            if (obj.options.type == 'searchbar') {
                el.classList.add('jdropdown-searchbar');
            } else if (obj.options.type == 'list') {
                el.classList.add('jdropdown-list');
            } else if (obj.options.type == 'picker') {
                el.classList.add('jdropdown-picker');
            } else {
                if (helpers.getWindowWidth() < 800) {
                    if (obj.options.autocomplete) {
                        el.classList.add('jdropdown-searchbar');
                        obj.options.type = 'searchbar';
                    } else {
                        el.classList.add('jdropdown-picker');
                        obj.options.type = 'picker';
                    }
                } else {
                    if (obj.options.width) {
                        el.style.width = obj.options.width;
                        el.style.minWidth = obj.options.width;
                    } else {
                        el.style.removeProperty('width');
                        el.style.removeProperty('min-width');
                    }

                    el.classList.add('jdropdown-default');
                    obj.options.type = 'default';
                }
            }

            // Close button
            if (obj.options.type == 'searchbar') {
                containerHeader.appendChild(closeButton);
            } else {
                container.insertBefore(closeButton, container.firstChild);
            }

            // Load the content
            if (obj.options.url && !options.data) {
                ajax({
                    url: obj.options.url,
                    method: 'GET',
                    dataType: 'json',
                    success: function (data) {
                        if (data) {
                            success(data, obj.options.value);
                        }
                    }
                });
            } else {
                success(obj.options.data, obj.options.value);
            }

            // Return the instance
            return obj;
        }

        // Helpers
        var containerHeader = null;
        var container = null;
        var content = null;
        var closeButton = null;
        var resetButton = null;
        var backdrop = null;

        var keyTimer = null;

        /**
         * Init dropdown
         */
        var init = function () {
            // Do not accept null
            if (!options) {
                options = {};
            }

            // If the element is a SELECT tag, create a configuration object
            if (el.tagName == 'SELECT') {
                var ret = Component.extractFromDom(el, options);
                el = ret.el;
                options = ret.options;
            }

            // Place holder
            if (!options.placeholder && el.getAttribute('placeholder')) {
                options.placeholder = el.getAttribute('placeholder');
            }

            // Value container
            obj.value = {};
            // Containers
            obj.items = [];
            obj.groups = [];
            // Search options
            obj.search = '';
            obj.results = null;

            // Create dropdown
            el.classList.add('jdropdown');

            // Header container
            containerHeader = document.createElement('div');
            containerHeader.className = 'jdropdown-container-header';

            // Header
            obj.header = document.createElement('input');
            obj.header.className = 'jdropdown-header jss_object';
            obj.header.type = 'text';
            obj.header.setAttribute('autocomplete', 'off');
            obj.header.onfocus = function () {
                if (typeof (obj.options.onfocus) == 'function') {
                    obj.options.onfocus(el);
                }
            }

            obj.header.onblur = function () {
                if (typeof (obj.options.onblur) == 'function') {
                    obj.options.onblur(el);
                }
            }

            obj.header.onkeyup = function (e) {
                if (obj.options.autocomplete == true && !keyTimer) {
                    if (obj.search != obj.header.value.trim()) {
                        keyTimer = setTimeout(function () {
                            obj.find(obj.header.value.trim());
                            keyTimer = null;
                        }, 400);
                    }

                    if (!el.classList.contains('jdropdown-focus')) {
                        obj.open();
                    }
                } else {
                    if (!obj.options.autocomplete) {
                        obj.next(e.key);
                    }
                }
            }

            // Global controls
            if (!Component.hasEvents) {
                // Execute only one time
                Component.hasEvents = true;
                // Enter and Esc
                document.addEventListener("keydown", Component.keydown);
            }

            // Container
            container = document.createElement('div');
            container.className = 'jdropdown-container';

            // Dropdown content
            content = document.createElement('div');
            content.className = 'jdropdown-content';

            // Close button
            closeButton = document.createElement('div');
            closeButton.className = 'jdropdown-close';
            closeButton.textContent = 'Done';

            // Reset button
            resetButton = document.createElement('div');
            resetButton.className = 'jdropdown-reset';
            resetButton.textContent = 'x';
            resetButton.onclick = function () {
                obj.reset();
                obj.close();
            }

            // Create backdrop
            backdrop = document.createElement('div');
            backdrop.className = 'jdropdown-backdrop';

            // Append elements
            containerHeader.appendChild(obj.header);

            container.appendChild(content);
            el.appendChild(containerHeader);
            el.appendChild(container);
            el.appendChild(backdrop);

            // Set the otiptions
            obj.setOptions(options);

            if ('ontouchsend' in document.documentElement === true) {
                el.addEventListener('touchsend', Component.mouseup);
            } else {
                el.addEventListener('mouseup', Component.mouseup);
            }

            // Lazyloading
            if (obj.options.lazyLoading == true) {
                LazyLoading(content, {
                    loadUp: obj.loadUp,
                    loadDown: obj.loadDown,
                });
            }

            content.onwheel = function (e) {
                e.stopPropagation();
            }

            // Change method
            el.change = obj.setValue;

            // Global generic value handler
            el.val = function (val) {
                if (val === undefined) {
                    return obj.getValue(obj.options.multiple ? true : false);
                } else {
                    obj.setValue(val);
                }
            }

            // Keep object available from the node
            el.dropdown = obj;
        }

        /**
         * Get the current remote source of data URL
         */
        obj.getUrl = function () {
            return obj.options.url;
        }

        /**
         * Set the new data from a remote source
         * @param {string} url - url from the remote source
         * @param {function} callback - callback when the data is loaded
         */
        obj.setUrl = function (url, callback) {
            obj.options.url = url;

            ajax({
                url: obj.options.url,
                method: 'GET',
                dataType: 'json',
                success: function (data) {
                    obj.setData(data);
                    // Callback
                    if (typeof (callback) == 'function') {
                        callback(obj);
                    }
                }
            });
        }

        /**
         * Set ID for one item
         */
        obj.setId = function (item, v) {
            // Legacy purposes
            if (!obj.options.format) {
                var property = 'value';
            } else {
                var property = 'id';
            }

            if (typeof (item) == 'object') {
                item[property] = v;
            } else {
                obj.items[item].data[property] = v;
            }
        }

        const add = function(title, id) {
            if (! title) {
                let current = obj.options.autocomplete == true ? obj.header.value : '';
                title = prompt(dictionary.translate('Add A New Option'), current);
                if (! title) {
                    return false;
                }
            }

            // Id
            if (! id) {
                id = helpers.guid();
            }

            // Create new item
            if (!obj.options.format) {
                var item = {
                    value: id,
                    text: title,
                }
            } else {
                var item = {
                    id: id,
                    name: title,
                }
            }

            // Callback
            if (typeof (obj.options.onbeforeinsert) == 'function') {
                let ret = obj.options.onbeforeinsert(obj, item);
                if (ret === false) {
                    return false;
                } else if (ret) {
                    item = ret;
                }
            }

            // Add item to the main list
            obj.options.data.push(item);

            // Create DOM
            var newItem = obj.createItem(item);

            // Append DOM to the list
            content.appendChild(newItem.element);

            // Callback
            if (typeof (obj.options.oninsert) == 'function') {
                obj.options.oninsert(obj, item, newItem);
            }

            // Show content
            if (content.style.display == 'none') {
                content.style.display = '';
            }

            // Search?
            if (obj.results) {
                obj.results.push(newItem);
            }

            return item;
        }

        /**
         * Add a new item
         * @param {string} title - title of the new item
         * @param {string} id - value/id of the new item
         */
        obj.add = function (title, id) {
            if (typeof (obj.options.prompt) == 'function') {
                return obj.options.prompt.call(obj, add);
            }
            return add(title, id);
        }

        /**
         * Create a new item
         */
        obj.createItem = function (data, group, groupName) {
            // Keep the correct source of data
            if (!obj.options.format) {
                if (!data.value && data.id !== undefined) {
                    data.value = data.id;
                    //delete data.id;
                }
                if (!data.text && data.name !== undefined) {
                    data.text = data.name;
                    //delete data.name;
                }
            } else {
                if (!data.id && data.value !== undefined) {
                    data.id = data.value;
                    //delete data.value;
                }
                if (!data.name && data.text !== undefined) {
                    data.name = data.text
                    //delete data.text;
                }
            }

            // Create item
            var item = {};
            item.element = document.createElement('div');
            item.element.className = 'jdropdown-item';
            item.element.indexValue = obj.items.length;
            item.data = data;

            // Groupd DOM
            if (group) {
                item.group = group;
            }

            // Id
            if (data.id) {
                item.element.setAttribute('id', data.id);
            }

            // Disabled
            if (data.disabled == true) {
                item.element.setAttribute('data-disabled', true);
            }

            // Tooltip
            if (data.tooltip) {
                item.element.setAttribute('title', data.tooltip);
            }

            // Image
            if (data.image) {
                var image = document.createElement('img');
                image.className = 'jdropdown-image';
                image.src = data.image;
                if (!data.title) {
                    image.classList.add('jdropdown-image-small');
                }
                item.element.appendChild(image);
            } else if (data.icon) {
                var icon = document.createElement('span');
                icon.className = "jdropdown-icon material-icons";
                icon.innerText = data.icon;
                if (!data.title) {
                    icon.classList.add('jdropdown-icon-small');
                }
                if (data.color) {
                    icon.style.color = data.color;
                }
                item.element.appendChild(icon);
            } else if (data.color) {
                var color = document.createElement('div');
                color.className = 'jdropdown-color';
                color.style.backgroundColor = data.color;
                item.element.appendChild(color);
            }

            // Set content
            if (!obj.options.format) {
                var text = data.text;
            } else {
                var text = data.name;
            }

            var node = document.createElement('div');
            node.className = 'jdropdown-description';
            node.textContent = text || '&nbsp;';

            // Title
            if (data.title) {
                var title = document.createElement('div');
                title.className = 'jdropdown-title';
                title.innerText = data.title;
                node.appendChild(title);
            }

            // Set content
            if (!obj.options.format) {
                var val = data.value;
            } else {
                var val = data.id;
            }

            // Value
            if (obj.value[val]) {
                item.element.classList.add('jdropdown-selected');
                item.selected = true;
            }

            // Keep DOM accessible
            obj.items.push(item);

            // Add node to item
            item.element.appendChild(node);

            return item;
        }

        obj.appendData = function (data) {
            // Create elements
            if (data.length) {
                // Helpers
                var items = [];
                var groups = [];

                // Prepare data
                for (var i = 0; i < data.length; i++) {
                    // Process groups
                    if (data[i].group) {
                        if (!groups[data[i].group]) {
                            groups[data[i].group] = [];
                        }
                        groups[data[i].group].push(i);
                    } else {
                        items.push(i);
                    }
                }

                // Number of items counter
                var counter = 0;

                // Groups
                var groupNames = Object.keys(groups);

                // Append groups in case exists
                if (groupNames.length > 0) {
                    for (var i = 0; i < groupNames.length; i++) {
                        // Group container
                        var group = document.createElement('div');
                        group.className = 'jdropdown-group';
                        // Group name
                        var groupName = document.createElement('div');
                        groupName.className = 'jdropdown-group-name';
                        groupName.textContent = groupNames[i];
                        // Group arrow
                        var groupArrow = document.createElement('i');
                        groupArrow.className = 'jdropdown-group-arrow jdropdown-group-arrow-down';
                        groupName.appendChild(groupArrow);
                        // Group items
                        var groupContent = document.createElement('div');
                        groupContent.className = 'jdropdown-group-items';
                        for (var j = 0; j < groups[groupNames[i]].length; j++) {
                            var item = obj.createItem(data[groups[groupNames[i]][j]], group, groupNames[i]);

                            if (obj.options.lazyLoading == false || counter < 200) {
                                groupContent.appendChild(item.element);
                                counter++;
                            }
                        }
                        // Group itens
                        group.appendChild(groupName);
                        group.appendChild(groupContent);
                        // Keep group DOM
                        obj.groups.push(group);
                        // Only add to the screen if children on the group
                        if (groupContent.children.length > 0) {
                            // Add DOM to the content
                            content.appendChild(group);
                        }
                    }
                }

                if (items.length) {
                    for (var i = 0; i < items.length; i++) {
                        var item = obj.createItem(data[items[i]]);
                        if (obj.options.lazyLoading == false || counter < 200) {
                            content.appendChild(item.element);
                            counter++;
                        }
                    }
                }
            }
        }

        obj.setData = function (data) {
            // Reset current value
            resetValue();

            // Make sure the content container is blank
            content.textContent = '';

            // Reset
            obj.header.value = '';

            // Reset items and values
            obj.items = [];

            // Prepare data
            if (data && data.length) {
                for (var i = 0; i < data.length; i++) {
                    // Compatibility
                    if (typeof (data[i]) != 'object') {
                        // Correct format
                        if (!obj.options.format) {
                            data[i] = {
                                value: data[i],
                                text: data[i]
                            }
                        } else {
                            data[i] = {
                                id: data[i],
                                name: data[i]
                            }
                        }
                    }
                }

                // Append data
                obj.appendData(data);

                // Update data
                obj.options.data = data;
            } else {
                // Update data
                obj.options.data = [];
            }

            obj.close();
        }

        obj.getData = function () {
            return obj.options.data;
        }

        /**
         * Get position of the item
         */
        obj.getPosition = function (val) {
            for (var i = 0; i < obj.items.length; i++) {
                if (Value(i) == val) {
                    return i;
                }
            }
            return false;
        }

        /**
         * Get dropdown current text
         */
        obj.getText = function (asArray) {
            // Get value
            var v = getText();
            // Return value
            if (asArray) {
                return v;
            } else {
                return v.join('; ');
            }
        }

        /**
         * Get dropdown current value
         */
        obj.getValue = function (asArray) {
            // Get value
            var v = getValue();
            // Return value
            if (asArray) {
                return v;
            } else {
                return v.join(';');
            }
        }

        /**
         * Change event
         */
        var change = function (oldValue) {
            // Lemonade JS
            if (el.value != obj.options.value) {
                el.value = obj.options.value;
                if (typeof (el.oninput) == 'function') {
                    el.oninput({
                        type: 'input',
                        target: el,
                        value: el.value
                    });
                }
            }

            // Events
            if (typeof (obj.options.onchange) == 'function') {
                obj.options.onchange(el, obj, oldValue, obj.options.value);
            }
        }

        /**
         * Set value
         */
        obj.setValue = function (newValue) {
            // Current value
            var oldValue = obj.getValue();
            // New value
            if (Array.isArray(newValue)) {
                newValue = newValue.join(';')
            }

            if (oldValue !== newValue) {
                // Set value
                applyValue(newValue);

                // Change
                change(oldValue);
            }
        }

        obj.resetSelected = function () {
            obj.setValue(null);
        }

        obj.selectIndex = function (index, force) {
            // Make sure is a number
            var index = parseInt(index);

            // Only select those existing elements
            if (obj.items && obj.items[index] && (force === true || obj.items[index].data.disabled !== true)) {
                // Reset cursor to a new position
                obj.setCursor(index, false);

                // Behaviour
                if (!obj.options.multiple) {
                    // Update value
                    if (obj.items[index].selected) {
                        if (obj.options.allowEmpty !== false) {
                            obj.setValue(null);
                        }
                    } else {
                        obj.setValue(Value(index));
                    }

                    // Close component
                    obj.close();
                } else {
                    // Old value
                    var oldValue = obj.options.value;

                    // Toggle option
                    if (obj.items[index].selected) {
                        obj.items[index].element.classList.remove('jdropdown-selected');
                        obj.items[index].selected = false;

                        delete obj.value[Value(index)];
                    } else {
                        // Select element
                        obj.items[index].element.classList.add('jdropdown-selected');
                        obj.items[index].selected = true;

                        // Set value
                        obj.value[Value(index)] = Text(index);
                    }

                    // Global value
                    obj.options.value = Object.keys(obj.value).join(';');

                    // Update labels for multiple dropdown
                    if (obj.options.autocomplete == false) {
                        obj.header.value = getText().join('; ');
                    }

                    // Events
                    change(oldValue);
                }
            }
        }

        obj.selectItem = function (item) {
            obj.selectIndex(item.indexValue);
        }

        var exists = function (k, result) {
            for (var j = 0; j < result.length; j++) {
                if (!obj.options.format) {
                    if (result[j].value == k) {
                        return true;
                    }
                } else {
                    if (result[j].id == k) {
                        return true;
                    }
                }
            }
            return false;
        }

        obj.find = function (str) {
            if (obj.search == str.trim()) {
                return false;
            }

            // Search term
            obj.search = str;

            // Reset index
            obj.setCursor();

            // Remove nodes from all groups
            if (obj.groups.length) {
                for (var i = 0; i < obj.groups.length; i++) {
                    obj.groups[i].lastChild.textContent = '';
                }
            }

            // Remove all nodes
            content.textContent = '';

            // Remove current items in the remote search
            if (obj.options.remoteSearch == true) {
                // Reset results
                obj.results = null;
                // URL
                var url = obj.options.url;

                // Ajax call
                let o = {
                    url: url,
                    method: 'GET',
                    data: { q: str },
                    dataType: 'json',
                    success: function (result) {
                        // Reset items
                        obj.items = [];

                        // Add the current selected items to the results in case they are not there
                        var current = Object.keys(obj.value);
                        if (current.length) {
                            for (var i = 0; i < current.length; i++) {
                                if (!exists(current[i], result)) {
                                    if (!obj.options.format) {
                                        result.unshift({value: current[i], text: obj.value[current[i]]});
                                    } else {
                                        result.unshift({id: current[i], name: obj.value[current[i]]});
                                    }
                                }
                            }
                        }
                        // Append data
                        obj.appendData(result);
                        // Show or hide results
                        if (!result.length) {
                            content.style.display = 'none';
                        } else {
                            content.style.display = '';
                        }

                        if (typeof(obj.options.onsearch) === 'function') {
                            obj.options.onsearch(obj, result);
                        }
                    }
                }

                if (typeof(obj.options.onbeforesearch) === 'function') {
                    let ret = obj.options.onbeforesearch(obj, o);
                    if (ret === false) {
                        return;
                    } else if (typeof(ret) === 'object') {
                        o = ret;
                    }
                }

                // Remote search
                ajax(o);
            } else {
                // Search terms
                str = new RegExp(str, 'gi');

                // Reset search
                var results = [];

                // Append options
                for (var i = 0; i < obj.items.length; i++) {
                    // Item label
                    var label = Text(i);
                    // Item title
                    var title = obj.items[i].data.title || '';
                    // Group name
                    var groupName = obj.items[i].data.group || '';
                    // Synonym
                    var synonym = obj.items[i].data.synonym || '';
                    if (synonym) {
                        synonym = synonym.join(' ');
                    }

                    if (str == null || obj.items[i].selected == true || label.toString().match(str) || title.match(str) || groupName.match(str) || synonym.match(str)) {
                        results.push(obj.items[i]);
                    }
                }

                if (!results.length) {
                    content.style.display = 'none';

                    // Results
                    obj.results = null;
                } else {
                    content.style.display = '';

                    // Results
                    obj.results = results;

                    // Show 200 items at once
                    var number = results.length || 0;

                    // Lazyloading
                    if (obj.options.lazyLoading == true && number > 200) {
                        number = 200;
                    }

                    for (var i = 0; i < number; i++) {
                        if (obj.results[i].group) {
                            if (!obj.results[i].group.parentNode) {
                                content.appendChild(obj.results[i].group);
                            }
                            obj.results[i].group.lastChild.appendChild(obj.results[i].element);
                        } else {
                            content.appendChild(obj.results[i].element);
                        }
                    }
                }
            }

            // Auto focus
            if (obj.options.autofocus == true) {
                obj.first();
            }
        }

        obj.open = function () {
            // Focus
            if (!el.classList.contains('jdropdown-focus')) {
                // Current dropdown
                Component.current = obj;

                // Start tracking
                tracking(obj, true);

                // Add focus
                el.classList.add('jdropdown-focus');

                // Animation
                if (helpers.getWindowWidth() < 800) {
                    if (obj.options.type == null || obj.options.type == 'picker') {
                        animation.slideBottom(container, 1);
                    }
                }

                // Filter
                if (obj.options.autocomplete == true) {
                    obj.header.value = obj.search;
                    obj.header.focus();
                }

                // Set cursor for the first or first selected element
                var k = getValue();
                if (k[0]) {
                    var cursor = obj.getPosition(k[0]);
                    if (cursor !== false) {
                        obj.setCursor(cursor);
                    }
                }

                // Container Size
                if (!obj.options.type || obj.options.type == 'default') {
                    var rect = el.getBoundingClientRect();
                    var rectContainer = container.getBoundingClientRect();

                    if (obj.options.position) {
                        container.style.position = 'fixed';
                        if (window.innerHeight < rect.bottom + rectContainer.height) {
                            container.style.top = '';
                            container.style.bottom = (window.innerHeight - rect.top) + 1 + 'px';
                        } else {
                            container.style.top = rect.bottom + 'px';
                            container.style.bottom = '';
                        }
                        container.style.left = rect.left + 'px';
                    } else {
                        if (window.innerHeight < rect.bottom + rectContainer.height) {
                            container.style.top = '';
                            container.style.bottom = rect.height + 1 + 'px';
                        } else {
                            container.style.top = '';
                            container.style.bottom = '';
                        }
                    }

                    container.style.minWidth = rect.width + 'px';

                    if (obj.options.maxWidth) {
                        container.style.maxWidth = obj.options.maxWidth;
                    }

                    if (!obj.items.length && obj.options.autocomplete == true) {
                        content.style.display = 'none';
                    } else {
                        content.style.display = '';
                    }
                }
            }

            // Events
            if (typeof (obj.options.onopen) == 'function') {
                obj.options.onopen(el);
            }
        }

        obj.close = function (ignoreEvents) {
            if (el.classList.contains('jdropdown-focus')) {
                // Update labels
                obj.header.value = obj.getText();
                // Remove cursor
                obj.setCursor();
                // Events
                if (!ignoreEvents && typeof (obj.options.onclose) == 'function') {
                    obj.options.onclose(el);
                }
                // Blur
                if (obj.header.blur) {
                    obj.header.blur();
                }
                // Remove focus
                el.classList.remove('jdropdown-focus');
                // Start tracking
                tracking(obj, false);
                // Current dropdown
                Component.current = null;
            }

            return obj.getValue();
        }

        /**
         * Set cursor
         */
        obj.setCursor = function (index, setPosition) {
            // Remove current cursor
            if (obj.currentIndex != null) {
                // Remove visual cursor
                if (obj.items && obj.items[obj.currentIndex]) {
                    obj.items[obj.currentIndex].element.classList.remove('jdropdown-cursor');
                }
            }

            if (index == undefined) {
                obj.currentIndex = null;
            } else {
                index = parseInt(index);

                // Cursor only for visible items
                if (obj.items[index].element.parentNode) {
                    obj.items[index].element.classList.add('jdropdown-cursor');
                    obj.currentIndex = index;

                    // Update scroll to the cursor element
                    if (setPosition !== false && obj.items[obj.currentIndex].element) {
                        var container = content.scrollTop;
                        var element = obj.items[obj.currentIndex].element;
                        content.scrollTop = element.offsetTop - element.scrollTop + element.clientTop - 95;
                    }
                }
            }
        }

        // Compatibility
        obj.resetCursor = obj.setCursor;
        obj.updateCursor = obj.setCursor;

        /**
         * Reset cursor and selected items
         */
        obj.reset = function () {
            // Reset cursor
            obj.setCursor();

            // Reset selected
            obj.setValue(null);
        }

        /**
         * First available item
         */
        obj.first = function () {
            if (obj.options.lazyLoading === true) {
                obj.loadFirst();
            }

            var items = content.querySelectorAll('.jdropdown-item');
            if (items.length) {
                var newIndex = items[0].indexValue;
                obj.setCursor(newIndex);
            }
        }

        /**
         * Last available item
         */
        obj.last = function () {
            if (obj.options.lazyLoading === true) {
                obj.loadLast();
            }

            var items = content.querySelectorAll('.jdropdown-item');
            if (items.length) {
                var newIndex = items[items.length - 1].indexValue;
                obj.setCursor(newIndex);
            }
        }

        obj.next = function (letter) {
            var newIndex = null;

            if (letter) {
                if (letter.length == 1) {
                    // Current index
                    var current = obj.currentIndex || -1;
                    // Letter
                    letter = letter.toLowerCase();

                    var e = null;
                    var l = null;
                    var items = content.querySelectorAll('.jdropdown-item');
                    if (items.length) {
                        for (var i = 0; i < items.length; i++) {
                            if (items[i].indexValue > current) {
                                if (e = obj.items[items[i].indexValue]) {
                                    if (l = e.element.innerText[0]) {
                                        l = l.toLowerCase();
                                        if (letter == l) {
                                            newIndex = items[i].indexValue;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        obj.setCursor(newIndex);
                    }
                }
            } else {
                if (obj.currentIndex == undefined || obj.currentIndex == null) {
                    obj.first();
                } else {
                    var element = obj.items[obj.currentIndex].element;

                    var next = element.nextElementSibling;
                    if (next) {
                        if (next.classList.contains('jdropdown-group')) {
                            next = next.lastChild.firstChild;
                        }
                        newIndex = next.indexValue;
                    } else {
                        if (element.parentNode.classList.contains('jdropdown-group-items')) {
                            if (next = element.parentNode.parentNode.nextElementSibling) {
                                if (next.classList.contains('jdropdown-group')) {
                                    next = next.lastChild.firstChild;
                                } else if (next.classList.contains('jdropdown-item')) {
                                    newIndex = next.indexValue;
                                } else {
                                    next = null;
                                }
                            }

                            if (next) {
                                newIndex = next.indexValue;
                            }
                        }
                    }

                    if (newIndex !== null) {
                        obj.setCursor(newIndex);
                    }
                }
            }
        }

        obj.prev = function () {
            var newIndex = null;

            if (obj.currentIndex === null) {
                obj.first();
            } else {
                var element = obj.items[obj.currentIndex].element;

                var prev = element.previousElementSibling;
                if (prev) {
                    if (prev.classList.contains('jdropdown-group')) {
                        prev = prev.lastChild.lastChild;
                    }
                    newIndex = prev.indexValue;
                } else {
                    if (element.parentNode.classList.contains('jdropdown-group-items')) {
                        if (prev = element.parentNode.parentNode.previousElementSibling) {
                            if (prev.classList.contains('jdropdown-group')) {
                                prev = prev.lastChild.lastChild;
                            } else if (prev.classList.contains('jdropdown-item')) {
                                newIndex = prev.indexValue;
                            } else {
                                prev = null
                            }
                        }

                        if (prev) {
                            newIndex = prev.indexValue;
                        }
                    }
                }
            }

            if (newIndex !== null) {
                obj.setCursor(newIndex);
            }
        }

        obj.loadFirst = function () {
            // Search
            if (obj.results) {
                var results = obj.results;
            } else {
                var results = obj.items;
            }

            // Show 200 items at once
            var number = results.length || 0;

            // Lazyloading
            if (obj.options.lazyLoading == true && number > 200) {
                number = 200;
            }

            // Reset container
            content.textContent = '';

            // First 200 items
            for (var i = 0; i < number; i++) {
                if (results[i].group) {
                    if (!results[i].group.parentNode) {
                        content.appendChild(results[i].group);
                    }
                    results[i].group.lastChild.appendChild(results[i].element);
                } else {
                    content.appendChild(results[i].element);
                }
            }

            // Scroll go to the begin
            content.scrollTop = 0;
        }

        obj.loadLast = function () {
            // Search
            if (obj.results) {
                var results = obj.results;
            } else {
                var results = obj.items;
            }

            // Show first page
            var number = results.length;

            // Max 200 items
            if (number > 200) {
                number = number - 200;

                // Reset container
                content.textContent = '';

                // First 200 items
                for (var i = number; i < results.length; i++) {
                    if (results[i].group) {
                        if (!results[i].group.parentNode) {
                            content.appendChild(results[i].group);
                        }
                        results[i].group.lastChild.appendChild(results[i].element);
                    } else {
                        content.appendChild(results[i].element);
                    }
                }

                // Scroll go to the begin
                content.scrollTop = content.scrollHeight;
            }
        }

        obj.loadUp = function () {
            var test = false;

            // Search
            if (obj.results) {
                var results = obj.results;
            } else {
                var results = obj.items;
            }

            var items = content.querySelectorAll('.jdropdown-item');
            var fistItem = items[0].indexValue;
            fistItem = obj.items[fistItem];
            var index = results.indexOf(fistItem) - 1;

            if (index > 0) {
                var number = 0;

                while (index > 0 && results[index] && number < 200) {
                    if (results[index].group) {
                        if (!results[index].group.parentNode) {
                            content.insertBefore(results[index].group, content.firstChild);
                        }
                        results[index].group.lastChild.insertBefore(results[index].element, results[index].group.lastChild.firstChild);
                    } else {
                        content.insertBefore(results[index].element, content.firstChild);
                    }

                    index--;
                    number++;
                }

                // New item added
                test = true;
            }

            return test;
        }

        obj.loadDown = function () {
            var test = false;

            // Search
            if (obj.results) {
                var results = obj.results;
            } else {
                var results = obj.items;
            }

            var items = content.querySelectorAll('.jdropdown-item');
            var lastItem = items[items.length - 1].indexValue;
            lastItem = obj.items[lastItem];
            var index = results.indexOf(lastItem) + 1;

            if (index < results.length) {
                var number = 0;
                while (index < results.length && results[index] && number < 200) {
                    if (results[index].group) {
                        if (!results[index].group.parentNode) {
                            content.appendChild(results[index].group);
                        }
                        results[index].group.lastChild.appendChild(results[index].element);
                    } else {
                        content.appendChild(results[index].element);
                    }

                    index++;
                    number++;
                }

                // New item added
                test = true;
            }

            return test;
        }

        init();

        return obj;
    });

    Component.keydown = function (e) {
        var dropdown = null;
        if (dropdown = Component.current) {
            if (e.which == 13 || e.which == 9) {  // enter or tab
                if (dropdown.header.value && dropdown.currentIndex == null && dropdown.options.newOptions) {
                    // if they typed something in, but it matched nothing, and newOptions are allowed, start that flow
                    dropdown.add();
                } else {
                    // Quick Select/Filter
                    if (dropdown.currentIndex == null && dropdown.options.autocomplete == true && dropdown.header.value != "") {
                        dropdown.find(dropdown.header.value);
                    }
                    dropdown.selectIndex(dropdown.currentIndex);
                }
            } else if (e.which == 38) {  // up arrow
                if (dropdown.currentIndex == null) {
                    dropdown.first();
                } else if (dropdown.currentIndex > 0) {
                    dropdown.prev();
                }
                e.preventDefault();
            } else if (e.which == 40) {  // down arrow
                if (dropdown.currentIndex == null) {
                    dropdown.first();
                } else if (dropdown.currentIndex + 1 < dropdown.items.length) {
                    dropdown.next();
                }
                e.preventDefault();
            } else if (e.which == 36) {
                dropdown.first();
                if (!e.target.classList.contains('jdropdown-header')) {
                    e.preventDefault();
                }
            } else if (e.which == 35) {
                dropdown.last();
                if (!e.target.classList.contains('jdropdown-header')) {
                    e.preventDefault();
                }
            } else if (e.which == 27) {
                dropdown.close();
            } else if (e.which == 33) {  // page up
                if (dropdown.currentIndex == null) {
                    dropdown.first();
                } else if (dropdown.currentIndex > 0) {
                    for (var i = 0; i < 7; i++) {
                        dropdown.prev()
                    }
                }
                e.preventDefault();
            } else if (e.which == 34) {  // page down
                if (dropdown.currentIndex == null) {
                    dropdown.first();
                } else if (dropdown.currentIndex + 1 < dropdown.items.length) {
                    for (var i = 0; i < 7; i++) {
                        dropdown.next()
                    }
                }
                e.preventDefault();
            }
        }
    }

    Component.mouseup = function (e) {
        var element = helpers.findElement(e.target, 'jdropdown');
        if (element) {
            var dropdown = element.dropdown;
            if (e.target.classList.contains('jdropdown-header')) {
                if (element.classList.contains('jdropdown-focus') && element.classList.contains('jdropdown-default')) {
                    var rect = element.getBoundingClientRect();

                    if (e.changedTouches && e.changedTouches[0]) {
                        var x = e.changedTouches[0].clientX;
                        var y = e.changedTouches[0].clientY;
                    } else {
                        var x = e.clientX;
                        var y = e.clientY;
                    }

                    if (rect.width - (x - rect.left) < 30) {
                        if (e.target.classList.contains('jdropdown-add')) {
                            dropdown.add();
                        } else {
                            dropdown.close();
                        }
                    } else {
                        if (dropdown.options.autocomplete == false) {
                            dropdown.close();
                        }
                    }
                } else {
                    dropdown.open();
                }
            } else if (e.target.classList.contains('jdropdown-group-name')) {
                var items = e.target.nextSibling.children;
                if (e.target.nextSibling.style.display != 'none') {
                    for (var i = 0; i < items.length; i++) {
                        if (items[i].style.display != 'none') {
                            dropdown.selectItem(items[i]);
                        }
                    }
                }
            } else if (e.target.classList.contains('jdropdown-group-arrow')) {
                if (e.target.classList.contains('jdropdown-group-arrow-down')) {
                    e.target.classList.remove('jdropdown-group-arrow-down');
                    e.target.classList.add('jdropdown-group-arrow-up');
                    e.target.parentNode.nextSibling.style.display = 'none';
                } else {
                    e.target.classList.remove('jdropdown-group-arrow-up');
                    e.target.classList.add('jdropdown-group-arrow-down');
                    e.target.parentNode.nextSibling.style.display = '';
                }
            } else if (e.target.classList.contains('jdropdown-item')) {
                dropdown.selectItem(e.target);
            } else if (e.target.classList.contains('jdropdown-image')) {
                dropdown.selectItem(e.target.parentNode);
            } else if (e.target.classList.contains('jdropdown-description')) {
                dropdown.selectItem(e.target.parentNode);
            } else if (e.target.classList.contains('jdropdown-title')) {
                dropdown.selectItem(e.target.parentNode.parentNode);
            } else if (e.target.classList.contains('jdropdown-close') || e.target.classList.contains('jdropdown-backdrop')) {
                dropdown.close();
            }
        }
    }

    Component.extractFromDom = function (el, options) {
        // Keep reference
        var select = el;
        if (!options) {
            options = {};
        }
        // Prepare configuration
        if (el.getAttribute('multiple') && (!options || options.multiple == undefined)) {
            options.multiple = true;
        }
        if (el.getAttribute('placeholder') && (!options || options.placeholder == undefined)) {
            options.placeholder = el.getAttribute('placeholder');
        }
        if (el.getAttribute('data-autocomplete') && (!options || options.autocomplete == undefined)) {
            options.autocomplete = true;
        }
        if (!options || options.width == undefined) {
            options.width = el.offsetWidth;
        }
        if (el.value && (!options || options.value == undefined)) {
            options.value = el.value;
        }
        if (!options || options.data == undefined) {
            options.data = [];
            for (var j = 0; j < el.children.length; j++) {
                if (el.children[j].tagName == 'OPTGROUP') {
                    for (var i = 0; i < el.children[j].children.length; i++) {
                        options.data.push({
                            value: el.children[j].children[i].value,
                            text: el.children[j].children[i].textContent,
                            group: el.children[j].getAttribute('label'),
                        });
                    }
                } else {
                    options.data.push({
                        value: el.children[j].value,
                        text: el.children[j].textContent,
                    });
                }
            }
        }
        if (!options || options.onchange == undefined) {
            options.onchange = function (a, b, c, d) {
                if (options.multiple == true) {
                    if (obj.items[b].classList.contains('jdropdown-selected')) {
                        select.options[b].setAttribute('selected', 'selected');
                    } else {
                        select.options[b].removeAttribute('selected');
                    }
                } else {
                    select.value = d;
                }
            }
        }
        // Create DIV
        var div = document.createElement('div');
        el.parentNode.insertBefore(div, el);
        el.style.display = 'none';
        el = div;

        return {el: el, options: options};
    }

    return Component;
}

/* harmony default export */ var dropdown = (Dropdown());
;// CONCATENATED MODULE: ./src/plugins/picker.js



function Picker(el, options) {
    // Already created, update options
    if (el.picker) {
        return el.picker.setOptions(options, true);
    }

    // New instance
    var obj = { type: 'picker' };
    obj.options = {};

    var dropdownHeader = null;
    var dropdownContent = null;

    /**
     * The element passed is a DOM element
     */
    var isDOM = function(o) {
        return (o instanceof Element || o instanceof HTMLDocument);
    }

    /**
     * Create the content options
     */
    var createContent = function() {
        dropdownContent.innerHTML = '';

        // Create items
        var keys = Object.keys(obj.options.data);

        // Go though all options
        for (var i = 0; i < keys.length; i++) {
            // Item
            var dropdownItem = document.createElement('div');
            dropdownItem.classList.add('jpicker-item');
            dropdownItem.setAttribute('role', 'option');
            dropdownItem.k = keys[i];
            dropdownItem.v = obj.options.data[keys[i]];
            // Label
            var item = obj.getLabel(keys[i], dropdownItem);
            if (isDOM(item)) {
                dropdownItem.appendChild(item);
            } else {
                dropdownItem.innerHTML = item;
            }
            // Append
            dropdownContent.appendChild(dropdownItem);
        }
    }

    /**
     * Set or reset the options for the picker
     */
    obj.setOptions = function(options, reset) {
        // Default configuration
        var defaults = {
            value: 0,
            data: null,
            render: null,
            onchange: null,
            onmouseover: null,
            onselect: null,
            onopen: null,
            onclose: null,
            onload: null,
            width: null,
            header: true,
            right: false,
            bottom: false,
            content: false,
            columns: null,
            grid: null,
            height: null,
        }

        // Legacy purpose only
        if (options && options.options) {
            options.data = options.options;
        }

        // Loop through the initial configuration
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                if (typeof(obj.options[property]) == 'undefined' || reset === true) {
                    obj.options[property] = defaults[property];
                }
            }
        }

        // Start using the options
        if (obj.options.header === false) {
            dropdownHeader.style.display = 'none';
        } else {
            dropdownHeader.style.display = '';
        }

        // Width
        if (obj.options.width) {
            dropdownHeader.style.width = parseInt(obj.options.width) + 'px';
        } else {
            dropdownHeader.style.width = '';
        }

        // Height
        if (obj.options.height) {
            dropdownContent.style.maxHeight = obj.options.height + 'px';
            dropdownContent.style.overflow = 'scroll';
        } else {
            dropdownContent.style.overflow = '';
        }

        if (obj.options.columns > 0) {
            if (! obj.options.grid) {
                dropdownContent.classList.add('jpicker-columns');
                dropdownContent.style.width = obj.options.width ? obj.options.width : 36 * obj.options.columns + 'px';
            } else {
                dropdownContent.classList.add('jpicker-grid');
                dropdownContent.style.gridTemplateColumns = 'repeat(' + obj.options.grid + ', 1fr)';
            }
        }

        if (isNaN(parseInt(obj.options.value))) {
            obj.options.value = 0;
        }

        // Create list from data
        createContent();

        // Set value
        obj.setValue(obj.options.value);

        // Set options all returns the own instance
        return obj;
    }

    obj.getValue = function() {
        return obj.options.value;
    }

    obj.setValue = function(k, e) {
        // Set label
        obj.setLabel(k);

        // Update value
        obj.options.value = String(k);

        // Lemonade JS
        if (el.value != obj.options.value) {
            el.value = obj.options.value;
            if (typeof(el.oninput) == 'function') {
                el.oninput({
                    type: 'input',
                    target: el,
                    value: el.value
                });
            }
        }

        if (dropdownContent.children[k] && dropdownContent.children[k].getAttribute('type') !== 'generic') {
            obj.close();
        }

        // Call method
        if (e) {
            if (typeof (obj.options.onchange) == 'function') {
                var v = obj.options.data[k];

                obj.options.onchange(el, obj, v, v, k, e);
            }
        }
    }

    obj.getLabel = function(v, item) {
        var label = obj.options.data[v] || null;
        if (typeof(obj.options.render) == 'function') {
            label = obj.options.render(label, item);
        }
        return label;
    }

    obj.setLabel = function(v) {
        var item;

        if (obj.options.content) {
            item = document.createElement('i');
            item.textContent = obj.options.content;
            item.classList.add('material-icons');
        } else {
            item = obj.getLabel(v, null);
        }

        // Label
        if (isDOM(item)) {
            dropdownHeader.textContent = '';
            dropdownHeader.appendChild(item);
        } else {
            dropdownHeader.innerHTML = item;
        }
    }

    obj.open = function() {
        if (! el.classList.contains('jpicker-focus')) {
            // Start tracking the element
            tracking(obj, true);

            // Open picker
            el.classList.add('jpicker-focus');
            el.focus();

            var top = 0;
            var left = 0;

            dropdownContent.style.marginLeft = '';

            var rectHeader = dropdownHeader.getBoundingClientRect();
            var rectContent = dropdownContent.getBoundingClientRect();

            if (window.innerHeight < rectHeader.bottom + rectContent.height || obj.options.bottom) {
                top = -1 * (rectContent.height + 4);
            } else {
                top = rectHeader.height + 4;
            }

            if (obj.options.right === true) {
                left = -1 * rectContent.width + rectHeader.width;
            }

            if (rectContent.left + left < 0) {
                left = left + rectContent.left + 10;
            }
            if (rectContent.left + rectContent.width > window.innerWidth) {
                left = -1 * (10 + rectContent.left + rectContent.width - window.innerWidth);
            }

            dropdownContent.style.marginTop = parseInt(top) + 'px';
            dropdownContent.style.marginLeft = parseInt(left) + 'px';

            //dropdownContent.style.marginTop
            if (typeof obj.options.onopen == 'function') {
                obj.options.onopen(el, obj);
            }
        }
    }

    obj.close = function() {
        if (el.classList.contains('jpicker-focus')) {
            el.classList.remove('jpicker-focus');

            // Start tracking the element
            tracking(obj, false);

            if (typeof obj.options.onclose == 'function') {
                obj.options.onclose(el, obj);
            }
        }
    }

    /**
     * Create floating picker
     */
    var init = function() {
        let id = helpers.guid();

        // Class
        el.classList.add('jpicker');
        el.setAttribute('role', 'combobox');
        el.setAttribute('aria-haspopup', 'listbox');
        el.setAttribute('aria-expanded', 'false');
        el.setAttribute('aria-controls', id);
        el.setAttribute('tabindex', '0');
        el.onmousedown = function(e) {
            if (! el.classList.contains('jpicker-focus')) {
                obj.open();
            }
        }

        // Dropdown Header
        dropdownHeader = document.createElement('div');
        dropdownHeader.classList.add('jpicker-header');

        // Dropdown content
        dropdownContent = document.createElement('div');
        dropdownContent.setAttribute('id', id);
        dropdownContent.setAttribute('role', 'listbox');
        dropdownContent.classList.add('jpicker-content');
        dropdownContent.onclick = function(e) {
            var item = helpers.findElement(e.target, 'jpicker-item');
            if (item) {
                if (item.parentNode === dropdownContent) {
                    // Update label
                    obj.setValue(item.k, e);
                }
            }
        }
        // Append content and header
        el.appendChild(dropdownHeader);
        el.appendChild(dropdownContent);

        // Default value
        el.value = options.value || 0;

        // Set options
        obj.setOptions(options);

        if (typeof(obj.options.onload) == 'function') {
            obj.options.onload(el, obj);
        }

        // Change
        el.change = obj.setValue;

        // Global generic value handler
        el.val = function(val) {
            if (val === undefined) {
                return obj.getValue();
            } else {
                obj.setValue(val);
            }
        }

        // Reference
        el.picker = obj;
    }

    init();

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/toolbar.js





function Toolbar(el, options) {
    // New instance
    var obj = { type:'toolbar' };
    obj.options = {};

    // Default configuration
    var defaults = {
        app: null,
        container: false,
        badge: false,
        title: false,
        responsive: false,
        maxWidth: null,
        bottom: true,
        items: [],
    }

    // Loop through our object
    for (var property in defaults) {
        if (options && options.hasOwnProperty(property)) {
            obj.options[property] = options[property];
        } else {
            obj.options[property] = defaults[property];
        }
    }

    if (! el && options.app && options.app.el) {
        el = document.createElement('div');
        options.app.el.appendChild(el);
    }

    // Arrow
    var toolbarArrow = document.createElement('div');
    toolbarArrow.classList.add('jtoolbar-item');
    toolbarArrow.classList.add('jtoolbar-arrow');

    var toolbarFloating = document.createElement('div');
    toolbarFloating.classList.add('jtoolbar-floating');
    toolbarArrow.appendChild(toolbarFloating);

    obj.selectItem = function(element) {
        var elements = toolbarContent.children;
        for (var i = 0; i < elements.length; i++) {
            if (element != elements[i]) {
                elements[i].classList.remove('jtoolbar-selected');
            }
        }
        element.classList.add('jtoolbar-selected');
    }

    obj.hide = function() {
        animation.slideBottom(el, 0, function() {
            el.style.display = 'none';
        });
    }

    obj.show = function() {
        el.style.display = '';
        animation.slideBottom(el, 1);
    }

    obj.get = function() {
        return el;
    }

    obj.setBadge = function(index, value) {
        toolbarContent.children[index].children[1].firstChild.innerHTML = value;
    }

    obj.destroy = function() {
        toolbar.remove();
        el.innerHTML = '';
    }

    obj.update = function(a, b) {
        for (var i = 0; i < toolbarContent.children.length; i++) {
            // Toolbar element
            var toolbarItem = toolbarContent.children[i];
            // State management
            if (typeof(toolbarItem.updateState) == 'function') {
                toolbarItem.updateState(el, obj, toolbarItem, a, b);
            }
        }
        for (var i = 0; i < toolbarFloating.children.length; i++) {
            // Toolbar element
            var toolbarItem = toolbarFloating.children[i];
            // State management
            if (typeof(toolbarItem.updateState) == 'function') {
                toolbarItem.updateState(el, obj, toolbarItem, a, b);
            }
        }
    }

    obj.create = function(items) {
        // Reset anything in the toolbar
        toolbarContent.innerHTML = '';
        // Create elements in the toolbar
        for (var i = 0; i < items.length; i++) {
            var toolbarItem = document.createElement('div');
            toolbarItem.classList.add('jtoolbar-item');

            if (items[i].width) {
                toolbarItem.style.width = parseInt(items[i].width) + 'px'; 
            }

            if (items[i].k) {
                toolbarItem.k = items[i].k;
            }

            if (items[i].tooltip) {
                toolbarItem.setAttribute('title', items[i].tooltip);
                toolbarItem.setAttribute('aria-label', items[i].tooltip);
            }

            // Id
            if (items[i].id) {
                toolbarItem.setAttribute('id', items[i].id);
            }

            // Selected
            if (items[i].updateState) {
                toolbarItem.updateState = items[i].updateState;
            }

            if (items[i].active) {
                toolbarItem.classList.add('jtoolbar-active');
            }

            if (items[i].disabled) {
                toolbarItem.classList.add('jtoolbar-disabled');
            }

            if (items[i].type == 'select' || items[i].type == 'dropdown') {
                Picker(toolbarItem, items[i]);
            } else if (items[i].type == 'divisor') {
                toolbarItem.classList.add('jtoolbar-divisor');
            } else if (items[i].type == 'label') {
                toolbarItem.classList.add('jtoolbar-label');
                toolbarItem.innerHTML = items[i].content;
            } else {
                // Material icons
                var toolbarIcon = document.createElement('i');
                if (typeof(items[i].class) === 'undefined') {
                    toolbarIcon.classList.add('material-icons');
                } else {
                    var c = items[i].class.split(' ');
                    for (var j = 0; j < c.length; j++) {
                        toolbarIcon.classList.add(c[j]);
                    }
                }
                toolbarIcon.innerHTML = items[i].content ? items[i].content : '';
                toolbarItem.setAttribute('role', 'button');
                toolbarItem.appendChild(toolbarIcon);

                // Badge options
                if (obj.options.badge == true) {
                    var toolbarBadge = document.createElement('div');
                    toolbarBadge.classList.add('jbadge');
                    var toolbarBadgeContent = document.createElement('div');
                    toolbarBadgeContent.innerHTML = items[i].badge ? items[i].badge : '';
                    toolbarBadge.appendChild(toolbarBadgeContent);
                    toolbarItem.appendChild(toolbarBadge);
                }

                // Title
                if (items[i].title) {
                    if (obj.options.title == true) {
                        var toolbarTitle = document.createElement('span');
                        toolbarTitle.innerHTML = items[i].title;
                        toolbarItem.appendChild(toolbarTitle);
                    } else {
                        toolbarItem.setAttribute('title', items[i].title);
                    }
                }

                if (obj.options.app && items[i].route) {
                    // Route
                    toolbarItem.route = items[i].route;
                    // Onclick for route
                    toolbarItem.onclick = function() {
                        obj.options.app.pages(this.route);
                    }
                    // Create pages
                    obj.options.app.pages(items[i].route, {
                        toolbarItem: toolbarItem,
                        closed: true
                    });
                }

                // Render
                if (typeof(items[i].render) === 'function') {
                    items[i].render(toolbarItem, items[i]);
                }
            }

            if (items[i].onclick) {
                toolbarItem.onclick = items[i].onclick.bind(items[i], el, obj, toolbarItem);
            }

            toolbarContent.appendChild(toolbarItem);
        }

        // Fits to the page
        setTimeout(function() {
            obj.refresh();
        }, 0);
    }

    obj.open = function() {
        toolbarArrow.classList.add('jtoolbar-arrow-selected');

        var rectElement = el.getBoundingClientRect();
        var rect = toolbarFloating.getBoundingClientRect();
        if (rect.bottom > window.innerHeight || obj.options.bottom) {
            toolbarFloating.style.bottom = '0';
        } else {
            toolbarFloating.style.removeProperty('bottom');
        }

        toolbarFloating.style.right = '0';

        toolbarArrow.children[0].focus();
        // Start tracking
        tracking(obj, true);
    }

    obj.close = function() {
        toolbarArrow.classList.remove('jtoolbar-arrow-selected')
        // End tracking
        tracking(obj, false);
    }

    obj.refresh = function() {
        if (obj.options.responsive == true) {
            // Width of the c
            var rect = el.parentNode.getBoundingClientRect();
            if (! obj.options.maxWidth) {
                obj.options.maxWidth = rect.width;
            }
            // Available parent space
            var available = parseInt(obj.options.maxWidth);
            // Remove arrow
            if (toolbarArrow.parentNode) {
                toolbarArrow.parentNode.removeChild(toolbarArrow);
            }
            // Move all items to the toolbar
            while (toolbarFloating.firstChild) {
                toolbarContent.appendChild(toolbarFloating.firstChild);
            }
            // Toolbar is larger than the parent, move elements to the floating element
            if (available < toolbarContent.offsetWidth) {
                // Give space to the floating element
                available -= 50;
                // Move to the floating option
                while (toolbarContent.lastChild && available < toolbarContent.offsetWidth) {
                    toolbarFloating.insertBefore(toolbarContent.lastChild, toolbarFloating.firstChild);
                }
            }
            // Show arrow
            if (toolbarFloating.children.length > 0) {
                toolbarContent.appendChild(toolbarArrow);
            }
        }
    }

    obj.setReadonly = function(state) {
        state = state ? 'add' : 'remove';
        el.classList[state]('jtoolbar-disabled');
    }

    el.onclick = function(e) {
        var element = helpers.findElement(e.target, 'jtoolbar-item');
        if (element) {
            obj.selectItem(element);
        }

        if (e.target.classList.contains('jtoolbar-arrow')) {
            obj.open();
        }
    }

    window.addEventListener('resize', function() {
        obj.refresh();
    });

    // Toolbar
    el.classList.add('jtoolbar');
    // Reset content
    el.innerHTML = '';
    // Container
    if (obj.options.container == true) {
        el.classList.add('jtoolbar-container');
    }
    // Content
    var toolbarContent = document.createElement('div');
    el.appendChild(toolbarContent);
    // Special toolbar for mobile applications
    if (obj.options.app) {
        el.classList.add('jtoolbar-mobile');
    }
    // Create toolbar
    obj.create(obj.options.items);
    // Shortcut
    el.toolbar = obj;

    return obj;
}
;// CONCATENATED MODULE: ./src/utils/filter.js

// Valid tags
const validTags = [
    'html','body','address','span', 'div', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'p', 'b', 'i', 'blockquote',
    'strong', 'em', 'ul', 'ol', 'li', 'a', 'code', 'pre', 'hr', 'br', 'img',
    'figure', 'picture', 'figcaption', 'iframe', 'table', 'thead', 'tbody', 'tfoot', 'tr',
    'th', 'td', 'caption', 'u', 'del', 'ins', 'sub', 'sup', 'small', 'mark',
    'input', 'textarea', 'select', 'option', 'button', 'label', 'fieldset',
    'legend', 'audio', 'video', 'abbr', 'cite', 'kbd', 'section', 'article',
    'nav', 'aside', 'header', 'footer', 'main', 'details', 'summary', 'svg', 'line', 'source'
];
// Valid properties
const validProperty = ['width', 'height', 'align', 'border', 'src', 'tabindex'];
// Valid CSS attributes
const validStyle = ['color', 'font-weight', 'font-size', 'background', 'background-color', 'margin'];

const parse = function(element, img) {
    // Remove elements that are not white-listed
    if (element.tagName && validTags.indexOf(element.tagName.toLowerCase()) === -1) {
        if (element.innerText) {
            element.innerHTML = element.innerText;
        }
    }
    // Remove attributes
    if (element.attributes && element.attributes.length) {
        let style = null;
        // Process style attribute
        let elementStyle = element.getAttribute('style');
        if (elementStyle) {
            style = [];
            let t = elementStyle.split(';');
            for (let j = 0; j < t.length; j++) {
                let v = t[j].trim().split(':');
                if (validStyle.indexOf(v[0].trim()) >= 0) {
                    let k = v.shift();
                    v = v.join(':');
                    style.push(k + ':' + v);
                }
            }
        }
        // Process image
        if (element.tagName.toUpperCase() === 'IMG') {
            if (! element.src) {
                element.parentNode.removeChild(element);
            } else {
                // Check if is data
                element.setAttribute('tabindex', '900');
                // Check attributes for persistence
                img.push(element.src);
            }
        }
        // Remove attributes
        let attr = [];
        for (let i = 0; i < element.attributes.length; i++) {
            attr.push(element.attributes[i].name);
        }
        if (attr.length) {
            attr.forEach(function (v) {
                if (validProperty.indexOf(v) === -1) {
                    element.removeAttribute(v);
                } else {
                    // Protection XSS
                    let at = element.getAttribute(v);
                    if (at.indexOf('<') !== -1) {
                        element.setAttribute(v, at.replace('<', '&#60;'));
                    }
                }
            });
        }
        element.style = '';
        // Add valid style
        if (style && style.length) {
            element.setAttribute('style', style.join(';'));
        }
    }
    // Parse children
    if (element.children.length) {
        for (let i = element.children.length; i > 0; i--) {
            parse(element.children[i - 1], img);
        }
    }
}

const filter = function(data, img) {
    if (data) {
        data = data.replace(new RegExp('<!--(.*?)-->', 'gsi'), '');
    }
    let parser = new DOMParser();
    let d = parser.parseFromString(data, "text/html");
    parse(d, img);
    let div = document.createElement('div');
    div.innerHTML = d.firstChild.innerHTML;
    return div;
}

/* harmony default export */ var utils_filter = (filter);
;// CONCATENATED MODULE: ./src/plugins/editor.js






function Editor() {
    var Component = (function(el, options) {
        // New instance
        var obj = { type:'editor' };
        obj.options = {};

        // Default configuration
        var defaults = {
            // Load data from a remove location
            url: null,
            // Initial HTML content
            value: '',
            // Initial snippet
            snippet: null,
            // Add toolbar
            toolbar: true,
            toolbarOnTop: false,
            // Website parser is to read websites and images from cross domain
            remoteParser: null,
            // Placeholder
            placeholder: null,
            // Parse URL
            filterPaste: true,
            // Accept drop files
            dropZone: true,
            dropAsSnippet: false,
            acceptImages: true,
            acceptFiles: false,
            maxFileSize: 5000000,
            allowImageResize: true,
            // Style
            maxHeight: null,
            height: null,
            focus: false,
            // Events
            onclick: null,
            onfocus: null,
            onblur: null,
            onload: null,
            onkeyup: null,
            onkeydown: null,
            onchange: null,
            extensions: null,
            type: null,
        };

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        // Private controllers
        var editorTimer = null;
        var editorAction = null;
        var files = [];

        // Keep the reference for the container
        obj.el = el;

        if (typeof(obj.options.onclick) == 'function') {
            el.onclick = function(e) {
                obj.options.onclick(el, obj, e);
            }
        }

        // Prepare container
        el.classList.add('jeditor-container');

        // Snippet
        var snippet = document.createElement('div');
        snippet.className = 'jsnippet';
        snippet.setAttribute('contenteditable', false);

        // Toolbar
        var toolbar = document.createElement('div');
        toolbar.className = 'jeditor-toolbar';

        obj.editor = document.createElement('div');
        obj.editor.setAttribute('contenteditable', true);
        obj.editor.setAttribute('spellcheck', false);
        obj.editor.classList.add('jeditor');

        // Placeholder
        if (obj.options.placeholder) {
            obj.editor.setAttribute('data-placeholder', obj.options.placeholder);
        }

        // Max height
        if (obj.options.maxHeight || obj.options.height) {
            obj.editor.style.overflowY = 'auto';

            if (obj.options.maxHeight) {
                obj.editor.style.maxHeight = obj.options.maxHeight;
            }
            if (obj.options.height) {
                obj.editor.style.height = obj.options.height;
            }
        }

        // Set editor initial value
        if (obj.options.url) {
            ajax({
                url: obj.options.url,
                dataType: 'html',
                success: function(result) {
                    obj.editor.innerHTML = result;

                    Component.setCursor(obj.editor, obj.options.focus == 'initial' ? true : false);
                }
            })
        } else {
            if (obj.options.value) {
                obj.editor.innerHTML = obj.options.value;
            } else {
                // Create from existing elements
                for (var i = 0; i < el.children.length; i++) {
                    obj.editor.appendChild(el.children[i]);
                }
            }
        }

        // Make sure element is empty
        el.innerHTML = '';

        /**
         * Onchange event controllers
         */
        var change = function(e) {
            if (typeof(obj.options.onchange) == 'function') {
                obj.options.onchange(el, obj, e);
            }

            // Update value
            obj.options.value = obj.getData();

            // Lemonade JS
            if (el.value != obj.options.value) {
                el.value = obj.options.value;
                if (typeof(el.oninput) == 'function') {
                    el.oninput({
                        type: 'input',
                        target: el,
                        value: el.value
                    });
                }
            }
        }

        /**
         * Extract images from a HTML string
         */
        var extractImageFromHtml = function(html) {
            let img = [];
            // Create temp element
            var div = document.createElement('div');
            utils_filter(html, img);
            if (img.length) {
                for (var i = 0; i < img.length; i++) {
                    obj.addImage(img[i]);
                }
            }
        }

        /**
         * Insert node at caret
         */
        var insertNodeAtCaret = function(newNode) {
            var sel, range;

            if (window.getSelection) {
                sel = window.getSelection();
                if (sel.rangeCount) {
                    range = sel.getRangeAt(0);
                    var selectedText = range.toString();
                    range.deleteContents();
                    range.insertNode(newNode);
                    // move the cursor after element
                    range.setStartAfter(newNode);
                    range.setEndAfter(newNode);
                    sel.removeAllRanges();
                    sel.addRange(range);
                }
            }
        }

        var updateTotalImages = function() {
            var o = null;
            if (o = snippet.children[0]) {
                // Make sure is a grid
                if (! o.classList.contains('jslider-grid')) {
                    o.classList.add('jslider-grid');
                }
                // Quantify of images
                var number = o.children.length;
                // Set the configuration of the grid
                o.setAttribute('data-number', number > 4 ? 4 : number);
                // Total of images inside the grid
                if (number > 4) {
                    o.setAttribute('data-total', number - 4);
                } else {
                    o.removeAttribute('data-total');
                }
            }
        }

        /**
         * Append image to the snippet
         */
        var appendImage = function(image) {
            if (! snippet.innerHTML) {
                obj.appendSnippet({});
            }
            snippet.children[0].appendChild(image);
            updateTotalImages();
        }

        /**
         * Append snippet
         * @Param object data
         */
        obj.appendSnippet = function(data) {
            // Reset snippet
            snippet.innerHTML = '';

            // Attributes
            var a = [ 'image', 'title', 'description', 'host', 'url' ];

            for (var i = 0; i < a.length; i++) {
                var div = document.createElement('div');
                div.className = 'jsnippet-' + a[i];
                div.setAttribute('data-k', a[i]);
                snippet.appendChild(div);
                if (data[a[i]]) {
                    if (a[i] == 'image') {
                        if (! Array.isArray(data.image)) {
                            data.image = [ data.image ];
                        }
                        for (var j = 0; j < data.image.length; j++) {
                            var img = document.createElement('img');
                            img.src = data.image[j];
                            div.appendChild(img);
                        }
                    } else {
                        div.innerHTML = data[a[i]];
                    }
                }
            }

            obj.editor.appendChild(document.createElement('br'));
            obj.editor.appendChild(snippet);
        }

        /**
         * Set editor value
         */
        obj.setData = function(o) {
            if (typeof(o) == 'object') {
                obj.editor.innerHTML = o.content;
            } else {
                obj.editor.innerHTML = o;
            }

            if (obj.options.focus) {
                Component.setCursor(obj.editor, true);
            }

            // Reset files container
            files = [];
        }

        obj.getFiles = function() {
            var f = obj.editor.querySelectorAll('.jfile');
            var d = [];
            for (var i = 0; i < f.length; i++) {
                if (files[f[i].src]) {
                    d.push(files[f[i].src]);
                }
            }
            return d;
        }

        obj.getText = function() {
            return obj.editor.innerText;
        }

        /**
         * Get editor data
         */
        obj.getData = function(json) {
            if (! json) {
                var data = obj.editor.innerHTML;
            } else {
                var data = {
                    content : '',
                }

                // Get snippet
                if (snippet.innerHTML) {
                    var index = 0;
                    data.snippet = {};
                    for (var i = 0; i < snippet.children.length; i++) {
                        // Get key from element
                        var key = snippet.children[i].getAttribute('data-k');
                        if (key) {
                            if (key == 'image') {
                                if (! data.snippet.image) {
                                    data.snippet.image = [];
                                }
                                // Get all images
                                for (var j = 0; j < snippet.children[i].children.length; j++) {
                                    data.snippet.image.push(snippet.children[i].children[j].getAttribute('src'))
                                }
                            } else {
                                data.snippet[key] = snippet.children[i].innerHTML;
                            }
                        }
                    }
                }

                // Get files
                var f = Object.keys(files);
                if (f.length) {
                    data.files = [];
                    for (var i = 0; i < f.length; i++) {
                        data.files.push(files[f[i]]);
                    }
                }

                // Get content
                var d = document.createElement('div');
                d.innerHTML = obj.editor.innerHTML;
                var s = d.querySelector('.jsnippet');
                if (s) {
                    s.remove();
                }

                var text = d.innerHTML;
                text = text.replace(/<br>/g, "\n");
                text = text.replace(/<\/div>/g, "<\/div>\n");
                text = text.replace(/<(?:.|\n)*?>/gm, "");
                data.content = text.trim();

                // Process extensions
                processExtensions('getData', data);
            }

            return data;
        }

        // Reset
        obj.reset = function() {
            obj.editor.innerHTML = '';
            snippet.innerHTML = '';
            files = [];
        }

        obj.addPdf = function(data) {
            if (data.result.substr(0,4) != 'data') {
                console.error('Invalid source');
            } else {
                var canvas = document.createElement('canvas');
                canvas.width = 60;
                canvas.height = 60;

                var img = new Image();
                var ctx = canvas.getContext('2d');
                ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

                canvas.toBlob(function(blob) {
                    var newImage = document.createElement('img');
                    newImage.src = window.URL.createObjectURL(blob);
                    newImage.title = data.name;
                    newImage.className = 'jfile pdf';

                    files[newImage.src] = {
                        file: newImage.src,
                        extension: 'pdf',
                        content: data.result,
                    }

                    //insertNodeAtCaret(newImage);
                    document.execCommand('insertHtml', false, newImage.outerHTML);
                });
            }
        }

        obj.addImage = function(src, asSnippet) {
            if (! obj.options.acceptImages) {
                return;
            }

            if (! src) {
                src = '';
            }

            if (src.substr(0,4) != 'data' && ! obj.options.remoteParser) {
                console.error('remoteParser not defined in your initialization');
            } else {
                // This is to process cross domain images
                if (src.substr(0,4) == 'data') {
                    var extension = src.split(';')
                    extension = extension[0].split('/');
                    extension = extension[1];
                } else {
                    var extension = src.substr(src.lastIndexOf('.') + 1);
                    // Work for cross browsers
                    src = obj.options.remoteParser + src;
                }

                var img = new Image();

                img.onload = function onload() {
                    var canvas = document.createElement('canvas');
                    canvas.width = img.width;
                    canvas.height = img.height;

                    var ctx = canvas.getContext('2d');
                    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

                    canvas.toBlob(function(blob) {
                        var newImage = document.createElement('img');
                        newImage.src = window.URL.createObjectURL(blob);
                        newImage.classList.add('jfile');
                        newImage.setAttribute('tabindex', '900');
                        newImage.setAttribute('width', img.width);
                        newImage.setAttribute('height', img.height);
                        files[newImage.src] = {
                            file: newImage.src,
                            extension: extension,
                            content: canvas.toDataURL(),
                        }

                        if (obj.options.dropAsSnippet || asSnippet) {
                            appendImage(newImage);
                            // Just to understand the attachment is part of a snippet
                            files[newImage.src].snippet = true;
                        } else {
                            //insertNodeAtCaret(newImage);
                            document.execCommand('insertHtml', false, newImage.outerHTML);
                        }

                        change();
                    });
                };

                img.src = src;
            }
        }

        obj.addFile = function(files) {
            var reader = [];

            for (var i = 0; i < files.length; i++) {
                if (files[i].size > obj.options.maxFileSize) {
                    alert('The file is too big');
                } else {
                    // Only PDF or Images
                    var type = files[i].type.split('/');

                    if (type[0] == 'image') {
                        type = 1;
                    } else if (type[1] == 'pdf') {
                        type = 2;
                    } else {
                        type = 0;
                    }

                    if (type) {
                        // Create file
                        reader[i] = new FileReader();
                        reader[i].index = i;
                        reader[i].type = type;
                        reader[i].name = files[i].name;
                        reader[i].date = files[i].lastModified;
                        reader[i].size = files[i].size;
                        reader[i].addEventListener("load", function (data) {
                            // Get result
                            if (data.target.type == 2) {
                                if (obj.options.acceptFiles == true) {
                                    obj.addPdf(data.target);
                                }
                            } else {
                                obj.addImage(data.target.result);
                            }
                        }, false);

                        reader[i].readAsDataURL(files[i])
                    } else {
                        alert('The extension is not allowed');
                    }
                }
            }
        }

        // Destroy
        obj.destroy = function() {
            obj.editor.removeEventListener('mouseup', editorMouseUp);
            obj.editor.removeEventListener('mousedown', editorMouseDown);
            obj.editor.removeEventListener('mousemove', editorMouseMove);
            obj.editor.removeEventListener('keyup', editorKeyUp);
            obj.editor.removeEventListener('keydown', editorKeyDown);
            obj.editor.removeEventListener('dragstart', editorDragStart);
            obj.editor.removeEventListener('dragenter', editorDragEnter);
            obj.editor.removeEventListener('dragover', editorDragOver);
            obj.editor.removeEventListener('drop', editorDrop);
            obj.editor.removeEventListener('paste', editorPaste);
            obj.editor.removeEventListener('blur', editorBlur);
            obj.editor.removeEventListener('focus', editorFocus);

            el.editor = null;
            el.classList.remove('jeditor-container');

            toolbar.remove();
            snippet.remove();
            obj.editor.remove();
        }

        obj.upload = function() {
            helpers.click(obj.file);
        }

        var select = function(e) {
            var s = window.getSelection()
            var r = document.createRange();
            r.selectNode(e);
            s.addRange(r)
        }

        var editorPaste = function(e) {
            if (obj.options.filterPaste == true) {
                if (e.clipboardData || e.originalEvent.clipboardData) {
                    var html = (e.originalEvent || e).clipboardData.getData('text/html');
                    var text = (e.originalEvent || e).clipboardData.getData('text/plain');
                    var file = (e.originalEvent || e).clipboardData.files
                } else if (window.clipboardData) {
                    var html = window.clipboardData.getData('Html');
                    var text = window.clipboardData.getData('Text');
                    var file = window.clipboardData.files
                }

                if (file.length) {
                    // Paste a image from the clipboard
                    obj.addFile(file);
                } else {
                    if (! html) {
                        html = text.split('\r\n');
                        if (! e.target.innerText) {
                            html.map(function(v) {
                                var d = document.createElement('div');
                                d.innerText = v;
                                obj.editor.appendChild(d);
                            });
                        } else {
                            html = html.map(function(v) {
                                return '<div>' + v + '</div>';
                            });
                            document.execCommand('insertText', false, html.join(''));
                        }
                    } else {
                        let img = [];
                        var d = utils_filter(html, img);
                        if (img.length) {
                            for (var i = 0; i < img.length; i++) {
                                obj.addImage(img[i]);
                            }
                        }
                        // Paste to the editor
                        //insertNodeAtCaret(d);
                        document.execCommand('insertHtml', false, d.innerHTML);
                    }
                }

                e.preventDefault();
            }
        }

        var editorDragStart = function(e) {
            if (editorAction && editorAction.e) {
                e.preventDefault();
            }
        }

        var editorDragEnter = function(e) {
            if (editorAction || obj.options.dropZone == false) {
                // Do nothing
            } else {
                el.classList.add('jeditor-dragging');
                e.preventDefault();
            }
        }

        var editorDragOver = function(e) {
            if (editorAction || obj.options.dropZone == false) {
                // Do nothing
            } else {
                if (editorTimer) {
                    clearTimeout(editorTimer);
                }

                editorTimer = setTimeout(function() {
                    el.classList.remove('jeditor-dragging');
                }, 100);
                e.preventDefault();
            }
        }

        var editorDrop = function(e) {
            if (editorAction || obj.options.dropZone === false) {
                // Do nothing
            } else {
                // Position caret on the drop
                let range = null;
                if (document.caretRangeFromPoint) {
                    range=document.caretRangeFromPoint(e.clientX, e.clientY);
                } else if (e.rangeParent) {
                    range=document.createRange();
                    range.setStart(e.rangeParent,e.rangeOffset);
                }
                let sel = window.getSelection();
                sel.removeAllRanges();
                sel.addRange(range);
                sel.anchorNode.parentNode.focus();

                let html = (e.originalEvent || e).dataTransfer.getData('text/html');
                let text = (e.originalEvent || e).dataTransfer.getData('text/plain');
                let file = (e.originalEvent || e).dataTransfer.files;

                if (file.length) {
                    obj.addFile(file);
                } else if (text) {
                    extractImageFromHtml(html);
                }

                el.classList.remove('jeditor-dragging');
                e.preventDefault();
            }
        }

        var editorBlur = function(e) {
            // Process extensions
            processExtensions('onevent', e);
            // Apply changes
            change(e);
            // Blur
            if (typeof(obj.options.onblur) == 'function') {
                obj.options.onblur(el, obj, e);
            }
        }

        var editorFocus = function(e) {
            // Focus
            if (typeof(obj.options.onfocus) == 'function') {
                obj.options.onfocus(el, obj, e);
            }
        }

        var editorKeyUp = function(e) {
            if (! obj.editor.innerHTML) {
                obj.editor.innerHTML = '<div><br></div>';
            }
            if (typeof(obj.options.onkeyup) == 'function') {
                obj.options.onkeyup(el, obj, e);
            }
        }

        var editorKeyDown = function(e) {
            // Process extensions
            processExtensions('onevent', e);

            if (e.key == 'Delete') {
                if (e.target.tagName == 'IMG') {
                    var parent = e.target.parentNode;
                    select(e.target);
                    if (parent.classList.contains('jsnippet-image')) {
                        updateTotalImages();
                    }
                }
            }

            if (typeof(obj.options.onkeydown) == 'function') {
                obj.options.onkeydown(el, obj, e);
            }
        }

        var editorMouseUp = function(e) {
            if (editorAction && editorAction.e) {
                editorAction.e.classList.remove('resizing');

                if (editorAction.e.changed == true) {
                    var image = editorAction.e.cloneNode()
                    image.width = parseInt(editorAction.e.style.width) || editorAction.e.getAttribute('width');
                    image.height = parseInt(editorAction.e.style.height) || editorAction.e.getAttribute('height');
                    editorAction.e.style.width = '';
                    editorAction.e.style.height = '';
                    select(editorAction.e);
                    document.execCommand('insertHtml', false, image.outerHTML);
                }
            }

            editorAction = false;
        }

        var editorMouseDown = function(e) {
            var close = function(snippet) {
                var rect = snippet.getBoundingClientRect();
                if (rect.width - (e.clientX - rect.left) < 40 && e.clientY - rect.top < 40) {
                    snippet.innerHTML = '';
                    snippet.remove();
                }
            }

            if (e.target.tagName == 'IMG') {
                if (e.target.style.cursor) {
                    var rect = e.target.getBoundingClientRect();
                    editorAction = {
                        e: e.target,
                        x: e.clientX,
                        y: e.clientY,
                        w: rect.width,
                        h: rect.height,
                        d: e.target.style.cursor,
                    }

                    if (! e.target.getAttribute('width')) {
                        e.target.setAttribute('width', rect.width)
                    }

                    if (! e.target.getAttribute('height')) {
                        e.target.setAttribute('height', rect.height)
                    }

                    var s = window.getSelection();
                    if (s.rangeCount) {
                        for (var i = 0; i < s.rangeCount; i++) {
                            s.removeRange(s.getRangeAt(i));
                        }
                    }

                    e.target.classList.add('resizing');
                } else {
                    editorAction = true;
                }
            } else {
                if (e.target.classList.contains('jsnippet')) {
                    close(e.target);
                } else if (e.target.parentNode.classList.contains('jsnippet')) {
                    close(e.target.parentNode);
                }

                editorAction = true;
            }
        }

        var editorMouseMove = function(e) {
            if (e.target.tagName == 'IMG' && ! e.target.parentNode.classList.contains('jsnippet-image') && obj.options.allowImageResize == true) {
                if (e.target.getAttribute('tabindex')) {
                    var rect = e.target.getBoundingClientRect();
                    if (e.clientY - rect.top < 5) {
                        if (rect.width - (e.clientX - rect.left) < 5) {
                            e.target.style.cursor = 'ne-resize';
                        } else if (e.clientX - rect.left < 5) {
                            e.target.style.cursor = 'nw-resize';
                        } else {
                            e.target.style.cursor = 'n-resize';
                        }
                    } else if (rect.height - (e.clientY - rect.top) < 5) {
                        if (rect.width - (e.clientX - rect.left) < 5) {
                            e.target.style.cursor = 'se-resize';
                        } else if (e.clientX - rect.left < 5) {
                            e.target.style.cursor = 'sw-resize';
                        } else {
                            e.target.style.cursor = 's-resize';
                        }
                    } else if (rect.width - (e.clientX - rect.left) < 5) {
                        e.target.style.cursor = 'e-resize';
                    } else if (e.clientX - rect.left < 5) {
                        e.target.style.cursor = 'w-resize';
                    } else {
                        e.target.style.cursor = '';
                    }
                }
            }

            // Move
            if (e.which == 1 && editorAction && editorAction.d) {
                if (editorAction.d == 'e-resize' || editorAction.d == 'ne-resize' ||  editorAction.d == 'se-resize') {
                    editorAction.e.style.width = (editorAction.w + (e.clientX - editorAction.x));

                    if (e.shiftKey) {
                        var newHeight = (e.clientX - editorAction.x) * (editorAction.h / editorAction.w);
                        editorAction.e.style.height = editorAction.h + newHeight;
                    } else {
                        var newHeight =  null;
                    }
                }

                if (! newHeight) {
                    if (editorAction.d == 's-resize' || editorAction.d == 'se-resize' || editorAction.d == 'sw-resize') {
                        if (! e.shiftKey) {
                            editorAction.e.style.height = editorAction.h + (e.clientY - editorAction.y);
                        }
                    }
                }

                editorAction.e.changed = true;
            }
        }

        var processExtensions = function(method, data) {
            if (obj.options.extensions) {
                var ext = Object.keys(obj.options.extensions);
                if (ext.length) {
                    for (var i = 0; i < ext.length; i++)
                        if (obj.options.extensions[ext[i]] && typeof(obj.options.extensions[ext[i]][method]) == 'function') {
                            obj.options.extensions[ext[i]][method].call(obj, data);
                        }
                }
            }
        }

        var loadExtensions = function() {
            if (obj.options.extensions) {
                var ext = Object.keys(obj.options.extensions);
                if (ext.length) {
                    for (var i = 0; i < ext.length; i++) {
                        if (obj.options.extensions[ext[i]] && typeof (obj.options.extensions[ext[i]]) == 'function') {
                            obj.options.extensions[ext[i]] = obj.options.extensions[ext[i]](el, obj);
                        }
                    }
                }
            }
        }

        document.addEventListener('mouseup', editorMouseUp);
        document.addEventListener('mousemove', editorMouseMove);
        obj.editor.addEventListener('mousedown', editorMouseDown);
        obj.editor.addEventListener('keyup', editorKeyUp);
        obj.editor.addEventListener('keydown', editorKeyDown);
        obj.editor.addEventListener('dragstart', editorDragStart);
        obj.editor.addEventListener('dragenter', editorDragEnter);
        obj.editor.addEventListener('dragover', editorDragOver);
        obj.editor.addEventListener('drop', editorDrop);
        obj.editor.addEventListener('paste', editorPaste);
        obj.editor.addEventListener('focus', editorFocus);
        obj.editor.addEventListener('blur', editorBlur);

        // Append editor to the container
        el.appendChild(obj.editor);
        // Snippet
        if (obj.options.snippet) {
            obj.appendSnippet(obj.options.snippet);
        }

        // Add toolbar
        if (obj.options.toolbar) {
            // Default toolbar configuration
            if (Array.isArray(obj.options.toolbar)) {
                var toolbarOptions = {
                    container: true,
                    responsive: true,
                    items: obj.options.toolbar
                }
            } else if (obj.options.toolbar === true) {
                var toolbarOptions = {
                    container: true,
                    responsive: true,
                    items: [],
                }
            } else {
                var toolbarOptions = obj.options.toolbar;
            }

            // Default items
            if (! (toolbarOptions.items && toolbarOptions.items.length)) {
                toolbarOptions.items = Component.getDefaultToolbar(obj);
            }

            if (obj.options.toolbarOnTop) {
                // Add class
                el.classList.add('toolbar-on-top');
                // Append to the DOM
                el.insertBefore(toolbar, el.firstChild);
            } else {
                // Add padding to the editor
                obj.editor.style.padding = '15px';
                // Append to the DOM
                el.appendChild(toolbar);
            }

            // Create toolbar
            Toolbar(toolbar, toolbarOptions);

            toolbar.addEventListener('click', function() {
                obj.editor.focus();
            })
        }

        // Upload file
        obj.file = document.createElement('input');
        obj.file.style.display = 'none';
        obj.file.type = 'file';
        obj.file.setAttribute('accept', 'image/*');
        obj.file.onchange = function() {
            obj.addFile(this.files);
        }
        el.appendChild(obj.file);

        // Focus to the editor
        if (obj.options.focus) {
            Component.setCursor(obj.editor, obj.options.focus == 'initial' ? true : false);
        }

        // Change method
        el.change = obj.setData;

        // Global generic value handler
        el.val = function(val) {
            if (val === undefined) {
                // Data type
                var o = el.getAttribute('data-html') === 'true' ? false : true;
                return obj.getData(o);
            } else {
                obj.setData(val);
            }
        }

        loadExtensions();

        el.editor = obj;

        // Onload
        if (typeof(obj.options.onload) == 'function') {
            obj.options.onload(el, obj, obj.editor);
        }

        return obj;
    });

    Component.setCursor = function(element, first) {
        element.focus();
        document.execCommand('selectAll');
        var sel = window.getSelection();
        var range = sel.getRangeAt(0);
        if (first == true) {
            var node = range.startContainer;
            var size = 0;
        } else {
            var node = range.endContainer;
            var size = node.length;
        }
        range.setStart(node, size);
        range.setEnd(node, size);
        sel.removeAllRanges();
        sel.addRange(range);
    }

    Component.getDefaultToolbar = function(obj) {

        var color = function(a,b,c) {
            if (! c.color) {
                var t = null;
                var colorPicker = Color(c, {
                    onchange: function(o, v) {
                        if (c.k === 'color') {
                            document.execCommand('foreColor', false, v);
                        } else {
                            document.execCommand('backColor', false, v);
                        }
                    }
                });
                c.color.open();
            }
        }

        var items = [];

        items.push({
            content: 'undo',
            onclick: function() {
                document.execCommand('undo');
            }
        });

        items.push({
            content: 'redo',
            onclick: function() {
                document.execCommand('redo');
            }
        });

        items.push({
            type: 'divisor'
        });

        if (obj.options.toolbarOnTop) {
            items.push({
                type: 'select',
                width: '140px',
                options: ['Default', 'Verdana', 'Arial', 'Courier New'],
                render: function (e) {
                    return '<span style="font-family:' + e + '">' + e + '</span>';
                },
                onchange: function (a,b,c,d,e) {
                    document.execCommand("fontName", false, d);
                }
            });

            items.push({
                type: 'select',
                content: 'format_size',
                options: ['x-small', 'small', 'medium', 'large', 'x-large'],
                render: function (e) {
                    return '<span style="font-size:' + e + '">' + e + '</span>';
                },
                onchange: function (a,b,c,d,e) {
                    //var html = `<span style="font-size: ${c}">${text}</span>`;
                    //document.execCommand('insertHtml', false, html);
                    document.execCommand("fontSize", false, parseInt(e)+1);
                    //var f = window.getSelection().anchorNode.parentNode
                    //f.removeAttribute("size");
                    //f.style.fontSize = d;
                }
            });

            items.push({
                type: 'select',
                options: ['format_align_left', 'format_align_center', 'format_align_right', 'format_align_justify'],
                render: function (e) {
                    return '<i class="material-icons">' + e + '</i>';
                },
                onchange: function (a,b,c,d,e) {
                    var options = ['JustifyLeft','justifyCenter','justifyRight','justifyFull'];
                    document.execCommand(options[e]);
                }
            });

            items.push({
                type: 'divisor'
            });

            items.push({
                content: 'format_color_text',
                k: 'color',
                onclick: color,
            });

            items.push({
                content: 'format_color_fill',
                k: 'background-color',
                onclick: color,
            });
        }

        items.push({
            content: 'format_bold',
            onclick: function(a,b,c) {
                document.execCommand('bold');

                if (document.queryCommandState("bold")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            content: 'format_italic',
            onclick: function(a,b,c) {
                document.execCommand('italic');

                if (document.queryCommandState("italic")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            content: 'format_underline',
            onclick: function(a,b,c) {
                document.execCommand('underline');

                if (document.queryCommandState("underline")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            type:'divisor'
        });

        items.push({
            content: 'format_list_bulleted',
            onclick: function(a,b,c) {
                document.execCommand('insertUnorderedList');

                if (document.queryCommandState("insertUnorderedList")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            content: 'format_list_numbered',
            onclick: function(a,b,c) {
                document.execCommand('insertOrderedList');

                if (document.queryCommandState("insertOrderedList")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            content: 'format_indent_increase',
            onclick: function(a,b,c) {
                document.execCommand('indent', true, null);

                if (document.queryCommandState("indent")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        items.push({
            content: 'format_indent_decrease',
            onclick: function(a,b,c) {
                document.execCommand('outdent');

                if (document.queryCommandState("outdent")) {
                    c.classList.add('selected');
                } else {
                    c.classList.remove('selected');
                }
            }
        });

        if (obj.options.toolbarOnTop) {
            items.push({
                type: 'divisor'
            });

            items.push({
                content: 'photo',
                onclick: function () {
                    obj.upload();
                }
            });

            items.push({
                type: 'select',
                content: 'table_view',
                columns: 8,
                grid: 8,
                right: true,
                options: [
                    '0x0', '1x0', '2x0', '3x0', '4x0', '5x0', '6x0', '7x0',
                    '0x1', '1x1', '2x1', '3x1', '4x1', '5x1', '6x1', '7x1',
                    '0x2', '1x2', '2x2', '3x2', '4x2', '5x2', '6x2', '7x2',
                    '0x3', '1x3', '2x3', '3x3', '4x3', '5x3', '6x3', '7x3',
                    '0x4', '1x4', '2x4', '3x4', '4x4', '5x4', '6x4', '7x4',
                    '0x5', '1x5', '2x5', '3x5', '4x5', '5x5', '6x5', '7x5',
                    '0x6', '1x6', '2x6', '3x6', '4x6', '5x6', '6x6', '7x6',
                    '0x7', '1x7', '2x7', '3x7', '4x7', '5x7', '6x7', '7x7',
                ],
                render: function (e, item) {
                    if (item) {
                        item.onmouseover = this.onmouseover;
                        e = e.split('x');
                        item.setAttribute('data-x', e[0]);
                        item.setAttribute('data-y', e[1]);
                    }
                    var element = document.createElement('div');
                    item.style.margin = '1px';
                    item.style.border = '1px solid #ddd';
                    return element;
                },
                onmouseover: function (e) {
                    var x = parseInt(e.target.getAttribute('data-x'));
                    var y = parseInt(e.target.getAttribute('data-y'));
                    for (var i = 0; i < e.target.parentNode.children.length; i++) {
                        var element = e.target.parentNode.children[i];
                        var ex = parseInt(element.getAttribute('data-x'));
                        var ey = parseInt(element.getAttribute('data-y'));
                        if (ex <= x && ey <= y) {
                            element.style.backgroundColor = '#cae1fc';
                            element.style.borderColor = '#2977ff';
                        } else {
                            element.style.backgroundColor = '';
                            element.style.borderColor = '#ddd';
                        }
                    }
                },
                onchange: function (a, b, c) {
                    c = c.split('x');
                    var table = document.createElement('table');
                    var tbody = document.createElement('tbody');
                    for (var y = 0; y <= c[1]; y++) {
                        var tr = document.createElement('tr');
                        for (var x = 0; x <= c[0]; x++) {
                            var td = document.createElement('td');
                            td.innerHTML = '';
                            tr.appendChild(td);
                        }
                        tbody.appendChild(tr);
                    }
                    table.appendChild(tbody);
                    table.setAttribute('width', '100%');
                    table.setAttribute('cellpadding', '6');
                    table.setAttribute('cellspacing', '0');
                    document.execCommand('insertHTML', false, table.outerHTML);
                }
            });
        }

        return items;
    }

    return Component;
}

/* harmony default export */ var editor = (Editor());

;// CONCATENATED MODULE: ./src/plugins/floating.js
function Floating() {
    var Component = (function (el, options) {
        var obj = {};
        obj.options = {};

        // Default configuration
        var defaults = {
            type: 'big',
            title: 'Untitled',
            width: 510,
            height: 472,
        }

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        // Private methods

        var setContent = function () {
            var temp = document.createElement('div');
            while (el.children[0]) {
                temp.appendChild(el.children[0]);
            }

            obj.content = document.createElement('div');
            obj.content.className = 'jfloating_content';
            obj.content.innerHTML = el.innerHTML;

            while (temp.children[0]) {
                obj.content.appendChild(temp.children[0]);
            }

            obj.container = document.createElement('div');
            obj.container.className = 'jfloating';
            obj.container.appendChild(obj.content);

            if (obj.options.title) {
                obj.container.setAttribute('title', obj.options.title);
            } else {
                obj.container.classList.add('no-title');
            }

            // validate element dimensions
            if (obj.options.width) {
                obj.container.style.width = parseInt(obj.options.width) + 'px';
            }

            if (obj.options.height) {
                obj.container.style.height = parseInt(obj.options.height) + 'px';
            }

            el.innerHTML = '';
            el.appendChild(obj.container);
        }

        var setEvents = function () {
            if (obj.container) {
                obj.container.addEventListener('click', function (e) {
                    var rect = e.target.getBoundingClientRect();

                    if (e.target.classList.contains('jfloating')) {
                        if (e.changedTouches && e.changedTouches[0]) {
                            var x = e.changedTouches[0].clientX;
                            var y = e.changedTouches[0].clientY;
                        } else {
                            var x = e.clientX;
                            var y = e.clientY;
                        }

                        if (rect.width - (x - rect.left) < 50 && (y - rect.top) < 50) {
                            setTimeout(function () {
                                obj.close();
                            }, 100);
                        } else {
                            obj.setState();
                        }
                    }
                });
            }
        }

        var setType = function () {
            obj.container.classList.add('jfloating-' + obj.options.type);
        }

        obj.state = {
            isMinized: false,
        }

        obj.setState = function () {
            if (obj.state.isMinized) {
                obj.container.classList.remove('jfloating-minimized');
            } else {
                obj.container.classList.add('jfloating-minimized');
            }
            obj.state.isMinized = !obj.state.isMinized;
        }

        obj.close = function () {
            Components.elements.splice(Component.elements.indexOf(obj.container), 1);
            obj.updatePosition();
            el.remove();
        }

        obj.updatePosition = function () {
            for (var i = 0; i < Component.elements.length; i++) {
                var floating = Component.elements[i];
                var prevFloating = Component.elements[i - 1];
                floating.style.right = i * (prevFloating ? prevFloating.offsetWidth : floating.offsetWidth) * 1.01 + 'px';
            }
        }

        obj.init = function () {
            // Set content into root
            setContent();

            // Set dialog events
            setEvents();

            // Set dialog type
            setType();

            // Update floating position
            Component.elements.push(obj.container);
            obj.updatePosition();

            el.floating = obj;
        }

        obj.init();

        return obj;
    });

    Component.elements = [];

    return Component;
}

/* harmony default export */ var floating = (Floating());
;// CONCATENATED MODULE: ./src/plugins/validations.js


function Validations() {
    /**
     * Options: Object,
     * Properties:
     * Constraint,
     * Reference,
     * Value
     */

    const isNumeric = function(num) {
        return !isNaN(num) && num !== null && (typeof num !== 'string' || num.trim() !== '');
    }

    const numberCriterias = {
        'between': function(value, range) {
            return value >= range[0] && value <= range[1];
        },
        'not between': function(value, range) {
            return value < range[0] || value > range[1];
        },
        '<': function(value, range) {
            return value < range[0];
        },
        '<=': function(value, range) {
            return value <= range[0];
        },
        '>': function(value, range) {
            return value > range[0];
        },
        '>=': function(value, range) {
            return value >= range[0];
        },
        '=': function(value, range) {
            return value == range[0];
        },
        '!=': function(value, range) {
            return value != range[0];
        },
    }

    const dateCriterias = {
        'valid date': function() {
            return true;
        },
        '=': function(value, range) {
            return value === range[0];
        },
        '!=': function(value, range) {
            return value !== range[0];
        },
        '<': function(value, range) {
            return value < range[0];
        },
        '<=': function(value, range) {
            return value <= range[0];
        },
        '>': function(value, range) {
            return value > range[0];
        },
        '>=': function(value, range) {
            return value >= range[0];
        },
        'between': function(value, range) {
            return value >= range[0] && value <= range[1];
        },
        'not between': function(value, range) {
            return value < range[0] || value > range[1];
        },
    }

    const textCriterias = {
        'contains': function(value, range) {
            return value.includes(range[0]);
        },
        'not contains': function(value, range) {
            return !value.includes(range[0]);
        },
        'begins with': function(value, range) {
            return value.startsWith(range[0]);
        },
        'ends with': function(value, range) {
            return value.endsWith(range[0]);
        },
        '=': function(value, range) {
            return value === range[0];
        },
        '!=': function(value, range) {
            return value !== range[0];
        },
        'valid email': function(value) {
            var pattern = new RegExp(/^[^\s@]+@[^\s@]+\.[^\s@]+$/);

            return pattern.test(value);
        },
        'valid url': function(value) {
            var pattern = new RegExp(/(((https?:\/\/)|(www\.))[-A-Z0-9+&@#\/%?=~_|!:,.;]*[-A-Z0-9+&@#\/%=~_|]+)/ig);

            return pattern.test(value);
        },
    }

    // Component router
    const component = function(value, options) {
        if (typeof(component[options.type]) === 'function') {
            if (options.allowBlank && (typeof value === 'undefined' || value === '' || value === null)) {
                return true;
            }
            return component[options.type].call(this, value, options);
        }
        return null;
    }
    
    component.url = function(data) {
        var pattern = new RegExp(/(((https?:\/\/)|(www\.))[-A-Z0-9+&@#\/%?=~_|!:,.;]*[-A-Z0-9+&@#\/%=~_|]+)/ig);
        return pattern.test(data) ? true : false;
    }

    component.email = function(data) {
        var pattern = new RegExp(/^[^\s@]+@[^\s@]+\.[^\s@]+$/);
        return data && pattern.test(data) ? true : false;
    }
    
    component.required = function(data) {
        return data && data.trim() ? true : false;
    }

    component.empty = function(data) {
        return typeof data === 'undefined' || data === null || (typeof data === 'string' && !data.toString().trim());
    }

    component['not exist'] = component.empty;

    component.notEmpty = function(data) {
        return !component.empty(data);
    }

    component.exist = component.notEmpty;

    component.number = function(data, options) {
       if (! isNumeric(data)) {
           return false;
       }

       if (!options || !options.criteria) {
           return true;
       }

       if (!numberCriterias[options.criteria]) {
           return false;
       }

       let values = options.value.map(function(num) {
          return parseFloat(num);
       })

       return numberCriterias[options.criteria](data, values);
   };

    component.login = function(data) {
        let pattern = new RegExp(/^[a-zA-Z0-9._-]+$/);
        return data && pattern.test(data) ? true : false;
    }

    component.list = function(data, options) {
        let dataType = typeof data;
        if (dataType !== 'string' && dataType !== 'number') {
            return false;
        }
        let list;
        if (typeof(options.value[0]) === 'string') {
            if (options.source) {
                list = options.source;
            } else {
                list = options.value[0].split(',');
            }
        } else {
            list = options.value[0];
        }

        if (! Array.isArray(list)) {
            return false;
        } else {
            let validOption = list.findIndex(function (item) {
                return item == data;
            });

            return validOption > -1;
        }
    }

    const getCurrentDateWithoutTime = function() {
        let date = new Date();
        date.setHours(0, 0, 0, 0);
        return date;
    }

    const relativeDates = {
        'one year ago': function() {
            let date = getCurrentDateWithoutTime();

            date.setFullYear(date.getFullYear() - 1);

            return date;
        },
        'one month ago': function() {
            let date = getCurrentDateWithoutTime();

            date.setMonth(date.getMonth() - 1);

            return date;
        },
        'one week ago': function() {
            let date = getCurrentDateWithoutTime();

            date.setDate(date.getDate() - 7);

            return date;
        },
        yesterday: function() {
            let date = getCurrentDateWithoutTime();

            date.setDate(date.getDate() - 1);

            return date;
        },
        today: getCurrentDateWithoutTime,
        tomorrow: function() {
            let date = getCurrentDateWithoutTime();

            date.setDate(date.getDate() + 1);

            return date;
        },
    };

    component.date = function(data, options) {
        if (isNumeric(data) && data > 0 && data < 1000000) {
            data = helpers_date.numToDate(data);
        }

        if (new Date(data) == 'Invalid Date') {
            return false;
        }

        if (!options || !options.criteria) {
            return true;
        }

        if (!dateCriterias[options.criteria]) {
            return false;
        }

        let values = options.value.map(function(date) {
            if (typeof date === 'string' && relativeDates[date]) {
                return relativeDates[date]().getTime();
            }

            return new Date(date).getTime();
        });

        return dateCriterias[options.criteria](new Date(data).getTime(), values);
    }

    component.text = function(data, options) {
        if (typeof data === 'undefined' || data === null) {
            data = '';
        } else if (typeof data !== 'string') {
            return false;
        }

        if (!options || !options.criteria) {
            return true;
        }

        if (!textCriterias[options.criteria]) {
            return false;
        }

        return textCriterias[options.criteria](data, options.value);
    }

    component.textLength = function(data, options) {
        let textLength;
        if (typeof data === 'string') {
            textLength = data.length;
        } else if (typeof data !== 'undefined' && data !== null && typeof data.toString === 'function') {
            textLength = data.toString().length;
        } else {
            textLength = 0;
        }

        return component.number(textLength, options);
    }

    component.time = function(data, options) {
       if (! isNumeric(data)) {
           return false;
       }

       if (!options || !options.criteria) {
           return true;
       }

       if (!numberCriterias[options.criteria]) {
           return false;
       }

       let values = options.value.map(function(num) {
          return parseInt(parseFloat(num) * 10**17) / 10**17;
       })

       return numberCriterias[options.criteria](parseInt(parseFloat(data) * 10**17) / 10**17, values);
   };

    return component;
}

/* harmony default export */ var validations = (Validations());
;// CONCATENATED MODULE: ./src/plugins/form.js




function Form() {
    var Component = (function(el, options) {
        var obj = {};
        obj.options = {};

        // Default configuration
        var defaults = {
            url: null,
            message: 'Are you sure? There are unsaved information in your form',
            ignore: false,
            currentHash: null,
            submitButton:null,
            validations: null,
            onbeforeload: null,
            onload: null,
            onbeforesave: null,
            onsave: null,
            onbeforeremove: null,
            onremove: null,
            onerror: function(el, message) {
                alert(message);
            }
        };

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        // Validations
        if (! obj.options.validations) {
            obj.options.validations = {};
        }

        // Submit Button
        if (! obj.options.submitButton) {
            obj.options.submitButton = el.querySelector('input[type=submit]');
        }

        if (obj.options.submitButton && obj.options.url) {
            obj.options.submitButton.onclick = function() {
                obj.save();
            }
        }

        if (! obj.options.validations.email) {
            obj.options.validations.email = validations.email;
        }

        if (! obj.options.validations.length) {
            obj.options.validations.length = validations.length;
        }

        if (! obj.options.validations.required) {
            obj.options.validations.required = validations.required;
        }

        obj.setUrl = function(url) {
            obj.options.url = url;
        }

        obj.load = function() {
            ajax({
                url: obj.options.url,
                method: 'GET',
                dataType: 'json',
                queue: true,
                success: function(data) {
                    // Overwrite values from the backend
                    if (typeof(obj.options.onbeforeload) == 'function') {
                        var ret = obj.options.onbeforeload(el, data);
                        if (ret) {
                            data = ret;
                        }
                    }
                    // Apply values to the form
                    Component.setElements(el, data);
                    // Onload methods
                    if (typeof(obj.options.onload) == 'function') {
                        obj.options.onload(el, data);
                    }
                }
            });
        }

        obj.save = function() {
            var test = obj.validate();

            if (test) {
                obj.options.onerror(el, test);
            } else {
                var data = Component.getElements(el, true);

                if (typeof(obj.options.onbeforesave) == 'function') {
                    var data = obj.options.onbeforesave(el, data);

                    if (data === false) {
                        return;
                    }
                }

                ajax({
                    url: obj.options.url,
                    method: 'POST',
                    dataType: 'json',
                    data: data,
                    success: function(result) {
                        if (typeof(obj.options.onsave) == 'function') {
                            obj.options.onsave(el, data, result);
                        }
                    }
                });
            }
        }

        obj.remove = function() {
            if (typeof(obj.options.onbeforeremove) == 'function') {
                var ret = obj.options.onbeforeremove(el, obj);
                if (ret === false) {
                    return false;
                }
            }

            ajax({
                url: obj.options.url,
                method: 'DELETE',
                dataType: 'json',
                success: function(result) {
                    if (typeof(obj.options.onremove) == 'function') {
                        obj.options.onremove(el, obj, result);
                    }

                    obj.reset();
                }
            });
        }

        var addError = function(element) {
            // Add error in the element
            element.classList.add('error');
            // Submit button
            if (obj.options.submitButton) {
                obj.options.submitButton.setAttribute('disabled', true);
            }
            // Return error message
            var error = element.getAttribute('data-error') || 'There is an error in the form';
            element.setAttribute('title', error);
            return error;
        }

        var delError = function(element) {
            var error = false;
            // Remove class from this element
            element.classList.remove('error');
            element.removeAttribute('title');
            // Get elements in the form
            var elements = el.querySelectorAll("input, select, textarea, div[name]");
            // Run all elements
            for (var i = 0; i < elements.length; i++) {
                if (elements[i].getAttribute('data-validation')) {
                    if (elements[i].classList.contains('error')) {
                        error = true;
                    }
                }
            }

            if (obj.options.submitButton) {
                if (error) {
                    obj.options.submitButton.setAttribute('disabled', true);
                } else {
                    obj.options.submitButton.removeAttribute('disabled');
                }
            }
        }

        obj.validateElement = function(element) {
            // Test results
            var test = false;
            // Value
            var value = Component.getValue(element);
            // Validation
            var validation = element.getAttribute('data-validation');
            // Parse
            if (typeof(obj.options.validations[validation]) == 'function' && ! obj.options.validations[validation](value, element)) {
                // Not passed in the test
                test = addError(element);
            } else {
                if (element.classList.contains('error')) {
                    delError(element);
                }
            }

            return test;
        }

        obj.reset = function() {
            // Get elements in the form
            var name = null;
            var elements = el.querySelectorAll("input, select, textarea, div[name]");
            // Run all elements
            for (var i = 0; i < elements.length; i++) {
                if (name = elements[i].getAttribute('name')) {
                    if (elements[i].type == 'checkbox' || elements[i].type == 'radio') {
                        elements[i].checked = false;
                    } else {
                        if (typeof(elements[i].val) == 'function') {
                            elements[i].val('');
                        } else {
                            elements[i].value = '';
                        }
                    }
                }
            }
        }

        // Run form validation
        obj.validate = function() {
            var test = [];
            // Get elements in the form
            var elements = el.querySelectorAll("input, select, textarea, div[name]");
            // Run all elements
            for (var i = 0; i < elements.length; i++) {
                // Required
                if (elements[i].getAttribute('data-validation')) {
                    var res = obj.validateElement(elements[i]);
                    if (res) {
                        test.push(res);
                    }
                }
            }
            if (test.length > 0) {
                return test.join('<br>');
            } else {
                return false;
            }
        }

        // Check the form
        obj.getError = function() {
            // Validation
            return obj.validation() ? true : false;
        }

        // Return the form hash
        obj.setHash = function() {
            return obj.getHash(Component.getElements(el));
        }

        // Get the form hash
        obj.getHash = function(str) {
            var hash = 0, i, chr;

            if (str.length === 0) {
                return hash;
            } else {
                for (i = 0; i < str.length; i++) {
                  chr = str.charCodeAt(i);
                  hash = ((hash << 5) - hash) + chr;
                  hash |= 0;
                }
            }

            return hash;
        }

        // Is there any change in the form since start tracking?
        obj.isChanged = function() {
            var hash = obj.setHash();
            return (obj.options.currentHash != hash);
        }

        // Restart tracking
        obj.resetTracker = function() {
            obj.options.currentHash = obj.setHash();
            obj.options.ignore = false;
        }

        // Ignore flag
        obj.setIgnore = function(ignoreFlag) {
            obj.options.ignore = ignoreFlag ? true : false;
        }

        // Start tracking in one second
        setTimeout(function() {
            obj.options.currentHash = obj.setHash();
        }, 1000);

        // Validations
        el.addEventListener("keyup", function(e) {
            if (e.target.getAttribute('data-validation')) {
                obj.validateElement(e.target);
            }
        });

        // Alert
        if (! Component.hasEvents) {
            window.addEventListener("beforeunload", function (e) {
                if (obj.isChanged() && obj.options.ignore == false) {
                    var confirmationMessage =  obj.options.message? obj.options.message : "\o/";

                    if (confirmationMessage) {
                        if (typeof e == 'undefined') {
                            e = window.event;
                        }

                        if (e) {
                            e.returnValue = confirmationMessage;
                        }

                        return confirmationMessage;
                    } else {
                        return void(0);
                    }
                }
            });

            Component.hasEvents = true;
        }

        el.form = obj;

        return obj;
    });

    // Get value from one element
    Component.getValue = function(element) {
        var value = null;
        if (element.type == 'checkbox') {
            if (element.checked == true) {
                value = element.value || true;
            }
        } else if (element.type == 'radio') {
            if (element.checked == true) {
                value = element.value;
            }
        } else if (element.type == 'file') {
            value = element.files;
        } else if (element.tagName == 'select' && element.multiple == true) {
            value = [];
            var options = element.querySelectorAll("options[selected]");
            for (var j = 0; j < options.length; j++) {
                value.push(options[j].value);
            }
        } else if (typeof(element.val) == 'function') {
            value = element.val();
        } else {
            value = element.value || '';
        }

        return value;
    }

    // Get form elements
    Component.getElements = function(el, asArray) {
        var data = {};
        var name = null;
        var elements = el.querySelectorAll("input, select, textarea, div[name]");

        for (var i = 0; i < elements.length; i++) {
            if (name = elements[i].getAttribute('name')) {
                data[name] = Component.getValue(elements[i]) || '';
            }
        }

        return asArray == true ? data : JSON.stringify(data);
    }

    //Get form elements
    Component.setElements = function(el, data) {
        var name = null;
        var value = null;
        var elements = el.querySelectorAll("input, select, textarea, div[name]");
        for (var i = 0; i < elements.length; i++) {
            // Attributes
            var type = elements[i].getAttribute('type');
            if (name = elements[i].getAttribute('name')) {
                // Transform variable names in pathname
                name = name.replace(new RegExp(/\[(.*?)\]/ig), '.$1');
                value = null;
                // Seach for the data in the path
                if (name.match(/\./)) {
                    var tmp = Path.call(data, name) || '';
                    if (typeof(tmp) !== 'undefined') {
                        value = tmp;
                    }
                } else {
                    if (typeof(data[name]) !== 'undefined') {
                        value = data[name];
                    }
                }
                // Set the values
                if (value !== null) {
                    if (type == 'checkbox' || type == 'radio') {
                        elements[i].checked = value ? true : false;
                    } else if (type == 'file') {
                        // Do nothing
                    } else {
                        if (typeof (elements[i].val) == 'function') {
                            elements[i].val(value);
                        } else {
                            elements[i].value = value;
                        }
                    }
                }
            }
        }
    }

    return Component;
}

/* harmony default export */ var plugins_form = (Form());
;// CONCATENATED MODULE: ./src/plugins/modal.js




function Modal() {

    var Events = function() {
        //  Position
        var tracker = null;

        var keyDown = function (e) {
            if (e.which == 27) {
                var modals = document.querySelectorAll('.jmodal');
                for (var i = 0; i < modals.length; i++) {
                    modals[i].parentNode.modal.close();
                }
            }
        }

        var mouseUp = function (e) {
            let element = e.composedPath();
            var item = helpers.findElement(element[0], 'jmodal');
            if (item) {
                // Get target info
                var rect = item.getBoundingClientRect();

                if (e.changedTouches && e.changedTouches[0]) {
                    var x = e.changedTouches[0].clientX;
                    var y = e.changedTouches[0].clientY;
                } else {
                    var x = e.clientX;
                    var y = e.clientY;
                }

                if (rect.width - (x - rect.left) < 50 && (y - rect.top) < 50) {
                    item.parentNode.modal.close();
                }
            }

            if (tracker) {
                tracker.element.style.cursor = 'auto';
                tracker = null;
            }
        }

        var mouseDown = function (e) {
            let element = e.composedPath();
            var item = helpers.findElement(element[0], 'jmodal');
            if (item) {
                // Get target info
                var rect = item.getBoundingClientRect();

                if (e.changedTouches && e.changedTouches[0]) {
                    var x = e.changedTouches[0].clientX;
                    var y = e.changedTouches[0].clientY;
                } else {
                    var x = e.clientX;
                    var y = e.clientY;
                }

                if (rect.width - (x - rect.left) < 50 && (y - rect.top) < 50) {
                    // Do nothing
                } else {
                    if (y - rect.top < 50) {
                        if (document.selection) {
                            document.selection.empty();
                        } else if (window.getSelection) {
                            window.getSelection().removeAllRanges();
                        }

                        tracker = {
                            left: rect.left,
                            top: rect.top,
                            x: e.clientX,
                            y: e.clientY,
                            width: rect.width,
                            height: rect.height,
                            element: item,
                        }
                    }
                }
            }
        }

        var mouseMove = function (e) {
            if (tracker) {
                e = e || window.event;
                if (e.buttons) {
                    var mouseButton = e.buttons;
                } else if (e.button) {
                    var mouseButton = e.button;
                } else {
                    var mouseButton = e.which;
                }

                if (mouseButton) {
                    tracker.element.style.top = (tracker.top + (e.clientY - tracker.y) + (tracker.height / 2)) + 'px';
                    tracker.element.style.left = (tracker.left + (e.clientX - tracker.x) + (tracker.width / 2)) + 'px';
                    tracker.element.style.cursor = 'move';
                } else {
                    tracker.element.style.cursor = 'auto';
                }
            }
        }

        document.addEventListener('keydown', keyDown);
        document.addEventListener('mouseup', mouseUp);
        document.addEventListener('mousedown', mouseDown);
        document.addEventListener('mousemove', mouseMove);
    }

    var Component = (function (el, options) {
        var obj = {};
        obj.options = {};

        // Default configuration
        var defaults = {
            url: null,
            onopen: null,
            onclose: null,
            onload: null,
            closed: false,
            width: null,
            height: null,
            title: null,
            padding: null,
            backdrop: true,
            icon: null,
        };

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        // Title
        if (!obj.options.title && el.getAttribute('title')) {
            obj.options.title = el.getAttribute('title');
        }

        var temp = document.createElement('div');
        while (el.children[0]) {
            temp.appendChild(el.children[0]);
        }

        obj.title = document.createElement('div');
        obj.title.className = 'jmodal_title';
        if (obj.options.icon) {
            obj.title.setAttribute('data-icon', obj.options.icon);
        }

        obj.content = document.createElement('div');
        obj.content.className = 'jmodal_content';
        obj.content.innerHTML = el.innerHTML;

        while (temp.children[0]) {
            obj.content.appendChild(temp.children[0]);
        }

        obj.container = document.createElement('div');
        obj.container.className = 'jmodal';
        obj.container.appendChild(obj.title);
        obj.container.appendChild(obj.content);

        if (obj.options.padding) {
            obj.content.style.padding = obj.options.padding;
        }
        if (obj.options.width) {
            obj.container.style.width = obj.options.width;
        }
        if (obj.options.height) {
            obj.container.style.height = obj.options.height;
        }
        if (obj.options.title) {
            var title = document.createElement('h4');
            title.innerText = obj.options.title;
            obj.title.appendChild(title);
        }

        el.innerHTML = '';
        el.style.display = 'none';
        el.appendChild(obj.container);

        // Backdrop
        if (obj.options.backdrop) {
            var backdrop = document.createElement('div');
            backdrop.className = 'jmodal_backdrop';
            backdrop.onclick = function () {
                obj.close();
            }
            el.appendChild(backdrop);
        }

        obj.open = function () {
            el.style.display = 'block';
            // Fullscreen
            var rect = obj.container.getBoundingClientRect();
            if (helpers.getWindowWidth() < rect.width) {
                obj.container.style.top = '';
                obj.container.style.left = '';
                obj.container.classList.add('jmodal_fullscreen');
                animation.slideBottom(obj.container, 1);
            } else {
                if (obj.options.backdrop) {
                    backdrop.style.display = 'block';
                }
            }
            // Event
            if (typeof (obj.options.onopen) == 'function') {
                obj.options.onopen(el, obj);
            }
        }

        obj.resetPosition = function () {
            obj.container.style.top = '';
            obj.container.style.left = '';
        }

        obj.isOpen = function () {
            return el.style.display != 'none' ? true : false;
        }

        obj.close = function () {
            if (obj.isOpen()) {
                el.style.display = 'none';
                if (obj.options.backdrop) {
                    // Backdrop
                    backdrop.style.display = '';
                }
                // Remove fullscreen class
                obj.container.classList.remove('jmodal_fullscreen');
                // Event
                if (typeof (obj.options.onclose) == 'function') {
                    obj.options.onclose(el, obj);
                }
            }
        }

        if (obj.options.url) {
            ajax({
                url: obj.options.url,
                method: 'GET',
                dataType: 'text/html',
                success: function (data) {
                    obj.content.innerHTML = data;

                    if (!obj.options.closed) {
                        obj.open();
                    }

                    if (typeof (obj.options.onload) === 'function') {
                        obj.options.onload(obj);
                    }
                }
            });
        } else {
            if (!obj.options.closed) {
                obj.open();
            }

            if (typeof (obj.options.onload) === 'function') {
                obj.options.onload(obj);
            }
        }

        // Keep object available from the node
        el.modal = obj;

        // Create events when the first modal is create only
        Events();

        // Execute the events only once
        Events = function() {};

        return obj;
    });

    return Component;
}

/* harmony default export */ var modal = (Modal());
;// CONCATENATED MODULE: ./src/plugins/notification.js



function Notification() {
    var Component = function (options) {
        var obj = {};
        obj.options = {};

        // Default configuration
        var defaults = {
            icon: null,
            name: 'Notification',
            date: null,
            error: null,
            title: null,
            message: null,
            timeout: 4000,
            autoHide: true,
            closeable: true,
        };

        // Loop through our object
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                obj.options[property] = defaults[property];
            }
        }

        var notification = document.createElement('div');
        notification.className = 'jnotification';

        if (obj.options.error) {
            notification.classList.add('jnotification-error');
        }

        var notificationContainer = document.createElement('div');
        notificationContainer.className = 'jnotification-container';
        notification.appendChild(notificationContainer);

        var notificationHeader = document.createElement('div');
        notificationHeader.className = 'jnotification-header';
        notificationContainer.appendChild(notificationHeader);

        var notificationImage = document.createElement('div');
        notificationImage.className = 'jnotification-image';
        notificationHeader.appendChild(notificationImage);

        if (obj.options.icon) {
            var notificationIcon = document.createElement('img');
            notificationIcon.src = obj.options.icon;
            notificationImage.appendChild(notificationIcon);
        }

        var notificationName = document.createElement('div');
        notificationName.className = 'jnotification-name';
        notificationName.innerHTML = obj.options.name;
        notificationHeader.appendChild(notificationName);

        if (obj.options.closeable == true) {
            var notificationClose = document.createElement('div');
            notificationClose.className = 'jnotification-close';
            notificationClose.onclick = function () {
                obj.hide();
            }
            notificationHeader.appendChild(notificationClose);
        }

        var notificationDate = document.createElement('div');
        notificationDate.className = 'jnotification-date';
        notificationHeader.appendChild(notificationDate);

        var notificationContent = document.createElement('div');
        notificationContent.className = 'jnotification-content';
        notificationContainer.appendChild(notificationContent);

        if (obj.options.title) {
            var notificationTitle = document.createElement('div');
            notificationTitle.className = 'jnotification-title';
            notificationTitle.innerHTML = obj.options.title;
            notificationContent.appendChild(notificationTitle);
        }

        var notificationMessage = document.createElement('div');
        notificationMessage.className = 'jnotification-message';
        notificationMessage.innerHTML = obj.options.message;
        notificationContent.appendChild(notificationMessage);

        obj.show = function () {
            document.body.appendChild(notification);
            if (helpers.getWindowWidth() > 800) {
                animation.fadeIn(notification);
            } else {
                animation.slideTop(notification, 1);
            }
        }

        obj.hide = function () {
            if (helpers.getWindowWidth() > 800) {
                animation.fadeOut(notification, function () {
                    if (notification.parentNode) {
                        notification.parentNode.removeChild(notification);
                        if (notificationTimeout) {
                            clearTimeout(notificationTimeout);
                        }
                    }
                });
            } else {
                animation.slideTop(notification, 0, function () {
                    if (notification.parentNode) {
                        notification.parentNode.removeChild(notification);
                        if (notificationTimeout) {
                            clearTimeout(notificationTimeout);
                        }
                    }
                });
            }
        };

        obj.show();

        if (obj.options.autoHide == true) {
            var notificationTimeout = setTimeout(function () {
                obj.hide();
            }, obj.options.timeout);
        }

        if (helpers.getWindowWidth() < 800) {
            notification.addEventListener("swipeup", function (e) {
                obj.hide();
                e.preventDefault();
                e.stopPropagation();
            });
        }

        return obj;
    }

    Component.isVisible = function () {
        var j = document.querySelector('.jnotification');
        return j && j.parentNode ? true : false;
    }

    return Component;
}

/* harmony default export */ var notification = (Notification());
;// CONCATENATED MODULE: ./src/plugins/progressbar.js
function Progressbar(el, options) {
    var obj = {};
    obj.options = {};

    // Default configuration
    var defaults = {
        value: 0,
        onchange: null,
        width: null,
    };

    // Loop through the initial configuration
    for (var property in defaults) {
        if (options && options.hasOwnProperty(property)) {
            obj.options[property] = options[property];
        } else {
            obj.options[property] = defaults[property];
        }
    }

    // Class
    el.classList.add('jprogressbar');
    el.setAttribute('tabindex', 1);
    el.setAttribute('data-value', obj.options.value);

    var bar = document.createElement('div');
    bar.style.width = obj.options.value + '%';
    bar.style.color = '#fff';
    el.appendChild(bar);

    if (obj.options.width) {
        el.style.width = obj.options.width;
    }

    // Set value
    obj.setValue = function(value) {
        value = parseInt(value);
        obj.options.value = value;
        bar.style.width = value + '%';
        el.setAttribute('data-value', value + '%');

        if (value < 6) {
            el.style.color = '#000';
        } else {
            el.style.color = '#fff';
        }

        // Update value
        obj.options.value = value;

        if (typeof(obj.options.onchange) == 'function') {
            obj.options.onchange(el, value);
        }

        // Lemonade JS
        if (el.value != obj.options.value) {
            el.value = obj.options.value;
            if (typeof(el.oninput) == 'function') {
                el.oninput({
                    type: 'input',
                    target: el,
                    value: el.value
                });
            }
        }
    }

    obj.getValue = function() {
        return obj.options.value;
    }

    var action = function(e) {
        if (e.which) {
            // Get target info
            var rect = el.getBoundingClientRect();

            if (e.changedTouches && e.changedTouches[0]) {
                var x = e.changedTouches[0].clientX;
                var y = e.changedTouches[0].clientY;
            } else {
                var x = e.clientX;
                var y = e.clientY;
            }

            obj.setValue(Math.round((x - rect.left) / rect.width * 100));
        }
    }

    // Events
    if ('touchstart' in document.documentElement === true) {
        el.addEventListener('touchstart', action);
        el.addEventListener('touchend', action);
    } else {
        el.addEventListener('mousedown', action);
        el.addEventListener("mousemove", action);
    }

    // Change
    el.change = obj.setValue;

    // Global generic value handler
    el.val = function(val) {
        if (val === undefined) {
            return obj.getValue();
        } else {
            obj.setValue(val);
        }
    }

    // Reference
    el.progressbar = obj;

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/rating.js
function Rating(el, options) {
    // Already created, update options
    if (el.rating) {
        return el.rating.setOptions(options, true);
    }

    // New instance
    var obj = {};
    obj.options = {};

    obj.setOptions = function(options, reset) {
        // Default configuration
        var defaults = {
            number: 5,
            value: 0,
            tooltip: [ 'Very bad', 'Bad', 'Average', 'Good', 'Very good' ],
            onchange: null,
        };

        // Loop through the initial configuration
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                if (typeof(obj.options[property]) == 'undefined' || reset === true) {
                    obj.options[property] = defaults[property];
                }
            }
        }

        // Make sure the container is empty
        el.innerHTML = '';

        // Add elements
        for (var i = 0; i < obj.options.number; i++) {
            var div = document.createElement('div');
            div.setAttribute('data-index', (i + 1))
            div.setAttribute('title', obj.options.tooltip[i])
            el.appendChild(div);
        }

        // Selected option
        if (obj.options.value) {
            for (var i = 0; i < obj.options.number; i++) {
                if (i < obj.options.value) {
                    el.children[i].classList.add('jrating-selected');
                }
            }
        }

        return obj;
    }

    // Set value
    obj.setValue = function(index) {
        for (var i = 0; i < obj.options.number; i++) {
            if (i < index) {
                el.children[i].classList.add('jrating-selected');
            } else {
                el.children[i].classList.remove('jrating-over');
                el.children[i].classList.remove('jrating-selected');
            }
        }

        obj.options.value = index;

        if (typeof(obj.options.onchange) == 'function') {
            obj.options.onchange(el, index);
        }

        // Lemonade JS
        if (el.value != obj.options.value) {
            el.value = obj.options.value;
            if (typeof(el.oninput) == 'function') {
                el.oninput({
                    type: 'input',
                    target: el,
                    value: el.value
                });
            }
        }
    }

    obj.getValue = function() {
        return obj.options.value;
    }

    var init = function() {
        // Start plugin
        obj.setOptions(options);

        // Class
        el.classList.add('jrating');

        // Events
        el.addEventListener("click", function(e) {
            var index = e.target.getAttribute('data-index');
            if (index != undefined) {
                if (index == obj.options.value) {
                    obj.setValue(0);
                } else {
                    obj.setValue(index);
                }
            }
        });

        el.addEventListener("mouseover", function(e) {
            var index = e.target.getAttribute('data-index');
            for (var i = 0; i < obj.options.number; i++) {
                if (i < index) {
                    el.children[i].classList.add('jrating-over');
                } else {
                    el.children[i].classList.remove('jrating-over');
                }
            }
        });

        el.addEventListener("mouseout", function(e) {
            for (var i = 0; i < obj.options.number; i++) {
                el.children[i].classList.remove('jrating-over');
            }
        });

        // Change
        el.change = obj.setValue;

        // Global generic value handler
        el.val = function(val) {
            if (val === undefined) {
                return obj.getValue();
            } else {
                obj.setValue(val);
            }
        }

        // Reference
        el.rating = obj;
    }

    init();

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/search.js



function Search(el, options) {
    if (el.search) {
        return el.search;
    }

    var index =  null;

    var select = function(e) {
        if (e.target.classList.contains('jsearch_item')) {
            var element = e.target;
        } else {
            var element = e.target.parentNode;
        }

        obj.selectIndex(element);
        e.preventDefault();
    }

    var createList = function(data) {
        if (typeof(obj.options.onsearch) == 'function') {
            var ret = obj.options.onsearch(obj, data);
            if (ret) {
                data = ret;
            }
        }

        // Reset container
        container.innerHTML = '';
        // Print results
        if (! data.length) {
            // Show container
            el.style.display = '';
        } else {
            // Show container
            el.style.display = 'block';

            // Show items (only 10)
            var len = data.length < 11 ? data.length : 10;
            for (var i = 0; i < len; i++) {
                if (typeof(data[i]) == 'string') {
                    var text = data[i];
                    var value = data[i];
                } else {
                    // Legacy
                    var text = data[i].text;
                    if (! text && data[i].name) {
                        text = data[i].name;
                    }
                    var value = data[i].value;
                    if (! value && data[i].id) {
                        value = data[i].id;
                    }
                }

                var div = document.createElement('div');
                div.setAttribute('data-value', value);
                div.setAttribute('data-text', text);
                div.className = 'jsearch_item';

                if (data[i].id) {
                    div.setAttribute('id', data[i].id)
                }

                if (obj.options.forceSelect && i == 0) {
                    div.classList.add('selected');
                }
                var img = document.createElement('img');
                if (data[i].image) {
                    img.src = data[i].image;
                } else {
                    img.style.display = 'none';
                }
                div.appendChild(img);

                var item = document.createElement('div');
                item.innerHTML = text;
                div.appendChild(item);

                // Append item to the container
                container.appendChild(div);
            }
        }
    }

    var execute = function(str) {
        if (str != obj.terms) {
            // New terms
            obj.terms = str;
            // New index
            if (obj.options.forceSelect) {
                index = 0;
            } else {
                index = null;
            }
            // Array or remote search
            if (Array.isArray(obj.options.data)) {
                var test = function(o) {
                    if (typeof(o) == 'string') {
                        if ((''+o).toLowerCase().search(str.toLowerCase()) >= 0) {
                            return true;
                        }
                    } else {
                        for (var key in o) {
                            var value = o[key];
                            if ((''+value).toLowerCase().search(str.toLowerCase()) >= 0) {
                                return true;
                            }
                        }
                    }
                    return false;
                }

                var results = obj.options.data.filter(function(item) {
                    return test(item);
                });

                // Show items
                createList(results);
            } else {
                // Get remove results
                ajax({
                    url: obj.options.data + str,
                    method: 'GET',
                    dataType: 'json',
                    success: function(data) {
                        // Show items
                        createList(data);
                    }
                });
            }
        }
    }

    // Search timer
    var timer = null;

    // Search methods
    var obj = function(str) {
        if (timer) {
            clearTimeout(timer);
        }
        timer = setTimeout(function() {
            execute(str);
        }, 500);
    }
    if(options.forceSelect === null) {
        options.forceSelect = true;
    }
    obj.options = {
        data: options.data || null,
        input: options.input || null,
        searchByNode: options.searchByNode || null,
        onselect: options.onselect || null,
        forceSelect: options.forceSelect,
        onsearch: options.onsearch || null,
        onbeforesearch: options.onbeforesearch || null,
    };

    obj.selectIndex = function(item) {
        var id = item.getAttribute('id');
        var text = item.getAttribute('data-text');
        var value = item.getAttribute('data-value');
        var image = item.children[0].src || '';
        // Onselect
        if (typeof(obj.options.onselect) == 'function') {
            obj.options.onselect(obj, text, value, id, image);
        }
        // Close container
        obj.close();
    }

    obj.open = function() {
        el.style.display = 'block';
    }

    obj.close = function() {
        if (timer) {
            clearTimeout(timer);
        }
        // Current terms
        obj.terms = '';
        // Remove results
        container.innerHTML = '';
        // Hide
        el.style.display = '';
    }

    obj.isOpened = function() {
        return el.style.display ? true : false;
    }

    obj.keydown = function(e) {
        if (obj.isOpened()) {
            if (e.key == 'Enter') {
                // Enter
                if (index!==null && container.children[index]) {
                    obj.selectIndex(container.children[index]);
                    e.preventDefault();
                } else {
                    obj.close();
                }
            } else if (e.key === 'ArrowUp') {
                // Up
                if (index!==null && container.children[0]) {
                    container.children[index].classList.remove('selected');
                    if(!obj.options.forceSelect && index === 0) {
                        index = null;
                    } else {
                        index = Math.max(0, index-1);
                        container.children[index].classList.add('selected');
                    }
                }
                e.preventDefault();
            } else if (e.key === 'ArrowDown') {
                // Down
                if(index == null) {
                    index = -1;
                } else {
                    container.children[index].classList.remove('selected');
                }
                if (index < 9 && container.children[index+1]) {
                    index++;
                }
                container.children[index].classList.add('selected');
                e.preventDefault();
            }
        }
    }

    obj.keyup = function(e) {
        if (! obj.options.searchByNode && obj.options.input) {
            if (obj.options.input.tagName === 'DIV') {
                var terms = obj.options.input.innerText;
            } else {
                var terms = obj.options.input.value;
            }
        } else {
            // Current node
            var node = helpers.getNode();
            if (node) {
                var terms = node.innerText;
            }
        }

        if (typeof(obj.options.onbeforesearch) == 'function') {
            var ret = obj.options.onbeforesearch(obj, terms);
            if (ret) {
                terms = ret;
            } else {
                if (ret === false) {
                    // Ignore event
                    return;
                }
            }
        }

        obj(terms);
    }

    obj.blur = function(e) {
        obj.close();
    }

    // Add events
    if (obj.options.input) {
        obj.options.input.addEventListener("keyup", obj.keyup);
        obj.options.input.addEventListener("keydown", obj.keydown);
        obj.options.input.addEventListener("blur", obj.blur);
    }

    // Append element
    var container = document.createElement('div');
    container.classList.add('jsearch_container');
    container.onmousedown = select;
    el.appendChild(container);

    el.classList.add('jsearch');
    el.search = obj;

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/slider.js
function Slider(el, options) {
    var obj = {};
    obj.options = {};
    obj.currentImage = null;

    if (options) {
        obj.options = options;
    }

    // Focus
    el.setAttribute('tabindex', '900')

    // Items
    obj.options.items = [];

    if (! el.classList.contains('jslider')) {
        el.classList.add('jslider');
        el.classList.add('unselectable');

        if (obj.options.height) {
            el.style.minHeight = parseInt(obj.options.height) + 'px';
        }
        if (obj.options.width) {
            el.style.width = parseInt(obj.options.width) + 'px';
        }
        if (obj.options.grid) {
            el.classList.add('jslider-grid');
            var number = el.children.length;
            if (number > 4) {
                el.setAttribute('data-total', number - 4);
            }
            el.setAttribute('data-number', (number > 4 ? 4 : number));
        }

        // Add slider counter
        var counter = document.createElement('div');
        counter.classList.add('jslider-counter');

        // Move children inside
        if (el.children.length > 0) {
            // Keep children items
            for (var i = 0; i < el.children.length; i++) {
                obj.options.items.push(el.children[i]);
                
                // counter click event
                var item = document.createElement('div');
                item.onclick = function() {
                    var index = Array.prototype.slice.call(counter.children).indexOf(this);
                    obj.show(obj.currentImage = obj.options.items[index]);
                }
                counter.appendChild(item);
            }
        }
        // Add caption
        var caption = document.createElement('div');
        caption.className = 'jslider-caption';

        // Add close buttom
        var controls = document.createElement('div');
        var close = document.createElement('div');
        close.className = 'jslider-close';
        close.innerHTML = '';
        
        close.onclick = function() {
            obj.close();
        }
        controls.appendChild(caption);
        controls.appendChild(close);
    }

    obj.updateCounter = function(index) {
        for (var i = 0; i < counter.children.length; i ++) {
            if (counter.children[i].classList.contains('jslider-counter-focus')) {
                counter.children[i].classList.remove('jslider-counter-focus');
                break;
            }
        }
        counter.children[index].classList.add('jslider-counter-focus');
    }

    obj.show = function(target) {
        if (! target) {
            var target = el.children[0];
        }

        // Focus element
        el.classList.add('jslider-focus');
        el.classList.remove('jslider-grid');
        el.appendChild(controls);
        el.appendChild(counter);

        // Update counter
        var index = obj.options.items.indexOf(target);
        obj.updateCounter(index);

        // Remove display
        for (var i = 0; i < el.children.length; i++) {
            el.children[i].style.display = '';
        }
        target.style.display = 'block';

        // Is there any previous
        if (target.previousElementSibling) {
            el.classList.add('jslider-left');
        } else {
            el.classList.remove('jslider-left');
        }

        // Is there any next
        if (target.nextElementSibling && target.nextElementSibling.tagName == 'IMG') {
            el.classList.add('jslider-right');
        } else {
            el.classList.remove('jslider-right');
        }

        obj.currentImage = target;

        // Vertical image
        if (obj.currentImage.offsetHeight > obj.currentImage.offsetWidth) {
            obj.currentImage.classList.add('jslider-vertical');
        }

        controls.children[0].innerText = obj.currentImage.getAttribute('title');
    }

    obj.open = function() {
        obj.show();

        // Event
        if (typeof(obj.options.onopen) == 'function') {
            obj.options.onopen(el);
        }
    }

    obj.close = function() {
        // Remove control classes
        el.classList.remove('jslider-focus');
        el.classList.remove('jslider-left');
        el.classList.remove('jslider-right');
        // Show as a grid depending on the configuration
        if (obj.options.grid) {
            el.classList.add('jslider-grid');
        }
        // Remove display
        for (var i = 0; i < el.children.length; i++) {
            el.children[i].style.display = '';
        }
        // Remove controls from the component
        counter.remove();
        controls.remove();
        // Current image
        obj.currentImage = null;
        // Event
        if (typeof(obj.options.onclose) == 'function') {
            obj.options.onclose(el);
        }
    }

    obj.reset = function() {
        el.innerHTML = '';
    }

    obj.next = function() {
        var nextImage = obj.currentImage.nextElementSibling;
        if (nextImage && nextImage.tagName === 'IMG') {
            obj.show(obj.currentImage.nextElementSibling);
        }
    }
    
    obj.prev = function() {
        if (obj.currentImage.previousElementSibling) {
            obj.show(obj.currentImage.previousElementSibling);
        }
    }

    var mouseUp = function(e) {
        // Open slider
        if (e.target.tagName == 'IMG') {
            obj.show(e.target);
        } else if (! e.target.classList.contains('jslider-close') && ! (e.target.parentNode.classList.contains('jslider-counter') || e.target.classList.contains('jslider-counter'))){
            // Arrow controls
            var offsetX = e.offsetX || e.changedTouches[0].clientX;
            if (e.target.clientWidth - offsetX < 40) {
                // Show next image
                obj.next();
            } else if (offsetX < 40) {
                // Show previous image
                obj.prev();
            }
        }
    }

    if ('ontouchend' in document.documentElement === true) {
        el.addEventListener('touchend', mouseUp);
    } else {
        el.addEventListener('mouseup', mouseUp);
    }

    // Add global events
    el.addEventListener("swipeleft", function(e) {
        obj.next();
        e.preventDefault();
        e.stopPropagation();
    });

    el.addEventListener("swiperight", function(e) {
        obj.prev();
        e.preventDefault();
        e.stopPropagation();
    });

    el.addEventListener('keydown', function(e) {
        if (e.which == 27) {
            obj.close();
        }
    });

    el.slider = obj;

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/tags.js




function Tags(el, options) {
    // Redefine configuration
    if (el.tags) {
        return el.tags.setOptions(options, true);
    }

    var obj = { type:'tags' };
    obj.options = {};

    // Limit
    var limit = function() {
        return obj.options.limit && el.children.length >= obj.options.limit ? true : false;
    }

    // Search helpers
    var search = null;
    var searchContainer = null;

    obj.setOptions = function(options, reset) {
        /**
         * @typedef {Object} defaults
         * @property {(string|Array)} value - Initial value of the compontent
         * @property {number} limit - Max number of tags inside the element
         * @property {string} search - The URL for suggestions
         * @property {string} placeholder - The default instruction text on the element
         * @property {validation} validation - Method to validate the tags
         * @property {requestCallback} onbeforechange - Method to be execute before any changes on the element
         * @property {requestCallback} onchange - Method to be execute after any changes on the element
         * @property {requestCallback} onfocus - Method to be execute when on focus
         * @property {requestCallback} onblur - Method to be execute when on blur
         * @property {requestCallback} onload - Method to be execute when the element is loaded
         */
        var defaults = {
            value: '',
            limit: null,
            search: null,
            placeholder: null,
            validation: null,
            onbeforepaste: null,
            onbeforechange: null,
            onremoveitem: null,
            onlimit: null,
            onchange: null,
            onfocus: null,
            onblur: null,
            onload: null,
        }

        // Loop through though the default configuration
        for (var property in defaults) {
            if (options && options.hasOwnProperty(property)) {
                obj.options[property] = options[property];
            } else {
                if (typeof(obj.options[property]) == 'undefined' || reset === true) {
                    obj.options[property] = defaults[property];
                }
            }
        }

        // Placeholder
        if (obj.options.placeholder) {
            el.setAttribute('data-placeholder', obj.options.placeholder);
        } else {
            el.removeAttribute('data-placeholder');
        }
        el.placeholder = obj.options.placeholder;

        // Update value
        obj.setValue(obj.options.value);

        // Validate items
        filter();

        // Create search box
        if (obj.options.search) {
            if (! searchContainer) {
                searchContainer = document.createElement('div');
                el.parentNode.insertBefore(searchContainer, el.nextSibling);

                // Create container
                search = Search(searchContainer, {
                    data: obj.options.search,
                    onselect: function(a,b,c) {
                        obj.selectIndex(b,c);
                    }
                });
            }
        } else {
            if (searchContainer) {
                search = null;
                searchContainer.remove();
                searchContainer = null;
            }
        }

        return obj;
    }

    /**
     * Add a new tag to the element
     * @param {(?string|Array)} value - The value of the new element
     */
    obj.add = function(value, focus) {
        if (typeof(obj.options.onbeforechange) == 'function') {
            var ret = obj.options.onbeforechange(el, obj, obj.options.value, value);
            if (ret === false) {
                return false;
            } else { 
                if (ret != null) {
                    value = ret;
                }
            }
        }

        // Make sure search is closed
        if (search) {
            search.close();
        }

        if (limit()) {
            if (typeof(obj.options.onlimit) == 'function') {
                obj.options.onlimit(obj, obj.options.limit);
            } else {
                alert(dictionary.translate('You reach the limit number of entries') + ' ' + obj.options.limit);
            }
        } else {
            // Get node
            var node = helpers.getNode();

            if (node && node.parentNode && node.parentNode.classList.contains('jtags') &&
                node.nextSibling && (! (node.nextSibling.innerText && node.nextSibling.innerText.trim()))) {
                div = node.nextSibling;
            } else {
                // Remove not used last item
                if (el.lastChild) {
                    if (! el.lastChild.innerText.trim()) {
                        el.removeChild(el.lastChild);
                    }
                }

                // Mix argument string or array
                if (! value || typeof(value) == 'string') {
                    var div = createElement(value, value, node);
                } else {
                    for (var i = 0; i <= value.length; i++) {
                        if (! limit()) {
                            if (! value[i] || typeof(value[i]) == 'string') {
                                var t = value[i] || '';
                                var v = null;
                            } else {
                                var t = value[i].text;
                                var v = value[i].value;
                            }

                            // Add element
                            var div = createElement(t, v);
                        }
                    }
                }

                // Change
                change();
            }

            // Place caret
            if (focus) {
                setFocus(div);
            }
        }
    }

    obj.setLimit = function(limit) {
        obj.options.limit = limit;
        var n = el.children.length - limit;
        while (el.children.length > limit) {
            el.removeChild(el.lastChild);
        }
    }

    // Remove a item node
    obj.remove = function(node) {
        // Remove node
        node.parentNode.removeChild(node);
        // Make sure element is not blank
        if (! el.children.length) {
            obj.add('', true);
        } else {
            change();
        }

        if (typeof(obj.options.onremoveitem) == 'function') {
            obj.options.onremoveitem(el, obj, node);
        }
    }

    /**
     * Get all tags in the element
     * @return {Array} data - All tags as an array
     */
    obj.getData = function() {
        var data = [];
        for (var i = 0; i < el.children.length; i++) {
            // Get value
            var text = el.children[i].innerText.replace("\n", "");
            // Get id
            var value = el.children[i].getAttribute('data-value');
            if (! value) {
                value = text;
            }
            // Item
            if (text || value) {
                data.push({ text: text, value: value });
            }
        }
        return data;
    }

    /**
     * Get the value of one tag. Null for all tags
     * @param {?number} index - Tag index number. Null for all tags.
     * @return {string} value - All tags separated by comma
     */
    obj.getValue = function(index) {
        var value = null;

        if (index != null) {
            // Get one individual value
            value = el.children[index].getAttribute('data-value');
            if (! value) {
                value = el.children[index].innerText.replace("\n", "");
            }
        } else {
            // Get all
            var data = [];
            for (var i = 0; i < el.children.length; i++) {
                value = el.children[i].innerText.replace("\n", "");
                if (value) {
                    data.push(obj.getValue(i));
                }
            }
            value = data.join(',');
        }

        return value;
    }

    /**
     * Set the value of the element based on a string separeted by (,|;|\r\n)
     * @param {mixed} value - A string or array object with values
     */
    obj.setValue = function(mixed) {
        if (! mixed) {
            obj.reset();
        } else {
            if (el.value != mixed) {
                if (Array.isArray(mixed)) {
                    obj.add(mixed);
                } else {
                    // Remove whitespaces
                    var text = (''+mixed).trim();
                    // Tags
                    var data = extractTags(text);
                    // Reset
                    el.innerHTML = '';
                    // Add tags to the element
                    obj.add(data);
                }
            }
        }
    }

    /**
     * Reset the data from the element
     */
    obj.reset = function() {
        // Empty class
        el.classList.add('jtags-empty');
        // Empty element
        el.innerHTML = '<div></div>';
        // Execute changes
        change();
    }

    /**
     * Verify if all tags in the element are valid
     * @return {boolean}
     */
    obj.isValid = function() {
        var test = 0;
        for (var i = 0; i < el.children.length; i++) {
            if (el.children[i].classList.contains('jtags_error')) {
                test++;
            }
        }
        return test == 0 ? true : false;
    }

    /**
     * Add one element from the suggestions to the element
     * @param {object} item - Node element in the suggestions container
     */ 
    obj.selectIndex = function(text, value) {
        var node = helpers.getNode();
        if (node) {
            // Append text to the caret
            node.innerText = text;
            // Set node id
            if (value) {
                node.setAttribute('data-value', value);
            }
            // Remove any error
            node.classList.remove('jtags_error');
            if (! limit()) {
                // Add new item
                obj.add('', true);
            }
        }
    }

    /**
     * Search for suggestions
     * @param {object} node - Target node for any suggestions
     */
    obj.search = function(node) {
        // Search for
        var terms = node.innerText;
    }

    // Destroy tags element
    obj.destroy = function() {
        // Bind events
        el.removeEventListener('mouseup', tagsMouseUp);
        el.removeEventListener('keydown', tagsKeyDown);
        el.removeEventListener('keyup', tagsKeyUp);
        el.removeEventListener('paste', tagsPaste);
        el.removeEventListener('focus', tagsFocus);
        el.removeEventListener('blur', tagsBlur);

        // Remove element
        el.parentNode.removeChild(el);
    }

    var setFocus = function(node) {
        if (el.children.length) {
            var range = document.createRange();
            var sel = window.getSelection();
            if (! node) {
                var node = el.childNodes[el.childNodes.length-1];
            }
            range.setStart(node, node.length)
            range.collapse(true)
            sel.removeAllRanges()
            sel.addRange(range)
            el.scrollLeft = el.scrollWidth;
        }
    }

    var createElement = function(label, value, node) {
        var div = document.createElement('div');
        div.textContent = label ? label : '';
        if (value) {
            div.setAttribute('data-value', value);
        }

        if (node && node.parentNode.classList.contains('jtags')) {
            el.insertBefore(div, node.nextSibling);
        } else {
            el.appendChild(div);
        }

        return div;
    }

    var change = function() {
        // Value
        var value = obj.getValue();

        if (value != obj.options.value) {
            obj.options.value = value;
            if (typeof(obj.options.onchange) == 'function') {
                obj.options.onchange(el, obj, obj.options.value);
            }

            // Lemonade JS
            if (el.value != obj.options.value) {
                el.value = obj.options.value;
                if (typeof(el.oninput) == 'function') {
                    el.oninput({
                        type: 'input',
                        target: el,
                        value: el.value
                    });
                }
            }
        }

        filter();
    }

    /**
     * Filter tags
     */
    var filter = function() {
        for (var i = 0; i < el.children.length; i++) {
            if (el.children[i].tagName === 'DIV') {
                // Create label design
                if (!obj.getValue(i)) {
                    el.children[i].classList.remove('jtags_label');
                } else {
                    el.children[i].classList.add('jtags_label');

                    // Validation in place
                    if (typeof (obj.options.validation) == 'function') {
                        if (obj.getValue(i)) {
                            if (!obj.options.validation(el.children[i], el.children[i].innerText, el.children[i].getAttribute('data-value'))) {
                                el.children[i].classList.add('jtags_error');
                            } else {
                                el.children[i].classList.remove('jtags_error');
                            }
                        } else {
                            el.children[i].classList.remove('jtags_error');
                        }
                    } else {
                        el.children[i].classList.remove('jtags_error');
                    }
                }
            }
        }

        isEmpty();
    }

    var isEmpty = function() {
        // Can't be empty
        if (! el.innerText.trim()) {
            if (! el.children.length || el.children[0].tagName === 'BR') {
                el.innerHTML = '';
                setFocus(createElement());
            }
        } else {
            el.classList.remove('jtags-empty');
        }
    }

    /**
     * Extract tags from a string
     * @param {string} text - Raw string
     * @return {Array} data - Array with extracted tags
     */
    var extractTags = function(text) {
        /** @type {Array} */
        var data = [];

        /** @type {string} */
        var word = '';

        // Remove whitespaces
        text = text.trim();

        if (text) {
            for (var i = 0; i < text.length; i++) {
                if (text[i] == ',' || text[i] == ';' || text[i] == '\n') {
                    if (word) {
                        data.push(word.trim());
                        word = '';
                    }
                } else {
                    word += text[i];
                }
            }

            if (word) {
                data.push(word);
            }
        }

        return data;
    }

    /** @type {number} */
    var anchorOffset = 0;

    /**
     * Processing event keydown on the element
     * @param e {object}
     */
    var tagsKeyDown = function(e) {
        // Anchoroffset
        anchorOffset = window.getSelection().anchorOffset;

        // Verify if is empty
        isEmpty();

        // Comma
        if (e.key === 'Tab'  || e.key === ';' || e.key === ',') {
            var n = window.getSelection().anchorOffset;
            if (n > 1) {
                if (limit()) {
                    if (typeof(obj.options.onlimit) == 'function') {
                        obj.options.onlimit(obj, obj.options.limit)
                    }
                } else {
                    obj.add('', true);
                }
            }
            e.preventDefault();
        } else if (e.key == 'Enter') {
            if (! search || ! search.isOpened()) {
                var n = window.getSelection().anchorOffset;
                if (n > 1) {
                    if (! limit()) {
                        obj.add('', true);
                    }
                }
                e.preventDefault();
            }
        } else if (e.key == 'Backspace') {
            // Back space - do not let last item to be removed
            if (el.children.length == 1 && window.getSelection().anchorOffset < 1) {
                e.preventDefault();
            }
        }

        // Search events
        if (search) {
            search.keydown(e);
        }

        // Verify if is empty
        isEmpty();
    }

    /**
     * Processing event keyup on the element
     * @param e {object}
     */
    var tagsKeyUp = function(e) {
        if (e.which == 39) {
            // Right arrow
            var n = window.getSelection().anchorOffset;
            if (n > 1 && n == anchorOffset) {
                obj.add('', true);
            }
        } else if (e.which == 13 || e.which == 38 || e.which == 40) {
            e.preventDefault();
        } else {
            if (search) {
                search.keyup(e);
            }
        }

        filter();
    }

    /**
     * Processing event paste on the element
     * @param e {object}
     */
    var tagsPaste =  function(e) {
        if (e.clipboardData || e.originalEvent.clipboardData) {
            var text = (e.originalEvent || e).clipboardData.getData('text/plain');
        } else if (window.clipboardData) {
            var text = window.clipboardData.getData('Text');
        }

        var data = extractTags(text);

        if (typeof(obj.options.onbeforepaste) == 'function') {
            var ret = obj.options.onbeforepaste(el, obj, data);
            if (ret === false) {
                e.preventDefault();
                return false;
            } else {
                if (ret) {
                    data = ret;
                }
            }
        }

        if (data.length > 1) {
            obj.add(data, true);
            e.preventDefault();
        } else if (data[0]) {
            document.execCommand('insertText', false, data[0])
            e.preventDefault();
        }
    }

    /**
     * Processing event mouseup on the element
     * @param e {object}
     */
    var tagsMouseUp = function(e) {
        if (e.target.parentNode && e.target.parentNode.classList.contains('jtags')) {
            if (e.target.classList.contains('jtags_label') || e.target.classList.contains('jtags_error')) {
                var rect = e.target.getBoundingClientRect();
                if (rect.width - (e.clientX - rect.left) < 16) {
                    obj.remove(e.target);
                }
            }
        }

        // Set focus in the last item
        if (e.target == el) {
            setFocus();
        }
    }

    var tagsFocus = function() {
        if (! el.classList.contains('jtags-focus')) {
            if (! el.children.length || obj.getValue(el.children.length - 1)) {
                if (! limit()) {
                    createElement('');
                }
            }

            if (typeof(obj.options.onfocus) == 'function') {
                obj.options.onfocus(el, obj, obj.getValue());
            }

            el.classList.add('jtags-focus');
        }
    }

    var tagsBlur = function() {
        if (el.classList.contains('jtags-focus')) {
            if (search) {
                search.close();
            }

            for (var i = 0; i < el.children.length - 1; i++) {
                // Create label design
                if (! obj.getValue(i)) {
                    el.removeChild(el.children[i]);
                }
            }

            change();

            el.classList.remove('jtags-focus');

            if (typeof(obj.options.onblur) == 'function') {
                obj.options.onblur(el, obj, obj.getValue());
            }
        }
    }

    var init = function() {
        // Bind events
        if ('touchend' in document.documentElement === true) {
            el.addEventListener('touchend', tagsMouseUp);
        } else {
            el.addEventListener('mouseup', tagsMouseUp);
        }

        el.addEventListener('keydown', tagsKeyDown);
        el.addEventListener('keyup', tagsKeyUp);
        el.addEventListener('paste', tagsPaste);
        el.addEventListener('focus', tagsFocus);
        el.addEventListener('blur', tagsBlur);

        // Editable
        el.setAttribute('contenteditable', true);

        // Prepare container
        el.classList.add('jtags');

        // Initial options
        obj.setOptions(options);

        if (typeof(obj.options.onload) == 'function') {
            obj.options.onload(el, obj);
        }

        // Change methods
        el.change = obj.setValue;

        // Global generic value handler
        el.val = function(val) {
            if (val === undefined) {
                return obj.getValue();
            } else {
                obj.setValue(val);
            }
        }

        el.tags = obj;
    }

    init();

    return obj;
}
;// CONCATENATED MODULE: ./src/plugins/upload.js





function Upload(el, options) {
    var obj = {};
    obj.options = {};

    // Default configuration
    var defaults = {
        type: 'image',
        extension: '*',
        input: false,
        minWidth: false,
        maxWidth: null,
        maxHeight: null,
        maxJpegSizeBytes: null, // For example, 350Kb would be 350000
        onchange: null,
        multiple: false,
        remoteParser: null,
    };

    // Loop through our object
    for (var property in defaults) {
        if (options && options.hasOwnProperty(property)) {
            obj.options[property] = options[property];
        } else {
            obj.options[property] = defaults[property];
        }
    }

    // Multiple
    if (obj.options.multiple == true) {
        el.setAttribute('data-multiple', true);
    }

    // Container
    el.content = [];

    // Upload icon
    el.classList.add('jupload');

    if (obj.options.input == true) {
        el.classList.add('input');
    }

    obj.add = function(data) {
        // Reset container for single files
        if (obj.options.multiple == false) {
            el.content = [];
            el.innerText = '';
        }

        // Append to the element
        if (obj.options.type == 'image') {
            var img = document.createElement('img');
            img.setAttribute('src', data.file);
            img.setAttribute('tabindex', -1);
            if (! el.getAttribute('name')) {
                img.className = 'jfile';
                img.content = data;
            }
            el.appendChild(img);
        } else {
            if (data.name) {
                var name = data.name;
            } else {
                var name = data.file;
            }
            var div = document.createElement('div');
            div.innerText = name || obj.options.type;
            div.classList.add('jupload-item');
            div.setAttribute('tabindex', -1);
            el.appendChild(div);
        }

        if (data.content) {
            data.file = helpers.guid();
        }

        // Push content
        el.content.push(data);

        // Onchange
        if (typeof(obj.options.onchange) == 'function') {
            obj.options.onchange(el, data);
        }
    }

    obj.addFromFile = function(file) {
        var type = file.type.split('/');
        if (type[0] == obj.options.type) {
            var readFile = new FileReader();
            readFile.addEventListener("load", function (v) {
                var data = {
                    file: v.srcElement.result,
                    extension: file.name.substr(file.name.lastIndexOf('.') + 1),
                    name: file.name,
                    size: file.size,
                    lastmodified: file.lastModified,
                    content: v.srcElement.result,
                }

                obj.add(data);
            });

            readFile.readAsDataURL(file);
        } else {
            alert(dictionary.translate('This extension is not allowed'));
        }
    }

    obj.addFromUrl = function(src) {
        if (src.substr(0,4) != 'data' && ! obj.options.remoteParser) {
            console.error('remoteParser not defined in your initialization');
        } else {
            // This is to process cross domain images
            if (src.substr(0,4) == 'data') {
                var extension = src.split(';')
                extension = extension[0].split('/');
                var type = extension[0].replace('data:','');
                if (type == obj.options.type) {
                    var data = {
                        file: src,
                        name: '',
                        extension: extension[1],
                        content: src,
                    }
                    obj.add(data);
                } else {
                    alert(obj.options.text.extensionNotAllowed);
                }
            } else {
                var extension = src.substr(src.lastIndexOf('.') + 1);
                // Work for cross browsers
                src = obj.options.remoteParser + src;
                // Get remove content
                ajax({
                    url: src,
                    type: 'GET',
                    dataType: 'blob',
                    success: function(data) {
                        //add(extension[0].replace('data:',''), data);
                    }
                })
            }
        }
    }

    var getDataURL = function(canvas, type) {
        var compression = 0.92;
        var lastContentLength = null;
        var content = canvas.toDataURL(type, compression);
        while (obj.options.maxJpegSizeBytes && type === 'image/jpeg' &&
               content.length > obj.options.maxJpegSizeBytes && content.length !== lastContentLength) {
            // Apply the compression
            compression *= 0.9;
            lastContentLength = content.length;
            content = canvas.toDataURL(type, compression);
        }
        return content;
    }

    var mime = obj.options.type + '/' + obj.options.extension;
    var input = document.createElement('input');
    input.type = 'file';
    input.setAttribute('accept', mime);
    input.onchange = function() {
        for (var i = 0; i < this.files.length; i++) {
            obj.addFromFile(this.files[i]);
        }
    }

    // Allow multiple files
    if (obj.options.multiple == true) {
        input.setAttribute('multiple', true);
    }

    var current = null;

    el.addEventListener("click", function(e) {
        current = null;
        if (! el.children.length || e.target === el) {
            helpers.click(input);
        } else {
            if (e.target.parentNode == el) {
                current = e.target;
            }
        }
    });

    el.addEventListener("dblclick", function(e) {
        helpers.click(input);
    });

    el.addEventListener('dragenter', function(e) {
        el.style.border = '1px dashed #000';
    });

    el.addEventListener('dragleave', function(e) {
        el.style.border = '1px solid #eee';
    });

    el.addEventListener('dragstop', function(e) {
        el.style.border = '1px solid #eee';
    });

    el.addEventListener('dragover', function(e) {
        e.preventDefault();
    });

    el.addEventListener('keydown', function(e) {
        if (current && e.which == 46) {
            var index = Array.prototype.indexOf.call(el.children, current);
            if (index >= 0) {
                el.content.splice(index, 1);
                current.remove();
                current = null;
            }
        }
    });

    el.addEventListener('drop', function(e) {
        e.preventDefault();
        e.stopPropagation();

        var html = (e.originalEvent || e).dataTransfer.getData('text/html');
        var file = (e.originalEvent || e).dataTransfer.files;

        if (file.length) {
            for (var i = 0; i < e.dataTransfer.files.length; i++) {
                obj.addFromFile(e.dataTransfer.files[i]);
            }
        } else if (html) {
            if (obj.options.multiple == false) {
                el.innerText = '';
            }

            // Create temp element
            let img = [];
            utils_filter(html, img);
            if (img.length) {
                for (var i = 0; i < img.length; i++) {
                    obj.addFromUrl(img[i]);
                }
            }
        }

        el.style.border = '1px solid #eee';

        return false;
    });

    el.val = function(val) {
        if (val === undefined) {
            return el.content && el.content.length ? el.content : null;
        } else {
            // Reset
            el.innerText = '';
            el.content = [];

            if (val) {
                if (Array.isArray(val)) {
                    for (var i = 0; i < val.length; i++) {
                        if (typeof(val[i]) == 'string') {
                            obj.add({ file: val[i] });
                        } else {
                            obj.add(val[i]);
                        }
                    }
                } else if (typeof(val) == 'string') {
                    obj.add({ file: val });
                }
            }
        }
    }

    el.upload = el.image = obj;

    return obj;
}

// EXTERNAL MODULE: ./packages/sha512/sha512.js
var sha512 = __webpack_require__(195);
var sha512_default = /*#__PURE__*/__webpack_require__.n(sha512);
;// CONCATENATED MODULE: ./src/jsuites.js




















































var jsuites_jSuites = {
    // Helpers
    ...dictionary,
    ...helpers,
    /** Current version */
    version: '5.13.3',
    /** Bind new extensions to Jsuites */
    setExtensions: function(o) {
        if (typeof(o) == 'object') {
            var k = Object.keys(o);
            for (var i = 0; i < k.length; i++) {
                jsuites_jSuites[k[i]] = o[k[i]];
            }
        }
    },
    tracking: tracking,
    path: Path,
    sorting: Sorting,
    lazyLoading: LazyLoading,
    // Plugins
    ajax: ajax,
    animation: animation,
    calendar: calendar,
    color: Color,
    contextmenu: contextmenu,
    dropdown: dropdown,
    editor: editor,
    floating: floating,
    form: plugins_form,
    mask: mask,
    modal: modal,
    notification: notification,
    palette: palette,
    picker: Picker,
    progressbar: Progressbar,
    rating: Rating,
    search: Search,
    slider: Slider,
    tabs: Tabs,
    tags: Tags,
    toolbar: Toolbar,
    upload: Upload,
    validations: validations,
}

// Legacy
jsuites_jSuites.image = Upload;
jsuites_jSuites.image.create = function(data) {
    var img = document.createElement('img');
    img.setAttribute('src', data.file);
    img.className = 'jfile';
    img.setAttribute('tabindex', -1);
    img.content = data;

    return img;
}

jsuites_jSuites.tracker = plugins_form;
jsuites_jSuites.loading = animation.loading;
jsuites_jSuites.sha512 = (sha512_default());


/** Core events */
const Events = function() {

    if (typeof(window['jSuitesStateControl']) === 'undefined') {
        window['jSuitesStateControl'] = [];
    } else {
        // Do nothing
        return;
    }

    const find = function(DOMElement, component) {
        if (DOMElement[component.type] && DOMElement[component.type] == component) {
            return true;
        }
        if (DOMElement.component && DOMElement.component == component) {
            return true;
        }
        if (DOMElement.parentNode) {
            return find(DOMElement.parentNode, component);
        }
        return false;
    }

    const isOpened = function(e) {
        let state = window['jSuitesStateControl'];
        if (state && state.length > 0) {
            for (let i = 0; i < state.length; i++) {
                if (state[i] && ! find(e, state[i])) {
                    state[i].close();
                }
            }
        }
    }

    // Width of the border
    let cornerSize = 15;

    // Current element
    let element = null;

    // Controllers
    let editorAction = false;

    // Event state
    let state = {
        x: null,
        y: null,
    }

    // Tooltip element
    let tooltip = document.createElement('div')
    tooltip.classList.add('jtooltip');

    const isWebcomponent = function(e) {
        return e && (e.shadowRoot || (e.tagName && e.tagName.includes('-')));
    }

    const getElement = function(e) {
        let d;
        let element;
        // Which component I am clicking
        let path = e.path || (e.composedPath && e.composedPath());

        // If path available get the first element in the chain
        if (path) {
            element = path[0];
            // Adjustment sales force
            if (element && isWebcomponent(element) && ! element.shadowRoot && e.toElement) {
                element = e.toElement;
            }
        } else {
            // Try to guess using the coordinates
            if (e.target && isWebcomponent(e.target)) {
                d = e.target.shadowRoot;
            } else {
                d = document;
            }
            // Get the first target element
            element = d.elementFromPoint(x, y);
        }
        return element;
    }

    // Events
    const mouseDown = function(e) {
        // Verify current components tracking
        if (e.changedTouches && e.changedTouches[0]) {
            var x = e.changedTouches[0].clientX;
            var y = e.changedTouches[0].clientY;
        } else {
            var x = e.clientX;
            var y = e.clientY;
        }

        let element = getElement(e);
        // Editable
        let editable = element && element.tagName === 'DIV' && element.getAttribute('contentEditable');
        // Check if this is the floating
        let item = jsuites_jSuites.findElement(element, 'jpanel');
        // Jfloating found
        if (item && ! item.classList.contains("readonly") && ! editable) {
            // Keep the tracking information
            let rect = item.getBoundingClientRect();
            let angle = 0;
            if (item.style.rotate) {
                // Extract the angle value from the match and convert it to a number
                angle = parseFloat(item.style.rotate);
            }
            let action = 'move';
            if (element.getAttribute('data-action')) {
                action = element.getAttribute('data-action');
            } else {
                if (item.style.cursor) {
                    action = 'resize';
                } else {
                    item.style.cursor = 'move';
                }
            }

            // Action
            editorAction = {
                action: action,
                a: angle,
                e: item,
                x: x,
                y: y,
                l: rect.left,
                t: rect.top,
                b: rect.bottom,
                r: rect.right,
                w: rect.width,
                h: rect.height,
                d: item.style.cursor,
                actioned: false,
            }
            // Make sure width and height styling is OK
            if (! item.style.width) {
                item.style.width = rect.width + 'px';
            }
            if (! item.style.height) {
                item.style.height = rect.height + 'px';
            }
        } else {
            // No floating action found
            editorAction = false;
        }

        isOpened(element);

        focus(e);
    }

    const calculateAngle = function(x1, y1, x2, y2, x3, y3) {
        // Calculate dx and dy for the first line
        const dx1 = x2 - x1;
        const dy1 = y2 - y1;
        // Calculate dx and dy for the second line
        const dx2 = x3 - x1;
        const dy2 = y3 - y1;
        // Calculate the angle for the first line
        let angle1 = Math.atan2(dy1, dx1);
        // Calculate the angle for the second line
        let angle2 = Math.atan2(dy2, dx2);
        // Calculate the angle difference in radians
        let angleDifference = angle2 - angle1;
        // Convert the angle difference to degrees
        angleDifference = angleDifference * (180 / Math.PI);
        // Normalize the angle difference to be within [0, 360) degrees
        if (angleDifference < 0) {
            angleDifference += 360;
        }
        return angleDifference;
    }

    const mouseUp = function(e) {
        if (editorAction && editorAction.e) {
            if (typeof(editorAction.e.refresh) == 'function' && state.actioned) {
                editorAction.e.refresh();
            }
            editorAction.e.style.cursor = '';
        }

        // Reset
        state = {
            x: null,
            y: null,
        }

        editorAction = false;
    }

    const mouseMove = function(e) {
        if (editorAction) {
            let x = e.clientX || e.pageX;
            let y = e.clientY || e.pageY;

            if (state.x == null && state.y == null) {
                state.x = x;
                state.y = y;
            }

            // Action on going
            if (editorAction.action === 'move') {
                var dx = x - state.x;
                var dy = y - state.y;
                var top = editorAction.e.offsetTop + dy;
                var left = editorAction.e.offsetLeft + dx;

                // Update position
                editorAction.e.style.top = top + 'px';
                editorAction.e.style.left = left + 'px';

                // Update element
                if (typeof (editorAction.e.refresh) == 'function') {
                    state.actioned = true;
                    editorAction.e.refresh('position', top, left);
                }
            } else if (editorAction.action === 'rotate') {
                let ox = editorAction.l+editorAction.w/2;
                let oy = editorAction.t+editorAction.h/2;
                let angle = calculateAngle(ox, oy, editorAction.x, editorAction.y, x, y);
                angle = angle + editorAction.a % 360;
                angle = Math.round(angle / 2) * 2;
                editorAction.e.style.rotate = `${angle}deg`;
                // Update element
                if (typeof (editorAction.e.refresh) == 'function') {
                    state.actioned = true;
                    editorAction.e.refresh('rotate', angle);
                }
            } else if (editorAction.action === 'resize') {
                let top = null;
                let left = null;
                let width = null;
                let height = null;

                if (editorAction.d == 'e-resize' || editorAction.d == 'ne-resize' || editorAction.d == 'se-resize') {
                    width = editorAction.e.offsetWidth + (x - state.x);

                    if (e.shiftKey) {
                        height = editorAction.e.offsetHeight + (x - state.x) * (editorAction.e.offsetHeight / editorAction.e.offsetWidth);
                    }
                } else if (editorAction.d === 'w-resize' || editorAction.d == 'nw-resize'|| editorAction.d == 'sw-resize') {
                    left = editorAction.e.offsetLeft + (x - state.x);
                    width = editorAction.e.offsetLeft + editorAction.e.offsetWidth - left;

                    if (e.shiftKey) {
                        height = editorAction.e.offsetHeight - (x - state.x) * (editorAction.e.offsetHeight / editorAction.e.offsetWidth);
                    }
                }

                if (editorAction.d == 's-resize' || editorAction.d == 'se-resize' || editorAction.d == 'sw-resize') {
                    if (! height) {
                        height = editorAction.e.offsetHeight + (y - state.y);
                    }
                } else if (editorAction.d === 'n-resize' || editorAction.d == 'ne-resize' || editorAction.d == 'nw-resize') {
                    top = editorAction.e.offsetTop + (y - state.y);
                    height = editorAction.e.offsetTop + editorAction.e.offsetHeight - top;
                }

                if (top) {
                    editorAction.e.style.top = top + 'px';
                }
                if (left) {
                    editorAction.e.style.left = left + 'px';
                }
                if (width) {
                    editorAction.e.style.width = width + 'px';
                }
                if (height) {
                    editorAction.e.style.height = height + 'px';
                }

                // Update element
                if (typeof(editorAction.e.refresh) == 'function') {
                    state.actioned = true;
                    editorAction.e.refresh('dimensions', width, height);
                }
            }

            state.x = x;
            state.y = y;
        } else {
            let element = getElement(e);
            // Resize action
            let item = jsuites_jSuites.findElement(element, 'jpanel');
            // Found eligible component
            if (item) {
                // Resizing action
                let controls = item.classList.contains('jpanel-controls');
                if (controls) {
                    let position = element.getAttribute('data-position');
                    if (position) {
                        item.style.cursor = position;
                    } else {
                        item.style.cursor = '';
                    }
                } else if (item.getAttribute('tabindex')) {
                    let rect = item.getBoundingClientRect();
                    //console.log(e.clientY - rect.top, rect.width - (e.clientX - rect.left), cornerSize)
                    if (e.clientY - rect.top < cornerSize) {
                        if (rect.width - (e.clientX - rect.left) < cornerSize) {
                            item.style.cursor = 'ne-resize';
                        } else if (e.clientX - rect.left < cornerSize) {
                            item.style.cursor = 'nw-resize';
                        } else {
                            item.style.cursor = 'n-resize';
                        }
                    } else if (rect.height - (e.clientY - rect.top) < cornerSize) {
                        if (rect.width - (e.clientX - rect.left) < cornerSize) {
                            item.style.cursor = 'se-resize';
                        } else if (e.clientX - rect.left < cornerSize) {
                            item.style.cursor = 'sw-resize';
                        } else {
                            item.style.cursor = 's-resize';
                        }
                    } else if (rect.width - (e.clientX - rect.left) < cornerSize) {
                        item.style.cursor = 'e-resize';
                    } else if (e.clientX - rect.left < cornerSize) {
                        item.style.cursor = 'w-resize';
                    } else {
                        item.style.cursor = '';
                    }
                }
            }
        }
    }

    let position = ['n','ne','e','se','s','sw','w','nw','rotate'];
    position.forEach(function(v, k) {
        position[k] = document.createElement('div');
        position[k].classList.add('jpanel-action');
        if (v === 'rotate') {
            position[k].setAttribute('data-action', 'rotate');
        } else {
            position[k].setAttribute('data-action', 'resize');
            position[k].setAttribute('data-position', v + '-resize');
        }
    });

    let currentElement;

    const focus = function(e) {
        let element = getElement(e);
        // Check if this is floating
        let item = jsuites_jSuites.findElement(element, 'jpanel');
        if (item && ! item.classList.contains("readonly") && item.classList.contains('jpanel-controls')) {
            item.append(...position);

            if (! item.classList.contains('jpanel-rotate')) {
                position[position.length-1].remove();
            }

            currentElement = item;
        } else {
            blur(e);
        }
    }

    const blur = function(e) {
        if (currentElement) {
            position.forEach(function(v) {
                v.remove();
            });
            currentElement = null;
        }
    }

    const mouseOver = function(e) {
        let element = getElement(e);
        var message = element.getAttribute('data-tooltip');
        if (message) {
            // Instructions
            tooltip.innerText = message;

            // Position
            if (e.changedTouches && e.changedTouches[0]) {
                var x = e.changedTouches[0].clientX;
                var y = e.changedTouches[0].clientY;
            } else {
                var x = e.clientX;
                var y = e.clientY;
            }

            tooltip.style.top = y + 'px';
            tooltip.style.left = x + 'px';
            document.body.appendChild(tooltip);
        } else if (tooltip.innerText) {
            tooltip.innerText = '';
            document.body.removeChild(tooltip);
        }
    }

    const contextMenu = function(e) {
        var item = document.activeElement;
        if (item && typeof(item.contextmenu) == 'function') {
            // Create edition
            item.contextmenu(e);

            e.preventDefault();
            e.stopImmediatePropagation();
        } else {
            // Search for possible context menus
            item = jsuites_jSuites.findElement(e.target, function(o) {
                return o.tagName && o.getAttribute('aria-contextmenu-id');
            });

            if (item) {
                var o = document.querySelector('#' + item);
                if (! o) {
                    console.error('JSUITES: contextmenu id not found: ' + item);
                } else {
                    o.contextmenu.open(e);
                    e.preventDefault();
                    e.stopImmediatePropagation();
                }
            }
        }
    }

    const keyDown = function(e) {
        let item = document.activeElement;
        if (item) {
            if (e.key === "Delete" && typeof(item.delete) == 'function') {
                item.delete();
                e.preventDefault();
                e.stopImmediatePropagation();
            }
        }

        let state = window['jSuitesStateControl'];
        if (state && state.length > 0) {
            item = state[state.length - 1];
            if (item) {
                if (e.key === "Escape" && typeof(item.isOpened) == 'function' && typeof(item.close) == 'function') {
                    if (item.isOpened()) {
                        item.close();
                        e.preventDefault();
                        e.stopImmediatePropagation();
                    }
                }
            }
        }
    }

    const input = function(e) {
        if (e.target.getAttribute('data-mask') || e.target.mask) {
            jsuites_jSuites.mask(e);
        }
    }

    document.addEventListener('focusin', focus);
    document.addEventListener('mouseup', mouseUp);
    document.addEventListener("mousedown", mouseDown);
    document.addEventListener('mousemove', mouseMove);
    document.addEventListener('mouseover', mouseOver);
    document.addEventListener('keydown', keyDown);
    document.addEventListener('contextmenu', contextMenu);
    document.addEventListener('input', input);
}

if (typeof(document) !== "undefined") {
    Events();
}

/* harmony default export */ var jsuites = (jsuites_jSuites);
}();
jSuites = __webpack_exports__["default"];
/******/ })()
;

    return jSuites;
})));
  // --- END jSuites unminified code ---

  // --- START jSpreadsheet unminified code ---
  var jSuites = typeof window !== 'undefined' ? window.jSuites : (typeof globalThis !== 'undefined' ? globalThis.jSuites : undefined);
  if (! jSuites && typeof(require) === 'function') {
    jSuites = require('jsuites');
  }

  var formula = typeof window !== 'undefined' ? window.formula : (typeof globalThis !== 'undefined' ? globalThis.formula : undefined);
  if (! formula && typeof(require) === 'function') {
    formula = require('@jspreadsheet/formula');
  }

;(function (global, factory) {
    typeof exports === 'object' && typeof module !== 'undefined' ? module.exports = factory() :
    typeof define === 'function' && define.amd ? define(factory) :
    global.jspreadsheet = factory();
}(this, (function () {

var jspreadsheet;(function(){"use strict";var __webpack_modules__={805:function(e,t){const s=function(e){const t=this,s=[];for(let n=0;n<e.length;n++){const o=e[n].x,r=e[n].y,l=t.options.columns[o].name?t.options.columns[o].name:o;s[r]||(s[r]={row:r,data:{}}),s[r].data[l]=e[n].value}return s.filter((function(e){return null!=e}))},n=function(e,t){const s=this,n=o.call(s.parent,"onbeforesave",s.parent,s,t);if(n)t=n;else if(!1===n)return!1;jSuites.ajax({url:e,method:"POST",dataType:"json",data:{data:JSON.stringify(t)},success:function(e){o.call(s,"onsave",s.parent,s,t)}})},o=function(e){const t=this;let o=null,r=t.parent?t.parent:t;if(!r.ignoreEvents&&("function"==typeof r.config.onevent&&(o=r.config.onevent.apply(this,arguments)),"function"==typeof r.config[e]&&(o=r.config[e].apply(this,Array.prototype.slice.call(arguments,1))),"object"==typeof r.plugins)){const e=Object.keys(r.plugins);for(let t=0;t<e.length;t++){const s=e[t],n=r.plugins[s];"function"==typeof n.onevent&&(o=n.onevent.apply(this,arguments))}}if("onafterchanges"==e){const e=arguments;if("object"==typeof r.plugins&&Object.entries(r.plugins).forEach((function([,s]){"function"==typeof s.persistence&&s.persistence(t,"setValue",{data:e[2]})})),t.options.persistence){const e=1==t.options.persistence?t.options.url:t.options.persistence,o=s.call(t,arguments[2]);n.call(t,e,o)}}return o};t.A=o},829:function(e,t,s){s.d(t,{F8:function(){return l},N$:function(){return r},dr:function(){return i}});var n=s(530),o=s(657);const r=function(e){const t=this;if(t.options.filters){e=parseInt(e),t.resetSelection();let s=[];if("checkbox"==t.options.columns[e].type)s.push({id:"true",name:"True"}),s.push({id:"false",name:"False"});else{const n=[];let o=!1;for(let s=0;s<t.options.data.length;s++){const r=t.options.data[s][e],l=t.records[s][e].element.innerHTML;r&&l?n[r]=l:o=!0}const r=Object.keys(n);s=[];for(let e=0;e<r.length;e++)s.push({id:r[e],name:n[r[e]]});o&&s.push({value:"",id:"",name:"(Blanks)"})}const n=document.createElement("div");t.filter.children[e+1].innerHTML="",t.filter.children[e+1].appendChild(n),t.filter.children[e+1].style.paddingLeft="0px",t.filter.children[e+1].style.paddingRight="0px",t.filter.children[e+1].style.overflow="initial";const r={data:s,multiple:!0,autocomplete:!0,opened:!0,value:void 0!==t.filters[e]?t.filters[e]:null,width:"100%",position:1==t.options.tableOverflow||1==t.parent.config.fullscreen,onclose:function(s){i.call(t),t.filters[e]=s.dropdown.getValue(!0),t.filter.children[e+1].innerHTML=s.dropdown.getText(),t.filter.children[e+1].style.paddingLeft="",t.filter.children[e+1].style.paddingRight="",t.filter.children[e+1].style.overflow="",l.call(t,e),o.G9.call(t)}};jSuites.dropdown(n,r)}else console.log("Jspreadsheet: filters not enabled.")},l=function(e){const t=this;if(!e)for(let s=0;s<t.filter.children.length;s++)t.filters[s]&&(e=s);const s=function(e,s,n){for(let o=0;o<e.length;o++){const r=""+t.options.data[n][s],l=""+t.records[n][s].element.innerHTML;if(e[o]==r||e[o]==l)return!0}return!1},o=t.filters[e];t.results=[];for(let n=0;n<t.options.data.length;n++)s(o,e,n)&&t.results.push(n);t.results.length||(t.results=null),n.hG.call(t)},i=function(){const e=this;if(e.options.filters)for(let t=0;t<e.filter.children.length;t++)e.filter.children[t].innerHTML="&nbsp;",e.filters[t]=null;e.results=null,n.hG.call(e)}},160:function(e,t,s){s.d(t,{e:function(){return o}});var n=s(530);const o=function(e){const t=this;if(e&&(t.options.footers=e),t.options.footers){t.tfoot||(t.tfoot=document.createElement("tfoot"),t.table.appendChild(t.tfoot));for(let e=0;e<t.options.footers.length;e++){let s;if(t.tfoot.children[e])s=t.tfoot.children[e];else{s=document.createElement("tr");const e=document.createElement("td");s.appendChild(e),t.tfoot.appendChild(s)}for(let o=0;o<t.headers.length;o++){let r;if(t.options.footers[e][o]||(t.options.footers[e][o]=""),t.tfoot.children[e].children[o+1])r=t.tfoot.children[e].children[o+1];else{r=document.createElement("td"),s.appendChild(r);const e=t.options.columns[o].align||t.options.defaultColAlign||"center";r.style.textAlign=e}r.textContent=n.$x.call(t,+t.records.length+o,e,t.options.footers[e][o]),r.style.display=t.cols[o].colElement.style.display}}}}},296:function(e,t,s){s.d(t,{w:function(){return n}});const n=function(){const e=this;let t=0;if(e.options.freezeColumns>0)for(let s=0;s<e.options.freezeColumns;s++){let n;n=e.options.columns&&e.options.columns[s]&&void 0!==e.options.columns[s].width?parseInt(e.options.columns[s].width):void 0!==e.options.defaultColWidth?parseInt(e.options.defaultColWidth):100,t+=n}return t}},978:function(e,t,s){s.r(t),s.d(t,{createFromTable:function(){return u},getCaretIndex:function(){return o},getCellNameFromCoords:function(){return i},getColumnName:function(){return l},getCoordsFromCellName:function(){return a},getCoordsFromRange:function(){return c},invert:function(){return r},parseCSV:function(){return d}});var n=s(689);const o=function(e){let t;t=this.config.root?this.config.root:window;let s=0;const n=t.getSelection();if(n&&0!==n.rangeCount){const t=n.getRangeAt(0),o=t.cloneRange();o.selectNodeContents(e),o.setEnd(t.endContainer,t.endOffset),s=o.toString().length}return s},r=function(e){const t=[],s=Object.keys(e);for(let n=0;n<s.length;n++)t[e[s[n]]]=s[n];return t},l=function(e){let t,s=e+1,n="";for(;s>0;)t=(s-1)%26,n=String.fromCharCode(65+t).toString()+n,s=parseInt((s-t)/26);return n},i=function(e,t){return l(parseInt(e))+(parseInt(t)+1)},a=function(e){const t=/^[a-zA-Z]+/.exec(e);if(t){let s=0;for(let e=0;e<t[0].length;e++)s+=parseInt(t[0].charCodeAt(e)-64)*Math.pow(26,t[0].length-1-e);s--,s<0&&(s=0);let n=parseInt(/[0-9]+$/.exec(e))||null;return n>0&&n--,[s,n]}},c=function(e){const[t,s]=e.split(":");return[...a(t),...a(s)]},d=function(e,t){t=t||",",e=e.replace(/\r?\n$|\r$|\n$/g,"");const s=[];let n=!1,o=0,r=0,l=0;for(let i=0;i<e.length;i++){const a=e[i],c=e[i+1];s[r]=s[r]||[],s[r][l]=s[r][l]||"",'"'==a&&n&&'"'==c?(s[r][l]+=a,++i):'"'!=a?a!=t||n?"\r"!=a||"\n"!=c||n?"\n"==a&&!n||"\r"==a&&!n?(++r,o=Math.max(o,l),l=0):s[r][l]+=a:(++r,o=Math.max(o,l),l=0,++i):++l:n=!n}return s.forEach(((e,t)=>{for(let t=e.length;t<=o;t++)e.push("")})),s},u=function(e,t){if("TABLE"==e.tagName){t||(t={}),t.columns=[],t.data=[];const s=e.querySelectorAll("colgroup > col");if(s.length)for(let e=0;e<s.length;e++){let n=s[e].style.width;n||(n=s[e].getAttribute("width")),n&&(t.columns[e]||(t.columns[e]={}),t.columns[e].width=n)}const o=function(e,s){let n=e.getBoundingClientRect();const o=n.width>50?n.width:50;t.columns[s]||(t.columns[s]={}),e.getAttribute("data-celltype")?t.columns[s].type=e.getAttribute("data-celltype"):t.columns[s].type="text",t.columns[s].width=o+"px",t.columns[s].title=e.innerHTML,e.style.textAlign&&(t.columns[s].align=e.style.textAlign),(n=e.getAttribute("name"))&&(t.columns[s].name=n),(n=e.getAttribute("id"))&&(t.columns[s].id=n),(n=e.getAttribute("data-mask"))&&(t.columns[s].mask=n)},r=[];let l=e.querySelectorAll(":scope > thead > tr");if(l.length){for(let e=0;e<l.length-1;e++){const t=[];for(let s=0;s<l[e].children.length;s++){const n={title:l[e].children[s].textContent,colspan:l[e].children[s].getAttribute("colspan")||1};t.push(n)}r.push(t)}l=l[l.length-1].children;for(let e=0;e<l.length;e++)o(l[e],e)}let i=0;const a={},c={},d={},u={};let p=e.querySelectorAll(":scope > tr, :scope > tbody > tr");for(let e=0;e<p.length;e++)if(t.data[i]=[],1!=t.parseTableFirstRowAsHeader||l.length||0!=e){for(let s=0;s<p[e].children.length;s++){let o=p[e].children[s].getAttribute("data-formula");o?"="!=o.substr(0,1)&&(o="="+o):o=p[e].children[s].innerHTML,t.data[i].push(o);const r=(0,n.t3)([s,e]),l=p[e].children[s].getAttribute("class");l&&(u[r]=l);const c=parseInt(p[e].children[s].getAttribute("colspan"))||0,h=parseInt(p[e].children[s].getAttribute("rowspan"))||0;(c||h)&&(a[r]=[c||1,h||1]),p[e].children[s].style&&"none"==p[e].children[s].style.display&&(p[e].children[s].style.display="");const m=p[e].children[s].getAttribute("style");m&&(d[r]=m),p[e].children[s].classList.contains("styleBold")&&(d[r]?d[r]+="; font-weight:bold;":d[r]="font-weight:bold;")}p[e].style&&p[e].style.height&&(c[e]={height:p[e].style.height}),i++}else for(let t=0;t<p[e].children.length;t++)o(p[e].children[t],t);if(Object.keys(r).length>0&&(t.nestedHeaders=r),Object.keys(d).length>0&&(t.style=d),Object.keys(a).length>0&&(t.mergeCells=a),Object.keys(c).length>0&&(t.rows=c),Object.keys(u).length>0&&(t.classes=u),p=e.querySelectorAll("tfoot tr"),p.length){const e=[];for(let t=0;t<p.length;t++){let s=[];for(let e=0;e<p[t].children.length;e++)s.push(p[t].children[e].textContent);e.push(s)}Object.keys(e).length>0&&(t.footers=e)}if(1==t.parseTableAutoCellType){const e=[];for(let s=0;s<t.columns.length;s++){let n=!0,o=!0;e[s]=[];for(let r=0;r<t.data.length;r++){const l=t.data[r][s];e[s][l]||(e[s][l]=0),e[s][l]++,l.length>25&&(n=!1),10==l.length&&"-"==l.substr(4,1)&&"-"==l.substr(7,1)||(o=!1)}const r=Object.keys(e[s]).length;o?t.columns[s].type="calendar":1==n&&r>1&&r<=parseInt(.1*t.data.length)&&(t.columns[s].type="dropdown",t.columns[s].source=Object.keys(e[s]))}}return t}console.log("Element is not a table")}},911:function(e,t,s){s.d(t,{Dh:function(){return c},ZS:function(){return h},tN:function(){return p}});var n=s(805),o=s(689),r=s(530),l=s(910),i=s(94),a=s(657);const c=function(e){const t=this;if(1!=t.ignoreHistory){const s=++t.historyIndex;t.history=t.history=t.history.slice(0,s+1),t.history[s]=e}},d=function(e,t){const s=this,n=t.insertBefore?+t.rowNumber:t.rowNumber+1;if(1==s.options.search&&s.results&&s.results.length!=s.rows.length&&s.resetSearch(),1==e){const e=t.numOfRows;for(let t=n;t<e+n;t++)s.rows[t].element.parentNode.removeChild(s.rows[t].element);s.records.splice(n,e),s.options.data.splice(n,e),s.rows.splice(n,e),a.at.call(s,1,n,e+n-1)}else{const e=t.rowRecords.map((e=>[...e]));s.records=(0,o.Hh)(s.records,n,e);const r=t.rowData.map((e=>[...e]));s.options.data=(0,o.Hh)(s.options.data,n,r),s.rows=(0,o.Hh)(s.rows,n,t.rowNode);let l=0;for(let e=n;e<t.numOfRows+n;e++)s.tbody.insertBefore(t.rowNode[l].element,s.tbody.children[e]),l++}for(let e=n;e<s.rows.length;e++)s.rows[e].y=e;for(let e=n;e<s.records.length;e++)for(let t=0;t<s.records[e].length;t++)s.records[e][t].y=e;s.options.pagination>0&&s.page(s.pageNumber),r.o8.call(s)},u=function(e,t){const s=this,n=t.insertBefore?t.columnNumber:t.columnNumber+1;if(1==e){const e=t.numOfColumns;s.options.columns.splice(n,e);for(let t=n;t<e+n;t++)s.headers[t].parentNode.removeChild(s.headers[t]),s.cols[t].colElement.parentNode.removeChild(s.cols[t].colElement);s.headers.splice(n,e),s.cols.splice(n,e);for(let o=0;o<t.data.length;o++){for(let t=n;t<e+n;t++)s.records[o][t].element.parentNode.removeChild(s.records[o][t].element);s.records[o].splice(n,e),s.options.data[o].splice(n,e)}if(s.options.footers)for(let t=0;t<s.options.footers.length;t++)s.options.footers[t].splice(n,e)}else{s.options.columns=(0,o.Hh)(s.options.columns,n,t.columns),s.headers=(0,o.Hh)(s.headers,n,t.headers),s.cols=(0,o.Hh)(s.cols,n,t.cols);let e=0;for(let o=n;o<t.numOfColumns+n;o++)s.headerContainer.insertBefore(t.headers[e],s.headerContainer.children[o+1]),s.colgroupContainer.insertBefore(t.cols[e].colElement,s.colgroupContainer.children[o+1]),e++;for(let e=0;e<t.data.length;e++){s.options.data[e]=(0,o.Hh)(s.options.data[e],n,t.data[e]),s.records[e]=(0,o.Hh)(s.records[e],n,t.records[e]);let r=0;for(let o=n;o<t.numOfColumns+n;o++)s.rows[e].element.insertBefore(t.records[e][r].element,s.rows[e].element.children[o+1]),r++}if(s.options.footers)for(let e=0;e<s.options.footers.length;e++)s.options.footers[e]=(0,o.Hh)(s.options.footers[e],n,t.footers[e])}for(let e=n;e<s.cols.length;e++)s.cols[e].x=e;for(let e=0;e<s.records.length;e++)for(let t=n;t<s.records[e].length;t++)s.records[e][t].x=t;if(s.options.nestedHeaders&&s.options.nestedHeaders.length>0&&s.options.nestedHeaders[0]&&s.options.nestedHeaders[0][0])for(let n=0;n<s.options.nestedHeaders.length;n++){let o;o=1==e?parseInt(s.options.nestedHeaders[n][s.options.nestedHeaders[n].length-1].colspan)-t.numOfColumns:parseInt(s.options.nestedHeaders[n][s.options.nestedHeaders[n].length-1].colspan)+t.numOfColumns,s.options.nestedHeaders[n][s.options.nestedHeaders[n].length-1].colspan=o,s.thead.children[n].children[s.thead.children[n].children.length-1].setAttribute("colspan",o)}r.o8.call(s)},p=function(){const e=this,t=!!e.parent.ignoreEvents,s=!!e.ignoreHistory;e.parent.ignoreEvents=!0,e.ignoreHistory=!0;const o=[];let r;if(e.historyIndex>=0)if(r=e.history[e.historyIndex--],"insertRow"==r.action)d.call(e,1,r);else if("deleteRow"==r.action)d.call(e,0,r);else if("insertColumn"==r.action)u.call(e,1,r);else if("deleteColumn"==r.action)u.call(e,0,r);else if("moveRow"==r.action)e.moveRow(r.newValue,r.oldValue);else if("moveColumn"==r.action)e.moveColumn(r.newValue,r.oldValue);else if("setMerge"==r.action)e.removeMerge(r.column,r.data);else if("setStyle"==r.action)e.setStyle(r.oldValue,null,null,1);else if("setWidth"==r.action)e.setWidth(r.column,r.oldValue);else if("setHeight"==r.action)e.setHeight(r.row,r.oldValue);else if("setHeader"==r.action)e.setHeader(r.column,r.oldValue);else if("setComments"==r.action)e.setComments(r.oldValue);else if("orderBy"==r.action){let t=[];for(let e=0;e<r.rows.length;e++)t[r.rows[e]]=e;i.Th.call(e,r.column,r.order?0:1),i.iY.call(e,t)}else if("setValue"==r.action){for(let t=0;t<r.records.length;t++)o.push({x:r.records[t].x,y:r.records[t].y,value:r.records[t].oldValue}),r.oldStyle&&e.resetStyle(r.oldStyle);e.setValue(o),r.selection&&e.updateSelectionFromCoords(r.selection[0],r.selection[1],r.selection[2],r.selection[3])}e.parent.ignoreEvents=t,e.ignoreHistory=s,n.A.call(e,"onundo",e,r)},h=function(){const e=this,t=!!e.parent.ignoreEvents,s=!!e.ignoreHistory;let o;if(e.parent.ignoreEvents=!0,e.ignoreHistory=!0,e.historyIndex<e.history.length-1)if(o=e.history[++e.historyIndex],"insertRow"==o.action)d.call(e,0,o);else if("deleteRow"==o.action)d.call(e,1,o);else if("insertColumn"==o.action)u.call(e,0,o);else if("deleteColumn"==o.action)u.call(e,1,o);else if("moveRow"==o.action)e.moveRow(o.oldValue,o.newValue);else if("moveColumn"==o.action)e.moveColumn(o.oldValue,o.newValue);else if("setMerge"==o.action)l.FU.call(e,o.column,o.colspan,o.rowspan,1);else if("setStyle"==o.action)e.setStyle(o.newValue,null,null,1);else if("setWidth"==o.action)e.setWidth(o.column,o.newValue);else if("setHeight"==o.action)e.setHeight(o.row,o.newValue);else if("setHeader"==o.action)e.setHeader(o.column,o.newValue);else if("setComments"==o.action)e.setComments(o.newValue);else if("orderBy"==o.action)i.Th.call(e,o.column,o.order),i.iY.call(e,o.rows);else if("setValue"==o.action){e.setValue(o.records);for(let t=0;t<o.records.length;t++)o.oldStyle&&e.resetStyle(o.newStyle);o.selection&&e.updateSelectionFromCoords(o.selection[0],o.selection[1],o.selection[2],o.selection[3])}e.parent.ignoreEvents=t,e.ignoreHistory=s,n.A.call(e,"onredo",e,o)}},530:function(__unused_webpack_module,__webpack_exports__,__webpack_require__){__webpack_require__.d(__webpack_exports__,{$O:function(){return getWorksheetActive},$x:function(){return parseValue},C6:function(){return showIndex},Em:function(){return executeFormula},P9:function(){return createCell},Rs:function(){return updateScroll},TI:function(){return hideIndex},Xr:function(){return getCellFromCoords},Y5:function(){return fullscreen},am:function(){return updateTable},dw:function(){return isFormula},eN:function(){return getWorksheetInstance},hG:function(){return updateResult},ju:function(){return createNestedHeader},k9:function(){return updateCell},o8:function(){return updateTableReferences},p9:function(){return getLabel},rS:function(){return getMask},tT:function(){return getCell},xF:function(){return updateFormulaChain},yB:function(){return updateFormula}});var _dispatch_js__WEBPACK_IMPORTED_MODULE_3__=__webpack_require__(805),_selection_js__WEBPACK_IMPORTED_MODULE_1__=__webpack_require__(657),_helpers_js__WEBPACK_IMPORTED_MODULE_4__=__webpack_require__(978),_meta_js__WEBPACK_IMPORTED_MODULE_5__=__webpack_require__(654),_freeze_js__WEBPACK_IMPORTED_MODULE_6__=__webpack_require__(296),_pagination_js__WEBPACK_IMPORTED_MODULE_7__=__webpack_require__(167),_footer_js__WEBPACK_IMPORTED_MODULE_0__=__webpack_require__(160),_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__=__webpack_require__(689);const updateTable=function(){const e=this;if(e.options.minSpareRows>0){let t=0;for(let s=e.rows.length-1;s>=0;s--){let n=!1;for(let t=0;t<e.headers.length;t++)e.options.data[s][t]&&(n=!0);if(n)break;t++}e.options.minSpareRows-t>0&&e.insertRow(e.options.minSpareRows-t)}if(e.options.minSpareCols>0){let t=0;for(let s=e.headers.length-1;s>=0;s--){let n=!1;for(let t=0;t<e.rows.length;t++)e.options.data[t][s]&&(n=!0);if(n)break;t++}e.options.minSpareCols-t>0&&e.insertColumn(e.options.minSpareCols-t)}e.options.footers&&_footer_js__WEBPACK_IMPORTED_MODULE_0__.e.call(e),setTimeout((function(){_selection_js__WEBPACK_IMPORTED_MODULE_1__.Aq.call(e)}),0)},parseNumber=function(e,t){const s=t&&this.options.columns[t].decimal?this.options.columns[t].decimal:".";let n=""+e;return n=n.split(s),n[0]=n[0].match(/[+-]?[0-9]/g),n[0]&&(n[0]=n[0].join("")),n[1]&&(n[1]=n[1].match(/[0-9]*/g).join("")),n[0]&&Number.isInteger(Number(n[0]))?n[1]?Number(n[0]+"."+n[1]):Number(n[0]+".00"):null},executeFormula=function(expression,x,y){const obj=this,formulaResults=[],formulaLoopProtection=[],execute=function(expression,x,y){const parentId=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([x,y]);if(formulaLoopProtection[parentId])return console.error("Reference loop detected"),"#ERROR";formulaLoopProtection[parentId]=!0;const tokensUpdate=function(e){for(let t=0;t<e.length;t++){const s=[],n=e[t].split(":"),o=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(n[0],!0),r=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(n[1],!0);let l,i,a,c;o[0]<=r[0]?(l=o[0],i=r[0]):(l=r[0],i=o[0]),o[1]<=r[1]?(a=o[1],c=r[1]):(a=r[1],c=o[1]);for(let e=a;e<=c;e++)for(let t=l;t<=i;t++)s.push((0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([t,e]));expression=expression.replace(e[t],s.join(","))}};expression=expression.replace(/\$?([A-Z]+)\$?([0-9]+)/g,"$1$2");let tokens=expression.match(/([A-Z]+[0-9]+)\:([A-Z]+[0-9]+)/g);if(tokens&&tokens.length&&tokensUpdate(tokens),tokens=expression.match(/([A-Z]+[0-9]+)/g),tokens&&tokens.indexOf(parentId)>-1)return console.error("Self Reference detected"),"#ERROR";{const formulaExpressions={};if(tokens)for(let i=0;i<tokens.length;i++)if(obj.formula[tokens[i]]||(obj.formula[tokens[i]]=[]),obj.formula[tokens[i]].indexOf(parentId)<0&&obj.formula[tokens[i]].push(parentId),eval("typeof("+tokens[i]+') == "undefined"')){const e=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(tokens[i],1);let t;if(t=void 0!==obj.options.data[e[1]]&&void 0!==obj.options.data[e[1]][e[0]]?obj.options.data[e[1]][e[0]]:"","="==(""+t).substr(0,1)&&(void 0!==formulaResults[tokens[i]]?t=formulaResults[tokens[i]]:(t=execute(t,e[0],e[1]),formulaResults[tokens[i]]=t)),""==(""+t).trim())formulaExpressions[tokens[i]]=null;else if(t==Number(t)&&0!=obj.parent.config.autoCasting)formulaExpressions[tokens[i]]=Number(t);else{const s=parseNumber.call(obj,t,e[0]);0!=obj.parent.config.autoCasting&&s?formulaExpressions[tokens[i]]=s:formulaExpressions[tokens[i]]='"'+t+'"'}}const ret=_dispatch_js__WEBPACK_IMPORTED_MODULE_3__.A.call(obj,"onbeforeformula",obj,expression,x,y);if(!1===ret)return expression;let res;ret&&(expression=ret);try{res=formula(expression.substr(1),formulaExpressions,x,y,obj),"function"==typeof res&&(res="#ERROR")}catch(e){res="#ERROR",!0===obj.parent.config.debugFormulas&&console.log(expression.substr(1),formulaExpressions,e)}return res}};return execute(expression,x,y)},parseValue=function(e,t,s,n){const o=this;"="==(""+s).substr(0,1)&&0!=o.parent.config.parseFormulas&&(s=executeFormula.call(o,s,e,t));const r=o.options.columns&&o.options.columns[e];if(r&&!isFormula(s)){let e=null;if(e=getMask(r)){s&&s==Number(s)&&(s=Number(s));let t=jSuites.mask.render(s,e,!0);if(n&&e.mask){const o=e.mask.split(";");o[1]&&(o[1].match(new RegExp("\\[Red\\]","gi"))&&(s<0?n.classList.add("red"):n.classList.remove("red")),o[1].match(new RegExp("\\(","gi"))&&s<0&&(t="("+t+")"))}t&&(s=t)}}return s},getDropDownValue=function(e,t){const s=this,n=[];if(s.options.columns&&s.options.columns[e]&&s.options.columns[e].source){const o=[],r=s.options.columns[e].source;for(let e=0;e<r.length;e++)"object"==typeof r[e]?o[r[e].id]=r[e].name:o[r[e]]=r[e];const l=Array.isArray(t)?t:(""+t).split(";");for(let e=0;e<l.length;e++)"object"==typeof l[e]?n.push(o[l[e].id]):o[l[e]]&&n.push(o[l[e]])}else console.error("Invalid column");return n.length>0?n.join("; "):""},validDate=function(e){return"-"==(e=""+e).substr(4,1)&&"-"==e.substr(7,1)||4==(e=e.split("-"))[0].length&&e[0]==Number(e[0])&&2==e[1].length&&e[1]==Number(e[1])},stripScript=function(e){const t=new Option;t.innerHTML=e;let s=null;for(e=t.getElementsByTagName("script");s=e[0];)s.parentNode.removeChild(s);return t.innerHTML},createCell=function(e,t,s){const n=this;let o=document.createElement("td");if(o.setAttribute("data-x",e),o.setAttribute("data-y",t),"none"===n.headers[e].style.display&&(o.style.display="none"),"="==(""+s).substr(0,1)&&1==n.options.secureFormulas){const e=secureFormula(s);e!=s&&(s=e)}if(n.options.columns&&n.options.columns[e]&&"object"==typeof n.options.columns[e].type)!0===n.parent.config.parseHTML?o.innerHTML=s:o.textContent=s,"function"==typeof n.options.columns[e].type.createCell&&n.options.columns[e].type.createCell(o,s,parseInt(e),parseInt(t),n,n.options.columns[e]);else if(n.options.columns&&n.options.columns[e]&&"hidden"==n.options.columns[e].type)o.style.display="none",o.textContent=s;else if(n.options.columns&&n.options.columns[e]&&("checkbox"==n.options.columns[e].type||"radio"==n.options.columns[e].type)){const r=document.createElement("input");r.type=n.options.columns[e].type,r.name="c"+e,r.checked=1==s||1==s||"true"==s,r.onclick=function(){n.setValue(o,this.checked)},1!=n.options.columns[e].readOnly&&0!=n.options.editable||r.setAttribute("disabled","disabled"),o.appendChild(r),n.options.data[t][e]=r.checked}else if(n.options.columns&&n.options.columns[e]&&"calendar"==n.options.columns[e].type){let t=null;if(!validDate(s)){const o=jSuites.calendar.extractDateFromString(s,n.options.columns[e].options&&n.options.columns[e].options.format||"YYYY-MM-DD");o&&(t=o)}o.textContent=jSuites.calendar.getDateString(t||s,n.options.columns[e].options&&n.options.columns[e].options.format)}else if(n.options.columns&&n.options.columns[e]&&"dropdown"==n.options.columns[e].type)o.classList.add("jss_dropdown"),o.textContent=getDropDownValue.call(n,e,s);else if(n.options.columns&&n.options.columns[e]&&"color"==n.options.columns[e].type)if("square"==n.options.columns[e].render){const e=document.createElement("div");e.className="color",e.style.backgroundColor=s,o.appendChild(e)}else o.style.color=s,o.textContent=s;else if(n.options.columns&&n.options.columns[e]&&"image"==n.options.columns[e].type){if(s&&"data:image"==s.substr(0,10)){const e=document.createElement("img");e.src=s,o.appendChild(e)}}else n.options.columns&&n.options.columns[e]&&"html"==n.options.columns[e].type||!0===n.parent.config.parseHTML?o.innerHTML=stripScript(parseValue.call(this,e,t,s,o)):o.textContent=parseValue.call(this,e,t,s,o);n.options.columns&&n.options.columns[e]&&1==n.options.columns[e].readOnly&&(o.className="readonly");const r=n.options.columns&&n.options.columns[e]&&n.options.columns[e].align||n.options.defaultColAlign||"center";return o.style.textAlign=r,n.options.columns&&n.options.columns[e]&&0==n.options.columns[e].wordWrap||!(1==n.options.wordWrap||n.options.columns&&n.options.columns[e]&&1==n.options.columns[e].wordWrap||o.innerHTML.length>200)||(o.style.whiteSpace="pre-wrap"),e>0&&1==this.options.textOverflow&&(s||o.innerHTML?n.records[t][e-1].element.style.overflow="hidden":e==n.options.columns.length-1&&(o.style.overflow="hidden")),_dispatch_js__WEBPACK_IMPORTED_MODULE_3__.A.call(n,"oncreatecell",n,o,e,t,s),o},updateCell=function(e,t,s,n){const o=this;let r;if(1!=o.records[t][e].element.classList.contains("readonly")||n){if("="==(""+s).substr(0,1)&&1==o.options.secureFormulas){const e=secureFormula(s);e!=s&&(s=e)}const n=_dispatch_js__WEBPACK_IMPORTED_MODULE_3__.A.call(o,"onbeforechange",o,o.records[t][e].element,e,t,s);if(null!=n&&(s=n),o.options.columns&&o.options.columns[e]&&"object"==typeof o.options.columns[e].type&&"function"==typeof o.options.columns[e].type.updateCell){const n=o.options.columns[e].type.updateCell(o.records[t][e].element,s,parseInt(e),parseInt(t),o,o.options.columns[e]);void 0!==n&&(s=n)}r={x:e,y:t,col:e,row:t,value:s,oldValue:o.options.data[t][e]};let l=o.options.columns&&o.options.columns[e]&&"object"==typeof o.options.columns[e].type?o.options.columns[e].type:null;if(l)o.options.data[t][e]=s,"function"==typeof l.setValue&&l.setValue(o.records[t][e].element,s);else if(o.options.columns&&o.options.columns[e]&&("checkbox"==o.options.columns[e].type||"radio"==o.options.columns[e].type)){if("radio"==o.options.columns[e].type)for(let t=0;t<o.options.data.length;t++)o.options.data[t][e]=!1;o.records[t][e].element.children[0].checked=1==s||1==s||"true"==s||"TRUE"==s,o.options.data[t][e]=o.records[t][e].element.children[0].checked}else if(o.options.columns&&o.options.columns[e]&&"dropdown"==o.options.columns[e].type)o.options.data[t][e]=s,o.records[t][e].element.textContent=getDropDownValue.call(o,e,s);else if(o.options.columns&&o.options.columns[e]&&"calendar"==o.options.columns[e].type){let n=null;if(!validDate(s)){const t=jSuites.calendar.extractDateFromString(s,o.options.columns[e].options&&o.options.columns[e].options.format||"YYYY-MM-DD");t&&(n=t)}o.options.data[t][e]=s,o.records[t][e].element.textContent=jSuites.calendar.getDateString(n||s,o.options.columns[e].options&&o.options.columns[e].options.format)}else if(o.options.columns&&o.options.columns[e]&&"color"==o.options.columns[e].type)if(o.options.data[t][e]=s,"square"==o.options.columns[e].render){const n=document.createElement("div");n.className="color",n.style.backgroundColor=s,o.records[t][e].element.textContent="",o.records[t][e].element.appendChild(n)}else o.records[t][e].element.style.color=s,o.records[t][e].element.textContent=s;else if(o.options.columns&&o.options.columns[e]&&"image"==o.options.columns[e].type){if(s=""+s,o.options.data[t][e]=s,o.records[t][e].element.innerHTML="",s&&"data:image"==s.substr(0,10)){const n=document.createElement("img");n.src=s,o.records[t][e].element.appendChild(n)}}else o.options.data[t][e]=s,o.options.columns&&o.options.columns[e]&&"html"==o.options.columns[e].type?o.records[t][e].element.innerHTML=stripScript(parseValue.call(o,e,t,s)):!0===o.parent.config.parseHTML?o.records[t][e].element.innerHTML=stripScript(parseValue.call(o,e,t,s,o.records[t][e].element)):o.records[t][e].element.textContent=parseValue.call(o,e,t,s,o.records[t][e].element),o.options.columns&&o.options.columns[e]&&0==o.options.columns[e].wordWrap||!(1==o.options.wordWrap||o.options.columns&&o.options.columns[e]&&1==o.options.columns[e].wordWrap||o.records[t][e].element.innerHTML.length>200)?o.records[t][e].element.style.whiteSpace="":o.records[t][e].element.style.whiteSpace="pre-wrap";e>0&&(o.records[t][e-1].element.style.overflow=s?"hidden":""),o.options.columns&&o.options.columns[e]&&"function"==typeof o.options.columns[e].render&&o.options.columns[e].render(o.records[t]&&o.records[t][e]?o.records[t][e].element:null,s,parseInt(e),parseInt(t),o,o.options.columns[e]),_dispatch_js__WEBPACK_IMPORTED_MODULE_3__.A.call(o,"onchange",o,o.records[t]&&o.records[t][e]?o.records[t][e].element:null,e,t,s,r.oldValue)}else r={x:e,y:t,col:e,row:t};return r},isFormula=function(e){const t=(""+e)[0];return"="==t||"#"==t},getMask=function(e){if(e.format||e.mask||e.locale){const t={};return e.mask?t.mask=e.mask:e.format?t.mask=e.format:(t.locale=e.locale,t.options=e.options),e.decimal&&(t.options||(t.options={}),t.options={decimal:e.decimal}),t}return null},secureFormula=function(e){let t="",s=0;for(let n=0;n<e.length;n++)'"'==e[n]&&(s=0==s?1:0),t+=1==s?e[n]:e[n].toUpperCase();return t};let chainLoopProtection=[];const updateFormulaChain=function(e,t,s){const n=this,o=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([e,t]);if(n.formula[o]&&n.formula[o].length>0)if(chainLoopProtection[o])n.records[t][e].element.innerHTML="#ERROR",n.formula[o]="";else{chainLoopProtection[o]=!0;for(let e=0;e<n.formula[o].length;e++){const t=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(n.formula[o][e],!0),r=""+n.options.data[t[1]][t[0]];"="==r.substr(0,1)?s.push(updateCell.call(n,t[0],t[1],r,!0)):Object.keys(n.formula)[e]=null,updateFormulaChain.call(n,t[0],t[1],s)}}chainLoopProtection=[]},updateFormula=function(e,t){const s=/[A-Z]/,n=/[0-9]/;let o="",r=null,l=null,i="";for(let a=0;a<e.length;a++)s.exec(e[a])?(r=1,l=0,i+=e[a]):n.exec(e[a])?(l=r?1:0,i+=e[a]):(r&&l&&(i=t[i]?t[i]:i),o+=i,o+=e[a],r=0,l=0,i="");return i&&(r&&l&&(i=t[i]?t[i]:i),o+=i),o},updateFormulas=function(e){const t=this;for(let s=0;s<t.options.data.length;s++)for(let n=0;n<t.options.data[0].length;n++){const o=""+t.options.data[s][n];if("="==o.substr(0,1)){const r=updateFormula(o,e);r!=o&&(t.options.data[s][n]=r)}}const s=[],n=Object.keys(t.formula);for(let o=0;o<n.length;o++){let r=n[o];const l=t.formula[r];e[r]&&(r=e[r]),s[r]=[];for(let t=0;t<l.length;t++){let n=l[t];e[n]&&(n=e[n]),s[r].push(n)}}t.formula=s},updateTableReferences=function(){const e=this;if(e.skipUpdateTableReferences)return;for(let t=0;t<e.headers.length;t++)e.headers[t].getAttribute("data-x")!=t&&(e.headers[t].setAttribute("data-x",t),e.headers[t].getAttribute("title")||(e.headers[t].innerHTML=(0,_helpers_js__WEBPACK_IMPORTED_MODULE_4__.getColumnName)(t)));for(let t=0;t<e.rows.length;t++)e.rows[t]&&e.rows[t].element.getAttribute("data-y")!=t&&(e.rows[t].element.setAttribute("data-y",t),e.rows[t].element.children[0].setAttribute("data-y",t),e.rows[t].element.children[0].innerHTML=t+1);const t=[],s=[],n=function(s,n,o,r){if(s!=o&&e.records[r][o].element.setAttribute("data-x",o),n!=r&&e.records[r][o].element.setAttribute("data-y",r),s!=o||n!=r){const e=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([s,n]),l=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([o,r]);t[e]=l}};for(let t=0;t<e.records.length;t++)for(let o=0;o<e.records[0].length;o++)if(e.records[t][o]){const r=e.records[t][o].element.getAttribute("data-x"),l=e.records[t][o].element.getAttribute("data-y");if(e.records[t][o].element.getAttribute("data-merged")){const e=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([r,l]),n=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.t3)([o,t]);if(null==s[e])if(e==n)s[e]=!1;else{const i=parseInt(o-r),a=parseInt(t-l);s[e]=[n,i,a]}}else n(r,l,o,t)}const o=Object.keys(s);if(o.length)for(let t=0;t<o.length;t++)if(s[o[t]]){const r=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(o[t],!0);let l=r[0],i=r[1];n(l,i,l+s[o[t]][1],i+s[o[t]][2]);const a=o[t],c=s[o[t]][0];for(let n=0;n<e.options.mergeCells[a][2].length;n++)l=parseInt(e.options.mergeCells[a][2][n].getAttribute("data-x")),i=parseInt(e.options.mergeCells[a][2][n].getAttribute("data-y")),e.options.mergeCells[a][2][n].setAttribute("data-x",l+s[o[t]][1]),e.options.mergeCells[a][2][n].setAttribute("data-y",i+s[o[t]][2]);e.options.mergeCells[c]=e.options.mergeCells[a],delete e.options.mergeCells[a]}updateFormulas.call(e,t),_meta_js__WEBPACK_IMPORTED_MODULE_5__.hs.call(e,t),_selection_js__WEBPACK_IMPORTED_MODULE_1__.G9.call(e),updateTable.call(e)},updateScroll=function(e){const t=this,s=t.content.getBoundingClientRect(),n=s.left,o=s.top,r=s.width,l=s.height,i=t.records[t.selectedCell[3]][t.selectedCell[2]].element.getBoundingClientRect(),a=i.left,c=i.top,d=i.width,u=i.height;let p,h;0==e||1==e?(p=a-n+t.content.scrollLeft,h=c-o+t.content.scrollTop-2):(p=a-n+t.content.scrollLeft+d,h=c-o+t.content.scrollTop+u),h>t.content.scrollTop+30&&h<t.content.scrollTop+l||(h<t.content.scrollTop+30?t.content.scrollTop=h-u:t.content.scrollTop=h-(l-2));const m=_freeze_js__WEBPACK_IMPORTED_MODULE_6__.w.call(t);p>t.content.scrollLeft+m&&p<t.content.scrollLeft+r||(p<t.content.scrollLeft+30?(t.content.scrollLeft=p,t.content.scrollLeft<50&&(t.content.scrollLeft=0)):p<t.content.scrollLeft+m?t.content.scrollLeft=p-m-1:t.content.scrollLeft=p-(r-20))},updateResult=function(){const e=this;let t=0,s=0;for(t=1==e.options.lazyLoading?100:e.options.pagination>0?e.options.pagination:e.results?e.results.length:e.rows.length;e.tbody.firstChild;)e.tbody.removeChild(e.tbody.firstChild);for(let n=0;n<e.rows.length;n++)!e.results||e.results.indexOf(n)>-1?(s<t&&(e.tbody.appendChild(e.rows[n].element),s++),e.rows[n].element.style.display=""):e.rows[n].element.style.display="none";return e.options.pagination>0&&_pagination_js__WEBPACK_IMPORTED_MODULE_7__.IV.call(e),_selection_js__WEBPACK_IMPORTED_MODULE_1__.Aq.call(e),t},getCell=function(e,t){if("string"==typeof e){const s=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(e,!0);e=s[0],t=s[1]}return this.records[t][e].element},getCellFromCoords=function(e,t){return this.records[t][e].element},getLabel=function(e,t){if("string"==typeof e){const s=(0,_internalHelpers_js__WEBPACK_IMPORTED_MODULE_2__.vu)(e,!0);e=s[0],t=s[1]}return this.records[t][e].element.innerHTML},fullscreen=function(e){const t=this;null==e&&(e=!t.config.fullscreen),t.config.fullscreen!=e&&(t.config.fullscreen=e,1==e?t.element.classList.add("fullscreen"):t.element.classList.remove("fullscreen"))},showIndex=function(){this.table.classList.remove("jss_hidden_index")},hideIndex=function(){this.table.classList.add("jss_hidden_index")},createNestedHeader=function(e){const t=this,s=document.createElement("tr");s.classList.add("jss_nested");const n=document.createElement("td");n.classList.add("jss_selectall"),s.appendChild(n),e.element=s;let o=0;for(let n=0;n<e.length;n++){e[n].colspan||(e[n].colspan=1),e[n].title||(e[n].title=""),e[n].id||(e[n].id="");let r=e[n].colspan;const l=[];for(let e=0;e<r;e++)t.options.columns[o]&&"hidden"==t.options.columns[o].type&&r++,l.push(o),o++;const i=document.createElement("td");i.setAttribute("data-column",l.join(",")),i.setAttribute("colspan",e[n].colspan),i.setAttribute("align",e[n].align||"center"),i.setAttribute("id",e[n].id),i.textContent=e[n].title,s.appendChild(i)}return s},getWorksheetActive=function(){const e=this.parent?this.parent:this;return e.element.tabs?e.element.tabs.getActive():0},getWorksheetInstance=function(e){const t=void 0!==e?e:getWorksheetActive.call(this);return this.worksheets[t]}},689:function(e,t,s){s.d(t,{Hh:function(){return o},t3:function(){return l},vu:function(){return r}});var n=s(978);const o=function(e,t,s){if(t<=e.length)return e.slice(0,t).concat(s).concat(e.slice(t));const n=e.slice(0,e.length);for(;t>n.length;)n.push(void 0);return n.concat(s)},r=function(e,t){const s=/^[a-zA-Z]+/.exec(e);if(s){let n=0;for(let e=0;e<s[0].length;e++)n+=parseInt(s[0].charCodeAt(e)-64)*Math.pow(26,s[0].length-1-e);n--,n<0&&(n=0);let o=parseInt(/[0-9]+$/.exec(e));o>0&&o--,e=1==t?[n,o]:n+"-"+o}return e},l=function(e){return Array.isArray(e)||(e=e.split("-")),(0,n.getColumnName)(parseInt(e[0]))+(parseInt(e[1])+1)}},497:function(e,t,s){s.d(t,{AG:function(){return o},G_:function(){return r},p6:function(){return l},wu:function(){return n}});const n=function(e){const t=this;let s;s=1!=t.options.search&&1!=t.options.filters||!t.results?t.rows:t.results;const n=100;null!=e&&-1!=e||(e=Math.ceil(s.length/n)-1);let o=e*n,r=e*n+n;r>s.length&&(r=s.length),o=r-100,o<0&&(o=0);for(let e=o;e<r;e++)1!=t.options.search&&1!=t.options.filters||!t.results?t.tbody.appendChild(t.rows[e].element):t.tbody.appendChild(t.rows[s[e]].element),t.tbody.children.length>n&&t.tbody.removeChild(t.tbody.firstChild)},o=function(){const e=this;if(e.selectedCell){const t=parseInt(e.tbody.firstChild.getAttribute("data-y"))/100,s=parseInt(e.selectedCell[3]/100),n=parseInt(e.rows.length/100);if(t!=s&&s<=n&&!Array.prototype.indexOf.call(e.tbody.children,e.rows[e.selectedCell[3]].element))return e.loadPage(s),!0}return!1},r=function(){const e=this;let t;t=1!=e.options.search&&1!=e.options.filters||!e.results?e.rows:e.results;let s=0;if(t.length>100){let n=parseInt(e.tbody.firstChild.getAttribute("data-y"));if(1!=e.options.search&&1!=e.options.filters||!e.results||(n=t.indexOf(n)),n>0)for(let o=0;o<30;o++)n-=1,n>-1&&(1!=e.options.search&&1!=e.options.filters||!e.results?e.tbody.insertBefore(e.rows[n].element,e.tbody.firstChild):e.tbody.insertBefore(e.rows[t[n]].element,e.tbody.firstChild),e.tbody.children.length>100&&(e.tbody.removeChild(e.tbody.lastChild),s=1))}return s},l=function(){const e=this;let t;t=1!=e.options.search&&1!=e.options.filters||!e.results?e.rows:e.results;let s=0;if(t.length>100){let n=parseInt(e.tbody.lastChild.getAttribute("data-y"));if(1!=e.options.search&&1!=e.options.filters||!e.results||(n=t.indexOf(n)),n<e.rows.length-1)for(let o=0;o<=30;o++)n<t.length&&(1!=e.options.search&&1!=e.options.filters||!e.results?e.tbody.appendChild(e.rows[n].element):e.tbody.appendChild(e.rows[t[n]].element),e.tbody.children.length>100&&(e.tbody.removeChild(e.tbody.firstChild),s=1)),n+=1}return s}},910:function(e,t,s){s.d(t,{D0:function(){return c},FU:function(){return u},Lt:function(){return a},VP:function(){return h},Zp:function(){return p},fd:function(){return d}});var n=s(689),o=s(530),r=s(911),l=s(805),i=s(657);const a=function(e,t){const s=this,o=[];if(s.options.mergeCells){const r=Object.keys(s.options.mergeCells);for(let l=0;l<r.length;l++){const i=(0,n.vu)(r[l],!0),a=s.options.mergeCells[r[l]][0],c=i[0],d=i[0]+(a>1?a-1:0);null==t?c<=e&&d>=e&&o.push(r[l]):t?c<e&&d>=e&&o.push(r[l]):c<=e&&d>e&&o.push(r[l])}}return o},c=function(e,t){const s=this,o=[];if(s.options.mergeCells){const r=Object.keys(s.options.mergeCells);for(let l=0;l<r.length;l++){const i=(0,n.vu)(r[l],!0),a=s.options.mergeCells[r[l]][1],c=i[1],d=i[1]+(a>1?a-1:0);null==t?c<=e&&d>=e&&o.push(r[l]):t?c<e&&d>=e&&o.push(r[l]):c<=e&&d>e&&o.push(r[l])}}return o},d=function(e){const t=this;let s={};if(e)s=t.options.mergeCells&&t.options.mergeCells[e]?[t.options.mergeCells[e][0],t.options.mergeCells[e][1]]:null;else if(t.options.mergeCells){t.options.mergeCells;const e=Object.keys(t.options.mergeCells);for(let n=0;n<e.length;n++)s[e[n]]=[t.options.mergeCells[e[n]][0],t.options.mergeCells[e[n]][1]]}return s},u=function(e,t,s,a){const c=this;let d=!1;if(e){if("string"!=typeof e)return null}else{if(!c.highlighted.length)return alert(jSuites.translate("No cells selected")),null;{const o=parseInt(c.highlighted[0].getAttribute("data-x")),r=parseInt(c.highlighted[0].getAttribute("data-y")),l=parseInt(c.highlighted[c.highlighted.length-1].getAttribute("data-x")),i=parseInt(c.highlighted[c.highlighted.length-1].getAttribute("data-y"));e=(0,n.t3)([o,r]),t=l-o+1,s=i-r+1}}const u=(0,n.vu)(e,!0);if(c.options.mergeCells&&c.options.mergeCells[e])c.records[u[1]][u[0]].element.getAttribute("data-merged")&&(d="Cell already merged");else if((!t||t<2)&&(!s||s<2))d="Invalid merged properties";else for(let e=u[1];e<u[1]+s;e++)for(let s=u[0];s<u[0]+t;s++)(0,n.t3)([s,e]),c.records[e][s].element.getAttribute("data-merged")&&(d="There is a conflict with another merged cell");if(d)alert(jSuites.translate(d));else{t>1?c.records[u[1]][u[0]].element.setAttribute("colspan",t):t=1,s>1?c.records[u[1]][u[0]].element.setAttribute("rowspan",s):s=1,c.options.mergeCells||(c.options.mergeCells={}),c.options.mergeCells[e]=[t,s,[]],c.records[u[1]][u[0]].element.setAttribute("data-merged","true"),c.records[u[1]][u[0]].element.style.overflow="hidden";const n=[];for(let r=u[1];r<u[1]+s;r++)for(let s=u[0];s<u[0]+t;s++)u[0]==s&&u[1]==r||(n.push(c.options.data[r][s]),o.k9.call(c,s,r,"",!0),c.options.mergeCells[e][2].push(c.records[r][s].element),c.records[r][s].element.style.display="none",c.records[r][s].element=c.records[u[1]][u[0]].element);i.c6.call(c,c.records[u[1]][u[0]].element),a||(r.Dh.call(c,{action:"setMerge",column:e,colspan:t,rowspan:s,data:n}),l.A.call(c,"onmerge",c,{[e]:[t,s]}))}},p=function(e,t,s){const r=this;if(r.options.mergeCells&&r.options.mergeCells[e]){const l=(0,n.vu)(e,!0);r.records[l[1]][l[0]].element.removeAttribute("colspan"),r.records[l[1]][l[0]].element.removeAttribute("rowspan"),r.records[l[1]][l[0]].element.removeAttribute("data-merged");const a=r.options.mergeCells[e];let c,d,u=0;for(c=0;c<a[1];c++)for(d=0;d<a[0];d++)(c>0||d>0)&&(r.records[l[1]+c][l[0]+d].element=a[2][u],r.records[l[1]+c][l[0]+d].element.style.display="",t&&t[u]&&o.k9.call(r,l[0]+d,l[1]+c,t[u]),u++);i.c6.call(r,r.records[l[1]][l[0]].element,r.records[l[1]+c-1][l[0]+d-1].element),s||delete r.options.mergeCells[e]}},h=function(e){const t=this;if(t.options.mergeCells){t.options.mergeCells;const s=Object.keys(t.options.mergeCells);for(let n=0;n<s.length;n++)p.call(t,s[n],null,e)}}},654:function(e,t,s){s.d(t,{IQ:function(){return o},hs:function(){return r},iZ:function(){return l}});var n=s(805);const o=function(e,t){const s=this;return e?t?s.options.meta&&s.options.meta[e]&&s.options.meta[e][t]?s.options.meta[e][t]:null:s.options.meta&&s.options.meta[e]?s.options.meta[e]:null:s.options.meta},r=function(e){const t=this;if(t.options.meta){const s={},n=Object.keys(t.options.meta);for(let o=0;o<n.length;o++)e[n[o]]?s[e[n[o]]]=t.options.meta[n[o]]:s[n[o]]=t.options.meta[n[o]];t.options.meta=s}},l=function(e,t,s){const o=this;if(o.options.meta||(o.options.meta={}),t&&s)o.options.meta[e]||(o.options.meta[e]={}),o.options.meta[e][t]=s,n.A.call(o,"onchangemeta",o,{[e]:{[t]:s}});else{const t=Object.keys(e);for(let s=0;s<t.length;s++){o.options.meta[t[s]]||(o.options.meta[t[s]]={});const n=Object.keys(e[t[s]]);for(let r=0;r<n.length;r++)o.options.meta[t[s]][n[r]]=e[t[s]][n[r]]}n.A.call(o,"onchangemeta",o,e)}}},94:function(e,t,s){s.d(t,{My:function(){return d},Th:function(){return a},iY:function(){return c}});var n=s(911),o=s(805),r=s(530),l=s(497),i=s(829);const a=function(e,t){const s=this;for(let e=0;e<s.headers.length;e++)s.headers[e].classList.remove("arrow-up"),s.headers[e].classList.remove("arrow-down");t?s.headers[e].classList.add("arrow-up"):s.headers[e].classList.add("arrow-down")},c=function(e){const t=this;let s=[];for(let n=0;n<e.length;n++)s[n]=t.options.data[e[n]];t.options.data=s,s=[];for(let n=0;n<e.length;n++){s[n]=t.records[e[n]];for(let e=0;e<s[n].length;e++)s[n][e].y=n}t.records=s,s=[];for(let n=0;n<e.length;n++)s[n]=t.rows[e[n]],s[n].y=n;if(t.rows=s,r.o8.call(t),t.results&&t.results.length)t.searchInput.value?t.search(t.searchInput.value):i.F8.call(t);else if(t.results=null,t.pageNumber=0,t.options.pagination>0)t.page(0);else if(1==t.options.lazyLoading)l.wu.call(t,0);else for(let e=0;e<t.rows.length;e++)t.tbody.appendChild(t.rows[e].element)},d=function(e,t){const s=this;if(e>=0){if(s.options.mergeCells&&Object.keys(s.options.mergeCells).length>0){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;s.destroyMerge()}t=null==t?s.headers[e].classList.contains("arrow-down")?1:0:t?1:0;let r=[];if(s.options.columns&&s.options.columns[e]&&("number"==s.options.columns[e].type||"numeric"==s.options.columns[e].type||"percentage"==s.options.columns[e].type||"autonumber"==s.options.columns[e].type||"color"==s.options.columns[e].type))for(let t=0;t<s.options.data.length;t++)r[t]=[t,Number(s.options.data[t][e])];else if(s.options.columns&&s.options.columns[e]&&("calendar"==s.options.columns[e].type||"checkbox"==s.options.columns[e].type||"radio"==s.options.columns[e].type))for(let t=0;t<s.options.data.length;t++)r[t]=[t,s.options.data[t][e]];else for(let t=0;t<s.options.data.length;t++)r[t]=[t,s.records[t][e].element.textContent.toLowerCase()];"function"!=typeof s.parent.config.sorting&&(s.parent.config.sorting=function(e){return function(t,s){const n=t[1],o=s[1];return e?""===n&&""!==o?1:""!==n&&""===o||n>o?-1:n<o?1:0:""===n&&""!==o?1:""!==n&&""===o?-1:n>o?1:n<o?-1:0}}),r=r.sort(s.parent.config.sorting(t));const l=[];for(let e=0;e<r.length;e++)l[e]=r[e][0];return n.Dh.call(s,{action:"orderBy",rows:l,column:e,order:t}),a.call(s,e,t),c.call(s,l),o.A.call(s,"onsort",s,e,t,l.map((e=>e))),!0}}},167:function(e,t,s){s.d(t,{$f:function(){return a},IV:function(){return l},MY:function(){return i},ho:function(){return r}});var n=s(805),o=s(657);const r=function(e){const t=this;return 1!=t.options.search&&1!=t.options.filters||!t.results||(e=t.results.indexOf(e)),Math.ceil((parseInt(e)+1)/parseInt(t.options.pagination))-1},l=function(){const e=this;if(e.pagination.children[0].innerHTML="",e.pagination.children[1].innerHTML="",e.options.pagination){let t;if(t=1!=e.options.search&&1!=e.options.filters||!e.results?e.rows.length:e.results.length,t){const s=Math.ceil(t/e.options.pagination);let n,o;if(e.pageNumber<6?(n=1,o=s<10?s:10):s-e.pageNumber<5?(n=s-9,o=s,n<1&&(n=1)):(n=e.pageNumber-4,o=e.pageNumber+5),n>1){const t=document.createElement("div");t.className="jss_page",t.innerHTML="<",t.title=1,e.pagination.children[1].appendChild(t)}for(let t=n;t<=o;t++){const s=document.createElement("div");s.className="jss_page",s.innerHTML=t,e.pagination.children[1].appendChild(s),e.pageNumber==t-1&&s.classList.add("jss_page_selected")}if(o<s){const t=document.createElement("div");t.className="jss_page",t.innerHTML=">",t.title=s,e.pagination.children[1].appendChild(t)}const r=function(e){const t=Array.prototype.slice.call(arguments,1);return e.replace(/{(\d+)}/g,(function(e,s){return void 0!==t[s]?t[s]:e}))};e.pagination.children[0].innerHTML=r(jSuites.translate("Showing page {0} of {1} entries"),e.pageNumber+1,s)}else e.pagination.children[0].innerHTML=jSuites.translate("No records found")}},i=function(e){const t=this,s=t.pageNumber;let r;r=1!=t.options.search&&1!=t.options.filters||!t.results?t.rows:t.results;const i=parseInt(t.options.pagination);null!=e&&-1!=e||(e=Math.ceil(r.length/i)-1),t.pageNumber=e;let a=e*i,c=e*i+i;for(c>r.length&&(c=r.length),a<0&&(a=0);t.tbody.firstChild;)t.tbody.removeChild(t.tbody.firstChild);for(let e=a;e<c;e++)1!=t.options.search&&1!=t.options.filters||!t.results?t.tbody.appendChild(t.rows[e].element):t.tbody.appendChild(t.rows[r[e]].element);t.options.pagination>0&&l.call(t),o.Aq.call(t),n.A.call(t,"onchangepage",t,e,s,t.options.pagination)},a=function(){const e=this;let t;return t=1!=e.options.search&&1!=e.options.filters||!e.results?e.rows.length:e.results.length,Math.ceil(t/e.options.pagination)}},657:function(e,t,s){s.d(t,{AH:function(){return m},Aq:function(){return d},G9:function(){return g},Jg:function(){return f},Lo:function(){return v},R5:function(){return _},Ub:function(){return B},at:function(){return w},c6:function(){return p},eO:function(){return x},ef:function(){return A},gE:function(){return u},gG:function(){return y},kA:function(){return h},kF:function(){return C},kV:function(){return k},sp:function(){return E},tW:function(){return j}});var n=s(805),o=s(296),r=s(978),l=s(911),i=s(530),a=s(689),c=s(392);const d=function(){const e=this;if(e.highlighted&&e.highlighted.length){const t=e.highlighted[e.highlighted.length-1].element,s=t.getAttribute("data-x"),n=e.content.getBoundingClientRect(),r=n.left,l=n.top,i=t.getBoundingClientRect(),a=i.left,c=i.top,d=i.width,u=i.height,p=a-r+e.content.scrollLeft+d-4,h=c-l+e.content.scrollTop+u-4;if(e.corner.style.top=h+"px",e.corner.style.left=p+"px",e.options.freezeColumns){const t=o.w.call(e);s>e.options.freezeColumns-1&&a-r+d<t?e.corner.style.display="none":0!=e.options.selectionCopy&&(e.corner.style.display="")}else 0!=e.options.selectionCopy&&(e.corner.style.display="")}else e.corner.style.top="-2000px",e.corner.style.left="-2000px";(0,c.nK)(e)},u=function(e){const t=this;let s;if(t.highlighted&&t.highlighted.length){s=1;for(let e=0;e<t.highlighted.length;e++){t.highlighted[e].element.classList.remove("highlight"),t.highlighted[e].element.classList.remove("highlight-left"),t.highlighted[e].element.classList.remove("highlight-right"),t.highlighted[e].element.classList.remove("highlight-top"),t.highlighted[e].element.classList.remove("highlight-bottom"),t.highlighted[e].element.classList.remove("highlight-selected");const s=parseInt(t.highlighted[e].element.getAttribute("data-x")),n=parseInt(t.highlighted[e].element.getAttribute("data-y"));let o,r;if(t.highlighted[e].element.getAttribute("data-merged")){const l=parseInt(t.highlighted[e].element.getAttribute("colspan")),i=parseInt(t.highlighted[e].element.getAttribute("rowspan"));o=l>0?s+(l-1):s,r=i>0?n+(i-1):n}else o=s,r=n;for(let e=s;e<=o;e++)t.headers[e]&&t.headers[e].classList.remove("selected");for(let e=n;e<=r;e++)t.rows[e]&&t.rows[e].element.classList.remove("selected")}}else s=0;return t.highlighted=[],t.selectedCell=null,t.corner.style.top="-2000px",t.corner.style.left="-2000px",1==e&&1==s&&n.A.call(t,"onblur",t),s},p=function(e,t,s){const n=e.getAttribute("data-x"),o=e.getAttribute("data-y");let r,l;t?(r=t.getAttribute("data-x"),l=t.getAttribute("data-y")):(r=n,l=o),m.call(this,n,o,r,l,s)},h=function(){const e=document.querySelectorAll(".jss_worksheet .copying");for(let t=0;t<e.length;t++)e[t].classList.remove("copying"),e[t].classList.remove("copying-left"),e[t].classList.remove("copying-right"),e[t].classList.remove("copying-top"),e[t].classList.remove("copying-bottom")},m=function(e,t,s,o,r){const l=this;if(null==t){if(t=0,o=l.rows.length-1,null==e)return}else null==e&&(e=0,s=l.options.data[0].length-1);null==s&&(s=e),null==o&&(o=t),e>=l.headers.length&&(e=l.headers.length-1),t>=l.rows.length&&(t=l.rows.length-1),s>=l.headers.length&&(s=l.headers.length-1),o>=l.rows.length&&(o=l.rows.length-1);let i,a,c,u,p=null,m=null,f=null,g=null;parseInt(e)<parseInt(s)?(i=parseInt(e),a=parseInt(s)):(i=parseInt(s),a=parseInt(e)),parseInt(t)<parseInt(o)?(c=parseInt(t),u=parseInt(o)):(c=parseInt(o),u=parseInt(t));for(let e=i;e<=a;e++)for(let t=c;t<=u;t++)if(l.records[t][e]&&l.records[t][e].element.getAttribute("data-merged")){const s=parseInt(l.records[t][e].element.getAttribute("data-x")),n=parseInt(l.records[t][e].element.getAttribute("data-y")),o=parseInt(l.records[t][e].element.getAttribute("colspan")),r=parseInt(l.records[t][e].element.getAttribute("rowspan"));o>1&&(s<i&&(i=s),s+o>a&&(a=s+o-1)),r&&(n<c&&(c=n),n+r>u&&(u=n+r-1))}for(let e=c;e<=u;e++)"none"!=l.rows[e].element.style.display&&(null==f&&(f=e),g=e);for(let e=i;e<=a;e++)for(let t=c;t<=u;t++)l.options.columns&&l.options.columns[e]&&"hidden"==l.options.columns[e].type||(null==p&&(p=e),m=e);if(p||(p=0),m||(m=0),!1===n.A.call(l,"onbeforeselection",l,p,f,m,g,r))return!1;const y=l.resetSelection();l.selectedCell=[e,t,s,o],l.records[t][e]&&l.records[t][e].element.classList.add("highlight-selected");for(let e=i;e<=a;e++)for(let t=c;t<=u;t++)"none"!=l.rows[t].element.style.display&&"none"!=l.records[t][e].element.style.display&&(l.records[t][e].element.classList.add("highlight"),l.highlighted.push(l.records[t][e]));for(let e=p;e<=m;e++)l.options.columns&&l.options.columns[e]&&"hidden"==l.options.columns[e].type||!l.cols[e].colElement.style||"none"==l.cols[e].colElement.style.display||(l.records[f]&&l.records[f][e]&&l.records[f][e].element.classList.add("highlight-top"),l.records[g]&&l.records[g][e]&&l.records[g][e].element.classList.add("highlight-bottom"),l.headers[e].classList.add("selected"));for(let e=f;e<=g;e++)l.rows[e]&&"none"!=l.rows[e].element.style.display&&(l.records[e][p].element.classList.add("highlight-left"),l.records[e][m].element.classList.add("highlight-right"),l.rows[e].element.classList.add("selected"));l.selectedContainer=[p,f,m,g],0==y&&(n.A.call(l,"onfocus",l),h()),n.A.call(l,"onselection",l,p,f,m,g,r),d.call(l)},f=function(e){const t=this;if(!t.selectedCell)return[];const s=[];for(let n=Math.min(t.selectedCell[0],t.selectedCell[2]);n<=Math.max(t.selectedCell[0],t.selectedCell[2]);n++)e&&"none"==t.headers[n].style.display||s.push(n);return s},g=function(){const e=this;e.selectedCell&&e.updateSelectionFromCoords(e.selectedCell[0],e.selectedCell[1],e.selectedCell[2],e.selectedCell[3])},y=function(){const e=this;for(let t=0;t<e.selection.length;t++)e.selection[t].classList.remove("selection"),e.selection[t].classList.remove("selection-left"),e.selection[t].classList.remove("selection-right"),e.selection[t].classList.remove("selection-top"),e.selection[t].classList.remove("selection-bottom");e.selection=[]},b=function(e){return 1==(e=""+e).length&&(e="0"+e),e},C=function(e,t){const s=this,o=s.getData(!0,!1),r=s.selectedContainer,c=parseInt(e.getAttribute("data-x")),d=parseInt(e.getAttribute("data-y")),u=parseInt(t.getAttribute("data-x")),p=parseInt(t.getAttribute("data-y")),h=[];let m,f,g=!1;r[0]==c?(m=d<r[1]?d-r[1]:1,f=0):(f=c<r[0]?c-r[0]:1,m=0);let y=0,C=0;for(let e=d;e<=p;e++)if(!s.rows[e]||"none"!=s.rows[e].element.style.display){null==o[C]&&(C=0),y=0,r[0]!=c&&(f=c<r[0]?c-r[0]:1);for(let t=c;t<=u;t++){if(s.records[e][t]&&!s.records[e][t].element.classList.contains("readonly")&&"none"!=s.records[e][t].element.style.display&&0==g){if(!s.selection.length&&""!=s.options.data[e][t]){g=!0;continue}(null==o[C]||null==o[C][y])&&(y=0);let n=o[C][y];if(n&&!o[1]&&0!=s.parent.config.autoIncrement)if(!s.options.columns||!s.options.columns[t]||s.options.columns[t].type&&"text"!=s.options.columns[t].type&&"number"!=s.options.columns[t].type){if(s.options.columns&&s.options.columns[t]&&"calendar"==s.options.columns[t].type){const e=new Date(n);e.setDate(e.getDate()+m),n=e.getFullYear()+"-"+b(parseInt(e.getMonth()+1))+"-"+b(e.getDate())+" 00:00:00"}}else if("="==(""+n).substr(0,1)){const e=n.match(/([A-Z]+[0-9]+)/g);if(e){const t=[];for(let s=0;s<e.length;s++){const n=(0,a.vu)(e[s],1);n[0]+=f,n[1]+=m,n[1]<0&&(n[1]=0);const o=(0,a.t3)([n[0],n[1]]);o!=e[s]&&(t[e[s]]=o)}t&&(n=(0,i.yB)(n,t))}}else n==Number(n)&&(n=Number(n)+m);h.push(i.k9.call(s,t,e,n)),i.xF.call(s,t,e,h)}y++,r[0]!=c&&f++}C++,m++}l.Dh.call(s,{action:"setValue",records:h,selection:s.selectedCell}),i.am.call(s);const j=h.map((function(e){return{x:e.x,y:e.y,value:e.value,oldValue:e.oldValue}}));n.A.call(s,"onafterchanges",s,j)},j=function(e){let t,s,n=0;if(!e||0===e.length)return n;for(t=0;t<e.length;t++)s=e.charCodeAt(t),n=(n<<5)-n+s,n|=0;return n},w=function(e,t,s){const n=this;if(1==e){if(n.selectedCell&&(t>=n.selectedCell[1]&&t<=n.selectedCell[3]||s>=n.selectedCell[1]&&s<=n.selectedCell[3]))return void n.resetSelection()}else if(n.selectedCell&&(t>=n.selectedCell[0]&&t<=n.selectedCell[2]||s>=n.selectedCell[0]&&s<=n.selectedCell[2]))return void n.resetSelection()},_=function(e){const t=this;if(!t.selectedCell)return[];const s=[];for(let n=Math.min(t.selectedCell[1],t.selectedCell[3]);n<=Math.max(t.selectedCell[1],t.selectedCell[3]);n++)e&&"none"==t.rows[n].element.style.display||s.push(n);return s},B=function(){const e=this;e.selectedCell||(e.selectedCell=[]),e.selectedCell[0]=0,e.selectedCell[1]=0,e.selectedCell[2]=e.headers.length-1,e.selectedCell[3]=e.records.length-1,e.updateSelectionFromCoords(e.selectedCell[0],e.selectedCell[1],e.selectedCell[2],e.selectedCell[3])},v=function(){const e=this;return e.selectedCell?[Math.min(e.selectedCell[0],e.selectedCell[2]),Math.min(e.selectedCell[1],e.selectedCell[3]),Math.max(e.selectedCell[0],e.selectedCell[2]),Math.max(e.selectedCell[1],e.selectedCell[3])]:null},A=function(e){const t=this,s=v.call(t);if(!s)return[];const n=[];for(let o=s[1];o<=s[3];o++)for(let l=s[0];l<=s[2];l++)e?n.push((0,r.getCellNameFromCoords)(l,o)):n.push(t.records[o][l]);return n},x=function(){const e=this,t=v.call(e);if(!t)return"";const s=(0,r.getCellNameFromCoords)(t[0],t[1]),n=(0,r.getCellNameFromCoords)(t[2],t[3]);return s===n?e.options.worksheetName+"!"+s:e.options.worksheetName+"!"+s+":"+n},E=function(e,t){const s=v.call(this);return e>=s[0]&&e<=s[2]&&t>=s[1]&&t<=s[3]},k=function(){const e=v.call(this);return e?[e]:[]}},392:function(e,t,s){s.d(t,{Ar:function(){return u},ll:function(){return d},nK:function(){return c}});var n=s(978),o=s(530);const r=function(e,t){0!=t.options.editable?e.classList.remove("jtoolbar-disabled"):e.classList.add("jtoolbar-disabled")},l=function(){const e=[],t=this,s=function(){return o.eN.call(t)};e.push({content:"undo",onclick:function(){s().undo()}}),e.push({content:"redo",onclick:function(){s().redo()}}),e.push({content:"save",onclick:function(){const e=s();e&&e.download()}}),e.push({type:"divisor"}),e.push({type:"select",width:"120px",options:["Default","Verdana","Arial","Courier New"],render:function(e){return'<span style="font-family:'+e+'">'+e+"</span>"},onchange:function(e,t,n,o,r){const l=s();let i=l.getSelected(!0);if(i){let e=r?o:"";l.setStyle(Object.fromEntries(i.map((function(t){return[t,"font-family: "+e]}))))}},updateState:function(e,t,n){r(n,s())}}),e.push({type:"select",width:"48px",content:"format_size",options:["x-small","small","medium","large","x-large"],render:function(e){return'<span style="font-size:'+e+'">'+e+"</span>"},onchange:function(e,t,n,o){const r=s();let l=r.getSelected(!0);l&&r.setStyle(Object.fromEntries(l.map((function(e){return[e,"font-size: "+o]}))))},updateState:function(e,t,n){r(n,s())}}),e.push({type:"select",options:["left","center","right","justify"],render:function(e){return'<i class="material-icons">format_align_'+e+"</i>"},onchange:function(e,t,n,o){const r=s();let l=r.getSelected(!0);l&&r.setStyle(Object.fromEntries(l.map((function(e){return[e,"text-align: "+o]}))))},updateState:function(e,t,n){r(n,s())}}),e.push({content:"format_bold",onclick:function(e,t,n){const o=s();let r=o.getSelected(!0);r&&o.setStyle(Object.fromEntries(r.map((function(e){return[e,"font-weight:bold"]}))))},updateState:function(e,t,n){r(n,s())}}),e.push({type:"color",content:"format_color_text",k:"color",updateState:function(e,t,n){r(n,s())}}),e.push({type:"color",content:"format_color_fill",k:"background-color",updateState:function(e,t,n,o){r(n,s())}});let l=["top","middle","bottom"];return e.push({type:"select",options:["vertical_align_top","vertical_align_center","vertical_align_bottom"],render:function(e){return'<i class="material-icons">'+e+"</i>"},value:1,onchange:function(e,t,n,o,r){const i=s();let a=i.getSelected(!0);a&&i.setStyle(Object.fromEntries(a.map((function(e){return[e,"vertical-align: "+l[r]]}))))},updateState:function(e,t,n){r(n,s())}}),e.push({content:"web",tooltip:jSuites.translate("Merge the selected cells"),onclick:function(){const e=s();if(e.selectedCell&&confirm(jSuites.translate("The merged cells will retain the value of the top-left cell only. Are you sure?"))){const t=[Math.min(e.selectedCell[0],e.selectedCell[2]),Math.min(e.selectedCell[1],e.selectedCell[3]),Math.max(e.selectedCell[0],e.selectedCell[2]),Math.max(e.selectedCell[1],e.selectedCell[3])];let s=(0,n.getCellNameFromCoords)(t[0],t[1]);if(e.records[t[1]][t[0]].element.getAttribute("data-merged"))e.removeMerge(s);else{let n=t[2]-t[0]+1,o=t[3]-t[1]+1;1===n&&1===o||e.setMerge(s,n,o)}}},updateState:function(e,t,n){r(n,s())}}),e.push({type:"select",options:["border_all","border_outer","border_inner","border_horizontal","border_vertical","border_left","border_top","border_right","border_bottom","border_clear"],columns:5,render:function(e){return'<i class="material-icons">'+e+"</i>"},right:!0,onchange:function(e,t,o,r){const l=s();if(l.selectedCell){const e=[Math.min(l.selectedCell[0],l.selectedCell[2]),Math.min(l.selectedCell[1],l.selectedCell[3]),Math.max(l.selectedCell[0],l.selectedCell[2]),Math.max(l.selectedCell[1],l.selectedCell[3])];let s=r;if(e){let o=t.thickness||1,r=t.color||"black";const i=t.style||"solid";"double"===i&&(o+=2);let a={},c=e[0],d=e[1],u=e[2],p=e[3];const h=function(e,t,n){let l=["","","",""];l[0]=("border_top"===s||"border_outer"===s)&&n===d||("border_inner"===s||"border_horizontal"===s)&&n>d||"border_all"===s?"border-top: "+o+"px "+i+" "+r:"border-top: ",l[1]="border_all"!==s&&"border_right"!==s&&"border_outer"!==s||t!==u?"border-right: ":"border-right: "+o+"px "+i+" "+r,l[2]="border_all"!==s&&"border_bottom"!==s&&"border_outer"!==s||n!==p?"border-bottom: ":"border-bottom: "+o+"px "+i+" "+r,l[3]=("border_left"===s||"border_outer"===s)&&t===c||("border_inner"===s||"border_vertical"===s)&&t>c||"border_all"===s?"border-left: "+o+"px "+i+" "+r:"border-left: ",a[e]=l.join(";")};for(let t=e[1];t<=e[3];t++)for(let s=e[0];s<=e[2];s++)h((0,n.getCellNameFromCoords)(s,t),s,t),l.records[t][s].element.getAttribute("data-merged")&&h((0,n.getCellNameFromCoords)(e[0],e[1]),s,t);Object.keys(a)&&l.setStyle(a)}}},onload:function(e,t){let s=document.createElement("div"),n=document.createElement("div");s.appendChild(n);let o=jSuites.color(n,{closeOnChange:!1,onchange:function(e,s){e.parentNode.children[1].style.color=s,t.color=s}}),r=document.createElement("i");r.classList.add("material-icons"),r.innerHTML="color_lens",r.onclick=function(){o.open()},s.appendChild(r),e.children[1].appendChild(s),n=document.createElement("div"),jSuites.picker(n,{type:"select",data:[1,2,3,4,5],render:function(e){return'<div style="height: '+e+'px; width: 30px; background-color: black;"></div>'},onchange:function(e,s,n,o){t.thickness=o},width:"50px"}),e.children[1].appendChild(n);const l=document.createElement("div");jSuites.picker(l,{type:"select",data:["solid","dotted","dashed","double"],render:function(e){return"double"===e?'<div style="width: 30px; border-top: 3px '+e+' black;"></div>':'<div style="width: 30px; border-top: 2px '+e+' black;"></div>'},onchange:function(e,s,n,o){t.style=o},width:"50px"}),e.children[1].appendChild(l),n=document.createElement("div"),n.style.flex="1",e.children[1].appendChild(n)},updateState:function(e,t,n){r(n,s())}}),e.push({type:"divisor"}),e.push({content:"fullscreen",tooltip:"Toggle Fullscreen",onclick:function(e,s,n){"fullscreen"===n.children[0].textContent?(t.fullscreen(!0),n.children[0].textContent="fullscreen_exit"):(t.fullscreen(!1),n.children[0].textContent="fullscreen")},updateState:function(e,t,s,n){!0===n.parent.config.fullscreen?s.children[0].textContent="fullscreen_exit":s.children[0].textContent="fullscreen"}}),e},i=function(e){const t=this,s=e.items;for(let e=0;e<s.length;e++)s[e].tooltip&&(s[e].title=s[e].tooltip,delete s[e].tooltip),"select"==s[e].type?s[e].options?(s[e].data=s[e].options,delete s[e].options):(s[e].data=s[e].v,delete s[e].v,s[e].k&&!s[e].onchange&&(s[e].onchange=function(n,r,l){const i=o.eN.call(t),a=i.getSelected(!0);i.setStyle(Object.fromEntries(a.map((function(t){return[t,s[e].k+": "+l]}))))})):"color"==s[e].type&&(s[e].type="i",s[e].onclick=function(n,r,l){l.color||(jSuites.color(l,{onchange:function(n,r){const l=o.eN.call(t),i=l.getSelected(!0);l.setStyle(Object.fromEntries(i.map((function(t){return[t,s[e].k+": "+r]}))))},onopen:function(e){e.color.select("")}}),l.color.open())})},a=function(e){const t=this,s=document.createElement("div");return s.classList.add("jss_toolbar"),i.call(t,e),"object"==typeof t.plugins&&Object.entries(t.plugins).forEach((function([,t]){if("function"==typeof t.toolbar){const s=t.toolbar(e);s&&(e=s)}})),jSuites.toolbar(s,e),s},c=function(e){e.parent.toolbar&&e.parent.toolbar.toolbar.update(e)},d=function(){const e=this;if(e.config.toolbar&&!e.toolbar){let t;Array.isArray(e.config.toolbar)?t={items:e.config.toolbar}:"object"==typeof e.config.toolbar?t=e.config.toolbar:(t={items:l.call(e)},"function"==typeof e.config.toolbar&&(t=e.config.toolbar(t))),e.toolbar=e.element.insertBefore(a.call(e,t),e.element.children[1])}},u=function(){const e=this;e.toolbar&&(e.toolbar.parentNode.removeChild(e.toolbar),delete e.toolbar)}}},__webpack_module_cache__={};function __webpack_require__(e){var t=__webpack_module_cache__[e];if(void 0!==t)return t.exports;var s=__webpack_module_cache__[e]={exports:{}};return __webpack_modules__[e](s,s.exports,__webpack_require__),s.exports}__webpack_require__.d=function(e,t){for(var s in t)__webpack_require__.o(t,s)&&!__webpack_require__.o(e,s)&&Object.defineProperty(e,s,{enumerable:!0,get:t[s]})},__webpack_require__.o=function(e,t){return Object.prototype.hasOwnProperty.call(e,t)},__webpack_require__.r=function(e){"undefined"!=typeof Symbol&&Symbol.toStringTag&&Object.defineProperty(e,Symbol.toStringTag,{value:"Module"}),Object.defineProperty(e,"__esModule",{value:!0})};var __webpack_exports__={};__webpack_require__.d(__webpack_exports__,{default:function(){return src}});const lib={jspreadsheet:{}};var libraryBase=lib,dispatch=__webpack_require__(805),internal=__webpack_require__(530),utils_history=__webpack_require__(911);const openEditor=function(e,t,s){const n=this,o=e.getAttribute("data-y"),r=e.getAttribute("data-x");dispatch.A.call(n,"oneditionstart",n,e,parseInt(r),parseInt(o)),r>0&&(n.records[o][r-1].element.style.overflow="hidden");const l=function(t){const s=e.getBoundingClientRect(),n=document.createElement(t);return n.style.width=s.width+"px",n.style.height=s.height-2+"px",n.style.minHeight=s.height-2+"px",e.classList.add("editor"),e.innerHTML="",e.appendChild(n),n};if(1==e.classList.contains("readonly"));else if(n.edition=[n.records[o][r].element,n.records[o][r].element.innerHTML,r,o],n.options.columns&&n.options.columns[r]&&"object"==typeof n.options.columns[r].type)n.options.columns[r].type.openEditor(e,n.options.data[o][r],parseInt(r),parseInt(o),n,n.options.columns[r],s),dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]);else if(n.options.columns&&n.options.columns[r]&&"hidden"==n.options.columns[r].type);else if(n.options.columns&&n.options.columns[r]&&("checkbox"==n.options.columns[r].type||"radio"==n.options.columns[r].type)){const t=!e.children[0].checked;n.setValue(e,t),n.edition=null}else if(n.options.columns&&n.options.columns[r]&&"dropdown"==n.options.columns[r].type){let t,s=n.options.data[o][r];n.options.columns[r].multiple&&!Array.isArray(s)&&(s=s.split(";")),t="function"==typeof n.options.columns[r].filter?n.options.columns[r].filter(n.element,e,r,o,n.options.columns[r].source):n.options.columns[r].source;const i=[];if(t)for(let e=0;e<t.length;e++)i.push(t[e]);const a=l("div");dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]);const c={data:i,multiple:!!n.options.columns[r].multiple,autocomplete:!!n.options.columns[r].autocomplete,opened:!0,value:s,width:"100%",height:a.style.minHeight,position:1==n.options.tableOverflow||1==n.parent.config.fullscreen,onclose:function(){closeEditor.call(n,e,!0)}};n.options.columns[r].options&&n.options.columns[r].options.type&&(c.type=n.options.columns[r].options.type),jSuites.dropdown(a,c)}else if(n.options.columns&&n.options.columns[r]&&("calendar"==n.options.columns[r].type||"color"==n.options.columns[r].type)){const t=n.options.data[o][r],s=l("input");dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]),s.value=t;const i=n.options.columns[r].options?{...n.options.columns[r].options}:{};if(1!=n.options.tableOverflow&&1!=n.parent.config.fullscreen||(i.position=!0),i.value=n.options.data[o][r],i.opened=!0,i.onclose=function(t,s){closeEditor.call(n,e,!0)},"color"==n.options.columns[r].type){jSuites.color(s,i);const t=e.getBoundingClientRect();i.position&&(s.nextSibling.children[1].style.top=t.top+t.height+"px",s.nextSibling.children[1].style.left=t.left+"px")}else i.format||(i.format="YYYY-MM-DD"),jSuites.calendar(s,i);s.focus()}else if(n.options.columns&&n.options.columns[r]&&"html"==n.options.columns[r].type){const t=n.options.data[o][r],s=l("div");dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]),s.style.position="relative";const i=document.createElement("div");i.classList.add("jss_richtext"),s.appendChild(i),jSuites.editor(i,{focus:!0,value:t});const a=e.getBoundingClientRect(),c=i.getBoundingClientRect();window.innerHeight<a.bottom+c.height?i.style.top=a.bottom-(c.height+2)+"px":i.style.top=a.top+"px",window.innerWidth<a.left+c.width?i.style.left=a.right-(c.width+2)+"px":i.style.left=a.left+"px"}else if(n.options.columns&&n.options.columns[r]&&"image"==n.options.columns[r].type){const t=e.children[0],s=l("div");dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]),s.style.position="relative";const i=document.createElement("div");i.classList.add("jclose"),t&&t.src&&i.appendChild(t),s.appendChild(i),jSuites.image(i,n.options.columns[r]);const a=e.getBoundingClientRect(),c=i.getBoundingClientRect();window.innerHeight<a.bottom+c.height?i.style.top=a.top-(c.height+2)+"px":i.style.top=a.top+"px",i.style.left=a.left+"px"}else{const s=1==t?"":n.options.data[o][r];let i;i=n.options.columns&&n.options.columns[r]&&0==n.options.columns[r].wordWrap||!(1==n.options.wordWrap||n.options.columns&&n.options.columns[r]&&1==n.options.columns[r].wordWrap)?l("input"):l("textarea"),dispatch.A.call(n,"oncreateeditor",n,e,parseInt(r),parseInt(o),null,n.options.columns[r]),i.focus(),i.value=s;const a=n.options.columns&&n.options.columns[r];if(!(0,internal.dw)(s)&&a){const e=(0,internal.rS)(a);if(e){if(!a.disabledMaskOnEdition)if(a.mask){const e=a.mask.split(";");i.setAttribute("data-mask",e[0])}else a.locale&&i.setAttribute("data-locale",a.locale);e.input=i,i.mask=e,jSuites.mask.render(s,e,!1)}}i.onblur=function(){closeEditor.call(n,e,!0)},i.scrollLeft=i.scrollWidth}},closeEditor=function(e,t){const s=this,n=parseInt(e.getAttribute("data-x")),o=parseInt(e.getAttribute("data-y"));let r;if(1==t){if(s.options.columns&&s.options.columns[n]&&"object"==typeof s.options.columns[n].type)r=s.options.columns[n].type.closeEditor(e,t,parseInt(n),parseInt(o),s,s.options.columns[n]);else if(s.options.columns&&s.options.columns[n]&&("checkbox"==s.options.columns[n].type||"radio"==s.options.columns[n].type||"hidden"==s.options.columns[n].type));else if(s.options.columns&&s.options.columns[n]&&"dropdown"==s.options.columns[n].type)r=e.children[0].dropdown.close(!0);else if(s.options.columns&&s.options.columns[n]&&"calendar"==s.options.columns[n].type)r=e.children[0].calendar.close(!0);else if(s.options.columns&&s.options.columns[n]&&"color"==s.options.columns[n].type)r=e.children[0].color.close(!0);else if(s.options.columns&&s.options.columns[n]&&"html"==s.options.columns[n].type)r=e.children[0].children[0].editor.getData();else if(s.options.columns&&s.options.columns[n]&&"image"==s.options.columns[n].type){const t=e.children[0].children[0].children[0];r=t&&"IMG"==t.tagName?t.src:""}else if(s.options.columns&&s.options.columns[n]&&"numeric"==s.options.columns[n].type)r=e.children[0].value,"="!=(""+r).substr(0,1)&&""==r&&(r=s.options.columns[n].allowEmpty?"":0),e.children[0].onblur=null;else{r=e.children[0].value,e.children[0].onblur=null;const t=s.options.columns&&s.options.columns[n];if(t){const e=(0,internal.rS)(t);if(e&&""!==r&&!(0,internal.dw)(r)&&"number"!=typeof r){const t=jSuites.mask.extract(r,e,!0);t&&""!==t.value&&(r=t.value)}}}s.options.data[o][n]==r?e.innerHTML=s.edition[1]:s.setValue(e,r)}else s.options.columns&&s.options.columns[n]&&"object"==typeof s.options.columns[n].type?s.options.columns[n].type.closeEditor(e,t,parseInt(n),parseInt(o),s,s.options.columns[n]):s.options.columns&&s.options.columns[n]&&"dropdown"==s.options.columns[n].type?e.children[0].dropdown.close(!0):s.options.columns&&s.options.columns[n]&&"calendar"==s.options.columns[n].type?e.children[0].calendar.close(!0):s.options.columns&&s.options.columns[n]&&"color"==s.options.columns[n].type?e.children[0].color.close(!0):e.children[0].onblur=null,e.innerHTML=s.edition&&s.edition[1]?s.edition[1]:"";dispatch.A.call(s,"oneditionend",s,e,n,o,r,t),e.classList.remove("editor"),s.edition=null},setCheckRadioValue=function(){const e=this,t=[],s=Object.keys(e.highlighted);for(let n=0;n<s.length;n++){const s=e.highlighted[n].element.getAttribute("data-x"),o=e.highlighted[n].element.getAttribute("data-y");"checkbox"!=e.options.columns[s].type&&"radio"!=e.options.columns[s].type||t.push(internal.k9.call(e,s,o,!e.options.data[o][s]))}if(t.length){utils_history.Dh.call(e,{action:"setValue",records:t,selection:e.selectedCell});const s=t.map((function(e){return{x:e.x,y:e.y,value:e.value,oldValue:e.oldValue}}));dispatch.A.call(e,"onafterchanges",e,s)}};var lazyLoading=__webpack_require__(497);const upGet=function(e,t){const s=this;e=parseInt(e);for(let n=(t=parseInt(t))-1;n>=0;n--)if("none"!=s.records[n][e].element.style.display&&"none"!=s.rows[n].element.style.display){if(s.records[n][e].element.getAttribute("data-merged")&&s.records[n][e].element==s.records[t][e].element)continue;t=n;break}return t},upVisible=function(e,t){const s=this;let n,o;if(0==e?(n=parseInt(s.selectedCell[0]),o=parseInt(s.selectedCell[1])):(n=parseInt(s.selectedCell[2]),o=parseInt(s.selectedCell[3])),0==t){for(let e=0;e<o;e++)if("none"!=s.records[e][n].element.style.display&&"none"!=s.rows[e].element.style.display){o=e;break}}else o=upGet.call(s,n,o);0==e?(s.selectedCell[0]=n,s.selectedCell[1]=o):(s.selectedCell[2]=n,s.selectedCell[3]=o)},up=function(e,t){const s=this;if(e?s.selectedCell[3]>0&&upVisible.call(s,1,t?0:1):(s.selectedCell[1]>0&&upVisible.call(s,0,t?0:1),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),1==s.options.lazyLoading)if(0==s.selectedCell[1]||0==s.selectedCell[3])lazyLoading.wu.call(s,0),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]);else if(lazyLoading.AG.call(s))s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]);else{const e=parseInt(s.tbody.firstChild.getAttribute("data-y"));s.selectedCell[1]-e<30&&(lazyLoading.G_.call(s),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]))}else if(s.options.pagination>0){const e=s.whichPage(s.selectedCell[3]);e!=s.pageNumber&&s.page(e)}internal.Rs.call(s,1)},rightGet=function(e,t){const s=this;e=parseInt(e),t=parseInt(t);for(let n=e+1;n<s.headers.length;n++)if("none"!=s.records[t][n].element.style.display){if(s.records[t][n].element.getAttribute("data-merged")&&s.records[t][n].element==s.records[t][e].element)continue;e=n;break}return e},rightVisible=function(e,t){const s=this;let n,o;if(0==e?(n=parseInt(s.selectedCell[0]),o=parseInt(s.selectedCell[1])):(n=parseInt(s.selectedCell[2]),o=parseInt(s.selectedCell[3])),0==t){for(let e=s.headers.length-1;e>n;e--)if("none"!=s.records[o][e].element.style.display){n=e;break}}else n=rightGet.call(s,n,o);0==e?(s.selectedCell[0]=n,s.selectedCell[1]=o):(s.selectedCell[2]=n,s.selectedCell[3]=o)},right=function(e,t){const s=this;e?s.selectedCell[2]<s.headers.length-1&&rightVisible.call(s,1,t?0:1):(s.selectedCell[0]<s.headers.length-1&&rightVisible.call(s,0,t?0:1),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),internal.Rs.call(s,2)},downGet=function(e,t){const s=this;e=parseInt(e);for(let n=(t=parseInt(t))+1;n<s.rows.length;n++)if("none"!=s.records[n][e].element.style.display&&"none"!=s.rows[n].element.style.display){if(s.records[n][e].element.getAttribute("data-merged")&&s.records[n][e].element==s.records[t][e].element)continue;t=n;break}return t},downVisible=function(e,t){const s=this;let n,o;if(0==e?(n=parseInt(s.selectedCell[0]),o=parseInt(s.selectedCell[1])):(n=parseInt(s.selectedCell[2]),o=parseInt(s.selectedCell[3])),0==t){for(let e=s.rows.length-1;e>o;e--)if("none"!=s.records[e][n].element.style.display&&"none"!=s.rows[e].element.style.display){o=e;break}}else o=downGet.call(s,n,o);0==e?(s.selectedCell[0]=n,s.selectedCell[1]=o):(s.selectedCell[2]=n,s.selectedCell[3]=o)},down=function(e,t){const s=this;if(e?s.selectedCell[3]<s.records.length-1&&downVisible.call(s,1,t?0:1):(s.selectedCell[1]<s.records.length-1&&downVisible.call(s,0,t?0:1),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),1==s.options.lazyLoading)s.selectedCell[1]==s.records.length-1||s.selectedCell[3]==s.records.length-1?(lazyLoading.wu.call(s,-1),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3])):lazyLoading.AG.call(s)?s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]):parseInt(s.tbody.lastChild.getAttribute("data-y"))-s.selectedCell[3]<30&&(lazyLoading.p6.call(s),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]));else if(s.options.pagination>0){const e=s.whichPage(s.selectedCell[3]);e!=s.pageNumber&&s.page(e)}internal.Rs.call(s,3)},leftGet=function(e,t){const s=this;e=parseInt(e),t=parseInt(t);for(let n=e-1;n>=0;n--)if("none"!=s.records[t][n].element.style.display){if(s.records[t][n].element.getAttribute("data-merged")&&s.records[t][n].element==s.records[t][e].element)continue;e=n;break}return e},leftVisible=function(e,t){const s=this;let n,o;if(0==e?(n=parseInt(s.selectedCell[0]),o=parseInt(s.selectedCell[1])):(n=parseInt(s.selectedCell[2]),o=parseInt(s.selectedCell[3])),0==t){for(let e=0;e<n;e++)if("none"!=s.records[o][e].element.style.display){n=e;break}}else n=leftGet.call(s,n,o);0==e?(s.selectedCell[0]=n,s.selectedCell[1]=o):(s.selectedCell[2]=n,s.selectedCell[3]=o)},left=function(e,t){const s=this;e?s.selectedCell[2]>0&&leftVisible.call(s,1,t?0:1):(s.selectedCell[0]>0&&leftVisible.call(s,0,t?0:1),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),internal.Rs.call(s,0)},first=function(e,t){const s=this;if(e?t?s.selectedCell[3]=0:leftVisible.call(s,1,0):(t?s.selectedCell[1]=0:leftVisible.call(s,0,0),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),1!=s.options.lazyLoading||0!=s.selectedCell[1]&&0!=s.selectedCell[3]){if(s.options.pagination>0){const e=s.whichPage(s.selectedCell[3]);e!=s.pageNumber&&s.page(e)}}else lazyLoading.wu.call(s,0);s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),internal.Rs.call(s,1)},last=function(e,t){const s=this;if(e?t?s.selectedCell[3]=s.records.length-1:rightVisible.call(s,1,0):(t?s.selectedCell[1]=s.records.length-1:rightVisible.call(s,0,0),s.selectedCell[2]=s.selectedCell[0],s.selectedCell[3]=s.selectedCell[1]),1!=s.options.lazyLoading||s.selectedCell[1]!=s.records.length-1&&s.selectedCell[3]!=s.records.length-1){if(s.options.pagination>0){const e=s.whichPage(s.selectedCell[3]);e!=s.pageNumber&&s.page(e)}}else lazyLoading.wu.call(s,-1);s.updateSelectionFromCoords(s.selectedCell[0],s.selectedCell[1],s.selectedCell[2],s.selectedCell[3]),internal.Rs.call(s,3)};var merges=__webpack_require__(910),selection=__webpack_require__(657),helpers=__webpack_require__(978),internalHelpers=__webpack_require__(689);const copy=function(e,t,s,n,o,r,l){const i=this;t||(t="\t");const a=new RegExp(t,"ig"),c=[];let d=[],u=[];const p=[],h=[],m=i.options.data[0].length,f=i.options.data.length;let g="",y=!1,b="",C="",j=0,w=0,_=0,B=0,v=!0;for(let t=0;t<f;t++)for(let s=0;s<m;s++)e&&!i.records[t][s].element.classList.contains("highlight")||(_<=s&&(_=s),B<=t&&(B=t));if(m===_+1&&f===B+1&&!1,o&&(1==i.parent.config.includeHeadersOnDownload||n)){if(i.options.nestedHeaders&&i.options.nestedHeaders.length>0){g=i.options.nestedHeaders;for(let e=0;e<g.length;e++){const s=[];for(let t=0;t<g[e].length;t++){const n=parseInt(g[e][t].colspan);s.push(g[e][t].title);for(let e=0;e<n-1;e++)s.push("")}C+=s.join(t)+"\r\n"}}y=!0}i.style=[];for(let s=0;s<f;s++){d=[],u=[];for(let t=0;t<m;t++)if(!e||i.records[s][t].element.classList.contains("highlight")){1==y&&c.push(i.headers[t].textContent);let e,n=i.options.data[s][t];n.match&&(n.match(a)||n.match(/,/g)||n.match(/\n/)||n.match(/\"/))&&(n=n.replace(new RegExp('"',"g"),'""'),n='"'+n+'"'),d.push(n),i.options.columns&&i.options.columns[t]&&("checkbox"==i.options.columns[t].type||"radio"==i.options.columns[t].type)?e=n:(e=i.records[s][t].element.innerHTML,e.match&&(e.match(a)||e.match(/,/g)||e.match(/\n/)||e.match(/\"/))&&(e=e.replace(new RegExp('"',"g"),'""'),e='"'+e+'"')),u.push(e),g=i.records[s][t].element.getAttribute("style"),g=g.replace("display: none;",""),i.style.push(g||"")}d.length&&(y&&(j=d.length,p.push(c.join(t))),p.push(d.join(t))),u.length&&(w++,y&&(h.push(c.join(t)),y=!1),h.push(u.join(t)))}m==j&&f==w&&(b=C);const A=b+p.join("\r\n");let x=b+h.join("\r\n");if(!s){const e=[Math.min(i.selectedCell[0],i.selectedCell[2]),Math.min(i.selectedCell[1],i.selectedCell[3]),Math.max(i.selectedCell[0],i.selectedCell[2]),Math.max(i.selectedCell[1],i.selectedCell[3])],t=dispatch.A.call(i,"oncopy",i,e,x,r);if(t)x=t;else if(!1===t)return!1;i.textarea.value=x,i.textarea.select(),document.execCommand("copy")}if(i.data=1==l?x:A,i.hashString=selection.tW.call(i,i.data),!s&&(selection.kA.call(i),i.highlighted))for(let e=0;e<i.highlighted.length;e++)i.highlighted[e].element.classList.add("copying"),i.highlighted[e].element.classList.contains("highlight-left")&&i.highlighted[e].element.classList.add("copying-left"),i.highlighted[e].element.classList.contains("highlight-right")&&i.highlighted[e].element.classList.add("copying-right"),i.highlighted[e].element.classList.contains("highlight-top")&&i.highlighted[e].element.classList.add("copying-top"),i.highlighted[e].element.classList.contains("highlight-bottom")&&i.highlighted[e].element.classList.add("copying-bottom");return i.data},paste=function(e,t,s){const n=this,o=(0,selection.tW)(s);let r=o==n.hashString?n.style:null;o==n.hashString&&(s=n.data),s=(0,helpers.parseCSV)(s,"\t");const l=n.selectedCell[2]-e+1,i=n.selectedCell[3]-t+1,a=s[0].length;if(l>1&Number.isInteger(l/a)){const e=l/a;if(r){const t=[];for(let s=0;s<r.length;s+=a){const n=r.slice(s,s+a);for(let s=0;s<e;s++)t.push(...n)}r=t}const t=s.map((function(t,s){const n=Array.apply(null,{length:e*t.length}).map((function(e,s){return t[s%t.length]}));return n}));s=t}const c=s.length;if(i>1&Number.isInteger(i/c)){const e=i/c;if(r){const t=[];for(let s=0;s<e;s++)t.push(...r);r=t}const t=Array.apply(null,{length:e*c}).map((function(e,t){return s[t%c]}));s=t}const d=dispatch.A.call(n,"onbeforepaste",n,s.map((function(e){return e.map((function(e){return{value:e}}))})),e,t);if(!1===d)return!1;if(d&&(s=d),null!=e&&null!=t&&s){let o=0,l=0;const i=[],a={},c={};let d=0,u=parseInt(e),p=parseInt(t),h=null;const m=n.headers.slice(u).filter((e=>"none"===e.style.display)).length,f=u+m+s[0].length,g=n.headers.length;f>g&&(n.skipUpdateTableReferences=!0,n.insertColumn(f-g));const y=n.rows.slice(p).filter((e=>"none"===e.element.style.display)).length,b=p+y+s.length,C=n.rows.length;for(b>C&&(n.skipUpdateTableReferences=!0,n.insertRow(b-C)),n.skipUpdateTableReferences&&(n.skipUpdateTableReferences=!1,internal.o8.call(n));h=s[l];){for(o=0,u=parseInt(e);null!=h[o];){let e=h[o];n.options.columns&&n.options.columns[o]&&"calendar"==n.options.columns[o].type&&(e=jSuites.calendar.extractDateFromString(e,n.options.columns[o].options&&n.options.columns[o].options.format||"YYYY-MM-DD"));const t=internal.k9.call(n,u,p,e);if(i.push(t),internal.xF.call(n,u,p,i),r&&r[d]){const e=(0,internalHelpers.t3)([u,p]);a[e]=r[d],c[e]=n.getStyle(e),n.records[p][u].element.setAttribute("style",r[d]),d++}if(o++,null!=h[o]){if(u>=n.headers.length-1){if(0==n.options.allowInsertColumn)break;n.insertColumn()}u=rightGet.call(n,u,p)}}if(l++,s[l]){if(p>=n.rows.length-1){if(0==n.options.allowInsertRow)break;n.insertRow()}p=downGet.call(n,e,p)}}selection.AH.call(n,e,t,u,p),utils_history.Dh.call(n,{action:"setValue",records:i,selection:n.selectedCell,newStyle:a,oldStyle:c}),internal.am.call(n);const j=[];for(let n=0;n<s.length;n++)for(let o=0;o<s[n].length;o++)j.push({x:o+e,y:n+t,value:s[n][o]});dispatch.A.call(n,"onpaste",n,j);const w=i.map((function(e){return{x:e.x,y:e.y,value:e.value,oldValue:e.oldValue}}));dispatch.A.call(n,"onafterchanges",n,w)}(0,selection.kA)()};var filter=__webpack_require__(829),footer=__webpack_require__(160);const getNumberOfColumns=function(){const e=this;let t=e.options.columns&&e.options.columns.length||0;if(e.options.data&&void 0!==e.options.data[0]){const s=Object.keys(e.options.data[0]);s.length>t&&(t=s.length)}return e.options.minDimensions&&e.options.minDimensions[0]>t&&(t=e.options.minDimensions[0]),t},createCellHeader=function(e){const t=this,s=t.options.columns&&t.options.columns[e]&&t.options.columns[e].width||t.options.defaultColWidth||100,n=t.options.columns&&t.options.columns[e]&&t.options.columns[e].align||t.options.defaultColAlign||"center";t.headers[e]=document.createElement("td"),t.headers[e].textContent=t.options.columns&&t.options.columns[e]&&t.options.columns[e].title||(0,helpers.getColumnName)(e),t.headers[e].setAttribute("data-x",e),t.headers[e].style.textAlign=n,t.options.columns&&t.options.columns[e]&&t.options.columns[e].title&&t.headers[e].setAttribute("title",t.headers[e].innerText),t.options.columns&&t.options.columns[e]&&t.options.columns[e].id&&t.headers[e].setAttribute("id",t.options.columns[e].id);const o=document.createElement("col");o.setAttribute("width",s),t.cols[e]={colElement:o,x:e},t.options.columns&&t.options.columns[e]&&"hidden"==t.options.columns[e].type&&(t.headers[e].style.display="none",o.style.display="none")},insertColumn=function(e,t,s,n){const o=this;if(0!=o.options.allowInsertColumn){let r,l=[];Array.isArray(e)?(r=1,e&&(l=e)):r="number"==typeof e?e:1,s=!!s;const i=Math.max(o.options.columns.length,...o.options.data.map((function(e){return e.length})))-1;(null==t||t>=parseInt(i)||t<0)&&(t=i),n||(n=[]);for(let e=0;e<r;e++)n[e]||(n[e]={});const a=[];if(Array.isArray(e)){const r=[];for(let t=0;t<o.options.data.length;t++)r.push(t<e.length?e[t]:"");const l={column:t+(s?0:1),options:Object.assign({},n[0]),data:r};a.push(l)}else for(let o=0;o<e;o++){const e={column:t+o+(s?0:1),options:Object.assign({},n[o])};a.push(e)}if(!1===dispatch.A.call(o,"onbeforeinsertcolumn",o,a))return!1;if(o.options.mergeCells&&Object.keys(o.options.mergeCells).length>0&&merges.Lt.call(o,t,s).length){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;o.destroyMerge()}const c=s?t:t+1;o.options.columns=(0,internalHelpers.Hh)(o.options.columns,c,n);const d=o.headers.splice(c),u=o.cols.splice(c),p=[],h=[],m=[],f=[],g=[];for(let e=c;e<r+c;e++)createCellHeader.call(o,e),o.headerContainer.insertBefore(o.headers[e],o.headerContainer.children[e+1]),o.colgroupContainer.insertBefore(o.cols[e].colElement,o.colgroupContainer.children[e+1]),p.push(o.headers[e]),h.push(o.cols[e]);if(o.options.footers)for(let e=0;e<o.options.footers.length;e++){g[e]=[];for(let t=0;t<r;t++)g[e].push("");o.options.footers[e].splice(c,0,g[e])}for(let e=0;e<o.options.data.length;e++){const t=o.options.data[e].splice(c),s=o.records[e].splice(c);f[e]=[],m[e]=[];for(let t=c;t<r+c;t++){const s=l[e]?l[e]:"";o.options.data[e][t]=s;const n=internal.P9.call(o,t,e,o.options.data[e][t]);o.records[e][t]={element:n,y:e},o.rows[e]&&o.rows[e].element.insertBefore(n,o.rows[e].element.children[t+1]),o.options.columns&&o.options.columns[t]&&"function"==typeof o.options.columns[t].render&&o.options.columns[t].render(n,s,parseInt(t),parseInt(e),o,o.options.columns[t]),f[e].push(s),m[e].push({element:n,x:t,y:e})}Array.prototype.push.apply(o.options.data[e],t),Array.prototype.push.apply(o.records[e],s)}Array.prototype.push.apply(o.headers,d),Array.prototype.push.apply(o.cols,u);for(let e=c;e<o.cols.length;e++)o.cols[e].x=e;for(let e=0;e<o.records.length;e++)for(let t=0;t<o.records[e].length;t++)o.records[e][t].x=t;if(o.options.nestedHeaders&&o.options.nestedHeaders.length>0&&o.options.nestedHeaders[0]&&o.options.nestedHeaders[0][0])for(let e=0;e<o.options.nestedHeaders.length;e++){const t=parseInt(o.options.nestedHeaders[e][o.options.nestedHeaders[e].length-1].colspan)+r;o.options.nestedHeaders[e][o.options.nestedHeaders[e].length-1].colspan=t,o.thead.children[e].children[o.thead.children[e].children.length-1].setAttribute("colspan",t);let s=o.thead.children[e].children[o.thead.children[e].children.length-1].getAttribute("data-column");s=s.split(",");for(let e=c;e<r+c;e++)s.push(e);o.thead.children[e].children[o.thead.children[e].children.length-1].setAttribute("data-column",s)}utils_history.Dh.call(o,{action:"insertColumn",columnNumber:t,numOfColumns:r,insertBefore:s,columns:n,headers:p,cols:h,records:m,footers:g,data:f}),internal.o8.call(o),dispatch.A.call(o,"oninsertcolumn",o,a)}},moveColumn=function(e,t){const s=this;if(s.options.mergeCells&&Object.keys(s.options.mergeCells).length>0){let n;if(n=e>t?1:0,merges.Lt.call(s,e).length||merges.Lt.call(s,t,n).length){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;s.destroyMerge()}}if((e=parseInt(e))>(t=parseInt(t))){s.headerContainer.insertBefore(s.headers[e],s.headers[t]),s.colgroupContainer.insertBefore(s.cols[e].colElement,s.cols[t].colElement);for(let n=0;n<s.rows.length;n++)s.rows[n].element.insertBefore(s.records[n][e].element,s.records[n][t].element)}else{s.headerContainer.insertBefore(s.headers[e],s.headers[t].nextSibling),s.colgroupContainer.insertBefore(s.cols[e].colElement,s.cols[t].colElement.nextSibling);for(let n=0;n<s.rows.length;n++)s.rows[n].element.insertBefore(s.records[n][e].element,s.records[n][t].element.nextSibling)}s.options.columns.splice(t,0,s.options.columns.splice(e,1)[0]),s.headers.splice(t,0,s.headers.splice(e,1)[0]),s.cols.splice(t,0,s.cols.splice(e,1)[0]);const n=Math.min(e,t),o=Math.max(e,t);for(let n=0;n<s.rows.length;n++)s.options.data[n].splice(t,0,s.options.data[n].splice(e,1)[0]),s.records[n].splice(t,0,s.records[n].splice(e,1)[0]);for(let e=n;e<=o;e++)s.cols[e].x=e;for(let e=0;e<s.records.length;e++)for(let t=n;t<=o;t++)s.records[e][t].x=t;if(s.options.footers)for(let n=0;n<s.options.footers.length;n++)s.options.footers[n].splice(t,0,s.options.footers[n].splice(e,1)[0]);utils_history.Dh.call(s,{action:"moveColumn",oldValue:e,newValue:t}),internal.o8.call(s),dispatch.A.call(s,"onmovecolumn",s,e,t,1)},deleteColumn=function(e,t){const s=this;if(0!=s.options.allowDeleteColumn)if(s.headers.length>1){if(null==e){const n=s.getSelectedColumns(!0);n.length?(e=parseInt(n[0]),t=parseInt(n.length)):(e=s.headers.length-1,t=1)}const n=s.options.data[0].length-1;(null==e||e>n||e<0)&&(e=n),t||(t=1),t>s.options.data[0].length-e&&(t=s.options.data[0].length-e);const o=[];for(let s=0;s<t;s++)o.push(s+e);if(!1===dispatch.A.call(s,"onbeforedeletecolumn",s,o))return!1;if(parseInt(e)>-1){let n=!1;if(s.options.mergeCells&&Object.keys(s.options.mergeCells).length>0)for(let o=e;o<e+t;o++)merges.Lt.call(s,o,null).length&&(n=!0);if(n){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;s.destroyMerge()}const r=s.options.columns?s.options.columns.splice(e,t):void 0;for(let n=e;n<e+t;n++)s.cols[n].colElement.className="",s.headers[n].className="",s.cols[n].colElement.parentNode.removeChild(s.cols[n].colElement),s.headers[n].parentNode.removeChild(s.headers[n]);const l=s.headers.splice(e,t),i=s.cols.splice(e,t),a=[],c=[],d=[];for(let n=0;n<s.options.data.length;n++)for(let o=e;o<e+t;o++)s.records[n][o].element.className="",s.records[n][o].element.parentNode.removeChild(s.records[n][o].element);for(let n=0;n<s.options.data.length;n++)c[n]=s.options.data[n].splice(e,t),a[n]=s.records[n].splice(e,t);for(let t=e;t<s.cols.length;t++)s.cols[t].x=t;for(let t=0;t<s.records.length;t++)for(let n=e;n<s.records[t].length;n++)s.records[t][n].x=n;if(s.options.footers)for(let n=0;n<s.options.footers.length;n++)d[n]=s.options.footers[n].splice(e,t);if(selection.at.call(s,0,e,e+t-1),s.options.nestedHeaders&&s.options.nestedHeaders.length>0&&s.options.nestedHeaders[0]&&s.options.nestedHeaders[0][0])for(let e=0;e<s.options.nestedHeaders.length;e++){const n=parseInt(s.options.nestedHeaders[e][s.options.nestedHeaders[e].length-1].colspan)-t;s.options.nestedHeaders[e][s.options.nestedHeaders[e].length-1].colspan=n,s.thead.children[e].children[s.thead.children[e].children.length-1].setAttribute("colspan",n)}utils_history.Dh.call(s,{action:"deleteColumn",columnNumber:e,numOfColumns:t,insertBefore:1,columns:r,headers:l,cols:i,records:a,footers:d,data:c}),internal.o8.call(s),dispatch.A.call(s,"ondeletecolumn",s,o)}}else console.error("Jspreadsheet: It is not possible to delete the last column")},getWidth=function(e){const t=this;let s;if(void 0===e){s=[];for(let e=0;e<t.headers.length;e++)s.push(t.options.columns&&t.options.columns[e]&&t.options.columns[e].width||t.options.defaultColWidth||100)}else s=parseInt(t.cols[e].colElement.getAttribute("width"));return s},setWidth=function(e,t,s){const n=this;if(t){if(Array.isArray(e)){s||(s=[]);for(let o=0;o<e.length;o++){s[o]||(s[o]=parseInt(n.cols[e[o]].colElement.getAttribute("width")));const r=Array.isArray(t)&&t[o]?t[o]:t;n.cols[e[o]].colElement.setAttribute("width",r),n.options.columns||(n.options.columns=[]),n.options.columns[e[o]]||(n.options.columns[e[o]]={}),n.options.columns[e[o]].width=r}}else s||(s=parseInt(n.cols[e].colElement.getAttribute("width"))),n.cols[e].colElement.setAttribute("width",t),n.options.columns||(n.options.columns=[]),n.options.columns[e]||(n.options.columns[e]={}),n.options.columns[e].width=t;utils_history.Dh.call(n,{action:"setWidth",column:e,oldValue:s,newValue:t}),dispatch.A.call(n,"onresizecolumn",n,e,t,s),selection.Aq.call(n)}},showColumn=function(e){const t=this;Array.isArray(e)||(e=[e]);for(let s=0;s<e.length;s++){const n=e[s];t.headers[n].style.display="",t.cols[n].colElement.style.display="",t.filter&&t.filter.children.length>n+1&&(t.filter.children[n+1].style.display="");for(let e=0;e<t.options.data.length;e++)t.records[e][n].element.style.display=""}t.options.footers&&footer.e.call(t),t.resetSelection()},hideColumn=function(e){const t=this;Array.isArray(e)||(e=[e]);for(let s=0;s<e.length;s++){const n=e[s];t.headers[n].style.display="none",t.cols[n].colElement.style.display="none",t.filter&&t.filter.children.length>n+1&&(t.filter.children[n+1].style.display="none");for(let e=0;e<t.options.data.length;e++)t.records[e][n].element.style.display="none"}t.options.footers&&footer.e.call(t),t.resetSelection()},getColumnData=function(e,t){const s=this,n=[];for(let o=0;o<s.options.data.length;o++)t?n.push(s.records[o][e].element.innerHTML):n.push(s.options.data[o][e]);return n},setColumnData=function(e,t,s){const n=this;for(let o=0;o<n.rows.length;o++){const r=(0,internalHelpers.t3)([e,o]);null!=t[o]&&n.setValue(r,t[o],s)}},createRow=function(e,t){const s=this;s.records[e]||(s.records[e]=[]),t||(t=s.options.data[e]);const n={element:document.createElement("tr"),y:e};s.rows[e]=n,n.element.setAttribute("data-y",e);let o=null;s.options.defaultRowHeight&&(n.element.style.height=s.options.defaultRowHeight+"px"),s.options.rows&&s.options.rows[e]&&(s.options.rows[e].height&&(n.element.style.height=s.options.rows[e].height),s.options.rows[e].title&&(o=s.options.rows[e].title)),o||(o=parseInt(e+1));const r=document.createElement("td");r.innerHTML=o,r.setAttribute("data-y",e),r.className="jss_row",n.element.appendChild(r);const l=getNumberOfColumns.call(s);for(let o=0;o<l;o++)s.records[e][o]={element:internal.P9.call(this,o,e,t[o]),x:o,y:e},n.element.appendChild(s.records[e][o].element),s.options.columns&&s.options.columns[o]&&"function"==typeof s.options.columns[o].render&&s.options.columns[o].render(s.records[e][o].element,t[o],parseInt(o),parseInt(e),s,s.options.columns[o]);return n},insertRow=function(e,t,s){const n=this;if(0!=n.options.allowInsertRow){let o,r=[];Array.isArray(e)?(o=1,e&&(r=e)):o=void 0!==e?e:1,s=!!s;const l=n.options.data.length-1;(null==t||t>=parseInt(l)||t<0)&&(t=l);const i=[];for(let e=0;e<o;e++){const o=[];for(let e=0;e<n.options.columns.length;e++)o[e]=r[e]?r[e]:"";i.push({row:e+t+(s?0:1),data:o})}if(!1===dispatch.A.call(n,"onbeforeinsertrow",n,i))return!1;if(n.options.mergeCells&&Object.keys(n.options.mergeCells).length>0&&merges.D0.call(n,t,s).length){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;n.destroyMerge()}if(1==n.options.search){if(n.results&&n.results.length!=n.rows.length){if(!confirm(jSuites.translate("This action will clear your search results. Are you sure?")))return!1;n.resetSearch()}n.results=null}const a=s?t:t+1,c=n.records.splice(a),d=n.options.data.splice(a),u=n.rows.splice(a),p=[],h=[],m=[];for(let e=a;e<o+a;e++){n.options.data[e]=[];for(let t=0;t<n.options.columns.length;t++)n.options.data[e][t]=r[t]?r[t]:"";const s=createRow.call(n,e,n.options.data[e]);u[0]?Array.prototype.indexOf.call(n.tbody.children,u[0].element)>=0&&n.tbody.insertBefore(s.element,u[0].element):Array.prototype.indexOf.call(n.tbody.children,n.rows[t].element)>=0&&n.tbody.appendChild(s.element),p.push([...n.records[e]]),h.push([...n.options.data[e]]),m.push(s)}Array.prototype.push.apply(n.records,c),Array.prototype.push.apply(n.options.data,d),Array.prototype.push.apply(n.rows,u);for(let e=a;e<n.rows.length;e++)n.rows[e].y=e;for(let e=a;e<n.records.length;e++)for(let t=0;t<n.records[e].length;t++)n.records[e][t].y=e;n.options.pagination>0&&n.page(n.pageNumber),utils_history.Dh.call(n,{action:"insertRow",rowNumber:t,numOfRows:o,insertBefore:s,rowRecords:p,rowData:h,rowNode:m}),internal.o8.call(n),dispatch.A.call(n,"oninsertrow",n,i)}},moveRow=function(e,t,s){const n=this;if(n.options.mergeCells&&Object.keys(n.options.mergeCells).length>0){let s;if(s=e>t?1:0,merges.D0.call(n,e).length||merges.D0.call(n,t,s).length){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;n.destroyMerge()}}if(1==n.options.search){if(n.results&&n.results.length!=n.rows.length){if(!confirm(jSuites.translate("This action will clear your search results. Are you sure?")))return!1;n.resetSearch()}n.results=null}s||(Array.prototype.indexOf.call(n.tbody.children,n.rows[t].element)>=0?e>t?n.tbody.insertBefore(n.rows[e].element,n.rows[t].element):n.tbody.insertBefore(n.rows[e].element,n.rows[t].element.nextSibling):n.tbody.removeChild(n.rows[e].element)),n.rows.splice(t,0,n.rows.splice(e,1)[0]),n.records.splice(t,0,n.records.splice(e,1)[0]),n.options.data.splice(t,0,n.options.data.splice(e,1)[0]);const o=Math.min(e,t),r=Math.max(e,t);for(let e=o;e<=r;e++)n.rows[e].y=e;for(let e=o;e<=r;e++)for(let t=0;t<n.records[e].length;t++)n.records[e][t].y=e;n.options.pagination>0&&n.tbody.children.length!=n.options.pagination&&n.page(n.pageNumber),utils_history.Dh.call(n,{action:"moveRow",oldValue:e,newValue:t}),internal.o8.call(n),dispatch.A.call(n,"onmoverow",n,parseInt(e),parseInt(t),1)},deleteRow=function(e,t){const s=this;if(0!=s.options.allowDeleteRow)if(1==s.options.allowDeletingAllRows||s.options.data.length>1){if(null==e){const n=selection.R5.call(s);0===n.length?(e=s.options.data.length-1,t=1):(e=n[0],t=n.length)}let n=s.options.data.length-1;(null==e||e>n||e<0)&&(e=n),t||(t=1),e+t>=s.options.data.length&&(t=s.options.data.length-e);const o=[];for(let s=0;s<t;s++)o.push(s+e);if(!1===dispatch.A.call(s,"onbeforedeleterow",s,o))return!1;if(parseInt(e)>-1){let r=!1;if(s.options.mergeCells&&Object.keys(s.options.mergeCells).length>0)for(let n=e;n<e+t;n++)merges.D0.call(s,n,!1).length&&(r=!0);if(r){if(!confirm(jSuites.translate("This action will destroy any existing merged cells. Are you sure?")))return!1;s.destroyMerge()}if(1==s.options.search){if(s.results&&s.results.length!=s.rows.length){if(!confirm(jSuites.translate("This action will clear your search results. Are you sure?")))return!1;s.resetSearch()}s.results=null}1!=s.options.allowDeletingAllRows&&n+1===t&&(t--,console.error("Jspreadsheet: It is not possible to delete the last row"));for(let n=e;n<e+t;n++)Array.prototype.indexOf.call(s.tbody.children,s.rows[n].element)>=0&&(s.rows[n].element.className="",s.rows[n].element.parentNode.removeChild(s.rows[n].element));const l=s.records.splice(e,t),i=s.options.data.splice(e,t),a=s.rows.splice(e,t);for(let t=e;t<s.rows.length;t++)s.rows[t].y=t;for(let t=e;t<s.records.length;t++)for(let e=0;e<s.records[t].length;e++)s.records[t][e].y=t;s.options.pagination>0&&s.tbody.children.length!=s.options.pagination&&s.page(s.pageNumber),selection.at.call(s,1,e,e+t-1),utils_history.Dh.call(s,{action:"deleteRow",rowNumber:e,numOfRows:t,insertBefore:1,rowRecords:l,rowData:i,rowNode:a}),internal.o8.call(s),dispatch.A.call(s,"ondeleterow",s,o)}}else console.error("Jspreadsheet: It is not possible to delete the last row")},getHeight=function(e){const t=this;let s;if(void 0===e){s=[];for(let e=0;e<t.rows.length;e++){const n=t.rows[e].element.style.height;n&&(s[e]=n)}}else"object"==typeof e&&(e=$(e).getAttribute("data-y")),s=t.rows[e].element.style.height;return s},setHeight=function(e,t,s){const n=this;t>0&&(s||(s=n.rows[e].element.getAttribute("height"))||(s=n.rows[e].element.getBoundingClientRect().height),t=parseInt(t),n.rows[e].element.style.height=t+"px",n.options.rows||(n.options.rows=[]),n.options.rows[e]||(n.options.rows[e]={}),n.options.rows[e].height=t,utils_history.Dh.call(n,{action:"setHeight",row:e,oldValue:s,newValue:t}),dispatch.A.call(n,"onresizerow",n,e,t,s),selection.Aq.call(n))},showRow=function(e){const t=this;Array.isArray(e)||(e=[e]),e.forEach((function(e){t.rows[e].element.style.display=""}))},hideRow=function(e){const t=this;Array.isArray(e)||(e=[e]),e.forEach((function(e){t.rows[e].element.style.display="none"}))},getRowData=function(e,t){return t?this.records[e].map((function(e){return e.element.innerHTML})):this.options.data[e]},setRowData=function(e,t,s){const n=this;for(let o=0;o<n.headers.length;o++){const r=(0,internalHelpers.t3)([o,e]);null!=t[o]&&n.setValue(r,t[o],s)}};var version={version:"5.0.0",host:"https://bossanova.uk/jspreadsheet",license:"MIT",print:function(){return[["Jspreadsheet CE",this.version,this.host,this.license].join("\r\n")]}};const getElement=function(e){let t=0,s=0;return function e(n){n.className&&(n.classList.contains("jss_container")&&(s=n),n.classList.contains("jss_spreadsheet")&&(s=n.querySelector(":scope > .jtabs-content > .jtabs-selected"))),"THEAD"==n.tagName?t=1:"TBODY"==n.tagName&&(t=2),n.parentNode&&(s||e(n.parentNode))}(e),[s,t]},mouseUpControls=function(e){if(libraryBase.jspreadsheet.current)if(libraryBase.jspreadsheet.current.resizing){if(libraryBase.jspreadsheet.current.resizing.column){const e=parseInt(libraryBase.jspreadsheet.current.cols[libraryBase.jspreadsheet.current.resizing.column].colElement.getAttribute("width")),t=libraryBase.jspreadsheet.current.getSelectedColumns();if(t.length>1){const s=[];for(let e=0;e<t.length;e++)s.push(parseInt(libraryBase.jspreadsheet.current.cols[t[e]].colElement.getAttribute("width")));s[t.indexOf(parseInt(libraryBase.jspreadsheet.current.resizing.column))]=libraryBase.jspreadsheet.current.resizing.width,setWidth.call(libraryBase.jspreadsheet.current,t,e,s)}else setWidth.call(libraryBase.jspreadsheet.current,parseInt(libraryBase.jspreadsheet.current.resizing.column),e,libraryBase.jspreadsheet.current.resizing.width);libraryBase.jspreadsheet.current.headers[libraryBase.jspreadsheet.current.resizing.column].classList.remove("resizing");for(let e=0;e<libraryBase.jspreadsheet.current.records.length;e++)libraryBase.jspreadsheet.current.records[e][libraryBase.jspreadsheet.current.resizing.column]&&libraryBase.jspreadsheet.current.records[e][libraryBase.jspreadsheet.current.resizing.column].element.classList.remove("resizing")}else{libraryBase.jspreadsheet.current.rows[libraryBase.jspreadsheet.current.resizing.row].element.children[0].classList.remove("resizing");let e=libraryBase.jspreadsheet.current.rows[libraryBase.jspreadsheet.current.resizing.row].element.getAttribute("height");setHeight.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.resizing.row,e,libraryBase.jspreadsheet.current.resizing.height),libraryBase.jspreadsheet.current.resizing.element.classList.remove("resizing")}libraryBase.jspreadsheet.current.resizing=null}else if(libraryBase.jspreadsheet.current.dragging){if(libraryBase.jspreadsheet.current.dragging){if(libraryBase.jspreadsheet.current.dragging.column){const t=e.target.getAttribute("data-x");libraryBase.jspreadsheet.current.headers[libraryBase.jspreadsheet.current.dragging.column].classList.remove("dragging");for(let e=0;e<libraryBase.jspreadsheet.current.rows.length;e++)libraryBase.jspreadsheet.current.records[e][libraryBase.jspreadsheet.current.dragging.column]&&libraryBase.jspreadsheet.current.records[e][libraryBase.jspreadsheet.current.dragging.column].element.classList.remove("dragging");for(let e=0;e<libraryBase.jspreadsheet.current.headers.length;e++)libraryBase.jspreadsheet.current.headers[e].classList.remove("dragging-left"),libraryBase.jspreadsheet.current.headers[e].classList.remove("dragging-right");t&&libraryBase.jspreadsheet.current.dragging.column!=libraryBase.jspreadsheet.current.dragging.destination&&libraryBase.jspreadsheet.current.moveColumn(libraryBase.jspreadsheet.current.dragging.column,libraryBase.jspreadsheet.current.dragging.destination)}else{let e;libraryBase.jspreadsheet.current.dragging.element.nextSibling?(e=parseInt(libraryBase.jspreadsheet.current.dragging.element.nextSibling.getAttribute("data-y")),libraryBase.jspreadsheet.current.dragging.row<e&&(e-=1)):e=parseInt(libraryBase.jspreadsheet.current.dragging.element.previousSibling.getAttribute("data-y")),libraryBase.jspreadsheet.current.dragging.row!=libraryBase.jspreadsheet.current.dragging.destination&&moveRow.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.dragging.row,e,!0),libraryBase.jspreadsheet.current.dragging.element.classList.remove("dragging")}libraryBase.jspreadsheet.current.dragging=null}}else libraryBase.jspreadsheet.current.selectedCorner&&(libraryBase.jspreadsheet.current.selectedCorner=!1,libraryBase.jspreadsheet.current.selection.length>0&&(selection.kF.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.selection[0],libraryBase.jspreadsheet.current.selection[libraryBase.jspreadsheet.current.selection.length-1]),selection.gG.call(libraryBase.jspreadsheet.current)));libraryBase.jspreadsheet.timeControl&&(clearTimeout(libraryBase.jspreadsheet.timeControl),libraryBase.jspreadsheet.timeControl=null),libraryBase.jspreadsheet.isMouseAction=!1},mouseDownControls=function(e){let t;t=(e=e||window.event).buttons?e.buttons:e.button?e.button:e.which;const s=getElement(e.target);if(s[0]?libraryBase.jspreadsheet.current!=s[0].jssWorksheet&&(libraryBase.jspreadsheet.current&&(libraryBase.jspreadsheet.current.edition&&closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0),libraryBase.jspreadsheet.current.resetSelection()),libraryBase.jspreadsheet.current=s[0].jssWorksheet):libraryBase.jspreadsheet.current&&(libraryBase.jspreadsheet.current.edition&&closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0),e.target.classList.contains("jss_object")||(selection.gE.call(libraryBase.jspreadsheet.current,!0),libraryBase.jspreadsheet.current=null)),libraryBase.jspreadsheet.current&&1==t){if(e.target.classList.contains("jss_selectall"))libraryBase.jspreadsheet.current&&selection.Ub.call(libraryBase.jspreadsheet.current);else if(e.target.classList.contains("jss_corner"))0!=libraryBase.jspreadsheet.current.options.editable&&(libraryBase.jspreadsheet.current.selectedCorner=!0);else{if(1==s[1]){const t=e.target.getAttribute("data-x");if(t){const s=e.target.getBoundingClientRect();if(0!=libraryBase.jspreadsheet.current.options.columnResize&&s.width-e.offsetX<6){libraryBase.jspreadsheet.current.resizing={mousePosition:e.pageX,column:t,width:s.width},libraryBase.jspreadsheet.current.headers[t].classList.add("resizing");for(let e=0;e<libraryBase.jspreadsheet.current.records.length;e++)libraryBase.jspreadsheet.current.records[e][t]&&libraryBase.jspreadsheet.current.records[e][t].element.classList.add("resizing")}else if(0!=libraryBase.jspreadsheet.current.options.columnDrag&&s.height-e.offsetY<6)if(merges.Lt.call(libraryBase.jspreadsheet.current,t).length)console.error("Jspreadsheet: This column is part of a merged cell.");else{libraryBase.jspreadsheet.current.resetSelection(),libraryBase.jspreadsheet.current.dragging={element:e.target,column:t,destination:t},libraryBase.jspreadsheet.current.headers[t].classList.add("dragging");for(let e=0;e<libraryBase.jspreadsheet.current.records.length;e++)libraryBase.jspreadsheet.current.records[e][t]&&libraryBase.jspreadsheet.current.records[e][t].element.classList.add("dragging")}else{let s,n;libraryBase.jspreadsheet.current.selectedHeader&&(e.shiftKey||e.ctrlKey)?(s=libraryBase.jspreadsheet.current.selectedHeader,n=t):(libraryBase.jspreadsheet.current.selectedHeader==t&&0!=libraryBase.jspreadsheet.current.options.allowRenameColumn&&(libraryBase.jspreadsheet.timeControl=setTimeout((function(){libraryBase.jspreadsheet.current.setHeader(t)}),800)),libraryBase.jspreadsheet.current.selectedHeader=t,s=t,n=t),selection.AH.call(libraryBase.jspreadsheet.current,s,0,n,libraryBase.jspreadsheet.current.options.data.length-1,e)}}else if(e.target.parentNode.classList.contains("jss_nested")){let t,s;if(e.target.getAttribute("data-column")){const n=e.target.getAttribute("data-column").split(",");t=parseInt(n[0]),s=parseInt(n[n.length-1])}else t=0,s=libraryBase.jspreadsheet.current.options.columns.length-1;selection.AH.call(libraryBase.jspreadsheet.current,t,0,s,libraryBase.jspreadsheet.current.options.data.length-1,e)}}else libraryBase.jspreadsheet.current.selectedHeader=!1;if(2==s[1]){const t=parseInt(e.target.getAttribute("data-y"));if(e.target.classList.contains("jss_row")){const s=e.target.getBoundingClientRect();if(0!=libraryBase.jspreadsheet.current.options.rowResize&&s.height-e.offsetY<6)libraryBase.jspreadsheet.current.resizing={element:e.target.parentNode,mousePosition:e.pageY,row:t,height:s.height},e.target.parentNode.classList.add("resizing");else if(0!=libraryBase.jspreadsheet.current.options.rowDrag&&s.width-e.offsetX<6)merges.D0.call(libraryBase.jspreadsheet.current,t).length?console.error("Jspreadsheet: This row is part of a merged cell"):1==libraryBase.jspreadsheet.current.options.search&&libraryBase.jspreadsheet.current.results?console.error("Jspreadsheet: Please clear your search before perform this action"):(libraryBase.jspreadsheet.current.resetSelection(),libraryBase.jspreadsheet.current.dragging={element:e.target.parentNode,row:t,destination:t},e.target.parentNode.classList.add("dragging"));else{let s,n;null!=libraryBase.jspreadsheet.current.selectedRow&&(e.shiftKey||e.ctrlKey)?(s=libraryBase.jspreadsheet.current.selectedRow,n=t):(libraryBase.jspreadsheet.current.selectedRow=t,s=t,n=t),selection.AH.call(libraryBase.jspreadsheet.current,null,s,null,n,e)}}else if(e.target.classList.contains("jclose")&&e.target.clientWidth-e.offsetX<50&&e.offsetY<50)closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0);else{const t=function(e){const s=e.getAttribute("data-x"),n=e.getAttribute("data-y");return s&&n?[s,n]:e.parentNode?t(e.parentNode):void 0},s=t(e.target);if(s){const t=s[0],n=s[1];libraryBase.jspreadsheet.current.edition&&(libraryBase.jspreadsheet.current.edition[2]==t&&libraryBase.jspreadsheet.current.edition[3]==n||closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0)),libraryBase.jspreadsheet.current.edition||(e.shiftKey?selection.AH.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.selectedCell[0],libraryBase.jspreadsheet.current.selectedCell[1],t,n,e):selection.AH.call(libraryBase.jspreadsheet.current,t,n,void 0,void 0,e)),libraryBase.jspreadsheet.current.selectedHeader=null,libraryBase.jspreadsheet.current.selectedRow=null}}}else libraryBase.jspreadsheet.current.selectedRow=!1;e.target.classList.contains("jss_page")&&("<"==e.target.textContent?libraryBase.jspreadsheet.current.page(0):">"==e.target.textContent?libraryBase.jspreadsheet.current.page(e.target.getAttribute("title")-1):libraryBase.jspreadsheet.current.page(e.target.textContent-1))}libraryBase.jspreadsheet.current.edition?libraryBase.jspreadsheet.isMouseAction=!1:libraryBase.jspreadsheet.isMouseAction=!0}else libraryBase.jspreadsheet.isMouseAction=!1},mouseMoveControls=function(e){let t;if(t=(e=e||window.event).buttons?e.buttons:e.button?e.button:e.which,t||(libraryBase.jspreadsheet.isMouseAction=!1),libraryBase.jspreadsheet.current)if(1==libraryBase.jspreadsheet.isMouseAction){if(libraryBase.jspreadsheet.current.resizing)if(libraryBase.jspreadsheet.current.resizing.column){const t=e.pageX-libraryBase.jspreadsheet.current.resizing.mousePosition;if(libraryBase.jspreadsheet.current.resizing.width+t>0){const e=libraryBase.jspreadsheet.current.resizing.width+t;libraryBase.jspreadsheet.current.cols[libraryBase.jspreadsheet.current.resizing.column].colElement.setAttribute("width",e),selection.Aq.call(libraryBase.jspreadsheet.current)}}else{const t=e.pageY-libraryBase.jspreadsheet.current.resizing.mousePosition;if(libraryBase.jspreadsheet.current.resizing.height+t>0){const e=libraryBase.jspreadsheet.current.resizing.height+t;libraryBase.jspreadsheet.current.rows[libraryBase.jspreadsheet.current.resizing.row].element.setAttribute("height",e),selection.Aq.call(libraryBase.jspreadsheet.current)}}else if(libraryBase.jspreadsheet.current.dragging)if(libraryBase.jspreadsheet.current.dragging.column){const t=e.target.getAttribute("data-x");if(t)if(merges.Lt.call(libraryBase.jspreadsheet.current,t).length)console.error("Jspreadsheet: This column is part of a merged cell.");else{for(let e=0;e<libraryBase.jspreadsheet.current.headers.length;e++)libraryBase.jspreadsheet.current.headers[e].classList.remove("dragging-left"),libraryBase.jspreadsheet.current.headers[e].classList.remove("dragging-right");libraryBase.jspreadsheet.current.dragging.column==t?libraryBase.jspreadsheet.current.dragging.destination=parseInt(t):e.target.clientWidth/2>e.offsetX?(libraryBase.jspreadsheet.current.dragging.column<t?libraryBase.jspreadsheet.current.dragging.destination=parseInt(t)-1:libraryBase.jspreadsheet.current.dragging.destination=parseInt(t),libraryBase.jspreadsheet.current.headers[t].classList.add("dragging-left")):(libraryBase.jspreadsheet.current.dragging.column<t?libraryBase.jspreadsheet.current.dragging.destination=parseInt(t):libraryBase.jspreadsheet.current.dragging.destination=parseInt(t)+1,libraryBase.jspreadsheet.current.headers[t].classList.add("dragging-right"))}}else{const t=e.target.getAttribute("data-y");if(t)if(merges.D0.call(libraryBase.jspreadsheet.current,t).length)console.error("Jspreadsheet: This row is part of a merged cell.");else{const t=e.target.clientHeight/2>e.offsetY?e.target.parentNode.nextSibling:e.target.parentNode;libraryBase.jspreadsheet.current.dragging.element!=t&&(e.target.parentNode.parentNode.insertBefore(libraryBase.jspreadsheet.current.dragging.element,t),libraryBase.jspreadsheet.current.dragging.destination=Array.prototype.indexOf.call(libraryBase.jspreadsheet.current.dragging.element.parentNode.children,libraryBase.jspreadsheet.current.dragging.element))}}}else{const t=e.target.getAttribute("data-x"),s=e.target.getAttribute("data-y"),n=e.target.getBoundingClientRect();libraryBase.jspreadsheet.current.cursor&&(libraryBase.jspreadsheet.current.cursor.style.cursor="",libraryBase.jspreadsheet.current.cursor=null),e.target.parentNode.parentNode&&e.target.parentNode.parentNode.className&&(e.target.parentNode.parentNode.classList.contains("resizable")&&(e.target&&t&&!s&&n.width-(e.clientX-n.left)<6?(libraryBase.jspreadsheet.current.cursor=e.target,libraryBase.jspreadsheet.current.cursor.style.cursor="col-resize"):e.target&&!t&&s&&n.height-(e.clientY-n.top)<6&&(libraryBase.jspreadsheet.current.cursor=e.target,libraryBase.jspreadsheet.current.cursor.style.cursor="row-resize")),e.target.parentNode.parentNode.classList.contains("draggable")&&(e.target&&!t&&s&&n.width-(e.clientX-n.left)<6||e.target&&t&&!s&&n.height-(e.clientY-n.top)<6)&&(libraryBase.jspreadsheet.current.cursor=e.target,libraryBase.jspreadsheet.current.cursor.style.cursor="move"))}},updateCopySelection=function(e,t){const s=this;selection.gG.call(s);const n=s.selectedContainer[0],o=s.selectedContainer[1],r=s.selectedContainer[2],l=s.selectedContainer[3];if(null!=e&&null!=t){let i,a,c,d;e-r>0?(i=parseInt(r)+1,a=parseInt(e)):(i=parseInt(e),a=parseInt(n)-1),t-l>0?(c=parseInt(l)+1,d=parseInt(t)):(c=parseInt(t),d=parseInt(o)-1),a-i<=d-c?(i=parseInt(n),a=parseInt(r)):(c=parseInt(o),d=parseInt(l));for(let e=c;e<=d;e++)for(let t=i;t<=a;t++)s.records[e][t]&&"none"!=s.rows[e].element.style.display&&"none"!=s.records[e][t].element.style.display&&(s.records[e][t].element.classList.add("selection"),s.records[c][t].element.classList.add("selection-top"),s.records[d][t].element.classList.add("selection-bottom"),s.records[e][i].element.classList.add("selection-left"),s.records[e][a].element.classList.add("selection-right"),s.selection.push(s.records[e][t].element))}},mouseOverControls=function(e){let t;if(t=(e=e||window.event).buttons?e.buttons:e.button?e.button:e.which,t||(libraryBase.jspreadsheet.isMouseAction=!1),libraryBase.jspreadsheet.current&&1==libraryBase.jspreadsheet.isMouseAction){const t=getElement(e.target);if(t[0]){if(libraryBase.jspreadsheet.current!=t[0].jssWorksheet&&libraryBase.jspreadsheet.current)return!1;let s=e.target.getAttribute("data-x");const n=e.target.getAttribute("data-y");if(libraryBase.jspreadsheet.current.resizing||libraryBase.jspreadsheet.current.dragging);else{if(1==t[1]&&libraryBase.jspreadsheet.current.selectedHeader){s=e.target.getAttribute("data-x");const t=libraryBase.jspreadsheet.current.selectedHeader,n=s;selection.AH.call(libraryBase.jspreadsheet.current,t,0,n,libraryBase.jspreadsheet.current.options.data.length-1,e)}if(2==t[1])if(e.target.classList.contains("jss_row")){if(null!=libraryBase.jspreadsheet.current.selectedRow){const t=libraryBase.jspreadsheet.current.selectedRow,s=n;selection.AH.call(libraryBase.jspreadsheet.current,0,t,libraryBase.jspreadsheet.current.options.data[0].length-1,s,e)}}else libraryBase.jspreadsheet.current.edition||s&&n&&(libraryBase.jspreadsheet.current.selectedCorner?updateCopySelection.call(libraryBase.jspreadsheet.current,s,n):libraryBase.jspreadsheet.current.selectedCell&&selection.AH.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.selectedCell[0],libraryBase.jspreadsheet.current.selectedCell[1],s,n,e))}}}libraryBase.jspreadsheet.timeControl&&(clearTimeout(libraryBase.jspreadsheet.timeControl),libraryBase.jspreadsheet.timeControl=null)},doubleClickControls=function(e){if(libraryBase.jspreadsheet.current)if(e.target.classList.contains("jss_corner")){if(libraryBase.jspreadsheet.current.highlighted.length>0){const e=libraryBase.jspreadsheet.current.highlighted[0].element.getAttribute("data-x"),t=parseInt(libraryBase.jspreadsheet.current.highlighted[libraryBase.jspreadsheet.current.highlighted.length-1].element.getAttribute("data-y"))+1,s=libraryBase.jspreadsheet.current.highlighted[libraryBase.jspreadsheet.current.highlighted.length-1].element.getAttribute("data-x"),n=libraryBase.jspreadsheet.current.records.length-1;selection.kF.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.records[t][e].element,libraryBase.jspreadsheet.current.records[n][s].element)}}else if(e.target.classList.contains("jss_column_filter")){const t=e.target.getAttribute("data-x");filter.N$.call(libraryBase.jspreadsheet.current,t)}else{const t=getElement(e.target);if(1==t[1]&&0!=libraryBase.jspreadsheet.current.options.columnSorting){const t=e.target.getAttribute("data-x");t&&libraryBase.jspreadsheet.current.orderBy(parseInt(t))}if(2==t[1]&&0!=libraryBase.jspreadsheet.current.options.editable&&!libraryBase.jspreadsheet.current.edition){const t=function(e){if(e.parentNode){const s=e.getAttribute("data-x"),n=e.getAttribute("data-y");return s&&n?e:t(e.parentNode)}},s=t(e.target);s&&s.classList.contains("highlight")&&openEditor.call(libraryBase.jspreadsheet.current,s,void 0,e)}}},pasteControls=function(e){libraryBase.jspreadsheet.current&&libraryBase.jspreadsheet.current.selectedCell&&(libraryBase.jspreadsheet.current.edition||0!=libraryBase.jspreadsheet.current.options.editable&&(e&&e.clipboardData?(paste.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.selectedCell[0],libraryBase.jspreadsheet.current.selectedCell[1],e.clipboardData.getData("text")),e.preventDefault()):window.clipboardData&&paste.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.selectedCell[0],libraryBase.jspreadsheet.current.selectedCell[1],window.clipboardData.getData("text"))))},getRole=function(e){if(e.classList.contains("jss_selectall"))return"select-all";if(e.classList.contains("jss_corner"))return"fill-handle";let t=e;for(;!t.classList.contains("jss_spreadsheet");){if(t.classList.contains("jss_row"))return"row";if(t.classList.contains("jss_nested"))return"nested";if(t.classList.contains("jtabs-headers"))return"tabs";if(t.classList.contains("jtoolbar"))return"toolbar";if(t.classList.contains("jss_pagination"))return"pagination";if("TBODY"===t.tagName)return"cell";if("TFOOT"===t.tagName)return 0===getElementIndex(e)?"grid":"footer";if("THEAD"===t.tagName)return"header";t=t.parentElement}return"applications"},defaultContextMenu=function(e,t,s,n){const o=[];if("header"===n&&(0!=e.options.allowInsertColumn&&o.push({title:jSuites.translate("Insert a new column before"),onclick:function(){e.insertColumn(1,parseInt(t),1)}}),0!=e.options.allowInsertColumn&&o.push({title:jSuites.translate("Insert a new column after"),onclick:function(){e.insertColumn(1,parseInt(t),0)}}),0!=e.options.allowDeleteColumn&&o.push({title:jSuites.translate("Delete selected columns"),onclick:function(){e.deleteColumn(e.getSelectedColumns().length?void 0:parseInt(t))}}),0!=e.options.allowRenameColumn&&o.push({title:jSuites.translate("Rename this column"),onclick:function(){const s=e.getHeader(t),n=prompt(jSuites.translate("Column name"),s);e.setHeader(t,n)}}),0!=e.options.columnSorting&&(o.push({type:"line"}),o.push({title:jSuites.translate("Order ascending"),onclick:function(){e.orderBy(t,0)}}),o.push({title:jSuites.translate("Order descending"),onclick:function(){e.orderBy(t,1)}}))),"row"!==n&&"cell"!==n||(0!=e.options.allowInsertRow&&(o.push({title:jSuites.translate("Insert a new row before"),onclick:function(){e.insertRow(1,parseInt(s),1)}}),o.push({title:jSuites.translate("Insert a new row after"),onclick:function(){e.insertRow(1,parseInt(s))}})),0!=e.options.allowDeleteRow&&o.push({title:jSuites.translate("Delete selected rows"),onclick:function(){e.deleteRow(e.getSelectedRows().length?void 0:parseInt(s))}})),"cell"===n&&0!=e.options.allowComments){o.push({type:"line"});const n=e.records[s][t].element.getAttribute("title")||"";o.push({title:jSuites.translate(n?"Edit comments":"Add comments"),onclick:function(){const o=prompt(jSuites.translate("Comments"),n);o&&e.setComments((0,helpers.getCellNameFromCoords)(t,s),o)}}),n&&o.push({title:jSuites.translate("Clear comments"),onclick:function(){e.setComments((0,helpers.getCellNameFromCoords)(t,s),"")}})}return 0!==o.length&&o.push({type:"line"}),"header"!==n&&"row"!==n&&"cell"!==n||(o.push({title:jSuites.translate("Copy")+"...",shortcut:"Ctrl + C",onclick:function(){copy.call(e,!0)}}),navigator&&navigator.clipboard&&o.push({title:jSuites.translate("Paste")+"...",shortcut:"Ctrl + V",onclick:function(){e.selectedCell&&navigator.clipboard.readText().then((function(t){t&&paste.call(e,e.selectedCell[0],e.selectedCell[1],t)}))}})),0!=e.parent.config.allowExport&&o.push({title:jSuites.translate("Save as")+"...",shortcut:"Ctrl + S",onclick:function(){e.download()}}),0!=e.parent.config.about&&o.push({title:jSuites.translate("About"),onclick:function(){void 0===e.parent.config.about||!0===e.parent.config.about?alert(version.print()):alert(e.parent.config.about)}}),o},getElementIndex=function(e){const t=e.parentElement.children;for(let s=0;s<t.length;s++)if(e===t[s])return s;return-1},contextMenuControls=function(e){if("buttons"in(e=e||window.event)?e.buttons:e.which||e.button,libraryBase.jspreadsheet.current){const t=libraryBase.jspreadsheet.current.parent;if(libraryBase.jspreadsheet.current.edition)e.preventDefault();else if(t.contextMenu.contextmenu.close(),libraryBase.jspreadsheet.current){const s=getRole(e.target);let n=null,o=null;if("cell"===s){let t=e.target;for(;"TD"!==t.tagName;)t=t.parentNode;o=t.getAttribute("data-y"),n=t.getAttribute("data-x"),(!libraryBase.jspreadsheet.current.selectedCell||n<parseInt(libraryBase.jspreadsheet.current.selectedCell[0])||n>parseInt(libraryBase.jspreadsheet.current.selectedCell[2])||o<parseInt(libraryBase.jspreadsheet.current.selectedCell[1])||o>parseInt(libraryBase.jspreadsheet.current.selectedCell[3]))&&selection.AH.call(libraryBase.jspreadsheet.current,n,o,n,o,e)}else if("row"===s||"header"===s)"row"===s?o=e.target.getAttribute("data-y"):n=e.target.getAttribute("data-x"),(!libraryBase.jspreadsheet.current.selectedCell||n<parseInt(libraryBase.jspreadsheet.current.selectedCell[0])||n>parseInt(libraryBase.jspreadsheet.current.selectedCell[2])||o<parseInt(libraryBase.jspreadsheet.current.selectedCell[1])||o>parseInt(libraryBase.jspreadsheet.current.selectedCell[3]))&&selection.AH.call(libraryBase.jspreadsheet.current,n,o,n,o,e);else if("nested"===s){const t=e.target.getAttribute("data-column").split(",");n=getElementIndex(e.target)-1,o=getElementIndex(e.target.parentElement),libraryBase.jspreadsheet.current.selectedCell&&t[0]==parseInt(libraryBase.jspreadsheet.current.selectedCell[0])&&t[t.length-1]==parseInt(libraryBase.jspreadsheet.current.selectedCell[2])&&null==libraryBase.jspreadsheet.current.selectedCell[1]&&null==libraryBase.jspreadsheet.current.selectedCell[3]||selection.AH.call(libraryBase.jspreadsheet.current,t[0],null,t[t.length-1],null,e)}else"select-all"===s?selection.Ub.call(libraryBase.jspreadsheet.current):"tabs"===s?n=getElementIndex(e.target):"footer"===s&&(n=getElementIndex(e.target)-1,o=getElementIndex(e.target.parentElement));let r=defaultContextMenu(libraryBase.jspreadsheet.current,parseInt(n),parseInt(o),s);if("function"==typeof t.config.contextMenu){const l=t.config.contextMenu(libraryBase.jspreadsheet.current,n,o,e,r,s,n,o);if(l)r=l;else if(!1===l)return}"object"==typeof t.plugins&&Object.entries(t.plugins).forEach((function([,t]){if("function"==typeof t.contextMenu){const l=t.contextMenu(libraryBase.jspreadsheet.current,null!==n?parseInt(n):null,null!==o?parseInt(o):null,e,r,s,null!==n?parseInt(n):null,null!==o?parseInt(o):null);l&&(r=l)}})),t.contextMenu.contextmenu.open(e,r),e.preventDefault()}}},touchStartControls=function(e){const t=getElement(e.target);if(t[0]?libraryBase.jspreadsheet.current!=t[0].jssWorksheet&&(libraryBase.jspreadsheet.current&&libraryBase.jspreadsheet.current.resetSelection(),libraryBase.jspreadsheet.current=t[0].jssWorksheet):libraryBase.jspreadsheet.current&&(libraryBase.jspreadsheet.current.resetSelection(),libraryBase.jspreadsheet.current=null),libraryBase.jspreadsheet.current&&!libraryBase.jspreadsheet.current.edition){const t=e.target.getAttribute("data-x"),s=e.target.getAttribute("data-y");t&&s&&(selection.AH.call(libraryBase.jspreadsheet.current,t,s,void 0,void 0,e),libraryBase.jspreadsheet.timeControl=setTimeout((function(){"color"==libraryBase.jspreadsheet.current.options.columns[t].type?libraryBase.jspreadsheet.tmpElement=null:libraryBase.jspreadsheet.tmpElement=e.target,openEditor.call(libraryBase.jspreadsheet.current,e.target,!1,e)}),500))}},touchEndControls=function(e){libraryBase.jspreadsheet.timeControl&&(clearTimeout(libraryBase.jspreadsheet.timeControl),libraryBase.jspreadsheet.timeControl=null,libraryBase.jspreadsheet.tmpElement&&"INPUT"==libraryBase.jspreadsheet.tmpElement.children[0].tagName&&libraryBase.jspreadsheet.tmpElement.children[0].focus(),libraryBase.jspreadsheet.tmpElement=null)},cutControls=function(e){libraryBase.jspreadsheet.current&&(libraryBase.jspreadsheet.current.edition||(copy.call(libraryBase.jspreadsheet.current,!0,void 0,void 0,void 0,void 0,!0),0!=libraryBase.jspreadsheet.current.options.editable&&libraryBase.jspreadsheet.current.setValue(libraryBase.jspreadsheet.current.highlighted.map((function(e){return e.element})),"")))},copyControls=function(e){libraryBase.jspreadsheet.current&&copyControls.enabled&&(libraryBase.jspreadsheet.current.edition||copy.call(libraryBase.jspreadsheet.current,!0))},isMac=function(){return navigator.platform.toUpperCase().indexOf("MAC")>=0},isCtrl=function(e){return isMac()?e.metaKey:e.ctrlKey},keyDownControls=function(e){if(libraryBase.jspreadsheet.current){if(libraryBase.jspreadsheet.current.edition)if(27==e.which)libraryBase.jspreadsheet.current.edition&&closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!1),e.preventDefault();else if(13==e.which)if(libraryBase.jspreadsheet.current.options.columns&&libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]]&&"calendar"==libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]].type)closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0);else if(libraryBase.jspreadsheet.current.options.columns&&libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]]&&"dropdown"==libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]].type);else if((1==libraryBase.jspreadsheet.current.options.wordWrap||libraryBase.jspreadsheet.current.options.columns&&libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]]&&1==libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]].wordWrap||libraryBase.jspreadsheet.current.options.data[libraryBase.jspreadsheet.current.edition[3]][libraryBase.jspreadsheet.current.edition[2]]&&libraryBase.jspreadsheet.current.options.data[libraryBase.jspreadsheet.current.edition[3]][libraryBase.jspreadsheet.current.edition[2]].length>200)&&e.altKey){const e=libraryBase.jspreadsheet.current.edition[0].children[0];let t=libraryBase.jspreadsheet.current.edition[0].children[0].value;const s=e.selectionStart;t=t.slice(0,s)+"\n"+t.slice(s),e.value=t,e.focus(),e.selectionStart=s+1,e.selectionEnd=s+1}else libraryBase.jspreadsheet.current.edition[0].children[0].blur();else 9==e.which&&(libraryBase.jspreadsheet.current.options.columns&&libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]]&&["calendar","html"].includes(libraryBase.jspreadsheet.current.options.columns[libraryBase.jspreadsheet.current.edition[2]].type)?closeEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.edition[0],!0):libraryBase.jspreadsheet.current.edition[0].children[0].blur());if(!libraryBase.jspreadsheet.current.edition&&libraryBase.jspreadsheet.current.selectedCell)if(37==e.which)left.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(39==e.which)right.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(38==e.which)up.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(40==e.which)down.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(36==e.which)first.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(35==e.which)last.call(libraryBase.jspreadsheet.current,e.shiftKey,e.ctrlKey),e.preventDefault();else if(46==e.which||8==e.which)0!=libraryBase.jspreadsheet.current.options.editable&&(null!=libraryBase.jspreadsheet.current.selectedRow?0!=libraryBase.jspreadsheet.current.options.allowDeleteRow&&confirm(jSuites.translate("Are you sure to delete the selected rows?"))&&libraryBase.jspreadsheet.current.deleteRow():libraryBase.jspreadsheet.current.selectedHeader?0!=libraryBase.jspreadsheet.current.options.allowDeleteColumn&&confirm(jSuites.translate("Are you sure to delete the selected columns?"))&&libraryBase.jspreadsheet.current.deleteColumn():libraryBase.jspreadsheet.current.setValue(libraryBase.jspreadsheet.current.highlighted.map((function(e){return e.element})),""));else if(13==e.which)e.shiftKey?up.call(libraryBase.jspreadsheet.current):(0!=libraryBase.jspreadsheet.current.options.allowInsertRow&&0!=libraryBase.jspreadsheet.current.options.allowManualInsertRow&&libraryBase.jspreadsheet.current.selectedCell[1]==libraryBase.jspreadsheet.current.options.data.length-1&&libraryBase.jspreadsheet.current.insertRow(),down.call(libraryBase.jspreadsheet.current)),e.preventDefault();else if(9==e.which)e.shiftKey?left.call(libraryBase.jspreadsheet.current):(0!=libraryBase.jspreadsheet.current.options.allowInsertColumn&&0!=libraryBase.jspreadsheet.current.options.allowManualInsertColumn&&libraryBase.jspreadsheet.current.selectedCell[0]==libraryBase.jspreadsheet.current.options.data[0].length-1&&libraryBase.jspreadsheet.current.insertColumn(),right.call(libraryBase.jspreadsheet.current)),e.preventDefault();else if(!e.ctrlKey&&!e.metaKey||e.shiftKey){if(libraryBase.jspreadsheet.current.selectedCell&&0!=libraryBase.jspreadsheet.current.options.editable){const t=libraryBase.jspreadsheet.current.selectedCell[1],s=libraryBase.jspreadsheet.current.selectedCell[0];32==e.keyCode?(e.preventDefault(),"checkbox"==libraryBase.jspreadsheet.current.options.columns[s].type||"radio"==libraryBase.jspreadsheet.current.options.columns[s].type?setCheckRadioValue.call(libraryBase.jspreadsheet.current):openEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.records[t][s].element,!0,e)):113==e.keyCode?openEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.records[t][s].element,!1,e):1!==e.key.length&&"Process"!==e.key||e.altKey||isCtrl(e)||(openEditor.call(libraryBase.jspreadsheet.current,libraryBase.jspreadsheet.current.records[t][s].element,!0,e),libraryBase.jspreadsheet.current.options.columns&&libraryBase.jspreadsheet.current.options.columns[s]&&"calendar"==libraryBase.jspreadsheet.current.options.columns[s].type&&e.preventDefault())}}else 65==e.which?(selection.Ub.call(libraryBase.jspreadsheet.current),e.preventDefault()):83==e.which?(libraryBase.jspreadsheet.current.download(),e.preventDefault()):89==e.which?(libraryBase.jspreadsheet.current.redo(),e.preventDefault()):90==e.which?(libraryBase.jspreadsheet.current.undo(),e.preventDefault()):67==e.which?(copy.call(libraryBase.jspreadsheet.current,!0),e.preventDefault()):88==e.which?(0!=libraryBase.jspreadsheet.current.options.editable?cutControls():copyControls(),e.preventDefault()):86==e.which&&pasteControls();else e.target.classList.contains("jss_search")&&(libraryBase.jspreadsheet.timeControl&&clearTimeout(libraryBase.jspreadsheet.timeControl),libraryBase.jspreadsheet.timeControl=setTimeout((function(){libraryBase.jspreadsheet.current.search(e.target.value)}),200))}},wheelControls=function(e){const t=this;1==t.options.lazyLoading&&null==libraryBase.jspreadsheet.timeControlLoading&&(libraryBase.jspreadsheet.timeControlLoading=setTimeout((function(){t.content.scrollTop+t.content.clientHeight>=t.content.scrollHeight-10?lazyLoading.p6.call(t)&&(t.content.scrollTop+t.content.clientHeight>t.content.scrollHeight-10&&(t.content.scrollTop=t.content.scrollTop-t.content.clientHeight),selection.Aq.call(t)):t.content.scrollTop<=t.content.clientHeight&&lazyLoading.G_.call(t)&&(t.content.scrollTop<10&&(t.content.scrollTop=t.content.scrollTop+t.content.clientHeight),selection.Aq.call(t)),libraryBase.jspreadsheet.timeControlLoading=null}),100))};let scrollLeft=0;const updateFreezePosition=function(){const e=this;scrollLeft=e.content.scrollLeft;let t=0;if(scrollLeft>50)for(let s=0;s<e.options.freezeColumns;s++){if(s>0&&(!e.options.columns||!e.options.columns[s-1]||"hidden"!==e.options.columns[s-1].type)){let n;n=e.options.columns&&e.options.columns[s-1]&&void 0!==e.options.columns[s-1].width?parseInt(e.options.columns[s-1].width):void 0!==e.options.defaultColWidth?parseInt(e.options.defaultColWidth):100,t+=parseInt(n)}e.headers[s].classList.add("jss_freezed"),e.headers[s].style.left=t+"px";for(let t=0;t<e.rows.length;t++)if(e.rows[t]&&e.records[t][s]){const n=scrollLeft+(s>0?e.records[t][s-1].element.style.width:0)-51+"px";e.records[t][s].element.classList.add("jss_freezed"),e.records[t][s].element.style.left=n}}else for(let t=0;t<e.options.freezeColumns;t++){e.headers[t].classList.remove("jss_freezed"),e.headers[t].style.left="";for(let s=0;s<e.rows.length;s++)e.records[s][t]&&(e.records[s][t].element.classList.remove("jss_freezed"),e.records[s][t].element.style.left="")}selection.Aq.call(e)},scrollControls=function(e){const t=this;wheelControls.call(t),t.options.freezeColumns>0&&t.content.scrollLeft!=scrollLeft&&updateFreezePosition.call(t),1!=t.options.lazyLoading&&1!=t.options.tableOverflow||t.edition&&"jdropdown"!=e.target.className.substr(0,9)&&closeEditor.call(t,t.edition[0],!0)},setEvents=function(e){destroyEvents(e),e.addEventListener("mouseup",mouseUpControls),e.addEventListener("mousedown",mouseDownControls),e.addEventListener("mousemove",mouseMoveControls),e.addEventListener("mouseover",mouseOverControls),e.addEventListener("dblclick",doubleClickControls),e.addEventListener("paste",pasteControls),e.addEventListener("contextmenu",contextMenuControls),e.addEventListener("touchstart",touchStartControls),e.addEventListener("touchend",touchEndControls),e.addEventListener("touchcancel",touchEndControls),e.addEventListener("touchmove",touchEndControls),document.addEventListener("keydown",keyDownControls)},destroyEvents=function(e){e.removeEventListener("mouseup",mouseUpControls),e.removeEventListener("mousedown",mouseDownControls),e.removeEventListener("mousemove",mouseMoveControls),e.removeEventListener("mouseover",mouseOverControls),e.removeEventListener("dblclick",doubleClickControls),e.removeEventListener("paste",pasteControls),e.removeEventListener("contextmenu",contextMenuControls),e.removeEventListener("touchstart",touchStartControls),e.removeEventListener("touchend",touchEndControls),e.removeEventListener("touchcancel",touchEndControls),document.removeEventListener("keydown",keyDownControls)};var toolbar=__webpack_require__(392),pagination=__webpack_require__(167);const setData=function(e){const t=this;if(e&&(t.options.data=e),t.options.data||(t.options.data=[]),t.options.data&&t.options.data[0]&&!Array.isArray(t.options.data[0])){e=[];for(let s=0;s<t.options.data.length;s++){const n=[];for(let e=0;e<t.options.columns.length;e++)n[e]=t.options.data[s][t.options.columns[e].name];e.push(n)}t.options.data=e}let s=0,n=0;const o=t.options.columns&&t.options.columns.length||0,r=t.options.data.length,l=t.options.minDimensions[0],i=t.options.minDimensions[1],a=l>o?l:o,c=i>r?i:r;for(s=0;s<c;s++)for(n=0;n<a;n++)null==t.options.data[s]&&(t.options.data[s]=[]),null==t.options.data[s][n]&&(t.options.data[s][n]="");let d,u;for(t.rows=[],t.results=null,t.records=[],t.history=[],t.historyIndex=-1,t.tbody.innerHTML="",1==t.options.lazyLoading?(d=0,u=t.options.data.length<100?t.options.data.length:100,t.options.pagination&&(t.options.pagination=!1,console.error("Jspreadsheet: Pagination will be disable due the lazyLoading"))):t.options.pagination?(t.pageNumber||(t.pageNumber=0),t.options.pagination,d=t.options.pagination*t.pageNumber,u=t.options.pagination*t.pageNumber+t.options.pagination,t.options.data.length<u&&(u=t.options.data.length)):(d=0,u=t.options.data.length),s=0;s<t.options.data.length;s++){const e=createRow.call(t,s,t.options.data[s]);s>=d&&s<u&&t.tbody.appendChild(e.element)}if(1==t.options.lazyLoading||t.options.pagination&&pagination.IV.call(t),t.options.mergeCells){const e=Object.keys(t.options.mergeCells);for(let s=0;s<e.length;s++){const n=t.options.mergeCells[e[s]];merges.FU.call(t,e[s],n[0],n[1],1)}}internal.am.call(t)},getValue=function(e,t){const s=this;let n,o;if("string"!=typeof e)return null;n=(e=(0,internalHelpers.vu)(e,!0))[0],o=e[1];let r=null;return null!=n&&null!=o&&(s.records[o]&&s.records[o][n]&&t?r=s.records[o][n].element.innerHTML:s.options.data[o]&&"undefined"!=s.options.data[o][n]&&(r=s.options.data[o][n])),r},getValueFromCoords=function(e,t,s){const n=this;let o=null;return null!=e&&null!=t&&(n.records[t]&&n.records[t][e]&&s?o=n.records[t][e].element.innerHTML:n.options.data[t]&&"undefined"!=n.options.data[t][e]&&(o=n.options.data[t][e])),o},setValue=function(e,t,s){const n=this,o=[];if("string"==typeof e){const r=(0,internalHelpers.vu)(e,!0),l=r[0],i=r[1];o.push(internal.k9.call(n,l,i,t,s)),internal.xF.call(n,l,i,o)}else{let r=null,l=null;if(e&&e.getAttribute&&(r=e.getAttribute("data-x"),l=e.getAttribute("data-y")),null!=r&&null!=l)o.push(internal.k9.call(n,r,l,t,s)),internal.xF.call(n,r,l,o);else{const r=Object.keys(e);if(r.length>0)for(let l=0;l<r.length;l++){let r,i;if("string"==typeof e[l]){const t=(0,internalHelpers.vu)(e[l],!0);r=t[0],i=t[1]}else null!=e[l].x&&null!=e[l].y?(r=e[l].x,i=e[l].y,null!=e[l].value&&(t=e[l].value)):(r=e[l].getAttribute("data-x"),i=e[l].getAttribute("data-y"));null!=r&&null!=i&&(o.push(internal.k9.call(n,r,i,t,s)),internal.xF.call(n,r,i,o))}}}utils_history.Dh.call(n,{action:"setValue",records:o,selection:n.selectedCell}),internal.am.call(n);const r=o.map((function(e){return{x:e.x,y:e.y,value:e.value,oldValue:e.oldValue}}));dispatch.A.call(n,"onafterchanges",n,r)},setValueFromCoords=function(e,t,s,n){const o=this,r=[];r.push(internal.k9.call(o,e,t,s,n)),internal.xF.call(o,e,t,r),utils_history.Dh.call(o,{action:"setValue",records:r,selection:o.selectedCell}),internal.am.call(o);const l=r.map((function(e){return{x:e.x,y:e.y,value:e.value,oldValue:e.oldValue}}));dispatch.A.call(o,"onafterchanges",o,l)},getData=function(e,t,s,n){const o=this,r=[];let l=0,i=0;const a=Math.max(...o.options.data.map((function(e){return e.length}))),c=o.options.data.length;for(let s=0;s<c;s++){l=0;for(let n=0;n<a;n++)e&&!o.records[s][n].element.classList.contains("highlight")||(r[i]||(r[i]=[]),r[i][l]=t?o.records[s][n].element.innerHTML:o.options.data[s][n],l++);l>0&&i++}return s?r.map((function(e){return e.join(s)})).join("\r\n")+"\r\n":n?r.map((function(e){const t={};return e.forEach((function(e,s){t[s]=e})),t})):r},getDataFromRange=function(e,t){const s=this,n=(0,helpers.getCoordsFromRange)(e),o=[];for(let e=n[1];e<=n[3];e++){o.push([]);for(let r=n[0];r<=n[2];r++)t?o[o.length-1].push(s.records[e][r].element.innerHTML):o[o.length-1].push(s.options.data[e][r])}return o},search=function(e){const t=this;if(t.options.filters&&filter.dr.call(t),t.resetSelection(),t.pageNumber=0,t.results=[],e){t.searchInput.value!==e&&(t.searchInput.value=e);const s=function(e,s,n){for(let o=0;o<e.length;o++)if((""+e[o]).toLowerCase().search(s)>=0||(""+t.records[n][o].element.innerHTML).toLowerCase().search(s)>=0)return!0;return!1},n=function(e){-1==t.results.indexOf(e)&&t.results.push(e)};let o=e.replace(/[-[\]{}()*+?.,\\^$|#\s]/g,"\\$&");o=new RegExp(o,"i"),t.options.data.forEach((function(e,r){if(s(e,o,r)){const e=merges.D0.call(t,r);if(e.length)for(let s=0;s<e.length;s++){const o=(0,internalHelpers.vu)(e[s],!0);for(let r=0;r<t.options.mergeCells[e[s]][1];r++)n(o[1]+r)}else n(r)}}))}else t.results=null;internal.hG.call(t)},resetSearch=function(){const e=this;e.searchInput.value="",e.search(""),e.results=null},getHeader=function(e){return this.headers[e].textContent},getHeaders=function(e){const t=this,s=[];for(let e=0;e<t.headers.length;e++)s.push(t.getHeader(e));return e?s:s.join(t.options.csvDelimiter)},setHeader=function(e,t){const s=this;if(s.headers[e]){const n=s.headers[e].textContent,o=s.options.columns&&s.options.columns[e]&&s.options.columns[e].title||"";t||(t=(0,helpers.getColumnName)(e)),s.headers[e].textContent=t,s.headers[e].setAttribute("title",t),s.options.columns||(s.options.columns=[]),s.options.columns[e]||(s.options.columns[e]={}),s.options.columns[e].title=t,utils_history.Dh.call(s,{action:"setHeader",column:e,oldValue:n,newValue:t}),dispatch.A.call(s,"onchangeheader",s,parseInt(e),t,o)}},getStyle=function(e,t){const s=this;if(e)return e=(0,internalHelpers.vu)(e,!0),t?s.records[e[1]][e[0]].element.style[t]:s.records[e[1]][e[0]].element.getAttribute("style");{const e={},n=s.options.data[0].length,o=s.options.data.length;for(let r=0;r<o;r++)for(let o=0;o<n;o++){const n=t?s.records[r][o].element.style[t]:s.records[r][o].element.getAttribute("style");n&&(e[(0,internalHelpers.t3)([o,r])]=n)}return e}},setStyle=function(e,t,s,n,o){const r=this,l={},i={},a=function(e,t,s){const o=(0,internalHelpers.vu)(e,!0);if(r.records[o[1]]&&r.records[o[1]][o[0]]&&(0==r.records[o[1]][o[0]].element.classList.contains("readonly")||n)){const a=r.records[o[1]][o[0]].element.style[t];a!=s||n?r.records[o[1]][o[0]].element.style[t]=s:(s="",r.records[o[1]][o[0]].element.style[t]=""),i[e]||(i[e]=[]),l[e]||(l[e]=[]),i[e].push([t+":"+a]),l[e].push([t+":"+s])}};if(t&&s)"string"==typeof e&&a(e,t,s);else{const t=Object.keys(e);for(let s=0;s<t.length;s++){let n=e[t[s]];"string"==typeof n&&(n=n.split(";"));for(let e=0;e<n.length;e++)"string"==typeof n[e]&&(n[e]=n[e].split(":")),n[e][0].trim()&&a(t[s],n[e][0].trim(),n[e][1])}}let c=Object.keys(i);for(let e=0;e<c.length;e++)i[c[e]]=i[c[e]].join(";");c=Object.keys(l);for(let e=0;e<c.length;e++)l[c[e]]=l[c[e]].join(";");o||utils_history.Dh.call(r,{action:"setStyle",oldValue:i,newValue:l}),dispatch.A.call(r,"onchangestyle",r,l)},resetStyle=function(e,t){const s=this,n=Object.keys(e);for(let e=0;e<n.length;e++){const t=(0,internalHelpers.vu)(n[e],!0);s.records[t[1]]&&s.records[t[1]][t[0]]&&s.records[t[1]][t[0]].element.setAttribute("style","")}s.setStyle(e,null,null,null,t)},download=function(e,t){const s=this;if(0==s.parent.config.allowExport)console.error("Export not allowed");else{let n="";n+=copy.call(s,!1,s.options.csvDelimiter,!0,e,!0,void 0,t);const o=new Blob(["\ufeff"+n],{type:"text/csv;charset=utf-8;"});if(window.navigator&&window.navigator.msSaveOrOpenBlob)window.navigator.msSaveOrOpenBlob(o,(s.options.csvFileName||s.options.worksheetName)+".csv");else{const e=document.createElement("a");e.setAttribute("target","_top");const t=URL.createObjectURL(o);e.href=t,e.setAttribute("download",(s.options.csvFileName||s.options.worksheetName)+".csv"),document.body.appendChild(e),e.click(),e.parentNode.removeChild(e)}}},getComments=function(e){const t=this;if(e)return"string"!=typeof e?getComments.call(t):(e=(0,internalHelpers.vu)(e,!0),t.records[e[1]][e[0]].element.getAttribute("title")||"");{const e={};for(let s=0;s<t.options.data.length;s++)for(let n=0;n<t.options.columns.length;n++){const o=t.records[s][n].element.getAttribute("title");o&&(e[(0,internalHelpers.t3)([n,s])]=o)}return e}},setComments=function(e,t){const s=this;let n;n="string"==typeof e?{[e]:t}:e;const o={};Object.entries(n).forEach((function([e,t]){const n=(0,helpers.getCoordsFromCellName)(e);o[e]=s.records[n[1]][n[0]].element.getAttribute("title"),s.records[n[1]][n[0]].element.setAttribute("title",t||""),t?(s.records[n[1]][n[0]].element.classList.add("jss_comments"),s.options.comments||(s.options.comments={}),s.options.comments[e]=t):(s.records[n[1]][n[0]].element.classList.remove("jss_comments"),s.options.comments&&s.options.comments[e]&&delete s.options.comments[e])})),utils_history.Dh.call(s,{action:"setComments",newValue:n,oldValue:o}),dispatch.A.call(s,"oncomments",s,n,o)};var orderBy=__webpack_require__(94);const getWorksheetConfig=function(){return this.options},getSpreadsheetConfig=function(){return this.config},setConfig=function(e,t){const s=this,n=Object.keys(e);let o;s.parent?o=s.parent:(t=!0,o=s),n.forEach((function(n){t?(o.config[n]=e[n],"toolbar"===n&&(!0===e[n]?o.showToolbar():!1===e[n]&&o.hideToolbar())):s.options[n]=e[n]}))};var meta=__webpack_require__(654);const setReadOnly=function(e,t){const s=this;let n;if("string"==typeof e){const t=(0,helpers.getCoordsFromCellName)(e);n=s.records[t[1]][t[0]]}else{const t=parseInt(e.getAttribute("data-x")),o=parseInt(e.getAttribute("data-y"));n=s.records[o][t]}t?n.element.classList.add("readonly"):n.element.classList.remove("readonly")},isReadOnly=function(e,t){if("string"==typeof e&&void 0===t){const s=(0,helpers.getCoordsFromCellName)(e);[e,t]=s}return this.records[t][e].element.classList.contains("readonly")},setWorksheetFunctions=function(e){for(let t=0;t<worksheetPublicMethodsLength;t++){const[s,n]=worksheetPublicMethods[t];e[s]=n.bind(e)}},createTable=function(){let e=this;setWorksheetFunctions(e),e.table=document.createElement("table"),e.thead=document.createElement("thead"),e.tbody=document.createElement("tbody"),e.headers=[],e.cols=[],e.content=document.createElement("div"),e.content.classList.add("jss_content"),e.content.onscroll=function(t){scrollControls.call(e,t)},e.content.onwheel=function(t){wheelControls.call(e,t)};const t=document.createElement("div"),s=document.createElement("label");s.innerHTML=jSuites.translate("Search")+": ",t.appendChild(s),e.searchInput=document.createElement("input"),e.searchInput.classList.add("jss_search"),s.appendChild(e.searchInput),e.searchInput.onfocus=function(){e.resetSelection()};const n=document.createElement("div");if(e.options.pagination>0&&e.options.paginationOptions&&e.options.paginationOptions.length>0){e.paginationDropdown=document.createElement("select"),e.paginationDropdown.classList.add("jss_pagination_dropdown"),e.paginationDropdown.onchange=function(){e.options.pagination=parseInt(this.value),e.page(0)};for(let t=0;t<e.options.paginationOptions.length;t++){const s=document.createElement("option");s.value=e.options.paginationOptions[t],s.innerHTML=e.options.paginationOptions[t],e.paginationDropdown.appendChild(s)}e.paginationDropdown.value=e.options.pagination,n.appendChild(document.createTextNode(jSuites.translate("Show "))),n.appendChild(e.paginationDropdown),n.appendChild(document.createTextNode(jSuites.translate("entries")))}const o=document.createElement("div");o.classList.add("jss_filter"),o.appendChild(n),o.appendChild(t),e.colgroupContainer=document.createElement("colgroup");let r=document.createElement("col");if(r.setAttribute("width","50"),e.colgroupContainer.appendChild(r),e.options.nestedHeaders&&e.options.nestedHeaders.length>0&&e.options.nestedHeaders[0]&&e.options.nestedHeaders[0][0])for(let t=0;t<e.options.nestedHeaders.length;t++)e.thead.appendChild(internal.ju.call(e,e.options.nestedHeaders[t]));e.headerContainer=document.createElement("tr"),r=document.createElement("td"),r.classList.add("jss_selectall"),e.headerContainer.appendChild(r);const l=getNumberOfColumns.call(e);for(let t=0;t<l;t++)createCellHeader.call(e,t),e.headerContainer.appendChild(e.headers[t]),e.colgroupContainer.appendChild(e.cols[t].colElement);if(e.thead.appendChild(e.headerContainer),1==e.options.filters){e.filter=document.createElement("tr");const t=document.createElement("td");e.filter.appendChild(t);for(let t=0;t<e.options.columns.length;t++){const s=document.createElement("td");s.innerHTML="&nbsp;",s.setAttribute("data-x",t),s.className="jss_column_filter","hidden"==e.options.columns[t].type&&(s.style.display="none"),e.filter.appendChild(s)}e.thead.appendChild(e.filter)}e.table=document.createElement("table"),e.table.classList.add("jss_worksheet"),e.table.setAttribute("cellpadding","0"),e.table.setAttribute("cellspacing","0"),e.table.setAttribute("unselectable","yes"),e.table.appendChild(e.colgroupContainer),e.table.appendChild(e.thead),e.table.appendChild(e.tbody),e.options.textOverflow||e.table.classList.add("jss_overflow"),e.corner=document.createElement("div"),e.corner.className="jss_corner",e.corner.setAttribute("unselectable","on"),e.corner.setAttribute("onselectstart","return false"),0==e.options.selectionCopy&&(e.corner.style.display="none"),e.textarea=document.createElement("textarea"),e.textarea.className="jss_textarea",e.textarea.id="jss_textarea",e.textarea.tabIndex="-1",e.textarea.ariaHidden="true";const i=document.createElement("a");i.setAttribute("href","https://bossanova.uk/jspreadsheet/"),e.ads=document.createElement("div"),e.ads.className="jss_about";const a=document.createElement("span");a.innerHTML="Jspreadsheet CE",i.appendChild(a),e.ads.appendChild(i),document.createElement("div").classList.add("jss_table"),e.pagination=document.createElement("div"),e.pagination.classList.add("jss_pagination");const c=document.createElement("div"),d=document.createElement("div");if(e.pagination.appendChild(c),e.pagination.appendChild(d),e.options.pagination||(e.pagination.style.display="none"),1==e.options.search&&e.element.appendChild(o),e.content.appendChild(e.table),e.content.appendChild(e.corner),e.content.appendChild(e.textarea),e.element.appendChild(e.content),e.element.appendChild(e.pagination),e.element.appendChild(e.ads),e.element.classList.add("jss_container"),e.element.jssWorksheet=e,e.element.jspreadsheet=e,1==e.options.tableOverflow&&(e.options.tableHeight&&(e.content.style["overflow-y"]="auto",e.content.style["box-shadow"]="rgb(221 221 221) 2px 2px 5px 0.1px",e.content.style.maxHeight="string"==typeof e.options.tableHeight?e.options.tableHeight:e.options.tableHeight+"px"),e.options.tableWidth&&(e.content.style["overflow-x"]="auto",e.content.style.width="string"==typeof e.options.tableWidth?e.options.tableWidth:e.options.tableWidth+"px")),1!=e.options.tableOverflow&&e.parent.config.toolbar&&e.element.classList.add("with-toolbar"),0!=e.options.columnDrag&&e.thead.classList.add("draggable"),0!=e.options.columnResize&&e.thead.classList.add("resizable"),0!=e.options.rowDrag&&e.tbody.classList.add("draggable"),0!=e.options.rowResize&&e.tbody.classList.add("resizable"),e.setData.call(e),e.options.style&&(e.setStyle(e.options.style,null,null,1,1),delete e.options.style),Object.defineProperty(e.options,"style",{enumerable:!0,configurable:!0,get(){return e.getStyle()}}),e.options.comments&&e.setComments(e.options.comments),e.options.classes){const t=Object.keys(e.options.classes);for(let s=0;s<t.length;s++){const n=(0,internalHelpers.vu)(t[s],!0);e.records[n[1]][n[0]].element.classList.add(e.options.classes[t[s]])}}},prepareTable=function(){const e=this;1==e.options.lazyLoading&&1!=e.options.tableOverflow&&1!=e.parent.config.fullscreen&&(console.error("Jspreadsheet: The lazyloading only works when tableOverflow = yes or fullscreen = yes"),e.options.lazyLoading=!1),e.options.columns||(e.options.columns=[]);let t,s=e.options.columns.length;if(e.options.data&&void 0!==e.options.data[0])if(Array.isArray(e.options.data[0])){const t=e.options.data[0].length;t>s&&(s=t)}else t=Object.keys(e.options.data[0]),t.length>s&&(s=t.length);e.options.minDimensions||(e.options.minDimensions=[0,0]),e.options.minDimensions[0]>s&&(s=e.options.minDimensions[0]);const n=[];for(let o=0;o<s;o++)e.options.columns[o]||(e.options.columns[o]={}),!e.options.columns[o].name&&t&&t[o]&&(e.options.columns[o].name=t[o]),"dropdown"==e.options.columns[o].type&&e.options.columns[o].url&&n.push({url:e.options.columns[o].url,index:o,method:"GET",dataType:"json",success:function(t){e.options.columns[this.index].source||(e.options.columns[this.index].source=[]);for(let s=0;s<t.length;s++)e.options.columns[this.index].source.push(t[s])}});n.length?jSuites.ajax(n,(function(){createTable.call(e)})):createTable.call(e)},getNextDefaultWorksheetName=function(e){const t=/^Sheet(\d+)$/;let s=0;return e.worksheets.forEach((function(e){const n=t.exec(e.options.worksheetName);n&&(s=Math.max(s,parseInt(n[1])))})),"Sheet"+(s+1)},buildWorksheet=async function(){const e=this,t=(e.element,e.parent);"object"==typeof t.plugins&&Object.entries(t.plugins).forEach((function([,t]){"function"==typeof t.beforeinit&&t.beforeinit(e)})),libraryBase.jspreadsheet.current=e;const s=[];if(e.options.csv){const t=new Promise((t=>{jSuites.ajax({url:e.options.csv,method:"GET",dataType:"text",success:function(s){const n=(0,helpers.parseCSV)(s,e.options.csvDelimiter);if(1==e.options.csvHeaders&&n.length>0){const t=n.shift();if(t.length>0){e.options.columns||(e.options.columns=[]);for(let s=0;s<t.length;s++)e.options.columns[s]||(e.options.columns[s]={}),void 0===e.options.columns[s].title&&(e.options.columns[s].title=t[s])}}e.options.data=n,prepareTable.call(e),t()}})}));s.push(t)}else if(e.options.url){const t=new Promise((t=>{jSuites.ajax({url:e.options.url,method:"GET",dataType:"json",success:function(s){e.options.data=s.data?s.data:s,prepareTable.call(e),t()}})}));s.push(t)}else prepareTable.call(e);await Promise.all(s),"object"==typeof t.plugins&&Object.entries(t.plugins).forEach((function([,t]){"function"==typeof t.init&&t.init(e)}))},createWorksheetObj=function(e){const t=this.parent;e.worksheetName||(e.worksheetName=getNextDefaultWorksheetName(this.parent));const s={parent:t,options:e,filters:[],formula:[],history:[],selection:[],historyIndex:-1};return t.config.worksheets.push(s.options),t.worksheets.push(s),s},createWorksheet=function(e){const t=this.parent;t.creationThroughJss=!0,createWorksheetObj.call(this,e),t.element.tabs.create(e.worksheetName)},openWorksheet=function(e){this.parent.element.tabs.open(e)},deleteWorksheet=function(e){const t=this;t.parent.element.tabs.remove(e);const s=t.parent.worksheets.splice(e,1)[0];dispatch.A.call(t.parent,"ondeleteworksheet",s,e)},worksheetPublicMethods=[["selectAll",selection.Ub],["updateSelectionFromCoords",function(e,t,s,n){return selection.AH.call(this,e,t,s,n)}],["resetSelection",function(){return selection.gE.call(this)}],["getSelection",selection.Lo],["getSelected",selection.ef],["getSelectedColumns",selection.Jg],["getSelectedRows",selection.R5],["getData",getData],["setData",setData],["getValue",getValue],["getValueFromCoords",getValueFromCoords],["setValue",setValue],["setValueFromCoords",setValueFromCoords],["getWidth",getWidth],["setWidth",function(e,t){return setWidth.call(this,e,t)}],["insertRow",insertRow],["moveRow",function(e,t){return moveRow.call(this,e,t)}],["deleteRow",deleteRow],["hideRow",hideRow],["showRow",showRow],["getRowData",getRowData],["setRowData",setRowData],["getHeight",getHeight],["setHeight",function(e,t){return setHeight.call(this,e,t)}],["getMerge",merges.fd],["setMerge",function(e,t,s){return merges.FU.call(this,e,t,s)}],["destroyMerge",function(){return merges.VP.call(this)}],["removeMerge",function(e,t){return merges.Zp.call(this,e,t)}],["search",search],["resetSearch",resetSearch],["getHeader",getHeader],["getHeaders",getHeaders],["setHeader",setHeader],["getStyle",getStyle],["setStyle",function(e,t,s,n){return setStyle.call(this,e,t,s,n)}],["resetStyle",resetStyle],["insertColumn",insertColumn],["moveColumn",moveColumn],["deleteColumn",deleteColumn],["getColumnData",getColumnData],["setColumnData",setColumnData],["whichPage",pagination.ho],["page",pagination.MY],["download",download],["getComments",getComments],["setComments",setComments],["orderBy",orderBy.My],["undo",utils_history.tN],["redo",utils_history.ZS],["getCell",internal.tT],["getCellFromCoords",internal.Xr],["getLabel",internal.p9],["getConfig",getWorksheetConfig],["setConfig",setConfig],["getMeta",function(e){return meta.IQ.call(this,e)}],["setMeta",meta.iZ],["showColumn",showColumn],["hideColumn",hideColumn],["showIndex",internal.C6],["hideIndex",internal.TI],["getWorksheetActive",internal.$O],["openEditor",openEditor],["closeEditor",closeEditor],["createWorksheet",createWorksheet],["openWorksheet",openWorksheet],["deleteWorksheet",deleteWorksheet],["copy",function(e){e?cutControls():copy.call(this,!0)}],["paste",paste],["executeFormula",internal.Em],["getDataFromRange",getDataFromRange],["quantiyOfPages",pagination.$f],["getRange",selection.eO],["isSelected",selection.sp],["setReadOnly",setReadOnly],["isReadOnly",isReadOnly],["getHighlighted",selection.kV],["dispatch",dispatch.A],["down",down],["first",first],["last",last],["left",left],["right",right],["up",up],["openFilter",filter.N$],["resetFilters",filter.dr]],worksheetPublicMethodsLength=worksheetPublicMethods.length,factory=function(){},createWorksheets=async function(e,t,s){let n=t.worksheets;if(!n)throw new Error("JSS: worksheets are not defined");{let o={animation:!0,onbeforecreate:function(t,s){return s||getNextDefaultWorksheetName(e)},oncreate:function(s,n){if(e.creationThroughJss)e.creationThroughJss=!1;else{const t=s.tabs.headers.children[s.tabs.headers.children.length-2].innerHTML;createWorksheetObj.call(e.worksheets[0],{minDimensions:[10,15],worksheetName:t})}const o=e.worksheets[e.worksheets.length-1];o.element=n,buildWorksheet.call(o).then((function(){(0,toolbar.nK)(o),dispatch.A.call(o,"oncreateworksheet",o,t,e.worksheets.length-1)}))},onchange:function(t,s,n){0!=e.worksheets.length&&e.worksheets[n]&&(0,toolbar.nK)(e.worksheets[n])}};1==t.tabs?o.allowCreate=!0:o.hideHeaders=!0,o.data=[];let r=1;for(let e=0;e<n.length;e++)n[e].worksheetName||(n[e].worksheetName="Sheet"+r++),o.data.push({title:n[e].worksheetName,content:""});s.classList.add("jss_spreadsheet"),s.tabIndex=0;const l=jSuites.tabs(s,o),i=t.style;delete t.style;for(let t=0;t<n.length;t++)n[t].style&&Object.entries(n[t].style).forEach((function([e,s]){"number"==typeof s&&(n[t].style[e]=i[s])})),e.worksheets.push({parent:e,element:l.content.children[t],options:n[t],filters:[],formula:[],history:[],selection:[],historyIndex:-1}),await buildWorksheet.call(e.worksheets[t])}};factory.spreadsheet=async function(e,t,s){if("TABLE"==e.tagName){t||(t={}),t.worksheets||(t.worksheets=[]);const s=(0,helpers.createFromTable)(e,t.worksheets[0]);t.worksheets[0]=s;const n=document.createElement("div");e.parentNode.insertBefore(n,e),e.remove(),e=n}let n={worksheets:s,config:t,element:e,el:e};return n.contextMenu=document.createElement("div"),n.contextMenu.className="jss_contextmenu",n.getWorksheetActive=internal.$O.bind(n),n.fullscreen=internal.Y5.bind(n),n.showToolbar=toolbar.ll.bind(n),n.hideToolbar=toolbar.Ar.bind(n),n.getConfig=getSpreadsheetConfig.bind(n),n.setConfig=setConfig.bind(n),n.setPlugins=function(e){n.plugins||(n.plugins={}),"object"==typeof e&&Object.entries(e).forEach((function([e,t]){n.plugins[e]=t.call(libraryBase.jspreadsheet,n,{},n.config)}))},n.setPlugins(t.plugins),await createWorksheets(n,t,e),n.element.appendChild(n.contextMenu),jSuites.contextmenu(n.contextMenu,{onclick:function(){n.contextMenu.contextmenu.close(!1)}}),1==n.config.fullscreen&&n.element.classList.add("fullscreen"),toolbar.ll.call(n),t.root?setEvents(t.root):setEvents(document),e.spreadsheet=n,n},factory.worksheet=function(e,t,s){let n={parent:e,options:{}};return void 0===s?e.worksheets.push(n):e.worksheets.splice(s,0,n),Object.assign(n.options,t),n};var utils_factory=factory;libraryBase.jspreadsheet=function(e,t){try{let s=[];return utils_factory.spreadsheet(e,t,s).then((e=>{libraryBase.jspreadsheet.spreadsheet.push(e),dispatch.A.call(e,"onload",e)})),s}catch(e){console.error(e)}},libraryBase.jspreadsheet.getWorksheetInstanceByName=function(e,t){const s=libraryBase.jspreadsheet.spreadsheet.find((e=>e.config.namespace===t));if(s)return{};if(null==e){const e=s.worksheets.map((e=>[e.options.worksheetName,e]));return Object.fromEntries(e)}return s.worksheets.find((t=>t.options.worksheetName===e))},libraryBase.jspreadsheet.setDictionary=function(e){jSuites.setDictionary(e)},libraryBase.jspreadsheet.destroy=function(e,t){if(e.spreadsheet){const s=libraryBase.jspreadsheet.spreadsheet.indexOf(e.spreadsheet);libraryBase.jspreadsheet.spreadsheet.splice(s,1);const n=e.spreadsheet.config.root||document;e.spreadsheet=null,e.innerHTML="",t&&destroyEvents(n)}},libraryBase.jspreadsheet.destroyAll=function(){for(let e=0;e<libraryBase.jspreadsheet.spreadsheet.length;e++){const t=libraryBase.jspreadsheet.spreadsheet[e];libraryBase.jspreadsheet.destroy(t.element)}},libraryBase.jspreadsheet.current=null,libraryBase.jspreadsheet.spreadsheet=[],libraryBase.jspreadsheet.helpers={},libraryBase.jspreadsheet.version=function(){return version},Object.entries(helpers).forEach((([e,t])=>{libraryBase.jspreadsheet.helpers[e]=t}));var src=libraryBase.jspreadsheet;jspreadsheet=__webpack_exports__.default})();

    return jspreadsheet;
})));
  // --- END jSpreadsheet unminified code ---

}).call(typeof window !== 'undefined' ? window : globalThis);

const jspreadsheet = typeof window !== 'undefined' ? window.jspreadsheet : globalThis.jspreadsheet;
const jSuites = typeof window !== 'undefined' ? window.jSuites : globalThis.jSuites;

export { jspreadsheet, jSuites };
export default jspreadsheet;
