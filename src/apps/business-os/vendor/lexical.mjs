var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __commonJS = (cb, mod14) => function __require() {
  return mod14 || (0, cb[__getOwnPropNames(cb)[0]])((mod14 = { exports: {} }).exports, mod14), mod14.exports;
};
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod14, isNodeMode, target) => (target = mod14 != null ? __create(__getProtoOf(mod14)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod14 || !mod14.__esModule ? __defProp(target, "default", { value: mod14, enumerable: true }) : target,
  mod14
));

// node_modules/prismjs/prism.js
var require_prism = __commonJS({
  "node_modules/prismjs/prism.js"(exports, module) {
    var _self = typeof window !== "undefined" ? window : typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope ? self : {};
    var Prism2 = (function(_self2) {
      var lang = /(?:^|\s)lang(?:uage)?-([\w-]+)(?=\s|$)/i;
      var uniqueId = 0;
      var plainTextGrammar = {};
      var _2 = {
        /**
         * By default, Prism will attempt to highlight all code elements (by calling {@link Prism.highlightAll}) on the
         * current page after the page finished loading. This might be a problem if e.g. you wanted to asynchronously load
         * additional languages or plugins yourself.
         *
         * By setting this value to `true`, Prism will not automatically highlight all code elements on the page.
         *
         * You obviously have to change this value before the automatic highlighting started. To do this, you can add an
         * empty Prism object into the global scope before loading the Prism script like this:
         *
         * ```js
         * window.Prism = window.Prism || {};
         * Prism.manual = true;
         * // add a new <script> to load Prism's script
         * ```
         *
         * @default false
         * @type {boolean}
         * @memberof Prism
         * @public
         */
        manual: _self2.Prism && _self2.Prism.manual,
        /**
         * By default, if Prism is in a web worker, it assumes that it is in a worker it created itself, so it uses
         * `addEventListener` to communicate with its parent instance. However, if you're using Prism manually in your
         * own worker, you don't want it to do this.
         *
         * By setting this value to `true`, Prism will not add its own listeners to the worker.
         *
         * You obviously have to change this value before Prism executes. To do this, you can add an
         * empty Prism object into the global scope before loading the Prism script like this:
         *
         * ```js
         * window.Prism = window.Prism || {};
         * Prism.disableWorkerMessageHandler = true;
         * // Load Prism's script
         * ```
         *
         * @default false
         * @type {boolean}
         * @memberof Prism
         * @public
         */
        disableWorkerMessageHandler: _self2.Prism && _self2.Prism.disableWorkerMessageHandler,
        /**
         * A namespace for utility methods.
         *
         * All function in this namespace that are not explicitly marked as _public_ are for __internal use only__ and may
         * change or disappear at any time.
         *
         * @namespace
         * @memberof Prism
         */
        util: {
          encode: function encode(tokens) {
            if (tokens instanceof Token) {
              return new Token(tokens.type, encode(tokens.content), tokens.alias);
            } else if (Array.isArray(tokens)) {
              return tokens.map(encode);
            } else {
              return tokens.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\u00a0/g, " ");
            }
          },
          /**
           * Returns the name of the type of the given value.
           *
           * @param {any} o
           * @returns {string}
           * @example
           * type(null)      === 'Null'
           * type(undefined) === 'Undefined'
           * type(123)       === 'Number'
           * type('foo')     === 'String'
           * type(true)      === 'Boolean'
           * type([1, 2])    === 'Array'
           * type({})        === 'Object'
           * type(String)    === 'Function'
           * type(/abc+/)    === 'RegExp'
           */
          type: function(o2) {
            return Object.prototype.toString.call(o2).slice(8, -1);
          },
          /**
           * Returns a unique number for the given object. Later calls will still return the same number.
           *
           * @param {Object} obj
           * @returns {number}
           */
          objId: function(obj) {
            if (!obj["__id"]) {
              Object.defineProperty(obj, "__id", { value: ++uniqueId });
            }
            return obj["__id"];
          },
          /**
           * Creates a deep clone of the given object.
           *
           * The main intended use of this function is to clone language definitions.
           *
           * @param {T} o
           * @param {Record<number, any>} [visited]
           * @returns {T}
           * @template T
           */
          clone: function deepClone(o2, visited) {
            visited = visited || {};
            var clone;
            var id;
            switch (_2.util.type(o2)) {
              case "Object":
                id = _2.util.objId(o2);
                if (visited[id]) {
                  return visited[id];
                }
                clone = /** @type {Record<string, any>} */
                {};
                visited[id] = clone;
                for (var key in o2) {
                  if (o2.hasOwnProperty(key)) {
                    clone[key] = deepClone(o2[key], visited);
                  }
                }
                return (
                  /** @type {any} */
                  clone
                );
              case "Array":
                id = _2.util.objId(o2);
                if (visited[id]) {
                  return visited[id];
                }
                clone = [];
                visited[id] = clone;
                /** @type {Array} */
                /** @type {any} */
                o2.forEach(function(v2, i2) {
                  clone[i2] = deepClone(v2, visited);
                });
                return (
                  /** @type {any} */
                  clone
                );
              default:
                return o2;
            }
          },
          /**
           * Returns the Prism language of the given element set by a `language-xxxx` or `lang-xxxx` class.
           *
           * If no language is set for the element or the element is `null` or `undefined`, `none` will be returned.
           *
           * @param {Element} element
           * @returns {string}
           */
          getLanguage: function(element) {
            while (element) {
              var m2 = lang.exec(element.className);
              if (m2) {
                return m2[1].toLowerCase();
              }
              element = element.parentElement;
            }
            return "none";
          },
          /**
           * Sets the Prism `language-xxxx` class of the given element.
           *
           * @param {Element} element
           * @param {string} language
           * @returns {void}
           */
          setLanguage: function(element, language) {
            element.className = element.className.replace(RegExp(lang, "gi"), "");
            element.classList.add("language-" + language);
          },
          /**
           * Returns the script element that is currently executing.
           *
           * This does __not__ work for line script element.
           *
           * @returns {HTMLScriptElement | null}
           */
          currentScript: function() {
            if (typeof document === "undefined") {
              return null;
            }
            if (document.currentScript && document.currentScript.tagName === "SCRIPT" && 1 < 2) {
              return (
                /** @type {any} */
                document.currentScript
              );
            }
            try {
              throw new Error();
            } catch (err) {
              var src = (/at [^(\r\n]*\((.*):[^:]+:[^:]+\)$/i.exec(err.stack) || [])[1];
              if (src) {
                var scripts = document.getElementsByTagName("script");
                for (var i2 in scripts) {
                  if (scripts[i2].src == src) {
                    return scripts[i2];
                  }
                }
              }
              return null;
            }
          },
          /**
           * Returns whether a given class is active for `element`.
           *
           * The class can be activated if `element` or one of its ancestors has the given class and it can be deactivated
           * if `element` or one of its ancestors has the negated version of the given class. The _negated version_ of the
           * given class is just the given class with a `no-` prefix.
           *
           * Whether the class is active is determined by the closest ancestor of `element` (where `element` itself is
           * closest ancestor) that has the given class or the negated version of it. If neither `element` nor any of its
           * ancestors have the given class or the negated version of it, then the default activation will be returned.
           *
           * In the paradoxical situation where the closest ancestor contains __both__ the given class and the negated
           * version of it, the class is considered active.
           *
           * @param {Element} element
           * @param {string} className
           * @param {boolean} [defaultActivation=false]
           * @returns {boolean}
           */
          isActive: function(element, className, defaultActivation) {
            var no = "no-" + className;
            while (element) {
              var classList = element.classList;
              if (classList.contains(className)) {
                return true;
              }
              if (classList.contains(no)) {
                return false;
              }
              element = element.parentElement;
            }
            return !!defaultActivation;
          }
        },
        /**
         * This namespace contains all currently loaded languages and the some helper functions to create and modify languages.
         *
         * @namespace
         * @memberof Prism
         * @public
         */
        languages: {
          /**
           * The grammar for plain, unformatted text.
           */
          plain: plainTextGrammar,
          plaintext: plainTextGrammar,
          text: plainTextGrammar,
          txt: plainTextGrammar,
          /**
           * Creates a deep copy of the language with the given id and appends the given tokens.
           *
           * If a token in `redef` also appears in the copied language, then the existing token in the copied language
           * will be overwritten at its original position.
           *
           * ## Best practices
           *
           * Since the position of overwriting tokens (token in `redef` that overwrite tokens in the copied language)
           * doesn't matter, they can technically be in any order. However, this can be confusing to others that trying to
           * understand the language definition because, normally, the order of tokens matters in Prism grammars.
           *
           * Therefore, it is encouraged to order overwriting tokens according to the positions of the overwritten tokens.
           * Furthermore, all non-overwriting tokens should be placed after the overwriting ones.
           *
           * @param {string} id The id of the language to extend. This has to be a key in `Prism.languages`.
           * @param {Grammar} redef The new tokens to append.
           * @returns {Grammar} The new language created.
           * @public
           * @example
           * Prism.languages['css-with-colors'] = Prism.languages.extend('css', {
           *     // Prism.languages.css already has a 'comment' token, so this token will overwrite CSS' 'comment' token
           *     // at its original position
           *     'comment': { ... },
           *     // CSS doesn't have a 'color' token, so this token will be appended
           *     'color': /\b(?:red|green|blue)\b/
           * });
           */
          extend: function(id, redef) {
            var lang2 = _2.util.clone(_2.languages[id]);
            for (var key in redef) {
              lang2[key] = redef[key];
            }
            return lang2;
          },
          /**
           * Inserts tokens _before_ another token in a language definition or any other grammar.
           *
           * ## Usage
           *
           * This helper method makes it easy to modify existing languages. For example, the CSS language definition
           * not only defines CSS highlighting for CSS documents, but also needs to define highlighting for CSS embedded
           * in HTML through `<style>` elements. To do this, it needs to modify `Prism.languages.markup` and add the
           * appropriate tokens. However, `Prism.languages.markup` is a regular JavaScript object literal, so if you do
           * this:
           *
           * ```js
           * Prism.languages.markup.style = {
           *     // token
           * };
           * ```
           *
           * then the `style` token will be added (and processed) at the end. `insertBefore` allows you to insert tokens
           * before existing tokens. For the CSS example above, you would use it like this:
           *
           * ```js
           * Prism.languages.insertBefore('markup', 'cdata', {
           *     'style': {
           *         // token
           *     }
           * });
           * ```
           *
           * ## Special cases
           *
           * If the grammars of `inside` and `insert` have tokens with the same name, the tokens in `inside`'s grammar
           * will be ignored.
           *
           * This behavior can be used to insert tokens after `before`:
           *
           * ```js
           * Prism.languages.insertBefore('markup', 'comment', {
           *     'comment': Prism.languages.markup.comment,
           *     // tokens after 'comment'
           * });
           * ```
           *
           * ## Limitations
           *
           * The main problem `insertBefore` has to solve is iteration order. Since ES2015, the iteration order for object
           * properties is guaranteed to be the insertion order (except for integer keys) but some browsers behave
           * differently when keys are deleted and re-inserted. So `insertBefore` can't be implemented by temporarily
           * deleting properties which is necessary to insert at arbitrary positions.
           *
           * To solve this problem, `insertBefore` doesn't actually insert the given tokens into the target object.
           * Instead, it will create a new object and replace all references to the target object with the new one. This
           * can be done without temporarily deleting properties, so the iteration order is well-defined.
           *
           * However, only references that can be reached from `Prism.languages` or `insert` will be replaced. I.e. if
           * you hold the target object in a variable, then the value of the variable will not change.
           *
           * ```js
           * var oldMarkup = Prism.languages.markup;
           * var newMarkup = Prism.languages.insertBefore('markup', 'comment', { ... });
           *
           * assert(oldMarkup !== Prism.languages.markup);
           * assert(newMarkup === Prism.languages.markup);
           * ```
           *
           * @param {string} inside The property of `root` (e.g. a language id in `Prism.languages`) that contains the
           * object to be modified.
           * @param {string} before The key to insert before.
           * @param {Grammar} insert An object containing the key-value pairs to be inserted.
           * @param {Object<string, any>} [root] The object containing `inside`, i.e. the object that contains the
           * object to be modified.
           *
           * Defaults to `Prism.languages`.
           * @returns {Grammar} The new grammar object.
           * @public
           */
          insertBefore: function(inside, before, insert, root) {
            root = root || /** @type {any} */
            _2.languages;
            var grammar = root[inside];
            var ret = {};
            for (var token in grammar) {
              if (grammar.hasOwnProperty(token)) {
                if (token == before) {
                  for (var newToken in insert) {
                    if (insert.hasOwnProperty(newToken)) {
                      ret[newToken] = insert[newToken];
                    }
                  }
                }
                if (!insert.hasOwnProperty(token)) {
                  ret[token] = grammar[token];
                }
              }
            }
            var old = root[inside];
            root[inside] = ret;
            _2.languages.DFS(_2.languages, function(key, value) {
              if (value === old && key != inside) {
                this[key] = ret;
              }
            });
            return ret;
          },
          // Traverse a language definition with Depth First Search
          DFS: function DFS(o2, callback, type, visited) {
            visited = visited || {};
            var objId = _2.util.objId;
            for (var i2 in o2) {
              if (o2.hasOwnProperty(i2)) {
                callback.call(o2, i2, o2[i2], type || i2);
                var property = o2[i2];
                var propertyType = _2.util.type(property);
                if (propertyType === "Object" && !visited[objId(property)]) {
                  visited[objId(property)] = true;
                  DFS(property, callback, null, visited);
                } else if (propertyType === "Array" && !visited[objId(property)]) {
                  visited[objId(property)] = true;
                  DFS(property, callback, i2, visited);
                }
              }
            }
          }
        },
        plugins: {},
        /**
         * This is the most high-level function in Prism’s API.
         * It fetches all the elements that have a `.language-xxxx` class and then calls {@link Prism.highlightElement} on
         * each one of them.
         *
         * This is equivalent to `Prism.highlightAllUnder(document, async, callback)`.
         *
         * @param {boolean} [async=false] Same as in {@link Prism.highlightAllUnder}.
         * @param {HighlightCallback} [callback] Same as in {@link Prism.highlightAllUnder}.
         * @memberof Prism
         * @public
         */
        highlightAll: function(async, callback) {
          _2.highlightAllUnder(document, async, callback);
        },
        /**
         * Fetches all the descendants of `container` that have a `.language-xxxx` class and then calls
         * {@link Prism.highlightElement} on each one of them.
         *
         * The following hooks will be run:
         * 1. `before-highlightall`
         * 2. `before-all-elements-highlight`
         * 3. All hooks of {@link Prism.highlightElement} for each element.
         *
         * @param {ParentNode} container The root element, whose descendants that have a `.language-xxxx` class will be highlighted.
         * @param {boolean} [async=false] Whether each element is to be highlighted asynchronously using Web Workers.
         * @param {HighlightCallback} [callback] An optional callback to be invoked on each element after its highlighting is done.
         * @memberof Prism
         * @public
         */
        highlightAllUnder: function(container, async, callback) {
          var env = {
            callback,
            container,
            selector: 'code[class*="language-"], [class*="language-"] code, code[class*="lang-"], [class*="lang-"] code'
          };
          _2.hooks.run("before-highlightall", env);
          env.elements = Array.prototype.slice.apply(env.container.querySelectorAll(env.selector));
          _2.hooks.run("before-all-elements-highlight", env);
          for (var i2 = 0, element; element = env.elements[i2++]; ) {
            _2.highlightElement(element, async === true, env.callback);
          }
        },
        /**
         * Highlights the code inside a single element.
         *
         * The following hooks will be run:
         * 1. `before-sanity-check`
         * 2. `before-highlight`
         * 3. All hooks of {@link Prism.highlight}. These hooks will be run by an asynchronous worker if `async` is `true`.
         * 4. `before-insert`
         * 5. `after-highlight`
         * 6. `complete`
         *
         * Some the above hooks will be skipped if the element doesn't contain any text or there is no grammar loaded for
         * the element's language.
         *
         * @param {Element} element The element containing the code.
         * It must have a class of `language-xxxx` to be processed, where `xxxx` is a valid language identifier.
         * @param {boolean} [async=false] Whether the element is to be highlighted asynchronously using Web Workers
         * to improve performance and avoid blocking the UI when highlighting very large chunks of code. This option is
         * [disabled by default](https://prismjs.com/faq.html#why-is-asynchronous-highlighting-disabled-by-default).
         *
         * Note: All language definitions required to highlight the code must be included in the main `prism.js` file for
         * asynchronous highlighting to work. You can build your own bundle on the
         * [Download page](https://prismjs.com/download.html).
         * @param {HighlightCallback} [callback] An optional callback to be invoked after the highlighting is done.
         * Mostly useful when `async` is `true`, since in that case, the highlighting is done asynchronously.
         * @memberof Prism
         * @public
         */
        highlightElement: function(element, async, callback) {
          var language = _2.util.getLanguage(element);
          var grammar = _2.languages[language];
          _2.util.setLanguage(element, language);
          var parent = element.parentElement;
          if (parent && parent.nodeName.toLowerCase() === "pre") {
            _2.util.setLanguage(parent, language);
          }
          var code = element.textContent;
          var env = {
            element,
            language,
            grammar,
            code
          };
          function insertHighlightedCode(highlightedCode) {
            env.highlightedCode = highlightedCode;
            _2.hooks.run("before-insert", env);
            env.element.innerHTML = env.highlightedCode;
            _2.hooks.run("after-highlight", env);
            _2.hooks.run("complete", env);
            callback && callback.call(env.element);
          }
          _2.hooks.run("before-sanity-check", env);
          parent = env.element.parentElement;
          if (parent && parent.nodeName.toLowerCase() === "pre" && !parent.hasAttribute("tabindex")) {
            parent.setAttribute("tabindex", "0");
          }
          if (!env.code) {
            _2.hooks.run("complete", env);
            callback && callback.call(env.element);
            return;
          }
          _2.hooks.run("before-highlight", env);
          if (!env.grammar) {
            insertHighlightedCode(_2.util.encode(env.code));
            return;
          }
          if (async && _self2.Worker) {
            var worker = new Worker(_2.filename);
            worker.onmessage = function(evt) {
              insertHighlightedCode(evt.data);
            };
            worker.postMessage(JSON.stringify({
              language: env.language,
              code: env.code,
              immediateClose: true
            }));
          } else {
            insertHighlightedCode(_2.highlight(env.code, env.grammar, env.language));
          }
        },
        /**
         * Low-level function, only use if you know what you’re doing. It accepts a string of text as input
         * and the language definitions to use, and returns a string with the HTML produced.
         *
         * The following hooks will be run:
         * 1. `before-tokenize`
         * 2. `after-tokenize`
         * 3. `wrap`: On each {@link Token}.
         *
         * @param {string} text A string with the code to be highlighted.
         * @param {Grammar} grammar An object containing the tokens to use.
         *
         * Usually a language definition like `Prism.languages.markup`.
         * @param {string} language The name of the language definition passed to `grammar`.
         * @returns {string} The highlighted HTML.
         * @memberof Prism
         * @public
         * @example
         * Prism.highlight('var foo = true;', Prism.languages.javascript, 'javascript');
         */
        highlight: function(text, grammar, language) {
          var env = {
            code: text,
            grammar,
            language
          };
          _2.hooks.run("before-tokenize", env);
          if (!env.grammar) {
            throw new Error('The language "' + env.language + '" has no grammar.');
          }
          env.tokens = _2.tokenize(env.code, env.grammar);
          _2.hooks.run("after-tokenize", env);
          return Token.stringify(_2.util.encode(env.tokens), env.language);
        },
        /**
         * This is the heart of Prism, and the most low-level function you can use. It accepts a string of text as input
         * and the language definitions to use, and returns an array with the tokenized code.
         *
         * When the language definition includes nested tokens, the function is called recursively on each of these tokens.
         *
         * This method could be useful in other contexts as well, as a very crude parser.
         *
         * @param {string} text A string with the code to be highlighted.
         * @param {Grammar} grammar An object containing the tokens to use.
         *
         * Usually a language definition like `Prism.languages.markup`.
         * @returns {TokenStream} An array of strings and tokens, a token stream.
         * @memberof Prism
         * @public
         * @example
         * let code = `var foo = 0;`;
         * let tokens = Prism.tokenize(code, Prism.languages.javascript);
         * tokens.forEach(token => {
         *     if (token instanceof Prism.Token && token.type === 'number') {
         *         console.log(`Found numeric literal: ${token.content}`);
         *     }
         * });
         */
        tokenize: function(text, grammar) {
          var rest = grammar.rest;
          if (rest) {
            for (var token in rest) {
              grammar[token] = rest[token];
            }
            delete grammar.rest;
          }
          var tokenList = new LinkedList();
          addAfter(tokenList, tokenList.head, text);
          matchGrammar(text, tokenList, grammar, tokenList.head, 0);
          return toArray(tokenList);
        },
        /**
         * @namespace
         * @memberof Prism
         * @public
         */
        hooks: {
          all: {},
          /**
           * Adds the given callback to the list of callbacks for the given hook.
           *
           * The callback will be invoked when the hook it is registered for is run.
           * Hooks are usually directly run by a highlight function but you can also run hooks yourself.
           *
           * One callback function can be registered to multiple hooks and the same hook multiple times.
           *
           * @param {string} name The name of the hook.
           * @param {HookCallback} callback The callback function which is given environment variables.
           * @public
           */
          add: function(name, callback) {
            var hooks = _2.hooks.all;
            hooks[name] = hooks[name] || [];
            hooks[name].push(callback);
          },
          /**
           * Runs a hook invoking all registered callbacks with the given environment variables.
           *
           * Callbacks will be invoked synchronously and in the order in which they were registered.
           *
           * @param {string} name The name of the hook.
           * @param {Object<string, any>} env The environment variables of the hook passed to all callbacks registered.
           * @public
           */
          run: function(name, env) {
            var callbacks = _2.hooks.all[name];
            if (!callbacks || !callbacks.length) {
              return;
            }
            for (var i2 = 0, callback; callback = callbacks[i2++]; ) {
              callback(env);
            }
          }
        },
        Token
      };
      _self2.Prism = _2;
      function Token(type, content, alias, matchedStr) {
        this.type = type;
        this.content = content;
        this.alias = alias;
        this.length = (matchedStr || "").length | 0;
      }
      Token.stringify = function stringify(o2, language) {
        if (typeof o2 == "string") {
          return o2;
        }
        if (Array.isArray(o2)) {
          var s2 = "";
          o2.forEach(function(e2) {
            s2 += stringify(e2, language);
          });
          return s2;
        }
        var env = {
          type: o2.type,
          content: stringify(o2.content, language),
          tag: "span",
          classes: ["token", o2.type],
          attributes: {},
          language
        };
        var aliases = o2.alias;
        if (aliases) {
          if (Array.isArray(aliases)) {
            Array.prototype.push.apply(env.classes, aliases);
          } else {
            env.classes.push(aliases);
          }
        }
        _2.hooks.run("wrap", env);
        var attributes = "";
        for (var name in env.attributes) {
          attributes += " " + name + '="' + (env.attributes[name] || "").replace(/"/g, "&quot;") + '"';
        }
        return "<" + env.tag + ' class="' + env.classes.join(" ") + '"' + attributes + ">" + env.content + "</" + env.tag + ">";
      };
      function matchPattern(pattern, pos, text, lookbehind) {
        pattern.lastIndex = pos;
        var match = pattern.exec(text);
        if (match && lookbehind && match[1]) {
          var lookbehindLength = match[1].length;
          match.index += lookbehindLength;
          match[0] = match[0].slice(lookbehindLength);
        }
        return match;
      }
      function matchGrammar(text, tokenList, grammar, startNode, startPos, rematch) {
        for (var token in grammar) {
          if (!grammar.hasOwnProperty(token) || !grammar[token]) {
            continue;
          }
          var patterns = grammar[token];
          patterns = Array.isArray(patterns) ? patterns : [patterns];
          for (var j2 = 0; j2 < patterns.length; ++j2) {
            if (rematch && rematch.cause == token + "," + j2) {
              return;
            }
            var patternObj = patterns[j2];
            var inside = patternObj.inside;
            var lookbehind = !!patternObj.lookbehind;
            var greedy = !!patternObj.greedy;
            var alias = patternObj.alias;
            if (greedy && !patternObj.pattern.global) {
              var flags = patternObj.pattern.toString().match(/[imsuy]*$/)[0];
              patternObj.pattern = RegExp(patternObj.pattern.source, flags + "g");
            }
            var pattern = patternObj.pattern || patternObj;
            for (var currentNode = startNode.next, pos = startPos; currentNode !== tokenList.tail; pos += currentNode.value.length, currentNode = currentNode.next) {
              if (rematch && pos >= rematch.reach) {
                break;
              }
              var str = currentNode.value;
              if (tokenList.length > text.length) {
                return;
              }
              if (str instanceof Token) {
                continue;
              }
              var removeCount = 1;
              var match;
              if (greedy) {
                match = matchPattern(pattern, pos, text, lookbehind);
                if (!match || match.index >= text.length) {
                  break;
                }
                var from = match.index;
                var to = match.index + match[0].length;
                var p2 = pos;
                p2 += currentNode.value.length;
                while (from >= p2) {
                  currentNode = currentNode.next;
                  p2 += currentNode.value.length;
                }
                p2 -= currentNode.value.length;
                pos = p2;
                if (currentNode.value instanceof Token) {
                  continue;
                }
                for (var k = currentNode; k !== tokenList.tail && (p2 < to || typeof k.value === "string"); k = k.next) {
                  removeCount++;
                  p2 += k.value.length;
                }
                removeCount--;
                str = text.slice(pos, p2);
                match.index -= pos;
              } else {
                match = matchPattern(pattern, 0, str, lookbehind);
                if (!match) {
                  continue;
                }
              }
              var from = match.index;
              var matchStr = match[0];
              var before = str.slice(0, from);
              var after = str.slice(from + matchStr.length);
              var reach = pos + str.length;
              if (rematch && reach > rematch.reach) {
                rematch.reach = reach;
              }
              var removeFrom = currentNode.prev;
              if (before) {
                removeFrom = addAfter(tokenList, removeFrom, before);
                pos += before.length;
              }
              removeRange(tokenList, removeFrom, removeCount);
              var wrapped = new Token(token, inside ? _2.tokenize(matchStr, inside) : matchStr, alias, matchStr);
              currentNode = addAfter(tokenList, removeFrom, wrapped);
              if (after) {
                addAfter(tokenList, currentNode, after);
              }
              if (removeCount > 1) {
                var nestedRematch = {
                  cause: token + "," + j2,
                  reach
                };
                matchGrammar(text, tokenList, grammar, currentNode.prev, pos, nestedRematch);
                if (rematch && nestedRematch.reach > rematch.reach) {
                  rematch.reach = nestedRematch.reach;
                }
              }
            }
          }
        }
      }
      function LinkedList() {
        var head = { value: null, prev: null, next: null };
        var tail = { value: null, prev: head, next: null };
        head.next = tail;
        this.head = head;
        this.tail = tail;
        this.length = 0;
      }
      function addAfter(list, node, value) {
        var next = node.next;
        var newNode = { value, prev: node, next };
        node.next = newNode;
        next.prev = newNode;
        list.length++;
        return newNode;
      }
      function removeRange(list, node, count) {
        var next = node.next;
        for (var i2 = 0; i2 < count && next !== list.tail; i2++) {
          next = next.next;
        }
        node.next = next;
        next.prev = node;
        list.length -= i2;
      }
      function toArray(list) {
        var array = [];
        var node = list.head.next;
        while (node !== list.tail) {
          array.push(node.value);
          node = node.next;
        }
        return array;
      }
      if (!_self2.document) {
        if (!_self2.addEventListener) {
          return _2;
        }
        if (!_2.disableWorkerMessageHandler) {
          _self2.addEventListener("message", function(evt) {
            var message = JSON.parse(evt.data);
            var lang2 = message.language;
            var code = message.code;
            var immediateClose = message.immediateClose;
            _self2.postMessage(_2.highlight(code, _2.languages[lang2], lang2));
            if (immediateClose) {
              _self2.close();
            }
          }, false);
        }
        return _2;
      }
      var script = _2.util.currentScript();
      if (script) {
        _2.filename = script.src;
        if (script.hasAttribute("data-manual")) {
          _2.manual = true;
        }
      }
      function highlightAutomaticallyCallback() {
        if (!_2.manual) {
          _2.highlightAll();
        }
      }
      if (!_2.manual) {
        var readyState = document.readyState;
        if (readyState === "loading" || readyState === "interactive" && script && script.defer) {
          document.addEventListener("DOMContentLoaded", highlightAutomaticallyCallback);
        } else {
          if (window.requestAnimationFrame) {
            window.requestAnimationFrame(highlightAutomaticallyCallback);
          } else {
            window.setTimeout(highlightAutomaticallyCallback, 16);
          }
        }
      }
      return _2;
    })(_self);
    if (typeof module !== "undefined" && module.exports) {
      module.exports = Prism2;
    }
    if (typeof global !== "undefined") {
      global.Prism = Prism2;
    }
    Prism2.languages.markup = {
      "comment": {
        pattern: /<!--(?:(?!<!--)[\s\S])*?-->/,
        greedy: true
      },
      "prolog": {
        pattern: /<\?[\s\S]+?\?>/,
        greedy: true
      },
      "doctype": {
        // https://www.w3.org/TR/xml/#NT-doctypedecl
        pattern: /<!DOCTYPE(?:[^>"'[\]]|"[^"]*"|'[^']*')+(?:\[(?:[^<"'\]]|"[^"]*"|'[^']*'|<(?!!--)|<!--(?:[^-]|-(?!->))*-->)*\]\s*)?>/i,
        greedy: true,
        inside: {
          "internal-subset": {
            pattern: /(^[^\[]*\[)[\s\S]+(?=\]>$)/,
            lookbehind: true,
            greedy: true,
            inside: null
            // see below
          },
          "string": {
            pattern: /"[^"]*"|'[^']*'/,
            greedy: true
          },
          "punctuation": /^<!|>$|[[\]]/,
          "doctype-tag": /^DOCTYPE/i,
          "name": /[^\s<>'"]+/
        }
      },
      "cdata": {
        pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
        greedy: true
      },
      "tag": {
        pattern: /<\/?(?!\d)[^\s>\/=$<%]+(?:\s(?:\s*[^\s>\/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))|(?=[\s/>])))+)?\s*\/?>/,
        greedy: true,
        inside: {
          "tag": {
            pattern: /^<\/?[^\s>\/]+/,
            inside: {
              "punctuation": /^<\/?/,
              "namespace": /^[^\s>\/:]+:/
            }
          },
          "special-attr": [],
          "attr-value": {
            pattern: /=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+)/,
            inside: {
              "punctuation": [
                {
                  pattern: /^=/,
                  alias: "attr-equals"
                },
                {
                  pattern: /^(\s*)["']|["']$/,
                  lookbehind: true
                }
              ]
            }
          },
          "punctuation": /\/?>/,
          "attr-name": {
            pattern: /[^\s>\/]+/,
            inside: {
              "namespace": /^[^\s>\/:]+:/
            }
          }
        }
      },
      "entity": [
        {
          pattern: /&[\da-z]{1,8};/i,
          alias: "named-entity"
        },
        /&#x?[\da-f]{1,8};/i
      ]
    };
    Prism2.languages.markup["tag"].inside["attr-value"].inside["entity"] = Prism2.languages.markup["entity"];
    Prism2.languages.markup["doctype"].inside["internal-subset"].inside = Prism2.languages.markup;
    Prism2.hooks.add("wrap", function(env) {
      if (env.type === "entity") {
        env.attributes["title"] = env.content.replace(/&amp;/, "&");
      }
    });
    Object.defineProperty(Prism2.languages.markup.tag, "addInlined", {
      /**
       * Adds an inlined language to markup.
       *
       * An example of an inlined language is CSS with `<style>` tags.
       *
       * @param {string} tagName The name of the tag that contains the inlined language. This name will be treated as
       * case insensitive.
       * @param {string} lang The language key.
       * @example
       * addInlined('style', 'css');
       */
      value: function addInlined2(tagName, lang) {
        var includedCdataInside = {};
        includedCdataInside["language-" + lang] = {
          pattern: /(^<!\[CDATA\[)[\s\S]+?(?=\]\]>$)/i,
          lookbehind: true,
          inside: Prism2.languages[lang]
        };
        includedCdataInside["cdata"] = /^<!\[CDATA\[|\]\]>$/i;
        var inside = {
          "included-cdata": {
            pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
            inside: includedCdataInside
          }
        };
        inside["language-" + lang] = {
          pattern: /[\s\S]+/,
          inside: Prism2.languages[lang]
        };
        var def = {};
        def[tagName] = {
          pattern: RegExp(/(<__[^>]*>)(?:<!\[CDATA\[(?:[^\]]|\](?!\]>))*\]\]>|(?!<!\[CDATA\[)[\s\S])*?(?=<\/__>)/.source.replace(/__/g, function() {
            return tagName;
          }), "i"),
          lookbehind: true,
          greedy: true,
          inside
        };
        Prism2.languages.insertBefore("markup", "cdata", def);
      }
    });
    Object.defineProperty(Prism2.languages.markup.tag, "addAttribute", {
      /**
       * Adds an pattern to highlight languages embedded in HTML attributes.
       *
       * An example of an inlined language is CSS with `style` attributes.
       *
       * @param {string} attrName The name of the tag that contains the inlined language. This name will be treated as
       * case insensitive.
       * @param {string} lang The language key.
       * @example
       * addAttribute('style', 'css');
       */
      value: function(attrName, lang) {
        Prism2.languages.markup.tag.inside["special-attr"].push({
          pattern: RegExp(
            /(^|["'\s])/.source + "(?:" + attrName + ")" + /\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))/.source,
            "i"
          ),
          lookbehind: true,
          inside: {
            "attr-name": /^[^\s=]+/,
            "attr-value": {
              pattern: /=[\s\S]+/,
              inside: {
                "value": {
                  pattern: /(^=\s*(["']|(?!["'])))\S[\s\S]*(?=\2$)/,
                  lookbehind: true,
                  alias: [lang, "language-" + lang],
                  inside: Prism2.languages[lang]
                },
                "punctuation": [
                  {
                    pattern: /^=/,
                    alias: "attr-equals"
                  },
                  /"|'/
                ]
              }
            }
          }
        });
      }
    });
    Prism2.languages.html = Prism2.languages.markup;
    Prism2.languages.mathml = Prism2.languages.markup;
    Prism2.languages.svg = Prism2.languages.markup;
    Prism2.languages.xml = Prism2.languages.extend("markup", {});
    Prism2.languages.ssml = Prism2.languages.xml;
    Prism2.languages.atom = Prism2.languages.xml;
    Prism2.languages.rss = Prism2.languages.xml;
    (function(Prism3) {
      var string = /(?:"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')/;
      Prism3.languages.css = {
        "comment": /\/\*[\s\S]*?\*\//,
        "atrule": {
          pattern: RegExp("@[\\w-](?:" + /[^;{\s"']|\s+(?!\s)/.source + "|" + string.source + ")*?" + /(?:;|(?=\s*\{))/.source),
          inside: {
            "rule": /^@[\w-]+/,
            "selector-function-argument": {
              pattern: /(\bselector\s*\(\s*(?![\s)]))(?:[^()\s]|\s+(?![\s)])|\((?:[^()]|\([^()]*\))*\))+(?=\s*\))/,
              lookbehind: true,
              alias: "selector"
            },
            "keyword": {
              pattern: /(^|[^\w-])(?:and|not|only|or)(?![\w-])/,
              lookbehind: true
            }
            // See rest below
          }
        },
        "url": {
          // https://drafts.csswg.org/css-values-3/#urls
          pattern: RegExp("\\burl\\((?:" + string.source + "|" + /(?:[^\\\r\n()"']|\\[\s\S])*/.source + ")\\)", "i"),
          greedy: true,
          inside: {
            "function": /^url/i,
            "punctuation": /^\(|\)$/,
            "string": {
              pattern: RegExp("^" + string.source + "$"),
              alias: "url"
            }
          }
        },
        "selector": {
          pattern: RegExp(`(^|[{}\\s])[^{}\\s](?:[^{};"'\\s]|\\s+(?![\\s{])|` + string.source + ")*(?=\\s*\\{)"),
          lookbehind: true
        },
        "string": {
          pattern: string,
          greedy: true
        },
        "property": {
          pattern: /(^|[^-\w\xA0-\uFFFF])(?!\s)[-_a-z\xA0-\uFFFF](?:(?!\s)[-\w\xA0-\uFFFF])*(?=\s*:)/i,
          lookbehind: true
        },
        "important": /!important\b/i,
        "function": {
          pattern: /(^|[^-a-z0-9])[-a-z0-9]+(?=\()/i,
          lookbehind: true
        },
        "punctuation": /[(){};:,]/
      };
      Prism3.languages.css["atrule"].inside.rest = Prism3.languages.css;
      var markup = Prism3.languages.markup;
      if (markup) {
        markup.tag.addInlined("style", "css");
        markup.tag.addAttribute("style", "css");
      }
    })(Prism2);
    Prism2.languages.clike = {
      "comment": [
        {
          pattern: /(^|[^\\])\/\*[\s\S]*?(?:\*\/|$)/,
          lookbehind: true,
          greedy: true
        },
        {
          pattern: /(^|[^\\:])\/\/.*/,
          lookbehind: true,
          greedy: true
        }
      ],
      "string": {
        pattern: /(["'])(?:\\(?:\r\n|[\s\S])|(?!\1)[^\\\r\n])*\1/,
        greedy: true
      },
      "class-name": {
        pattern: /(\b(?:class|extends|implements|instanceof|interface|new|trait)\s+|\bcatch\s+\()[\w.\\]+/i,
        lookbehind: true,
        inside: {
          "punctuation": /[.\\]/
        }
      },
      "keyword": /\b(?:break|catch|continue|do|else|finally|for|function|if|in|instanceof|new|null|return|throw|try|while)\b/,
      "boolean": /\b(?:false|true)\b/,
      "function": /\b\w+(?=\()/,
      "number": /\b0x[\da-f]+\b|(?:\b\d+(?:\.\d*)?|\B\.\d+)(?:e[+-]?\d+)?/i,
      "operator": /[<>]=?|[!=]=?=?|--?|\+\+?|&&?|\|\|?|[?*/~^%]/,
      "punctuation": /[{}[\];(),.:]/
    };
    Prism2.languages.javascript = Prism2.languages.extend("clike", {
      "class-name": [
        Prism2.languages.clike["class-name"],
        {
          pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$A-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\.(?:constructor|prototype))/,
          lookbehind: true
        }
      ],
      "keyword": [
        {
          pattern: /((?:^|\})\s*)catch\b/,
          lookbehind: true
        },
        {
          pattern: /(^|[^.]|\.\.\.\s*)\b(?:as|assert(?=\s*\{)|async(?=\s*(?:function\b|\(|[$\w\xA0-\uFFFF]|$))|await|break|case|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally(?=\s*(?:\{|$))|for|from(?=\s*(?:['"]|$))|function|(?:get|set)(?=\s*(?:[#\[$\w\xA0-\uFFFF]|$))|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)\b/,
          lookbehind: true
        }
      ],
      // Allow for all non-ASCII characters (See http://stackoverflow.com/a/2008444)
      "function": /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*(?:\.\s*(?:apply|bind|call)\s*)?\()/,
      "number": {
        pattern: RegExp(
          /(^|[^\w$])/.source + "(?:" + // constant
          (/NaN|Infinity/.source + "|" + // binary integer
          /0[bB][01]+(?:_[01]+)*n?/.source + "|" + // octal integer
          /0[oO][0-7]+(?:_[0-7]+)*n?/.source + "|" + // hexadecimal integer
          /0[xX][\dA-Fa-f]+(?:_[\dA-Fa-f]+)*n?/.source + "|" + // decimal bigint
          /\d+(?:_\d+)*n/.source + "|" + // decimal number (integer or float) but no bigint
          /(?:\d+(?:_\d+)*(?:\.(?:\d+(?:_\d+)*)?)?|\.\d+(?:_\d+)*)(?:[Ee][+-]?\d+(?:_\d+)*)?/.source) + ")" + /(?![\w$])/.source
        ),
        lookbehind: true
      },
      "operator": /--|\+\+|\*\*=?|=>|&&=?|\|\|=?|[!=]==|<<=?|>>>?=?|[-+*/%&|^!=<>]=?|\.{3}|\?\?=?|\?\.?|[~:]/
    });
    Prism2.languages.javascript["class-name"][0].pattern = /(\b(?:class|extends|implements|instanceof|interface|new)\s+)[\w.\\]+/;
    Prism2.languages.insertBefore("javascript", "keyword", {
      "regex": {
        pattern: RegExp(
          // lookbehind
          // eslint-disable-next-line regexp/no-dupe-characters-character-class
          /((?:^|[^$\w\xA0-\uFFFF."'\])\s]|\b(?:return|yield))\s*)/.source + // Regex pattern:
          // There are 2 regex patterns here. The RegExp set notation proposal added support for nested character
          // classes if the `v` flag is present. Unfortunately, nested CCs are both context-free and incompatible
          // with the only syntax, so we have to define 2 different regex patterns.
          /\//.source + "(?:" + /(?:\[(?:[^\]\\\r\n]|\\.)*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}/.source + "|" + // `v` flag syntax. This supports 3 levels of nested character classes.
          /(?:\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.)*\])*\])*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}v[dgimyus]{0,7}/.source + ")" + // lookahead
          /(?=(?:\s|\/\*(?:[^*]|\*(?!\/))*\*\/)*(?:$|[\r\n,.;:})\]]|\/\/))/.source
        ),
        lookbehind: true,
        greedy: true,
        inside: {
          "regex-source": {
            pattern: /^(\/)[\s\S]+(?=\/[a-z]*$)/,
            lookbehind: true,
            alias: "language-regex",
            inside: Prism2.languages.regex
          },
          "regex-delimiter": /^\/|\/$/,
          "regex-flags": /^[a-z]+$/
        }
      },
      // This must be declared before keyword because we use "function" inside the look-forward
      "function-variable": {
        pattern: /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*[=:]\s*(?:async\s*)?(?:\bfunction\b|(?:\((?:[^()]|\([^()]*\))*\)|(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)\s*=>))/,
        alias: "function"
      },
      "parameter": [
        {
          pattern: /(function(?:\s+(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)?\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\))/,
          lookbehind: true,
          inside: Prism2.languages.javascript
        },
        {
          pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$a-z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*=>)/i,
          lookbehind: true,
          inside: Prism2.languages.javascript
        },
        {
          pattern: /(\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*=>)/,
          lookbehind: true,
          inside: Prism2.languages.javascript
        },
        {
          pattern: /((?:\b|\s|^)(?!(?:as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally|for|from|function|get|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|set|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)(?![$\w\xA0-\uFFFF]))(?:(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*\s*)\(\s*|\]\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*\{)/,
          lookbehind: true,
          inside: Prism2.languages.javascript
        }
      ],
      "constant": /\b[A-Z](?:[A-Z_]|\dx?)*\b/
    });
    Prism2.languages.insertBefore("javascript", "string", {
      "hashbang": {
        pattern: /^#!.*/,
        greedy: true,
        alias: "comment"
      },
      "template-string": {
        pattern: /`(?:\\[\s\S]|\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}|(?!\$\{)[^\\`])*`/,
        greedy: true,
        inside: {
          "template-punctuation": {
            pattern: /^`|`$/,
            alias: "string"
          },
          "interpolation": {
            pattern: /((?:^|[^\\])(?:\\{2})*)\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}/,
            lookbehind: true,
            inside: {
              "interpolation-punctuation": {
                pattern: /^\$\{|\}$/,
                alias: "punctuation"
              },
              rest: Prism2.languages.javascript
            }
          },
          "string": /[\s\S]+/
        }
      },
      "string-property": {
        pattern: /((?:^|[,{])[ \t]*)(["'])(?:\\(?:\r\n|[\s\S])|(?!\2)[^\\\r\n])*\2(?=\s*:)/m,
        lookbehind: true,
        greedy: true,
        alias: "property"
      }
    });
    Prism2.languages.insertBefore("javascript", "operator", {
      "literal-property": {
        pattern: /((?:^|[,{])[ \t]*)(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*:)/m,
        lookbehind: true,
        alias: "property"
      }
    });
    if (Prism2.languages.markup) {
      Prism2.languages.markup.tag.addInlined("script", "javascript");
      Prism2.languages.markup.tag.addAttribute(
        /on(?:abort|blur|change|click|composition(?:end|start|update)|dblclick|error|focus(?:in|out)?|key(?:down|up)|load|mouse(?:down|enter|leave|move|out|over|up)|reset|resize|scroll|select|slotchange|submit|unload|wheel)/.source,
        "javascript"
      );
    }
    Prism2.languages.js = Prism2.languages.javascript;
    (function() {
      if (typeof Prism2 === "undefined" || typeof document === "undefined") {
        return;
      }
      if (!Element.prototype.matches) {
        Element.prototype.matches = Element.prototype.msMatchesSelector || Element.prototype.webkitMatchesSelector;
      }
      var LOADING_MESSAGE = "Loading\u2026";
      var FAILURE_MESSAGE = function(status, message) {
        return "\u2716 Error " + status + " while fetching file: " + message;
      };
      var FAILURE_EMPTY_MESSAGE = "\u2716 Error: File does not exist or is empty";
      var EXTENSIONS = {
        "js": "javascript",
        "py": "python",
        "rb": "ruby",
        "ps1": "powershell",
        "psm1": "powershell",
        "sh": "bash",
        "bat": "batch",
        "h": "c",
        "tex": "latex"
      };
      var STATUS_ATTR = "data-src-status";
      var STATUS_LOADING = "loading";
      var STATUS_LOADED = "loaded";
      var STATUS_FAILED = "failed";
      var SELECTOR = "pre[data-src]:not([" + STATUS_ATTR + '="' + STATUS_LOADED + '"]):not([' + STATUS_ATTR + '="' + STATUS_LOADING + '"])';
      function loadFile(src, success, error) {
        var xhr = new XMLHttpRequest();
        xhr.open("GET", src, true);
        xhr.onreadystatechange = function() {
          if (xhr.readyState == 4) {
            if (xhr.status < 400 && xhr.responseText) {
              success(xhr.responseText);
            } else {
              if (xhr.status >= 400) {
                error(FAILURE_MESSAGE(xhr.status, xhr.statusText));
              } else {
                error(FAILURE_EMPTY_MESSAGE);
              }
            }
          }
        };
        xhr.send(null);
      }
      function parseRange(range) {
        var m2 = /^\s*(\d+)\s*(?:(,)\s*(?:(\d+)\s*)?)?$/.exec(range || "");
        if (m2) {
          var start = Number(m2[1]);
          var comma = m2[2];
          var end = m2[3];
          if (!comma) {
            return [start, start];
          }
          if (!end) {
            return [start, void 0];
          }
          return [start, Number(end)];
        }
        return void 0;
      }
      Prism2.hooks.add("before-highlightall", function(env) {
        env.selector += ", " + SELECTOR;
      });
      Prism2.hooks.add("before-sanity-check", function(env) {
        var pre = (
          /** @type {HTMLPreElement} */
          env.element
        );
        if (pre.matches(SELECTOR)) {
          env.code = "";
          pre.setAttribute(STATUS_ATTR, STATUS_LOADING);
          var code = pre.appendChild(document.createElement("CODE"));
          code.textContent = LOADING_MESSAGE;
          var src = pre.getAttribute("data-src");
          var language = env.language;
          if (language === "none") {
            var extension = (/\.(\w+)$/.exec(src) || [, "none"])[1];
            language = EXTENSIONS[extension] || extension;
          }
          Prism2.util.setLanguage(code, language);
          Prism2.util.setLanguage(pre, language);
          var autoloader = Prism2.plugins.autoloader;
          if (autoloader) {
            autoloader.loadLanguages(language);
          }
          loadFile(
            src,
            function(text) {
              pre.setAttribute(STATUS_ATTR, STATUS_LOADED);
              var range = parseRange(pre.getAttribute("data-range"));
              if (range) {
                var lines = text.split(/\r\n?|\n/g);
                var start = range[0];
                var end = range[1] == null ? lines.length : range[1];
                if (start < 0) {
                  start += lines.length;
                }
                start = Math.max(0, Math.min(start - 1, lines.length));
                if (end < 0) {
                  end += lines.length;
                }
                end = Math.max(0, Math.min(end, lines.length));
                text = lines.slice(start, end).join("\n");
                if (!pre.hasAttribute("data-start")) {
                  pre.setAttribute("data-start", String(start + 1));
                }
              }
              code.textContent = text;
              Prism2.highlightElement(code);
            },
            function(error) {
              pre.setAttribute(STATUS_ATTR, STATUS_FAILED);
              code.textContent = error;
            }
          );
        }
      });
      Prism2.plugins.fileHighlight = {
        /**
         * Executes the File Highlight plugin for all matching `pre` elements under the given container.
         *
         * Note: Elements which are already loaded or currently loading will not be touched by this method.
         *
         * @param {ParentNode} [container=document]
         */
        highlight: function highlight(container) {
          var elements = (container || document).querySelectorAll(SELECTOR);
          for (var i2 = 0, element; element = elements[i2++]; ) {
            Prism2.highlightElement(element);
          }
        }
      };
      var logged = false;
      Prism2.fileHighlight = function() {
        if (!logged) {
          console.warn("Prism.fileHighlight is deprecated. Use `Prism.plugins.fileHighlight.highlight` instead.");
          logged = true;
        }
        Prism2.plugins.fileHighlight.highlight.apply(this, arguments);
      };
    })();
  }
});

// node_modules/lexical/Lexical.dev.mjs
var Lexical_dev_exports = {};
__export(Lexical_dev_exports, {
  $addUpdateTag: () => $addUpdateTag,
  $applyNodeReplacement: () => $applyNodeReplacement,
  $caretFromPoint: () => $caretFromPoint,
  $caretRangeFromSelection: () => $caretRangeFromSelection,
  $cloneWithProperties: () => $cloneWithProperties,
  $cloneWithPropertiesEphemeral: () => $cloneWithPropertiesEphemeral,
  $comparePointCaretNext: () => $comparePointCaretNext,
  $copyNode: () => $copyNode,
  $create: () => $create,
  $createChildrenArray: () => $createChildrenArray,
  $createLineBreakNode: () => $createLineBreakNode,
  $createNodeSelection: () => $createNodeSelection,
  $createParagraphNode: () => $createParagraphNode,
  $createPoint: () => $createPoint,
  $createRangeSelection: () => $createRangeSelection,
  $createRangeSelectionFromDom: () => $createRangeSelectionFromDom,
  $createTabNode: () => $createTabNode,
  $createTextNode: () => $createTextNode,
  $extendCaretToRange: () => $extendCaretToRange,
  $findMatchingParent: () => $findMatchingParent,
  $getAdjacentChildCaret: () => $getAdjacentChildCaret,
  $getAdjacentNode: () => $getAdjacentNode,
  $getAdjacentSiblingOrParentSiblingCaret: () => $getAdjacentSiblingOrParentSiblingCaret,
  $getCaretInDirection: () => $getCaretInDirection,
  $getCaretRange: () => $getCaretRange,
  $getCaretRangeInDirection: () => $getCaretRangeInDirection,
  $getCharacterOffsets: () => $getCharacterOffsets,
  $getChildCaret: () => $getChildCaret,
  $getChildCaretAtIndex: () => $getChildCaretAtIndex,
  $getChildCaretOrSelf: () => $getChildCaretOrSelf,
  $getCollapsedCaretRange: () => $getCollapsedCaretRange,
  $getCommonAncestor: () => $getCommonAncestor,
  $getCommonAncestorResultBranchOrder: () => $getCommonAncestorResultBranchOrder,
  $getEditor: () => $getEditor,
  $getEditorDOMRenderConfig: () => $getEditorDOMRenderConfig,
  $getNearestNodeFromDOMNode: () => $getNearestNodeFromDOMNode,
  $getNearestRootOrShadowRoot: () => $getNearestRootOrShadowRoot,
  $getNodeByKey: () => $getNodeByKey,
  $getNodeByKeyOrThrow: () => $getNodeByKeyOrThrow,
  $getNodeFromDOMNode: () => $getNodeFromDOMNode,
  $getPreviousSelection: () => $getPreviousSelection,
  $getRoot: () => $getRoot,
  $getSelection: () => $getSelection,
  $getSiblingCaret: () => $getSiblingCaret,
  $getState: () => $getState,
  $getStateChange: () => $getStateChange,
  $getTextContent: () => $getTextContent,
  $getTextNodeOffset: () => $getTextNodeOffset,
  $getTextPointCaret: () => $getTextPointCaret,
  $getTextPointCaretSlice: () => $getTextPointCaretSlice,
  $getWritableNodeState: () => $getWritableNodeState,
  $hasAncestor: () => $hasAncestor,
  $hasUpdateTag: () => $hasUpdateTag,
  $insertNodes: () => $insertNodes,
  $isBlockElementNode: () => $isBlockElementNode,
  $isChildCaret: () => $isChildCaret,
  $isDecoratorNode: () => $isDecoratorNode,
  $isEditorState: () => $isEditorState,
  $isElementNode: () => $isElementNode,
  $isExtendableTextPointCaret: () => $isExtendableTextPointCaret,
  $isInlineElementOrDecoratorNode: () => $isInlineElementOrDecoratorNode,
  $isLeafNode: () => $isLeafNode,
  $isLexicalNode: () => $isLexicalNode,
  $isLineBreakNode: () => $isLineBreakNode,
  $isNodeCaret: () => $isNodeCaret,
  $isNodeSelection: () => $isNodeSelection,
  $isParagraphNode: () => $isParagraphNode,
  $isRangeSelection: () => $isRangeSelection,
  $isRootNode: () => $isRootNode,
  $isRootOrShadowRoot: () => $isRootOrShadowRoot,
  $isSiblingCaret: () => $isSiblingCaret,
  $isTabNode: () => $isTabNode,
  $isTextNode: () => $isTextNode,
  $isTextPointCaret: () => $isTextPointCaret,
  $isTextPointCaretSlice: () => $isTextPointCaretSlice,
  $isTokenOrSegmented: () => $isTokenOrSegmented,
  $isTokenOrTab: () => $isTokenOrTab,
  $nodesOfType: () => $nodesOfType,
  $normalizeCaret: () => $normalizeCaret,
  $normalizeSelection__EXPERIMENTAL: () => $normalizeSelection,
  $onUpdate: () => $onUpdate,
  $parseSerializedNode: () => $parseSerializedNode,
  $removeTextFromCaretRange: () => $removeTextFromCaretRange,
  $rewindSiblingCaret: () => $rewindSiblingCaret,
  $selectAll: () => $selectAll,
  $setCompositionKey: () => $setCompositionKey,
  $setPointFromCaret: () => $setPointFromCaret,
  $setSelection: () => $setSelection,
  $setSelectionFromCaretRange: () => $setSelectionFromCaretRange,
  $setState: () => $setState,
  $splitAtPointCaretNext: () => $splitAtPointCaretNext,
  $splitNode: () => $splitNode,
  $updateRangeSelectionFromCaretRange: () => $updateRangeSelectionFromCaretRange,
  ArtificialNode__DO_NOT_USE: () => ArtificialNode__DO_NOT_USE,
  BEFORE_INPUT_COMMAND: () => BEFORE_INPUT_COMMAND,
  BLUR_COMMAND: () => BLUR_COMMAND,
  CAN_REDO_COMMAND: () => CAN_REDO_COMMAND,
  CAN_UNDO_COMMAND: () => CAN_UNDO_COMMAND,
  CLEAR_EDITOR_COMMAND: () => CLEAR_EDITOR_COMMAND,
  CLEAR_HISTORY_COMMAND: () => CLEAR_HISTORY_COMMAND,
  CLICK_COMMAND: () => CLICK_COMMAND,
  COLLABORATION_TAG: () => COLLABORATION_TAG,
  COMMAND_PRIORITY_BEFORE_CRITICAL: () => COMMAND_PRIORITY_BEFORE_CRITICAL,
  COMMAND_PRIORITY_BEFORE_EDITOR: () => COMMAND_PRIORITY_BEFORE_EDITOR,
  COMMAND_PRIORITY_BEFORE_HIGH: () => COMMAND_PRIORITY_BEFORE_HIGH,
  COMMAND_PRIORITY_BEFORE_LOW: () => COMMAND_PRIORITY_BEFORE_LOW,
  COMMAND_PRIORITY_BEFORE_NORMAL: () => COMMAND_PRIORITY_BEFORE_NORMAL,
  COMMAND_PRIORITY_CRITICAL: () => COMMAND_PRIORITY_CRITICAL,
  COMMAND_PRIORITY_EDITOR: () => COMMAND_PRIORITY_EDITOR,
  COMMAND_PRIORITY_HIGH: () => COMMAND_PRIORITY_HIGH,
  COMMAND_PRIORITY_LOW: () => COMMAND_PRIORITY_LOW,
  COMMAND_PRIORITY_NORMAL: () => COMMAND_PRIORITY_NORMAL,
  COMPOSITION_END_COMMAND: () => COMPOSITION_END_COMMAND,
  COMPOSITION_END_TAG: () => COMPOSITION_END_TAG,
  COMPOSITION_START_COMMAND: () => COMPOSITION_START_COMMAND,
  COMPOSITION_START_TAG: () => COMPOSITION_START_TAG,
  CONTROLLED_TEXT_INSERTION_COMMAND: () => CONTROLLED_TEXT_INSERTION_COMMAND,
  COPY_COMMAND: () => COPY_COMMAND,
  CUT_COMMAND: () => CUT_COMMAND,
  DEFAULT_EDITOR_DOM_CONFIG: () => DEFAULT_EDITOR_DOM_CONFIG,
  DELETE_CHARACTER_COMMAND: () => DELETE_CHARACTER_COMMAND,
  DELETE_LINE_COMMAND: () => DELETE_LINE_COMMAND,
  DELETE_WORD_COMMAND: () => DELETE_WORD_COMMAND,
  DRAGEND_COMMAND: () => DRAGEND_COMMAND,
  DRAGOVER_COMMAND: () => DRAGOVER_COMMAND,
  DRAGSTART_COMMAND: () => DRAGSTART_COMMAND,
  DROP_COMMAND: () => DROP_COMMAND,
  DecoratorNode: () => DecoratorNode,
  ElementNode: () => ElementNode,
  FOCUS_COMMAND: () => FOCUS_COMMAND,
  FORMAT_ELEMENT_COMMAND: () => FORMAT_ELEMENT_COMMAND,
  FORMAT_TEXT_COMMAND: () => FORMAT_TEXT_COMMAND,
  HISTORIC_TAG: () => HISTORIC_TAG,
  HISTORY_MERGE_TAG: () => HISTORY_MERGE_TAG,
  HISTORY_PUSH_TAG: () => HISTORY_PUSH_TAG,
  INDENT_CONTENT_COMMAND: () => INDENT_CONTENT_COMMAND,
  INPUT_COMMAND: () => INPUT_COMMAND,
  INSERT_LINE_BREAK_COMMAND: () => INSERT_LINE_BREAK_COMMAND,
  INSERT_PARAGRAPH_COMMAND: () => INSERT_PARAGRAPH_COMMAND,
  INSERT_TAB_COMMAND: () => INSERT_TAB_COMMAND,
  INTERNAL_$isBlock: () => INTERNAL_$isBlock,
  IS_ALL_FORMATTING: () => IS_ALL_FORMATTING,
  IS_BOLD: () => IS_BOLD,
  IS_CODE: () => IS_CODE,
  IS_HIGHLIGHT: () => IS_HIGHLIGHT,
  IS_ITALIC: () => IS_ITALIC,
  IS_STRIKETHROUGH: () => IS_STRIKETHROUGH,
  IS_SUBSCRIPT: () => IS_SUBSCRIPT,
  IS_SUPERSCRIPT: () => IS_SUPERSCRIPT,
  IS_UNDERLINE: () => IS_UNDERLINE,
  KEY_ARROW_DOWN_COMMAND: () => KEY_ARROW_DOWN_COMMAND,
  KEY_ARROW_LEFT_COMMAND: () => KEY_ARROW_LEFT_COMMAND,
  KEY_ARROW_RIGHT_COMMAND: () => KEY_ARROW_RIGHT_COMMAND,
  KEY_ARROW_UP_COMMAND: () => KEY_ARROW_UP_COMMAND,
  KEY_BACKSPACE_COMMAND: () => KEY_BACKSPACE_COMMAND,
  KEY_DELETE_COMMAND: () => KEY_DELETE_COMMAND,
  KEY_DOWN_COMMAND: () => KEY_DOWN_COMMAND,
  KEY_ENTER_COMMAND: () => KEY_ENTER_COMMAND,
  KEY_ESCAPE_COMMAND: () => KEY_ESCAPE_COMMAND,
  KEY_MODIFIER_COMMAND: () => KEY_MODIFIER_COMMAND,
  KEY_SPACE_COMMAND: () => KEY_SPACE_COMMAND,
  KEY_TAB_COMMAND: () => KEY_TAB_COMMAND,
  LineBreakNode: () => LineBreakNode,
  MOVE_TO_END: () => MOVE_TO_END,
  MOVE_TO_START: () => MOVE_TO_START,
  NODE_STATE_KEY: () => NODE_STATE_KEY,
  OUTDENT_CONTENT_COMMAND: () => OUTDENT_CONTENT_COMMAND,
  PASTE_COMMAND: () => PASTE_COMMAND,
  PASTE_TAG: () => PASTE_TAG,
  ParagraphNode: () => ParagraphNode,
  REDO_COMMAND: () => REDO_COMMAND,
  REMOVE_TEXT_COMMAND: () => REMOVE_TEXT_COMMAND,
  RootNode: () => RootNode,
  SELECTION_CHANGE_COMMAND: () => SELECTION_CHANGE_COMMAND,
  SELECTION_INSERT_CLIPBOARD_NODES_COMMAND: () => SELECTION_INSERT_CLIPBOARD_NODES_COMMAND,
  SELECT_ALL_COMMAND: () => SELECT_ALL_COMMAND,
  SKIP_COLLAB_TAG: () => SKIP_COLLAB_TAG,
  SKIP_DOM_SELECTION_TAG: () => SKIP_DOM_SELECTION_TAG,
  SKIP_SCROLL_INTO_VIEW_TAG: () => SKIP_SCROLL_INTO_VIEW_TAG,
  SKIP_SELECTION_FOCUS_TAG: () => SKIP_SELECTION_FOCUS_TAG,
  TEXT_TYPE_TO_FORMAT: () => TEXT_TYPE_TO_FORMAT,
  TabNode: () => TabNode,
  TextNode: () => TextNode,
  UNDO_COMMAND: () => UNDO_COMMAND,
  addClassNamesToElement: () => addClassNamesToElement,
  buildImportMap: () => buildImportMap,
  configExtension: () => configExtension,
  createCommand: () => createCommand,
  createEditor: () => createEditor,
  createSharedNodeState: () => createSharedNodeState,
  createState: () => createState,
  declarePeerDependency: () => declarePeerDependency,
  defineExtension: () => defineExtension,
  flipDirection: () => flipDirection,
  getDOMOwnerDocument: () => getDOMOwnerDocument,
  getDOMSelection: () => getDOMSelection,
  getDOMSelectionFromTarget: () => getDOMSelectionFromTarget,
  getDOMTextNode: () => getDOMTextNode,
  getEditorPropertyFromDOMNode: () => getEditorPropertyFromDOMNode,
  getNearestEditorFromDOMNode: () => getNearestEditorFromDOMNode,
  getRegisteredNode: () => getRegisteredNode,
  getRegisteredNodeOrThrow: () => getRegisteredNodeOrThrow,
  getStaticNodeConfig: () => getStaticNodeConfig,
  getStyleObjectFromCSS: () => getStyleObjectFromCSS,
  getTextDirection: () => getTextDirection,
  getTransformSetFromKlass: () => getTransformSetFromKlass,
  isBlockDomNode: () => isBlockDomNode,
  isCurrentlyReadOnlyMode: () => isCurrentlyReadOnlyMode,
  isDOMDocumentNode: () => isDOMDocumentNode,
  isDOMNode: () => isDOMNode,
  isDOMTextNode: () => isDOMTextNode,
  isDOMUnmanaged: () => isDOMUnmanaged,
  isDocumentFragment: () => isDocumentFragment,
  isExactShortcutMatch: () => isExactShortcutMatch,
  isHTMLAnchorElement: () => isHTMLAnchorElement,
  isHTMLElement: () => isHTMLElement,
  isInlineDomNode: () => isInlineDomNode,
  isLexicalEditor: () => isLexicalEditor,
  isModifierMatch: () => isModifierMatch,
  isSelectionCapturedInDecoratorInput: () => isSelectionCapturedInDecoratorInput,
  isSelectionWithinEditor: () => isSelectionWithinEditor,
  makeStepwiseIterator: () => makeStepwiseIterator,
  mergeRegister: () => mergeRegister,
  normalizeClassNames: () => normalizeClassNames,
  removeClassNamesFromElement: () => removeClassNamesFromElement,
  removeFromParent: () => removeFromParent,
  resetRandomKey: () => resetRandomKey,
  safeCast: () => safeCast,
  setDOMStyleFromCSS: () => setDOMStyleFromCSS,
  setDOMStyleObject: () => setDOMStyleObject,
  setDOMUnmanaged: () => setDOMUnmanaged,
  setNodeIndentFromDOM: () => setNodeIndentFromDOM,
  shallowMergeConfig: () => shallowMergeConfig,
  toggleTextFormatType: () => toggleTextFormatType
});
function formatDevErrorMessage(message) {
  throw new Error(message);
}
var CAN_USE_DOM = typeof window !== "undefined" && typeof window.document !== "undefined" && typeof window.document.createElement !== "undefined";
var documentMode = CAN_USE_DOM && "documentMode" in document ? document.documentMode : null;
var IS_APPLE = CAN_USE_DOM && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
var IS_FIREFOX = CAN_USE_DOM && /^(?!.*Seamonkey)(?=.*Firefox).*/i.test(navigator.userAgent);
var CAN_USE_BEFORE_INPUT = CAN_USE_DOM && "InputEvent" in window && !documentMode ? "getTargetRanges" in new window.InputEvent("input") : false;
var IS_IOS = CAN_USE_DOM && /iPad|iPhone|iPod/.test(navigator.userAgent) && !window.MSStream;
var IS_ANDROID = CAN_USE_DOM && /Android/.test(navigator.userAgent);
var IS_SAFARI = CAN_USE_DOM && /Version\/[\d.]+.*Safari/.test(navigator.userAgent) && !IS_ANDROID;
var IS_CHROME = CAN_USE_DOM && /^(?=.*Chrome).*/i.test(navigator.userAgent);
var IS_ANDROID_CHROME = CAN_USE_DOM && IS_ANDROID && IS_CHROME;
var IS_APPLE_WEBKIT = CAN_USE_DOM && /AppleWebKit\/[\d.]+/.test(navigator.userAgent) && IS_APPLE && !IS_CHROME;
var DOM_ELEMENT_TYPE = 1;
var DOM_TEXT_TYPE = 3;
var DOM_DOCUMENT_TYPE = 9;
var DOM_DOCUMENT_FRAGMENT_TYPE = 11;
var NO_DIRTY_NODES = 0;
var HAS_DIRTY_NODES = 1;
var FULL_RECONCILE = 2;
var IS_NORMAL = 0;
var IS_TOKEN = 1;
var IS_SEGMENTED = 2;
var IS_BOLD = 1;
var IS_ITALIC = 1 << 1;
var IS_STRIKETHROUGH = 1 << 2;
var IS_UNDERLINE = 1 << 3;
var IS_CODE = 1 << 4;
var IS_SUBSCRIPT = 1 << 5;
var IS_SUPERSCRIPT = 1 << 6;
var IS_HIGHLIGHT = 1 << 7;
var IS_LOWERCASE = 1 << 8;
var IS_UPPERCASE = 1 << 9;
var IS_CAPITALIZE = 1 << 10;
var IS_ALL_FORMATTING = IS_BOLD | IS_ITALIC | IS_STRIKETHROUGH | IS_UNDERLINE | IS_CODE | IS_SUBSCRIPT | IS_SUPERSCRIPT | IS_HIGHLIGHT | IS_LOWERCASE | IS_UPPERCASE | IS_CAPITALIZE;
var IS_DIRECTIONLESS = 1;
var IS_UNMERGEABLE = 1 << 1;
var IS_ALIGN_LEFT = 1;
var IS_ALIGN_CENTER = 2;
var IS_ALIGN_RIGHT = 3;
var IS_ALIGN_JUSTIFY = 4;
var IS_ALIGN_START = 5;
var IS_ALIGN_END = 6;
var NON_BREAKING_SPACE = "\xA0";
var ZERO_WIDTH_SPACE = "\u200B";
var COMPOSITION_SUFFIX = IS_SAFARI || IS_IOS || IS_APPLE_WEBKIT ? NON_BREAKING_SPACE : ZERO_WIDTH_SPACE;
var DOUBLE_LINE_BREAK = "\n\n";
var COMPOSITION_START_CHAR = IS_FIREFOX ? NON_BREAKING_SPACE : COMPOSITION_SUFFIX;
var RTL = "\u0591-\u07FF\uFB1D-\uFDFD\uFE70-\uFEFC";
var LTR = "A-Za-z\xC0-\xD6\xD8-\xF6\xF8-\u02B8\u0300-\u0590\u0800-\u1FFF\u200E\u2C00-\uFB1C\uFE00-\uFE6F\uFEFD-\uFFFF";
var RTL_REGEX = new RegExp("^[^" + LTR + "]*[" + RTL + "]");
var LTR_REGEX = new RegExp("^[^" + RTL + "]*[" + LTR + "]");
var TEXT_TYPE_TO_FORMAT = {
  bold: IS_BOLD,
  capitalize: IS_CAPITALIZE,
  code: IS_CODE,
  highlight: IS_HIGHLIGHT,
  italic: IS_ITALIC,
  lowercase: IS_LOWERCASE,
  strikethrough: IS_STRIKETHROUGH,
  subscript: IS_SUBSCRIPT,
  superscript: IS_SUPERSCRIPT,
  underline: IS_UNDERLINE,
  uppercase: IS_UPPERCASE
};
var DETAIL_TYPE_TO_DETAIL = {
  directionless: IS_DIRECTIONLESS,
  unmergeable: IS_UNMERGEABLE
};
var ELEMENT_TYPE_TO_FORMAT = {
  center: IS_ALIGN_CENTER,
  end: IS_ALIGN_END,
  justify: IS_ALIGN_JUSTIFY,
  left: IS_ALIGN_LEFT,
  right: IS_ALIGN_RIGHT,
  start: IS_ALIGN_START
};
var ELEMENT_FORMAT_TO_TYPE = {
  [IS_ALIGN_CENTER]: "center",
  [IS_ALIGN_END]: "end",
  [IS_ALIGN_JUSTIFY]: "justify",
  [IS_ALIGN_LEFT]: "left",
  [IS_ALIGN_RIGHT]: "right",
  [IS_ALIGN_START]: "start"
};
var TEXT_MODE_TO_TYPE = {
  normal: IS_NORMAL,
  segmented: IS_SEGMENTED,
  token: IS_TOKEN
};
var TEXT_TYPE_TO_MODE = {
  [IS_NORMAL]: "normal",
  [IS_SEGMENTED]: "segmented",
  [IS_TOKEN]: "token"
};
var NODE_STATE_KEY = "$";
var PROTOTYPE_CONFIG_METHOD = "$config";
var DequeSet = class {
  _front = /* @__PURE__ */ new Set();
  _back = /* @__PURE__ */ new Set();
  _cache;
  get size() {
    return this._front.size + this._back.size;
  }
  addBack(v2) {
    delete this._cache;
    if (!this._front.has(v2)) {
      this._back.add(v2);
    }
    return this;
  }
  addFront(v2) {
    delete this._cache;
    if (!this._back.has(v2)) {
      this._front.add(v2);
    }
    return this;
  }
  delete(v2) {
    delete this._cache;
    return this._front.delete(v2) || this._back.delete(v2);
  }
  toArray() {
    const arr = Array.from(this._front).reverse();
    for (const v2 of this._back) {
      arr.push(v2);
    }
    return arr;
  }
  toReadonlyArray() {
    this._cache = this._cache || this.toArray();
    return this._cache;
  }
  [Symbol.iterator]() {
    return this.toReadonlyArray()[Symbol.iterator]();
  }
};
function $garbageCollectDetachedDecorators(editor, pendingEditorState) {
  const currentDecorators = editor._decorators;
  const pendingDecorators = editor._pendingDecorators;
  let decorators = pendingDecorators || currentDecorators;
  const nodeMap = pendingEditorState._nodeMap;
  let key;
  for (key in decorators) {
    if (!nodeMap.has(key)) {
      if (decorators === currentDecorators) {
        decorators = cloneDecorators(editor);
      }
      delete decorators[key];
    }
  }
}
function $garbageCollectDetachedDeepChildNodes(node, parentKey, prevNodeMap, nodeMap, nodeMapDelete, dirtyNodes) {
  let child = node.getFirstChild();
  while (child !== null) {
    const childKey = child.__key;
    if (child.__parent === parentKey) {
      if ($isElementNode(child)) {
        $garbageCollectDetachedDeepChildNodes(child, childKey, prevNodeMap, nodeMap, nodeMapDelete, dirtyNodes);
      }
      if (!prevNodeMap.has(childKey)) {
        dirtyNodes.delete(childKey);
      }
      nodeMapDelete.push(childKey);
    }
    child = child.getNextSibling();
  }
}
function $garbageCollectDetachedNodes(prevEditorState, editorState, dirtyLeaves, dirtyElements) {
  const prevNodeMap = prevEditorState._nodeMap;
  const nodeMap = editorState._nodeMap;
  const nodeMapDelete = [];
  for (const [nodeKey] of dirtyElements) {
    const node = nodeMap.get(nodeKey);
    if (node !== void 0) {
      if (!node.isAttached()) {
        if ($isElementNode(node)) {
          $garbageCollectDetachedDeepChildNodes(node, nodeKey, prevNodeMap, nodeMap, nodeMapDelete, dirtyElements);
        }
        if (!prevNodeMap.has(nodeKey)) {
          dirtyElements.delete(nodeKey);
        }
        nodeMapDelete.push(nodeKey);
      }
    }
  }
  for (const nodeKey of nodeMapDelete) {
    nodeMap.delete(nodeKey);
  }
  for (const nodeKey of dirtyLeaves) {
    const node = nodeMap.get(nodeKey);
    if (node !== void 0 && !node.isAttached()) {
      if (!prevNodeMap.has(nodeKey)) {
        dirtyLeaves.delete(nodeKey);
      }
      nodeMap.delete(nodeKey);
    }
  }
}
var TEXT_MUTATION_VARIANCE = 100;
var isProcessingMutations = false;
var lastTextEntryTimeStamp = 0;
function getIsProcessingMutations() {
  return isProcessingMutations;
}
function updateTimeStamp(event) {
  lastTextEntryTimeStamp = event.timeStamp;
}
function initTextEntryListener(editor) {
  if (lastTextEntryTimeStamp === 0) {
    getWindow(editor).addEventListener("textInput", updateTimeStamp, true);
  }
}
function isManagedLineBreak(dom, target, editor) {
  const isBR = dom.nodeName === "BR";
  const lexicalLineBreak = target.__lexicalLineBreak;
  return lexicalLineBreak && (dom === lexicalLineBreak || isBR && dom.previousSibling === lexicalLineBreak) || isBR && getNodeKeyFromDOMNode(dom, editor) !== void 0;
}
function getLastSelection(editor) {
  return editor.getEditorState().read(() => {
    const selection = $getSelection();
    return selection !== null ? selection.clone() : null;
  });
}
function $handleTextMutation(target, node, editor) {
  const domSelection = getDOMSelection(getWindow(editor));
  let anchorOffset = null;
  let focusOffset = null;
  if (domSelection !== null && domSelection.anchorNode === target) {
    anchorOffset = domSelection.anchorOffset;
    focusOffset = domSelection.focusOffset;
  }
  const text = target.nodeValue;
  if (text !== null) {
    $updateTextNodeFromDOMContent(node, text, anchorOffset, focusOffset, false);
  }
}
function shouldUpdateTextNodeFromMutation(selection, targetDOM, targetNode) {
  if ($isRangeSelection(selection)) {
    const anchorNode = selection.anchor.getNode();
    if (anchorNode.is(targetNode) && selection.format !== anchorNode.getFormat()) {
      return false;
    }
  }
  return isDOMTextNode(targetDOM) && targetNode.isAttached();
}
function $getNearestManagedNodePairFromDOMNode(startingDOM, editor, editorState, rootElement) {
  for (let dom = startingDOM; dom && !isDOMUnmanaged(dom); dom = getParentElement(dom)) {
    const key = getNodeKeyFromDOMNode(dom, editor);
    if (key !== void 0) {
      const node = $getNodeByKey(key, editorState);
      if (node) {
        return $isDecoratorNode(node) || !isHTMLElement(dom) ? void 0 : [dom, node];
      }
    } else if (dom === rootElement) {
      return [rootElement, internalGetRoot(editorState)];
    }
  }
}
function flushMutations(editor, mutations, observer) {
  isProcessingMutations = true;
  const shouldFlushTextMutations = performance.now() - lastTextEntryTimeStamp > TEXT_MUTATION_VARIANCE;
  try {
    updateEditorSync(editor, () => {
      const selection = $getSelection() || getLastSelection(editor);
      const badDOMTargets = /* @__PURE__ */ new Map();
      const rootElement = editor.getRootElement();
      const currentEditorState = editor._editorState;
      const blockCursorElement = editor._blockCursorElement;
      let shouldRevertSelection = false;
      let possibleTextForFirefoxPaste = "";
      for (let i2 = 0; i2 < mutations.length; i2++) {
        const mutation = mutations[i2];
        const type = mutation.type;
        const targetDOM = mutation.target;
        const pair = $getNearestManagedNodePairFromDOMNode(targetDOM, editor, currentEditorState, rootElement);
        if (!pair) {
          continue;
        }
        const [nodeDOM, targetNode] = pair;
        if (type === "characterData") {
          if (
            // TODO there is an edge case here if a mutation happens too quickly
            //      after text input, it may never be handled since we do not
            //      track the ignored mutations in any way
            shouldFlushTextMutations && $isTextNode(targetNode) && isDOMTextNode(targetDOM) && shouldUpdateTextNodeFromMutation(selection, targetDOM, targetNode)
          ) {
            $handleTextMutation(targetDOM, targetNode, editor);
          }
        } else if (type === "childList") {
          shouldRevertSelection = true;
          const addedDOMs = mutation.addedNodes;
          for (let s2 = 0; s2 < addedDOMs.length; s2++) {
            const addedDOM = addedDOMs[s2];
            const node = $getNodeFromDOMNode(addedDOM);
            const parentDOM = addedDOM.parentNode;
            if (parentDOM != null && addedDOM !== blockCursorElement && node === null && !isManagedLineBreak(addedDOM, parentDOM, editor)) {
              if (IS_FIREFOX) {
                const possibleText = (isHTMLElement(addedDOM) ? addedDOM.innerText : null) || addedDOM.nodeValue;
                if (possibleText) {
                  possibleTextForFirefoxPaste += possibleText;
                }
              }
              parentDOM.removeChild(addedDOM);
            }
          }
          const removedDOMs = mutation.removedNodes;
          const removedDOMsLength = removedDOMs.length;
          if (removedDOMsLength > 0) {
            let unremovedBRs = 0;
            for (let s2 = 0; s2 < removedDOMsLength; s2++) {
              const removedDOM = removedDOMs[s2];
              if (isManagedLineBreak(removedDOM, targetDOM, editor) || blockCursorElement === removedDOM) {
                targetDOM.appendChild(removedDOM);
                unremovedBRs++;
              }
            }
            if (removedDOMsLength !== unremovedBRs) {
              badDOMTargets.set(nodeDOM, targetNode);
            }
          }
        }
      }
      if (badDOMTargets.size > 0) {
        for (const [nodeDOM, targetNode] of badDOMTargets) {
          targetNode.reconcileObservedMutation(nodeDOM, editor);
        }
      }
      const records = observer.takeRecords();
      if (records.length > 0) {
        for (let i2 = 0; i2 < records.length; i2++) {
          const record = records[i2];
          const addedNodes = record.addedNodes;
          const target = record.target;
          for (let s2 = 0; s2 < addedNodes.length; s2++) {
            const addedDOM = addedNodes[s2];
            const parentDOM = addedDOM.parentNode;
            if (parentDOM != null && addedDOM.nodeName === "BR" && !isManagedLineBreak(addedDOM, target, editor)) {
              parentDOM.removeChild(addedDOM);
            }
          }
        }
        observer.takeRecords();
      }
      if (selection !== null) {
        if (shouldRevertSelection) {
          $setSelection(selection);
        }
        if (IS_FIREFOX && isFirefoxClipboardEvents(editor)) {
          selection.insertRawText(possibleTextForFirefoxPaste);
        }
      }
    });
  } finally {
    isProcessingMutations = false;
  }
}
function flushRootMutations(editor) {
  const observer = editor._observer;
  if (observer !== null) {
    const mutations = observer.takeRecords();
    flushMutations(editor, mutations, observer);
  }
}
function initMutationObserver(editor) {
  initTextEntryListener(editor);
  editor._observer = new MutationObserver((mutations, observer) => {
    flushMutations(editor, mutations, observer);
  });
}
var StateConfig = class {
  /** The string key used when serializing this state to JSON */
  key;
  /** The parse function from the StateValueConfig passed to createState */
  parse;
  /**
   * The unparse function from the StateValueConfig passed to createState,
   * with a default that is simply a pass-through that assumes the value is
   * JSON serializable.
   */
  unparse;
  /**
   * An equality function from the StateValueConfig, with a default of
   * Object.is.
   */
  isEqual;
  /**
   * The result of `stateValueConfig.parse(undefined)`, which is computed only
   * once and used as the default value. When the current value `isEqual` to
   * the `defaultValue`, it will not be serialized to JSON.
   */
  defaultValue;
  resetOnCopyNode;
  constructor(key, stateValueConfig) {
    this.key = key;
    this.parse = stateValueConfig.parse.bind(stateValueConfig);
    this.unparse = (stateValueConfig.unparse || coerceToJSON).bind(stateValueConfig);
    this.isEqual = (stateValueConfig.isEqual || Object.is).bind(stateValueConfig);
    this.defaultValue = this.parse(void 0);
    this.resetOnCopyNode = stateValueConfig.resetOnCopyNode || false;
  }
};
// @__NO_SIDE_EFFECTS__
function createState(key, valueConfig) {
  return new StateConfig(key, valueConfig);
}
function $getState(node, stateConfig, version = "latest") {
  const latestOrDirectNode = version === "latest" ? node.getLatest() : node;
  const state = latestOrDirectNode.__state;
  if (state) {
    $checkCollision(node, stateConfig, state);
    return state.getValue(stateConfig);
  }
  return stateConfig.defaultValue;
}
function $getStateChange(node, prevNode, stateConfig) {
  const value = $getState(node, stateConfig, "direct");
  const prevValue = $getState(prevNode, stateConfig, "direct");
  return stateConfig.isEqual(value, prevValue) ? null : [value, prevValue];
}
function $setState(node, stateConfig, valueOrUpdater) {
  errorOnReadOnly();
  let value;
  if (typeof valueOrUpdater === "function") {
    const latest = node.getLatest();
    const prevValue = $getState(latest, stateConfig);
    value = valueOrUpdater(prevValue);
    if (stateConfig.isEqual(prevValue, value)) {
      return latest;
    }
  } else {
    value = valueOrUpdater;
  }
  const writable = node.getWritable();
  const state = $getWritableNodeState(writable);
  $checkCollision(node, stateConfig, state);
  state.updateFromKnown(stateConfig, value);
  return writable;
}
function $checkCollision(node, stateConfig, state) {
  {
    const collision = state.sharedNodeState.sharedConfigMap.get(stateConfig.key);
    if (collision !== void 0 && collision !== stateConfig) {
      {
        formatDevErrorMessage(`$setState: State key collision ${JSON.stringify(stateConfig.key)} detected in ${node.constructor.name} node with type ${node.getType()} and key ${node.getKey()}. Only one StateConfig with a given key should be used on a node.`);
      }
    }
  }
}
function createSharedNodeState(nodeConfig) {
  const sharedConfigMap = /* @__PURE__ */ new Map();
  const flatKeys = /* @__PURE__ */ new Set();
  for (let klass = typeof nodeConfig === "function" ? nodeConfig : nodeConfig.replace; klass.prototype && klass.prototype.getType !== void 0; klass = Object.getPrototypeOf(klass)) {
    const {
      ownNodeConfig
    } = getStaticNodeConfig(klass);
    if (ownNodeConfig && ownNodeConfig.stateConfigs) {
      for (const requiredStateConfig of ownNodeConfig.stateConfigs) {
        let stateConfig;
        if ("stateConfig" in requiredStateConfig) {
          stateConfig = requiredStateConfig.stateConfig;
          if (requiredStateConfig.flat) {
            flatKeys.add(stateConfig.key);
          }
        } else {
          stateConfig = requiredStateConfig;
        }
        sharedConfigMap.set(stateConfig.key, stateConfig);
      }
    }
  }
  return {
    flatKeys,
    sharedConfigMap
  };
}
var NodeState = class _NodeState {
  /**
   * @internal
   *
   * Track the (versioned) node that this NodeState was created for, to
   * facilitate copy-on-write for NodeState. When a LexicalNode is cloned,
   * it will *reference* the NodeState from its prevNode. From the nextNode
   * you can continue to read state without copying, but the first $setState
   * will trigger a copy of the prevNode's NodeState with the node property
   * updated.
   */
  node;
  /**
   * @internal
   *
   * State that has already been parsed in a get state, so it is safe. (can be returned with
   * just a cast since the proof was given before).
   *
   * Note that it uses StateConfig, so in addition to (1) the CURRENT VALUE, it has access to
   * (2) the State key (3) the DEFAULT VALUE and (4) the PARSE FUNCTION
   */
  knownState;
  /**
   * @internal
   *
   * A copy of serializedNode[NODE_STATE_KEY] that is made when JSON is
   * imported but has not been parsed yet.
   *
   * It stays here until a get state requires us to parse it, and since we
   * then know the value is safe we move it to knownState.
   *
   * Note that since only string keys are used here, we can only allow this
   * state to pass-through on export or on the next version since there is
   * no known value configuration. This pass-through is to support scenarios
   * where multiple versions of the editor code are working in parallel so
   * an old version of your code doesnt erase metadata that was
   * set by a newer version of your code.
   */
  unknownState;
  /**
   * @internal
   *
   * This sharedNodeState is preserved across all instances of a given
   * node type in an editor and remains writable. It is how keys are resolved
   * to configuration.
   */
  sharedNodeState;
  /**
   * @internal
   *
   * The count of known or unknown keys in this state, ignoring the
   * intersection between the two sets.
   */
  size;
  /**
   * @internal
   */
  constructor(node, sharedNodeState, unknownState = void 0, knownState = /* @__PURE__ */ new Map(), size = void 0) {
    this.node = node;
    this.sharedNodeState = sharedNodeState;
    this.unknownState = unknownState;
    this.knownState = knownState;
    const {
      sharedConfigMap
    } = this.sharedNodeState;
    const computedSize = size !== void 0 ? size : computeSize(sharedConfigMap, unknownState, knownState);
    {
      if (!(size === void 0 || computedSize === size)) {
        formatDevErrorMessage(`NodeState: size != computedSize (${String(size)} != ${String(computedSize)})`);
      }
      for (const stateConfig of knownState.keys()) {
        if (!sharedConfigMap.has(stateConfig.key)) {
          formatDevErrorMessage(`NodeState: sharedConfigMap missing knownState key ${stateConfig.key}`);
        }
      }
    }
    this.size = computedSize;
  }
  /**
   * @internal
   *
   * Get the value from knownState, or parse it from unknownState
   * if it contains the given key.
   *
   * Updates the sharedConfigMap when no known state is found.
   * Updates unknownState and knownState when an unknownState is parsed.
   */
  getValue(stateConfig) {
    const known = this.knownState.get(stateConfig);
    if (known !== void 0) {
      return known;
    }
    this.sharedNodeState.sharedConfigMap.set(stateConfig.key, stateConfig);
    let parsed = stateConfig.defaultValue;
    if (this.unknownState && stateConfig.key in this.unknownState) {
      const jsonValue = this.unknownState[stateConfig.key];
      if (jsonValue !== void 0) {
        parsed = stateConfig.parse(jsonValue);
      }
      this.updateFromKnown(stateConfig, parsed);
    }
    return parsed;
  }
  /**
   * @internal
   *
   * Used only for advanced use cases, such as collab. The intent here is to
   * allow you to diff states with a more stable interface than the properties
   * of this class.
   */
  getInternalState() {
    return [this.unknownState, this.knownState];
  }
  /**
   * Encode this NodeState to JSON in the format that its node expects.
   * This returns `{[NODE_STATE_KEY]?: UnknownStateRecord}` rather than
   * `UnknownStateRecord | undefined` so that we can support flattening
   * specific entries in the future when nodes can declare what
   * their required StateConfigs are.
   */
  toJSON() {
    const state = {
      ...this.unknownState
    };
    const flatState = {};
    for (const [stateConfig, v2] of this.knownState) {
      if (stateConfig.isEqual(v2, stateConfig.defaultValue)) {
        delete state[stateConfig.key];
      } else {
        state[stateConfig.key] = stateConfig.unparse(v2);
      }
    }
    for (const key of this.sharedNodeState.flatKeys) {
      if (key in state) {
        flatState[key] = state[key];
        delete state[key];
      }
    }
    if (undefinedIfEmpty(state)) {
      flatState[NODE_STATE_KEY] = state;
    }
    return flatState;
  }
  /**
   * @internal
   *
   * A NodeState is writable when the node to update matches
   * the node associated with the NodeState. This basically
   * mirrors how the EditorState NodeMap works, but in a
   * bottom-up organization rather than a top-down organization.
   *
   * This allows us to implement the same "copy on write"
   * pattern for state, without having the state version
   * update every time the node version changes (e.g. when
   * its parent or siblings change).
   *
   * @param node The node to associate with the state
   * @returns The next writable state
   */
  getWritable(node) {
    if (this.node === node) {
      return this;
    }
    const {
      sharedNodeState,
      unknownState
    } = this;
    const nextKnownState = new Map(this.knownState);
    return new _NodeState(node, sharedNodeState, parseAndPruneNextUnknownState(sharedNodeState.sharedConfigMap, nextKnownState, unknownState), nextKnownState, this.size);
  }
  /** @internal */
  resetOnCopyNode() {
    for (const stateConfig of this.knownState.keys()) {
      if (stateConfig.resetOnCopyNode) {
        this.knownState.set(stateConfig, stateConfig.defaultValue);
      }
    }
    return this;
  }
  /** @internal */
  updateFromKnown(stateConfig, value) {
    const key = stateConfig.key;
    this.sharedNodeState.sharedConfigMap.set(key, stateConfig);
    const {
      knownState,
      unknownState
    } = this;
    if (!(knownState.has(stateConfig) || unknownState && key in unknownState)) {
      if (unknownState) {
        delete unknownState[key];
        this.unknownState = undefinedIfEmpty(unknownState);
      }
      this.size++;
    }
    knownState.set(stateConfig, value);
  }
  /**
   * @internal
   *
   * This is intended for advanced use cases only, such
   * as collab or dev tools.
   *
   * Update a single key value pair from unknown state,
   * parsing it if the key is known to this node. This is
   * basically like updateFromJSON, but the effect is
   * isolated to a single entry.
   *
   * @param k The string key from an UnknownStateRecord
   * @param v The unknown value from an UnknownStateRecord
   */
  updateFromUnknown(k, v2) {
    const stateConfig = this.sharedNodeState.sharedConfigMap.get(k);
    if (stateConfig) {
      this.updateFromKnown(stateConfig, stateConfig.parse(v2));
    } else {
      this.unknownState = this.unknownState || {};
      if (!(k in this.unknownState)) {
        this.size++;
      }
      this.unknownState[k] = v2;
    }
  }
  /**
   * @internal
   *
   * Reset all existing state to default or empty values,
   * and perform any updates from the given unknownState.
   *
   * This is used when initializing a node's state from JSON,
   * or when resetting a node's state from JSON.
   *
   * @param unknownState The new state in serialized form
   */
  updateFromJSON(unknownState) {
    const {
      knownState
    } = this;
    for (const stateConfig of knownState.keys()) {
      knownState.set(stateConfig, stateConfig.defaultValue);
    }
    this.size = knownState.size;
    this.unknownState = void 0;
    if (unknownState) {
      for (const [k, v2] of Object.entries(unknownState)) {
        this.updateFromUnknown(k, v2);
      }
    }
  }
};
function $getWritableNodeState(node) {
  const writable = node.getWritable();
  const state = writable.__state ? writable.__state.getWritable(writable) : new NodeState(writable, $getSharedNodeState(writable));
  writable.__state = state;
  return state;
}
function $getSharedNodeState(node) {
  return node.__state ? node.__state.sharedNodeState : getRegisteredNodeOrThrow($getEditor(), node.getType()).sharedNodeState;
}
function $updateStateFromJSON(node, serialized) {
  const writable = node.getWritable();
  const unknownState = serialized[NODE_STATE_KEY];
  let parseState = unknownState;
  for (const k of $getSharedNodeState(writable).flatKeys) {
    if (k in serialized) {
      if (parseState === void 0 || parseState === unknownState) {
        parseState = {
          ...unknownState
        };
      }
      parseState[k] = serialized[k];
    }
  }
  if (writable.__state || parseState) {
    $getWritableNodeState(node).updateFromJSON(parseState);
  }
  return writable;
}
function nodeStatesAreEquivalent(a2, b2) {
  if (a2 === b2) {
    return true;
  }
  const keys = /* @__PURE__ */ new Set();
  return !(a2 && hasUnequalMapEntry(keys, a2, b2) || b2 && hasUnequalMapEntry(keys, b2, a2) || a2 && hasUnequalRecordEntry(keys, a2, b2) || b2 && hasUnequalRecordEntry(keys, b2, a2));
}
function computeSize(sharedConfigMap, unknownState, knownState) {
  let size = knownState.size;
  if (unknownState) {
    for (const k in unknownState) {
      const sharedConfig = sharedConfigMap.get(k);
      if (!sharedConfig || !knownState.has(sharedConfig)) {
        size++;
      }
    }
  }
  return size;
}
function undefinedIfEmpty(obj) {
  if (obj) {
    for (const key in obj) {
      return obj;
    }
  }
  return void 0;
}
function coerceToJSON(v2) {
  return v2;
}
function parseAndPruneNextUnknownState(sharedConfigMap, nextKnownState, unknownState) {
  let nextUnknownState = void 0;
  if (unknownState) {
    for (const [k, v2] of Object.entries(unknownState)) {
      const stateConfig = sharedConfigMap.get(k);
      if (stateConfig) {
        if (!nextKnownState.has(stateConfig)) {
          nextKnownState.set(stateConfig, stateConfig.parse(v2));
        }
      } else {
        nextUnknownState = nextUnknownState || {};
        nextUnknownState[k] = v2;
      }
    }
  }
  return nextUnknownState;
}
function hasUnequalMapEntry(keys, sourceState, otherState) {
  for (const [stateConfig, value] of sourceState.knownState) {
    if (keys.has(stateConfig.key)) {
      continue;
    }
    keys.add(stateConfig.key);
    const otherValue = otherState ? otherState.getValue(stateConfig) : stateConfig.defaultValue;
    if (otherValue !== value && !stateConfig.isEqual(otherValue, value)) {
      return true;
    }
  }
  return false;
}
function hasUnequalRecordEntry(keys, sourceState, otherState) {
  const {
    unknownState
  } = sourceState;
  const otherUnknownState = otherState ? otherState.unknownState : void 0;
  if (unknownState) {
    for (const [key, value] of Object.entries(unknownState)) {
      if (keys.has(key)) {
        continue;
      }
      keys.add(key);
      const otherValue = otherUnknownState ? otherUnknownState[key] : void 0;
      if (value !== otherValue) {
        return true;
      }
    }
  }
  return false;
}
function $cloneNodeState(from, to) {
  const state = from.__state;
  return state && state.node === from ? state.getWritable(to) : state;
}
function $canSimpleTextNodesBeMerged(node1, node2) {
  const node1Mode = node1.__mode;
  const node1Format = node1.__format;
  const node1Style = node1.__style;
  const node2Mode = node2.__mode;
  const node2Format = node2.__format;
  const node2Style = node2.__style;
  const node1State = node1.__state;
  const node2State = node2.__state;
  return (node1Mode === null || node1Mode === node2Mode) && (node1Format === null || node1Format === node2Format) && (node1Style === null || node1Style === node2Style) && (node1.__state === null || node1State === node2State || nodeStatesAreEquivalent(node1State, node2State));
}
function $mergeTextNodes(node1, node2) {
  const writableNode1 = node1.mergeWithSibling(node2);
  const normalizedNodes = getActiveEditor()._normalizedNodes;
  normalizedNodes.add(node1.__key);
  normalizedNodes.add(node2.__key);
  return writableNode1;
}
function $normalizeTextNode(textNode) {
  let node = textNode;
  if (node.__text === "" && node.isSimpleText() && !node.isUnmergeable()) {
    node.remove();
    return;
  }
  let previousNode;
  while ((previousNode = node.getPreviousSibling()) !== null && $isTextNode(previousNode) && previousNode.isSimpleText() && !previousNode.isUnmergeable()) {
    if (previousNode.__text === "") {
      previousNode.remove();
    } else if ($canSimpleTextNodesBeMerged(previousNode, node)) {
      node = $mergeTextNodes(previousNode, node);
      break;
    } else {
      break;
    }
  }
  let nextNode;
  while ((nextNode = node.getNextSibling()) !== null && $isTextNode(nextNode) && nextNode.isSimpleText() && !nextNode.isUnmergeable()) {
    if (nextNode.__text === "") {
      nextNode.remove();
    } else if ($canSimpleTextNodesBeMerged(node, nextNode)) {
      node = $mergeTextNodes(node, nextNode);
      break;
    } else {
      break;
    }
  }
}
function $normalizeSelection(selection) {
  $normalizePoint(selection.anchor);
  $normalizePoint(selection.focus);
  return selection;
}
function $normalizePoint(point) {
  while (point.type === "element") {
    const node = point.getNode();
    const offset = point.offset;
    let nextNode;
    let nextOffsetAtEnd;
    if (offset === node.getChildrenSize()) {
      nextNode = node.getChildAtIndex(offset - 1);
      nextOffsetAtEnd = true;
    } else {
      nextNode = node.getChildAtIndex(offset);
      nextOffsetAtEnd = false;
    }
    if ($isTextNode(nextNode)) {
      point.set(nextNode.__key, nextOffsetAtEnd ? nextNode.getTextContentSize() : 0, "text", true);
      break;
    } else if (!$isElementNode(nextNode)) {
      break;
    }
    point.set(nextNode.__key, nextOffsetAtEnd ? nextNode.getChildrenSize() : 0, "element", true);
  }
}
var subTreeTextContent = "";
var subTreeTextFormat = null;
var subTreeTextStyle = null;
var activeEditorConfig;
var activeEditor$1;
var activeEditorNodes;
var treatAllNodesAsDirty = false;
var activeEditorStateReadOnly = false;
var activeMutationListeners;
var activeDirtyElements;
var activeDirtyLeaves;
var activePrevNodeMap;
var activeNextNodeMap;
var activePrevKeyToDOMMap;
var mutatedNodes;
var activeEditorDOMRenderConfig;
function $destroyNode(key, parentDOM) {
  const node = activePrevNodeMap.get(key);
  if (parentDOM !== null) {
    const dom = getPrevElementByKeyOrThrow(key);
    if (dom.parentNode === parentDOM) {
      parentDOM.removeChild(dom);
    }
  }
  if (!activeNextNodeMap.has(key)) {
    activeEditor$1._keyToDOMMap.delete(key);
  }
  if ($isElementNode(node)) {
    const children = $createChildrenArray(node, activePrevNodeMap);
    $destroyChildren(children, 0, children.length - 1, null);
  }
  if (node !== void 0) {
    setMutatedNode(mutatedNodes, activeEditorNodes, activeMutationListeners, node, "destroyed");
  }
}
function $destroyChildren(children, _startIndex, endIndex, dom) {
  for (let startIndex = _startIndex; startIndex <= endIndex; ++startIndex) {
    const child = children[startIndex];
    if (child !== void 0) {
      $destroyNode(child, dom);
    }
  }
}
function setTextAlign(domStyle, value) {
  domStyle.setProperty("text-align", value);
}
var DEFAULT_INDENT_VALUE = "40px";
function setElementIndent(dom, indent) {
  const indentClassName = activeEditorConfig.theme.indent;
  if (typeof indentClassName === "string") {
    const elementHasClassName = dom.classList.contains(indentClassName);
    if (indent > 0 && !elementHasClassName) {
      dom.classList.add(indentClassName);
    } else if (indent < 1 && elementHasClassName) {
      dom.classList.remove(indentClassName);
    }
  }
  if (indent === 0) {
    dom.style.setProperty("padding-inline-start", "");
    return;
  }
  const indentationBaseValue = getComputedStyle(activeEditor$1._rootElement || dom).getPropertyValue("--lexical-indent-base-value") || DEFAULT_INDENT_VALUE;
  dom.style.setProperty("padding-inline-start", `calc(${indent} * ${indentationBaseValue})`);
}
function setElementFormat(dom, format) {
  const domStyle = dom.style;
  if (format === 0) {
    setTextAlign(domStyle, "");
  } else if (format === IS_ALIGN_LEFT) {
    setTextAlign(domStyle, "left");
  } else if (format === IS_ALIGN_CENTER) {
    setTextAlign(domStyle, "center");
  } else if (format === IS_ALIGN_RIGHT) {
    setTextAlign(domStyle, "right");
  } else if (format === IS_ALIGN_JUSTIFY) {
    setTextAlign(domStyle, "justify");
  } else if (format === IS_ALIGN_START) {
    setTextAlign(domStyle, "start");
  } else if (format === IS_ALIGN_END) {
    setTextAlign(domStyle, "end");
  }
}
function $getReconciledDirection(node) {
  const direction = node.__dir;
  if (direction !== null) {
    return direction;
  }
  if ($isRootNode(node)) {
    return null;
  }
  const parent = node.getParentOrThrow();
  if (!$isRootNode(parent) || parent.__dir !== null) {
    return null;
  }
  return "auto";
}
function $setElementDirection(dom, node) {
  const direction = $getReconciledDirection(node);
  if (direction !== null) {
    dom.dir = direction;
  } else {
    dom.removeAttribute("dir");
  }
}
function $createNode(key, slot) {
  const node = activeNextNodeMap.get(key);
  if (node === void 0) {
    {
      formatDevErrorMessage(`createNode: node does not exist in nodeMap`);
    }
  }
  const dom = activeEditorDOMRenderConfig.$createDOM(node, activeEditor$1);
  storeDOMWithKey(key, dom, activeEditor$1);
  if ($isTextNode(node)) {
    dom.setAttribute("data-lexical-text", "true");
  } else if ($isDecoratorNode(node)) {
    dom.setAttribute("data-lexical-decorator", "true");
  }
  if ($isElementNode(node)) {
    const indent = node.__indent;
    const childrenSize = node.__size;
    $setElementDirection(dom, node);
    if (indent !== 0) {
      setElementIndent(dom, indent);
    }
    if (childrenSize !== 0) {
      const endIndex = childrenSize - 1;
      const children = $createChildrenArray(node, activeNextNodeMap);
      $createChildren(children, node, 0, endIndex, activeEditorDOMRenderConfig.$getDOMSlot(node, dom, activeEditor$1));
    }
    const format = node.__format;
    if (format !== 0) {
      setElementFormat(dom, format);
    }
    if (!node.isInline()) {
      $reconcileElementTerminatingLineBreak(null, node, dom);
    }
  } else {
    const text = node.getTextContent();
    if ($isDecoratorNode(node)) {
      const decorator = node.decorate(activeEditor$1, activeEditorConfig);
      if (decorator !== null) {
        reconcileDecorator(key, decorator);
      }
      dom.contentEditable = "false";
    }
    subTreeTextContent += text;
  }
  if (slot !== null) {
    slot.insertChild(dom);
  }
  activeEditorDOMRenderConfig.$decorateDOM(node, null, dom, activeEditor$1);
  {
    Object.freeze(node);
  }
  setMutatedNode(mutatedNodes, activeEditorNodes, activeMutationListeners, node, "created");
  return dom;
}
function $createChildren(children, element, _startIndex, endIndex, slot) {
  const previousSubTreeTextContent = subTreeTextContent;
  subTreeTextContent = "";
  let startIndex = _startIndex;
  for (; startIndex <= endIndex; ++startIndex) {
    $createNode(children[startIndex], slot);
    const node = activeNextNodeMap.get(children[startIndex]);
    if (node !== null && $isTextNode(node)) {
      if (subTreeTextFormat === null) {
        subTreeTextFormat = node.getFormat();
        subTreeTextStyle = node.getStyle();
      }
    } else if (
      // inline $textContentRequiresDoubleLinebreakAtEnd
      $isElementNode(node) && startIndex < endIndex && !node.isInline()
    ) {
      subTreeTextContent += DOUBLE_LINE_BREAK;
    }
  }
  const dom = slot.element;
  dom.__lexicalTextContent = subTreeTextContent;
  subTreeTextContent = previousSubTreeTextContent + subTreeTextContent;
}
function isLastChildLineBreakOrDecorator(element, nodeMap) {
  if (element) {
    const lastKey = element.__last;
    if (lastKey) {
      const node = nodeMap.get(lastKey);
      if (node) {
        return $isLineBreakNode(node) ? "line-break" : $isDecoratorNode(node) && node.isInline() ? "decorator" : null;
      }
    }
    return "empty";
  }
  return null;
}
function $reconcileElementTerminatingLineBreak(prevElement, nextElement, dom) {
  const prevLineBreak = isLastChildLineBreakOrDecorator(prevElement, activePrevNodeMap);
  const nextLineBreak = isLastChildLineBreakOrDecorator(nextElement, activeNextNodeMap);
  if (prevLineBreak !== nextLineBreak) {
    activeEditorDOMRenderConfig.$getDOMSlot(nextElement, dom, activeEditor$1).setManagedLineBreak(nextLineBreak);
  }
}
function reconcileTextFormat(element) {
  if (subTreeTextFormat != null && subTreeTextFormat !== element.__textFormat && !activeEditorStateReadOnly) {
    element.setTextFormat(subTreeTextFormat);
  }
}
function reconcileTextStyle(element) {
  if (subTreeTextStyle != null && subTreeTextStyle !== element.__textStyle && !activeEditorStateReadOnly) {
    element.setTextStyle(subTreeTextStyle);
  }
}
function $reconcileChildrenWithDirection(prevElement, nextElement, dom) {
  subTreeTextFormat = null;
  subTreeTextStyle = null;
  $reconcileChildren(prevElement, nextElement, activeEditorDOMRenderConfig.$getDOMSlot(nextElement, dom, activeEditor$1));
  reconcileTextFormat(nextElement);
  reconcileTextStyle(nextElement);
}
function $reconcileChildren(prevElement, nextElement, slot) {
  const previousSubTreeTextContent = subTreeTextContent;
  const prevChildrenSize = prevElement.__size;
  const nextChildrenSize = nextElement.__size;
  subTreeTextContent = "";
  const dom = slot.element;
  if (prevChildrenSize === 1 && nextChildrenSize === 1) {
    const prevFirstChildKey = prevElement.__first;
    const nextFirstChildKey = nextElement.__first;
    if (prevFirstChildKey === nextFirstChildKey) {
      $reconcileNode(prevFirstChildKey, dom);
    } else {
      const lastDOM = getPrevElementByKeyOrThrow(prevFirstChildKey);
      const replacementDOM = $createNode(nextFirstChildKey, null);
      try {
        dom.replaceChild(replacementDOM, lastDOM);
      } catch (error) {
        if (typeof error === "object" && error != null) {
          const msg = `${error.toString()} Parent: ${dom.tagName}, new child: {tag: ${replacementDOM.tagName} key: ${nextFirstChildKey}}, old child: {tag: ${lastDOM.tagName}, key: ${prevFirstChildKey}}.`;
          throw new Error(msg);
        } else {
          throw error;
        }
      }
      $destroyNode(prevFirstChildKey, null);
    }
    const nextChildNode = activeNextNodeMap.get(nextFirstChildKey);
    if ($isTextNode(nextChildNode)) {
      if (subTreeTextFormat === null) {
        subTreeTextFormat = nextChildNode.getFormat();
        subTreeTextStyle = nextChildNode.getStyle();
      }
    }
  } else {
    const prevChildren = $createChildrenArray(prevElement, activePrevNodeMap);
    const nextChildren = $createChildrenArray(nextElement, activeNextNodeMap);
    if (!(prevChildren.length === prevChildrenSize)) {
      formatDevErrorMessage(`$reconcileChildren: prevChildren.length !== prevChildrenSize`);
    }
    if (!(nextChildren.length === nextChildrenSize)) {
      formatDevErrorMessage(`$reconcileChildren: nextChildren.length !== nextChildrenSize`);
    }
    if (prevChildrenSize === 0) {
      if (nextChildrenSize !== 0) {
        $createChildren(nextChildren, nextElement, 0, nextChildrenSize - 1, slot);
      }
    } else if (nextChildrenSize === 0) {
      if (prevChildrenSize !== 0) {
        const canUseFastPath = slot.after == null && slot.before == null && slot.element.__lexicalLineBreak == null;
        $destroyChildren(prevChildren, 0, prevChildrenSize - 1, canUseFastPath ? null : dom);
        if (canUseFastPath) {
          dom.textContent = "";
        }
      }
    } else {
      $reconcileNodeChildren(nextElement, prevChildren, nextChildren, prevChildrenSize, nextChildrenSize, slot);
    }
  }
  dom.__lexicalTextContent = subTreeTextContent;
  subTreeTextContent = previousSubTreeTextContent + subTreeTextContent;
}
function $reconcileNode(key, parentDOM) {
  const prevNode = activePrevNodeMap.get(key);
  let nextNode = activeNextNodeMap.get(key);
  if (prevNode === void 0 || nextNode === void 0) {
    {
      formatDevErrorMessage(`reconcileNode: prevNode or nextNode does not exist in nodeMap`);
    }
  }
  const isDirty = treatAllNodesAsDirty || activeDirtyLeaves.has(key) || activeDirtyElements.has(key);
  const dom = getElementByKeyOrThrow(activeEditor$1, key);
  if (prevNode === nextNode && !isDirty) {
    let text;
    if ($isElementNode(prevNode)) {
      const previousSubTreeTextContent = dom.__lexicalTextContent;
      if (typeof previousSubTreeTextContent === "string") {
        text = previousSubTreeTextContent;
      } else {
        text = prevNode.getTextContent();
        dom.__lexicalTextContent = text;
      }
    } else {
      text = prevNode.getTextContent();
    }
    subTreeTextContent += text;
    return dom;
  }
  if (prevNode !== nextNode && isDirty) {
    setMutatedNode(mutatedNodes, activeEditorNodes, activeMutationListeners, nextNode, "updated");
  }
  if (activeEditorDOMRenderConfig.$updateDOM(nextNode, prevNode, dom, activeEditor$1)) {
    const replacementDOM = $createNode(key, null);
    if (parentDOM === null) {
      {
        formatDevErrorMessage(`reconcileNode: parentDOM is null`);
      }
    }
    parentDOM.replaceChild(replacementDOM, dom);
    $destroyNode(key, null);
    return replacementDOM;
  }
  if ($isElementNode(prevNode)) {
    if (!$isElementNode(nextNode)) {
      formatDevErrorMessage(`Node with key ${key} changed from ElementNode to !ElementNode`);
    }
    const nextIndent = nextNode.__indent;
    if (treatAllNodesAsDirty || nextIndent !== prevNode.__indent) {
      setElementIndent(dom, nextIndent);
    }
    const nextFormat = nextNode.__format;
    if (treatAllNodesAsDirty || nextFormat !== prevNode.__format) {
      setElementFormat(dom, nextFormat);
    }
    if (isDirty) {
      $reconcileChildrenWithDirection(prevNode, nextNode, dom);
      if (!$isRootNode(nextNode) && !nextNode.isInline()) {
        $reconcileElementTerminatingLineBreak(prevNode, nextNode, dom);
      }
    } else {
      const previousSubTreeTextContent = dom.__lexicalTextContent;
      let text;
      if (typeof previousSubTreeTextContent === "string") {
        text = previousSubTreeTextContent;
      } else {
        text = prevNode.getTextContent();
        dom.__lexicalTextContent = text;
      }
      subTreeTextContent += text;
    }
    if (treatAllNodesAsDirty || nextNode.__dir !== prevNode.__dir) {
      $setElementDirection(dom, nextNode);
      if (
        // Root node direction changing from set to unset (or vice versa)
        // changes how children's direction is calculated.
        $isRootNode(nextNode) && // Can skip if all children already reconciled.
        !treatAllNodesAsDirty
      ) {
        for (const child of nextNode.getChildren()) {
          if ($isElementNode(child)) {
            const childDom = getElementByKeyOrThrow(activeEditor$1, child.getKey());
            $setElementDirection(childDom, child);
          }
        }
      }
    }
  } else {
    const text = nextNode.getTextContent();
    if ($isDecoratorNode(nextNode)) {
      const decorator = nextNode.decorate(activeEditor$1, activeEditorConfig);
      if (decorator !== null) {
        reconcileDecorator(key, decorator);
      }
    }
    subTreeTextContent += text;
  }
  if (!activeEditorStateReadOnly && $isRootNode(nextNode) && nextNode.__cachedText !== subTreeTextContent) {
    const nextRootNode = nextNode.getWritable();
    nextRootNode.__cachedText = subTreeTextContent;
    nextNode = nextRootNode;
  }
  activeEditorDOMRenderConfig.$decorateDOM(nextNode, prevNode, dom, activeEditor$1);
  {
    Object.freeze(nextNode);
  }
  return dom;
}
function reconcileDecorator(key, decorator) {
  let pendingDecorators = activeEditor$1._pendingDecorators;
  const currentDecorators = activeEditor$1._decorators;
  if (pendingDecorators === null) {
    if (currentDecorators[key] === decorator) {
      return;
    }
    pendingDecorators = cloneDecorators(activeEditor$1);
  }
  pendingDecorators[key] = decorator;
}
function getNextSibling(element) {
  let nextSibling = element.nextSibling;
  if (nextSibling !== null && nextSibling === activeEditor$1._blockCursorElement) {
    nextSibling = nextSibling.nextSibling;
  }
  return nextSibling;
}
function childrenSet(children, start) {
  const s2 = /* @__PURE__ */ new Set();
  for (let i2 = start; i2 < children.length; i2++) {
    s2.add(children[i2]);
  }
  return s2;
}
function $reconcileNodeChildren(nextElement, prevChildren, nextChildren, prevChildrenLength, nextChildrenLength, slot) {
  const prevEndIndex = prevChildrenLength - 1;
  const nextEndIndex = nextChildrenLength - 1;
  let prevChildrenSet;
  let nextChildrenSet;
  let siblingDOM = slot.getFirstChild();
  let prevIndex = 0;
  let nextIndex = 0;
  while (prevIndex <= prevEndIndex && nextIndex <= nextEndIndex) {
    const prevKey = prevChildren[prevIndex];
    const nextKey = nextChildren[nextIndex];
    if (prevKey === nextKey) {
      siblingDOM = getNextSibling($reconcileNode(nextKey, slot.element));
      prevIndex++;
      nextIndex++;
    } else {
      if (nextChildrenSet === void 0) {
        nextChildrenSet = childrenSet(nextChildren, nextIndex);
      }
      if (prevChildrenSet === void 0) {
        prevChildrenSet = childrenSet(prevChildren, prevIndex);
      } else if (!prevChildrenSet.has(prevKey)) {
        prevIndex++;
        continue;
      }
      if (!nextChildrenSet.has(prevKey)) {
        siblingDOM = getNextSibling(getPrevElementByKeyOrThrow(prevKey));
        $destroyNode(prevKey, slot.element);
        prevIndex++;
        prevChildrenSet.delete(prevKey);
        continue;
      }
      if (!prevChildrenSet.has(nextKey)) {
        $createNode(nextKey, slot.withBefore(siblingDOM));
        nextIndex++;
      } else {
        const childDOM = getElementByKeyOrThrow(activeEditor$1, nextKey);
        if (childDOM !== siblingDOM) {
          slot.withBefore(siblingDOM).insertChild(childDOM);
        }
        siblingDOM = getNextSibling($reconcileNode(nextKey, slot.element));
        prevIndex++;
        nextIndex++;
      }
    }
    const node = activeNextNodeMap.get(nextKey);
    if (node !== null && $isTextNode(node)) {
      if (subTreeTextFormat === null) {
        subTreeTextFormat = node.getFormat();
        subTreeTextStyle = node.getStyle();
      }
    } else if (
      // inline $textContentRequiresDoubleLinebreakAtEnd
      $isElementNode(node) && nextIndex <= nextEndIndex && !node.isInline()
    ) {
      subTreeTextContent += DOUBLE_LINE_BREAK;
    }
  }
  const appendNewChildren = prevIndex > prevEndIndex;
  const removeOldChildren = nextIndex > nextEndIndex;
  if (appendNewChildren && !removeOldChildren) {
    const previousNode = nextChildren[nextEndIndex + 1];
    const insertDOM = previousNode === void 0 ? null : activeEditor$1.getElementByKey(previousNode);
    $createChildren(nextChildren, nextElement, nextIndex, nextEndIndex, slot.withBefore(insertDOM));
  } else if (removeOldChildren && !appendNewChildren) {
    $destroyChildren(prevChildren, prevIndex, prevEndIndex, slot.element);
  }
}
function $reconcileRoot(prevEditorState, nextEditorState, editor, dirtyType, dirtyElements, dirtyLeaves) {
  subTreeTextContent = "";
  treatAllNodesAsDirty = dirtyType === FULL_RECONCILE;
  activeEditor$1 = editor;
  activeEditorConfig = editor._config;
  activeEditorDOMRenderConfig = editor._config.dom || DEFAULT_EDITOR_DOM_CONFIG;
  activeEditorNodes = editor._nodes;
  activeMutationListeners = activeEditor$1._listeners.mutation;
  activeDirtyElements = dirtyElements;
  activeDirtyLeaves = dirtyLeaves;
  activePrevNodeMap = prevEditorState._nodeMap;
  activeNextNodeMap = nextEditorState._nodeMap;
  activeEditorStateReadOnly = nextEditorState._readOnly;
  activePrevKeyToDOMMap = new Map(editor._keyToDOMMap);
  const currentMutatedNodes = /* @__PURE__ */ new Map();
  mutatedNodes = currentMutatedNodes;
  $reconcileNode("root", null);
  activeEditor$1 = void 0;
  activeEditorNodes = void 0;
  activeDirtyElements = void 0;
  activeDirtyLeaves = void 0;
  activePrevNodeMap = void 0;
  activeNextNodeMap = void 0;
  activeEditorConfig = void 0;
  activePrevKeyToDOMMap = void 0;
  mutatedNodes = void 0;
  activeEditorDOMRenderConfig = DEFAULT_EDITOR_DOM_CONFIG;
  return currentMutatedNodes;
}
function storeDOMWithKey(key, dom, editor) {
  const keyToDOMMap = editor._keyToDOMMap;
  setNodeKeyOnDOMNode(dom, editor, key);
  keyToDOMMap.set(key, dom);
}
function getPrevElementByKeyOrThrow(key) {
  const element = activePrevKeyToDOMMap.get(key);
  if (element === void 0) {
    {
      formatDevErrorMessage(`Reconciliation: could not find DOM element for node key ${key}`);
    }
  }
  return element;
}
function warnOnlyOnce(message) {
  {
    let run = false;
    return () => {
      if (!run) {
        console.warn(message);
      }
      run = true;
    };
  }
}
// @__NO_SIDE_EFFECTS__
function createCommand(type) {
  return {
    type
  };
}
var SELECTION_CHANGE_COMMAND = /* @__PURE__ */ createCommand("SELECTION_CHANGE_COMMAND");
var SELECTION_INSERT_CLIPBOARD_NODES_COMMAND = /* @__PURE__ */ createCommand("SELECTION_INSERT_CLIPBOARD_NODES_COMMAND");
var CLICK_COMMAND = /* @__PURE__ */ createCommand("CLICK_COMMAND");
var BEFORE_INPUT_COMMAND = /* @__PURE__ */ createCommand("BEFORE_INPUT_COMMAND");
var INPUT_COMMAND = /* @__PURE__ */ createCommand("INPUT_COMMAND");
var COMPOSITION_START_COMMAND = /* @__PURE__ */ createCommand("COMPOSITION_START_COMMAND");
var COMPOSITION_END_COMMAND = /* @__PURE__ */ createCommand("COMPOSITION_END_COMMAND");
var DELETE_CHARACTER_COMMAND = /* @__PURE__ */ createCommand("DELETE_CHARACTER_COMMAND");
var INSERT_LINE_BREAK_COMMAND = /* @__PURE__ */ createCommand("INSERT_LINE_BREAK_COMMAND");
var INSERT_PARAGRAPH_COMMAND = /* @__PURE__ */ createCommand("INSERT_PARAGRAPH_COMMAND");
var CONTROLLED_TEXT_INSERTION_COMMAND = /* @__PURE__ */ createCommand("CONTROLLED_TEXT_INSERTION_COMMAND");
var PASTE_COMMAND = /* @__PURE__ */ createCommand("PASTE_COMMAND");
var REMOVE_TEXT_COMMAND = /* @__PURE__ */ createCommand("REMOVE_TEXT_COMMAND");
var DELETE_WORD_COMMAND = /* @__PURE__ */ createCommand("DELETE_WORD_COMMAND");
var DELETE_LINE_COMMAND = /* @__PURE__ */ createCommand("DELETE_LINE_COMMAND");
var FORMAT_TEXT_COMMAND = /* @__PURE__ */ createCommand("FORMAT_TEXT_COMMAND");
var UNDO_COMMAND = /* @__PURE__ */ createCommand("UNDO_COMMAND");
var REDO_COMMAND = /* @__PURE__ */ createCommand("REDO_COMMAND");
var KEY_DOWN_COMMAND = /* @__PURE__ */ createCommand("KEYDOWN_COMMAND");
var KEY_ARROW_RIGHT_COMMAND = /* @__PURE__ */ createCommand("KEY_ARROW_RIGHT_COMMAND");
var MOVE_TO_END = /* @__PURE__ */ createCommand("MOVE_TO_END");
var KEY_ARROW_LEFT_COMMAND = /* @__PURE__ */ createCommand("KEY_ARROW_LEFT_COMMAND");
var MOVE_TO_START = /* @__PURE__ */ createCommand("MOVE_TO_START");
var KEY_ARROW_UP_COMMAND = /* @__PURE__ */ createCommand("KEY_ARROW_UP_COMMAND");
var KEY_ARROW_DOWN_COMMAND = /* @__PURE__ */ createCommand("KEY_ARROW_DOWN_COMMAND");
var KEY_ENTER_COMMAND = /* @__PURE__ */ createCommand("KEY_ENTER_COMMAND");
var KEY_SPACE_COMMAND = /* @__PURE__ */ createCommand("KEY_SPACE_COMMAND");
var KEY_BACKSPACE_COMMAND = /* @__PURE__ */ createCommand("KEY_BACKSPACE_COMMAND");
var KEY_ESCAPE_COMMAND = /* @__PURE__ */ createCommand("KEY_ESCAPE_COMMAND");
var KEY_DELETE_COMMAND = /* @__PURE__ */ createCommand("KEY_DELETE_COMMAND");
var KEY_TAB_COMMAND = /* @__PURE__ */ createCommand("KEY_TAB_COMMAND");
var INSERT_TAB_COMMAND = /* @__PURE__ */ createCommand("INSERT_TAB_COMMAND");
var INDENT_CONTENT_COMMAND = /* @__PURE__ */ createCommand("INDENT_CONTENT_COMMAND");
var OUTDENT_CONTENT_COMMAND = /* @__PURE__ */ createCommand("OUTDENT_CONTENT_COMMAND");
var DROP_COMMAND = /* @__PURE__ */ createCommand("DROP_COMMAND");
var FORMAT_ELEMENT_COMMAND = /* @__PURE__ */ createCommand("FORMAT_ELEMENT_COMMAND");
var DRAGSTART_COMMAND = /* @__PURE__ */ createCommand("DRAGSTART_COMMAND");
var DRAGOVER_COMMAND = /* @__PURE__ */ createCommand("DRAGOVER_COMMAND");
var DRAGEND_COMMAND = /* @__PURE__ */ createCommand("DRAGEND_COMMAND");
var COPY_COMMAND = /* @__PURE__ */ createCommand("COPY_COMMAND");
var CUT_COMMAND = /* @__PURE__ */ createCommand("CUT_COMMAND");
var SELECT_ALL_COMMAND = /* @__PURE__ */ createCommand("SELECT_ALL_COMMAND");
var CLEAR_EDITOR_COMMAND = /* @__PURE__ */ createCommand("CLEAR_EDITOR_COMMAND");
var CLEAR_HISTORY_COMMAND = /* @__PURE__ */ createCommand("CLEAR_HISTORY_COMMAND");
var CAN_REDO_COMMAND = /* @__PURE__ */ createCommand("CAN_REDO_COMMAND");
var CAN_UNDO_COMMAND = /* @__PURE__ */ createCommand("CAN_UNDO_COMMAND");
var FOCUS_COMMAND = /* @__PURE__ */ createCommand("FOCUS_COMMAND");
var BLUR_COMMAND = /* @__PURE__ */ createCommand("BLUR_COMMAND");
var KEY_MODIFIER_COMMAND = /* @__PURE__ */ createCommand("KEY_MODIFIER_COMMAND");
var PASS_THROUGH_COMMAND = Object.freeze({});
var ANDROID_COMPOSITION_LATENCY = 30;
var rootElementEvents = [["keydown", onKeyDown], ["pointerdown", onPointerDown], ["compositionstart", onCompositionStart], ["compositionend", onCompositionEnd], ["input", onInput], ["click", onClick], ["cut", PASS_THROUGH_COMMAND], ["copy", PASS_THROUGH_COMMAND], ["dragstart", PASS_THROUGH_COMMAND], ["dragover", PASS_THROUGH_COMMAND], ["dragend", PASS_THROUGH_COMMAND], ["paste", PASS_THROUGH_COMMAND], ["focus", PASS_THROUGH_COMMAND], ["blur", PASS_THROUGH_COMMAND], ["drop", PASS_THROUGH_COMMAND]];
if (CAN_USE_BEFORE_INPUT) {
  rootElementEvents.push(["beforeinput", (event, editor) => onBeforeInput(event, editor)]);
}
var lastKeyDownTimeStamp = 0;
var lastKeyCode = null;
var lastBeforeInputInsertTextTimeStamp = 0;
var unprocessedBeforeInputData = null;
var rootElementToDocument = /* @__PURE__ */ new WeakMap();
var rootElementsRegistered = /* @__PURE__ */ new WeakMap();
var isSelectionChangeFromDOMUpdate = false;
var isSelectionChangeFromMouseDown = false;
var isInsertLineBreak = false;
var isFirefoxEndingComposition = false;
var isSafariEndingComposition = false;
var safariEndCompositionEventData = "";
var postDeleteSelectionToRestore = null;
var collapsedSelectionFormat = [0, "", 0, "root", 0];
function $shouldPreventDefaultAndInsertText(selection, domTargetRange, text, timeStamp, isBeforeInput) {
  const anchor = selection.anchor;
  const focus = selection.focus;
  const anchorNode = anchor.getNode();
  const editor = getActiveEditor();
  const domSelection = getDOMSelection(getWindow(editor));
  const domAnchorNode = domSelection !== null ? domSelection.anchorNode : null;
  const anchorKey = anchor.key;
  const backingAnchorElement = editor.getElementByKey(anchorKey);
  const textLength = text.length;
  return anchorKey !== focus.key || // If we're working with a non-text node.
  !$isTextNode(anchorNode) || // If we are replacing a range with a single character or grapheme, and not composing.
  (!isBeforeInput && (!CAN_USE_BEFORE_INPUT || // We check to see if there has been
  // a recent beforeinput event for "textInput". If there has been one in the last
  // 50ms then we proceed as normal. However, if there is not, then this is likely
  // a dangling `input` event caused by execCommand('insertText').
  lastBeforeInputInsertTextTimeStamp < timeStamp + 50) || anchorNode.isDirty() && textLength < 2 || // TODO consider if there are other scenarios when multiple code units
  //      should be addressed here
  doesContainSurrogatePair(text)) && anchor.offset !== focus.offset && !anchorNode.isComposing() || // Any non standard text node.
  $isTokenOrSegmented(anchorNode) || // If the text length is more than a single character and we're either
  // dealing with this in "beforeinput" or where the node has already recently
  // been changed (thus is dirty).
  anchorNode.isDirty() && textLength > 1 || // If the DOM selection element is not the same as the backing node during beforeinput.
  (isBeforeInput || !CAN_USE_BEFORE_INPUT) && backingAnchorElement !== null && !anchorNode.isComposing() && domAnchorNode !== getDOMTextNode(backingAnchorElement) || // If TargetRange is not the same as the DOM selection; browser trying to edit random parts
  // of the editor.
  domSelection !== null && domTargetRange !== null && (!domTargetRange.collapsed || domTargetRange.startContainer !== domSelection.anchorNode || domTargetRange.startOffset !== domSelection.anchorOffset) || // Check if we're changing from bold to italics, or some other format.
  !anchorNode.isComposing() && (anchorNode.getFormat() !== selection.format || anchorNode.getStyle() !== selection.style) || // One last set of heuristics to check against.
  $shouldInsertTextAfterOrBeforeTextNode(selection, anchorNode);
}
function shouldSkipSelectionChange(domNode, offset) {
  return isDOMTextNode(domNode) && domNode.nodeValue !== null && offset !== 0 && offset !== domNode.nodeValue.length;
}
function onSelectionChange(domSelection, editor, isActive) {
  const {
    anchorNode: anchorDOM,
    anchorOffset,
    focusNode: focusDOM,
    focusOffset
  } = domSelection;
  if (isSelectionChangeFromDOMUpdate) {
    isSelectionChangeFromDOMUpdate = false;
    if (shouldSkipSelectionChange(anchorDOM, anchorOffset) && shouldSkipSelectionChange(focusDOM, focusOffset) && !postDeleteSelectionToRestore) {
      return;
    }
  }
  updateEditorSync(editor, () => {
    if (!isActive) {
      $setSelection(null);
      return;
    }
    if (!isSelectionWithinEditor(editor, anchorDOM, focusDOM)) {
      return;
    }
    let selection = $getSelection();
    if (postDeleteSelectionToRestore && $isRangeSelection(selection) && selection.isCollapsed()) {
      const curAnchor = selection.anchor;
      const prevAnchor = postDeleteSelectionToRestore.anchor;
      if (
        // Rightward shift in same node
        curAnchor.key === prevAnchor.key && curAnchor.offset === prevAnchor.offset + 1 || // Or rightward shift into sibling node
        curAnchor.offset === 1 && prevAnchor.getNode().is(curAnchor.getNode().getPreviousSibling())
      ) {
        selection = postDeleteSelectionToRestore.clone();
        $setSelection(selection);
      }
    }
    postDeleteSelectionToRestore = null;
    if ($isRangeSelection(selection)) {
      const anchor = selection.anchor;
      const anchorNode = anchor.getNode();
      if (selection.isCollapsed()) {
        if (domSelection.type === "Range" && domSelection.anchorNode === domSelection.focusNode) {
          selection.dirty = true;
        }
        const windowEvent = getWindow(editor).event;
        const currentTimeStamp = windowEvent ? windowEvent.timeStamp : performance.now();
        const [lastFormat, lastStyle, lastOffset, lastKey, timeStamp] = collapsedSelectionFormat;
        const root = $getRoot();
        const isRootTextContentEmpty = editor.isComposing() === false && root.getTextContent() === "";
        if (currentTimeStamp < timeStamp + 200 && anchor.offset === lastOffset && anchor.key === lastKey) {
          $updateSelectionFormatStyle(selection, lastFormat, lastStyle);
        } else {
          if (anchor.type === "text") {
            if (!$isTextNode(anchorNode)) {
              formatDevErrorMessage(`Point.getNode() must return TextNode when type is text`);
            }
            $updateSelectionFormatStyleFromTextNode(selection, anchorNode);
          } else if (anchor.type === "element" && !isRootTextContentEmpty) {
            if (!$isElementNode(anchorNode)) {
              formatDevErrorMessage(`Point.getNode() must return ElementNode when type is element`);
            }
            const lastNode = anchor.getNode();
            if (
              // This previously applied to all ParagraphNode
              lastNode.isEmpty()
            ) {
              $updateSelectionFormatStyleFromElementNode(selection, lastNode);
            } else {
              $updateSelectionFormatStyle(selection, selection.format, "");
            }
          }
        }
      } else {
        const anchorKey = anchor.key;
        const focus = selection.focus;
        const focusKey = focus.key;
        const nodes = selection.getNodes();
        const nodesLength = nodes.length;
        const isBackward = selection.isBackward();
        const startOffset = isBackward ? focusOffset : anchorOffset;
        const endOffset = isBackward ? anchorOffset : focusOffset;
        const startKey = isBackward ? focusKey : anchorKey;
        const endKey = isBackward ? anchorKey : focusKey;
        let combinedFormat = IS_ALL_FORMATTING;
        let hasTextNodes = false;
        for (let i2 = 0; i2 < nodesLength; i2++) {
          const node = nodes[i2];
          const textContentSize = node.getTextContentSize();
          if ($isTextNode(node) && textContentSize !== 0 && // Exclude empty text nodes at boundaries resulting from user's selection
          !(i2 === 0 && node.__key === startKey && startOffset === textContentSize || i2 === nodesLength - 1 && node.__key === endKey && endOffset === 0)) {
            hasTextNodes = true;
            combinedFormat &= node.getFormat();
            if (combinedFormat === 0) {
              break;
            }
          }
        }
        selection.format = hasTextNodes ? combinedFormat : 0;
      }
    }
    dispatchCommand(editor, SELECTION_CHANGE_COMMAND, void 0);
  });
}
function $updateSelectionFormatStyle(selection, format, style) {
  if (selection.format !== format || selection.style !== style) {
    selection.format = format;
    selection.style = style;
    selection.dirty = true;
  }
}
function $updateSelectionFormatStyleFromTextNode(selection, node) {
  const format = node.getFormat();
  const style = node.getStyle();
  $updateSelectionFormatStyle(selection, format, style);
}
function $updateSelectionFormatStyleFromElementNode(selection, node) {
  const format = node.getTextFormat();
  const style = node.getTextStyle();
  $updateSelectionFormatStyle(selection, format, style);
}
function onClick(event, editor) {
  updateEditorSync(editor, () => {
    const selection = $getSelection();
    const domSelection = getDOMSelection(getWindow(editor));
    const lastSelection = $getPreviousSelection();
    if (domSelection) {
      if ($isRangeSelection(selection)) {
        const anchor = selection.anchor;
        const anchorNode = anchor.getNode();
        if (anchor.type === "element" && anchor.offset === 0 && selection.isCollapsed() && !$isRootNode(anchorNode) && $getRoot().getChildrenSize() === 1 && anchorNode.getTopLevelElementOrThrow().isEmpty() && lastSelection !== null && selection.is(lastSelection)) {
          domSelection.removeAllRanges();
          selection.dirty = true;
        } else if (event.detail === 3 && !selection.isCollapsed()) {
          const focus = selection.focus;
          const focusNode = focus.getNode();
          if (anchorNode !== focusNode) {
            const parentNode = $findMatchingParent(anchorNode, (node) => $isElementNode(node) && !node.isInline());
            if ($isElementNode(parentNode)) {
              parentNode.select(0);
            }
          }
        }
      } else if (event.pointerType === "touch" || event.pointerType === "pen") {
        const domAnchorNode = domSelection.anchorNode;
        if (isHTMLElement(domAnchorNode) || isDOMTextNode(domAnchorNode)) {
          const newSelection = $internalCreateRangeSelection(lastSelection, domSelection, editor, event);
          $setSelection(newSelection);
        }
      }
    }
    dispatchCommand(editor, CLICK_COMMAND, event);
  });
}
function onPointerDown(event, editor) {
  const target = event.target;
  const pointerType = event.pointerType;
  if (isDOMNode(target) && pointerType !== "touch" && pointerType !== "pen" && event.button === 0) {
    updateEditorSync(editor, () => {
      if (!$isSelectionCapturedInDecorator(target)) {
        isSelectionChangeFromMouseDown = true;
      }
    });
  }
}
function getTargetRange(event) {
  if (!event.getTargetRanges) {
    return null;
  }
  const targetRanges = event.getTargetRanges();
  if (targetRanges.length === 0) {
    return null;
  }
  return targetRanges[0];
}
function $canRemoveText(anchorNode, focusNode) {
  return anchorNode !== focusNode || $isElementNode(anchorNode) || $isElementNode(focusNode) || !$isTokenOrTab(anchorNode) || !$isTokenOrTab(focusNode);
}
function isPossiblyAndroidKeyPress(timeStamp) {
  return lastKeyCode === "MediaLast" && timeStamp < lastKeyDownTimeStamp + ANDROID_COMPOSITION_LATENCY;
}
function registerDefaultCommandHandlers(editor) {
  editor.registerCommand(BEFORE_INPUT_COMMAND, $handleBeforeInput, COMMAND_PRIORITY_EDITOR);
  editor.registerCommand(INPUT_COMMAND, $handleInput, COMMAND_PRIORITY_EDITOR);
  editor.registerCommand(COMPOSITION_START_COMMAND, $handleCompositionStart, COMMAND_PRIORITY_EDITOR);
  editor.registerCommand(COMPOSITION_END_COMMAND, $handleCompositionEnd, COMMAND_PRIORITY_EDITOR);
  editor.registerCommand(KEY_DOWN_COMMAND, $handleKeyDown, COMMAND_PRIORITY_EDITOR);
}
function onBeforeInput(event, editor) {
  const inputType = event.inputType;
  if (inputType === "deleteCompositionText" || // If we're pasting in FF, we shouldn't get this event
  // as the `paste` event should have triggered, unless the
  // user has dom.event.clipboardevents.enabled disabled in
  // about:config. In that case, we need to process the
  // pasted content in the DOM mutation phase.
  IS_FIREFOX && isFirefoxClipboardEvents(editor)) {
    return;
  } else if (inputType === "insertCompositionText") {
    return;
  }
  dispatchCommand(editor, BEFORE_INPUT_COMMAND, event);
}
function $handleBeforeInput(event) {
  const inputType = event.inputType;
  const targetRange = getTargetRange(event);
  const editor = getActiveEditor();
  const selection = $getSelection();
  if (inputType === "deleteContentBackward") {
    if (selection === null) {
      const prevSelection = $getPreviousSelection();
      if (!$isRangeSelection(prevSelection)) {
        return true;
      }
      $setSelection(prevSelection.clone());
    }
    if ($isRangeSelection(selection)) {
      const isSelectionAnchorSameAsFocus = selection.anchor.key === selection.focus.key;
      if (isPossiblyAndroidKeyPress(event.timeStamp) && editor.isComposing() && isSelectionAnchorSameAsFocus) {
        $setCompositionKey(null);
        lastKeyDownTimeStamp = 0;
        setTimeout(() => {
          updateEditorSync(editor, () => {
            $setCompositionKey(null);
          });
        }, ANDROID_COMPOSITION_LATENCY);
        if ($isRangeSelection(selection)) {
          const anchorNode2 = selection.anchor.getNode();
          anchorNode2.markDirty();
          if (!$isTextNode(anchorNode2)) {
            formatDevErrorMessage(`Anchor node must be a TextNode`);
          }
          $updateSelectionFormatStyleFromTextNode(selection, anchorNode2);
        }
      } else {
        $setCompositionKey(null);
        event.preventDefault();
        const selectedNode = selection.anchor.getNode();
        const selectedNodeText = selectedNode.getTextContent();
        const selectedNodeCanInsertTextAfter = selectedNode.canInsertTextAfter();
        const hasSelectedAllTextInNode = selection.anchor.offset === 0 && selection.focus.offset === selectedNodeText.length;
        let shouldLetBrowserHandleDelete = IS_ANDROID_CHROME && isSelectionAnchorSameAsFocus && !hasSelectedAllTextInNode && selectedNodeCanInsertTextAfter;
        if (shouldLetBrowserHandleDelete && selection.isCollapsed()) {
          shouldLetBrowserHandleDelete = !$isDecoratorNode($getAdjacentNode(selection.anchor, true));
        }
        if (!shouldLetBrowserHandleDelete) {
          dispatchCommand(editor, DELETE_CHARACTER_COMMAND, true);
          const selectionAfterDelete = $getSelection();
          if (IS_ANDROID_CHROME && $isRangeSelection(selectionAfterDelete) && selectionAfterDelete.isCollapsed()) {
            postDeleteSelectionToRestore = selectionAfterDelete;
            setTimeout(() => postDeleteSelectionToRestore = null);
          }
        }
      }
      return true;
    }
  }
  if (!$isRangeSelection(selection)) {
    return true;
  }
  const data = event.data;
  if (unprocessedBeforeInputData !== null) {
    $updateSelectedTextFromDOM(false, editor, unprocessedBeforeInputData);
  }
  if ((!selection.dirty || unprocessedBeforeInputData !== null) && selection.isCollapsed() && !$isRootNode(selection.anchor.getNode()) && targetRange !== null) {
    selection.applyDOMRange(targetRange);
  }
  unprocessedBeforeInputData = null;
  const anchor = selection.anchor;
  const focus = selection.focus;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  if (inputType === "insertText" || inputType === "insertTranspose") {
    if (data === "\n") {
      event.preventDefault();
      dispatchCommand(editor, INSERT_LINE_BREAK_COMMAND, false);
    } else if (data === DOUBLE_LINE_BREAK) {
      event.preventDefault();
      dispatchCommand(editor, INSERT_PARAGRAPH_COMMAND, void 0);
    } else if (data == null && event.dataTransfer) {
      const text = event.dataTransfer.getData("text/plain");
      event.preventDefault();
      selection.insertRawText(text);
    } else if (data != null && $shouldPreventDefaultAndInsertText(selection, targetRange, data, event.timeStamp, true)) {
      event.preventDefault();
      dispatchCommand(editor, CONTROLLED_TEXT_INSERTION_COMMAND, data);
    } else {
      unprocessedBeforeInputData = data;
    }
    lastBeforeInputInsertTextTimeStamp = event.timeStamp;
    return true;
  }
  event.preventDefault();
  switch (inputType) {
    case "insertFromYank":
    case "insertFromDrop":
    case "insertReplacementText": {
      dispatchCommand(editor, CONTROLLED_TEXT_INSERTION_COMMAND, event);
      break;
    }
    case "insertFromComposition": {
      $setCompositionKey(null);
      dispatchCommand(editor, CONTROLLED_TEXT_INSERTION_COMMAND, event);
      break;
    }
    case "insertLineBreak": {
      $setCompositionKey(null);
      dispatchCommand(editor, INSERT_LINE_BREAK_COMMAND, false);
      break;
    }
    case "insertParagraph": {
      $setCompositionKey(null);
      if (isInsertLineBreak && !IS_IOS) {
        isInsertLineBreak = false;
        dispatchCommand(editor, INSERT_LINE_BREAK_COMMAND, false);
      } else {
        dispatchCommand(editor, INSERT_PARAGRAPH_COMMAND, void 0);
      }
      break;
    }
    case "insertFromPaste":
    case "insertFromPasteAsQuotation": {
      dispatchCommand(editor, PASTE_COMMAND, event);
      break;
    }
    case "deleteByComposition": {
      if ($canRemoveText(anchorNode, focusNode)) {
        dispatchCommand(editor, REMOVE_TEXT_COMMAND, event);
      }
      break;
    }
    case "deleteByDrag": {
      $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
      dispatchCommand(editor, REMOVE_TEXT_COMMAND, event);
      break;
    }
    case "deleteByCut": {
      dispatchCommand(editor, REMOVE_TEXT_COMMAND, event);
      break;
    }
    case "deleteContent": {
      dispatchCommand(editor, DELETE_CHARACTER_COMMAND, false);
      break;
    }
    case "deleteWordBackward": {
      dispatchCommand(editor, DELETE_WORD_COMMAND, true);
      break;
    }
    case "deleteWordForward": {
      dispatchCommand(editor, DELETE_WORD_COMMAND, false);
      break;
    }
    case "deleteHardLineBackward":
    case "deleteSoftLineBackward": {
      dispatchCommand(editor, DELETE_LINE_COMMAND, true);
      break;
    }
    case "deleteContentForward":
    case "deleteHardLineForward":
    case "deleteSoftLineForward": {
      dispatchCommand(editor, DELETE_LINE_COMMAND, false);
      break;
    }
    case "formatStrikeThrough": {
      dispatchCommand(editor, FORMAT_TEXT_COMMAND, "strikethrough");
      break;
    }
    case "formatBold": {
      dispatchCommand(editor, FORMAT_TEXT_COMMAND, "bold");
      break;
    }
    case "formatItalic": {
      dispatchCommand(editor, FORMAT_TEXT_COMMAND, "italic");
      break;
    }
    case "formatUnderline": {
      dispatchCommand(editor, FORMAT_TEXT_COMMAND, "underline");
      break;
    }
    case "historyUndo": {
      dispatchCommand(editor, UNDO_COMMAND, void 0);
      break;
    }
    case "historyRedo": {
      dispatchCommand(editor, REDO_COMMAND, void 0);
      break;
    }
  }
  return true;
}
function onInput(event, editor) {
  event.stopPropagation();
  updateEditorSync(editor, () => {
    editor.dispatchCommand(INPUT_COMMAND, event);
  }, {
    event
  });
  unprocessedBeforeInputData = null;
}
function $handleInput(event) {
  if (isHTMLElement(event.target) && $isSelectionCapturedInDecorator(event.target)) {
    return true;
  }
  const editor = getActiveEditor();
  const selection = $getSelection();
  const data = event.data;
  const targetRange = getTargetRange(event);
  if (data != null && $isRangeSelection(selection) && $shouldPreventDefaultAndInsertText(selection, targetRange, data, event.timeStamp, false)) {
    if (isFirefoxEndingComposition) {
      $onCompositionEndImpl(editor, data);
      isFirefoxEndingComposition = false;
    }
    const anchor = selection.anchor;
    const anchorNode = anchor.getNode();
    const domSelection = getDOMSelection(getWindow(editor));
    if (domSelection === null) {
      return true;
    }
    const isBackward = selection.isBackward();
    const startOffset = isBackward ? selection.anchor.offset : selection.focus.offset;
    const endOffset = isBackward ? selection.focus.offset : selection.anchor.offset;
    if (!CAN_USE_BEFORE_INPUT || selection.isCollapsed() || !$isTextNode(anchorNode) || domSelection.anchorNode === null || anchorNode.getTextContent().slice(0, startOffset) + data + anchorNode.getTextContent().slice(startOffset + endOffset) !== getAnchorTextFromDOM(domSelection.anchorNode)) {
      dispatchCommand(editor, CONTROLLED_TEXT_INSERTION_COMMAND, data);
    }
    const textLength = data.length;
    if (IS_FIREFOX && textLength > 1 && event.inputType === "insertCompositionText" && !editor.isComposing()) {
      selection.anchor.offset -= textLength;
    }
    if (IS_ANDROID_CHROME && editor.isComposing()) {
      lastKeyDownTimeStamp = 0;
      $setCompositionKey(null);
    }
  } else {
    const characterData = data !== null ? data : void 0;
    $updateSelectedTextFromDOM(false, editor, characterData);
    if (isFirefoxEndingComposition) {
      $onCompositionEndImpl(editor, data || void 0);
      isFirefoxEndingComposition = false;
    }
  }
  $flushMutations();
  return true;
}
function onCompositionStart(event, editor) {
  dispatchCommand(editor, COMPOSITION_START_COMMAND, event);
}
function $handleCompositionStart(event) {
  const editor = getActiveEditor();
  const selection = $getSelection();
  if ($isRangeSelection(selection) && !editor.isComposing()) {
    const anchor = selection.anchor;
    const node = selection.anchor.getNode();
    $setCompositionKey(anchor.key);
    $addUpdateTag(COMPOSITION_START_TAG);
    if (
      // If it has been 30ms since the last keydown, then we should
      // apply the empty space heuristic. We can't do this for Safari,
      // as the keydown fires after composition start.
      event.timeStamp < lastKeyDownTimeStamp + ANDROID_COMPOSITION_LATENCY || // FF has issues around composing multibyte characters, so we also
      // need to invoke the empty space heuristic below.
      anchor.type === "element" || !selection.isCollapsed() || node.getFormat() !== selection.format || $isTextNode(node) && node.getStyle() !== selection.style
    ) {
      dispatchCommand(editor, CONTROLLED_TEXT_INSERTION_COMMAND, COMPOSITION_START_CHAR);
    }
  }
  return true;
}
function $handleCompositionEnd(event) {
  const editor = getActiveEditor();
  $onCompositionEndImpl(editor, event.data);
  $addUpdateTag(COMPOSITION_END_TAG);
  return true;
}
function $onCompositionEndImpl(editor, data) {
  const compositionKey = editor._compositionKey;
  $setCompositionKey(null);
  if (compositionKey !== null && data != null) {
    if (data === "") {
      const node = $getNodeByKey(compositionKey);
      const domElement = editor.getElementByKey(compositionKey);
      const textNode = getDOMTextNode(domElement);
      if (textNode !== null && textNode.nodeValue !== null && $isTextNode(node)) {
        const domSelection = getDOMSelection(getWindow(editor));
        let anchorOffset = null;
        let focusOffset = null;
        if (domSelection !== null && domSelection.anchorNode === textNode) {
          anchorOffset = domSelection.anchorOffset;
          focusOffset = domSelection.focusOffset;
        }
        $updateTextNodeFromDOMContent(node, textNode.nodeValue, anchorOffset, focusOffset, true);
      }
      return;
    } else if (data[data.length - 1] === "\n") {
      const selection = $getSelection();
      if ($isRangeSelection(selection) || $isNodeSelection(selection)) {
        if ($isRangeSelection(selection)) {
          const focus = selection.focus;
          selection.anchor.set(focus.key, focus.offset, focus.type);
        }
        dispatchCommand(editor, KEY_ENTER_COMMAND, null);
        return;
      }
    }
  }
  $updateSelectedTextFromDOM(true, editor, data);
}
function onCompositionEnd(event, editor) {
  if (IS_FIREFOX) {
    isFirefoxEndingComposition = true;
  } else if (!IS_IOS && (IS_SAFARI || IS_APPLE_WEBKIT)) {
    isSafariEndingComposition = true;
    safariEndCompositionEventData = event.data;
  } else {
    dispatchCommand(editor, COMPOSITION_END_COMMAND, event);
  }
}
function onKeyDown(event, editor) {
  lastKeyDownTimeStamp = event.timeStamp;
  lastKeyCode = event.key;
  if (editor.isComposing()) {
    return;
  }
  dispatchCommand(editor, KEY_DOWN_COMMAND, event);
}
function $handleKeyDown(event) {
  const editor = getActiveEditor();
  if (event.key == null) {
    return true;
  }
  if (isSafariEndingComposition) {
    if (isBackspace(event)) {
      updateEditorSync(editor, () => {
        $onCompositionEndImpl(editor, safariEndCompositionEventData);
      });
      isSafariEndingComposition = false;
      safariEndCompositionEventData = "";
      return true;
    }
    isSafariEndingComposition = false;
    safariEndCompositionEventData = "";
  }
  if (isMoveForward(event)) {
    dispatchCommand(editor, KEY_ARROW_RIGHT_COMMAND, event);
  } else if (isMoveToEnd(event)) {
    dispatchCommand(editor, MOVE_TO_END, event);
  } else if (isMoveBackward(event)) {
    dispatchCommand(editor, KEY_ARROW_LEFT_COMMAND, event);
  } else if (isMoveToStart(event)) {
    dispatchCommand(editor, MOVE_TO_START, event);
  } else if (isMoveUp(event)) {
    dispatchCommand(editor, KEY_ARROW_UP_COMMAND, event);
  } else if (isMoveDown(event)) {
    dispatchCommand(editor, KEY_ARROW_DOWN_COMMAND, event);
  } else if (isLineBreak(event)) {
    isInsertLineBreak = true;
    dispatchCommand(editor, KEY_ENTER_COMMAND, event);
  } else if (isSpace(event)) {
    dispatchCommand(editor, KEY_SPACE_COMMAND, event);
  } else if (isOpenLineBreak(event)) {
    event.preventDefault();
    isInsertLineBreak = true;
    dispatchCommand(editor, INSERT_LINE_BREAK_COMMAND, true);
  } else if (isParagraph(event)) {
    isInsertLineBreak = false;
    dispatchCommand(editor, KEY_ENTER_COMMAND, event);
  } else if (isDeleteBackward(event)) {
    if (isBackspace(event)) {
      dispatchCommand(editor, KEY_BACKSPACE_COMMAND, event);
    } else {
      event.preventDefault();
      dispatchCommand(editor, DELETE_CHARACTER_COMMAND, true);
    }
  } else if (isEscape(event)) {
    dispatchCommand(editor, KEY_ESCAPE_COMMAND, event);
  } else if (isDeleteForward(event)) {
    if (isDelete(event)) {
      dispatchCommand(editor, KEY_DELETE_COMMAND, event);
    } else {
      event.preventDefault();
      dispatchCommand(editor, DELETE_CHARACTER_COMMAND, false);
    }
  } else if (isDeleteWordBackward(event)) {
    event.preventDefault();
    dispatchCommand(editor, DELETE_WORD_COMMAND, true);
  } else if (isDeleteWordForward(event)) {
    event.preventDefault();
    dispatchCommand(editor, DELETE_WORD_COMMAND, false);
  } else if (isDeleteLineBackward(event)) {
    event.preventDefault();
    dispatchCommand(editor, DELETE_LINE_COMMAND, true);
  } else if (isDeleteLineForward(event)) {
    event.preventDefault();
    dispatchCommand(editor, DELETE_LINE_COMMAND, false);
  } else if (isBold(event)) {
    event.preventDefault();
    dispatchCommand(editor, FORMAT_TEXT_COMMAND, "bold");
  } else if (isUnderline(event)) {
    event.preventDefault();
    dispatchCommand(editor, FORMAT_TEXT_COMMAND, "underline");
  } else if (isItalic(event)) {
    event.preventDefault();
    dispatchCommand(editor, FORMAT_TEXT_COMMAND, "italic");
  } else if (isTab(event)) {
    dispatchCommand(editor, KEY_TAB_COMMAND, event);
  } else if (isUndo(event)) {
    event.preventDefault();
    dispatchCommand(editor, UNDO_COMMAND, void 0);
  } else if (isRedo(event)) {
    event.preventDefault();
    dispatchCommand(editor, REDO_COMMAND, void 0);
  } else {
    const prevSelection = editor._editorState._selection;
    if (prevSelection !== null && !$isRangeSelection(prevSelection)) {
      if (isCopy(event)) {
        event.preventDefault();
        dispatchCommand(editor, COPY_COMMAND, event);
      } else if (isCut(event)) {
        event.preventDefault();
        dispatchCommand(editor, CUT_COMMAND, event);
      } else if (isSelectAll(event)) {
        event.preventDefault();
        dispatchCommand(editor, SELECT_ALL_COMMAND, event);
      }
    } else if (isSelectAll(event)) {
      event.preventDefault();
      dispatchCommand(editor, SELECT_ALL_COMMAND, event);
    }
  }
  if (isModifier(event)) {
    editor.dispatchCommand(KEY_MODIFIER_COMMAND, event);
  }
  return true;
}
function getRootElementRemoveHandles(rootElement) {
  let eventHandles = rootElement.__lexicalEventHandles;
  if (eventHandles === void 0) {
    eventHandles = [];
    rootElement.__lexicalEventHandles = eventHandles;
  }
  return eventHandles;
}
var activeNestedEditorsMap = /* @__PURE__ */ new Map();
function onDocumentSelectionChange(event) {
  const domSelection = getDOMSelectionFromTarget(event.target);
  if (domSelection === null) {
    return;
  }
  const nextActiveEditor = getNearestEditorFromDOMNode(domSelection.anchorNode);
  if (nextActiveEditor === null) {
    return;
  }
  if (isSelectionChangeFromMouseDown) {
    isSelectionChangeFromMouseDown = false;
    updateEditorSync(nextActiveEditor, () => {
      const lastSelection = $getPreviousSelection();
      const domAnchorNode = domSelection.anchorNode;
      if (isHTMLElement(domAnchorNode) || isDOMTextNode(domAnchorNode)) {
        const newSelection = $internalCreateRangeSelection(lastSelection, domSelection, nextActiveEditor, event);
        $setSelection(newSelection);
      }
    });
  }
  const editors = getEditorsToPropagate(nextActiveEditor);
  const rootEditor = editors[editors.length - 1];
  const rootEditorKey = rootEditor._key;
  const activeNestedEditor = activeNestedEditorsMap.get(rootEditorKey);
  const prevActiveEditor = activeNestedEditor || rootEditor;
  if (prevActiveEditor !== nextActiveEditor) {
    onSelectionChange(domSelection, prevActiveEditor, false);
  }
  onSelectionChange(domSelection, nextActiveEditor, true);
  if (nextActiveEditor !== rootEditor) {
    activeNestedEditorsMap.set(rootEditorKey, nextActiveEditor);
  } else if (activeNestedEditor) {
    activeNestedEditorsMap.delete(rootEditorKey);
  }
}
function stopLexicalPropagation(event) {
  event._lexicalHandled = true;
}
function hasStoppedLexicalPropagation(event) {
  const stopped = event._lexicalHandled === true;
  return stopped;
}
function addRootElementEvents(rootElement, editor) {
  const doc = rootElement.ownerDocument;
  rootElementToDocument.set(rootElement, doc);
  const documentRootElementsCount = rootElementsRegistered.get(doc) ?? 0;
  if (documentRootElementsCount < 1) {
    doc.addEventListener("selectionchange", onDocumentSelectionChange);
  }
  rootElementsRegistered.set(doc, documentRootElementsCount + 1);
  rootElement.__lexicalEditor = editor;
  const removeHandles = getRootElementRemoveHandles(rootElement);
  for (let i2 = 0; i2 < rootElementEvents.length; i2++) {
    const [eventName, onEvent] = rootElementEvents[i2];
    const eventHandler = typeof onEvent === "function" ? (event) => {
      if (hasStoppedLexicalPropagation(event)) {
        return;
      }
      stopLexicalPropagation(event);
      if (editor.isEditable() || eventName === "click") {
        onEvent(event, editor);
      }
    } : (event) => {
      if (hasStoppedLexicalPropagation(event)) {
        return;
      }
      stopLexicalPropagation(event);
      const isEditable = editor.isEditable();
      switch (eventName) {
        case "cut":
          return isEditable && dispatchCommand(editor, CUT_COMMAND, event);
        case "copy":
          return dispatchCommand(editor, COPY_COMMAND, event);
        case "paste":
          return isEditable && dispatchCommand(editor, PASTE_COMMAND, event);
        case "dragstart":
          return isEditable && dispatchCommand(editor, DRAGSTART_COMMAND, event);
        case "dragover":
          return isEditable && dispatchCommand(editor, DRAGOVER_COMMAND, event);
        case "dragend":
          return isEditable && dispatchCommand(editor, DRAGEND_COMMAND, event);
        case "focus":
          return isEditable && dispatchCommand(editor, FOCUS_COMMAND, event);
        case "blur": {
          return isEditable && dispatchCommand(editor, BLUR_COMMAND, event);
        }
        case "drop":
          return isEditable && dispatchCommand(editor, DROP_COMMAND, event);
      }
    };
    rootElement.addEventListener(eventName, eventHandler);
    removeHandles.push(() => {
      rootElement.removeEventListener(eventName, eventHandler);
    });
  }
}
var rootElementNotRegisteredWarning = warnOnlyOnce("Root element not registered");
function removeRootElementEvents(rootElement) {
  const doc = rootElementToDocument.get(rootElement);
  if (doc === void 0) {
    rootElementNotRegisteredWarning();
    return;
  }
  const documentRootElementsCount = rootElementsRegistered.get(doc);
  if (documentRootElementsCount === void 0) {
    rootElementNotRegisteredWarning();
    return;
  }
  const newCount = documentRootElementsCount - 1;
  if (!(newCount >= 0)) {
    formatDevErrorMessage(`Root element count less than 0`);
  }
  rootElementToDocument.delete(rootElement);
  rootElementsRegistered.set(doc, newCount);
  if (newCount === 0) {
    doc.removeEventListener("selectionchange", onDocumentSelectionChange);
  }
  const editor = getEditorPropertyFromDOMNode(rootElement);
  if (isLexicalEditor(editor)) {
    cleanActiveNestedEditorsMap(editor);
    rootElement.__lexicalEditor = null;
  } else if (editor) {
    {
      formatDevErrorMessage(`Attempted to remove event handlers from a node that does not belong to this build of Lexical`);
    }
  }
  const removeHandles = getRootElementRemoveHandles(rootElement);
  for (let i2 = 0; i2 < removeHandles.length; i2++) {
    removeHandles[i2]();
  }
  rootElement.__lexicalEventHandles = [];
}
function cleanActiveNestedEditorsMap(editor) {
  if (editor._parentEditor !== null) {
    const editors = getEditorsToPropagate(editor);
    const rootEditor = editors[editors.length - 1];
    const rootEditorKey = rootEditor._key;
    if (activeNestedEditorsMap.get(rootEditorKey) === editor) {
      activeNestedEditorsMap.delete(rootEditorKey);
    }
  } else {
    activeNestedEditorsMap.delete(editor._key);
  }
}
function markSelectionChangeFromDOMUpdate() {
  isSelectionChangeFromDOMUpdate = true;
}
function markCollapsedSelectionFormat(format, style, offset, key, timeStamp) {
  collapsedSelectionFormat = [format, style, offset, key, timeStamp];
}
function $removeNode(nodeToRemove, restoreSelection, preserveEmptyParent) {
  errorOnReadOnly();
  const key = nodeToRemove.__key;
  const parent = nodeToRemove.getParent();
  if (parent === null) {
    return;
  }
  const selection = $maybeMoveChildrenSelectionToParent(nodeToRemove);
  let selectionMoved = false;
  if ($isRangeSelection(selection) && restoreSelection) {
    const anchor = selection.anchor;
    const focus = selection.focus;
    if (anchor.key === key) {
      moveSelectionPointToSibling(anchor, nodeToRemove, parent, nodeToRemove.getPreviousSibling(), nodeToRemove.getNextSibling());
      selectionMoved = true;
    }
    if (focus.key === key) {
      moveSelectionPointToSibling(focus, nodeToRemove, parent, nodeToRemove.getPreviousSibling(), nodeToRemove.getNextSibling());
      selectionMoved = true;
    }
  } else if ($isNodeSelection(selection) && restoreSelection && nodeToRemove.isSelected()) {
    nodeToRemove.selectPrevious();
  }
  if ($isRangeSelection(selection) && restoreSelection && !selectionMoved) {
    const index = nodeToRemove.getIndexWithinParent();
    removeFromParent(nodeToRemove);
    $updateElementSelectionOnCreateDeleteNode(selection, parent, index, -1);
  } else {
    removeFromParent(nodeToRemove);
  }
  if (!preserveEmptyParent && !$isRootOrShadowRoot(parent) && !parent.canBeEmpty() && parent.isEmpty()) {
    $removeNode(parent, restoreSelection);
  }
  if (restoreSelection && selection && $isRootNode(parent) && parent.isEmpty()) {
    parent.selectEnd();
  }
}
function buildImportMap(importMap) {
  return importMap;
}
var EPHEMERAL = /* @__PURE__ */ Symbol.for("ephemeral");
function $isEphemeral(node) {
  return node[EPHEMERAL] || false;
}
function $markEphemeral(node) {
  node[EPHEMERAL] = true;
  return node;
}
var LexicalNode = class {
  /** @internal Allow us to look up the type including static props */
  /** @internal */
  __type;
  /** @internal */
  //@ts-ignore We set the key in the constructor.
  __key;
  /** @internal */
  __parent;
  /** @internal */
  __prev;
  /** @internal */
  __next;
  /** @internal */
  __state;
  // Flow doesn't support abstract classes unfortunately, so we can't _force_
  // subclasses of Node to implement statics. All subclasses of Node should have
  // a static getType and clone method though. We define getType and clone here so we can call it
  // on any  Node, and we throw this error by default since the subclass should provide
  // their own implementation.
  /**
   * Returns the string type of this node. Every node must
   * implement this and it MUST BE UNIQUE amongst nodes registered
   * on the editor.
   *
   */
  static getType() {
    const {
      ownNodeType
    } = getStaticNodeConfig(this);
    if (!(ownNodeType !== void 0)) {
      formatDevErrorMessage(`LexicalNode: Node ${this.name} does not implement .getType().`);
    }
    return ownNodeType;
  }
  /**
   * Clones this node, creating a new node with a different key
   * and adding it to the EditorState (but not attaching it anywhere!). All nodes must
   * implement this method.
   *
   */
  static clone(_data) {
    {
      formatDevErrorMessage(`LexicalNode: Node ${this.name} does not implement .clone().`);
    }
  }
  /**
   * Override this to implement the new static node configuration protocol,
   * this method is called directly on the prototype and must not depend
   * on anything initialized in the constructor. Generally it should be
   * a trivial implementation.
   *
   * @example
   * ```ts
   * class MyNode extends TextNode {
   *   $config() {
   *     return this.config('my-node', {extends: TextNode});
   *   }
   * }
   * ```
   */
  $config() {
    return {};
  }
  /**
   * This is a convenience method for $config that
   * aids in type inference. See {@link LexicalNode.$config}
   * for example usage.
   */
  config(type, config) {
    const parentKlass = config.extends || Object.getPrototypeOf(this.constructor);
    Object.assign(config, {
      extends: parentKlass,
      type
    });
    return {
      [type]: config
    };
  }
  /**
   * Perform any state updates on the clone of prevNode that are not already
   * handled by the constructor call in the static clone method. If you have
   * state to update in your clone that is not handled directly by the
   * constructor, it is advisable to override this method but it is required
   * to include a call to `super.afterCloneFrom(prevNode)` in your
   * implementation. This is only intended to be called by
   * {@link $cloneWithProperties} function or via a super call.
   *
   * @example
   * ```ts
   * class ClassesTextNode extends TextNode {
   *   // Not shown: static getType, static importJSON, exportJSON, createDOM, updateDOM
   *   __classes = new Set<string>();
   *   static clone(node: ClassesTextNode): ClassesTextNode {
   *     // The inherited TextNode constructor is used here, so
   *     // classes is not set by this method.
   *     return new ClassesTextNode(node.__text, node.__key);
   *   }
   *   afterCloneFrom(node: this): void {
   *     // This calls TextNode.afterCloneFrom and LexicalNode.afterCloneFrom
   *     // for necessary state updates
   *     super.afterCloneFrom(node);
   *     this.__addClasses(node.__classes);
   *   }
   *   // This method is a private implementation detail, it is not
   *   // suitable for the public API because it does not call getWritable
   *   __addClasses(classNames: Iterable<string>): this {
   *     for (const className of classNames) {
   *       this.__classes.add(className);
   *     }
   *     return this;
   *   }
   *   addClass(...classNames: string[]): this {
   *     return this.getWritable().__addClasses(classNames);
   *   }
   *   removeClass(...classNames: string[]): this {
   *     const node = this.getWritable();
   *     for (const className of classNames) {
   *       this.__classes.delete(className);
   *     }
   *     return this;
   *   }
   *   getClasses(): Set<string> {
   *     return this.getLatest().__classes;
   *   }
   * }
   * ```
   *
   */
  afterCloneFrom(prevNode) {
    if (this.__key === prevNode.__key) {
      this.__parent = prevNode.__parent;
      this.__next = prevNode.__next;
      this.__prev = prevNode.__prev;
      this.__state = prevNode.__state;
    } else if (prevNode.__state) {
      this.__state = prevNode.__state.getWritable(this);
    }
  }
  /**
   * Reset state in this copy of originalNode, if necessary
   *
   * @param originalNode
   */
  resetOnCopyNodeFrom(originalNode) {
    if (this.__state) {
      this.__state = this.__state.getWritable(this).resetOnCopyNode();
    }
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  static importDOM;
  constructor(key) {
    this.__type = this.constructor.getType();
    this.__parent = null;
    this.__prev = null;
    this.__next = null;
    Object.defineProperty(this, "__state", {
      configurable: true,
      enumerable: false,
      value: void 0,
      writable: true
    });
    $setNodeKey(this, key);
    {
      if (this.__type !== "root") {
        errorOnTypeKlassMismatch(this.__type, this.constructor);
      }
    }
  }
  // Getters and Traversers
  /**
   * Returns the string type of this node.
   */
  getType() {
    return this.__type;
  }
  isInline() {
    {
      formatDevErrorMessage(`LexicalNode: Node ${this.constructor.name} does not implement .isInline().`);
    }
  }
  /**
   * Returns true if there is a path between this node and the RootNode, false otherwise.
   * This is a way of determining if the node is "attached" EditorState. Unattached nodes
   * won't be reconciled and will ultimately be cleaned up by the Lexical GC.
   */
  isAttached() {
    let nodeKey = this.__key;
    while (nodeKey !== null) {
      if (nodeKey === "root") {
        return true;
      }
      const node = $getNodeByKey(nodeKey);
      if (node === null) {
        break;
      }
      nodeKey = node.__parent;
    }
    return false;
  }
  /**
   * Returns true if this node is contained within the provided Selection., false otherwise.
   * Relies on the algorithms implemented in {@link BaseSelection.getNodes} to determine
   * what's included.
   *
   * @param selection - The selection that we want to determine if the node is in.
   */
  isSelected(selection) {
    const targetSelection = selection || $getSelection();
    if (targetSelection == null) {
      return false;
    }
    const isSelected = targetSelection.getNodes().some((n2) => n2.__key === this.__key);
    if ($isTextNode(this)) {
      return isSelected;
    }
    const isElementRangeSelection = $isRangeSelection(targetSelection) && targetSelection.anchor.type === "element" && targetSelection.focus.type === "element";
    if (isElementRangeSelection) {
      if (targetSelection.isCollapsed()) {
        return false;
      }
      const parentNode = this.getParent();
      if ($isDecoratorNode(this) && this.isInline() && parentNode) {
        const firstPoint = targetSelection.isBackward() ? targetSelection.focus : targetSelection.anchor;
        if (parentNode.is(firstPoint.getNode()) && firstPoint.offset === parentNode.getChildrenSize() && this.is(parentNode.getLastChild())) {
          return false;
        }
      }
    }
    return isSelected;
  }
  /**
   * Returns this nodes key.
   */
  getKey() {
    return this.__key;
  }
  /**
   * Returns the zero-based index of this node within the parent.
   */
  getIndexWithinParent() {
    const parent = this.getParent();
    if (parent === null) {
      return -1;
    }
    let node = parent.getFirstChild();
    let index = 0;
    while (node !== null) {
      if (this.is(node)) {
        return index;
      }
      index++;
      node = node.getNextSibling();
    }
    return -1;
  }
  /**
   * Returns the parent of this node, or null if none is found.
   */
  getParent() {
    const parent = this.getLatest().__parent;
    if (parent === null) {
      return null;
    }
    return $getNodeByKey(parent);
  }
  /**
   * Returns the parent of this node, or throws if none is found.
   */
  getParentOrThrow() {
    const parent = this.getParent();
    if (parent === null) {
      {
        formatDevErrorMessage(`Expected node ${this.__key} to have a parent.`);
      }
    }
    return parent;
  }
  /**
   * Returns the highest (in the EditorState tree)
   * non-root ancestor of this node, or null if none is found. See {@link lexical!$isRootOrShadowRoot}
   * for more information on which Elements comprise "roots".
   */
  getTopLevelElement() {
    let node = this;
    while (node !== null) {
      const parent = node.getParent();
      if ($isRootOrShadowRoot(parent)) {
        if (!($isElementNode(node) || node === this && $isDecoratorNode(node))) {
          formatDevErrorMessage(`Children of root nodes must be elements or decorators`);
        }
        return node;
      }
      node = parent;
    }
    return null;
  }
  /**
   * Returns the highest (in the EditorState tree)
   * non-root ancestor of this node, or throws if none is found. See {@link lexical!$isRootOrShadowRoot}
   * for more information on which Elements comprise "roots".
   */
  getTopLevelElementOrThrow() {
    const parent = this.getTopLevelElement();
    if (parent === null) {
      {
        formatDevErrorMessage(`Expected node ${this.__key} to have a top parent element.`);
      }
    }
    return parent;
  }
  /**
   * Returns a list of the every ancestor of this node,
   * all the way up to the RootNode.
   *
   */
  getParents() {
    const parents = [];
    let node = this.getParent();
    while (node !== null) {
      parents.push(node);
      node = node.getParent();
    }
    return parents;
  }
  /**
   * Returns a list of the keys of every ancestor of this node,
   * all the way up to the RootNode.
   *
   */
  getParentKeys() {
    const parents = [];
    let node = this.getParent();
    while (node !== null) {
      parents.push(node.__key);
      node = node.getParent();
    }
    return parents;
  }
  /**
   * Returns the "previous" siblings - that is, the node that comes
   * before this one in the same parent.
   *
   */
  getPreviousSibling() {
    const self2 = this.getLatest();
    const prevKey = self2.__prev;
    return prevKey === null ? null : $getNodeByKey(prevKey);
  }
  /**
   * Returns the "previous" siblings - that is, the nodes that come between
   * this one and the first child of it's parent, inclusive.
   *
   */
  getPreviousSiblings() {
    const siblings = [];
    const parent = this.getParent();
    if (parent === null) {
      return siblings;
    }
    let node = parent.getFirstChild();
    while (node !== null) {
      if (node.is(this)) {
        break;
      }
      siblings.push(node);
      node = node.getNextSibling();
    }
    return siblings;
  }
  /**
   * Returns the "next" siblings - that is, the node that comes
   * after this one in the same parent
   *
   */
  getNextSibling() {
    const self2 = this.getLatest();
    const nextKey = self2.__next;
    return nextKey === null ? null : $getNodeByKey(nextKey);
  }
  /**
   * Returns all "next" siblings - that is, the nodes that come between this
   * one and the last child of it's parent, inclusive.
   *
   */
  getNextSiblings() {
    const siblings = [];
    let node = this.getNextSibling();
    while (node !== null) {
      siblings.push(node);
      node = node.getNextSibling();
    }
    return siblings;
  }
  /**
   * @deprecated use {@link $getCommonAncestor}
   *
   * Returns the closest common ancestor of this node and the provided one or null
   * if one cannot be found.
   *
   * @param node - the other node to find the common ancestor of.
   */
  getCommonAncestor(node) {
    const a2 = $isElementNode(this) ? this : this.getParent();
    const b2 = $isElementNode(node) ? node : node.getParent();
    const result = a2 && b2 ? $getCommonAncestor(a2, b2) : null;
    return result ? result.commonAncestor : null;
  }
  /**
   * Returns true if the provided node is the exact same one as this node, from Lexical's perspective.
   * Always use this instead of referential equality.
   *
   * @param object - the node to perform the equality comparison on.
   */
  is(object) {
    if (object == null) {
      return false;
    }
    return this.__key === object.__key;
  }
  /**
   * Returns true if this node logically precedes the target node in the
   * editor state, false otherwise (including if there is no common ancestor).
   *
   * Note that this notion of isBefore is based on post-order; a descendant
   * node is always before its ancestors. See also
   * {@link $getCommonAncestor} and {@link $comparePointCaretNext} for
   * more flexible ways to determine the relative positions of nodes.
   *
   * @param targetNode - the node we're testing to see if it's after this one.
   */
  isBefore(targetNode) {
    const compare = $getCommonAncestor(this, targetNode);
    if (compare === null) {
      return false;
    }
    if (compare.type === "descendant") {
      return true;
    }
    if (compare.type === "branch") {
      return $getCommonAncestorResultBranchOrder(compare) === -1;
    }
    if (!(compare.type === "same" || compare.type === "ancestor")) {
      formatDevErrorMessage(`LexicalNode.isBefore: exhaustiveness check`);
    }
    return false;
  }
  /**
   * Returns true if this node is an ancestor of and distinct from the target node, false otherwise.
   *
   * @param targetNode - the would-be child node.
   */
  isParentOf(targetNode) {
    const result = $getCommonAncestor(this, targetNode);
    return result !== null && result.type === "ancestor";
  }
  // TO-DO: this function can be simplified a lot
  /**
   * Returns a list of nodes that are between this node and
   * the target node in the EditorState.
   *
   * @param targetNode - the node that marks the other end of the range of nodes to be returned.
   */
  getNodesBetween(targetNode) {
    const isBefore = this.isBefore(targetNode);
    const nodes = [];
    const visited = /* @__PURE__ */ new Set();
    let node = this;
    while (true) {
      if (node === null) {
        break;
      }
      const key = node.__key;
      if (!visited.has(key)) {
        visited.add(key);
        nodes.push(node);
      }
      if (node === targetNode) {
        break;
      }
      const child = $isElementNode(node) ? isBefore ? node.getFirstChild() : node.getLastChild() : null;
      if (child !== null) {
        node = child;
        continue;
      }
      const nextSibling = isBefore ? node.getNextSibling() : node.getPreviousSibling();
      if (nextSibling !== null) {
        node = nextSibling;
        continue;
      }
      const parent = node.getParentOrThrow();
      if (!visited.has(parent.__key)) {
        nodes.push(parent);
      }
      if (parent === targetNode) {
        break;
      }
      let parentSibling = null;
      let ancestor = parent;
      do {
        if (ancestor === null) {
          {
            formatDevErrorMessage(`getNodesBetween: ancestor is null`);
          }
        }
        parentSibling = isBefore ? ancestor.getNextSibling() : ancestor.getPreviousSibling();
        ancestor = ancestor.getParent();
        if (ancestor !== null) {
          if (parentSibling === null && !visited.has(ancestor.__key)) {
            nodes.push(ancestor);
          }
        } else {
          break;
        }
      } while (parentSibling === null);
      node = parentSibling;
    }
    if (!isBefore) {
      nodes.reverse();
    }
    return nodes;
  }
  /**
   * Returns true if this node has been marked dirty during this update cycle.
   *
   */
  isDirty() {
    const editor = getActiveEditor();
    const dirtyLeaves = editor._dirtyLeaves;
    return dirtyLeaves !== null && dirtyLeaves.has(this.__key);
  }
  /**
   * Returns the latest version of the node from the active EditorState.
   * This is used to avoid getting values from stale node references.
   *
   */
  getLatest() {
    if ($isEphemeral(this)) {
      return this;
    }
    const latest = $getNodeByKey(this.__key);
    if (latest === null) {
      {
        formatDevErrorMessage(`Lexical node does not exist in active editor state. Avoid using the same node references between nested closures from editorState.read/editor.update.`);
      }
    }
    return latest;
  }
  /**
   * Returns a mutable version of the node using {@link $cloneWithProperties}
   * if necessary. Will throw an error if called outside of a Lexical Editor
   * {@link LexicalEditor.update} callback.
   *
   */
  getWritable() {
    if ($isEphemeral(this)) {
      return this;
    }
    errorOnReadOnly();
    const editorState = getActiveEditorState();
    const editor = getActiveEditor();
    const nodeMap = editorState._nodeMap;
    const key = this.__key;
    const latestNode = this.getLatest();
    const cloneNotNeeded = editor._cloneNotNeeded;
    const selection = $getSelection();
    if (selection !== null) {
      selection.setCachedNodes(null);
    }
    if (cloneNotNeeded.has(key)) {
      internalMarkNodeAsDirty(latestNode);
      return latestNode;
    }
    const mutableNode = $cloneWithProperties(latestNode);
    cloneNotNeeded.add(key);
    internalMarkNodeAsDirty(mutableNode);
    nodeMap.set(key, mutableNode);
    return mutableNode;
  }
  /**
   * Returns the text content of the node. Override this for
   * custom nodes that should have a representation in plain text
   * format (for copy + paste, for example)
   *
   */
  getTextContent() {
    return "";
  }
  /**
   * Returns the length of the string produced by calling getTextContent on this node.
   *
   */
  getTextContentSize() {
    return this.getTextContent().length;
  }
  // View
  /**
   * Called during the reconciliation process to determine which nodes
   * to insert into the DOM for this Lexical Node.
   *
   * This method must return exactly one HTMLElement. Nested elements are not supported.
   *
   * Do not attempt to update the Lexical EditorState during this phase of the update lifecycle.
   *
   * @param _config - allows access to things like the EditorTheme (to apply classes) during reconciliation.
   * @param _editor - allows access to the editor for context during reconciliation.
   *
   * */
  createDOM(_config, _editor) {
    {
      formatDevErrorMessage(`createDOM: base method not extended`);
    }
  }
  /**
   * Called when a node changes and should update the DOM
   * in whatever way is necessary to make it align with any changes that might
   * have happened during the update.
   *
   * Returning "true" here will cause lexical to unmount and recreate the DOM node
   * (by calling createDOM). You would need to do this if the element tag changes,
   * for instance.
   *
   * */
  updateDOM(_prevNode, _dom, _config) {
    {
      formatDevErrorMessage(`updateDOM: base method not extended`);
    }
  }
  /**
   * Controls how the this node is serialized to HTML. This is important for
   * copy and paste between Lexical and non-Lexical editors, or Lexical editors with different namespaces,
   * in which case the primary transfer format is HTML. It's also important if you're serializing
   * to HTML for any other reason via {@link @lexical/html!$generateHtmlFromNodes}. You could
   * also use this method to build your own HTML renderer.
   *
   * */
  exportDOM(editor) {
    const element = this.createDOM(editor._config, editor);
    return {
      element
    };
  }
  /**
   * Controls how the this node is serialized to JSON. This is important for
   * copy and paste between Lexical editors sharing the same namespace. It's also important
   * if you're serializing to JSON for persistent storage somewhere.
   * See [Serialization & Deserialization](https://lexical.dev/docs/concepts/serialization#lexical---html).
   *
   * */
  exportJSON() {
    const state = this.__state ? this.__state.toJSON() : void 0;
    return {
      type: this.__type,
      version: 1,
      ...state
    };
  }
  /**
   * Controls how the this node is deserialized from JSON. This is usually boilerplate,
   * but provides an abstraction between the node implementation and serialized interface that can
   * be important if you ever make breaking changes to a node schema (by adding or removing properties).
   * See [Serialization & Deserialization](https://lexical.dev/docs/concepts/serialization#lexical---html).
   *
   * */
  static importJSON(_serializedNode) {
    {
      formatDevErrorMessage(`LexicalNode: Node ${this.name} does not implement .importJSON().`);
    }
  }
  /**
   * Update this LexicalNode instance from serialized JSON. It's recommended
   * to implement as much logic as possible in this method instead of the
   * static importJSON method, so that the functionality can be inherited in subclasses.
   *
   * The LexicalUpdateJSON utility type should be used to ignore any type, version,
   * or children properties in the JSON so that the extended JSON from subclasses
   * are acceptable parameters for the super call.
   *
   * If overridden, this method must call super.
   *
   * @example
   * ```ts
   * class MyTextNode extends TextNode {
   *   // ...
   *   static importJSON(serializedNode: SerializedMyTextNode): MyTextNode {
   *     return $createMyTextNode()
   *       .updateFromJSON(serializedNode);
   *   }
   *   updateFromJSON(
   *     serializedNode: LexicalUpdateJSON<SerializedMyTextNode>,
   *   ): this {
   *     return super.updateFromJSON(serializedNode)
   *       .setMyProperty(serializedNode.myProperty);
   *   }
   * }
   * ```
   **/
  updateFromJSON(serializedNode) {
    return $updateStateFromJSON(this, serializedNode);
  }
  /**
   * @experimental
   *
   * Registers the returned function as a transform on the node during
   * Editor initialization. Most such use cases should be addressed via
   * the {@link LexicalEditor.registerNodeTransform} API.
   *
   * Experimental - use at your own risk.
   */
  static transform() {
    return null;
  }
  // Setters and mutators
  /**
   * Removes this LexicalNode from the EditorState. If the node isn't re-inserted
   * somewhere, the Lexical garbage collector will eventually clean it up.
   *
   * @param preserveEmptyParent - If falsy, the node's parent will be removed if
   * it's empty after the removal operation. This is the default behavior, subject to
   * other node heuristics such as {@link ElementNode#canBeEmpty}
   * */
  remove(preserveEmptyParent) {
    $removeNode(this, true, preserveEmptyParent);
  }
  /**
   * Replaces this LexicalNode with the provided node, optionally transferring the children
   * of the replaced node to the replacing node.
   *
   * @param replaceWith - The node to replace this one with.
   * @param includeChildren - Whether or not to transfer the children of this node to the replacing node.
   * */
  replace(replaceWith, includeChildren) {
    errorOnReadOnly();
    let selection = $getSelection();
    if (selection !== null) {
      selection = selection.clone();
    }
    errorOnInsertTextNodeOnRoot(this, replaceWith);
    const self2 = this.getLatest();
    const toReplaceKey = this.__key;
    const key = replaceWith.__key;
    const writableReplaceWith = replaceWith.getWritable();
    const writableParent = this.getParentOrThrow().getWritable();
    const size = writableParent.__size;
    removeFromParent(writableReplaceWith);
    const prevSibling = self2.getPreviousSibling();
    const nextSibling = self2.getNextSibling();
    const prevKey = self2.__prev;
    const nextKey = self2.__next;
    const parentKey = self2.__parent;
    $removeNode(self2, false, true);
    if (prevSibling === null) {
      writableParent.__first = key;
    } else {
      const writablePrevSibling = prevSibling.getWritable();
      writablePrevSibling.__next = key;
    }
    writableReplaceWith.__prev = prevKey;
    if (nextSibling === null) {
      writableParent.__last = key;
    } else {
      const writableNextSibling = nextSibling.getWritable();
      writableNextSibling.__prev = key;
    }
    writableReplaceWith.__next = nextKey;
    writableReplaceWith.__parent = parentKey;
    writableParent.__size = size;
    if (includeChildren) {
      if (!($isElementNode(this) && $isElementNode(writableReplaceWith))) {
        formatDevErrorMessage(`includeChildren should only be true for ElementNodes`);
      }
      this.getChildren().forEach((child) => {
        writableReplaceWith.append(child);
      });
    }
    if ($isRangeSelection(selection)) {
      $setSelection(selection);
      const anchor = selection.anchor;
      const focus = selection.focus;
      if (anchor.key === toReplaceKey) {
        $moveSelectionPointToEnd(anchor, writableReplaceWith);
      }
      if (focus.key === toReplaceKey) {
        $moveSelectionPointToEnd(focus, writableReplaceWith);
      }
    }
    if ($getCompositionKey() === toReplaceKey) {
      $setCompositionKey(key);
    }
    return writableReplaceWith;
  }
  /**
   * Inserts a node after this LexicalNode (as the next sibling).
   *
   * @param nodeToInsert - The node to insert after this one.
   * @param restoreSelection - Whether or not to attempt to resolve the
   * selection to the appropriate place after the operation is complete.
   * */
  insertAfter(nodeToInsert, restoreSelection = true) {
    errorOnReadOnly();
    errorOnInsertTextNodeOnRoot(this, nodeToInsert);
    const writableSelf = this.getWritable();
    const writableNodeToInsert = nodeToInsert.getWritable();
    const oldParent = writableNodeToInsert.getParent();
    const selection = $getSelection();
    let elementAnchorSelectionOnNode = false;
    let elementFocusSelectionOnNode = false;
    if (oldParent !== null) {
      const oldIndex = nodeToInsert.getIndexWithinParent();
      removeFromParent(writableNodeToInsert);
      if ($isRangeSelection(selection)) {
        const oldParentKey = oldParent.__key;
        const anchor = selection.anchor;
        const focus = selection.focus;
        elementAnchorSelectionOnNode = anchor.type === "element" && anchor.key === oldParentKey && anchor.offset === oldIndex + 1;
        elementFocusSelectionOnNode = focus.type === "element" && focus.key === oldParentKey && focus.offset === oldIndex + 1;
      }
    }
    const nextSibling = this.getNextSibling();
    const writableParent = this.getParentOrThrow().getWritable();
    const insertKey = writableNodeToInsert.__key;
    const nextKey = writableSelf.__next;
    if (nextSibling === null) {
      writableParent.__last = insertKey;
    } else {
      const writableNextSibling = nextSibling.getWritable();
      writableNextSibling.__prev = insertKey;
    }
    writableParent.__size++;
    writableSelf.__next = insertKey;
    writableNodeToInsert.__next = nextKey;
    writableNodeToInsert.__prev = writableSelf.__key;
    writableNodeToInsert.__parent = writableSelf.__parent;
    if (restoreSelection && $isRangeSelection(selection)) {
      const index = this.getIndexWithinParent();
      $updateElementSelectionOnCreateDeleteNode(selection, writableParent, index + 1);
      const writableParentKey = writableParent.__key;
      if (elementAnchorSelectionOnNode) {
        selection.anchor.set(writableParentKey, index + 2, "element");
      }
      if (elementFocusSelectionOnNode) {
        selection.focus.set(writableParentKey, index + 2, "element");
      }
    }
    return nodeToInsert;
  }
  /**
   * Inserts a node before this LexicalNode (as the previous sibling).
   *
   * @param nodeToInsert - The node to insert before this one.
   * @param restoreSelection - Whether or not to attempt to resolve the
   * selection to the appropriate place after the operation is complete.
   * */
  insertBefore(nodeToInsert, restoreSelection = true) {
    errorOnReadOnly();
    errorOnInsertTextNodeOnRoot(this, nodeToInsert);
    const writableSelf = this.getWritable();
    const writableNodeToInsert = nodeToInsert.getWritable();
    const insertKey = writableNodeToInsert.__key;
    removeFromParent(writableNodeToInsert);
    const prevSibling = this.getPreviousSibling();
    const writableParent = this.getParentOrThrow().getWritable();
    const prevKey = writableSelf.__prev;
    const index = this.getIndexWithinParent();
    if (prevSibling === null) {
      writableParent.__first = insertKey;
    } else {
      const writablePrevSibling = prevSibling.getWritable();
      writablePrevSibling.__next = insertKey;
    }
    writableParent.__size++;
    writableSelf.__prev = insertKey;
    writableNodeToInsert.__prev = prevKey;
    writableNodeToInsert.__next = writableSelf.__key;
    writableNodeToInsert.__parent = writableSelf.__parent;
    const selection = $getSelection();
    if (restoreSelection && $isRangeSelection(selection)) {
      const parent = this.getParentOrThrow();
      $updateElementSelectionOnCreateDeleteNode(selection, parent, index);
    }
    return nodeToInsert;
  }
  /**
   * Whether or not this node has a required parent. Used during copy + paste operations
   * to normalize nodes that would otherwise be orphaned. For example, ListItemNodes without
   * a ListNode parent or TextNodes with a ParagraphNode parent.
   *
   * */
  isParentRequired() {
    return false;
  }
  /**
   * The creation logic for any required parent. Should be implemented if {@link isParentRequired} returns true.
   *
   * */
  createParentElementNode() {
    return $createParagraphNode();
  }
  selectStart() {
    return this.selectPrevious();
  }
  selectEnd() {
    return this.selectNext(0, 0);
  }
  /**
   * Moves selection to the previous sibling of this node, at the specified offsets.
   *
   * @param anchorOffset - The anchor offset for selection.
   * @param focusOffset -  The focus offset for selection
   * */
  selectPrevious(anchorOffset, focusOffset) {
    errorOnReadOnly();
    const prevSibling = this.getPreviousSibling();
    const parent = this.getParentOrThrow();
    if (prevSibling === null) {
      return parent.select(0, 0);
    }
    if ($isElementNode(prevSibling)) {
      return prevSibling.select();
    } else if (!$isTextNode(prevSibling)) {
      const index = prevSibling.getIndexWithinParent() + 1;
      return parent.select(index, index);
    }
    return prevSibling.select(anchorOffset, focusOffset);
  }
  /**
   * Moves selection to the next sibling of this node, at the specified offsets.
   *
   * @param anchorOffset - The anchor offset for selection.
   * @param focusOffset -  The focus offset for selection
   * */
  selectNext(anchorOffset, focusOffset) {
    errorOnReadOnly();
    const nextSibling = this.getNextSibling();
    const parent = this.getParentOrThrow();
    if (nextSibling === null) {
      return parent.select();
    }
    if ($isElementNode(nextSibling)) {
      return nextSibling.select(0, 0);
    } else if (!$isTextNode(nextSibling)) {
      const index = nextSibling.getIndexWithinParent();
      return parent.select(index, index);
    }
    return nextSibling.select(anchorOffset, focusOffset);
  }
  /**
   * Marks a node dirty, triggering transforms and
   * forcing it to be reconciled during the update cycle.
   *
   * */
  markDirty() {
    this.getWritable();
  }
  /**
   * @internal
   *
   * When the reconciler detects that a node was mutated, this method
   * may be called to restore the node to a known good state.
   */
  reconcileObservedMutation(dom, editor) {
    this.markDirty();
  }
};
function errorOnTypeKlassMismatch(type, klass) {
  const registeredNode = getRegisteredNode(getActiveEditor(), type);
  if (registeredNode === void 0) {
    {
      formatDevErrorMessage(`Create node: Attempted to create node ${klass.name} that was not configured to be used on the editor.`);
    }
  }
  const editorKlass = registeredNode.klass;
  if (editorKlass !== klass) {
    {
      formatDevErrorMessage(`Create node: Type ${type} in node ${klass.name} does not match registered node ${editorKlass.name} with the same type`);
    }
  }
}
function insertRangeAfter(node, firstToInsert, lastToInsert) {
  const lastToInsert2 = firstToInsert.getParentOrThrow().getLastChild();
  let current = firstToInsert;
  const nodesToInsert = [firstToInsert];
  while (current !== lastToInsert2) {
    if (!current.getNextSibling()) {
      {
        formatDevErrorMessage(`insertRangeAfter: lastToInsert must be a later sibling of firstToInsert`);
      }
    }
    current = current.getNextSibling();
    nodesToInsert.push(current);
  }
  let currentNode = node;
  for (const nodeToInsert of nodesToInsert) {
    currentNode = currentNode.insertAfter(nodeToInsert);
  }
}
function $isLexicalNode(node) {
  return node instanceof LexicalNode;
}
var HISTORIC_TAG = "historic";
var HISTORY_PUSH_TAG = "history-push";
var HISTORY_MERGE_TAG = "history-merge";
var PASTE_TAG = "paste";
var COLLABORATION_TAG = "collaboration";
var SKIP_COLLAB_TAG = "skip-collab";
var SKIP_SCROLL_INTO_VIEW_TAG = "skip-scroll-into-view";
var SKIP_DOM_SELECTION_TAG = "skip-dom-selection";
var SKIP_SELECTION_FOCUS_TAG = "skip-selection-focus";
var FOCUS_TAG = "focus";
var COMPOSITION_START_TAG = "composition-start";
var COMPOSITION_END_TAG = "composition-end";
var IMPORTANT_REG_EXP = /\s*!important\s*$/i;
function getStyleObjectFromCSS(css) {
  const styles = {};
  if (!css) {
    return styles;
  }
  let currentProperty = "";
  let currentValue = "";
  let currentQuote = null;
  let inComment = false;
  let isEscaped = false;
  let isParsingValue = false;
  let parenthesisDepth = 0;
  for (let i2 = 0; i2 < css.length; i2++) {
    const char = css[i2];
    if (inComment) {
      if (char === "*" && css[i2 + 1] === "/") {
        inComment = false;
        i2++;
      }
      continue;
    }
    if (isEscaped) {
      if (isParsingValue) {
        currentValue += char;
      } else {
        currentProperty += char;
      }
      isEscaped = false;
      continue;
    }
    if (currentQuote !== null) {
      if (isParsingValue) {
        currentValue += char;
      } else {
        currentProperty += char;
      }
      if (char === "\\") {
        isEscaped = true;
      } else if (char === currentQuote) {
        currentQuote = null;
      }
      continue;
    }
    if (char === "/" && css[i2 + 1] === "*") {
      inComment = true;
      i2++;
      continue;
    }
    if (char === '"' || char === "'") {
      currentQuote = char;
      if (isParsingValue) {
        currentValue += char;
      } else {
        currentProperty += char;
      }
      continue;
    }
    if (char === "(") {
      parenthesisDepth++;
      if (isParsingValue) {
        currentValue += char;
      } else {
        currentProperty += char;
      }
      continue;
    }
    if (char === ")") {
      parenthesisDepth = Math.max(0, parenthesisDepth - 1);
      if (isParsingValue) {
        currentValue += char;
      } else {
        currentProperty += char;
      }
      continue;
    }
    if (!isParsingValue && char === ":" && parenthesisDepth === 0) {
      isParsingValue = true;
      continue;
    }
    if (char === ";" && parenthesisDepth === 0) {
      const property2 = currentProperty.trim();
      const value2 = currentValue.trim();
      if (property2 !== "" && value2 !== "") {
        styles[property2] = value2;
      }
      currentProperty = "";
      currentValue = "";
      isParsingValue = false;
      continue;
    }
    if (isParsingValue) {
      currentValue += char;
    } else {
      currentProperty += char;
    }
  }
  const property = currentProperty.trim();
  const value = currentValue.trim();
  if (property !== "" && value !== "") {
    styles[property] = value;
  }
  return styles;
}
function setDOMStyleProperty(domStyle, property, value) {
  const priority = IMPORTANT_REG_EXP.test(value) ? "important" : "";
  const nextValue = priority === "" ? value : value.replace(IMPORTANT_REG_EXP, "").trim();
  domStyle.setProperty(property, nextValue, priority);
}
function setDOMStyleObject(domStyle, styleObject) {
  for (const property in styleObject) {
    const value = styleObject[property];
    if (value == null) {
      domStyle.removeProperty(property);
    } else {
      setDOMStyleProperty(domStyle, property, value);
    }
  }
}
function setDOMStyleFromCSS(domStyle, cssText, prevCSSText = "") {
  if (cssText === prevCSSText) {
    return;
  }
  const prevCSS = getStyleObjectFromCSS(prevCSSText);
  const nextCSS = getStyleObjectFromCSS(cssText);
  for (const property in nextCSS) {
    delete prevCSS[property];
    setDOMStyleProperty(domStyle, property, nextCSS[property]);
  }
  for (const property in prevCSS) {
    domStyle.removeProperty(property);
  }
}
var LineBreakNode = class _LineBreakNode extends LexicalNode {
  /** @internal */
  static getType() {
    return "linebreak";
  }
  static clone(node) {
    return new _LineBreakNode(node.__key);
  }
  constructor(key) {
    super(key);
  }
  getTextContent() {
    return "\n";
  }
  createDOM() {
    return document.createElement("br");
  }
  updateDOM() {
    return false;
  }
  isInline() {
    return true;
  }
  static importDOM() {
    return {
      br: (node) => {
        if (isOnlyChildInBlockNode(node) || isLastChildInBlockNode(node)) {
          return null;
        }
        return {
          conversion: $convertLineBreakElement,
          priority: 0
        };
      }
    };
  }
  static importJSON(serializedLineBreakNode) {
    return $createLineBreakNode().updateFromJSON(serializedLineBreakNode);
  }
};
function $convertLineBreakElement(node) {
  return {
    node: $createLineBreakNode()
  };
}
function $createLineBreakNode() {
  return $applyNodeReplacement(new LineBreakNode());
}
function $isLineBreakNode(node) {
  return node instanceof LineBreakNode;
}
function isOnlyChildInBlockNode(node) {
  const parentElement = node.parentElement;
  if (parentElement !== null && isBlockDomNode(parentElement)) {
    const firstChild = parentElement.firstChild;
    if (firstChild === node || firstChild.nextSibling === node && isWhitespaceDomTextNode(firstChild)) {
      const lastChild = parentElement.lastChild;
      if (lastChild === node || lastChild.previousSibling === node && isWhitespaceDomTextNode(lastChild)) {
        return true;
      }
    }
  }
  return false;
}
function isLastChildInBlockNode(node) {
  const parentElement = node.parentElement;
  if (parentElement !== null && isBlockDomNode(parentElement)) {
    const firstChild = parentElement.firstChild;
    if (firstChild === node || firstChild.nextSibling === node && isWhitespaceDomTextNode(firstChild)) {
      return false;
    }
    const lastChild = parentElement.lastChild;
    if (lastChild === node || lastChild.previousSibling === node && isWhitespaceDomTextNode(lastChild)) {
      return true;
    }
  }
  return false;
}
function isWhitespaceDomTextNode(node) {
  return isDOMTextNode(node) && /^( |\t|\r?\n)+$/.test(node.textContent || "");
}
function getElementOuterTag(node, format) {
  if (format & IS_CODE) {
    return "code";
  }
  if (format & IS_HIGHLIGHT) {
    return "mark";
  }
  if (format & IS_SUBSCRIPT) {
    return "sub";
  }
  if (format & IS_SUPERSCRIPT) {
    return "sup";
  }
  return null;
}
function getElementInnerTag(node, format) {
  if (format & IS_BOLD) {
    return "strong";
  }
  if (format & IS_ITALIC) {
    return "em";
  }
  return "span";
}
function setTextThemeClassNames(tag, prevFormat, nextFormat, dom, textClassNames) {
  const domClassList = dom.classList;
  let classNames = getCachedClassNameArray(textClassNames, "base");
  if (classNames !== void 0) {
    domClassList.add(...classNames);
  }
  classNames = getCachedClassNameArray(textClassNames, "underlineStrikethrough");
  let hasUnderlineStrikethrough = false;
  const prevUnderlineStrikethrough = prevFormat & IS_UNDERLINE && prevFormat & IS_STRIKETHROUGH;
  const nextUnderlineStrikethrough = nextFormat & IS_UNDERLINE && nextFormat & IS_STRIKETHROUGH;
  if (classNames !== void 0) {
    if (nextUnderlineStrikethrough) {
      hasUnderlineStrikethrough = true;
      if (!prevUnderlineStrikethrough) {
        domClassList.add(...classNames);
      }
    } else if (prevUnderlineStrikethrough) {
      domClassList.remove(...classNames);
    }
  }
  for (const key in TEXT_TYPE_TO_FORMAT) {
    const format = key;
    const flag = TEXT_TYPE_TO_FORMAT[format];
    classNames = getCachedClassNameArray(textClassNames, key);
    if (classNames !== void 0) {
      if (nextFormat & flag) {
        if (hasUnderlineStrikethrough && (key === "underline" || key === "strikethrough")) {
          if (prevFormat & flag) {
            domClassList.remove(...classNames);
          }
          continue;
        }
        if ((prevFormat & flag) === 0 || prevUnderlineStrikethrough && key === "underline" || key === "strikethrough") {
          domClassList.add(...classNames);
        }
      } else if (prevFormat & flag) {
        domClassList.remove(...classNames);
      }
    }
  }
}
function diffComposedText(a2, b2) {
  const aLength = a2.length;
  const bLength = b2.length;
  let left = 0;
  let right = 0;
  while (left < aLength && left < bLength && a2[left] === b2[left]) {
    left++;
  }
  while (right + left < aLength && right + left < bLength && a2[aLength - right - 1] === b2[bLength - right - 1]) {
    right++;
  }
  return [left, aLength - left - right, b2.slice(left, bLength - right)];
}
function setTextContent(nextText, dom, node) {
  const firstChild = dom.firstChild;
  const isComposing = node.isComposing();
  const suffix = isComposing ? COMPOSITION_SUFFIX : "";
  const text = nextText + suffix;
  if (firstChild == null) {
    dom.textContent = text;
  } else {
    const nodeValue = firstChild.nodeValue;
    if (nodeValue !== text) {
      if (isComposing || IS_FIREFOX) {
        const [index, remove, insert] = diffComposedText(nodeValue, text);
        if (remove !== 0) {
          firstChild.deleteData(index, remove);
        }
        firstChild.insertData(index, insert);
      } else {
        firstChild.nodeValue = text;
      }
    }
  }
}
function createTextInnerDOM(innerDOM, node, innerTag, format, text, config) {
  setTextContent(text, innerDOM, node);
  const theme = config.theme;
  const textClassNames = theme.text;
  if (textClassNames !== void 0) {
    setTextThemeClassNames(innerTag, 0, format, innerDOM, textClassNames);
  }
}
function wrapElementWith(element, tag) {
  const el = document.createElement(tag);
  el.appendChild(element);
  return el;
}
var TextNode = class _TextNode extends LexicalNode {
  /** @internal */
  __text;
  /** @internal */
  __format;
  /** @internal */
  __style;
  /** @internal */
  __mode;
  /** @internal */
  __detail;
  static getType() {
    return "text";
  }
  static clone(node) {
    return new _TextNode(node.__text, node.__key);
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__text = prevNode.__text;
    this.__format = prevNode.__format;
    this.__style = prevNode.__style;
    this.__mode = prevNode.__mode;
    this.__detail = prevNode.__detail;
  }
  constructor(text = "", key) {
    super(key);
    this.__text = text;
    this.__format = 0;
    this.__style = "";
    this.__mode = 0;
    this.__detail = 0;
  }
  /**
   * Returns a 32-bit integer that represents the TextFormatTypes currently applied to the
   * TextNode. You probably don't want to use this method directly - consider using TextNode.hasFormat instead.
   *
   * @returns a number representing the format of the text node.
   */
  getFormat() {
    const self2 = this.getLatest();
    return self2.__format;
  }
  /**
   * Returns a 32-bit integer that represents the TextDetailTypes currently applied to the
   * TextNode. You probably don't want to use this method directly - consider using TextNode.isDirectionless
   * or TextNode.isUnmergeable instead.
   *
   * @returns a number representing the detail of the text node.
   */
  getDetail() {
    const self2 = this.getLatest();
    return self2.__detail;
  }
  /**
   * Returns the mode (TextModeType) of the TextNode, which may be "normal", "token", or "segmented"
   *
   * @returns TextModeType.
   */
  getMode() {
    const self2 = this.getLatest();
    return TEXT_TYPE_TO_MODE[self2.__mode];
  }
  /**
   * Returns the styles currently applied to the node. This is analogous to CSSText in the DOM.
   *
   * @returns CSSText-like string of styles applied to the underlying DOM node.
   */
  getStyle() {
    const self2 = this.getLatest();
    return self2.__style;
  }
  /**
   * Returns whether or not the node is in "token" mode. TextNodes in token mode can be navigated through character-by-character
   * with a RangeSelection, but are deleted as a single entity (not individually by character).
   *
   * @returns true if the node is in token mode, false otherwise.
   */
  isToken() {
    const self2 = this.getLatest();
    return self2.__mode === IS_TOKEN;
  }
  /**
   *
   * @returns true if Lexical detects that an IME or other 3rd-party script is attempting to
   * mutate the TextNode, false otherwise.
   */
  isComposing() {
    return this.__key === $getCompositionKey();
  }
  /**
   * Returns whether or not the node is in "segmented" mode. TextNodes in segmented mode can be navigated through character-by-character
   * with a RangeSelection, but are deleted in space-delimited "segments".
   *
   * @returns true if the node is in segmented mode, false otherwise.
   */
  isSegmented() {
    const self2 = this.getLatest();
    return self2.__mode === IS_SEGMENTED;
  }
  /**
   * Returns whether or not the node is "directionless". Directionless nodes don't respect changes between RTL and LTR modes.
   *
   * @returns true if the node is directionless, false otherwise.
   */
  isDirectionless() {
    const self2 = this.getLatest();
    return (self2.__detail & IS_DIRECTIONLESS) !== 0;
  }
  /**
   * Returns whether or not the node is unmergeable. In some scenarios, Lexical tries to merge
   * adjacent TextNodes into a single TextNode. If a TextNode is unmergeable, this won't happen.
   *
   * @returns true if the node is unmergeable, false otherwise.
   */
  isUnmergeable() {
    const self2 = this.getLatest();
    return (self2.__detail & IS_UNMERGEABLE) !== 0;
  }
  /**
   * Returns whether or not the node has the provided format applied. Use this with the human-readable TextFormatType
   * string values to get the format of a TextNode.
   *
   * @param type - the TextFormatType to check for.
   *
   * @returns true if the node has the provided format, false otherwise.
   */
  hasFormat(type) {
    const formatFlag = TEXT_TYPE_TO_FORMAT[type];
    return (this.getFormat() & formatFlag) !== 0;
  }
  /**
   * Returns whether or not the node is simple text. Simple text is defined as a TextNode that has the string type "text"
   * (i.e., not a subclass) and has no mode applied to it (i.e., not segmented or token).
   *
   * @returns true if the node is simple text, false otherwise.
   */
  isSimpleText() {
    return this.__type === "text" && this.__mode === 0;
  }
  /**
   * Returns the text content of the node as a string.
   *
   * @returns a string representing the text content of the node.
   */
  getTextContent() {
    const self2 = this.getLatest();
    return self2.__text;
  }
  /**
   * Returns the format flags applied to the node as a 32-bit integer.
   *
   * @returns a number representing the TextFormatTypes applied to the node.
   */
  getFormatFlags(type, alignWithFormat) {
    const self2 = this.getLatest();
    const format = self2.__format;
    return toggleTextFormatType(format, type, alignWithFormat);
  }
  /**
   *
   * @returns true if the text node supports font styling, false otherwise.
   */
  canHaveFormat() {
    return true;
  }
  /**
   * @returns true if the text node is inline, false otherwise.
   */
  isInline() {
    return true;
  }
  // View
  createDOM(config, editor) {
    const format = this.__format;
    const outerTag = getElementOuterTag(this, format);
    const innerTag = getElementInnerTag(this, format);
    const tag = outerTag === null ? innerTag : outerTag;
    const dom = document.createElement(tag);
    let innerDOM = dom;
    if (this.hasFormat("code")) {
      dom.setAttribute("spellcheck", "false");
    }
    if (outerTag !== null) {
      innerDOM = document.createElement(innerTag);
      dom.appendChild(innerDOM);
    }
    const text = this.__text;
    createTextInnerDOM(innerDOM, this, innerTag, format, text, config);
    const style = this.__style;
    if (style !== "") {
      setDOMStyleFromCSS(dom.style, style);
    }
    return dom;
  }
  updateDOM(prevNode, dom, config) {
    const nextText = this.__text;
    const prevFormat = prevNode.__format;
    const nextFormat = this.__format;
    const prevOuterTag = getElementOuterTag(this, prevFormat);
    const nextOuterTag = getElementOuterTag(this, nextFormat);
    const prevInnerTag = getElementInnerTag(this, prevFormat);
    const nextInnerTag = getElementInnerTag(this, nextFormat);
    const prevTag = prevOuterTag === null ? prevInnerTag : prevOuterTag;
    const nextTag = nextOuterTag === null ? nextInnerTag : nextOuterTag;
    if (prevTag !== nextTag) {
      return true;
    }
    if (prevOuterTag === nextOuterTag && prevInnerTag !== nextInnerTag) {
      const prevInnerDOM = dom.firstChild;
      if (prevInnerDOM == null) {
        {
          formatDevErrorMessage(`updateDOM: prevInnerDOM is null or undefined`);
        }
      }
      const nextInnerDOM = document.createElement(nextInnerTag);
      createTextInnerDOM(nextInnerDOM, this, nextInnerTag, nextFormat, nextText, config);
      dom.replaceChild(nextInnerDOM, prevInnerDOM);
      return false;
    }
    let innerDOM = dom;
    if (nextOuterTag !== null) {
      if (prevOuterTag !== null) {
        innerDOM = dom.firstChild;
        if (innerDOM == null) {
          {
            formatDevErrorMessage(`updateDOM: innerDOM is null or undefined`);
          }
        }
      }
    }
    setTextContent(nextText, innerDOM, this);
    const theme = config.theme;
    const textClassNames = theme.text;
    if (textClassNames !== void 0 && prevFormat !== nextFormat) {
      setTextThemeClassNames(nextInnerTag, prevFormat, nextFormat, innerDOM, textClassNames);
    }
    const prevStyle = prevNode.__style;
    const nextStyle = this.__style;
    if (prevStyle !== nextStyle) {
      setDOMStyleFromCSS(dom.style, nextStyle, prevStyle);
    }
    return false;
  }
  static importDOM() {
    return {
      "#text": () => ({
        conversion: $convertTextDOMNode,
        priority: 0
      }),
      b: () => ({
        conversion: convertBringAttentionToElement,
        priority: 0
      }),
      code: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      em: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      i: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      mark: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      s: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      span: () => ({
        conversion: convertSpanElement,
        priority: 0
      }),
      strong: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      sub: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      sup: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      }),
      u: () => ({
        conversion: convertTextFormatElement,
        priority: 0
      })
    };
  }
  static importJSON(serializedNode) {
    return $createTextNode().updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setTextContent(serializedNode.text).setFormat(serializedNode.format).setDetail(serializedNode.detail).setMode(serializedNode.mode).setStyle(serializedNode.style);
  }
  // This improves Lexical's basic text output in copy+paste plus
  // for headless mode where people might use Lexical to generate
  // HTML content and not have the ability to use CSS classes.
  exportDOM(editor) {
    let {
      element
    } = super.exportDOM(editor);
    if (!isHTMLElement(element)) {
      formatDevErrorMessage(`Expected TextNode createDOM to always return a HTMLElement`);
    }
    element.style.whiteSpace = "pre-wrap";
    if (this.hasFormat("lowercase")) {
      element.style.textTransform = "lowercase";
    } else if (this.hasFormat("uppercase")) {
      element.style.textTransform = "uppercase";
    } else if (this.hasFormat("capitalize")) {
      element.style.textTransform = "capitalize";
    }
    if (this.hasFormat("bold")) {
      element = wrapElementWith(element, "b");
    }
    if (this.hasFormat("italic")) {
      element = wrapElementWith(element, "i");
    }
    if (this.hasFormat("strikethrough")) {
      element = wrapElementWith(element, "s");
    }
    if (this.hasFormat("underline")) {
      element = wrapElementWith(element, "u");
    }
    return {
      element
    };
  }
  exportJSON() {
    return {
      detail: this.getDetail(),
      format: this.getFormat(),
      mode: this.getMode(),
      style: this.getStyle(),
      text: this.getTextContent(),
      // As an exception here we invoke super at the end for historical reasons.
      // Namely, to preserve the order of the properties and not to break the tests
      // that use the serialized string representation.
      ...super.exportJSON()
    };
  }
  // Mutators
  selectionTransform(prevSelection, nextSelection) {
    return;
  }
  /**
   * Sets the node format to the provided TextFormatType or 32-bit integer. Note that the TextFormatType
   * version of the argument can only specify one format and doing so will remove all other formats that
   * may be applied to the node. For toggling behavior, consider using {@link TextNode.toggleFormat}
   *
   * @param format - TextFormatType or 32-bit integer representing the node format.
   *
   * @returns this TextNode.
   * // TODO 0.12 This should just be a `string`.
   */
  setFormat(format) {
    const self2 = this.getWritable();
    self2.__format = typeof format === "string" ? TEXT_TYPE_TO_FORMAT[format] : format;
    return self2;
  }
  /**
   * Sets the node detail to the provided TextDetailType or 32-bit integer. Note that the TextDetailType
   * version of the argument can only specify one detail value and doing so will remove all other detail values that
   * may be applied to the node. For toggling behavior, consider using {@link TextNode.toggleDirectionless}
   * or {@link TextNode.toggleUnmergeable}
   *
   * @param detail - TextDetailType or 32-bit integer representing the node detail.
   *
   * @returns this TextNode.
   * // TODO 0.12 This should just be a `string`.
   */
  setDetail(detail) {
    const self2 = this.getWritable();
    self2.__detail = typeof detail === "string" ? DETAIL_TYPE_TO_DETAIL[detail] : detail;
    return self2;
  }
  /**
   * Sets the node style to the provided CSSText-like string. Set this property as you
   * would an HTMLElement style attribute to apply inline styles to the underlying DOM Element.
   *
   * @param style - CSSText to be applied to the underlying HTMLElement.
   *
   * @returns this TextNode.
   */
  setStyle(style) {
    const self2 = this.getWritable();
    self2.__style = style;
    return self2;
  }
  /**
   * Applies the provided format to this TextNode if it's not present. Removes it if it's present.
   * The subscript and superscript formats are mutually exclusive.
   * Prefer using this method to turn specific formats on and off.
   *
   * @param type - TextFormatType to toggle.
   *
   * @returns this TextNode.
   */
  toggleFormat(type) {
    const format = this.getFormat();
    const newFormat = toggleTextFormatType(format, type, null);
    return this.setFormat(newFormat);
  }
  /**
   * Toggles the directionless detail value of the node. Prefer using this method over setDetail.
   *
   * @returns this TextNode.
   */
  toggleDirectionless() {
    const self2 = this.getWritable();
    self2.__detail ^= IS_DIRECTIONLESS;
    return self2;
  }
  /**
   * Toggles the unmergeable detail value of the node. Prefer using this method over setDetail.
   *
   * @returns this TextNode.
   */
  toggleUnmergeable() {
    const self2 = this.getWritable();
    self2.__detail ^= IS_UNMERGEABLE;
    return self2;
  }
  /**
   * Sets the mode of the node.
   *
   * @returns this TextNode.
   */
  setMode(type) {
    const mode = TEXT_MODE_TO_TYPE[type];
    if (this.__mode === mode) {
      return this;
    }
    const self2 = this.getWritable();
    self2.__mode = mode;
    return self2;
  }
  /**
   * Sets the text content of the node.
   *
   * @param text - the string to set as the text value of the node.
   *
   * @returns this TextNode.
   */
  setTextContent(text) {
    if (this.__text === text) {
      return this;
    }
    const self2 = this.getWritable();
    self2.__text = text;
    return self2;
  }
  /**
   * Sets the current Lexical selection to be a RangeSelection with anchor and focus on this TextNode at the provided offsets.
   *
   * @param _anchorOffset - the offset at which the Selection anchor will be placed.
   * @param _focusOffset - the offset at which the Selection focus will be placed.
   *
   * @returns the new RangeSelection.
   */
  select(_anchorOffset, _focusOffset) {
    errorOnReadOnly();
    let anchorOffset = _anchorOffset;
    let focusOffset = _focusOffset;
    const selection = $getSelection();
    const text = this.getTextContent();
    const key = this.__key;
    if (typeof text === "string") {
      const lastOffset = text.length;
      if (anchorOffset === void 0) {
        anchorOffset = lastOffset;
      }
      if (focusOffset === void 0) {
        focusOffset = lastOffset;
      }
    } else {
      anchorOffset = 0;
      focusOffset = 0;
    }
    if (!$isRangeSelection(selection)) {
      return $internalMakeRangeSelection(key, anchorOffset, key, focusOffset, "text", "text");
    } else {
      const compositionKey = $getCompositionKey();
      if (compositionKey === selection.anchor.key || compositionKey === selection.focus.key) {
        $setCompositionKey(key);
      }
      selection.setTextNodeRange(this, anchorOffset, this, focusOffset);
    }
    return selection;
  }
  selectStart() {
    return this.select(0, 0);
  }
  selectEnd() {
    const size = this.getTextContentSize();
    return this.select(size, size);
  }
  /**
   * Inserts the provided text into this TextNode at the provided offset, deleting the number of characters
   * specified. Can optionally calculate a new selection after the operation is complete.
   *
   * @param offset - the offset at which the splice operation should begin.
   * @param delCount - the number of characters to delete, starting from the offset.
   * @param newText - the text to insert into the TextNode at the offset.
   * @param moveSelection - optional, whether or not to move selection to the end of the inserted substring.
   *
   * @returns this TextNode.
   */
  spliceText(offset, delCount, newText, moveSelection) {
    const writableSelf = this.getWritable();
    const text = writableSelf.__text;
    const handledTextLength = newText.length;
    let index = offset;
    if (index < 0) {
      index = handledTextLength + index;
      if (index < 0) {
        index = 0;
      }
    }
    const selection = $getSelection();
    if (moveSelection && $isRangeSelection(selection)) {
      const newOffset = offset + handledTextLength;
      selection.setTextNodeRange(writableSelf, newOffset, writableSelf, newOffset);
    }
    const updatedText = text.slice(0, index) + newText + text.slice(index + delCount);
    writableSelf.__text = updatedText;
    return writableSelf;
  }
  /**
   * This method is meant to be overridden by TextNode subclasses to control the behavior of those nodes
   * when a user event would cause text to be inserted before them in the editor. If true, Lexical will attempt
   * to insert text into this node. If false, it will insert the text in a new sibling node.
   *
   * @returns true if text can be inserted before the node, false otherwise.
   */
  canInsertTextBefore() {
    return true;
  }
  /**
   * This method is meant to be overridden by TextNode subclasses to control the behavior of those nodes
   * when a user event would cause text to be inserted after them in the editor. If true, Lexical will attempt
   * to insert text into this node. If false, it will insert the text in a new sibling node.
   *
   * @returns true if text can be inserted after the node, false otherwise.
   */
  canInsertTextAfter() {
    return true;
  }
  /**
   * Splits this TextNode at the provided character offsets, forming new TextNodes from the substrings
   * formed by the split, and inserting those new TextNodes into the editor, replacing the one that was split.
   *
   * @param splitOffsets - rest param of the text content character offsets at which this node should be split.
   *
   * @returns an Array containing the newly-created TextNodes.
   */
  splitText(...splitOffsets) {
    errorOnReadOnly();
    const self2 = this.getLatest();
    const textContent = self2.getTextContent();
    if (textContent === "") {
      return [];
    }
    const key = self2.__key;
    const compositionKey = $getCompositionKey();
    const textLength = textContent.length;
    splitOffsets.sort((a2, b2) => a2 - b2);
    splitOffsets.push(textLength);
    const parts = [];
    const splitOffsetsLength = splitOffsets.length;
    for (let start = 0, offsetIndex = 0; start < textLength && offsetIndex <= splitOffsetsLength; offsetIndex++) {
      const end = splitOffsets[offsetIndex];
      if (end > start) {
        parts.push(textContent.slice(start, end));
        start = end;
      }
    }
    const partsLength = parts.length;
    if (partsLength === 1) {
      return [self2];
    }
    const firstPart = parts[0];
    const parent = self2.getParent();
    let writableNode;
    const format = self2.getFormat();
    const style = self2.getStyle();
    const detail = self2.__detail;
    let hasReplacedSelf = false;
    let startTextPoint = null;
    let endTextPoint = null;
    const selection = $getSelection();
    if ($isRangeSelection(selection)) {
      const [startPoint, endPoint] = selection.isBackward() ? [selection.focus, selection.anchor] : [selection.anchor, selection.focus];
      if (startPoint.type === "text" && startPoint.key === key) {
        startTextPoint = startPoint;
      }
      if (endPoint.type === "text" && endPoint.key === key) {
        endTextPoint = endPoint;
      }
    }
    if (self2.isSegmented()) {
      writableNode = $createTextNode(firstPart);
      writableNode.__format = format;
      writableNode.__style = style;
      writableNode.__detail = detail;
      writableNode.__state = $cloneNodeState(self2, writableNode);
      hasReplacedSelf = true;
    } else {
      writableNode = self2.setTextContent(firstPart);
    }
    const splitNodes = [writableNode];
    for (let i2 = 1; i2 < partsLength; i2++) {
      const part = parts[i2];
      const sibling = $createTextNode(part);
      sibling.__format = format;
      sibling.__style = style;
      sibling.__detail = detail;
      sibling.__state = $cloneNodeState(self2, sibling);
      const siblingKey = sibling.__key;
      if (compositionKey === key) {
        $setCompositionKey(siblingKey);
      }
      splitNodes.push(sibling);
    }
    const originalStartOffset = startTextPoint ? startTextPoint.offset : null;
    const originalEndOffset = endTextPoint ? endTextPoint.offset : null;
    let startOffset = 0;
    for (const node of splitNodes) {
      if (!(startTextPoint || endTextPoint)) {
        break;
      }
      const endOffset = startOffset + node.getTextContentSize();
      if (startTextPoint !== null && originalStartOffset !== null && originalStartOffset <= endOffset && originalStartOffset >= startOffset) {
        startTextPoint.set(node.getKey(), originalStartOffset - startOffset, "text");
        if (originalStartOffset < endOffset) {
          startTextPoint = null;
        }
      }
      if (endTextPoint !== null && originalEndOffset !== null && originalEndOffset <= endOffset && originalEndOffset >= startOffset) {
        endTextPoint.set(node.getKey(), originalEndOffset - startOffset, "text");
        break;
      }
      startOffset = endOffset;
    }
    if (parent !== null) {
      internalMarkSiblingsAsDirty(this);
      const writableParent = parent.getWritable();
      const insertionIndex = this.getIndexWithinParent();
      if (hasReplacedSelf) {
        writableParent.splice(insertionIndex, 0, splitNodes);
        this.remove();
      } else {
        writableParent.splice(insertionIndex, 1, splitNodes);
      }
      if ($isRangeSelection(selection)) {
        $updateElementSelectionOnCreateDeleteNode(selection, parent, insertionIndex, partsLength - 1);
      }
    }
    return splitNodes;
  }
  /**
   * Merges the target TextNode into this TextNode, removing the target node.
   *
   * @param target - the TextNode to merge into this one.
   *
   * @returns this TextNode.
   */
  mergeWithSibling(target) {
    const isBefore = target === this.getPreviousSibling();
    if (!isBefore && target !== this.getNextSibling()) {
      {
        formatDevErrorMessage(`mergeWithSibling: sibling must be a previous or next sibling`);
      }
    }
    const key = this.__key;
    const targetKey = target.__key;
    const text = this.__text;
    const textLength = text.length;
    const compositionKey = $getCompositionKey();
    if (compositionKey === targetKey) {
      $setCompositionKey(key);
    }
    const selection = $getSelection();
    if ($isRangeSelection(selection)) {
      const anchor = selection.anchor;
      const focus = selection.focus;
      if (anchor !== null && anchor.key === targetKey) {
        adjustPointOffsetForMergedSibling(anchor, isBefore, key, target, textLength);
      }
      if (focus !== null && focus.key === targetKey) {
        adjustPointOffsetForMergedSibling(focus, isBefore, key, target, textLength);
      }
    }
    const targetText = target.__text;
    const newText = isBefore ? targetText + text : text + targetText;
    this.setTextContent(newText);
    const writableSelf = this.getWritable();
    target.remove();
    return writableSelf;
  }
  /**
   * This method is meant to be overridden by TextNode subclasses to control the behavior of those nodes
   * when used with the registerLexicalTextEntity function. If you're using registerLexicalTextEntity, the
   * node class that you create and replace matched text with should return true from this method.
   *
   * @returns true if the node is to be treated as a "text entity", false otherwise.
   */
  isTextEntity() {
    return false;
  }
};
function convertSpanElement(domNode) {
  const span = domNode;
  const style = span.style;
  return {
    forChild: applyTextFormatFromStyle(style),
    node: null
  };
}
function convertBringAttentionToElement(domNode) {
  const b2 = domNode;
  const hasNormalFontWeight = b2.style.fontWeight === "normal";
  return {
    forChild: applyTextFormatFromStyle(b2.style, hasNormalFontWeight ? void 0 : "bold"),
    node: null
  };
}
var preParentCache = /* @__PURE__ */ new WeakMap();
function isNodePre(node) {
  if (!isHTMLElement(node)) {
    return false;
  } else if (node.nodeName === "PRE") {
    return true;
  }
  const whiteSpace = node.style.whiteSpace;
  return typeof whiteSpace === "string" && whiteSpace.startsWith("pre");
}
function findParentPreDOMNode(node) {
  let cached;
  let parent = node.parentNode;
  const visited = [node];
  while (parent !== null && (cached = preParentCache.get(parent)) === void 0 && !isNodePre(parent)) {
    visited.push(parent);
    parent = parent.parentNode;
  }
  const resultNode = cached === void 0 ? parent : cached;
  for (let i2 = 0; i2 < visited.length; i2++) {
    preParentCache.set(visited[i2], resultNode);
  }
  return resultNode;
}
function $convertTextDOMNode(domNode) {
  const domNode_ = domNode;
  const parentDom = domNode.parentElement;
  if (!(parentDom !== null)) {
    formatDevErrorMessage(`Expected parentElement of Text not to be null`);
  }
  let textContent = domNode_.textContent || "";
  if (findParentPreDOMNode(domNode_) !== null) {
    const parts = textContent.split(/(\r?\n|\t)/);
    const nodes = [];
    const length = parts.length;
    for (let i2 = 0; i2 < length; i2++) {
      const part = parts[i2];
      if (part === "\n" || part === "\r\n") {
        nodes.push($createLineBreakNode());
      } else if (part === "	") {
        nodes.push($createTabNode());
      } else if (part !== "") {
        nodes.push($createTextNode(part));
      }
    }
    return {
      node: nodes
    };
  }
  textContent = textContent.replace(/\r/g, "").replace(/[ \t\n]+/g, " ");
  if (textContent === "") {
    return {
      node: null
    };
  }
  if (textContent[0] === " ") {
    let previousText = domNode_;
    let isStartOfLine = true;
    while (previousText !== null && (previousText = findTextInLine(previousText, false)) !== null) {
      const previousTextContent = previousText.textContent || "";
      if (previousTextContent.length > 0) {
        if (/[ \t\n]$/.test(previousTextContent)) {
          textContent = textContent.slice(1);
        }
        isStartOfLine = false;
        break;
      }
    }
    if (isStartOfLine) {
      textContent = textContent.slice(1);
    }
  }
  if (textContent[textContent.length - 1] === " ") {
    let nextText = domNode_;
    let isEndOfLine = true;
    while (nextText !== null && (nextText = findTextInLine(nextText, true)) !== null) {
      const nextTextContent = (nextText.textContent || "").replace(/^( |\t|\r?\n)+/, "");
      if (nextTextContent.length > 0) {
        isEndOfLine = false;
        break;
      }
    }
    if (isEndOfLine) {
      textContent = textContent.slice(0, textContent.length - 1);
    }
  }
  if (textContent === "") {
    return {
      node: null
    };
  }
  return {
    node: $createTextNode(textContent)
  };
}
function findTextInLine(text, forward) {
  let node = text;
  while (true) {
    let sibling;
    while ((sibling = forward ? node.nextSibling : node.previousSibling) === null) {
      const parentElement = node.parentElement;
      if (parentElement === null) {
        return null;
      }
      node = parentElement;
    }
    node = sibling;
    if (isHTMLElement(node)) {
      const display = node.style.display;
      if (display === "" && !isInlineDomNode(node) || display !== "" && !display.startsWith("inline")) {
        return null;
      }
    }
    let descendant = node;
    while ((descendant = forward ? node.firstChild : node.lastChild) !== null) {
      node = descendant;
    }
    if (isDOMTextNode(node)) {
      return node;
    } else if (node.nodeName === "BR") {
      return null;
    }
  }
}
var nodeNameToTextFormat = {
  code: "code",
  em: "italic",
  i: "italic",
  mark: "highlight",
  s: "strikethrough",
  strong: "bold",
  sub: "subscript",
  sup: "superscript",
  u: "underline"
};
function convertTextFormatElement(domNode) {
  const format = nodeNameToTextFormat[domNode.nodeName.toLowerCase()];
  if (format === void 0) {
    return {
      node: null
    };
  }
  return {
    forChild: applyTextFormatFromStyle(domNode.style, format),
    node: null
  };
}
function $createTextNode(text = "") {
  return $applyNodeReplacement(new TextNode(text));
}
function $isTextNode(node) {
  return node instanceof TextNode;
}
function applyTextFormatFromStyle(style, shouldApply) {
  const fontWeight = style.fontWeight;
  const textDecoration = style.textDecoration.split(" ");
  const hasBoldFontWeight = fontWeight === "700" || fontWeight === "bold";
  const hasLinethroughTextDecoration = textDecoration.includes("line-through");
  const hasItalicFontStyle = style.fontStyle === "italic";
  const hasUnderlineTextDecoration = textDecoration.includes("underline");
  const verticalAlign = style.verticalAlign;
  return (lexicalNode) => {
    if (!$isTextNode(lexicalNode)) {
      return lexicalNode;
    }
    if (hasBoldFontWeight && !lexicalNode.hasFormat("bold")) {
      lexicalNode.toggleFormat("bold");
    }
    if (hasLinethroughTextDecoration && !lexicalNode.hasFormat("strikethrough")) {
      lexicalNode.toggleFormat("strikethrough");
    }
    if (hasItalicFontStyle && !lexicalNode.hasFormat("italic")) {
      lexicalNode.toggleFormat("italic");
    }
    if (hasUnderlineTextDecoration && !lexicalNode.hasFormat("underline")) {
      lexicalNode.toggleFormat("underline");
    }
    if (verticalAlign === "sub" && !lexicalNode.hasFormat("subscript")) {
      lexicalNode.toggleFormat("subscript");
    }
    if (verticalAlign === "super" && !lexicalNode.hasFormat("superscript")) {
      lexicalNode.toggleFormat("superscript");
    }
    if (shouldApply && !lexicalNode.hasFormat(shouldApply)) {
      lexicalNode.toggleFormat(shouldApply);
    }
    return lexicalNode;
  };
}
var TabNode = class _TabNode extends TextNode {
  static getType() {
    return "tab";
  }
  static clone(node) {
    return new _TabNode(node.__key);
  }
  constructor(key) {
    super("	", key);
    this.__detail = IS_UNMERGEABLE;
  }
  static importDOM() {
    return null;
  }
  createDOM(config) {
    const dom = super.createDOM(config);
    const classNames = getCachedClassNameArray(config.theme, "tab");
    if (classNames !== void 0) {
      const domClassList = dom.classList;
      domClassList.add(...classNames);
    }
    return dom;
  }
  static importJSON(serializedTabNode) {
    return $createTabNode().updateFromJSON(serializedTabNode);
  }
  setTextContent(text) {
    if (!(text === "	" || text === "")) {
      formatDevErrorMessage(`TabNode does not support setTextContent`);
    }
    return super.setTextContent("	");
  }
  spliceText(offset, delCount, newText, moveSelection) {
    if (!(newText === "" && delCount === 0 || newText === "	" && delCount === 1)) {
      formatDevErrorMessage(`TabNode does not support spliceText`);
    }
    return this;
  }
  setDetail(detail) {
    if (!(detail === IS_UNMERGEABLE)) {
      formatDevErrorMessage(`TabNode does not support setDetail`);
    }
    return this;
  }
  setMode(type) {
    if (!(type === "normal")) {
      formatDevErrorMessage(`TabNode does not support setMode`);
    }
    return this;
  }
  canInsertTextBefore() {
    return false;
  }
  canInsertTextAfter() {
    return false;
  }
};
function $createTabNode() {
  return $applyNodeReplacement(new TabNode());
}
function $isTabNode(node) {
  return node instanceof TabNode;
}
var Point = class {
  key;
  offset;
  type;
  _selection;
  constructor(key, offset, type) {
    {
      Object.defineProperty(this, "_selection", {
        enumerable: false,
        writable: true
      });
    }
    this._selection = null;
    this.key = key;
    this.offset = offset;
    this.type = type;
  }
  is(point) {
    return this.key === point.key && this.offset === point.offset && this.type === point.type;
  }
  isBefore(b2) {
    if (this.key === b2.key) {
      return this.offset < b2.offset;
    }
    const aCaret = $normalizeCaret($caretFromPoint(this, "next"));
    const bCaret = $normalizeCaret($caretFromPoint(b2, "next"));
    return $comparePointCaretNext(aCaret, bCaret) < 0;
  }
  getNode() {
    const key = this.key;
    const node = $getNodeByKey(key);
    if (node === null) {
      {
        formatDevErrorMessage(`Point.getNode: node not found`);
      }
    }
    return node;
  }
  set(key, offset, type, onlyIfChanged) {
    const selection = this._selection;
    const oldKey = this.key;
    if (onlyIfChanged && this.key === key && this.offset === offset && this.type === type) {
      return;
    }
    this.key = key;
    this.offset = offset;
    this.type = type;
    {
      const node = $getNodeByKey(key);
      if (!(type === "text" ? $isTextNode(node) : $isElementNode(node))) {
        formatDevErrorMessage(`PointType.set: node with key ${key} is ${node ? node.__type : "[not found]"} and can not be used for a ${type} point`);
      }
    }
    if (!isCurrentlyReadOnlyMode()) {
      if ($getCompositionKey() === oldKey) {
        $setCompositionKey(key);
      }
      if (selection !== null) {
        selection.setCachedNodes(null);
        selection.dirty = true;
      }
    }
  }
};
function $createPoint(key, offset, type) {
  return new Point(key, offset, type);
}
function selectPointOnNode(point, node) {
  let key = node.__key;
  let offset = point.offset;
  let type = "element";
  if ($isTextNode(node)) {
    type = "text";
    const textContentLength = node.getTextContentSize();
    if (offset > textContentLength) {
      offset = textContentLength;
    }
  } else if (!$isElementNode(node)) {
    const nextSibling = node.getNextSibling();
    if ($isTextNode(nextSibling)) {
      key = nextSibling.__key;
      offset = 0;
      type = "text";
    } else {
      const parentNode = node.getParent();
      if (parentNode) {
        key = parentNode.__key;
        offset = node.getIndexWithinParent() + 1;
      }
    }
  }
  point.set(key, offset, type);
}
function $moveSelectionPointToEnd(point, node) {
  if ($isElementNode(node)) {
    const lastNode = node.getLastDescendant();
    if ($isElementNode(lastNode) || $isTextNode(lastNode)) {
      selectPointOnNode(point, lastNode);
    } else {
      selectPointOnNode(point, node);
    }
  } else {
    selectPointOnNode(point, node);
  }
}
function $transferStartingElementPointToTextPoint(start, end, format, style) {
  const element = start.getNode();
  const placementNode = element.getChildAtIndex(start.offset);
  const textNode = $createTextNode();
  textNode.setFormat(format);
  textNode.setStyle(style);
  if ($isParagraphNode(placementNode)) {
    placementNode.splice(0, 0, [textNode]);
  } else {
    const target = $isRootNode(element) ? $createParagraphNode().append(textNode) : textNode;
    if (placementNode === null) {
      element.append(target);
    } else {
      placementNode.insertBefore(target);
    }
  }
  if (start.is(end)) {
    end.set(textNode.__key, 0, "text");
  }
  start.set(textNode.__key, 0, "text");
}
var NodeSelection = class _NodeSelection {
  _nodes;
  _cachedNodes;
  dirty;
  constructor(objects) {
    this._cachedNodes = null;
    this._nodes = objects;
    this.dirty = false;
  }
  getCachedNodes() {
    return this._cachedNodes;
  }
  setCachedNodes(nodes) {
    this._cachedNodes = nodes;
  }
  is(selection) {
    if (!$isNodeSelection(selection)) {
      return false;
    }
    const a2 = this._nodes;
    const b2 = selection._nodes;
    return a2.size === b2.size && Array.from(a2).every((key) => b2.has(key));
  }
  isCollapsed() {
    return false;
  }
  isBackward() {
    return false;
  }
  getStartEndPoints() {
    return null;
  }
  add(key) {
    this.dirty = true;
    this._nodes.add(key);
    this._cachedNodes = null;
  }
  delete(key) {
    this.dirty = true;
    this._nodes.delete(key);
    this._cachedNodes = null;
  }
  clear() {
    this.dirty = true;
    this._nodes.clear();
    this._cachedNodes = null;
  }
  has(key) {
    return this._nodes.has(key);
  }
  clone() {
    return new _NodeSelection(new Set(this._nodes));
  }
  extract() {
    return this.getNodes();
  }
  insertRawText(text) {
  }
  insertText() {
  }
  insertNodes(nodes) {
    const selectedNodes = this.getNodes();
    const selectedNodesLength = selectedNodes.length;
    const lastSelectedNode = selectedNodes[selectedNodesLength - 1];
    let selectionAtEnd;
    if ($isTextNode(lastSelectedNode)) {
      selectionAtEnd = lastSelectedNode.select();
    } else {
      const index = lastSelectedNode.getIndexWithinParent() + 1;
      selectionAtEnd = lastSelectedNode.getParentOrThrow().select(index, index);
    }
    selectionAtEnd.insertNodes(nodes);
    for (let i2 = 0; i2 < selectedNodesLength; i2++) {
      selectedNodes[i2].remove();
    }
  }
  getNodes() {
    const cachedNodes = this._cachedNodes;
    if (cachedNodes !== null) {
      return cachedNodes;
    }
    const objects = this._nodes;
    const nodes = [];
    for (const object of objects) {
      const node = $getNodeByKey(object);
      if (node !== null) {
        nodes.push(node);
      }
    }
    if (!isCurrentlyReadOnlyMode()) {
      this._cachedNodes = nodes;
    }
    return nodes;
  }
  getTextContent() {
    const nodes = this.getNodes();
    let textContent = "";
    for (let i2 = 0; i2 < nodes.length; i2++) {
      textContent += nodes[i2].getTextContent();
    }
    return textContent;
  }
  /**
   * Remove all nodes in the NodeSelection. If there were any nodes,
   * replace the selection with a new RangeSelection at the previous
   * location of the first node.
   */
  deleteNodes() {
    const nodes = this.getNodes();
    if (($getSelection() || $getPreviousSelection()) === this && nodes[0]) {
      const firstCaret = $getSiblingCaret(nodes[0], "next");
      $setSelectionFromCaretRange($getCaretRange(firstCaret, firstCaret));
    }
    for (const node of nodes) {
      node.remove();
    }
  }
};
function $isRangeSelection(x2) {
  return x2 instanceof RangeSelection;
}
var RangeSelection = class _RangeSelection {
  format;
  style;
  anchor;
  focus;
  _cachedNodes;
  dirty;
  constructor(anchor, focus, format, style) {
    this.anchor = anchor;
    this.focus = focus;
    anchor._selection = this;
    focus._selection = this;
    this._cachedNodes = null;
    this.format = format;
    this.style = style;
    this.dirty = false;
  }
  getCachedNodes() {
    return this._cachedNodes;
  }
  setCachedNodes(nodes) {
    this._cachedNodes = nodes;
  }
  /**
   * Used to check if the provided selections is equal to this one by value,
   * including anchor, focus, format, and style properties.
   * @param selection - the Selection to compare this one to.
   * @returns true if the Selections are equal, false otherwise.
   */
  is(selection) {
    if (!$isRangeSelection(selection)) {
      return false;
    }
    return this.anchor.is(selection.anchor) && this.focus.is(selection.focus) && this.format === selection.format && this.style === selection.style;
  }
  /**
   * Returns whether the Selection is "collapsed", meaning the anchor and focus are
   * the same node and have the same offset.
   *
   * @returns true if the Selection is collapsed, false otherwise.
   */
  isCollapsed() {
    return this.anchor.is(this.focus);
  }
  /**
   * Gets all the nodes in the Selection. Uses caching to make it generally suitable
   * for use in hot paths.
   *
   * See also the {@link CaretRange} APIs (starting with
   * {@link $caretRangeFromSelection}), which are likely to provide a better
   * foundation for any operation where partial selection is relevant
   * (e.g. the anchor or focus are inside an ElementNode and TextNode)
   *
   * @returns an Array containing all the nodes in the Selection
   */
  getNodes() {
    const cachedNodes = this._cachedNodes;
    if (cachedNodes !== null) {
      return cachedNodes;
    }
    const range = $getCaretRangeInDirection($caretRangeFromSelection(this), "next");
    const nodes = $getNodesFromCaretRangeCompat(range);
    {
      if (this.isCollapsed() && nodes.length > 1) {
        {
          formatDevErrorMessage(`RangeSelection.getNodes() returned ${String(nodes.length)} > 1 nodes in a collapsed selection`);
        }
      }
    }
    if (!isCurrentlyReadOnlyMode()) {
      this._cachedNodes = nodes;
    }
    return nodes;
  }
  /**
   * Sets this Selection to be of type "text" at the provided anchor and focus values.
   *
   * @param anchorNode - the anchor node to set on the Selection
   * @param anchorOffset - the offset to set on the Selection
   * @param focusNode - the focus node to set on the Selection
   * @param focusOffset - the focus offset to set on the Selection
   */
  setTextNodeRange(anchorNode, anchorOffset, focusNode, focusOffset) {
    this.anchor.set(anchorNode.__key, anchorOffset, "text");
    this.focus.set(focusNode.__key, focusOffset, "text");
  }
  /**
   * Gets the (plain) text content of all the nodes in the selection.
   *
   * @returns a string representing the text content of all the nodes in the Selection
   */
  getTextContent() {
    const nodes = this.getNodes();
    if (nodes.length === 0) {
      return "";
    }
    const firstNode = nodes[0];
    const lastNode = nodes[nodes.length - 1];
    const anchor = this.anchor;
    const focus = this.focus;
    const isBefore = anchor.isBefore(focus);
    const [anchorOffset, focusOffset] = $getCharacterOffsets(this);
    let textContent = "";
    let prevWasElement = true;
    for (let i2 = 0; i2 < nodes.length; i2++) {
      const node = nodes[i2];
      if ($isElementNode(node) && !node.isInline()) {
        if (!prevWasElement) {
          textContent += "\n";
        }
        if (node.isEmpty()) {
          prevWasElement = false;
        } else {
          prevWasElement = true;
        }
      } else {
        prevWasElement = false;
        if ($isTextNode(node)) {
          let text = node.getTextContent();
          if (node === firstNode) {
            if (node === lastNode) {
              if (anchor.type !== "element" || focus.type !== "element" || focus.offset === anchor.offset) {
                text = anchorOffset < focusOffset ? text.slice(anchorOffset, focusOffset) : text.slice(focusOffset, anchorOffset);
              }
            } else {
              text = isBefore ? text.slice(anchorOffset) : text.slice(focusOffset);
            }
          } else if (node === lastNode) {
            text = isBefore ? text.slice(0, focusOffset) : text.slice(0, anchorOffset);
          }
          textContent += text;
        } else if (($isDecoratorNode(node) || $isLineBreakNode(node)) && (node !== lastNode || !this.isCollapsed())) {
          textContent += node.getTextContent();
        }
      }
    }
    return textContent;
  }
  /**
   * Attempts to map a DOM selection range onto this Lexical Selection,
   * setting the anchor, focus, and type accordingly
   *
   * @param range a DOM Selection range conforming to the StaticRange interface.
   */
  applyDOMRange(range) {
    const editor = getActiveEditor();
    const currentEditorState = editor.getEditorState();
    const lastSelection = currentEditorState._selection;
    const resolvedSelectionPoints = $internalResolveSelectionPoints(range.startContainer, range.startOffset, range.endContainer, range.endOffset, editor, lastSelection);
    if (resolvedSelectionPoints === null) {
      return;
    }
    const [anchorPoint, focusPoint] = resolvedSelectionPoints;
    this.anchor.set(anchorPoint.key, anchorPoint.offset, anchorPoint.type, true);
    this.focus.set(focusPoint.key, focusPoint.offset, focusPoint.type, true);
    $normalizeSelection(this);
  }
  /**
   * Creates a new RangeSelection, copying over all the property values from this one.
   *
   * @returns a new RangeSelection with the same property values as this one.
   */
  clone() {
    const anchor = this.anchor;
    const focus = this.focus;
    const selection = new _RangeSelection($createPoint(anchor.key, anchor.offset, anchor.type), $createPoint(focus.key, focus.offset, focus.type), this.format, this.style);
    return selection;
  }
  /**
   * Toggles the provided format on all the TextNodes in the Selection.
   *
   * @param format a string TextFormatType to toggle on the TextNodes in the selection
   */
  toggleFormat(format) {
    this.format = toggleTextFormatType(this.format, format, null);
    this.dirty = true;
  }
  /**
   * Sets the value of the format property on the Selection
   *
   * @param format - the format to set at the value of the format property.
   */
  setFormat(format) {
    this.format = format;
    this.dirty = true;
  }
  /**
   * Sets the value of the style property on the Selection
   *
   * @param style - the style to set at the value of the style property.
   */
  setStyle(style) {
    this.style = style;
    this.dirty = true;
  }
  /**
   * Returns whether the provided TextFormatType is present on the Selection. This will be true if any node in the Selection
   * has the specified format.
   *
   * @param type the TextFormatType to check for.
   * @returns true if the provided format is currently toggled on on the Selection, false otherwise.
   */
  hasFormat(type) {
    const formatFlag = TEXT_TYPE_TO_FORMAT[type];
    return (this.format & formatFlag) !== 0;
  }
  /**
   * Attempts to insert the provided text into the EditorState at the current Selection.
   * converts tabs, newlines, and carriage returns into LexicalNodes.
   *
   * @param text the text to insert into the Selection
   */
  insertRawText(text) {
    const parts = text.split(/(\r?\n|\t)/);
    const nodes = [];
    const length = parts.length;
    for (let i2 = 0; i2 < length; i2++) {
      const part = parts[i2];
      if (part === "\n" || part === "\r\n") {
        nodes.push($createLineBreakNode());
      } else if (part === "	") {
        nodes.push($createTabNode());
      } else {
        nodes.push($createTextNode(part));
      }
    }
    this.insertNodes(nodes);
  }
  /**
   * Insert the provided text into the EditorState at the current Selection.
   *
   * @param text the text to insert into the Selection
   */
  insertText(text) {
    const anchor = this.anchor;
    const focus = this.focus;
    const format = this.format;
    const style = this.style;
    let firstPoint = anchor;
    let endPoint = focus;
    if (!this.isCollapsed() && focus.isBefore(anchor)) {
      firstPoint = focus;
      endPoint = anchor;
    }
    if (firstPoint.type === "element") {
      $transferStartingElementPointToTextPoint(firstPoint, endPoint, format, style);
    }
    if (endPoint.type === "element") {
      $setPointFromCaret(endPoint, $normalizeCaret($caretFromPoint(endPoint, "next")));
    }
    const startOffset = firstPoint.offset;
    let endOffset = endPoint.offset;
    const selectedNodes = this.getNodes();
    const selectedNodesLength = selectedNodes.length;
    let firstNode = selectedNodes[0];
    if (!$isTextNode(firstNode)) {
      {
        formatDevErrorMessage(`insertText: first node is not a text node`);
      }
    }
    const firstNodeText = firstNode.getTextContent();
    const firstNodeTextLength = firstNodeText.length;
    const firstNodeParent = firstNode.getParentOrThrow();
    const lastIndex = selectedNodesLength - 1;
    let lastNode = selectedNodes[lastIndex];
    if (selectedNodesLength === 1 && endPoint.type === "element") {
      endOffset = firstNodeTextLength;
      endPoint.set(firstPoint.key, endOffset, "text");
    }
    if (this.isCollapsed() && startOffset === firstNodeTextLength && ($isTokenOrSegmented(firstNode) || !firstNode.canInsertTextAfter() || !firstNodeParent.canInsertTextAfter() && firstNode.getNextSibling() === null)) {
      let nextSibling = firstNode.getNextSibling();
      if (!$isTextNode(nextSibling) || !nextSibling.canInsertTextBefore() || $isTokenOrSegmented(nextSibling)) {
        nextSibling = $createTextNode();
        nextSibling.setFormat(format);
        nextSibling.setStyle(style);
        if (!firstNodeParent.canInsertTextAfter()) {
          firstNodeParent.insertAfter(nextSibling);
        } else {
          firstNode.insertAfter(nextSibling);
        }
      }
      nextSibling.select(0, 0);
      firstNode = nextSibling;
      if (text !== "") {
        this.insertText(text);
        return;
      }
    } else if (this.isCollapsed() && startOffset === 0 && ($isTokenOrSegmented(firstNode) || !firstNode.canInsertTextBefore() || !firstNodeParent.canInsertTextBefore() && firstNode.getPreviousSibling() === null)) {
      let prevSibling = firstNode.getPreviousSibling();
      if (!$isTextNode(prevSibling) || $isTokenOrSegmented(prevSibling)) {
        prevSibling = $createTextNode();
        prevSibling.setFormat(format);
        if (!firstNodeParent.canInsertTextBefore()) {
          firstNodeParent.insertBefore(prevSibling);
        } else {
          firstNode.insertBefore(prevSibling);
        }
      }
      prevSibling.select();
      firstNode = prevSibling;
      if (text !== "") {
        this.insertText(text);
        return;
      }
    } else if (firstNode.isSegmented() && startOffset !== firstNodeTextLength) {
      const textNode = $createTextNode(firstNode.getTextContent());
      textNode.setFormat(format);
      firstNode.replace(textNode);
      firstNode = textNode;
    } else if (!this.isCollapsed() && text !== "") {
      const lastNodeParent = lastNode.getParent();
      if (!firstNodeParent.canInsertTextBefore() || !firstNodeParent.canInsertTextAfter() || $isElementNode(lastNodeParent) && (!lastNodeParent.canInsertTextBefore() || !lastNodeParent.canInsertTextAfter())) {
        this.insertText("");
        $normalizeSelectionPointsForBoundaries(this.anchor, this.focus);
        this.insertText(text);
        return;
      }
    }
    if (selectedNodesLength === 1) {
      if ($isTokenOrTab(firstNode)) {
        const textNode = $createTextNode(text);
        textNode.select();
        firstNode.replace(textNode);
        return;
      }
      const firstNodeFormat = firstNode.getFormat();
      const firstNodeStyle = firstNode.getStyle();
      if (startOffset === endOffset && (firstNodeFormat !== format || firstNodeStyle !== style)) {
        if (firstNode.getTextContent() === "") {
          firstNode.setFormat(format);
          firstNode.setStyle(style);
        } else {
          const textNode = $createTextNode(text);
          textNode.setFormat(format);
          textNode.setStyle(style);
          textNode.select();
          if (startOffset === 0) {
            firstNode.insertBefore(textNode, false);
          } else {
            const [targetNode] = firstNode.splitText(startOffset);
            targetNode.insertAfter(textNode, false);
          }
          if (textNode.isComposing() && this.anchor.type === "text") {
            this.anchor.offset -= text.length;
          }
          return;
        }
      } else if ($isTabNode(firstNode)) {
        const textNode = $createTextNode(text);
        textNode.setFormat(format);
        textNode.setStyle(style);
        textNode.select();
        firstNode.replace(textNode);
        return;
      }
      const delCount = endOffset - startOffset;
      firstNode = firstNode.spliceText(startOffset, delCount, text, true);
      if (firstNode.getTextContent() === "") {
        firstNode.remove();
      } else if (this.anchor.type === "text") {
        this.format = firstNodeFormat;
        this.style = firstNodeStyle;
        if (firstNode.isComposing()) {
          this.anchor.offset -= text.length;
        }
      }
    } else {
      const markedNodeKeysForKeep = /* @__PURE__ */ new Set([...firstNode.getParentKeys(), ...lastNode.getParentKeys()]);
      const firstElement = $isElementNode(firstNode) ? firstNode : firstNode.getParentOrThrow();
      let lastElement = $isElementNode(lastNode) ? lastNode : lastNode.getParentOrThrow();
      let lastElementChild = lastNode;
      if (!firstElement.is(lastElement) && lastElement.isInline()) {
        do {
          lastElementChild = lastElement;
          lastElement = lastElement.getParentOrThrow();
        } while (lastElement.isInline());
      }
      if (endPoint.type === "text" && (endOffset !== 0 || lastNode.getTextContent() === "") || endPoint.type === "element" && lastNode.getIndexWithinParent() < endOffset) {
        if ($isTextNode(lastNode) && !$isTokenOrTab(lastNode) && endOffset !== lastNode.getTextContentSize()) {
          if (lastNode.isSegmented()) {
            const textNode = $createTextNode(lastNode.getTextContent());
            lastNode.replace(textNode);
            lastNode = textNode;
          }
          if (!$isRootNode(endPoint.getNode()) && endPoint.type === "text") {
            lastNode = lastNode.spliceText(0, endOffset, "");
          }
          markedNodeKeysForKeep.add(lastNode.__key);
        } else {
          const lastNodeParent = lastNode.getParentOrThrow();
          if (!lastNodeParent.canBeEmpty() && lastNodeParent.getChildrenSize() === 1) {
            lastNodeParent.remove();
          } else {
            lastNode.remove();
          }
        }
      } else {
        markedNodeKeysForKeep.add(lastNode.__key);
      }
      const lastNodeChildren = lastElement.getChildren();
      const selectedNodesSet = new Set(selectedNodes);
      const firstAndLastElementsAreEqual = firstElement.is(lastElement);
      const insertionTarget = firstElement.isInline() && firstNode.getNextSibling() === null ? firstElement : firstNode;
      for (let i2 = lastNodeChildren.length - 1; i2 >= 0; i2--) {
        const lastNodeChild = lastNodeChildren[i2];
        if (lastNodeChild.is(firstNode) || $isElementNode(lastNodeChild) && lastNodeChild.isParentOf(firstNode)) {
          break;
        }
        if (lastNodeChild.isAttached()) {
          if (!selectedNodesSet.has(lastNodeChild) || lastNodeChild.is(lastElementChild)) {
            if (!firstAndLastElementsAreEqual) {
              insertionTarget.insertAfter(lastNodeChild, false);
            }
          } else {
            lastNodeChild.remove();
          }
        }
      }
      if (!firstAndLastElementsAreEqual) {
        let parent = lastElement;
        let lastRemovedParent = null;
        while (parent !== null) {
          const children = parent.getChildren();
          const childrenLength = children.length;
          if (childrenLength === 0 || children[childrenLength - 1].is(lastRemovedParent)) {
            markedNodeKeysForKeep.delete(parent.__key);
            lastRemovedParent = parent;
          }
          parent = parent.getParent();
        }
      }
      if (!$isTokenOrTab(firstNode)) {
        firstNode = firstNode.spliceText(startOffset, firstNodeTextLength - startOffset, text, true);
        if (firstNode.getTextContent() === "") {
          firstNode.remove();
        } else if (this.anchor.type === "text") {
          this.format = firstNode.getFormat();
          this.style = firstNode.getStyle();
          if (firstNode.isComposing()) {
            this.anchor.offset -= text.length;
          }
        }
      } else if (startOffset === firstNodeTextLength) {
        firstNode.select();
      } else {
        const textNode = $createTextNode(text);
        textNode.select();
        firstNode.replace(textNode);
      }
      for (let i2 = 1; i2 < selectedNodesLength; i2++) {
        const selectedNode = selectedNodes[i2];
        const key = selectedNode.__key;
        if (!markedNodeKeysForKeep.has(key)) {
          selectedNode.remove();
        }
      }
    }
  }
  /**
   * Removes the text in the Selection, adjusting the EditorState accordingly.
   */
  removeText() {
    const isCurrentSelection = $getSelection() === this;
    const newRange = $removeTextFromCaretRange($caretRangeFromSelection(this));
    $updateRangeSelectionFromCaretRange(this, newRange);
    if (isCurrentSelection && $getSelection() !== this) {
      $setSelection(this);
    }
  }
  // TO-DO: Migrate this method to the new utility function $forEachSelectedTextNode (share similar logic)
  /**
   * Applies the provided format to the TextNodes in the Selection, splitting or
   * merging nodes as necessary.
   *
   * @param formatType the format type to apply to the nodes in the Selection.
   * @param alignWithFormat a 32-bit integer representing formatting flags to align with.
   */
  formatText(formatType, alignWithFormat = null) {
    if (this.isCollapsed()) {
      this.toggleFormat(formatType);
      $setCompositionKey(null);
      return;
    }
    const selectedNodes = this.getNodes();
    const selectedTextNodes = [];
    for (const selectedNode of selectedNodes) {
      if ($isTextNode(selectedNode)) {
        selectedTextNodes.push(selectedNode);
      }
    }
    const applyFormatToElements = (alignWith) => {
      selectedNodes.forEach((node) => {
        if ($isElementNode(node)) {
          const newFormat = node.getFormatFlags(formatType, alignWith);
          node.setTextFormat(newFormat);
        }
      });
    };
    const selectedTextNodesLength = selectedTextNodes.length;
    if (selectedTextNodesLength === 0) {
      this.toggleFormat(formatType);
      $setCompositionKey(null);
      applyFormatToElements(alignWithFormat);
      return;
    }
    const anchor = this.anchor;
    const focus = this.focus;
    const isBackward = this.isBackward();
    const startPoint = isBackward ? focus : anchor;
    const endPoint = isBackward ? anchor : focus;
    let firstIndex = 0;
    let firstNode = selectedTextNodes[0];
    let startOffset = startPoint.type === "element" ? 0 : startPoint.offset;
    if (startPoint.type === "text" && startOffset === firstNode.getTextContentSize()) {
      firstIndex = 1;
      firstNode = selectedTextNodes[1];
      startOffset = 0;
    }
    if (firstNode == null) {
      return;
    }
    const firstNextFormat = firstNode.getFormatFlags(formatType, alignWithFormat);
    applyFormatToElements(firstNextFormat);
    const lastIndex = selectedTextNodesLength - 1;
    let lastNode = selectedTextNodes[lastIndex];
    const endOffset = endPoint.type === "text" ? endPoint.offset : lastNode.getTextContentSize();
    if (firstNode.is(lastNode)) {
      if (startOffset === endOffset) {
        return;
      }
      if ($isTokenOrSegmented(firstNode) || startOffset === 0 && endOffset === firstNode.getTextContentSize()) {
        firstNode.setFormat(firstNextFormat);
      } else {
        const splitNodes = firstNode.splitText(startOffset, endOffset);
        const replacement = startOffset === 0 ? splitNodes[0] : splitNodes[1];
        replacement.setFormat(firstNextFormat);
        if (startPoint.type === "text") {
          startPoint.set(replacement.__key, 0, "text");
        }
        if (endPoint.type === "text") {
          endPoint.set(replacement.__key, endOffset - startOffset, "text");
        }
      }
      this.format = firstNextFormat;
      return;
    }
    if (startOffset !== 0 && !$isTokenOrSegmented(firstNode)) {
      [, firstNode] = firstNode.splitText(startOffset);
      startOffset = 0;
    }
    firstNode.setFormat(firstNextFormat);
    const lastNextFormat = lastNode.getFormatFlags(formatType, firstNextFormat);
    if (endOffset > 0) {
      if (endOffset !== lastNode.getTextContentSize() && !$isTokenOrSegmented(lastNode)) {
        [lastNode] = lastNode.splitText(endOffset);
      }
      lastNode.setFormat(lastNextFormat);
    }
    for (let i2 = firstIndex + 1; i2 < lastIndex; i2++) {
      const textNode = selectedTextNodes[i2];
      const nextFormat = textNode.getFormatFlags(formatType, lastNextFormat);
      textNode.setFormat(nextFormat);
    }
    if (startPoint.type === "text") {
      startPoint.set(firstNode.__key, startOffset, "text");
    }
    if (endPoint.type === "text") {
      endPoint.set(lastNode.__key, endOffset, "text");
    }
    this.format = firstNextFormat | lastNextFormat;
  }
  /**
   * Attempts to "intelligently" insert an arbitrary list of Lexical nodes into the EditorState at the
   * current Selection according to a set of heuristics that determine how surrounding nodes
   * should be changed, replaced, or moved to accommodate the incoming ones.
   *
   * @param nodes - the nodes to insert
   */
  insertNodes(nodes) {
    if (nodes.length === 0) {
      return;
    }
    if (!this.isCollapsed()) {
      this.removeText();
    }
    if (this.anchor.key === "root") {
      this.insertParagraph();
      const selection = $getSelection();
      if (!$isRangeSelection(selection)) {
        formatDevErrorMessage(`Expected RangeSelection after insertParagraph`);
      }
      return selection.insertNodes(nodes);
    }
    const firstPoint = this.isBackward() ? this.focus : this.anchor;
    const firstNode = firstPoint.getNode();
    const firstBlock = $findMatchingParent(firstNode, INTERNAL_$isBlock);
    const last = nodes[nodes.length - 1];
    if ($isElementNode(firstBlock) && "__language" in firstBlock) {
      if ("__language" in nodes[0]) {
        this.insertText(nodes[0].getTextContent());
      } else {
        const index = $removeTextAndSplitBlock(this);
        firstBlock.splice(index, 0, nodes);
        last.selectEnd();
      }
      return;
    }
    const notInline = (node) => ($isElementNode(node) || $isDecoratorNode(node)) && !node.isInline();
    if (!nodes.some(notInline)) {
      if (!$isElementNode(firstBlock)) {
        formatDevErrorMessage(`Expected node ${firstNode.constructor.name} of type ${firstNode.getType()} to have a block ElementNode ancestor`);
      }
      const index = $removeTextAndSplitBlock(this);
      firstBlock.splice(index, 0, nodes);
      last.selectEnd();
      return;
    }
    const blocksParent = $wrapInlineNodes(nodes);
    const nodeToSelect = blocksParent.getLastDescendant();
    const blocks = blocksParent.getChildren();
    const isMergeable = (node) => $isElementNode(node) && INTERNAL_$isBlock(node) && !node.isEmpty() && $isElementNode(firstBlock) && (!firstBlock.isEmpty() || firstBlock.canMergeWhenEmpty());
    const shouldInsert = !$isElementNode(firstBlock) || !firstBlock.isEmpty();
    const insertedParagraph = shouldInsert ? this.insertParagraph() : null;
    const lastToInsert = blocks[blocks.length - 1];
    let firstToInsert = blocks[0];
    if (isMergeable(firstToInsert)) {
      if (!$isElementNode(firstBlock)) {
        formatDevErrorMessage(`Expected node ${firstNode.constructor.name} of type ${firstNode.getType()} to have a block ElementNode ancestor`);
      }
      firstBlock.append(...firstToInsert.getChildren());
      firstToInsert = blocks[1];
    }
    if (firstToInsert) {
      if (!(firstBlock !== null)) {
        formatDevErrorMessage(`Expected node ${firstNode.constructor.name} of type ${firstNode.getType()} to have a block ancestor`);
      }
      insertRangeAfter(firstBlock, firstToInsert);
    }
    const lastInsertedBlock = $findMatchingParent(nodeToSelect, INTERNAL_$isBlock);
    if (insertedParagraph && $isElementNode(lastInsertedBlock) && (insertedParagraph.canMergeWhenEmpty() || INTERNAL_$isBlock(lastToInsert))) {
      lastInsertedBlock.append(...insertedParagraph.getChildren());
      insertedParagraph.remove();
    }
    if ($isElementNode(firstBlock) && firstBlock.isEmpty()) {
      firstBlock.remove();
    }
    nodeToSelect.selectEnd();
    const lastChild = $isElementNode(firstBlock) ? firstBlock.getLastChild() : null;
    if ($isLineBreakNode(lastChild) && lastInsertedBlock !== firstBlock) {
      lastChild.remove();
    }
  }
  /**
   * Inserts a new ParagraphNode into the EditorState at the current Selection
   *
   * @returns the newly inserted node.
   */
  insertParagraph() {
    if (this.anchor.key === "root") {
      const paragraph = $createParagraphNode();
      $getRoot().splice(this.anchor.offset, 0, [paragraph]);
      paragraph.select();
      return paragraph;
    }
    const index = $removeTextAndSplitBlock(this);
    const block = $findMatchingParent(this.anchor.getNode(), INTERNAL_$isBlock);
    if (!$isElementNode(block)) {
      formatDevErrorMessage(`Expected ancestor to be a block ElementNode`);
    }
    const firstToAppend = block.getChildAtIndex(index);
    const nodesToInsert = firstToAppend ? [firstToAppend, ...firstToAppend.getNextSiblings()] : [];
    const newBlock = block.insertNewAfter(this, false);
    if (newBlock) {
      newBlock.append(...nodesToInsert);
      newBlock.selectStart();
      return newBlock;
    }
    return null;
  }
  /**
   * Inserts a logical linebreak, which may be a new LineBreakNode or a new ParagraphNode, into the EditorState at the
   * current Selection.
   */
  insertLineBreak(selectStart) {
    const lineBreak = $createLineBreakNode();
    this.insertNodes([lineBreak]);
    if (selectStart) {
      const parent = lineBreak.getParentOrThrow();
      const index = lineBreak.getIndexWithinParent();
      parent.select(index, index);
    }
  }
  /**
   * Extracts the nodes in the Selection, splitting nodes where necessary
   * to get offset-level precision.
   *
   * @returns The nodes in the Selection
   */
  extract() {
    const selectedNodes = [...this.getNodes()];
    const selectedNodesLength = selectedNodes.length;
    let firstNode = selectedNodes[0];
    let lastNode = selectedNodes[selectedNodesLength - 1];
    const [anchorOffset, focusOffset] = $getCharacterOffsets(this);
    const isBackward = this.isBackward();
    const [startPoint, endPoint] = isBackward ? [this.focus, this.anchor] : [this.anchor, this.focus];
    const [startOffset, endOffset] = isBackward ? [focusOffset, anchorOffset] : [anchorOffset, focusOffset];
    if (selectedNodesLength === 0) {
      return [];
    } else if (selectedNodesLength === 1) {
      if ($isTextNode(firstNode) && !this.isCollapsed()) {
        const splitNodes = firstNode.splitText(startOffset, endOffset);
        const node = startOffset === 0 ? splitNodes[0] : splitNodes[1];
        if (node) {
          startPoint.set(node.getKey(), 0, "text");
          endPoint.set(node.getKey(), node.getTextContentSize(), "text");
          return [node];
        }
        return [];
      }
      return [firstNode];
    }
    if ($isTextNode(firstNode)) {
      if (startOffset === firstNode.getTextContentSize()) {
        selectedNodes.shift();
      } else if (startOffset !== 0) {
        [, firstNode] = firstNode.splitText(startOffset);
        selectedNodes[0] = firstNode;
        startPoint.set(firstNode.getKey(), 0, "text");
      }
    }
    if ($isTextNode(lastNode)) {
      const lastNodeText = lastNode.getTextContent();
      const lastNodeTextLength = lastNodeText.length;
      if (endOffset === 0) {
        selectedNodes.pop();
      } else if (endOffset !== lastNodeTextLength) {
        [lastNode] = lastNode.splitText(endOffset);
        selectedNodes[selectedNodes.length - 1] = lastNode;
        endPoint.set(lastNode.getKey(), lastNode.getTextContentSize(), "text");
      }
    }
    return selectedNodes;
  }
  /**
   * Modifies the Selection according to the parameters and a set of heuristics that account for
   * various node types. Can be used to safely move or extend selection by one logical "unit" without
   * dealing explicitly with all the possible node types.
   *
   * @param alter the type of modification to perform
   * @param isBackward whether or not selection is backwards
   * @param granularity the granularity at which to apply the modification
   */
  modify(alter, isBackward, granularity) {
    if ($modifySelectionAroundDecoratorsAndBlocks(this, alter, isBackward, granularity)) {
      return;
    }
    const collapse = alter === "move";
    const editor = getActiveEditor();
    const domSelection = getDOMSelection(getWindow(editor));
    if (!domSelection) {
      return;
    }
    const blockCursorElement = editor._blockCursorElement;
    const rootElement = editor._rootElement;
    const focusNode = this.focus.getNode();
    if (rootElement !== null && blockCursorElement !== null && $isElementNode(focusNode) && !focusNode.isInline() && !focusNode.canBeEmpty()) {
      removeDOMBlockCursorElement(blockCursorElement, editor, rootElement);
    }
    if (this.dirty) {
      let nextAnchorDOM = getElementByKeyOrThrow(editor, this.anchor.key);
      let nextFocusDOM = getElementByKeyOrThrow(editor, this.focus.key);
      if (this.anchor.type === "text") {
        nextAnchorDOM = getDOMTextNode(nextAnchorDOM);
      }
      if (this.focus.type === "text") {
        nextFocusDOM = getDOMTextNode(nextFocusDOM);
      }
      if (nextAnchorDOM && nextFocusDOM) {
        setDOMSelectionBaseAndExtent(domSelection, nextAnchorDOM, this.anchor.offset, nextFocusDOM, this.focus.offset);
      }
    }
    moveNativeSelection(domSelection, alter, isBackward ? "backward" : "forward", granularity);
    if (domSelection.rangeCount > 0) {
      const range = domSelection.getRangeAt(0);
      const anchorNode = this.anchor.getNode();
      const root = $isRootNode(anchorNode) ? anchorNode : $getNearestRootOrShadowRoot(anchorNode);
      this.applyDOMRange(range);
      this.dirty = true;
      if (!collapse) {
        const nodes = this.getNodes();
        const validNodes = [];
        let shrinkSelection = false;
        for (let i2 = 0; i2 < nodes.length; i2++) {
          const nextNode = nodes[i2];
          if ($hasAncestor(nextNode, root)) {
            validNodes.push(nextNode);
          } else {
            shrinkSelection = true;
          }
        }
        if (shrinkSelection && validNodes.length > 0) {
          if (isBackward) {
            const firstValidNode = validNodes[0];
            if ($isElementNode(firstValidNode)) {
              firstValidNode.selectStart();
            } else {
              firstValidNode.getParentOrThrow().selectStart();
            }
          } else {
            const lastValidNode = validNodes[validNodes.length - 1];
            if ($isElementNode(lastValidNode)) {
              lastValidNode.selectEnd();
            } else {
              lastValidNode.getParentOrThrow().selectEnd();
            }
          }
        }
        if (domSelection.anchorNode !== range.startContainer || domSelection.anchorOffset !== range.startOffset) {
          $swapPoints(this);
        }
      }
    }
    if (granularity === "lineboundary") {
      $modifySelectionAroundDecoratorsAndBlocks(this, alter, isBackward, granularity, "decorators");
    }
  }
  /**
   * Helper for handling forward character and word deletion that prevents element nodes
   * like a table, columns layout being destroyed
   *
   * @param anchor the anchor
   * @param anchorNode the anchor node in the selection
   * @param isBackward whether or not selection is backwards
   */
  forwardDeletion(anchor, anchorNode, isBackward) {
    if (!isBackward && // Delete forward handle case
    (anchor.type === "element" && $isElementNode(anchorNode) && anchor.offset === anchorNode.getChildrenSize() || anchor.type === "text" && anchor.offset === anchorNode.getTextContentSize())) {
      const parent = anchorNode.getParent();
      const nextSibling = anchorNode.getNextSibling() || (parent === null ? null : parent.getNextSibling());
      if ($isElementNode(nextSibling) && nextSibling.isShadowRoot()) {
        return true;
      }
    }
    return false;
  }
  /**
   * Performs one logical character deletion operation on the EditorState based on the current Selection.
   * Handles different node types.
   *
   * @param isBackward whether or not the selection is backwards.
   */
  deleteCharacter(isBackward) {
    const wasCollapsed = this.isCollapsed();
    if (this.isCollapsed()) {
      const anchor = this.anchor;
      let anchorNode = anchor.getNode();
      if (this.forwardDeletion(anchor, anchorNode, isBackward)) {
        return;
      }
      const direction = isBackward ? "previous" : "next";
      const initialCaret = $caretFromPoint(anchor, direction);
      const initialRange = $extendCaretToRange(initialCaret);
      if (initialRange.getTextSlices().every((slice) => slice === null || slice.distance === 0)) {
        let state = {
          type: "initial"
        };
        for (const caret of initialRange.iterNodeCarets("shadowRoot")) {
          if ($isChildCaret(caret)) {
            if (caret.origin.isInline()) ;
            else if (caret.origin.isShadowRoot()) {
              if (state.type === "merge-block") {
                break;
              }
              if ($isElementNode(initialRange.anchor.origin) && initialRange.anchor.origin.isEmpty()) {
                const normCaret = $normalizeCaret(caret);
                $updateRangeSelectionFromCaretRange(this, $getCaretRange(normCaret, normCaret));
                initialRange.anchor.origin.remove();
              }
              return;
            } else if (state.type === "merge-next-block" || state.type === "merge-block") {
              state = {
                block: state.block,
                caret,
                type: "merge-block"
              };
            }
          } else if (state.type === "merge-block") {
            break;
          } else if ($isSiblingCaret(caret)) {
            if ($isElementNode(caret.origin)) {
              if (!caret.origin.isInline()) {
                state = {
                  block: caret.origin,
                  type: "merge-next-block"
                };
              } else if (!caret.origin.isParentOf(initialRange.anchor.origin)) {
                break;
              }
              continue;
            } else if ($isDecoratorNode(caret.origin)) {
              if (caret.origin.isIsolated()) ;
              else if (state.type === "merge-next-block" && (caret.origin.isKeyboardSelectable() || !caret.origin.isInline()) && $isElementNode(initialRange.anchor.origin) && initialRange.anchor.origin.isEmpty()) {
                initialRange.anchor.origin.remove();
                const nodeSelection = $createNodeSelection();
                nodeSelection.add(caret.origin.getKey());
                $setSelection(nodeSelection);
              } else {
                caret.origin.remove();
              }
              return;
            }
            break;
          }
        }
        if (state.type === "merge-block") {
          const {
            caret,
            block
          } = state;
          $updateRangeSelectionFromCaretRange(this, $getCaretRange(!caret.origin.isEmpty() && block.isEmpty() ? $rewindSiblingCaret($getSiblingCaret(block, caret.direction)) : initialRange.anchor, caret));
          return this.removeText();
        }
      }
      const focus = this.focus;
      this.modify("extend", isBackward, "character");
      if (!this.isCollapsed()) {
        const focusNode = focus.type === "text" ? focus.getNode() : null;
        anchorNode = anchor.type === "text" ? anchor.getNode() : null;
        if (focusNode !== null && focusNode.isSegmented()) {
          const offset = focus.offset;
          const textContentSize = focusNode.getTextContentSize();
          if (focusNode.is(anchorNode) || isBackward && offset !== textContentSize || !isBackward && offset !== 0) {
            $removeSegment(focusNode, isBackward, offset);
            return;
          }
        } else if (anchorNode !== null && anchorNode.isSegmented()) {
          const offset = anchor.offset;
          const textContentSize = anchorNode.getTextContentSize();
          if (anchorNode.is(focusNode) || isBackward && offset !== 0 || !isBackward && offset !== textContentSize) {
            $removeSegment(anchorNode, isBackward, offset);
            return;
          }
        }
        $updateCaretSelectionForUnicodeCharacter(this, isBackward);
      } else if (isBackward && anchor.offset === 0) {
        if ($collapseAtStart(this, anchor.getNode())) {
          return;
        }
      }
    }
    this.removeText();
    if (isBackward && !wasCollapsed && this.isCollapsed() && this.anchor.type === "element" && this.anchor.offset === 0) {
      const anchorNode = this.anchor.getNode();
      if (anchorNode.isEmpty() && $isRootNode(anchorNode.getParent()) && anchorNode.getPreviousSibling() === null) {
        $collapseAtStart(this, anchorNode);
      }
    }
  }
  /**
   * Performs one logical line deletion operation on the EditorState based on the current Selection.
   * Handles different node types.
   *
   * @param isBackward whether or not the selection is backwards.
   */
  deleteLine(isBackward) {
    if (this.isCollapsed()) {
      this.modify("extend", isBackward, "lineboundary");
    }
    if (this.isCollapsed()) {
      this.deleteCharacter(isBackward);
    } else {
      this.removeText();
    }
  }
  /**
   * Performs one logical word deletion operation on the EditorState based on the current Selection.
   * Handles different node types.
   *
   * @param isBackward whether or not the selection is backwards.
   */
  deleteWord(isBackward) {
    if (this.isCollapsed()) {
      const anchor = this.anchor;
      const anchorNode = anchor.getNode();
      if (this.forwardDeletion(anchor, anchorNode, isBackward)) {
        return;
      }
      this.modify("extend", isBackward, "word");
    }
    if (this.isCollapsed()) {
      this.deleteCharacter(isBackward);
    } else {
      this.removeText();
    }
  }
  /**
   * Returns whether the Selection is "backwards", meaning the focus
   * logically precedes the anchor in the EditorState.
   * @returns true if the Selection is backwards, false otherwise.
   */
  isBackward() {
    return this.focus.isBefore(this.anchor);
  }
  getStartEndPoints() {
    return [this.anchor, this.focus];
  }
};
function $isNodeSelection(x2) {
  return x2 instanceof NodeSelection;
}
function getCharacterOffset(point) {
  const offset = point.offset;
  if (point.type === "text") {
    return offset;
  }
  const parent = point.getNode();
  return offset === parent.getChildrenSize() ? parent.getTextContent().length : 0;
}
function $getCharacterOffsets(selection) {
  const anchorAndFocus = selection.getStartEndPoints();
  if (anchorAndFocus === null) {
    return [0, 0];
  }
  const [anchor, focus] = anchorAndFocus;
  if (anchor.type === "element" && focus.type === "element" && anchor.key === focus.key && anchor.offset === focus.offset) {
    return [0, 0];
  }
  return [getCharacterOffset(anchor), getCharacterOffset(focus)];
}
function $collapseAtStart(selection, startNode) {
  for (let node = startNode; node; node = node.getParent()) {
    if ($isElementNode(node)) {
      if (node.collapseAtStart(selection)) {
        return true;
      }
      if ($isRootOrShadowRoot(node)) {
        break;
      }
    }
    if (node.getPreviousSibling()) {
      break;
    }
  }
  return false;
}
function $swapPoints(selection) {
  const focus = selection.focus;
  const anchor = selection.anchor;
  const anchorKey = anchor.key;
  const anchorOffset = anchor.offset;
  const anchorType = anchor.type;
  anchor.set(focus.key, focus.offset, focus.type, true);
  focus.set(anchorKey, anchorOffset, anchorType, true);
}
function moveNativeSelection(domSelection, alter, direction, granularity) {
  domSelection.modify(alter, direction, granularity);
}
function $updateCaretSelectionForUnicodeCharacter(selection, isBackward) {
  const anchor = selection.anchor;
  const focus = selection.focus;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  if (anchorNode === focusNode && anchor.type === "text" && focus.type === "text") {
    const anchorOffset = anchor.offset;
    const focusOffset = focus.offset;
    const isBefore = anchorOffset < focusOffset;
    const startOffset = isBefore ? anchorOffset : focusOffset;
    const endOffset = isBefore ? focusOffset : anchorOffset;
    const characterOffset = endOffset - 1;
    if (startOffset !== characterOffset) {
      const text = anchorNode.getTextContent().slice(startOffset, endOffset);
      if (shouldDeleteExactlyOneCodeUnit(text)) {
        if (isBackward) {
          focus.set(focus.key, characterOffset, focus.type);
        } else {
          anchor.set(anchor.key, characterOffset, anchor.type);
        }
      }
    }
  }
}
function shouldDeleteExactlyOneCodeUnit(text) {
  {
    if (!(text.length > 1)) {
      formatDevErrorMessage(`shouldDeleteExactlyOneCodeUnit: expecting to be called only with sequences of two or more code units`);
    }
  }
  return !(doesContainSurrogatePair(text) || doesContainEmoji(text));
}
var doesContainEmoji = (() => {
  try {
    const re2 = new RegExp("\\p{Emoji}", "u");
    const test = re2.test.bind(re2);
    if (
      // Emoji in the BMP (heart) with variation selector
      test("\u2764\uFE0F") && // Emoji in the BMP (#) with variation selector
      test("#\uFE0F\u20E3") && // Emoji outside the BMP (thumbs up) that is encoded with a surrogate pair
      test("\u{1F44D}")
    ) {
      return test;
    }
  } catch (_e) {
  }
  return () => false;
})();
function $removeSegment(node, isBackward, offset) {
  const textNode = node;
  const textContent = textNode.getTextContent();
  const split = textContent.split(/(?=\s)/g);
  const splitLength = split.length;
  let segmentOffset = 0;
  let restoreOffset = 0;
  for (let i2 = 0; i2 < splitLength; i2++) {
    const text = split[i2];
    const isLast = i2 === splitLength - 1;
    restoreOffset = segmentOffset;
    segmentOffset += text.length;
    if (isBackward && segmentOffset === offset || segmentOffset > offset || isLast) {
      split.splice(i2, 1);
      if (isLast) {
        restoreOffset = void 0;
      }
      break;
    }
  }
  const nextTextContent = split.join("").trim();
  if (nextTextContent === "") {
    textNode.remove();
  } else {
    textNode.setTextContent(nextTextContent);
    textNode.select(restoreOffset, restoreOffset);
  }
}
function shouldResolveAncestor(resolvedElement, resolvedOffset, lastPoint) {
  const parent = resolvedElement.getParent();
  return lastPoint === null || parent === null || !parent.canBeEmpty() || parent !== lastPoint.getNode();
}
function $internalResolveSelectionPoint(dom, offset, lastPoint, editor) {
  let resolvedOffset = offset;
  let resolvedNode;
  if (isHTMLElement(dom)) {
    let moveSelectionToEnd = false;
    const childNodes = dom.childNodes;
    const childNodesLength = childNodes.length;
    const blockCursorElement = editor._blockCursorElement;
    if (resolvedOffset === childNodesLength) {
      moveSelectionToEnd = true;
      resolvedOffset = childNodesLength - 1;
    }
    let childDOM = childNodes[resolvedOffset];
    let hasBlockCursor = false;
    if (childDOM === blockCursorElement) {
      childDOM = childNodes[resolvedOffset + 1];
      hasBlockCursor = true;
    } else if (blockCursorElement !== null) {
      const blockCursorElementParent = blockCursorElement.parentNode;
      if (dom === blockCursorElementParent) {
        const blockCursorOffset = Array.prototype.indexOf.call(blockCursorElementParent.children, blockCursorElement);
        if (offset > blockCursorOffset) {
          resolvedOffset--;
        }
      }
    }
    resolvedNode = $getNodeFromDOM(childDOM);
    if ($isTextNode(resolvedNode)) {
      resolvedOffset = $getTextNodeOffset(resolvedNode, moveSelectionToEnd ? "next" : "previous");
    } else {
      let resolvedElement = $getNodeFromDOM(dom);
      if (resolvedElement === null) {
        return null;
      }
      if ($isElementNode(resolvedElement)) {
        const elementDOM = editor.getElementByKey(resolvedElement.getKey());
        if (!(elementDOM !== null)) {
          formatDevErrorMessage(`$internalResolveSelectionPoint: node in DOM but not keyToDOMMap`);
        }
        const slot = $getEditorDOMRenderConfig(editor).$getDOMSlot(resolvedElement, elementDOM, editor);
        [resolvedElement, resolvedOffset] = slot.resolveChildIndex(resolvedElement, elementDOM, dom, offset);
        if (!$isElementNode(resolvedElement)) {
          formatDevErrorMessage(`$internalResolveSelectionPoint: resolvedElement is not an ElementNode`);
        }
        if (moveSelectionToEnd && resolvedOffset >= resolvedElement.getChildrenSize()) {
          resolvedOffset = Math.max(0, resolvedElement.getChildrenSize() - 1);
        }
        let child = resolvedElement.getChildAtIndex(resolvedOffset);
        if ($isElementNode(child) && shouldResolveAncestor(child, resolvedOffset, lastPoint)) {
          const descendant = moveSelectionToEnd ? child.getLastDescendant() : child.getFirstDescendant();
          if (descendant === null) {
            resolvedElement = child;
          } else {
            child = descendant;
            resolvedElement = $isElementNode(child) ? child : child.getParentOrThrow();
          }
          resolvedOffset = 0;
        }
        if ($isTextNode(child)) {
          resolvedNode = child;
          resolvedElement = null;
          resolvedOffset = $getTextNodeOffset(child, moveSelectionToEnd ? "next" : "previous");
        } else if (child !== resolvedElement && moveSelectionToEnd && !hasBlockCursor) {
          if (!$isElementNode(resolvedElement)) {
            formatDevErrorMessage(`invariant`);
          }
          resolvedOffset = Math.min(resolvedElement.getChildrenSize(), resolvedOffset + 1);
        }
      } else {
        const index = resolvedElement.getIndexWithinParent();
        if (offset === 0 && $isDecoratorNode(resolvedElement) && $getNodeFromDOM(dom) === resolvedElement) {
          resolvedOffset = index;
        } else {
          resolvedOffset = index + 1;
        }
        resolvedElement = resolvedElement.getParentOrThrow();
      }
      if ($isElementNode(resolvedElement)) {
        return $createPoint(resolvedElement.__key, resolvedOffset, "element");
      }
    }
  } else {
    resolvedNode = $getNodeFromDOM(dom);
  }
  if (!$isTextNode(resolvedNode)) {
    return null;
  }
  return $createPoint(resolvedNode.__key, $getTextNodeOffset(resolvedNode, resolvedOffset, "clamp"), "text");
}
function resolveSelectionPointOnBoundary(point, isBackward, isCollapsed) {
  const offset = point.offset;
  const node = point.getNode();
  if (offset === 0) {
    const prevSibling = node.getPreviousSibling();
    const parent = node.getParent();
    if (!isBackward) {
      if ($isElementNode(prevSibling) && !isCollapsed && prevSibling.isInline()) {
        point.set(prevSibling.__key, prevSibling.getChildrenSize(), "element");
      } else if ($isTextNode(prevSibling)) {
        point.set(prevSibling.__key, prevSibling.getTextContent().length, "text");
      }
    } else if ((isCollapsed || !isBackward) && prevSibling === null && $isElementNode(parent) && parent.isInline()) {
      const parentSibling = parent.getPreviousSibling();
      if ($isTextNode(parentSibling)) {
        point.set(parentSibling.__key, parentSibling.getTextContent().length, "text");
      }
    }
  } else if (offset === node.getTextContent().length) {
    const nextSibling = node.getNextSibling();
    const parent = node.getParent();
    if (isBackward && $isElementNode(nextSibling) && nextSibling.isInline()) {
      point.set(nextSibling.__key, 0, "element");
    } else if ((isCollapsed || isBackward) && nextSibling === null && $isElementNode(parent) && parent.isInline() && !parent.canInsertTextAfter()) {
      const parentSibling = parent.getNextSibling();
      if ($isTextNode(parentSibling)) {
        point.set(parentSibling.__key, 0, "text");
      }
    }
  }
}
function $normalizeSelectionPointsForBoundaries(anchor, focus, lastSelection) {
  if (anchor.type === "text" && focus.type === "text") {
    const isBackward = anchor.isBefore(focus);
    const isCollapsed = anchor.is(focus);
    resolveSelectionPointOnBoundary(anchor, isBackward, isCollapsed);
    resolveSelectionPointOnBoundary(focus, !isBackward, isCollapsed);
    if (isCollapsed) {
      focus.set(anchor.key, anchor.offset, anchor.type);
    }
  }
}
function $internalResolveSelectionPoints(anchorDOM, anchorOffset, focusDOM, focusOffset, editor, lastSelection) {
  if (anchorDOM === null || focusDOM === null || !isSelectionWithinEditor(editor, anchorDOM, focusDOM)) {
    return null;
  }
  const resolvedAnchorPoint = $internalResolveSelectionPoint(anchorDOM, anchorOffset, $isRangeSelection(lastSelection) ? lastSelection.anchor : null, editor);
  if (resolvedAnchorPoint === null) {
    return null;
  }
  const resolvedFocusPoint = $internalResolveSelectionPoint(focusDOM, focusOffset, $isRangeSelection(lastSelection) ? lastSelection.focus : null, editor);
  if (resolvedFocusPoint === null) {
    return null;
  }
  {
    $validatePoint("anchor", resolvedAnchorPoint);
    $validatePoint("focus", resolvedFocusPoint);
  }
  if (resolvedAnchorPoint.type === "element" && resolvedFocusPoint.type === "element") {
    const anchorNode = $getNodeFromDOM(anchorDOM);
    const focusNode = $getNodeFromDOM(focusDOM);
    if ($isDecoratorNode(anchorNode) && $isDecoratorNode(focusNode)) {
      return null;
    }
  }
  $normalizeSelectionPointsForBoundaries(resolvedAnchorPoint, resolvedFocusPoint);
  return [resolvedAnchorPoint, resolvedFocusPoint];
}
function $isBlockElementNode(node) {
  return $isElementNode(node) && !node.isInline();
}
function $internalMakeRangeSelection(anchorKey, anchorOffset, focusKey, focusOffset, anchorType, focusType) {
  const editorState = getActiveEditorState();
  const selection = new RangeSelection($createPoint(anchorKey, anchorOffset, anchorType), $createPoint(focusKey, focusOffset, focusType), 0, "");
  selection.dirty = true;
  editorState._selection = selection;
  return selection;
}
function $createRangeSelection() {
  const anchor = $createPoint("root", 0, "element");
  const focus = $createPoint("root", 0, "element");
  return new RangeSelection(anchor, focus, 0, "");
}
function $createNodeSelection() {
  return new NodeSelection(/* @__PURE__ */ new Set());
}
function $internalCreateSelection(editor, event) {
  const currentEditorState = editor.getEditorState();
  const lastSelection = currentEditorState._selection;
  const domSelection = getDOMSelection(getWindow(editor));
  if ($isRangeSelection(lastSelection) || lastSelection == null) {
    return $internalCreateRangeSelection(lastSelection, domSelection, editor, event);
  }
  return lastSelection.clone();
}
function $createRangeSelectionFromDom(domSelection, editor) {
  return $internalCreateRangeSelection(null, domSelection, editor, null);
}
function $internalCreateRangeSelection(lastSelection, domSelection, editor, event) {
  const windowObj = editor._window;
  if (windowObj === null) {
    return null;
  }
  const windowEvent = event || windowObj.event;
  const eventType = windowEvent ? windowEvent.type : void 0;
  const isSelectionChange = eventType === "selectionchange";
  const useDOMSelection = !getIsProcessingMutations() && (isSelectionChange || eventType === "beforeinput" || eventType === "compositionstart" || eventType === "compositionend" || eventType === "click" && windowEvent && windowEvent.detail === 3 || eventType === "drop" || eventType === void 0);
  let anchorDOM, focusDOM, anchorOffset, focusOffset;
  if (!$isRangeSelection(lastSelection) || useDOMSelection) {
    if (domSelection === null) {
      return null;
    }
    anchorDOM = domSelection.anchorNode;
    focusDOM = domSelection.focusNode;
    anchorOffset = domSelection.anchorOffset;
    focusOffset = domSelection.focusOffset;
    if ((isSelectionChange || eventType === void 0) && $isRangeSelection(lastSelection) && !isSelectionWithinEditor(editor, anchorDOM, focusDOM)) {
      return lastSelection.clone();
    }
  } else {
    return lastSelection.clone();
  }
  const resolvedSelectionPoints = $internalResolveSelectionPoints(anchorDOM, anchorOffset, focusDOM, focusOffset, editor, lastSelection);
  if (resolvedSelectionPoints === null) {
    return null;
  }
  const [resolvedAnchorPoint, resolvedFocusPoint] = resolvedSelectionPoints;
  let format = 0;
  let style = "";
  if ($isRangeSelection(lastSelection)) {
    const lastAnchor = lastSelection.anchor;
    if (resolvedAnchorPoint.key === lastAnchor.key) {
      format = lastSelection.format;
      style = lastSelection.style;
    } else {
      const anchorNode = resolvedAnchorPoint.getNode();
      if ($isTextNode(anchorNode)) {
        format = anchorNode.getFormat();
        style = anchorNode.getStyle();
      } else if ($isElementNode(anchorNode)) {
        format = anchorNode.getTextFormat();
        style = anchorNode.getTextStyle();
      }
    }
  }
  return new RangeSelection(resolvedAnchorPoint, resolvedFocusPoint, format, style);
}
function $validatePoint(name, point) {
  const node = $getNodeByKey(point.key);
  if (!(node !== void 0)) {
    formatDevErrorMessage(`$validatePoint: ${name} key ${point.key} not found in current editorState`);
  }
  if (point.type === "text") {
    if (!$isTextNode(node)) {
      formatDevErrorMessage(`$validatePoint: ${name} key ${point.key} is not a TextNode`);
    }
    const size = node.getTextContentSize();
    if (!(point.offset <= size)) {
      formatDevErrorMessage(`$validatePoint: ${name} point.offset > node.getTextContentSize() (${String(point.offset)} > ${String(size)})`);
    }
  } else {
    if (!$isElementNode(node)) {
      formatDevErrorMessage(`$validatePoint: ${name} key ${point.key} is not an ElementNode`);
    }
    const size = node.getChildrenSize();
    if (!(point.offset <= size)) {
      formatDevErrorMessage(`$validatePoint: ${name} point.offset > node.getChildrenSize() (${String(point.offset)} > ${String(size)})`);
    }
  }
}
function $getSelection() {
  const editorState = getActiveEditorState();
  return editorState._selection;
}
function $getPreviousSelection() {
  const editor = getActiveEditor();
  return editor._editorState._selection;
}
function $updateElementSelectionOnCreateDeleteNode(selection, parentNode, nodeOffset, times = 1) {
  const anchor = selection.anchor;
  const focus = selection.focus;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  if (!parentNode.is(anchorNode) && !parentNode.is(focusNode)) {
    return;
  }
  const parentKey = parentNode.__key;
  if (selection.isCollapsed()) {
    const selectionOffset = anchor.offset;
    if (nodeOffset <= selectionOffset && times > 0 || nodeOffset < selectionOffset && times < 0) {
      const newSelectionOffset = Math.max(0, selectionOffset + times);
      anchor.set(parentKey, newSelectionOffset, "element");
      focus.set(parentKey, newSelectionOffset, "element");
      $updateSelectionResolveTextNodes(selection);
    }
  } else {
    const isBackward = selection.isBackward();
    const firstPoint = isBackward ? focus : anchor;
    const firstPointNode = firstPoint.getNode();
    const lastPoint = isBackward ? anchor : focus;
    const lastPointNode = lastPoint.getNode();
    if (parentNode.is(firstPointNode)) {
      const firstPointOffset = firstPoint.offset;
      if (nodeOffset <= firstPointOffset && times > 0 || nodeOffset < firstPointOffset && times < 0) {
        firstPoint.set(parentKey, Math.max(0, firstPointOffset + times), "element");
      }
    }
    if (parentNode.is(lastPointNode)) {
      const lastPointOffset = lastPoint.offset;
      if (nodeOffset <= lastPointOffset && times > 0 || nodeOffset < lastPointOffset && times < 0) {
        lastPoint.set(parentKey, Math.max(0, lastPointOffset + times), "element");
      }
    }
  }
  $updateSelectionResolveTextNodes(selection);
}
function $updateSelectionResolveTextNodes(selection) {
  const anchor = selection.anchor;
  const anchorOffset = anchor.offset;
  const focus = selection.focus;
  const focusOffset = focus.offset;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  if (selection.isCollapsed()) {
    if (!$isElementNode(anchorNode)) {
      return;
    }
    const childSize = anchorNode.getChildrenSize();
    const anchorOffsetAtEnd = anchorOffset >= childSize;
    const child = anchorOffsetAtEnd ? anchorNode.getChildAtIndex(childSize - 1) : anchorNode.getChildAtIndex(anchorOffset);
    if ($isTextNode(child)) {
      let newOffset = 0;
      if (anchorOffsetAtEnd) {
        newOffset = child.getTextContentSize();
      }
      anchor.set(child.__key, newOffset, "text");
      focus.set(child.__key, newOffset, "text");
    }
    return;
  }
  if ($isElementNode(anchorNode)) {
    const childSize = anchorNode.getChildrenSize();
    const anchorOffsetAtEnd = anchorOffset >= childSize;
    const child = anchorOffsetAtEnd ? anchorNode.getChildAtIndex(childSize - 1) : anchorNode.getChildAtIndex(anchorOffset);
    if ($isTextNode(child)) {
      let newOffset = 0;
      if (anchorOffsetAtEnd) {
        newOffset = child.getTextContentSize();
      }
      anchor.set(child.__key, newOffset, "text");
    }
  }
  if ($isElementNode(focusNode)) {
    const childSize = focusNode.getChildrenSize();
    const focusOffsetAtEnd = focusOffset >= childSize;
    const child = focusOffsetAtEnd ? focusNode.getChildAtIndex(childSize - 1) : focusNode.getChildAtIndex(focusOffset);
    if ($isTextNode(child)) {
      let newOffset = 0;
      if (focusOffsetAtEnd) {
        newOffset = child.getTextContentSize();
      }
      focus.set(child.__key, newOffset, "text");
    }
  }
}
function applySelectionTransforms(nextEditorState, editor) {
  const prevEditorState = editor.getEditorState();
  const prevSelection = prevEditorState._selection;
  const nextSelection = nextEditorState._selection;
  if ($isRangeSelection(nextSelection)) {
    const anchor = nextSelection.anchor;
    const focus = nextSelection.focus;
    let anchorNode;
    if (anchor.type === "text") {
      anchorNode = anchor.getNode();
      anchorNode.selectionTransform(prevSelection, nextSelection);
    }
    if (focus.type === "text") {
      const focusNode = focus.getNode();
      if (anchorNode !== focusNode) {
        focusNode.selectionTransform(prevSelection, nextSelection);
      }
    }
  }
}
function moveSelectionPointToSibling(point, node, parent, prevSibling, nextSibling) {
  let siblingKey = null;
  let offset = 0;
  let type = null;
  if (prevSibling !== null) {
    siblingKey = prevSibling.__key;
    if ($isTextNode(prevSibling)) {
      offset = prevSibling.getTextContentSize();
      type = "text";
    } else if ($isElementNode(prevSibling)) {
      offset = prevSibling.getChildrenSize();
      type = "element";
    }
  } else {
    if (nextSibling !== null) {
      siblingKey = nextSibling.__key;
      if ($isTextNode(nextSibling)) {
        type = "text";
      } else if ($isElementNode(nextSibling)) {
        type = "element";
      }
    }
  }
  if (siblingKey !== null && type !== null) {
    point.set(siblingKey, offset, type);
  } else {
    offset = node.getIndexWithinParent();
    if (offset === -1) {
      offset = parent.getChildrenSize();
    }
    point.set(parent.__key, offset, "element");
  }
}
function adjustPointOffsetForMergedSibling(point, isBefore, key, target, textLength) {
  if (point.type === "text") {
    point.set(key, point.offset + (isBefore ? 0 : textLength), "text");
  } else if (point.offset > target.getIndexWithinParent()) {
    point.set(point.key, point.offset - 1, "element");
  }
}
function setDOMSelectionBaseAndExtent(domSelection, nextAnchorDOM, nextAnchorOffset, nextFocusDOM, nextFocusOffset) {
  try {
    domSelection.setBaseAndExtent(nextAnchorDOM, nextAnchorOffset, nextFocusDOM, nextFocusOffset);
  } catch (error) {
    {
      console.warn(error);
    }
  }
}
function $getElementAndOffsetForPoint(editor, node, offset) {
  const element = getElementByKeyOrThrow(editor, node.getKey());
  if ($isElementNode(node)) {
    const slot = $getEditorDOMRenderConfig(editor).$getDOMSlot(node, element, editor);
    return [slot.element, offset + slot.getFirstChildOffset()];
  }
  return [element, offset];
}
function $updateDOMSelection(prevSelection, nextSelection, editor, domSelection, tags, rootElement, nodeCount) {
  const anchorDOMNode = domSelection.anchorNode;
  const focusDOMNode = domSelection.focusNode;
  const anchorOffset = domSelection.anchorOffset;
  const focusOffset = domSelection.focusOffset;
  const activeElement = document.activeElement;
  if (tags.has(COLLABORATION_TAG) && activeElement !== rootElement || activeElement !== null && isSelectionCapturedInDecoratorInput(activeElement)) {
    return;
  }
  if (!$isRangeSelection(nextSelection)) {
    if (prevSelection !== null && isSelectionWithinEditor(editor, anchorDOMNode, focusDOMNode)) {
      domSelection.removeAllRanges();
    }
    return;
  }
  const anchor = nextSelection.anchor;
  const focus = nextSelection.focus;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  const [anchorDOM, nextAnchorOffset] = $getElementAndOffsetForPoint(editor, anchorNode, anchor.offset);
  const [focusDOM, nextFocusOffset] = $getElementAndOffsetForPoint(editor, focusNode, focus.offset);
  const nextFormat = nextSelection.format;
  const nextStyle = nextSelection.style;
  const isCollapsed = nextSelection.isCollapsed();
  let nextAnchorNode = anchorDOM;
  let nextFocusNode = focusDOM;
  let anchorFormatOrStyleChanged = false;
  if (anchor.type === "text") {
    nextAnchorNode = getDOMTextNode(anchorDOM);
    anchorFormatOrStyleChanged = anchorNode.getFormat() !== nextFormat || anchorNode.getStyle() !== nextStyle;
  } else if ($isRangeSelection(prevSelection) && prevSelection.anchor.type === "text") {
    anchorFormatOrStyleChanged = true;
  }
  if (focus.type === "text") {
    nextFocusNode = getDOMTextNode(focusDOM);
  }
  if (nextAnchorNode === null || nextFocusNode === null) {
    return;
  }
  if (isCollapsed && (prevSelection === null || anchorFormatOrStyleChanged || $isRangeSelection(prevSelection) && (prevSelection.format !== nextFormat || prevSelection.style !== nextStyle))) {
    markCollapsedSelectionFormat(nextFormat, nextStyle, nextAnchorOffset, anchor.key, performance.now());
  }
  if (anchorOffset === nextAnchorOffset && focusOffset === nextFocusOffset && anchorDOMNode === nextAnchorNode && focusDOMNode === nextFocusNode && // Badly interpreted range selection when collapsed - #1482
  !(domSelection.type === "Range" && isCollapsed)) {
    if (activeElement === null || !rootElement.contains(activeElement)) {
      if (!tags.has(SKIP_SELECTION_FOCUS_TAG)) {
        rootElement.focus({
          preventScroll: true
        });
      }
    }
    if (anchor.type !== "element") {
      return;
    }
  }
  setDOMSelectionBaseAndExtent(domSelection, nextAnchorNode, nextAnchorOffset, nextFocusNode, nextFocusOffset);
  if (IS_FIREFOX && nextSelection.isCollapsed() && rootElement !== null && !tags.has(SKIP_SELECTION_FOCUS_TAG) && (document.activeElement === null || !rootElement.contains(document.activeElement))) {
    rootElement.focus({
      preventScroll: true
    });
  }
  if (!tags.has(SKIP_SCROLL_INTO_VIEW_TAG) && nextSelection.isCollapsed() && rootElement !== null && rootElement === document.activeElement) {
    const selectionTarget = $isRangeSelection(nextSelection) && nextSelection.anchor.type === "element" ? nextAnchorNode.childNodes[nextAnchorOffset] || null : domSelection.rangeCount > 0 ? domSelection.getRangeAt(0) : null;
    if (selectionTarget !== null) {
      let selectionRect;
      if (selectionTarget instanceof Text) {
        const range = document.createRange();
        range.selectNode(selectionTarget);
        selectionRect = range.getBoundingClientRect();
      } else {
        selectionRect = selectionTarget.getBoundingClientRect();
      }
      scrollIntoViewIfNeeded(editor, selectionRect, rootElement);
    }
  }
  markSelectionChangeFromDOMUpdate();
}
function $insertNodes(nodes) {
  let selection = $getSelection() || $getPreviousSelection();
  if (selection === null) {
    selection = $getRoot().selectEnd();
  }
  selection.insertNodes(nodes);
}
function $getTextContent() {
  const selection = $getSelection();
  if (selection === null) {
    return "";
  }
  return selection.getTextContent();
}
function $removeTextAndSplitBlock(selection) {
  let selection_ = selection;
  if (!selection.isCollapsed()) {
    selection_.removeText();
  }
  const newSelection = $getSelection();
  if ($isRangeSelection(newSelection)) {
    selection_ = newSelection;
  }
  if (!$isRangeSelection(selection_)) {
    formatDevErrorMessage(`Unexpected dirty selection to be null`);
  }
  const anchor = selection_.anchor;
  let node = anchor.getNode();
  let offset = anchor.offset;
  while (!INTERNAL_$isBlock(node)) {
    const prevNode = node;
    [node, offset] = $splitNodeAtPoint(node, offset);
    if (prevNode.is(node)) {
      break;
    }
  }
  return offset;
}
function $splitNodeAtPoint(node, offset) {
  const parent = node.getParent();
  if (!parent) {
    const paragraph = $createParagraphNode();
    $getRoot().append(paragraph);
    paragraph.select();
    return [$getRoot(), 0];
  }
  if ($isTextNode(node)) {
    const split = node.splitText(offset);
    if (split.length === 0) {
      return [parent, node.getIndexWithinParent()];
    }
    const x2 = offset === 0 ? 0 : 1;
    const index = split[0].getIndexWithinParent() + x2;
    return [parent, index];
  }
  if (!$isElementNode(node) || offset === 0) {
    return [parent, node.getIndexWithinParent()];
  }
  const firstToAppend = node.getChildAtIndex(offset);
  if (firstToAppend) {
    const insertPoint = new RangeSelection($createPoint(node.__key, offset, "element"), $createPoint(node.__key, offset, "element"), 0, "");
    const newElement = node.insertNewAfter(insertPoint);
    if (newElement) {
      newElement.append(firstToAppend, ...firstToAppend.getNextSiblings());
    }
  }
  return [parent, node.getIndexWithinParent() + 1];
}
function $wrapInlineNodes(nodes) {
  const virtualRoot = $createParagraphNode();
  let currentBlock = null;
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    const isLineBreakNode = $isLineBreakNode(node);
    if (isLineBreakNode || $isDecoratorNode(node) && node.isInline() || $isElementNode(node) && node.isInline() || $isTextNode(node) || node.isParentRequired()) {
      if (currentBlock === null) {
        currentBlock = node.createParentElementNode();
        virtualRoot.append(currentBlock);
        if (isLineBreakNode) {
          continue;
        }
      }
      if (currentBlock !== null) {
        currentBlock.append(node);
      }
    } else {
      virtualRoot.append(node);
      currentBlock = null;
    }
  }
  return virtualRoot;
}
function $getNodesFromCaretRangeCompat(range) {
  const nodes = [];
  const [beforeSlice, afterSlice] = range.getTextSlices();
  if (beforeSlice) {
    nodes.push(beforeSlice.caret.origin);
  }
  const seenAncestors = /* @__PURE__ */ new Set();
  const seenElements = /* @__PURE__ */ new Set();
  for (const caret of range) {
    if ($isChildCaret(caret)) {
      const {
        origin
      } = caret;
      if (nodes.length === 0) {
        seenAncestors.add(origin);
      } else {
        seenElements.add(origin);
        nodes.push(origin);
      }
    } else {
      const {
        origin
      } = caret;
      if (!$isElementNode(origin) || !seenElements.has(origin)) {
        nodes.push(origin);
      }
    }
  }
  if (afterSlice) {
    nodes.push(afterSlice.caret.origin);
  }
  if ($isSiblingCaret(range.focus) && $isElementNode(range.focus.origin) && range.focus.getNodeAtCaret() === null) {
    for (let reverseCaret = $getChildCaret(range.focus.origin, "previous"); $isChildCaret(reverseCaret) && seenAncestors.has(reverseCaret.origin) && !reverseCaret.origin.isEmpty() && reverseCaret.origin.is(nodes[nodes.length - 1]); reverseCaret = $getAdjacentChildCaret(reverseCaret)) {
      seenAncestors.delete(reverseCaret.origin);
      nodes.pop();
    }
  }
  while (nodes.length > 1) {
    const lastIncludedNode = nodes[nodes.length - 1];
    if ($isElementNode(lastIncludedNode)) {
      if (seenElements.has(lastIncludedNode) || lastIncludedNode.isEmpty() || seenAncestors.has(lastIncludedNode)) ;
      else {
        nodes.pop();
        continue;
      }
    }
    break;
  }
  if (nodes.length === 0 && range.isCollapsed()) {
    const normCaret = $normalizeCaret(range.anchor);
    const flippedNormCaret = $normalizeCaret(range.anchor.getFlipped());
    const $getCandidate = (caret) => $isTextPointCaret(caret) ? caret.origin : caret.getNodeAtCaret();
    const node = $getCandidate(normCaret) || $getCandidate(flippedNormCaret) || (range.anchor.getNodeAtCaret() ? normCaret.origin : flippedNormCaret.origin);
    nodes.push(node);
  }
  return nodes;
}
function $modifySelectionAroundDecoratorsAndBlocks(selection, alter, isBackward, granularity, mode = "decorators-and-blocks") {
  if (alter === "move" && granularity === "character" && !selection.isCollapsed()) {
    const [src, dst] = isBackward === selection.isBackward() ? [selection.focus, selection.anchor] : [selection.anchor, selection.focus];
    dst.set(src.key, src.offset, src.type);
    return true;
  }
  const initialFocus = $caretFromPoint(selection.focus, isBackward ? "previous" : "next");
  const isLineBoundary = granularity === "lineboundary";
  const collapse = alter === "move";
  let focus = initialFocus;
  let checkForBlock = mode === "decorators-and-blocks";
  if (!$isExtendableTextPointCaret(focus)) {
    for (const siblingCaret of focus) {
      checkForBlock = false;
      const {
        origin
      } = siblingCaret;
      if ($isDecoratorNode(origin) && !origin.isIsolated()) {
        focus = siblingCaret;
        if (isLineBoundary && origin.isInline()) {
          continue;
        }
      }
      break;
    }
    if (checkForBlock) {
      for (const nextCaret of $extendCaretToRange(initialFocus).iterNodeCarets(alter === "extend" ? "shadowRoot" : "root")) {
        if ($isChildCaret(nextCaret)) {
          if (!nextCaret.origin.isInline()) {
            focus = nextCaret;
          }
        } else if ($isElementNode(nextCaret.origin)) {
          continue;
        } else if ($isDecoratorNode(nextCaret.origin) && !nextCaret.origin.isInline()) {
          focus = nextCaret;
        }
        break;
      }
    }
  }
  if (focus === initialFocus) {
    return false;
  }
  if (collapse && !isLineBoundary && $isDecoratorNode(focus.origin) && focus.origin.isKeyboardSelectable()) {
    const nodeSelection = $createNodeSelection();
    nodeSelection.add(focus.origin.getKey());
    $setSelection(nodeSelection);
    return true;
  }
  focus = $normalizeCaret(focus);
  if (collapse) {
    $setPointFromCaret(selection.anchor, focus);
  }
  $setPointFromCaret(selection.focus, focus);
  return checkForBlock || !isLineBoundary;
}
var activeEditorState = null;
var activeEditor = null;
var isReadOnlyMode = false;
var isAttemptingToRecoverFromReconcilerError = false;
var infiniteTransformCount = 0;
var observerOptions = {
  characterData: true,
  childList: true,
  subtree: true
};
function isCurrentlyReadOnlyMode() {
  return isReadOnlyMode || activeEditorState !== null && activeEditorState._readOnly;
}
function errorOnReadOnly() {
  if (isReadOnlyMode) {
    {
      formatDevErrorMessage(`Cannot use method in read-only mode.`);
    }
  }
}
function errorOnInfiniteTransforms() {
  if (infiniteTransformCount > 99) {
    {
      formatDevErrorMessage(`One or more transforms are endlessly triggering additional transforms. May have encountered infinite recursion caused by transforms that have their preconditions too lose and/or conflict with each other.`);
    }
  }
}
function getActiveEditorState() {
  if (activeEditorState === null) {
    {
      formatDevErrorMessage(`Unable to find an active editor state. State helpers or node methods can only be used synchronously during the callback of editor.update(), editor.read(), or editorState.read().${collectBuildInformation()}`);
    }
  }
  return activeEditorState;
}
function getActiveEditor() {
  if (activeEditor === null) {
    {
      formatDevErrorMessage(`Unable to find an active editor. This method can only be used synchronously during the callback of editor.update(), editor.read(), or editor.getEditorState().read(..., {editor}).${collectBuildInformation()}`);
    }
  }
  return activeEditor;
}
function collectBuildInformation() {
  let compatibleEditors = 0;
  const incompatibleEditors = /* @__PURE__ */ new Set();
  const thisVersion = LexicalEditor.version;
  if (typeof window !== "undefined") {
    for (const node of document.querySelectorAll("[contenteditable]")) {
      const editor = getEditorPropertyFromDOMNode(node);
      if (isLexicalEditor(editor)) {
        compatibleEditors++;
      } else if (editor) {
        let version = String(editor.constructor.version || "<0.17.1");
        if (version === thisVersion) {
          version += " (separately built, likely a bundler configuration issue)";
        }
        incompatibleEditors.add(version);
      }
    }
  }
  let output = ` Detected on the page: ${compatibleEditors} compatible editor(s) with version ${thisVersion}`;
  if (incompatibleEditors.size) {
    output += ` and incompatible editors with versions ${Array.from(incompatibleEditors).join(", ")}`;
  }
  return output;
}
function internalGetActiveEditor() {
  return activeEditor;
}
function internalGetActiveEditorState() {
  return activeEditorState;
}
function $applyTransforms(editor, node, transformsCache) {
  const type = node.__type;
  const registeredNode = getRegisteredNodeOrThrow(editor, type);
  let transformsArr = transformsCache.get(type);
  if (transformsArr === void 0) {
    transformsArr = Array.from(registeredNode.transforms);
    transformsCache.set(type, transformsArr);
  }
  const transformsArrLength = transformsArr.length;
  for (let i2 = 0; i2 < transformsArrLength; i2++) {
    transformsArr[i2](node);
    if (!node.isAttached()) {
      break;
    }
  }
}
function $isNodeValidForTransform(node, compositionKey) {
  return node !== void 0 && // We don't want to transform nodes being composed
  node.__key !== compositionKey && node.isAttached();
}
function $normalizeAllDirtyTextNodes(editorState, editor) {
  const dirtyLeaves = editor._dirtyLeaves;
  const nodeMap = editorState._nodeMap;
  for (const nodeKey of dirtyLeaves) {
    const node = nodeMap.get(nodeKey);
    if ($isTextNode(node) && node.isAttached() && node.isSimpleText() && !node.isUnmergeable()) {
      $normalizeTextNode(node);
    }
  }
}
function addTags(editor, tags) {
  if (!tags) {
    return;
  }
  const updateTags = editor._updateTags;
  let tags_ = tags;
  if (!Array.isArray(tags)) {
    tags_ = [tags];
  }
  for (const tag of tags_) {
    updateTags.add(tag);
  }
}
function $applyAllTransforms(editorState, editor) {
  const dirtyLeaves = editor._dirtyLeaves;
  const dirtyElements = editor._dirtyElements;
  const nodeMap = editorState._nodeMap;
  const compositionKey = $getCompositionKey();
  const transformsCache = /* @__PURE__ */ new Map();
  let untransformedDirtyLeaves = dirtyLeaves;
  let untransformedDirtyLeavesLength = untransformedDirtyLeaves.size;
  let untransformedDirtyElements = dirtyElements;
  let untransformedDirtyElementsLength = untransformedDirtyElements.size;
  while (untransformedDirtyLeavesLength > 0 || untransformedDirtyElementsLength > 0) {
    if (untransformedDirtyLeavesLength > 0) {
      editor._dirtyLeaves = /* @__PURE__ */ new Set();
      for (const nodeKey of untransformedDirtyLeaves) {
        const node = nodeMap.get(nodeKey);
        if ($isTextNode(node) && node.isAttached() && node.isSimpleText() && !node.isUnmergeable()) {
          $normalizeTextNode(node);
        }
        if (node !== void 0 && $isNodeValidForTransform(node, compositionKey)) {
          $applyTransforms(editor, node, transformsCache);
        }
        dirtyLeaves.add(nodeKey);
      }
      untransformedDirtyLeaves = editor._dirtyLeaves;
      untransformedDirtyLeavesLength = untransformedDirtyLeaves.size;
      if (untransformedDirtyLeavesLength > 0) {
        infiniteTransformCount++;
        continue;
      }
    }
    editor._dirtyLeaves = /* @__PURE__ */ new Set();
    editor._dirtyElements = /* @__PURE__ */ new Map();
    const rootDirty = untransformedDirtyElements.delete("root");
    if (rootDirty) {
      untransformedDirtyElements.set("root", true);
    }
    for (const currentUntransformedDirtyElement of untransformedDirtyElements) {
      const nodeKey = currentUntransformedDirtyElement[0];
      const intentionallyMarkedAsDirty = currentUntransformedDirtyElement[1];
      dirtyElements.set(nodeKey, intentionallyMarkedAsDirty);
      if (!intentionallyMarkedAsDirty) {
        continue;
      }
      const node = nodeMap.get(nodeKey);
      if (node !== void 0 && $isNodeValidForTransform(node, compositionKey)) {
        $applyTransforms(editor, node, transformsCache);
      }
    }
    untransformedDirtyLeaves = editor._dirtyLeaves;
    untransformedDirtyLeavesLength = untransformedDirtyLeaves.size;
    untransformedDirtyElements = editor._dirtyElements;
    untransformedDirtyElementsLength = untransformedDirtyElements.size;
    infiniteTransformCount++;
  }
  editor._dirtyLeaves = dirtyLeaves;
  editor._dirtyElements = dirtyElements;
}
function $parseSerializedNode(serializedNode) {
  const internalSerializedNode = serializedNode;
  return $parseSerializedNodeImpl(internalSerializedNode, getActiveEditor()._nodes);
}
function $parseSerializedNodeImpl(serializedNode, registeredNodes) {
  const type = serializedNode.type;
  const registeredNode = registeredNodes.get(type);
  if (registeredNode === void 0) {
    {
      formatDevErrorMessage(`parseEditorState: type "${type}" + not found`);
    }
  }
  const nodeClass = registeredNode.klass;
  if (serializedNode.type !== nodeClass.getType()) {
    {
      formatDevErrorMessage(`LexicalNode: Node ${nodeClass.name} does not implement .importJSON().`);
    }
  }
  const node = nodeClass.importJSON(serializedNode);
  const children = serializedNode.children;
  if ($isElementNode(node) && Array.isArray(children)) {
    for (let i2 = 0; i2 < children.length; i2++) {
      const serializedJSONChildNode = children[i2];
      const childNode = $parseSerializedNodeImpl(serializedJSONChildNode, registeredNodes);
      node.append(childNode);
    }
  }
  return node;
}
function parseEditorState(serializedEditorState, editor, updateFn) {
  const editorState = createEmptyEditorState();
  const previousActiveEditorState = activeEditorState;
  const previousReadOnlyMode = isReadOnlyMode;
  const previousActiveEditor = activeEditor;
  const previousDirtyElements = editor._dirtyElements;
  const previousDirtyLeaves = editor._dirtyLeaves;
  const previousCloneNotNeeded = editor._cloneNotNeeded;
  const previousDirtyType = editor._dirtyType;
  editor._dirtyElements = /* @__PURE__ */ new Map();
  editor._dirtyLeaves = /* @__PURE__ */ new Set();
  editor._cloneNotNeeded = /* @__PURE__ */ new Set();
  editor._dirtyType = 0;
  activeEditorState = editorState;
  isReadOnlyMode = false;
  activeEditor = editor;
  setPendingNodeToClone(null);
  try {
    const registeredNodes = editor._nodes;
    const serializedNode = serializedEditorState.root;
    $parseSerializedNodeImpl(serializedNode, registeredNodes);
    if (updateFn) {
      updateFn();
    }
    editorState._readOnly = true;
    {
      handleDEVOnlyPendingUpdateGuarantees(editorState);
    }
  } catch (error) {
    if (error instanceof Error) {
      editor._onError(error);
    }
  } finally {
    editor._dirtyElements = previousDirtyElements;
    editor._dirtyLeaves = previousDirtyLeaves;
    editor._cloneNotNeeded = previousCloneNotNeeded;
    editor._dirtyType = previousDirtyType;
    activeEditorState = previousActiveEditorState;
    isReadOnlyMode = previousReadOnlyMode;
    activeEditor = previousActiveEditor;
  }
  return editorState;
}
function readEditorState(editor, editorState, callbackFn) {
  const previousActiveEditorState = activeEditorState;
  const previousReadOnlyMode = isReadOnlyMode;
  const previousActiveEditor = activeEditor;
  activeEditorState = editorState;
  isReadOnlyMode = true;
  activeEditor = editor;
  try {
    return callbackFn();
  } finally {
    activeEditorState = previousActiveEditorState;
    isReadOnlyMode = previousReadOnlyMode;
    activeEditor = previousActiveEditor;
  }
}
function handleDEVOnlyPendingUpdateGuarantees(pendingEditorState) {
  const nodeMap = pendingEditorState._nodeMap;
  nodeMap.set = () => {
    throw new Error("Cannot call set() on a frozen Lexical node map");
  };
  nodeMap.clear = () => {
    throw new Error("Cannot call clear() on a frozen Lexical node map");
  };
  nodeMap.delete = () => {
    throw new Error("Cannot call delete() on a frozen Lexical node map");
  };
}
function $commitPendingUpdates(editor, recoveryEditorState) {
  const pendingEditorState = editor._pendingEditorState;
  const rootElement = editor._rootElement;
  const shouldSkipDOM = editor._headless || rootElement === null;
  if (pendingEditorState === null) {
    if (editor._deferred.length > 0) {
      triggerDeferredUpdateCallbacks(editor, editor._deferred);
    }
    return;
  }
  const currentEditorState = editor._editorState;
  const currentSelection = currentEditorState._selection;
  const pendingSelection = pendingEditorState._selection;
  const needsUpdate = editor._dirtyType !== NO_DIRTY_NODES;
  const previousActiveEditorState = activeEditorState;
  const previousReadOnlyMode = isReadOnlyMode;
  const previousActiveEditor = activeEditor;
  const previouslyUpdating = editor._updating;
  const observer = editor._observer;
  let mutatedNodes2 = null;
  editor._pendingEditorState = null;
  editor._editorState = pendingEditorState;
  if (!shouldSkipDOM && needsUpdate && observer !== null) {
    activeEditor = editor;
    activeEditorState = pendingEditorState;
    isReadOnlyMode = false;
    editor._updating = true;
    try {
      const dirtyType = editor._dirtyType;
      const dirtyElements2 = editor._dirtyElements;
      const dirtyLeaves2 = editor._dirtyLeaves;
      observer.disconnect();
      mutatedNodes2 = $reconcileRoot(currentEditorState, pendingEditorState, editor, dirtyType, dirtyElements2, dirtyLeaves2);
    } catch (error) {
      if (error instanceof Error) {
        editor._onError(error);
      }
      if (!isAttemptingToRecoverFromReconcilerError) {
        resetEditor(editor, null, rootElement, pendingEditorState);
        initMutationObserver(editor);
        editor._dirtyType = FULL_RECONCILE;
        isAttemptingToRecoverFromReconcilerError = true;
        $commitPendingUpdates(editor, currentEditorState);
        isAttemptingToRecoverFromReconcilerError = false;
      } else {
        throw error;
      }
      return;
    } finally {
      observer.observe(rootElement, observerOptions);
      editor._updating = previouslyUpdating;
      activeEditorState = previousActiveEditorState;
      isReadOnlyMode = previousReadOnlyMode;
      activeEditor = previousActiveEditor;
    }
  }
  if (!pendingEditorState._readOnly) {
    pendingEditorState._readOnly = true;
    {
      handleDEVOnlyPendingUpdateGuarantees(pendingEditorState);
      if ($isRangeSelection(pendingSelection)) {
        Object.freeze(pendingSelection.anchor);
        Object.freeze(pendingSelection.focus);
      }
      Object.freeze(pendingSelection);
    }
  }
  const dirtyLeaves = editor._dirtyLeaves;
  const dirtyElements = editor._dirtyElements;
  const normalizedNodes = editor._normalizedNodes;
  const tags = editor._updateTags;
  const deferred = editor._deferred;
  if (needsUpdate) {
    editor._dirtyType = NO_DIRTY_NODES;
    editor._cloneNotNeeded.clear();
    editor._dirtyLeaves = /* @__PURE__ */ new Set();
    editor._dirtyElements = /* @__PURE__ */ new Map();
    editor._normalizedNodes = /* @__PURE__ */ new Set();
    editor._updateTags = /* @__PURE__ */ new Set();
  }
  $garbageCollectDetachedDecorators(editor, pendingEditorState);
  const domSelection = shouldSkipDOM ? null : getDOMSelection(getWindow(editor));
  if (editor._editable && // domSelection will be null in headless
  domSelection !== null && (needsUpdate || pendingSelection === null || pendingSelection.dirty || !pendingSelection.is(currentSelection)) && rootElement !== null && !tags.has(SKIP_DOM_SELECTION_TAG)) {
    activeEditor = editor;
    activeEditorState = pendingEditorState;
    try {
      if (observer !== null) {
        observer.disconnect();
      }
      if (needsUpdate || pendingSelection === null || pendingSelection.dirty) {
        const blockCursorElement = editor._blockCursorElement;
        if (blockCursorElement !== null) {
          removeDOMBlockCursorElement(blockCursorElement, editor, rootElement);
        }
        $updateDOMSelection(currentSelection, pendingSelection, editor, domSelection, tags, rootElement);
      }
      updateDOMBlockCursorElement(editor, rootElement, pendingSelection);
    } finally {
      if (observer !== null) {
        observer.observe(rootElement, observerOptions);
      }
      activeEditor = previousActiveEditor;
      activeEditorState = previousActiveEditorState;
    }
  }
  if (mutatedNodes2 !== null) {
    triggerMutationListeners(editor, mutatedNodes2, tags, dirtyLeaves, currentEditorState);
  }
  if (!$isRangeSelection(pendingSelection) && pendingSelection !== null && (currentSelection === null || !currentSelection.is(pendingSelection))) {
    editor.dispatchCommand(SELECTION_CHANGE_COMMAND, void 0);
  }
  const pendingDecorators = editor._pendingDecorators;
  if (pendingDecorators !== null) {
    editor._decorators = pendingDecorators;
    editor._pendingDecorators = null;
    triggerListeners("decorator", editor, true, pendingDecorators);
  }
  triggerTextContentListeners(editor, recoveryEditorState || currentEditorState, pendingEditorState);
  triggerListeners("update", editor, true, {
    dirtyElements,
    dirtyLeaves,
    editorState: pendingEditorState,
    mutatedNodes: mutatedNodes2,
    normalizedNodes,
    prevEditorState: recoveryEditorState || currentEditorState,
    tags
  });
  triggerDeferredUpdateCallbacks(editor, deferred);
  $triggerEnqueuedUpdates(editor);
}
function triggerTextContentListeners(editor, currentEditorState, pendingEditorState) {
  const currentTextContent = getEditorStateTextContent(currentEditorState);
  const latestTextContent = getEditorStateTextContent(pendingEditorState);
  if (currentTextContent !== latestTextContent) {
    triggerListeners("textcontent", editor, true, latestTextContent);
  }
}
function triggerMutationListeners(editor, mutatedNodes2, updateTags, dirtyLeaves, prevEditorState) {
  const listeners = Array.from(editor._listeners.mutation);
  const listenersLength = listeners.length;
  for (let i2 = 0; i2 < listenersLength; i2++) {
    const [listener, klassSet] = listeners[i2];
    for (const klass of klassSet) {
      const mutatedNodesByType = mutatedNodes2.get(klass);
      if (mutatedNodesByType !== void 0) {
        listener(mutatedNodesByType, {
          dirtyLeaves,
          prevEditorState,
          updateTags
        });
      }
    }
  }
}
function triggerListeners(type, editor, isCurrentlyEnqueuingUpdates, ...payload) {
  const previouslyUpdating = editor._updating;
  editor._updating = isCurrentlyEnqueuingUpdates;
  try {
    const listenerMap = editor._listeners[type];
    const listeners = Array.from(listenerMap);
    for (const [listener, unregister] of listeners) {
      if (unregister) {
        unregister();
      }
      const nextUnregister = listener(...payload);
      if (listenerMap.has(listener)) {
        listenerMap.set(listener, nextUnregister);
      } else if (nextUnregister) {
        nextUnregister();
      }
    }
  } finally {
    editor._updating = previouslyUpdating;
  }
}
function triggerCommandListeners(editor, type, payload, fromEditor) {
  const editors = getEditorsToPropagate(editor);
  let updatingParentEditor;
  for (let i2 = 4; i2 >= 0; i2--) {
    for (let e2 = 0; e2 < editors.length; e2++) {
      const currentEditor = editors[e2];
      if (e2 > 0 && currentEditor._updating) {
        updatingParentEditor = currentEditor;
        break;
      }
      const commandListeners = currentEditor._commands;
      const listenerInPriorityOrder = commandListeners.get(type);
      if (listenerInPriorityOrder !== void 0) {
        const listenersSet = listenerInPriorityOrder[i2];
        if (listenersSet.size > 0) {
          let returnVal = false;
          updateEditorSync(currentEditor, () => {
            for (const listener of listenersSet) {
              if (listener(payload, fromEditor)) {
                returnVal = true;
                return;
              }
            }
          });
          if (returnVal) {
            return returnVal;
          }
        }
      }
    }
  }
  if (updatingParentEditor) {
    updatingParentEditor.update(() => {
      triggerCommandListeners(updatingParentEditor, type, payload, fromEditor);
    });
  }
  return false;
}
function $triggerEnqueuedUpdates(editor) {
  const queuedUpdates = editor._updates;
  if (queuedUpdates.length !== 0) {
    const queuedUpdate = queuedUpdates.shift();
    if (queuedUpdate) {
      const [updateFn, options] = queuedUpdate;
      $beginUpdate(editor, updateFn, options);
    }
  }
}
function triggerDeferredUpdateCallbacks(editor, deferred) {
  editor._deferred = [];
  if (deferred.length !== 0) {
    const previouslyUpdating = editor._updating;
    editor._updating = true;
    try {
      for (let i2 = 0; i2 < deferred.length; i2++) {
        deferred[i2]();
      }
    } finally {
      editor._updating = previouslyUpdating;
    }
  }
}
function $processNestedUpdates(editor, initialSkipTransforms) {
  const queuedUpdates = editor._updates;
  let skipTransforms = initialSkipTransforms || false;
  while (queuedUpdates.length !== 0) {
    const queuedUpdate = queuedUpdates.shift();
    if (queuedUpdate) {
      const [nextUpdateFn, options] = queuedUpdate;
      const pendingEditorState = editor._pendingEditorState;
      let onUpdate;
      if (options !== void 0) {
        onUpdate = options.onUpdate;
        if (options.skipTransforms) {
          skipTransforms = true;
        }
        if (options.discrete) {
          if (!(pendingEditorState !== null)) {
            formatDevErrorMessage(`Unexpected empty pending editor state on discrete nested update`);
          }
          pendingEditorState._flushSync = true;
        }
        if (onUpdate) {
          editor._deferred.push(onUpdate);
        }
        addTags(editor, options.tag);
      }
      if (pendingEditorState == null) {
        $beginUpdate(editor, nextUpdateFn, options);
      } else {
        nextUpdateFn();
      }
    }
  }
  return skipTransforms;
}
function $beginUpdate(editor, updateFn, options) {
  const updateTags = editor._updateTags;
  let onUpdate;
  let skipTransforms = false;
  let discrete = false;
  if (options !== void 0) {
    onUpdate = options.onUpdate;
    addTags(editor, options.tag);
    skipTransforms = options.skipTransforms || false;
    discrete = options.discrete || false;
  }
  if (onUpdate) {
    editor._deferred.push(onUpdate);
  }
  const currentEditorState = editor._editorState;
  let pendingEditorState = editor._pendingEditorState;
  let editorStateWasCloned = false;
  if (pendingEditorState === null || pendingEditorState._readOnly) {
    pendingEditorState = editor._pendingEditorState = cloneEditorState(pendingEditorState || currentEditorState);
    editorStateWasCloned = true;
  }
  pendingEditorState._flushSync = discrete;
  const previousActiveEditorState = activeEditorState;
  const previousReadOnlyMode = isReadOnlyMode;
  const previousActiveEditor = activeEditor;
  const previouslyUpdating = editor._updating;
  activeEditorState = pendingEditorState;
  isReadOnlyMode = false;
  editor._updating = true;
  activeEditor = editor;
  const headless = editor._headless || editor.getRootElement() === null;
  setPendingNodeToClone(null);
  try {
    if (editorStateWasCloned) {
      if (headless) {
        if (currentEditorState._selection !== null) {
          pendingEditorState._selection = currentEditorState._selection.clone();
        }
      } else {
        pendingEditorState._selection = $internalCreateSelection(editor, options && options.event || null);
      }
    }
    const startingCompositionKey = editor._compositionKey;
    updateFn();
    skipTransforms = $processNestedUpdates(editor, skipTransforms);
    applySelectionTransforms(pendingEditorState, editor);
    if (editor._dirtyType !== NO_DIRTY_NODES) {
      if (skipTransforms) {
        $normalizeAllDirtyTextNodes(pendingEditorState, editor);
      } else {
        $applyAllTransforms(pendingEditorState, editor);
      }
      $processNestedUpdates(editor);
      $garbageCollectDetachedNodes(currentEditorState, pendingEditorState, editor._dirtyLeaves, editor._dirtyElements);
    }
    const endingCompositionKey = editor._compositionKey;
    if (startingCompositionKey !== endingCompositionKey) {
      pendingEditorState._flushSync = true;
    }
    const pendingSelection = pendingEditorState._selection;
    if ($isRangeSelection(pendingSelection)) {
      const pendingNodeMap = pendingEditorState._nodeMap;
      const anchorKey = pendingSelection.anchor.key;
      const focusKey = pendingSelection.focus.key;
      if (pendingNodeMap.get(anchorKey) === void 0 || pendingNodeMap.get(focusKey) === void 0) {
        {
          formatDevErrorMessage(`updateEditor: selection has been lost because the previously selected nodes have been removed and selection wasn't moved to another node. Ensure selection changes after removing/replacing a selected node.`);
        }
      }
    } else if ($isNodeSelection(pendingSelection)) {
      if (pendingSelection._nodes.size === 0) {
        pendingEditorState._selection = null;
      }
    }
  } catch (error) {
    if (error instanceof Error) {
      editor._onError(error);
    }
    editor._pendingEditorState = currentEditorState;
    editor._dirtyType = FULL_RECONCILE;
    editor._cloneNotNeeded.clear();
    editor._dirtyLeaves = /* @__PURE__ */ new Set();
    editor._dirtyElements.clear();
    $commitPendingUpdates(editor);
    return;
  } finally {
    activeEditorState = previousActiveEditorState;
    isReadOnlyMode = previousReadOnlyMode;
    activeEditor = previousActiveEditor;
    editor._updating = previouslyUpdating;
    infiniteTransformCount = 0;
  }
  const shouldUpdate = editor._dirtyType !== NO_DIRTY_NODES || editor._deferred.length > 0 || editorStateHasDirtySelection(pendingEditorState, editor);
  if (shouldUpdate) {
    if (pendingEditorState._flushSync) {
      pendingEditorState._flushSync = false;
      $commitPendingUpdates(editor);
    } else if (editorStateWasCloned) {
      scheduleMicroTask(() => {
        $commitPendingUpdates(editor);
      });
    }
  } else {
    pendingEditorState._flushSync = false;
    if (editorStateWasCloned) {
      updateTags.clear();
      editor._deferred = [];
      editor._pendingEditorState = null;
    }
  }
}
function updateEditorSync(editor, updateFn, options) {
  if (activeEditor === editor && options === void 0) {
    updateFn();
  } else {
    $beginUpdate(editor, updateFn, options);
  }
}
function updateEditor(editor, updateFn, options) {
  if (editor._updating) {
    editor._updates.push([updateFn, options]);
  } else {
    $beginUpdate(editor, updateFn, options);
  }
}
var ElementDOMSlot = class _ElementDOMSlot {
  element;
  before;
  after;
  constructor(element, before, after) {
    this.element = element;
    this.before = before || null;
    this.after = after || null;
  }
  /**
   * Return a new ElementDOMSlot where all managed children will be inserted before this node
   */
  withBefore(before) {
    return new _ElementDOMSlot(this.element, before, this.after);
  }
  /**
   * Return a new ElementDOMSlot where all managed children will be inserted after this node
   */
  withAfter(after) {
    return new _ElementDOMSlot(this.element, this.before, after);
  }
  /**
   * Return a new ElementDOMSlot with an updated root element
   */
  withElement(element) {
    if (this.element === element) {
      return this;
    }
    return new _ElementDOMSlot(element, this.before, this.after);
  }
  /**
   * Insert the given child before this.before and any reconciler managed line break node,
   * or append it if this.before is not defined
   */
  insertChild(dom) {
    const before = this.before || this.getManagedLineBreak();
    if (!(before === null || before.parentElement === this.element)) {
      formatDevErrorMessage(`ElementDOMSlot.insertChild: before is not in element`);
    }
    this.element.insertBefore(dom, before);
    return this;
  }
  /**
   * Remove the managed child from this container, will throw if it was not already there
   */
  removeChild(dom) {
    if (!(dom.parentElement === this.element)) {
      formatDevErrorMessage(`ElementDOMSlot.removeChild: dom is not in element`);
    }
    this.element.removeChild(dom);
    return this;
  }
  /**
   * Replace managed child prevDom with dom. Will throw if prevDom is not a child
   *
   * @param dom The new node to replace prevDom
   * @param prevDom the node that will be replaced
   */
  replaceChild(dom, prevDom) {
    if (!(prevDom.parentElement === this.element)) {
      formatDevErrorMessage(`ElementDOMSlot.replaceChild: prevDom is not in element`);
    }
    this.element.replaceChild(dom, prevDom);
    return this;
  }
  /**
   * Returns the first managed child of this node,
   * which will either be this.after.nextSibling or this.element.firstChild,
   * and will never be this.before if it is defined.
   */
  getFirstChild() {
    const firstChild = this.after ? this.after.nextSibling : this.element.firstChild;
    return firstChild === this.before || firstChild === this.getManagedLineBreak() ? null : firstChild;
  }
  /**
   * @internal
   */
  getManagedLineBreak() {
    const element = this.element;
    return element.__lexicalLineBreak || null;
  }
  /** @internal */
  setManagedLineBreak(lineBreakType) {
    if (lineBreakType === null) {
      this.removeManagedLineBreak();
    } else {
      const webkitHack = lineBreakType === "decorator" && (IS_APPLE_WEBKIT || IS_IOS || IS_SAFARI);
      this.insertManagedLineBreak(webkitHack);
    }
  }
  /** @internal */
  removeManagedLineBreak() {
    const br = this.getManagedLineBreak();
    if (br) {
      const element = this.element;
      const sibling = br.nodeName === "IMG" ? br.nextSibling : null;
      if (sibling) {
        element.removeChild(sibling);
      }
      element.removeChild(br);
      element.__lexicalLineBreak = void 0;
    }
  }
  /** @internal */
  insertManagedLineBreak(webkitHack) {
    const prevBreak = this.getManagedLineBreak();
    if (prevBreak) {
      if (webkitHack === (prevBreak.nodeName === "IMG")) {
        return;
      }
      this.removeManagedLineBreak();
    }
    const element = this.element;
    const before = this.before;
    const br = document.createElement("br");
    element.insertBefore(br, before);
    if (webkitHack) {
      const img = document.createElement("img");
      img.setAttribute("data-lexical-linebreak", "true");
      img.style.setProperty("display", "inline", "important");
      img.style.setProperty("border", "0px", "important");
      img.style.setProperty("margin", "0px", "important");
      img.alt = "";
      element.insertBefore(img, br);
      element.__lexicalLineBreak = img;
    } else {
      element.__lexicalLineBreak = br;
    }
  }
  /**
   * @internal
   *
   * Returns the offset of the first child
   */
  getFirstChildOffset() {
    let i2 = 0;
    for (let node = this.after; node !== null; node = node.previousSibling) {
      i2++;
    }
    return i2;
  }
  /**
   * @internal
   */
  resolveChildIndex(element, elementDOM, initialDOM, initialOffset) {
    if (initialDOM === this.element) {
      const firstChildOffset = this.getFirstChildOffset();
      return [element, Math.min(firstChildOffset + element.getChildrenSize(), Math.max(firstChildOffset, initialOffset))];
    }
    const initialPath = indexPath(elementDOM, initialDOM);
    initialPath.push(initialOffset);
    const elementPath = indexPath(elementDOM, this.element);
    let offset = element.getIndexWithinParent();
    for (let i2 = 0; i2 < elementPath.length; i2++) {
      const target = initialPath[i2];
      const source = elementPath[i2];
      if (target === void 0 || target < source) {
        break;
      } else if (target > source) {
        offset += 1;
        break;
      }
    }
    return [element.getParentOrThrow(), offset];
  }
};
function indexPath(root, child) {
  const path = [];
  let node = child;
  for (; node !== root && node !== null; node = node.parentNode) {
    let i2 = 0;
    for (let sibling = node.previousSibling; sibling !== null; sibling = sibling.previousSibling) {
      i2++;
    }
    path.push(i2);
  }
  if (!(node === root)) {
    formatDevErrorMessage(`indexPath: root is not a parent of child`);
  }
  return path.reverse();
}
var ElementNode = class extends LexicalNode {
  /** @internal */
  /** @internal */
  __first;
  /** @internal */
  __last;
  /** @internal */
  __size;
  /** @internal */
  __format;
  /** @internal */
  __style;
  /** @internal */
  __indent;
  /** @internal */
  __dir;
  /** @internal */
  __textFormat;
  /** @internal */
  __textStyle;
  constructor(key) {
    super(key);
    this.__first = null;
    this.__last = null;
    this.__size = 0;
    this.__format = 0;
    this.__style = "";
    this.__indent = 0;
    this.__dir = null;
    this.__textFormat = 0;
    this.__textStyle = "";
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    if (this.__key === prevNode.__key) {
      this.__first = prevNode.__first;
      this.__last = prevNode.__last;
      this.__size = prevNode.__size;
    }
    this.__indent = prevNode.__indent;
    this.__format = prevNode.__format;
    this.__style = prevNode.__style;
    this.__dir = prevNode.__dir;
    this.__textFormat = prevNode.__textFormat;
    this.__textStyle = prevNode.__textStyle;
  }
  getFormat() {
    const self2 = this.getLatest();
    return self2.__format;
  }
  getFormatType() {
    const format = this.getFormat();
    return ELEMENT_FORMAT_TO_TYPE[format] || "";
  }
  getStyle() {
    const self2 = this.getLatest();
    return self2.__style;
  }
  getIndent() {
    const self2 = this.getLatest();
    return self2.__indent;
  }
  getChildren() {
    const children = [];
    let child = this.getFirstChild();
    while (child !== null) {
      children.push(child);
      child = child.getNextSibling();
    }
    return children;
  }
  getChildrenKeys() {
    const children = [];
    let child = this.getFirstChild();
    while (child !== null) {
      children.push(child.__key);
      child = child.getNextSibling();
    }
    return children;
  }
  getChildrenSize() {
    const self2 = this.getLatest();
    return self2.__size;
  }
  isEmpty() {
    return this.getChildrenSize() === 0;
  }
  isDirty() {
    const editor = getActiveEditor();
    const dirtyElements = editor._dirtyElements;
    return dirtyElements !== null && dirtyElements.has(this.__key);
  }
  isLastChild() {
    const self2 = this.getLatest();
    const parentLastChild = this.getParentOrThrow().getLastChild();
    return parentLastChild !== null && parentLastChild.is(self2);
  }
  getAllTextNodes() {
    const textNodes = [];
    let child = this.getFirstChild();
    while (child !== null) {
      if ($isTextNode(child)) {
        textNodes.push(child);
      }
      if ($isElementNode(child)) {
        const subChildrenNodes = child.getAllTextNodes();
        textNodes.push(...subChildrenNodes);
      }
      child = child.getNextSibling();
    }
    return textNodes;
  }
  getFirstDescendant() {
    let node = this.getFirstChild();
    while ($isElementNode(node)) {
      const child = node.getFirstChild();
      if (child === null) {
        break;
      }
      node = child;
    }
    return node;
  }
  getLastDescendant() {
    let node = this.getLastChild();
    while ($isElementNode(node)) {
      const child = node.getLastChild();
      if (child === null) {
        break;
      }
      node = child;
    }
    return node;
  }
  getDescendantByIndex(index) {
    const children = this.getChildren();
    const childrenLength = children.length;
    if (index >= childrenLength) {
      const resolvedNode2 = children[childrenLength - 1];
      return $isElementNode(resolvedNode2) && resolvedNode2.getLastDescendant() || resolvedNode2 || null;
    }
    const resolvedNode = children[index];
    return $isElementNode(resolvedNode) && resolvedNode.getFirstDescendant() || resolvedNode || null;
  }
  getFirstChild() {
    const self2 = this.getLatest();
    const firstKey = self2.__first;
    return firstKey === null ? null : $getNodeByKey(firstKey);
  }
  getFirstChildOrThrow() {
    const firstChild = this.getFirstChild();
    if (firstChild === null) {
      {
        formatDevErrorMessage(`Expected node ${this.__key} to have a first child.`);
      }
    }
    return firstChild;
  }
  getLastChild() {
    const self2 = this.getLatest();
    const lastKey = self2.__last;
    return lastKey === null ? null : $getNodeByKey(lastKey);
  }
  getLastChildOrThrow() {
    const lastChild = this.getLastChild();
    if (lastChild === null) {
      {
        formatDevErrorMessage(`Expected node ${this.__key} to have a last child.`);
      }
    }
    return lastChild;
  }
  getChildAtIndex(index) {
    const size = this.getChildrenSize();
    let node;
    let i2;
    if (index < size / 2) {
      node = this.getFirstChild();
      i2 = 0;
      while (node !== null && i2 <= index) {
        if (i2 === index) {
          return node;
        }
        node = node.getNextSibling();
        i2++;
      }
      return null;
    }
    node = this.getLastChild();
    i2 = size - 1;
    while (node !== null && i2 >= index) {
      if (i2 === index) {
        return node;
      }
      node = node.getPreviousSibling();
      i2--;
    }
    return null;
  }
  getTextContent() {
    let textContent = "";
    const children = this.getChildren();
    const childrenLength = children.length;
    for (let i2 = 0; i2 < childrenLength; i2++) {
      const child = children[i2];
      textContent += child.getTextContent();
      if (
        // this is an inline $textContentRequiresDoubleLinebreakAtEnd(child)
        $isElementNode(child) && i2 !== childrenLength - 1 && !child.isInline()
      ) {
        textContent += DOUBLE_LINE_BREAK;
      }
    }
    return textContent;
  }
  getTextContentSize() {
    let textContentSize = 0;
    const children = this.getChildren();
    const childrenLength = children.length;
    for (let i2 = 0; i2 < childrenLength; i2++) {
      const child = children[i2];
      textContentSize += child.getTextContentSize();
      if (
        // This is an inline $textContentRequiresDoubleLinebreakAtEnd(child)
        $isElementNode(child) && i2 !== childrenLength - 1 && !child.isInline()
      ) {
        textContentSize += DOUBLE_LINE_BREAK.length;
      }
    }
    return textContentSize;
  }
  getDirection() {
    const self2 = this.getLatest();
    return self2.__dir;
  }
  getTextFormat() {
    const self2 = this.getLatest();
    return self2.__textFormat;
  }
  hasFormat(type) {
    if (type !== "") {
      const formatFlag = ELEMENT_TYPE_TO_FORMAT[type];
      return (this.getFormat() & formatFlag) !== 0;
    }
    return false;
  }
  hasTextFormat(type) {
    const formatFlag = TEXT_TYPE_TO_FORMAT[type];
    return (this.getTextFormat() & formatFlag) !== 0;
  }
  /**
   * Returns the format flags applied to the node as a 32-bit integer.
   *
   * @returns a number representing the TextFormatTypes applied to the node.
   */
  getFormatFlags(type, alignWithFormat) {
    const self2 = this.getLatest();
    const format = self2.__textFormat;
    return toggleTextFormatType(format, type, alignWithFormat);
  }
  getTextStyle() {
    const self2 = this.getLatest();
    return self2.__textStyle;
  }
  // Mutators
  select(_anchorOffset, _focusOffset) {
    errorOnReadOnly();
    const selection = $getSelection();
    let anchorOffset = _anchorOffset;
    let focusOffset = _focusOffset;
    const childrenCount = this.getChildrenSize();
    if (!this.canBeEmpty()) {
      if (_anchorOffset === 0 && _focusOffset === 0) {
        const firstChild = this.getFirstChild();
        if ($isTextNode(firstChild) || $isElementNode(firstChild)) {
          return firstChild.select(0, 0);
        }
      } else if ((_anchorOffset === void 0 || _anchorOffset === childrenCount) && (_focusOffset === void 0 || _focusOffset === childrenCount)) {
        const lastChild = this.getLastChild();
        if ($isTextNode(lastChild) || $isElementNode(lastChild)) {
          return lastChild.select();
        }
      }
    }
    if (anchorOffset === void 0) {
      anchorOffset = childrenCount;
    }
    if (focusOffset === void 0) {
      focusOffset = childrenCount;
    }
    const key = this.__key;
    if (!$isRangeSelection(selection)) {
      return $internalMakeRangeSelection(key, anchorOffset, key, focusOffset, "element", "element");
    } else {
      selection.anchor.set(key, anchorOffset, "element");
      selection.focus.set(key, focusOffset, "element");
      selection.dirty = true;
    }
    return selection;
  }
  selectStart() {
    const firstNode = this.getFirstDescendant();
    return firstNode ? firstNode.selectStart() : this.select();
  }
  selectEnd() {
    const lastNode = this.getLastDescendant();
    return lastNode ? lastNode.selectEnd() : this.select();
  }
  clear() {
    const writableSelf = this.getWritable();
    const children = this.getChildren();
    children.forEach((child) => child.remove());
    return writableSelf;
  }
  append(...nodesToAppend) {
    return this.splice(this.getChildrenSize(), 0, nodesToAppend);
  }
  setDirection(direction) {
    const self2 = this.getWritable();
    self2.__dir = direction;
    return self2;
  }
  setFormat(type) {
    const self2 = this.getWritable();
    self2.__format = type !== "" ? ELEMENT_TYPE_TO_FORMAT[type] || 0 : 0;
    return this;
  }
  setStyle(style) {
    const self2 = this.getWritable();
    self2.__style = style || "";
    return this;
  }
  setTextFormat(type) {
    const self2 = this.getWritable();
    self2.__textFormat = type;
    return self2;
  }
  setTextStyle(style) {
    const self2 = this.getWritable();
    self2.__textStyle = style;
    return self2;
  }
  setIndent(indentLevel) {
    const self2 = this.getWritable();
    self2.__indent = indentLevel;
    return this;
  }
  splice(start, deleteCount, nodesToInsert) {
    if (!!$isEphemeral(this)) {
      formatDevErrorMessage(`ElementNode.splice: Ephemeral nodes can not mutate their children (key ${this.__key} type ${this.__type})`);
    }
    const oldSize = this.getChildrenSize();
    const writableSelf = this.getWritable();
    if (!(start + deleteCount <= oldSize)) {
      formatDevErrorMessage(`ElementNode.splice: start + deleteCount > oldSize (${String(start)} + ${String(deleteCount)} > ${String(oldSize)})`);
    }
    const writableSelfKey = writableSelf.__key;
    const nodesToInsertKeys = [];
    const nodesToRemoveKeys = [];
    const nodeAfterRange = this.getChildAtIndex(start + deleteCount);
    let nodeBeforeRange = null;
    let newSize = oldSize - deleteCount + nodesToInsert.length;
    if (start !== 0) {
      if (start === oldSize) {
        nodeBeforeRange = this.getLastChild();
      } else {
        const node = this.getChildAtIndex(start);
        if (node !== null) {
          nodeBeforeRange = node.getPreviousSibling();
        }
      }
    }
    if (deleteCount > 0) {
      let nodeToDelete = nodeBeforeRange === null ? this.getFirstChild() : nodeBeforeRange.getNextSibling();
      for (let i2 = 0; i2 < deleteCount; i2++) {
        if (nodeToDelete === null) {
          {
            formatDevErrorMessage(`splice: sibling not found`);
          }
        }
        const nextSibling = nodeToDelete.getNextSibling();
        const nodeKeyToDelete = nodeToDelete.__key;
        const writableNodeToDelete = nodeToDelete.getWritable();
        removeFromParent(writableNodeToDelete);
        nodesToRemoveKeys.push(nodeKeyToDelete);
        nodeToDelete = nextSibling;
      }
    }
    let prevNode = nodeBeforeRange;
    for (const nodeToInsert of nodesToInsert) {
      if (prevNode !== null && nodeToInsert.is(prevNode)) {
        nodeBeforeRange = prevNode = prevNode.getPreviousSibling();
      }
      const writableNodeToInsert = nodeToInsert.getWritable();
      if (writableNodeToInsert.__parent === writableSelfKey) {
        newSize--;
      }
      removeFromParent(writableNodeToInsert);
      const nodeKeyToInsert = nodeToInsert.__key;
      if (prevNode === null) {
        writableSelf.__first = nodeKeyToInsert;
        writableNodeToInsert.__prev = null;
      } else {
        const writablePrevNode = prevNode.getWritable();
        writablePrevNode.__next = nodeKeyToInsert;
        writableNodeToInsert.__prev = writablePrevNode.__key;
      }
      if (nodeToInsert.__key === writableSelfKey) {
        {
          formatDevErrorMessage(`append: attempting to append self`);
        }
      }
      writableNodeToInsert.__parent = writableSelfKey;
      nodesToInsertKeys.push(nodeKeyToInsert);
      prevNode = nodeToInsert;
    }
    if (start + deleteCount === oldSize) {
      if (prevNode !== null) {
        const writablePrevNode = prevNode.getWritable();
        writablePrevNode.__next = null;
        writableSelf.__last = prevNode.__key;
      }
    } else if (nodeAfterRange !== null) {
      const writableNodeAfterRange = nodeAfterRange.getWritable();
      if (prevNode !== null) {
        const writablePrevNode = prevNode.getWritable();
        writableNodeAfterRange.__prev = prevNode.__key;
        writablePrevNode.__next = nodeAfterRange.__key;
      } else {
        writableNodeAfterRange.__prev = null;
      }
    }
    writableSelf.__size = newSize;
    if (nodesToRemoveKeys.length) {
      const selection = $getSelection();
      if ($isRangeSelection(selection)) {
        const nodesToRemoveKeySet = new Set(nodesToRemoveKeys);
        const nodesToInsertKeySet = new Set(nodesToInsertKeys);
        const {
          anchor,
          focus
        } = selection;
        if (isPointRemoved(anchor, nodesToRemoveKeySet, nodesToInsertKeySet)) {
          moveSelectionPointToSibling(anchor, anchor.getNode(), this, nodeBeforeRange, nodeAfterRange);
        }
        if (isPointRemoved(focus, nodesToRemoveKeySet, nodesToInsertKeySet)) {
          moveSelectionPointToSibling(focus, focus.getNode(), this, nodeBeforeRange, nodeAfterRange);
        }
        if (newSize === 0 && !this.canBeEmpty() && !$isRootOrShadowRoot(this)) {
          this.remove();
        }
      }
    }
    return writableSelf;
  }
  /**
   * @internal
   *
   * An experimental API that an ElementNode can override to control where its
   * children are inserted into the DOM, this is useful to add a wrapping node
   * or accessory nodes before or after the children. The root of the node returned
   * by createDOM must still be exactly one HTMLElement.
   */
  getDOMSlot(element) {
    return new ElementDOMSlot(element);
  }
  exportDOM(editor) {
    const {
      element
    } = super.exportDOM(editor);
    if (isHTMLElement(element)) {
      const indent = this.getIndent();
      if (indent > 0) {
        element.style.paddingInlineStart = `${indent * 40}px`;
      }
      const direction = this.getDirection();
      if (direction) {
        element.dir = direction;
      }
    }
    return {
      element
    };
  }
  // JSON serialization
  exportJSON() {
    const json = {
      children: [],
      direction: this.getDirection(),
      format: this.getFormatType(),
      indent: this.getIndent(),
      // As an exception here we invoke super at the end for historical reasons.
      // Namely, to preserve the order of the properties and not to break the tests
      // that use the serialized string representation.
      ...super.exportJSON()
    };
    const textFormat = this.getTextFormat();
    const textStyle = this.getTextStyle();
    if ((textFormat !== 0 || textStyle !== "") && !$isRootOrShadowRoot(this) && !this.getChildren().some($isTextNode)) {
      if (textFormat !== 0) {
        json.textFormat = textFormat;
      }
      if (textStyle !== "") {
        json.textStyle = textStyle;
      }
    }
    return json;
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setFormat(serializedNode.format).setIndent(serializedNode.indent).setDirection(serializedNode.direction).setTextFormat(serializedNode.textFormat || 0).setTextStyle(serializedNode.textStyle || "");
  }
  // These are intended to be extends for specific element heuristics.
  insertNewAfter(selection, restoreSelection) {
    return null;
  }
  canIndent() {
    return true;
  }
  /*
   * This method controls the behavior of the node during backwards
   * deletion (i.e., backspace) when selection is at the beginning of
   * the node (offset 0). You may use this to have the node replace
   * itself, change its state, or do nothing. When you do make such
   * a change, you should return true.
   *
   * When true is returned, the collapse phase will stop.
   * When false is returned, and isInline() is true, and getPreviousSibling() is null,
   * then this function will be called on its parent.
   */
  collapseAtStart(selection) {
    return false;
  }
  excludeFromCopy(destination) {
    return false;
  }
  /** @deprecated @internal */
  canReplaceWith(replacement) {
    return true;
  }
  /** @deprecated @internal */
  canInsertAfter(node) {
    return true;
  }
  canBeEmpty() {
    return true;
  }
  canInsertTextBefore() {
    return true;
  }
  canInsertTextAfter() {
    return true;
  }
  isInline() {
    return false;
  }
  // A shadow root is a Node that behaves like RootNode. The shadow root (and RootNode) mark the
  // end of the hierarchy, most implementations should treat it as there's nothing (upwards)
  // beyond this point. For example, node.getTopLevelElement(), when performed inside a TableCellNode
  // will return the immediate first child underneath TableCellNode instead of RootNode.
  isShadowRoot() {
    return false;
  }
  /** @deprecated @internal */
  canMergeWith(node) {
    return false;
  }
  extractWithChild(child, selection, destination) {
    return false;
  }
  /**
   * Determines whether this node, when empty, can merge with a first block
   * of nodes being inserted.
   *
   * This method is specifically called in {@link RangeSelection.insertNodes}
   * to determine merging behavior during nodes insertion.
   *
   * @example
   * // In a ListItemNode or QuoteNode implementation:
   * canMergeWhenEmpty(): true {
   *  return true;
   * }
   */
  canMergeWhenEmpty() {
    return false;
  }
  /** @internal */
  reconcileObservedMutation(dom, editor) {
    const slot = $getEditorDOMRenderConfig(editor).$getDOMSlot(this, dom, editor);
    let currentDOM = slot.getFirstChild();
    for (let currentNode = this.getFirstChild(); currentNode; currentNode = currentNode.getNextSibling()) {
      const correctDOM = editor.getElementByKey(currentNode.getKey());
      if (correctDOM === null) {
        continue;
      }
      if (currentDOM == null) {
        slot.insertChild(correctDOM);
        currentDOM = correctDOM;
      } else if (currentDOM !== correctDOM) {
        slot.replaceChild(correctDOM, currentDOM);
      }
      currentDOM = currentDOM.nextSibling;
    }
  }
};
function $isElementNode(node) {
  return node instanceof ElementNode;
}
function isPointRemoved(point, nodesToRemoveKeySet, nodesToInsertKeySet) {
  let node = point.getNode();
  while (node) {
    const nodeKey = node.__key;
    if (nodesToRemoveKeySet.has(nodeKey) && !nodesToInsertKeySet.has(nodeKey)) {
      return true;
    }
    node = node.getParent();
  }
  return false;
}
var DecoratorNode = class extends LexicalNode {
  /** @internal */
  /**
   * The returned value is added to the LexicalEditor._decorators
   */
  decorate(editor, config) {
    return null;
  }
  isIsolated() {
    return false;
  }
  isInline() {
    return true;
  }
  isKeyboardSelectable() {
    return true;
  }
};
function $isDecoratorNode(node) {
  return node instanceof DecoratorNode;
}
var RootNode = class _RootNode extends ElementNode {
  /** @internal */
  __cachedText;
  static getType() {
    return "root";
  }
  static clone() {
    return new _RootNode();
  }
  constructor() {
    super("root");
    this.__cachedText = null;
  }
  getTopLevelElementOrThrow() {
    {
      formatDevErrorMessage(`getTopLevelElementOrThrow: root nodes are not top level elements`);
    }
  }
  getTextContent() {
    const cachedText = this.__cachedText;
    if (isCurrentlyReadOnlyMode() || getActiveEditor()._dirtyType === NO_DIRTY_NODES) {
      if (cachedText !== null) {
        return cachedText;
      }
    }
    return super.getTextContent();
  }
  remove() {
    {
      formatDevErrorMessage(`remove: cannot be called on root nodes`);
    }
  }
  replace(node) {
    {
      formatDevErrorMessage(`replace: cannot be called on root nodes`);
    }
  }
  insertBefore(nodeToInsert) {
    {
      formatDevErrorMessage(`insertBefore: cannot be called on root nodes`);
    }
  }
  insertAfter(nodeToInsert) {
    {
      formatDevErrorMessage(`insertAfter: cannot be called on root nodes`);
    }
  }
  // View
  updateDOM(prevNode, dom) {
    return false;
  }
  // Mutate
  splice(start, deleteCount, nodesToInsert) {
    for (const node of nodesToInsert) {
      if (!($isElementNode(node) || $isDecoratorNode(node))) {
        formatDevErrorMessage(`rootNode.splice: Only element or decorator nodes can be inserted to the root node`);
      }
    }
    return super.splice(start, deleteCount, nodesToInsert);
  }
  static importJSON(serializedNode) {
    return $getRoot().updateFromJSON(serializedNode);
  }
  collapseAtStart() {
    return true;
  }
};
function $createRootNode() {
  return new RootNode();
}
function $isRootNode(node) {
  return node instanceof RootNode;
}
function editorStateHasDirtySelection(editorState, editor) {
  const currentSelection = editor.getEditorState()._selection;
  const pendingSelection = editorState._selection;
  if (pendingSelection !== null) {
    if (pendingSelection.dirty || !pendingSelection.is(currentSelection)) {
      return true;
    }
  } else if (currentSelection !== null) {
    return true;
  }
  return false;
}
function cloneEditorState(current) {
  return new EditorState(new Map(current._nodeMap));
}
function createEmptyEditorState() {
  return new EditorState(/* @__PURE__ */ new Map([["root", $createRootNode()]]));
}
function exportNodeToJSON(node) {
  const serializedNode = node.exportJSON();
  const nodeClass = node.constructor;
  if (serializedNode.type !== nodeClass.getType()) {
    {
      formatDevErrorMessage(`LexicalNode: Node ${nodeClass.name} does not match the serialized type. Check if .exportJSON() is implemented and it is returning the correct type.`);
    }
  }
  if ($isElementNode(node)) {
    const serializedChildren = serializedNode.children;
    if (!Array.isArray(serializedChildren)) {
      {
        formatDevErrorMessage(`LexicalNode: Node ${nodeClass.name} is an element but .exportJSON() does not have a children array.`);
      }
    }
    const children = node.getChildren();
    for (let i2 = 0; i2 < children.length; i2++) {
      const child = children[i2];
      const serializedChildNode = exportNodeToJSON(child);
      serializedChildren.push(serializedChildNode);
    }
  }
  return serializedNode;
}
function $isEditorState(x2) {
  return x2 instanceof EditorState;
}
var EditorState = class _EditorState {
  _nodeMap;
  _selection;
  _flushSync;
  _readOnly;
  constructor(nodeMap, selection) {
    this._nodeMap = nodeMap;
    this._selection = selection || null;
    this._flushSync = false;
    this._readOnly = false;
  }
  isEmpty() {
    return this._nodeMap.size === 1 && this._selection === null;
  }
  read(callbackFn, options) {
    return readEditorState(options && options.editor || null, this, callbackFn);
  }
  clone(selection) {
    const editorState = new _EditorState(this._nodeMap, selection === void 0 ? this._selection : selection);
    editorState._readOnly = true;
    return editorState;
  }
  toJSON() {
    return readEditorState(null, this, () => ({
      root: exportNodeToJSON($getRoot())
    }));
  }
};
var ArtificialNode__DO_NOT_USE = class extends ElementNode {
  static getType() {
    return "artificial";
  }
  createDOM(config) {
    const dom = document.createElement("div");
    return dom;
  }
};
var ParagraphNode = class _ParagraphNode extends ElementNode {
  /** @internal */
  static getType() {
    return "paragraph";
  }
  static clone(node) {
    return new _ParagraphNode(node.__key);
  }
  // View
  createDOM(config) {
    const dom = document.createElement("p");
    const classNames = getCachedClassNameArray(config.theme, "paragraph");
    if (classNames !== void 0) {
      const domClassList = dom.classList;
      domClassList.add(...classNames);
    }
    return dom;
  }
  updateDOM(prevNode, dom, config) {
    return false;
  }
  static importDOM() {
    return {
      p: (node) => ({
        conversion: $convertParagraphElement,
        priority: 0
      })
    };
  }
  exportDOM(editor) {
    const {
      element
    } = super.exportDOM(editor);
    if (isHTMLElement(element)) {
      if (this.isEmpty()) {
        element.append(document.createElement("br"));
      }
      const formatType = this.getFormatType();
      if (formatType) {
        element.style.textAlign = formatType;
      }
    }
    return {
      element
    };
  }
  static importJSON(serializedNode) {
    return $createParagraphNode().updateFromJSON(serializedNode);
  }
  exportJSON() {
    const json = super.exportJSON();
    if (json.textFormat === void 0 || json.textStyle === void 0) {
      const firstTextNode = this.getChildren().find($isTextNode);
      if (firstTextNode) {
        json.textFormat = firstTextNode.getFormat();
        json.textStyle = firstTextNode.getStyle();
      } else {
        json.textFormat = this.getTextFormat();
        json.textStyle = this.getTextStyle();
      }
    }
    return json;
  }
  // Mutation
  insertNewAfter(rangeSelection, restoreSelection) {
    const newElement = $createParagraphNode();
    newElement.setTextFormat(rangeSelection.format);
    newElement.setTextStyle(rangeSelection.style);
    const direction = this.getDirection();
    newElement.setDirection(direction);
    newElement.setFormat(this.getFormatType());
    newElement.setStyle(this.getStyle());
    this.insertAfter(newElement, restoreSelection);
    return newElement;
  }
  collapseAtStart() {
    const children = this.getChildren();
    if (children.length === 0 || $isTextNode(children[0]) && children[0].getTextContent().trim() === "") {
      const nextSibling = this.getNextSibling();
      if (nextSibling !== null) {
        this.selectNext();
        this.remove();
        return true;
      }
      const prevSibling = this.getPreviousSibling();
      if (prevSibling !== null) {
        this.selectPrevious();
        this.remove();
        return true;
      }
    }
    return false;
  }
};
function $convertParagraphElement(element) {
  const node = $createParagraphNode();
  if (element.style) {
    node.setFormat(element.style.textAlign);
    setNodeIndentFromDOM(element, node);
  }
  if (node.getFormatType() === "") {
    const align = element.getAttribute("align");
    if (align) {
      if (align && align in ELEMENT_TYPE_TO_FORMAT) {
        node.setFormat(align);
      }
    }
  }
  return {
    node
  };
}
function $createParagraphNode() {
  return $applyNodeReplacement(new ParagraphNode());
}
function $isParagraphNode(node) {
  return node instanceof ParagraphNode;
}
var DEFAULT_SKIP_INITIALIZATION = false;
var COMMAND_PRIORITY_EDITOR = 0;
var COMMAND_PRIORITY_LOW = 1;
var COMMAND_PRIORITY_NORMAL = 2;
var COMMAND_PRIORITY_HIGH = 3;
var COMMAND_PRIORITY_CRITICAL = 4;
var COMMAND_PRIORITY_BEFORE_EDITOR = -8;
var COMMAND_PRIORITY_BEFORE_LOW = -7;
var COMMAND_PRIORITY_BEFORE_NORMAL = -6;
var COMMAND_PRIORITY_BEFORE_HIGH = -5;
var COMMAND_PRIORITY_BEFORE_CRITICAL = -4;
function normalizePriority(priority) {
  return priority & 7;
}
function resetEditor(editor, prevRootElement, nextRootElement, pendingEditorState) {
  const keyNodeMap = editor._keyToDOMMap;
  keyNodeMap.clear();
  editor._editorState = createEmptyEditorState();
  editor._pendingEditorState = pendingEditorState;
  editor._compositionKey = null;
  editor._dirtyType = NO_DIRTY_NODES;
  editor._cloneNotNeeded.clear();
  editor._dirtyLeaves = /* @__PURE__ */ new Set();
  editor._dirtyElements.clear();
  editor._normalizedNodes = /* @__PURE__ */ new Set();
  editor._updateTags = /* @__PURE__ */ new Set();
  editor._updates = [];
  editor._blockCursorElement = null;
  const observer = editor._observer;
  if (observer !== null) {
    observer.disconnect();
    editor._observer = null;
  }
  if (prevRootElement !== null) {
    prevRootElement.textContent = "";
  }
  if (nextRootElement !== null) {
    nextRootElement.textContent = "";
    keyNodeMap.set("root", nextRootElement);
  }
}
function initializeConversionCache(nodes, additionalConversions) {
  const conversionCache = /* @__PURE__ */ new Map();
  const handledConversions = /* @__PURE__ */ new Set();
  const addConversionsToCache = (map) => {
    Object.keys(map).forEach((key) => {
      let currentCache = conversionCache.get(key);
      if (currentCache === void 0) {
        currentCache = [];
        conversionCache.set(key, currentCache);
      }
      currentCache.push(map[key]);
    });
  };
  nodes.forEach((node) => {
    const importDOM = node.klass.importDOM;
    if (importDOM == null || handledConversions.has(importDOM)) {
      return;
    }
    handledConversions.add(importDOM);
    const map = importDOM.call(node.klass);
    if (map !== null) {
      addConversionsToCache(map);
    }
  });
  if (additionalConversions) {
    addConversionsToCache(additionalConversions);
  }
  return conversionCache;
}
function getTransformSetFromKlass(klass) {
  const transforms = /* @__PURE__ */ new Set();
  const staticTransforms = /* @__PURE__ */ new Set();
  let currentKlass = klass;
  while (currentKlass) {
    const {
      ownNodeConfig
    } = getStaticNodeConfig(currentKlass);
    const staticTransform = currentKlass.transform;
    if (!staticTransforms.has(staticTransform)) {
      staticTransforms.add(staticTransform);
      const transform = currentKlass.transform();
      if (transform) {
        transforms.add(transform);
      }
    }
    if (ownNodeConfig) {
      const $transform = ownNodeConfig.$transform;
      if ($transform) {
        transforms.add($transform);
      }
      currentKlass = ownNodeConfig.extends;
    } else {
      const parent = Object.getPrototypeOf(currentKlass);
      currentKlass = parent.prototype instanceof LexicalNode && parent !== LexicalNode ? parent : void 0;
    }
  }
  return transforms;
}
var DEFAULT_EDITOR_DOM_CONFIG = {
  $createDOM: (node, editor) => node.createDOM(editor._config, editor),
  $decorateDOM: (_node, _prevNode, _dom, _editor) => {
  },
  $exportDOM: (node, editor) => {
    const registeredNode = getRegisteredNode(editor, node.getType());
    return registeredNode && registeredNode.exportDOM !== void 0 ? registeredNode.exportDOM(editor, node) : node.exportDOM(editor);
  },
  $extractWithChild: (node, childNode, selection, destination, _editor) => $isElementNode(node) && node.extractWithChild(childNode, selection, destination),
  $getDOMSlot: (node, dom, _editor) => {
    if (!$isElementNode(node)) {
      formatDevErrorMessage(`$getDOMSlot called on a non-ElementNode (key ${node.getKey()} type ${node.getType()})`);
    }
    return node.getDOMSlot(dom);
  },
  $shouldExclude: (node, _selection, _editor) => $isElementNode(node) && node.excludeFromCopy("html"),
  $shouldInclude: (node, selection, _editor) => selection ? node.isSelected(selection) : true,
  $updateDOM: (nextNode, prevNode, dom, editor) => nextNode.updateDOM(prevNode, dom, editor._config)
};
function createEditor(editorConfig) {
  const config = editorConfig || {};
  const activeEditor2 = internalGetActiveEditor();
  const theme = config.theme || {};
  const parentEditor = editorConfig === void 0 ? activeEditor2 : config.parentEditor || null;
  const disableEvents = config.disableEvents || false;
  const editorState = createEmptyEditorState();
  const namespace = config.namespace || (parentEditor !== null ? parentEditor._config.namespace : createUID());
  const initialEditorState = config.editorState;
  const nodes = [RootNode, TextNode, LineBreakNode, TabNode, ParagraphNode, ArtificialNode__DO_NOT_USE, ...config.nodes || []];
  const {
    onError,
    html
  } = config;
  const isEditable = config.editable !== void 0 ? config.editable : true;
  let registeredNodes;
  if (editorConfig === void 0 && activeEditor2 !== null) {
    registeredNodes = activeEditor2._nodes;
  } else {
    registeredNodes = /* @__PURE__ */ new Map();
    for (let i2 = 0; i2 < nodes.length; i2++) {
      let klass = nodes[i2];
      let replace = null;
      let replaceWithKlass = null;
      if (typeof klass !== "function") {
        const options = klass;
        klass = options.replace;
        replace = options.with;
        replaceWithKlass = options.withKlass || null;
      }
      void getStaticNodeConfig(klass);
      {
        const name = klass.name;
        const nodeType = hasOwnStaticMethod(klass, "getType") && klass.getType();
        if (replaceWithKlass) {
          if (!(replaceWithKlass.prototype instanceof klass)) {
            formatDevErrorMessage(`${replaceWithKlass.name} doesn't extend the ${name}`);
          }
        } else if (replace) {
          console.warn(`Override for ${name} specifies 'replace' without 'withKlass'. 'withKlass' will be required in a future version.`);
        }
        if (name !== "RootNode" && nodeType !== "root" && nodeType !== "artificial" && // This is mostly for the unit test suite which
        // uses LexicalNode in an otherwise incorrect way
        // by mocking its static getType
        klass !== LexicalNode) {
          ["getType", "clone"].forEach((method) => {
            if (!hasOwnStaticMethod(klass, method)) {
              console.warn(`${name} must implement static "${method}" method`);
            }
          });
          if (!hasOwnStaticMethod(klass, "importDOM") && hasOwnExportDOM(klass)) {
            console.warn(`${name} should implement "importDOM" if using a custom "exportDOM" method to ensure HTML serialization (important for copy & paste) works as expected`);
          }
          if (!hasOwnStaticMethod(klass, "importJSON")) {
            console.warn(`${name} should implement "importJSON" method to ensure JSON and default HTML serialization works as expected`);
          }
        }
      }
      const type = klass.getType();
      const transforms = getTransformSetFromKlass(klass);
      registeredNodes.set(type, {
        exportDOM: html && html.export ? html.export.get(klass) : void 0,
        klass,
        replace,
        replaceWithKlass,
        sharedNodeState: createSharedNodeState(nodes[i2]),
        transforms
      });
    }
  }
  const editor = new LexicalEditor(editorState, parentEditor, registeredNodes, {
    disableEvents,
    dom: {
      ...DEFAULT_EDITOR_DOM_CONFIG,
      ...editorConfig && editorConfig.dom
    },
    namespace,
    theme
  }, onError ? onError : console.error, initializeConversionCache(registeredNodes, html ? html.import : void 0), isEditable, editorConfig);
  if (initialEditorState !== void 0) {
    editor._pendingEditorState = initialEditorState;
    editor._dirtyType = FULL_RECONCILE;
  }
  registerDefaultCommandHandlers(editor);
  return editor;
}
function triggerListener(listenerMap, listener, args) {
  const unregister = listenerMap.get(listener);
  if (unregister) {
    unregister();
  }
  listenerMap.set(listener, listener(...args) || void 0);
}
function unregisterListener(listenerMap, listener) {
  const unregister = listenerMap.get(listener);
  listenerMap.delete(listener);
  if (unregister) {
    unregister();
  }
}
function registerListener(listenerMap, listener, unregister) {
  listenerMap.set(listener, unregister);
  return unregisterListener.bind(null, listenerMap, listener);
}
var LexicalEditor = class {
  /** @internal */
  /** The version with build identifiers for this editor (since 0.17.1) */
  static version;
  /** @internal */
  _headless;
  /** @internal */
  _parentEditor;
  /** @internal */
  _rootElement;
  /** @internal */
  _editorState;
  /** @internal */
  _pendingEditorState;
  /** @internal */
  _compositionKey;
  /** @internal */
  _deferred;
  /** @internal */
  _keyToDOMMap;
  /** @internal */
  _updates;
  /** @internal */
  _updating;
  /** @internal */
  _listeners;
  /** @internal */
  _commands;
  /** @internal */
  _nodes;
  /** @internal */
  _decorators;
  /** @internal */
  _pendingDecorators;
  /** @internal */
  _config;
  /** @internal */
  _dirtyType;
  /** @internal */
  _cloneNotNeeded;
  /** @internal */
  _dirtyLeaves;
  /** @internal */
  _dirtyElements;
  /** @internal */
  _normalizedNodes;
  /** @internal */
  _updateTags;
  /** @internal */
  _observer;
  /** @internal */
  _key;
  /** @internal */
  _onError;
  /** @internal */
  _htmlConversions;
  /** @internal */
  _window;
  /** @internal */
  _editable;
  /** @internal */
  _blockCursorElement;
  /** @internal */
  _createEditorArgs;
  /** @internal */
  constructor(editorState, parentEditor, nodes, config, onError, htmlConversions, editable, createEditorArgs) {
    this._createEditorArgs = createEditorArgs;
    this._parentEditor = parentEditor;
    this._rootElement = null;
    this._editorState = editorState;
    this._pendingEditorState = null;
    this._compositionKey = null;
    this._deferred = [];
    this._keyToDOMMap = /* @__PURE__ */ new Map();
    this._updates = [];
    this._updating = false;
    this._listeners = {
      decorator: /* @__PURE__ */ new Map(),
      editable: /* @__PURE__ */ new Map(),
      mutation: /* @__PURE__ */ new Map(),
      root: /* @__PURE__ */ new Map(),
      textcontent: /* @__PURE__ */ new Map(),
      update: /* @__PURE__ */ new Map()
    };
    this._commands = /* @__PURE__ */ new Map();
    this._config = config;
    this._nodes = nodes;
    this._decorators = {};
    this._pendingDecorators = null;
    this._dirtyType = NO_DIRTY_NODES;
    this._cloneNotNeeded = /* @__PURE__ */ new Set();
    this._dirtyLeaves = /* @__PURE__ */ new Set();
    this._dirtyElements = /* @__PURE__ */ new Map();
    this._normalizedNodes = /* @__PURE__ */ new Set();
    this._updateTags = /* @__PURE__ */ new Set();
    this._observer = null;
    this._key = createUID();
    this._onError = onError;
    this._htmlConversions = htmlConversions;
    this._editable = editable;
    this._headless = parentEditor !== null && parentEditor._headless;
    this._window = null;
    this._blockCursorElement = null;
  }
  /**
   *
   * @returns true if the editor is currently in "composition" mode due to receiving input
   * through an IME, or 3P extension, for example. Returns false otherwise.
   */
  isComposing() {
    return this._compositionKey != null;
  }
  /**
   * Registers a listener for Editor update event. Will trigger the provided callback
   * each time the editor goes through an update (via {@link LexicalEditor.update}) until the
   * teardown function is called.
   *
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerUpdateListener(listener) {
    return registerListener(this._listeners.update, listener);
  }
  /**
   * Registers a listener for for when the editor changes between editable and non-editable states.
   * Will trigger the provided callback each time the editor transitions between these states until the
   * teardown function is called.
   *
   * If the listener returns a function, that function will be called before the next transition or
   * teardown.
   *
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerEditableListener(listener) {
    return registerListener(this._listeners.editable, listener);
  }
  /**
   * Registers a listener for when the editor's decorator object changes. The decorator object contains
   * all DecoratorNode keys -> their decorated value. This is primarily used with external UI frameworks.
   *
   * Will trigger the provided callback each time the editor transitions between these states until the
   * teardown function is called.
   *
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerDecoratorListener(listener) {
    return registerListener(this._listeners.decorator, listener);
  }
  /**
   * Registers a listener for when Lexical commits an update to the DOM and the text content of
   * the editor changes from the previous state of the editor. If the text content is the
   * same between updates, no notifications to the listeners will happen.
   *
   * Will trigger the provided callback each time the editor transitions between these states until the
   * teardown function is called.
   *
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerTextContentListener(listener) {
    return registerListener(this._listeners.textcontent, listener);
  }
  /**
   * Registers a listener for when the editor's root DOM element (the content editable
   * Lexical attaches to) changes. This is primarily used to attach event listeners to the root
   *  element. The root listener function is executed directly upon registration and then on
   * any subsequent update.
   *
   * Will trigger the provided callback each time the editor transitions between these states until the
   * teardown function is called.
   *
   * If the listener returns a function, that function will be called before the next transition or
   * teardown.
   *
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerRootListener(listener) {
    const listenerMap = this._listeners.root;
    return mergeRegister(registerListener(listenerMap, listener, listener(this._rootElement, null) || void 0), () => triggerListener(listenerMap, listener, [null, this._rootElement]));
  }
  /**
   * Registers a listener that will trigger anytime the provided command
   * is dispatched with {@link LexicalEditor.dispatch}, subject to priority.
   * Listeners that run at a higher priority can "intercept" commands and
   * prevent them from propagating to other handlers by returning true.
   *
   * Listeners are always invoked in an {@link LexicalEditor.update} and can
   * call dollar functions.
   *
   * Listeners registered at the same priority level will run
   * deterministically in the order of registration.
   *
   * @param command - the command that will trigger the callback.
   * @param listener - the function that will execute when the command is dispatched.
   * @param priority - the relative priority of the listener. 0 | 1 | 2 | 3 | 4
   *   (or {@link COMMAND_PRIORITY_EDITOR} |
   *     {@link COMMAND_PRIORITY_LOW} |
   *     {@link COMMAND_PRIORITY_NORMAL} |
   *     {@link COMMAND_PRIORITY_HIGH} |
   *     {@link COMMAND_PRIORITY_CRITICAL})
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerCommand(command, listener, priority) {
    if (priority === void 0) {
      {
        formatDevErrorMessage(`Listener for type "command" requires a "priority".`);
      }
    }
    const commandsMap = this._commands;
    if (!commandsMap.has(command)) {
      commandsMap.set(command, [new DequeSet(), new DequeSet(), new DequeSet(), new DequeSet(), new DequeSet()]);
    }
    const listenersInPriorityOrder = commandsMap.get(command);
    if (listenersInPriorityOrder === void 0) {
      {
        formatDevErrorMessage(`registerCommand: Command ${String(command)} not found in command map`);
      }
    }
    const normalizedPriority = normalizePriority(priority);
    const listeners = listenersInPriorityOrder[normalizedPriority];
    if (normalizedPriority !== priority) {
      listeners.addFront(listener);
    } else {
      listeners.addBack(listener);
    }
    return () => {
      listeners.delete(listener);
      if (listenersInPriorityOrder.every((listenersSet) => listenersSet.size === 0)) {
        commandsMap.delete(command);
      }
    };
  }
  /**
   * Registers a listener that will run when a Lexical node of the provided class is
   * mutated. The listener will receive a list of nodes along with the type of mutation
   * that was performed on each: created, destroyed, or updated.
   *
   * One common use case for this is to attach DOM event listeners to the underlying DOM nodes as Lexical nodes are created.
   * {@link LexicalEditor.getElementByKey} can be used for this.
   *
   * If any existing nodes are in the DOM, and skipInitialization is not true, the listener
   * will be called immediately with an updateTag of 'registerMutationListener' where all
   * nodes have the 'created' NodeMutation. This can be controlled with the skipInitialization option
   * (whose default was previously true for backwards compatibility with &lt;=0.16.1 but has been changed to false as of 0.21.0).
   *
   * @param klass - The class of the node that you want to listen to mutations on.
   * @param listener - The logic you want to run when the node is mutated.
   * @param options - see {@link MutationListenerOptions}
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerMutationListener(klass, listener, options) {
    const klassToMutate = this.resolveRegisteredNodeAfterReplacements(this.getRegisteredNode(klass)).klass;
    const mutations = this._listeners.mutation;
    let klassSet = mutations.get(listener);
    if (klassSet === void 0) {
      klassSet = /* @__PURE__ */ new Set();
      mutations.set(listener, klassSet);
    }
    klassSet.add(klassToMutate);
    const skipInitialization = options && options.skipInitialization;
    if (!(skipInitialization === void 0 ? DEFAULT_SKIP_INITIALIZATION : skipInitialization)) {
      this.initializeMutationListener(listener, klassToMutate);
    }
    return () => {
      klassSet.delete(klassToMutate);
      if (klassSet.size === 0) {
        mutations.delete(listener);
      }
    };
  }
  /** @internal */
  getRegisteredNode(klass) {
    const registeredNode = this._nodes.get(klass.getType());
    if (registeredNode === void 0) {
      {
        formatDevErrorMessage(`Node ${klass.name} has not been registered. Ensure node has been passed to createEditor.`);
      }
    }
    return registeredNode;
  }
  /** @internal */
  resolveRegisteredNodeAfterReplacements(registeredNode) {
    while (registeredNode.replaceWithKlass) {
      registeredNode = this.getRegisteredNode(registeredNode.replaceWithKlass);
    }
    return registeredNode;
  }
  /** @internal */
  initializeMutationListener(listener, klass) {
    const prevEditorState = this._editorState;
    const nodeMap = getCachedTypeToNodeMap(prevEditorState).get(klass.getType());
    if (!nodeMap) {
      return;
    }
    const nodeMutationMap = /* @__PURE__ */ new Map();
    for (const k of nodeMap.keys()) {
      nodeMutationMap.set(k, "created");
    }
    if (nodeMutationMap.size > 0) {
      listener(nodeMutationMap, {
        dirtyLeaves: /* @__PURE__ */ new Set(),
        prevEditorState,
        updateTags: /* @__PURE__ */ new Set(["registerMutationListener"])
      });
    }
  }
  /** @internal */
  registerNodeTransformToKlass(klass, listener) {
    const registeredNode = this.getRegisteredNode(klass);
    registeredNode.transforms.add(listener);
    return registeredNode;
  }
  /**
   * Registers a listener that will run when a Lexical node of the provided class is
   * marked dirty during an update. The listener will continue to run as long as the node
   * is marked dirty. There are no guarantees around the order of transform execution!
   *
   * Watch out for infinite loops. See [Node Transforms](https://lexical.dev/docs/concepts/transforms)
   * @param klass - The class of the node that you want to run transforms on.
   * @param listener - The logic you want to run when the node is updated.
   * @returns a teardown function that can be used to cleanup the listener.
   */
  registerNodeTransform(klass, listener) {
    const registeredNode = this.registerNodeTransformToKlass(klass, listener);
    const registeredNodes = [registeredNode];
    const replaceWithKlass = registeredNode.replaceWithKlass;
    if (replaceWithKlass != null) {
      const registeredReplaceWithNode = this.registerNodeTransformToKlass(replaceWithKlass, listener);
      registeredNodes.push(registeredReplaceWithNode);
    }
    markNodesWithTypesAsDirty(this, registeredNodes.map((node) => node.klass.getType()));
    return () => {
      registeredNodes.forEach((node) => node.transforms.delete(listener));
    };
  }
  /**
   * Used to assert that a certain node is registered, usually by plugins to ensure nodes that they
   * depend on have been registered.
   * @returns True if the editor has registered the provided node type, false otherwise.
   */
  hasNode(node) {
    return this._nodes.has(node.getType());
  }
  /**
   * Used to assert that certain nodes are registered, usually by plugins to ensure nodes that they
   * depend on have been registered.
   * @returns True if the editor has registered all of the provided node types, false otherwise.
   */
  hasNodes(nodes) {
    return nodes.every(this.hasNode.bind(this));
  }
  /**
   * Dispatches a command of the specified type with the specified payload.
   * This triggers all command listeners (set by {@link LexicalEditor.registerCommand})
   * for this type, passing them the provided payload. The command listeners
   * will be triggered in an implicit {@link LexicalEditor.update}, unless
   * this was invoked from inside an update in which case that update context
   * will be re-used (as if this was a dollar function itself).
   * @param type - the type of command listeners to trigger.
   * @param payload - the data to pass as an argument to the command listeners.
   */
  dispatchCommand(type, payload) {
    return dispatchCommand(this, type, payload);
  }
  /**
   * Gets a map of all decorators in the editor.
   * @returns A mapping of call decorator keys to their decorated content
   */
  getDecorators() {
    return this._decorators;
  }
  /**
   *
   * @returns the current root element of the editor. If you want to register
   * an event listener, do it via {@link LexicalEditor.registerRootListener}, since
   * this reference may not be stable.
   */
  getRootElement() {
    return this._rootElement;
  }
  /**
   * Gets the key of the editor
   * @returns The editor key
   */
  getKey() {
    return this._key;
  }
  /**
   * Imperatively set the root contenteditable element that Lexical listens
   * for events on.
   */
  setRootElement(nextRootElement) {
    const prevRootElement = this._rootElement;
    if (nextRootElement !== prevRootElement) {
      const classNames = getCachedClassNameArray(this._config.theme, "root");
      const pendingEditorState = this._pendingEditorState || this._editorState;
      this._rootElement = nextRootElement;
      resetEditor(this, prevRootElement, nextRootElement, pendingEditorState);
      if (prevRootElement !== null) {
        if (!this._config.disableEvents) {
          removeRootElementEvents(prevRootElement);
        }
        if (classNames != null) {
          prevRootElement.classList.remove(...classNames);
        }
      }
      if (nextRootElement !== null) {
        const windowObj = getDefaultView(nextRootElement);
        const style = nextRootElement.style;
        style.userSelect = "text";
        style.whiteSpace = "pre-wrap";
        style.wordBreak = "break-word";
        nextRootElement.setAttribute("data-lexical-editor", "true");
        this._window = windowObj;
        this._dirtyType = FULL_RECONCILE;
        initMutationObserver(this);
        this._updateTags.add(HISTORY_MERGE_TAG);
        $commitPendingUpdates(this);
        if (!this._config.disableEvents) {
          addRootElementEvents(nextRootElement, this);
        }
        if (classNames != null) {
          nextRootElement.classList.add(...classNames);
        }
        {
          const nextRootElementParent = nextRootElement.parentElement;
          if (nextRootElementParent != null && ["flex", "inline-flex"].includes(getComputedStyle(nextRootElementParent).display)) {
            console.warn(`When using "display: flex" or "display: inline-flex" on an element containing content editable, Chrome may have unwanted focusing behavior when clicking outside of it. Consider wrapping the content editable within a non-flex element.`);
          }
        }
      } else {
        this._window = null;
        this._updateTags.add(HISTORY_MERGE_TAG);
        $commitPendingUpdates(this);
      }
      triggerListeners("root", this, false, nextRootElement, prevRootElement);
    }
  }
  /**
   * Gets the underlying HTMLElement associated with the LexicalNode for the given key.
   * @returns the HTMLElement rendered by the LexicalNode associated with the key.
   * @param key - the key of the LexicalNode.
   */
  getElementByKey(key) {
    return this._keyToDOMMap.get(key) || null;
  }
  /**
   * Gets the active editor state.
   * @returns The editor state
   */
  getEditorState() {
    return this._editorState;
  }
  /**
   * Imperatively set the EditorState. Triggers reconciliation like an update.
   * @param editorState - the state to set the editor
   * @param options - options for the update.
   */
  setEditorState(editorState, options) {
    if (editorState.isEmpty()) {
      {
        formatDevErrorMessage(`setEditorState: the editor state is empty. Ensure the editor state's root node never becomes empty.`);
      }
    }
    let writableEditorState = editorState;
    if (writableEditorState._readOnly) {
      writableEditorState = cloneEditorState(editorState);
      writableEditorState._selection = editorState._selection ? editorState._selection.clone() : null;
    }
    flushRootMutations(this);
    const pendingEditorState = this._pendingEditorState;
    const tags = this._updateTags;
    const tag = options !== void 0 ? options.tag : null;
    if (pendingEditorState !== null && !pendingEditorState.isEmpty()) {
      if (tag != null) {
        tags.add(tag);
      }
      $commitPendingUpdates(this);
    }
    this._pendingEditorState = writableEditorState;
    this._dirtyType = FULL_RECONCILE;
    this._dirtyElements.set("root", false);
    this._compositionKey = null;
    if (tag != null) {
      tags.add(tag);
    }
    if (!this._updating) {
      $commitPendingUpdates(this);
    }
  }
  /**
   * Parses a SerializedEditorState (usually produced by {@link EditorState.toJSON}) and returns
   * and EditorState object that can be, for example, passed to {@link LexicalEditor.setEditorState}. Typically,
   * deserialization from JSON stored in a database uses this method.
   * @param maybeStringifiedEditorState
   * @param updateFn
   * @returns
   */
  parseEditorState(maybeStringifiedEditorState, updateFn) {
    const serializedEditorState = typeof maybeStringifiedEditorState === "string" ? JSON.parse(maybeStringifiedEditorState) : maybeStringifiedEditorState;
    return parseEditorState(serializedEditorState, this, updateFn);
  }
  /**
   * Executes a read of the editor's state, with the
   * editor context available (useful for exporting and read-only DOM
   * operations). Much like update, but prevents any mutation of the
   * editor's state. Any pending updates will be flushed immediately before
   * the read.
   * @param callbackFn - A function that has access to read-only editor state.
   */
  read(callbackFn) {
    $commitPendingUpdates(this);
    return this.getEditorState().read(callbackFn, {
      editor: this
    });
  }
  /**
   * Executes an update to the editor state. The updateFn callback is the ONLY place
   * where Lexical editor state can be safely mutated.
   * @param updateFn - A function that has access to writable editor state.
   * @param options - A bag of options to control the behavior of the update.
   */
  update(updateFn, options) {
    updateEditor(this, updateFn, options);
  }
  /**
   * Focuses the editor by marking the existing selection as dirty, or by
   * creating a new selection at `defaultSelection` if one does not already
   * exist. If you want to force a specific selection, you should call
   * `root.selectStart()` or `root.selectEnd()` in an update.
   *
   * @param callbackFn - A function to run after the editor is focused.
   * @param options - A bag of options
   */
  focus(callbackFn, options = {}) {
    const rootElement = this._rootElement;
    if (rootElement !== null) {
      rootElement.setAttribute("autocapitalize", "off");
      updateEditorSync(this, () => {
        const selection = $getSelection();
        const root = $getRoot();
        if (selection !== null) {
          if (!selection.dirty) {
            $setSelection(selection.clone());
          }
        } else if (root.getChildrenSize() !== 0) {
          if (options.defaultSelection === "rootStart") {
            root.selectStart();
          } else {
            root.selectEnd();
          }
        }
        $addUpdateTag(FOCUS_TAG);
        $onUpdate(() => {
          rootElement.removeAttribute("autocapitalize");
          if (callbackFn) {
            callbackFn();
          }
        });
      });
      if (this._pendingEditorState === null) {
        rootElement.removeAttribute("autocapitalize");
      }
    }
  }
  /**
   * Removes focus from the editor.
   */
  blur() {
    const rootElement = this._rootElement;
    if (rootElement !== null) {
      rootElement.blur();
    }
    const domSelection = getDOMSelection(this._window);
    if (domSelection !== null) {
      domSelection.removeAllRanges();
    }
  }
  /**
   * Returns true if the editor is editable, false otherwise.
   * @returns True if the editor is editable, false otherwise.
   */
  isEditable() {
    return this._editable;
  }
  /**
   * Sets the editable property of the editor. When false, the
   * editor will not listen for user events on the underling contenteditable.
   * @param editable - the value to set the editable mode to.
   */
  setEditable(editable) {
    if (this._editable !== editable) {
      this._editable = editable;
      triggerListeners("editable", this, true, editable);
    }
  }
  /**
   * Returns a JSON-serializable javascript object NOT a JSON string.
   * You still must call JSON.stringify (or something else) to turn the
   * state into a string you can transfer over the wire and store in a database.
   *
   * See {@link LexicalNode.exportJSON}
   *
   * @returns A JSON-serializable javascript object
   */
  toJSON() {
    return {
      editorState: this._editorState.toJSON()
    };
  }
};
LexicalEditor.version = "0.44.0+dev.esm";
var pendingNodeToClone = null;
function setPendingNodeToClone(pendingNode) {
  pendingNodeToClone = pendingNode;
}
function getPendingNodeToClone() {
  const node = pendingNodeToClone;
  pendingNodeToClone = null;
  return node;
}
var keyCounter = 1;
function resetRandomKey() {
  keyCounter = 1;
}
function generateRandomKey() {
  return "" + keyCounter++;
}
function getRegisteredNodeOrThrow(editor, nodeType) {
  const registeredNode = getRegisteredNode(editor, nodeType);
  if (registeredNode === void 0) {
    {
      formatDevErrorMessage(`registeredNode: Type ${nodeType} not found`);
    }
  }
  return registeredNode;
}
function getRegisteredNode(editor, nodeType) {
  return editor._nodes.get(nodeType);
}
var scheduleMicroTask = typeof queueMicrotask === "function" ? queueMicrotask : (fn) => {
  Promise.resolve().then(fn);
};
function $isSelectionCapturedInDecorator(node) {
  return $isDecoratorNode($getNearestNodeFromDOMNode(node));
}
function isSelectionCapturedInDecoratorInput(anchorDOM) {
  const activeElement = document.activeElement;
  if (!isHTMLElement(activeElement)) {
    return false;
  }
  const nodeName = activeElement.nodeName;
  return $isDecoratorNode($getNearestNodeFromDOMNode(anchorDOM)) && (nodeName === "INPUT" || nodeName === "TEXTAREA" || activeElement.contentEditable === "true" && getEditorPropertyFromDOMNode(activeElement) == null);
}
function isSelectionWithinEditor(editor, anchorDOM, focusDOM) {
  const rootElement = editor.getRootElement();
  try {
    return rootElement !== null && rootElement.contains(anchorDOM) && rootElement.contains(focusDOM) && // Ignore if selection is within nested editor
    anchorDOM !== null && !isSelectionCapturedInDecoratorInput(anchorDOM) && getNearestEditorFromDOMNode(anchorDOM) === editor;
  } catch (_error) {
    return false;
  }
}
function isLexicalEditor(editor) {
  return editor instanceof LexicalEditor;
}
function getNearestEditorFromDOMNode(node) {
  let currentNode = node;
  while (currentNode != null) {
    const editor = getEditorPropertyFromDOMNode(currentNode);
    if (isLexicalEditor(editor)) {
      return editor;
    }
    currentNode = getParentElement(currentNode);
  }
  return null;
}
function getEditorPropertyFromDOMNode(node) {
  return node ? node.__lexicalEditor : null;
}
function getTextDirection(text) {
  if (RTL_REGEX.test(text)) {
    return "rtl";
  }
  if (LTR_REGEX.test(text)) {
    return "ltr";
  }
  return null;
}
function $isTokenOrTab(node) {
  return $isTabNode(node) || node.isToken();
}
function $isTokenOrSegmented(node) {
  return $isTokenOrTab(node) || node.isSegmented();
}
function isDOMTextNode(node) {
  return isDOMNode(node) && node.nodeType === DOM_TEXT_TYPE;
}
function isDOMDocumentNode(node) {
  return isDOMNode(node) && node.nodeType === DOM_DOCUMENT_TYPE;
}
function getDOMTextNode(element) {
  let node = element;
  while (node != null) {
    if (isDOMTextNode(node)) {
      return node;
    }
    node = node.firstChild;
  }
  return null;
}
function toggleTextFormatType(format, type, alignWithFormat) {
  const activeFormat = TEXT_TYPE_TO_FORMAT[type];
  if (alignWithFormat !== null && (format & activeFormat) === (alignWithFormat & activeFormat)) {
    return format;
  }
  let newFormat = format ^ activeFormat;
  if (type === "subscript") {
    newFormat &= ~TEXT_TYPE_TO_FORMAT.superscript;
  } else if (type === "superscript") {
    newFormat &= ~TEXT_TYPE_TO_FORMAT.subscript;
  } else if (type === "lowercase") {
    newFormat &= ~TEXT_TYPE_TO_FORMAT.uppercase;
    newFormat &= ~TEXT_TYPE_TO_FORMAT.capitalize;
  } else if (type === "uppercase") {
    newFormat &= ~TEXT_TYPE_TO_FORMAT.lowercase;
    newFormat &= ~TEXT_TYPE_TO_FORMAT.capitalize;
  } else if (type === "capitalize") {
    newFormat &= ~TEXT_TYPE_TO_FORMAT.lowercase;
    newFormat &= ~TEXT_TYPE_TO_FORMAT.uppercase;
  }
  return newFormat;
}
function $isLeafNode(node) {
  return $isTextNode(node) || $isLineBreakNode(node) || $isDecoratorNode(node);
}
function $setNodeKey(node, existingKey) {
  const pendingNode = getPendingNodeToClone();
  existingKey = existingKey || pendingNode && pendingNode.__key;
  if (existingKey != null) {
    {
      errorOnNodeKeyConstructorMismatch(node, existingKey, pendingNode);
    }
    node.__key = existingKey;
    return;
  }
  errorOnReadOnly();
  errorOnInfiniteTransforms();
  const editor = getActiveEditor();
  const editorState = getActiveEditorState();
  const key = generateRandomKey();
  editorState._nodeMap.set(key, node);
  if ($isElementNode(node)) {
    editor._dirtyElements.set(key, true);
  } else {
    editor._dirtyLeaves.add(key);
  }
  editor._cloneNotNeeded.add(key);
  editor._dirtyType = HAS_DIRTY_NODES;
  node.__key = key;
}
function errorOnNodeKeyConstructorMismatch(node, existingKey, pendingNode) {
  const editorState = internalGetActiveEditorState();
  if (!editorState) {
    return;
  }
  const existingNode = editorState._nodeMap.get(existingKey);
  if (pendingNode) {
    if (!(existingKey === pendingNode.__key)) {
      formatDevErrorMessage(`Lexical node with constructor ${node.constructor.name} (type ${node.getType()}) has an incorrect clone implementation, got ${String(existingKey)} for nodeKey when expecting ${pendingNode.__key}`);
    }
  }
  if (existingNode && existingNode.constructor !== node.constructor) {
    if (node.constructor.name !== existingNode.constructor.name) {
      {
        formatDevErrorMessage(`Lexical node with constructor ${node.constructor.name} attempted to re-use key from node in active editor state with constructor ${existingNode.constructor.name}. Keys must not be re-used when the type is changed.`);
      }
    } else {
      {
        formatDevErrorMessage(`Lexical node with constructor ${node.constructor.name} attempted to re-use key from node in active editor state with different constructor with the same name (possibly due to invalid Hot Module Replacement). Keys must not be re-used when the type is changed.`);
      }
    }
  }
}
function internalMarkParentElementsAsDirty(parentKey, nodeMap, dirtyElements) {
  let nextParentKey = parentKey;
  while (nextParentKey !== null) {
    if (dirtyElements.has(nextParentKey)) {
      return;
    }
    const node = nodeMap.get(nextParentKey);
    if (node === void 0) {
      break;
    }
    dirtyElements.set(nextParentKey, false);
    nextParentKey = node.__parent;
  }
}
function removeFromParent(node) {
  const oldParent = node.getParent();
  if (oldParent !== null) {
    const writableNode = node.getWritable();
    const writableParent = oldParent.getWritable();
    const prevSibling = node.getPreviousSibling();
    const nextSibling = node.getNextSibling();
    const nextSiblingKey = nextSibling !== null ? nextSibling.__key : null;
    const prevSiblingKey = prevSibling !== null ? prevSibling.__key : null;
    const writablePrevSibling = prevSibling !== null ? prevSibling.getWritable() : null;
    const writableNextSibling = nextSibling !== null ? nextSibling.getWritable() : null;
    if (prevSibling === null) {
      writableParent.__first = nextSiblingKey;
    }
    if (nextSibling === null) {
      writableParent.__last = prevSiblingKey;
    }
    if (writablePrevSibling !== null) {
      writablePrevSibling.__next = nextSiblingKey;
    }
    if (writableNextSibling !== null) {
      writableNextSibling.__prev = prevSiblingKey;
    }
    writableNode.__prev = null;
    writableNode.__next = null;
    writableNode.__parent = null;
    writableParent.__size--;
  }
}
function internalMarkNodeAsDirty(node) {
  errorOnInfiniteTransforms();
  if (!!$isEphemeral(node)) {
    formatDevErrorMessage(`internalMarkNodeAsDirty: Ephemeral nodes must not be marked as dirty (key ${node.__key} type ${node.__type})`);
  }
  const latest = node.getLatest();
  const parent = latest.__parent;
  const editorState = getActiveEditorState();
  const editor = getActiveEditor();
  const nodeMap = editorState._nodeMap;
  const dirtyElements = editor._dirtyElements;
  if (parent !== null) {
    internalMarkParentElementsAsDirty(parent, nodeMap, dirtyElements);
  }
  const key = latest.__key;
  editor._dirtyType = HAS_DIRTY_NODES;
  if ($isElementNode(node)) {
    dirtyElements.set(key, true);
  } else {
    editor._dirtyLeaves.add(key);
  }
}
function internalMarkSiblingsAsDirty(node) {
  const previousNode = node.getPreviousSibling();
  const nextNode = node.getNextSibling();
  if (previousNode !== null) {
    internalMarkNodeAsDirty(previousNode);
  }
  if (nextNode !== null) {
    internalMarkNodeAsDirty(nextNode);
  }
}
function $setCompositionKey(compositionKey) {
  errorOnReadOnly();
  const editor = getActiveEditor();
  const previousCompositionKey = editor._compositionKey;
  if (compositionKey !== previousCompositionKey) {
    editor._compositionKey = compositionKey;
    if (previousCompositionKey !== null) {
      const node = $getNodeByKey(previousCompositionKey);
      if (node !== null) {
        node.getWritable();
      }
    }
    if (compositionKey !== null) {
      const node = $getNodeByKey(compositionKey);
      if (node !== null) {
        node.getWritable();
      }
    }
  }
}
function $getCompositionKey() {
  if (isCurrentlyReadOnlyMode()) {
    return null;
  }
  const editor = getActiveEditor();
  return editor._compositionKey;
}
function $getNodeByKey(key, _editorState) {
  const editorState = _editorState || getActiveEditorState();
  const node = editorState._nodeMap.get(key);
  if (node === void 0) {
    return null;
  }
  return node;
}
function $getNodeFromDOMNode(dom, editorState) {
  const editor = getActiveEditor();
  const key = getNodeKeyFromDOMNode(dom, editor);
  if (key !== void 0) {
    return $getNodeByKey(key, editorState);
  }
  return null;
}
function setNodeKeyOnDOMNode(dom, editor, key) {
  const prop = `__lexicalKey_${editor._key}`;
  dom[prop] = key;
}
function getNodeKeyFromDOMNode(dom, editor) {
  const prop = `__lexicalKey_${editor._key}`;
  return dom[prop];
}
function $getNearestNodeFromDOMNode(startingDOM, editorState) {
  let dom = startingDOM;
  while (dom != null) {
    const node = $getNodeFromDOMNode(dom, editorState);
    if (node !== null) {
      return node;
    }
    dom = getParentElement(dom);
  }
  return null;
}
function cloneDecorators(editor) {
  const currentDecorators = editor._decorators;
  const pendingDecorators = Object.assign({}, currentDecorators);
  editor._pendingDecorators = pendingDecorators;
  return pendingDecorators;
}
function getEditorStateTextContent(editorState) {
  return editorState.read(() => $getRoot().getTextContent());
}
function markNodesWithTypesAsDirty(editor, types) {
  const cachedMap = getCachedTypeToNodeMap(editor.getEditorState());
  const dirtyNodeMaps = [];
  for (const type of types) {
    const nodeMap = cachedMap.get(type);
    if (nodeMap) {
      dirtyNodeMaps.push(nodeMap);
    }
  }
  if (dirtyNodeMaps.length === 0) {
    return;
  }
  editor.update(() => {
    for (const nodeMap of dirtyNodeMaps) {
      for (const nodeKey of nodeMap.keys()) {
        const latest = $getNodeByKey(nodeKey);
        if (latest) {
          latest.markDirty();
        }
      }
    }
  }, editor._pendingEditorState === null ? {
    tag: HISTORY_MERGE_TAG
  } : void 0);
}
function $getRoot() {
  return internalGetRoot(getActiveEditorState());
}
function internalGetRoot(editorState) {
  return editorState._nodeMap.get("root");
}
function $setSelection(selection) {
  errorOnReadOnly();
  const editorState = getActiveEditorState();
  if (selection !== null) {
    {
      if (Object.isFrozen(selection)) {
        {
          formatDevErrorMessage(`$setSelection called on frozen selection object. Ensure selection is cloned before passing in.`);
        }
      }
    }
    selection.dirty = true;
    selection.setCachedNodes(null);
  }
  editorState._selection = selection;
}
function $flushMutations() {
  errorOnReadOnly();
  const editor = getActiveEditor();
  flushRootMutations(editor);
}
function $getNodeFromDOM(dom) {
  const editor = getActiveEditor();
  const nodeKey = getNodeKeyFromDOMTree(dom, editor);
  if (nodeKey === null) {
    const rootElement = editor.getRootElement();
    if (dom === rootElement) {
      return $getNodeByKey("root");
    }
    return null;
  }
  return $getNodeByKey(nodeKey);
}
function getNodeKeyFromDOMTree(dom, editor) {
  let node = dom;
  while (node != null) {
    const key = getNodeKeyFromDOMNode(node, editor);
    if (key !== void 0) {
      return key;
    }
    node = getParentElement(node);
  }
  return null;
}
function doesContainSurrogatePair(str) {
  return /[\uD800-\uDBFF][\uDC00-\uDFFF]/g.test(str);
}
function getEditorsToPropagate(editor) {
  const editorsToPropagate = [];
  for (let currentEditor = editor; currentEditor !== null; currentEditor = currentEditor._parentEditor) {
    editorsToPropagate.push(currentEditor);
  }
  return editorsToPropagate;
}
function createUID() {
  return Math.random().toString(36).replace(/[^a-z]+/g, "").substring(0, 5);
}
function getAnchorTextFromDOM(anchorNode) {
  return isDOMTextNode(anchorNode) ? anchorNode.nodeValue : null;
}
function $updateSelectedTextFromDOM(isCompositionEnd, editor, data) {
  const domSelection = getDOMSelection(getWindow(editor));
  if (domSelection === null) {
    return;
  }
  const anchorNode = domSelection.anchorNode;
  let {
    anchorOffset,
    focusOffset
  } = domSelection;
  if (anchorNode !== null) {
    let textContent = getAnchorTextFromDOM(anchorNode);
    const node = $getNearestNodeFromDOMNode(anchorNode);
    if (textContent !== null && $isTextNode(node)) {
      if ((textContent === COMPOSITION_SUFFIX || textContent === COMPOSITION_START_CHAR) && data) {
        const offset = data.length;
        textContent = data;
        anchorOffset = offset;
        focusOffset = offset;
      }
      if (textContent !== null) {
        $updateTextNodeFromDOMContent(node, textContent, anchorOffset, focusOffset, isCompositionEnd);
      }
    }
  }
}
function $updateTextNodeFromDOMContent(textNode, textContent, anchorOffset, focusOffset, compositionEnd) {
  let node = textNode;
  if (node.isAttached() && (compositionEnd || !node.isDirty())) {
    const isComposing = node.isComposing();
    let normalizedTextContent = textContent;
    if (isComposing || compositionEnd) {
      if (textContent.endsWith(COMPOSITION_SUFFIX)) {
        normalizedTextContent = textContent.slice(0, -COMPOSITION_SUFFIX.length);
      }
      if (compositionEnd) {
        const char = COMPOSITION_START_CHAR;
        let index;
        while ((index = normalizedTextContent.indexOf(char)) !== -1) {
          normalizedTextContent = normalizedTextContent.slice(0, index) + normalizedTextContent.slice(index + char.length);
          if (anchorOffset !== null && anchorOffset > index) {
            anchorOffset = Math.max(index, anchorOffset - char.length);
          }
          if (focusOffset !== null && focusOffset > index) {
            focusOffset = Math.max(index, focusOffset - char.length);
          }
        }
      }
    }
    const prevTextContent = node.getTextContent();
    if (compositionEnd || normalizedTextContent !== prevTextContent) {
      if (normalizedTextContent === "") {
        $setCompositionKey(null);
        if (!IS_SAFARI && !IS_IOS && !IS_APPLE_WEBKIT) {
          const editor = getActiveEditor();
          setTimeout(() => {
            editor.update(() => {
              if (node.isAttached()) {
                node.remove();
              }
            });
          }, 20);
        } else {
          node.remove();
        }
        return;
      }
      const parent = node.getParent();
      const prevSelection = $getPreviousSelection();
      const prevTextContentSize = node.getTextContentSize();
      const compositionKey = $getCompositionKey();
      const nodeKey = node.getKey();
      if (node.isToken() || compositionKey !== null && nodeKey === compositionKey && !isComposing || // Check if character was added at the start or boundaries when not insertable, and we need
      // to clear this input from occurring as that action wasn't permitted.
      $isRangeSelection(prevSelection) && (parent !== null && !parent.canInsertTextBefore() && prevSelection.anchor.offset === 0 || prevSelection.anchor.key === textNode.__key && prevSelection.anchor.offset === 0 && !node.canInsertTextBefore() && !isComposing || prevSelection.focus.key === textNode.__key && prevSelection.focus.offset === prevTextContentSize && !node.canInsertTextAfter() && !isComposing)) {
        node.markDirty();
        return;
      }
      const selection = $getSelection();
      if (!$isRangeSelection(selection) || anchorOffset === null || focusOffset === null) {
        $setTextContentWithSelection(node, normalizedTextContent, selection);
        return;
      }
      selection.setTextNodeRange(node, anchorOffset, node, focusOffset);
      if (node.isSegmented()) {
        const originalTextContent = node.getTextContent();
        const replacement = $createTextNode(originalTextContent);
        node.replace(replacement);
        node = replacement;
      }
      $setTextContentWithSelection(node, normalizedTextContent, selection);
    }
  }
}
function $setTextContentWithSelection(node, textContent, selection) {
  node.setTextContent(textContent);
  if ($isRangeSelection(selection)) {
    const key = node.getKey();
    for (const k of ["anchor", "focus"]) {
      const pt = selection[k];
      if (pt.type === "text" && pt.key === key) {
        pt.offset = $getTextNodeOffset(node, pt.offset, "clamp");
      }
    }
  }
}
function $previousSiblingDoesNotAcceptText(node) {
  const previousSibling = node.getPreviousSibling();
  return ($isTextNode(previousSibling) || $isElementNode(previousSibling) && previousSibling.isInline()) && !previousSibling.canInsertTextAfter();
}
function $shouldInsertTextAfterOrBeforeTextNode(selection, node) {
  if (node.isSegmented()) {
    return true;
  }
  if (!selection.isCollapsed()) {
    return false;
  }
  const offset = selection.anchor.offset;
  const parent = node.getParentOrThrow();
  const isToken = $isTokenOrTab(node);
  if (offset === 0) {
    return !node.canInsertTextBefore() || !parent.canInsertTextBefore() && !node.isComposing() || isToken || $previousSiblingDoesNotAcceptText(node);
  } else if (offset === node.getTextContentSize()) {
    return !node.canInsertTextAfter() || !parent.canInsertTextAfter() && !node.isComposing() || isToken;
  } else {
    return false;
  }
}
function matchModifier(event, mask, prop) {
  const expected = mask[prop] || false;
  return expected === "any" || expected === event[prop];
}
function isModifierMatch(event, mask) {
  return matchModifier(event, mask, "altKey") && matchModifier(event, mask, "ctrlKey") && matchModifier(event, mask, "shiftKey") && matchModifier(event, mask, "metaKey");
}
function isExactShortcutMatch(event, expectedKey, mask) {
  if (!isModifierMatch(event, mask)) {
    return false;
  }
  if (event.key.toLowerCase() === expectedKey.toLowerCase()) {
    return true;
  }
  if (expectedKey.length > 1) {
    return false;
  }
  if (event.key.length === 1 && event.key.charCodeAt(0) <= 127) {
    return false;
  }
  if (event.code.startsWith("Digit") && /^\d$/.test(expectedKey)) {
    return event.code === `Digit${expectedKey}`;
  }
  const expectedCode = "Key" + expectedKey.toUpperCase();
  return event.code === expectedCode;
}
var CONTROL_OR_META = {
  ctrlKey: !IS_APPLE,
  metaKey: IS_APPLE
};
var CONTROL_OR_ALT = {
  altKey: IS_APPLE,
  ctrlKey: !IS_APPLE
};
function isTab(event) {
  return isExactShortcutMatch(event, "Tab", {
    shiftKey: "any"
  });
}
function isBold(event) {
  return isExactShortcutMatch(event, "b", CONTROL_OR_META);
}
function isItalic(event) {
  return isExactShortcutMatch(event, "i", CONTROL_OR_META);
}
function isUnderline(event) {
  return isExactShortcutMatch(event, "u", CONTROL_OR_META);
}
function isParagraph(event) {
  return isExactShortcutMatch(event, "Enter", {
    altKey: "any",
    ctrlKey: "any",
    metaKey: "any"
  });
}
function isLineBreak(event) {
  return isExactShortcutMatch(event, "Enter", {
    altKey: "any",
    ctrlKey: "any",
    metaKey: "any",
    shiftKey: true
  });
}
function isOpenLineBreak(event) {
  return IS_APPLE && isExactShortcutMatch(event, "o", {
    ctrlKey: true
  });
}
function isDeleteWordBackward(event) {
  return isExactShortcutMatch(event, "Backspace", CONTROL_OR_ALT);
}
function isDeleteWordForward(event) {
  return isExactShortcutMatch(event, "Delete", CONTROL_OR_ALT);
}
function isDeleteLineBackward(event) {
  return IS_APPLE && isExactShortcutMatch(event, "Backspace", {
    metaKey: true
  });
}
function isDeleteLineForward(event) {
  return IS_APPLE && (isExactShortcutMatch(event, "Delete", {
    metaKey: true
  }) || isExactShortcutMatch(event, "k", {
    ctrlKey: true
  }));
}
function isDeleteBackward(event) {
  return isExactShortcutMatch(event, "Backspace", {
    shiftKey: "any"
  }) || IS_APPLE && isExactShortcutMatch(event, "h", {
    ctrlKey: true
  });
}
function isDeleteForward(event) {
  return isExactShortcutMatch(event, "Delete", {}) || IS_APPLE && isExactShortcutMatch(event, "d", {
    ctrlKey: true
  });
}
function isUndo(event) {
  return isExactShortcutMatch(event, "z", CONTROL_OR_META);
}
function isRedo(event) {
  if (IS_APPLE) {
    return isExactShortcutMatch(event, "z", {
      metaKey: true,
      shiftKey: true
    });
  }
  return isExactShortcutMatch(event, "y", {
    ctrlKey: true
  }) || isExactShortcutMatch(event, "z", {
    ctrlKey: true,
    shiftKey: true
  });
}
function isCopy(event) {
  return isExactShortcutMatch(event, "c", CONTROL_OR_META);
}
function isCut(event) {
  return isExactShortcutMatch(event, "x", CONTROL_OR_META);
}
function isMoveBackward(event) {
  return isExactShortcutMatch(event, "ArrowLeft", {
    shiftKey: "any"
  });
}
function isMoveToStart(event) {
  return isExactShortcutMatch(event, "ArrowLeft", CONTROL_OR_META);
}
function isMoveForward(event) {
  return isExactShortcutMatch(event, "ArrowRight", {
    shiftKey: "any"
  });
}
function isMoveToEnd(event) {
  return isExactShortcutMatch(event, "ArrowRight", CONTROL_OR_META);
}
function isMoveUp(event) {
  return isExactShortcutMatch(event, "ArrowUp", {
    altKey: "any",
    shiftKey: "any"
  });
}
function isMoveDown(event) {
  return isExactShortcutMatch(event, "ArrowDown", {
    altKey: "any",
    shiftKey: "any"
  });
}
function isModifier(event) {
  return event.ctrlKey || event.shiftKey || event.altKey || event.metaKey;
}
function isSpace(event) {
  return event.key === " ";
}
function isBackspace(event) {
  return event.key === "Backspace";
}
function isEscape(event) {
  return event.key === "Escape";
}
function isDelete(event) {
  return event.key === "Delete";
}
function isSelectAll(event) {
  return isExactShortcutMatch(event, "a", CONTROL_OR_META);
}
function $selectAll(selection) {
  const root = $getRoot();
  if ($isRangeSelection(selection)) {
    const anchor = selection.anchor;
    const focus = selection.focus;
    const anchorNode = anchor.getNode();
    const topParent = anchorNode.getTopLevelElementOrThrow();
    const rootNode = topParent.getParentOrThrow();
    anchor.set(rootNode.getKey(), 0, "element");
    focus.set(rootNode.getKey(), rootNode.getChildrenSize(), "element");
    $normalizeSelection(selection);
    return selection;
  } else {
    const newSelection = root.select(0, root.getChildrenSize());
    $setSelection($normalizeSelection(newSelection));
    return newSelection;
  }
}
function getCachedClassNameArray(classNamesTheme, classNameThemeType) {
  if (classNamesTheme.__lexicalClassNameCache === void 0) {
    classNamesTheme.__lexicalClassNameCache = {};
  }
  const classNamesCache = classNamesTheme.__lexicalClassNameCache;
  const cachedClassNames = classNamesCache[classNameThemeType];
  if (cachedClassNames !== void 0) {
    return cachedClassNames;
  }
  const classNames = classNamesTheme[classNameThemeType];
  if (typeof classNames === "string") {
    const classNamesArr = normalizeClassNames(classNames);
    classNamesCache[classNameThemeType] = classNamesArr;
    return classNamesArr;
  }
  return classNames;
}
function setMutatedNode(mutatedNodes2, registeredNodes, mutationListeners, node, mutation) {
  if (mutationListeners.size === 0) {
    return;
  }
  const nodeType = node.__type;
  const nodeKey = node.__key;
  const registeredNode = registeredNodes.get(nodeType);
  if (registeredNode === void 0) {
    {
      formatDevErrorMessage(`Type ${nodeType} not in registeredNodes`);
    }
  }
  const klass = registeredNode.klass;
  let mutatedNodesByType = mutatedNodes2.get(klass);
  if (mutatedNodesByType === void 0) {
    mutatedNodesByType = /* @__PURE__ */ new Map();
    mutatedNodes2.set(klass, mutatedNodesByType);
  }
  const prevMutation = mutatedNodesByType.get(nodeKey);
  const isMove = prevMutation === "destroyed" && mutation === "created";
  if (prevMutation === void 0 || isMove) {
    mutatedNodesByType.set(nodeKey, isMove ? "updated" : mutation);
  }
}
function $nodesOfType(klass) {
  const klassType = klass.getType();
  const editorState = getActiveEditorState();
  if (editorState._readOnly) {
    const nodes2 = getCachedTypeToNodeMap(editorState).get(klassType);
    return nodes2 ? Array.from(nodes2.values()) : [];
  }
  const nodes = editorState._nodeMap;
  const nodesOfType = [];
  for (const [, node] of nodes) {
    if (node instanceof klass && node.__type === klassType && node.isAttached()) {
      nodesOfType.push(node);
    }
  }
  return nodesOfType;
}
function resolveElement(element, isBackward, focusOffset) {
  const parent = element.getParent();
  let offset = focusOffset;
  let block = element;
  if (parent !== null) {
    if (isBackward && focusOffset === 0) {
      offset = block.getIndexWithinParent();
      block = parent;
    } else if (!isBackward && focusOffset === block.getChildrenSize()) {
      offset = block.getIndexWithinParent() + 1;
      block = parent;
    }
  }
  return block.getChildAtIndex(isBackward ? offset - 1 : offset);
}
function $getAdjacentNode(focus, isBackward) {
  const focusOffset = focus.offset;
  if (focus.type === "element") {
    const block = focus.getNode();
    return resolveElement(block, isBackward, focusOffset);
  } else {
    const focusNode = focus.getNode();
    if (isBackward && focusOffset === 0 || !isBackward && focusOffset === focusNode.getTextContentSize()) {
      const possibleNode = isBackward ? focusNode.getPreviousSibling() : focusNode.getNextSibling();
      if (possibleNode === null) {
        return resolveElement(focusNode.getParentOrThrow(), isBackward, focusNode.getIndexWithinParent() + (isBackward ? 0 : 1));
      }
      return possibleNode;
    }
  }
  return null;
}
function isFirefoxClipboardEvents(editor) {
  const event = getWindow(editor).event;
  const inputType = event && event.inputType;
  return inputType === "insertFromPaste" || inputType === "insertFromPasteAsQuotation";
}
function dispatchCommand(editor, command, payload) {
  return triggerCommandListeners(editor, command, payload, editor);
}
function getElementByKeyOrThrow(editor, key) {
  const element = editor._keyToDOMMap.get(key);
  if (element === void 0) {
    {
      formatDevErrorMessage(`Reconciliation: could not find DOM element for node key ${key}`);
    }
  }
  return element;
}
function getParentElement(node) {
  const parentElement = node.assignedSlot || node.parentElement;
  return isDocumentFragment(parentElement) ? parentElement.host : parentElement;
}
function getDOMOwnerDocument(target) {
  return isDOMDocumentNode(target) ? target : isHTMLElement(target) ? target.ownerDocument : null;
}
function scrollIntoViewIfNeeded(editor, selectionRect, rootElement) {
  const doc = getDOMOwnerDocument(rootElement);
  const defaultView = getDefaultView(doc);
  if (doc === null || defaultView === null) {
    return;
  }
  let {
    top: currentTop,
    bottom: currentBottom
  } = selectionRect;
  let targetTop = 0;
  let targetBottom = 0;
  let element = rootElement;
  while (element !== null) {
    const isBodyElement = element === doc.body;
    if (isBodyElement) {
      targetTop = 0;
      targetBottom = getWindow(editor).innerHeight;
      const computedStyle = defaultView.getComputedStyle(doc.documentElement);
      const scrollPaddingTop = parseFloat(computedStyle.scrollPaddingTop);
      const scrollPaddingBottom = parseFloat(computedStyle.scrollPaddingBottom);
      if (isFinite(scrollPaddingTop)) {
        targetTop += scrollPaddingTop;
      }
      if (isFinite(scrollPaddingBottom)) {
        targetBottom -= scrollPaddingBottom;
      }
    } else {
      const targetRect = element.getBoundingClientRect();
      targetTop = targetRect.top;
      targetBottom = targetRect.bottom;
    }
    let diff = 0;
    if (currentTop < targetTop) {
      diff = -(targetTop - currentTop);
    } else if (currentBottom > targetBottom) {
      diff = currentBottom - targetBottom;
    }
    if (diff !== 0) {
      if (isBodyElement) {
        defaultView.scrollBy(0, diff);
      } else {
        const scrollTop = element.scrollTop;
        element.scrollTop += diff;
        const yOffset = element.scrollTop - scrollTop;
        currentTop -= yOffset;
        currentBottom -= yOffset;
      }
    }
    if (isBodyElement) {
      break;
    }
    element = getParentElement(element);
  }
}
function $hasUpdateTag(tag) {
  const editor = getActiveEditor();
  return editor._updateTags.has(tag);
}
function $addUpdateTag(tag) {
  errorOnReadOnly();
  const editor = getActiveEditor();
  editor._updateTags.add(tag);
}
function $onUpdate(updateFn) {
  errorOnReadOnly();
  const editor = getActiveEditor();
  editor._deferred.push(updateFn);
}
function $maybeMoveChildrenSelectionToParent(parentNode) {
  const selection = $getSelection();
  if (!$isRangeSelection(selection) || !$isElementNode(parentNode)) {
    return selection;
  }
  const {
    anchor,
    focus
  } = selection;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  if ($hasAncestor(anchorNode, parentNode)) {
    anchor.set(parentNode.__key, 0, "element");
  }
  if ($hasAncestor(focusNode, parentNode)) {
    focus.set(parentNode.__key, 0, "element");
  }
  return selection;
}
function $hasAncestor(child, targetNode) {
  let parent = child.getParent();
  while (parent !== null) {
    if (parent.is(targetNode)) {
      return true;
    }
    parent = parent.getParent();
  }
  return false;
}
function getDefaultView(domElem) {
  const ownerDoc = getDOMOwnerDocument(domElem);
  return ownerDoc ? ownerDoc.defaultView : null;
}
function getWindow(editor) {
  const windowObj = editor._window;
  if (windowObj === null) {
    {
      formatDevErrorMessage(`window object not found`);
    }
  }
  return windowObj;
}
function $isInlineElementOrDecoratorNode(node) {
  return $isElementNode(node) && node.isInline() || $isDecoratorNode(node) && node.isInline();
}
function $getNearestRootOrShadowRoot(node) {
  let parent = node.getParentOrThrow();
  while (parent !== null) {
    if ($isRootOrShadowRoot(parent)) {
      return parent;
    }
    parent = parent.getParentOrThrow();
  }
  return parent;
}
function $isRootOrShadowRoot(node) {
  return $isRootNode(node) || $isElementNode(node) && node.isShadowRoot();
}
function $copyNode(node, skipReset = false) {
  const copy = node.constructor.clone(node);
  $setNodeKey(copy, null);
  copy.afterCloneFrom(node);
  if (!skipReset) {
    copy.resetOnCopyNodeFrom(node);
  }
  return copy;
}
function $applyNodeReplacement(node) {
  const editor = getActiveEditor();
  const nodeType = node.getType();
  const registeredNode = getRegisteredNode(editor, nodeType);
  if (!(registeredNode !== void 0)) {
    formatDevErrorMessage(`$applyNodeReplacement node ${node.constructor.name} with type ${nodeType} must be registered to the editor. You can do this by passing the node class via the "nodes" array in the editor config.`);
  }
  const {
    replace,
    replaceWithKlass
  } = registeredNode;
  if (replace !== null) {
    const replacementNode = replace(node);
    const replacementNodeKlass = replacementNode.constructor;
    if (replaceWithKlass !== null) {
      if (!(replacementNode instanceof replaceWithKlass)) {
        formatDevErrorMessage(`$applyNodeReplacement failed. Expected replacement node to be an instance of ${replaceWithKlass.name} with type ${replaceWithKlass.getType()} but returned ${replacementNodeKlass.name} with type ${replacementNodeKlass.getType()} from original node ${node.constructor.name} with type ${nodeType}`);
      }
    } else {
      if (!(replacementNode instanceof node.constructor && replacementNodeKlass !== node.constructor)) {
        formatDevErrorMessage(`$applyNodeReplacement failed. Ensure replacement node ${replacementNodeKlass.name} with type ${replacementNodeKlass.getType()} is a subclass of the original node ${node.constructor.name} with type ${nodeType}.`);
      }
    }
    if (!(replacementNode.__key !== node.__key)) {
      formatDevErrorMessage(`$applyNodeReplacement failed. Ensure that the key argument is *not* used in your replace function (from node ${node.constructor.name} with type ${nodeType} to node ${replacementNodeKlass.name} with type ${replacementNodeKlass.getType()}), Node keys must never be re-used except by the static clone method.`);
    }
    return replacementNode;
  }
  return node;
}
function errorOnInsertTextNodeOnRoot(node, insertNode) {
  const parentNode = node.getParent();
  if ($isRootNode(parentNode) && !$isElementNode(insertNode) && !$isDecoratorNode(insertNode)) {
    {
      formatDevErrorMessage(`Only element or decorator nodes can be inserted in to the root node`);
    }
  }
}
function $getNodeByKeyOrThrow(key) {
  const node = $getNodeByKey(key);
  if (node === null) {
    {
      formatDevErrorMessage(`Expected node with key ${key} to exist but it's not in the nodeMap.`);
    }
  }
  return node;
}
function createBlockCursorElement(editorConfig) {
  const theme = editorConfig.theme;
  const element = document.createElement("div");
  element.contentEditable = "false";
  element.setAttribute("data-lexical-cursor", "true");
  let blockCursorTheme = theme.blockCursor;
  if (blockCursorTheme !== void 0) {
    if (typeof blockCursorTheme === "string") {
      const classNamesArr = normalizeClassNames(blockCursorTheme);
      blockCursorTheme = theme.blockCursor = classNamesArr;
    }
    if (blockCursorTheme !== void 0) {
      element.classList.add(...blockCursorTheme);
    }
  }
  return element;
}
function needsBlockCursor(node) {
  return ($isDecoratorNode(node) || $isElementNode(node) && !node.canBeEmpty()) && !node.isInline();
}
function removeDOMBlockCursorElement(blockCursorElement, editor, rootElement) {
  rootElement.style.removeProperty("caret-color");
  editor._blockCursorElement = null;
  const parentElement = blockCursorElement.parentElement;
  if (parentElement !== null) {
    parentElement.removeChild(blockCursorElement);
  }
}
function updateDOMBlockCursorElement(editor, rootElement, nextSelection) {
  let blockCursorElement = editor._blockCursorElement;
  if ($isRangeSelection(nextSelection) && nextSelection.isCollapsed() && nextSelection.anchor.type === "element" && rootElement.contains(document.activeElement)) {
    const anchor = nextSelection.anchor;
    const elementNode = anchor.getNode();
    const offset = anchor.offset;
    const elementNodeSize = elementNode.getChildrenSize();
    let isBlockCursor = false;
    let insertBeforeElement = null;
    if (offset === elementNodeSize) {
      const child = elementNode.getChildAtIndex(offset - 1);
      if (needsBlockCursor(child)) {
        isBlockCursor = true;
      }
    } else {
      const child = elementNode.getChildAtIndex(offset);
      if (child !== null && needsBlockCursor(child)) {
        const sibling = child.getPreviousSibling();
        if (sibling === null || needsBlockCursor(sibling)) {
          isBlockCursor = true;
          insertBeforeElement = editor.getElementByKey(child.__key);
        }
      }
    }
    if (isBlockCursor) {
      const elementDOM = editor.getElementByKey(elementNode.__key);
      if (blockCursorElement === null) {
        editor._blockCursorElement = blockCursorElement = createBlockCursorElement(editor._config);
      }
      rootElement.style.caretColor = "transparent";
      if (insertBeforeElement === null) {
        elementDOM.appendChild(blockCursorElement);
      } else {
        elementDOM.insertBefore(blockCursorElement, insertBeforeElement);
      }
      return;
    }
  }
  if (blockCursorElement !== null) {
    removeDOMBlockCursorElement(blockCursorElement, editor, rootElement);
  }
}
function getDOMSelection(targetWindow) {
  return !CAN_USE_DOM ? null : (targetWindow || window).getSelection();
}
function getDOMSelectionFromTarget(eventTarget) {
  const defaultView = getDefaultView(eventTarget);
  return defaultView ? defaultView.getSelection() : null;
}
function $splitNode(node, offset) {
  let startNode = node.getChildAtIndex(offset);
  if (startNode == null) {
    startNode = node;
  }
  if (!!$isRootOrShadowRoot(node)) {
    formatDevErrorMessage(`Can not call $splitNode() on root element`);
  }
  const recurse = (currentNode) => {
    const parent = currentNode.getParentOrThrow();
    const isParentRoot = $isRootOrShadowRoot(parent);
    const nodeToMove = currentNode === startNode && !isParentRoot ? currentNode : $copyNode(currentNode);
    if (isParentRoot) {
      if (!($isElementNode(currentNode) && $isElementNode(nodeToMove))) {
        formatDevErrorMessage(`Children of a root must be ElementNode`);
      }
      currentNode.insertAfter(nodeToMove);
      return [currentNode, nodeToMove, nodeToMove];
    } else {
      const [leftTree2, rightTree2, newParent] = recurse(parent);
      const nextSiblings = currentNode.getNextSiblings();
      newParent.append(nodeToMove, ...nextSiblings);
      return [leftTree2, rightTree2, nodeToMove];
    }
  };
  const [leftTree, rightTree] = recurse(startNode);
  return [leftTree, rightTree];
}
function isHTMLAnchorElement(x2) {
  return isHTMLElement(x2) && x2.tagName === "A";
}
function isHTMLElement(x2) {
  return isDOMNode(x2) && x2.nodeType === DOM_ELEMENT_TYPE;
}
function isDOMNode(x2) {
  return typeof x2 === "object" && x2 !== null && "nodeType" in x2 && typeof x2.nodeType === "number";
}
function isDocumentFragment(x2) {
  return isDOMNode(x2) && x2.nodeType === DOM_DOCUMENT_FRAGMENT_TYPE;
}
function isInlineDomNode(node) {
  const inlineNodes = new RegExp(/^(a|abbr|acronym|b|cite|code|del|em|i|ins|kbd|label|mark|output|q|ruby|s|samp|span|strong|sub|sup|time|u|tt|var|#text)$/, "i");
  return node.nodeName.match(inlineNodes) !== null;
}
function isBlockDomNode(node) {
  const blockNodes = new RegExp(/^(address|article|aside|blockquote|canvas|dd|div|dl|dt|fieldset|figcaption|figure|footer|form|h1|h2|h3|h4|h5|h6|header|hr|li|main|nav|noscript|ol|p|pre|section|table|td|tfoot|ul|video)$/, "i");
  return node.nodeName.match(blockNodes) !== null;
}
function INTERNAL_$isBlock(node) {
  if ($isDecoratorNode(node) && !node.isInline()) {
    return true;
  }
  if (!$isElementNode(node) || $isRootOrShadowRoot(node)) {
    return false;
  }
  const firstChild = node.getFirstChild();
  const isLeafElement = firstChild === null || $isLineBreakNode(firstChild) || $isTextNode(firstChild) || firstChild.isInline();
  return !node.isInline() && node.canBeEmpty() !== false && isLeafElement;
}
function $getEditor() {
  return getActiveEditor();
}
function $getEditorDOMRenderConfig(editor = $getEditor()) {
  return editor._config.dom || DEFAULT_EDITOR_DOM_CONFIG;
}
var cachedNodeMaps = /* @__PURE__ */ new WeakMap();
var EMPTY_TYPE_TO_NODE_MAP = /* @__PURE__ */ new Map();
function getCachedTypeToNodeMap(editorState) {
  if (!editorState._readOnly && editorState.isEmpty()) {
    return EMPTY_TYPE_TO_NODE_MAP;
  }
  if (!editorState._readOnly) {
    formatDevErrorMessage(`getCachedTypeToNodeMap called with a writable EditorState`);
  }
  let typeToNodeMap = cachedNodeMaps.get(editorState);
  if (!typeToNodeMap) {
    typeToNodeMap = computeTypeToNodeMap(editorState);
    cachedNodeMaps.set(editorState, typeToNodeMap);
  }
  return typeToNodeMap;
}
function computeTypeToNodeMap(editorState) {
  const typeToNodeMap = /* @__PURE__ */ new Map();
  for (const [nodeKey, node] of editorState._nodeMap) {
    const nodeType = node.__type;
    let nodeMap = typeToNodeMap.get(nodeType);
    if (!nodeMap) {
      nodeMap = /* @__PURE__ */ new Map();
      typeToNodeMap.set(nodeType, nodeMap);
    }
    nodeMap.set(nodeKey, node);
  }
  return typeToNodeMap;
}
function $cloneWithProperties(latestNode) {
  const constructor = latestNode.constructor;
  const mutableNode = constructor.clone(latestNode);
  mutableNode.afterCloneFrom(latestNode);
  {
    if (!(mutableNode.__key === latestNode.__key)) {
      formatDevErrorMessage(`$cloneWithProperties: ${constructor.name}.clone(node) (with type '${constructor.getType()}') did not return a node with the same key, make sure to specify node.__key as the last argument to the constructor`);
    }
    if (!(mutableNode.__parent === latestNode.__parent && mutableNode.__next === latestNode.__next && mutableNode.__prev === latestNode.__prev)) {
      formatDevErrorMessage(`$cloneWithProperties: ${constructor.name}.clone(node) (with type '${constructor.getType()}') overrode afterCloneFrom but did not call super.afterCloneFrom(prevNode)`);
    }
  }
  return mutableNode;
}
function $cloneWithPropertiesEphemeral(latestNode) {
  return $markEphemeral($cloneWithProperties(latestNode));
}
function setNodeIndentFromDOM(elementDom, elementNode) {
  const indentSize = parseInt(elementDom.style.paddingInlineStart, 10) || 0;
  const indent = Math.round(indentSize / 40);
  elementNode.setIndent(indent);
}
function setDOMUnmanaged(elementDom) {
  const el = elementDom;
  el.__lexicalUnmanaged = true;
}
function isDOMUnmanaged(elementDom) {
  const el = elementDom;
  return el.__lexicalUnmanaged === true;
}
function hasOwn(o2, k) {
  return Object.prototype.hasOwnProperty.call(o2, k);
}
function hasOwnStaticMethod(klass, k) {
  return hasOwn(klass, k) && klass[k] !== LexicalNode[k];
}
function hasOwnExportDOM(klass) {
  return hasOwn(klass.prototype, "exportDOM");
}
function isAbstractNodeClass(klass) {
  if (!(klass === LexicalNode || klass.prototype instanceof LexicalNode)) {
    let ownNodeType = "<unknown>";
    let version = "<unknown>";
    try {
      ownNodeType = klass.getType();
    } catch (_err) {
    }
    try {
      if (LexicalEditor.version) {
        version = JSON.parse(LexicalEditor.version);
      }
    } catch (_err) {
    }
    {
      formatDevErrorMessage(`${klass.name} (type ${ownNodeType}) does not subclass LexicalNode from the lexical package used by this editor (version ${version}). All lexical and @lexical/* packages used by an editor must have identical versions. If you suspect the version does match, then the problem may be caused by multiple copies of the same lexical module (e.g. both esm and cjs, or included directly in multiple entrypoints).`);
    }
  }
  return klass === DecoratorNode || klass === ElementNode || klass === LexicalNode;
}
function getStaticNodeConfig(klass) {
  const nodeConfigRecord = PROTOTYPE_CONFIG_METHOD in klass.prototype ? klass.prototype[PROTOTYPE_CONFIG_METHOD]() : void 0;
  const isAbstract = isAbstractNodeClass(klass);
  const nodeType = !isAbstract && hasOwnStaticMethod(klass, "getType") ? klass.getType() : void 0;
  let ownNodeConfig;
  let ownNodeType = nodeType;
  if (nodeConfigRecord) {
    if (nodeType) {
      ownNodeConfig = nodeConfigRecord[nodeType];
    } else {
      for (const [k, v2] of Object.entries(nodeConfigRecord)) {
        ownNodeType = k;
        ownNodeConfig = v2;
      }
    }
  }
  if (!isAbstract && ownNodeType) {
    if (!hasOwnStaticMethod(klass, "getType")) {
      klass.getType = () => ownNodeType;
    }
    if (!hasOwnStaticMethod(klass, "clone")) {
      if (TextNode.length === 0) {
        if (!(klass.length === 0)) {
          formatDevErrorMessage(`${klass.name} (type ${ownNodeType}) must implement a static clone method since its constructor has ${String(klass.length)} required arguments (expecting 0). Use an explicit default in the first argument of your constructor(prop: T=X, nodeKey?: NodeKey).`);
        }
      }
      klass.clone = (prevNode) => {
        setPendingNodeToClone(prevNode);
        return new klass();
      };
    }
    if (!hasOwnStaticMethod(klass, "importJSON")) {
      if (TextNode.length === 0) {
        if (!(klass.length === 0)) {
          formatDevErrorMessage(`${klass.name} (type ${ownNodeType}) must implement a static importJSON method since its constructor has ${String(klass.length)} required arguments (expecting 0). Use an explicit default in the first argument of your constructor(prop: T=X, nodeKey?: NodeKey).`);
        }
      }
      klass.importJSON = ownNodeConfig && ownNodeConfig.$importJSON || ((serializedNode) => new klass().updateFromJSON(serializedNode));
    }
    if (!hasOwnStaticMethod(klass, "importDOM") && ownNodeConfig) {
      const {
        importDOM
      } = ownNodeConfig;
      if (importDOM) {
        klass.importDOM = () => importDOM;
      }
    }
  }
  return {
    ownNodeConfig,
    ownNodeType
  };
}
function $create(klass) {
  const editor = $getEditor();
  errorOnReadOnly();
  const registeredNode = editor.resolveRegisteredNodeAfterReplacements(editor.getRegisteredNode(klass));
  return new registeredNode.klass();
}
var $findMatchingParent = (startingNode, findFn) => {
  let curr = startingNode;
  while (curr != null && !$isRootNode(curr)) {
    if (findFn(curr)) {
      return curr;
    }
    curr = curr.getParent();
  }
  return null;
};
function $createChildrenArray(element, nodeMap) {
  const children = [];
  let nodeKey = element.__first;
  while (nodeKey !== null) {
    const node = nodeMap === null ? $getNodeByKey(nodeKey) : nodeMap.get(nodeKey);
    if (node === null || node === void 0) {
      {
        formatDevErrorMessage(`$createChildrenArray: node does not exist in nodeMap`);
      }
    }
    children.push(nodeKey);
    nodeKey = node.__next;
  }
  return children;
}
var FLIP_DIRECTION = {
  next: "previous",
  previous: "next"
};
var AbstractCaret = class {
  origin;
  constructor(origin) {
    this.origin = origin;
  }
  [Symbol.iterator]() {
    return makeStepwiseIterator({
      hasNext: $isSiblingCaret,
      initial: this.getAdjacentCaret(),
      map: (caret) => caret,
      step: (caret) => caret.getAdjacentCaret()
    });
  }
  getAdjacentCaret() {
    return $getSiblingCaret(this.getNodeAtCaret(), this.direction);
  }
  getSiblingCaret() {
    return $getSiblingCaret(this.origin, this.direction);
  }
  remove() {
    const node = this.getNodeAtCaret();
    if (node) {
      node.remove();
    }
    return this;
  }
  replaceOrInsert(node, includeChildren) {
    const target = this.getNodeAtCaret();
    if (node.is(this.origin) || node.is(target)) ;
    else if (target === null) {
      this.insert(node);
    } else {
      target.replace(node, includeChildren);
    }
    return this;
  }
  splice(deleteCount, nodes, nodesDirection = "next") {
    const nodeIter = nodesDirection === this.direction ? nodes : Array.from(nodes).reverse();
    let caret = this;
    const parent = this.getParentAtCaret();
    const nodesToRemove = /* @__PURE__ */ new Map();
    for (let removeCaret = caret.getAdjacentCaret(); removeCaret !== null && nodesToRemove.size < deleteCount; removeCaret = removeCaret.getAdjacentCaret()) {
      const writableNode = removeCaret.origin.getWritable();
      nodesToRemove.set(writableNode.getKey(), writableNode);
    }
    for (const node of nodeIter) {
      if (nodesToRemove.size > 0) {
        const target = caret.getNodeAtCaret();
        if (target) {
          nodesToRemove.delete(target.getKey());
          nodesToRemove.delete(node.getKey());
          if (target.is(node) || caret.origin.is(node)) ;
          else {
            const nodeParent = node.getParent();
            if (nodeParent && nodeParent.is(parent)) {
              node.remove();
            }
            target.replace(node);
          }
        } else {
          if (!(target !== null)) {
            formatDevErrorMessage(`NodeCaret.splice: Underflow of expected nodesToRemove during splice (keys: ${Array.from(nodesToRemove).join(" ")})`);
          }
        }
      } else {
        caret.insert(node);
      }
      caret = $getSiblingCaret(node, this.direction);
    }
    for (const node of nodesToRemove.values()) {
      node.remove();
    }
    return this;
  }
};
var AbstractChildCaret = class _AbstractChildCaret extends AbstractCaret {
  type = "child";
  getLatest() {
    const origin = this.origin.getLatest();
    return origin === this.origin ? this : $getChildCaret(origin, this.direction);
  }
  /**
   * Get the SiblingCaret from this origin in the same direction.
   *
   * @param mode 'root' to return null at the root, 'shadowRoot' to return null at the root or any shadow root
   * @returns A SiblingCaret with this origin, or null if origin is a root according to mode.
   */
  getParentCaret(mode = "root") {
    return $getSiblingCaret($filterByMode(this.getParentAtCaret(), mode), this.direction);
  }
  getFlipped() {
    const dir = flipDirection(this.direction);
    return $getSiblingCaret(this.getNodeAtCaret(), dir) || $getChildCaret(this.origin, dir);
  }
  getParentAtCaret() {
    return this.origin;
  }
  getChildCaret() {
    return this;
  }
  isSameNodeCaret(other) {
    return other instanceof _AbstractChildCaret && this.direction === other.direction && this.origin.is(other.origin);
  }
  isSamePointCaret(other) {
    return this.isSameNodeCaret(other);
  }
};
var ChildCaretFirst = class extends AbstractChildCaret {
  direction = "next";
  getNodeAtCaret() {
    return this.origin.getFirstChild();
  }
  insert(node) {
    this.origin.splice(0, 0, [node]);
    return this;
  }
};
var ChildCaretLast = class extends AbstractChildCaret {
  direction = "previous";
  getNodeAtCaret() {
    return this.origin.getLastChild();
  }
  insert(node) {
    this.origin.splice(this.origin.getChildrenSize(), 0, [node]);
    return this;
  }
};
var MODE_PREDICATE = {
  root: $isRootNode,
  shadowRoot: $isRootOrShadowRoot
};
function flipDirection(direction) {
  return FLIP_DIRECTION[direction];
}
function $filterByMode(node, mode = "root") {
  return MODE_PREDICATE[mode](node) ? null : node;
}
var AbstractSiblingCaret = class _AbstractSiblingCaret extends AbstractCaret {
  type = "sibling";
  getLatest() {
    const origin = this.origin.getLatest();
    return origin === this.origin ? this : $getSiblingCaret(origin, this.direction);
  }
  getSiblingCaret() {
    return this;
  }
  getParentAtCaret() {
    return this.origin.getParent();
  }
  getChildCaret() {
    return $isElementNode(this.origin) ? $getChildCaret(this.origin, this.direction) : null;
  }
  getParentCaret(mode = "root") {
    return $getSiblingCaret($filterByMode(this.getParentAtCaret(), mode), this.direction);
  }
  getFlipped() {
    const dir = flipDirection(this.direction);
    return $getSiblingCaret(this.getNodeAtCaret(), dir) || $getChildCaret(this.origin.getParentOrThrow(), dir);
  }
  isSamePointCaret(other) {
    return other instanceof _AbstractSiblingCaret && this.direction === other.direction && this.origin.is(other.origin);
  }
  isSameNodeCaret(other) {
    return (other instanceof _AbstractSiblingCaret || other instanceof AbstractTextPointCaret) && this.direction === other.direction && this.origin.is(other.origin);
  }
};
var AbstractTextPointCaret = class _AbstractTextPointCaret extends AbstractCaret {
  type = "text";
  offset;
  constructor(origin, offset) {
    super(origin);
    this.offset = offset;
  }
  getLatest() {
    const origin = this.origin.getLatest();
    return origin === this.origin ? this : $getTextPointCaret(origin, this.direction, this.offset);
  }
  getParentAtCaret() {
    return this.origin.getParent();
  }
  getChildCaret() {
    return null;
  }
  getParentCaret(mode = "root") {
    return $getSiblingCaret($filterByMode(this.getParentAtCaret(), mode), this.direction);
  }
  getFlipped() {
    return $getTextPointCaret(this.origin, flipDirection(this.direction), this.offset);
  }
  isSamePointCaret(other) {
    return other instanceof _AbstractTextPointCaret && this.direction === other.direction && this.origin.is(other.origin) && this.offset === other.offset;
  }
  isSameNodeCaret(other) {
    return (other instanceof AbstractSiblingCaret || other instanceof _AbstractTextPointCaret) && this.direction === other.direction && this.origin.is(other.origin);
  }
  getSiblingCaret() {
    return $getSiblingCaret(this.origin, this.direction);
  }
};
function $isTextPointCaret(caret) {
  return caret instanceof AbstractTextPointCaret;
}
function $isNodeCaret(caret) {
  return caret instanceof AbstractCaret;
}
function $isSiblingCaret(caret) {
  return caret instanceof AbstractSiblingCaret;
}
function $isChildCaret(caret) {
  return caret instanceof AbstractChildCaret;
}
var SiblingCaretNext = class extends AbstractSiblingCaret {
  direction = "next";
  getNodeAtCaret() {
    return this.origin.getNextSibling();
  }
  insert(node) {
    this.origin.insertAfter(node);
    return this;
  }
};
var SiblingCaretPrevious = class extends AbstractSiblingCaret {
  direction = "previous";
  getNodeAtCaret() {
    return this.origin.getPreviousSibling();
  }
  insert(node) {
    this.origin.insertBefore(node);
    return this;
  }
};
var TextPointCaretNext = class extends AbstractTextPointCaret {
  direction = "next";
  getNodeAtCaret() {
    return this.origin.getNextSibling();
  }
  insert(node) {
    this.origin.insertAfter(node);
    return this;
  }
};
var TextPointCaretPrevious = class extends AbstractTextPointCaret {
  direction = "previous";
  getNodeAtCaret() {
    return this.origin.getPreviousSibling();
  }
  insert(node) {
    this.origin.insertBefore(node);
    return this;
  }
};
var TEXT_CTOR = {
  next: TextPointCaretNext,
  previous: TextPointCaretPrevious
};
var SIBLING_CTOR = {
  next: SiblingCaretNext,
  previous: SiblingCaretPrevious
};
var CHILD_CTOR = {
  next: ChildCaretFirst,
  previous: ChildCaretLast
};
function $getSiblingCaret(origin, direction) {
  return origin ? new SIBLING_CTOR[direction](origin) : null;
}
function $getTextPointCaret(origin, direction, offset) {
  return origin ? new TEXT_CTOR[direction](origin, $getTextNodeOffset(origin, offset)) : null;
}
function $getTextNodeOffset(origin, offset, mode = "error") {
  const size = origin.getTextContentSize();
  let numericOffset = offset === "next" ? size : offset === "previous" ? 0 : offset;
  if (numericOffset < 0 || numericOffset > size) {
    if (!(mode === "clamp")) {
      formatDevErrorMessage(`$getTextNodeOffset: invalid offset ${String(offset)} for size ${String(size)} at key ${origin.getKey()}`);
    }
    numericOffset = numericOffset < 0 ? 0 : size;
  }
  return numericOffset;
}
function $getTextPointCaretSlice(caret, distance) {
  return new TextPointCaretSliceImpl(caret, distance);
}
function $getChildCaret(origin, direction) {
  return $isElementNode(origin) ? new CHILD_CTOR[direction](origin) : null;
}
function $getChildCaretOrSelf(caret) {
  return caret && caret.getChildCaret() || caret;
}
function $getAdjacentChildCaret(caret) {
  return caret && $getChildCaretOrSelf(caret.getAdjacentCaret());
}
var CaretRangeImpl = class _CaretRangeImpl {
  type = "node-caret-range";
  direction;
  anchor;
  focus;
  constructor(anchor, focus, direction) {
    this.anchor = anchor;
    this.focus = focus;
    this.direction = direction;
  }
  getLatest() {
    const anchor = this.anchor.getLatest();
    const focus = this.focus.getLatest();
    return anchor === this.anchor && focus === this.focus ? this : new _CaretRangeImpl(anchor, focus, this.direction);
  }
  isCollapsed() {
    return this.anchor.isSamePointCaret(this.focus);
  }
  getTextSlices() {
    const getSlice = (k) => {
      const caret = this[k].getLatest();
      return $isTextPointCaret(caret) ? $getSliceFromTextPointCaret(caret, k) : null;
    };
    const anchorSlice = getSlice("anchor");
    const focusSlice = getSlice("focus");
    if (anchorSlice && focusSlice) {
      const {
        caret: anchorCaret
      } = anchorSlice;
      const {
        caret: focusCaret
      } = focusSlice;
      if (anchorCaret.isSameNodeCaret(focusCaret)) {
        return [$getTextPointCaretSlice(anchorCaret, focusCaret.offset - anchorCaret.offset), null];
      }
    }
    return [anchorSlice, focusSlice];
  }
  iterNodeCarets(rootMode = "root") {
    const anchor = $isTextPointCaret(this.anchor) ? this.anchor.getSiblingCaret() : this.anchor.getLatest();
    const focus = this.focus.getLatest();
    const isTextFocus = $isTextPointCaret(focus);
    const step = (state) => state.isSameNodeCaret(focus) ? null : $getAdjacentChildCaret(state) || state.getParentCaret(rootMode);
    return makeStepwiseIterator({
      hasNext: (state) => state !== null && !(isTextFocus && focus.isSameNodeCaret(state)),
      initial: anchor.isSameNodeCaret(focus) ? null : step(anchor),
      map: (state) => state,
      step
    });
  }
  [Symbol.iterator]() {
    return this.iterNodeCarets("root");
  }
};
var TextPointCaretSliceImpl = class {
  type = "slice";
  caret;
  distance;
  constructor(caret, distance) {
    this.caret = caret;
    this.distance = distance;
  }
  getSliceIndices() {
    const {
      distance,
      caret: {
        offset
      }
    } = this;
    const offsetB = offset + distance;
    return offsetB < offset ? [offsetB, offset] : [offset, offsetB];
  }
  getTextContent() {
    const [startIndex, endIndex] = this.getSliceIndices();
    return this.caret.origin.getTextContent().slice(startIndex, endIndex);
  }
  getTextContentSize() {
    return Math.abs(this.distance);
  }
  removeTextSlice() {
    const {
      caret: {
        origin,
        direction
      }
    } = this;
    const [indexStart, indexEnd] = this.getSliceIndices();
    const text = origin.getTextContent();
    return $getTextPointCaret(origin.setTextContent(text.slice(0, indexStart) + text.slice(indexEnd)), direction, indexStart);
  }
};
function $getSliceFromTextPointCaret(caret, anchorOrFocus) {
  const {
    direction,
    origin
  } = caret;
  const offsetB = $getTextNodeOffset(origin, anchorOrFocus === "focus" ? flipDirection(direction) : direction);
  return $getTextPointCaretSlice(caret, offsetB - caret.offset);
}
function $isTextPointCaretSlice(caretOrSlice) {
  return caretOrSlice instanceof TextPointCaretSliceImpl;
}
function $extendCaretToRange(anchor) {
  return $getCaretRange(anchor, $getSiblingCaret($getRoot(), anchor.direction));
}
function $getCollapsedCaretRange(anchor) {
  return $getCaretRange(anchor, anchor);
}
function $getCaretRange(anchor, focus) {
  if (!(anchor.direction === focus.direction)) {
    formatDevErrorMessage(`$getCaretRange: anchor and focus must be in the same direction`);
  }
  return new CaretRangeImpl(anchor, focus, anchor.direction);
}
function makeStepwiseIterator(config) {
  const {
    initial,
    hasNext,
    step,
    map
  } = config;
  let state = initial;
  return {
    [Symbol.iterator]() {
      return this;
    },
    next() {
      if (!hasNext(state)) {
        return {
          done: true,
          value: void 0
        };
      }
      const rval = {
        done: false,
        value: map(state)
      };
      state = step(state);
      return rval;
    }
  };
}
function compareNumber(a2, b2) {
  return Math.sign(a2 - b2);
}
function $comparePointCaretNext(a2, b2) {
  const compare = $getCommonAncestor(a2.origin, b2.origin);
  if (!(compare !== null)) {
    formatDevErrorMessage(`$comparePointCaretNext: a (key ${a2.origin.getKey()}) and b (key ${b2.origin.getKey()}) do not have a common ancestor`);
  }
  switch (compare.type) {
    case "same": {
      const aIsText = a2.type === "text";
      const bIsText = b2.type === "text";
      return aIsText && bIsText ? compareNumber(a2.offset, b2.offset) : a2.type === b2.type ? 0 : aIsText ? -1 : bIsText ? 1 : a2.type === "child" ? -1 : 1;
    }
    case "ancestor": {
      return a2.type === "child" ? -1 : 1;
    }
    case "descendant": {
      return b2.type === "child" ? 1 : -1;
    }
    case "branch": {
      return $getCommonAncestorResultBranchOrder(compare);
    }
  }
}
function $getCommonAncestorResultBranchOrder(compare) {
  const {
    a: a2,
    b: b2
  } = compare;
  const aKey = a2.__key;
  const bKey = b2.__key;
  let na = a2;
  let nb = b2;
  for (; na && nb; na = na.getNextSibling(), nb = nb.getNextSibling()) {
    if (na.__key === bKey) {
      return -1;
    } else if (nb.__key === aKey) {
      return 1;
    }
  }
  return na === null ? 1 : -1;
}
function $isSameNode(reference, other) {
  return other.is(reference);
}
function $initialElementTuple(node) {
  return $isElementNode(node) ? [node.getLatest(), null] : [node.getParent(), node.getLatest()];
}
function $getCommonAncestor(a2, b2) {
  if (a2.is(b2)) {
    return {
      commonAncestor: a2,
      type: "same"
    };
  }
  const aMap = /* @__PURE__ */ new Map();
  for (let [parent, child] = $initialElementTuple(a2); parent; child = parent, parent = parent.getParent()) {
    aMap.set(parent, child);
  }
  for (let [parent, child] = $initialElementTuple(b2); parent; child = parent, parent = parent.getParent()) {
    const aChild = aMap.get(parent);
    if (aChild === void 0) ;
    else if (aChild === null) {
      if (!$isSameNode(a2, parent)) {
        formatDevErrorMessage(`$originComparison: ancestor logic error`);
      }
      return {
        commonAncestor: parent,
        type: "ancestor"
      };
    } else if (child === null) {
      if (!$isSameNode(b2, parent)) {
        formatDevErrorMessage(`$originComparison: descendant logic error`);
      }
      return {
        commonAncestor: parent,
        type: "descendant"
      };
    } else {
      if (!(($isElementNode(aChild) || $isSameNode(a2, aChild)) && ($isElementNode(child) || $isSameNode(b2, child)) && parent.is(aChild.getParent()) && parent.is(child.getParent()))) {
        formatDevErrorMessage(`$originComparison: branch logic error`);
      }
      return {
        a: aChild,
        b: child,
        commonAncestor: parent,
        type: "branch"
      };
    }
  }
  return null;
}
function $caretFromPoint(point, direction) {
  const {
    type,
    key,
    offset
  } = point;
  const node = $getNodeByKeyOrThrow(point.key);
  if (type === "text") {
    if (!$isTextNode(node)) {
      formatDevErrorMessage(`$caretFromPoint: Node with type ${node.getType()} and key ${key} that does not inherit from TextNode encountered for text point`);
    }
    return $getTextPointCaret(node, direction, offset);
  }
  if (!$isElementNode(node)) {
    formatDevErrorMessage(`$caretFromPoint: Node with type ${node.getType()} and key ${key} that does not inherit from ElementNode encountered for element point`);
  }
  return $getChildCaretAtIndex(node, point.offset, direction);
}
function $setPointFromCaret(point, caret) {
  const {
    origin,
    direction
  } = caret;
  const isNext = direction === "next";
  if ($isTextPointCaret(caret)) {
    point.set(origin.getKey(), caret.offset, "text");
  } else if ($isSiblingCaret(caret)) {
    if ($isTextNode(origin)) {
      point.set(origin.getKey(), $getTextNodeOffset(origin, direction), "text");
    } else {
      point.set(origin.getParentOrThrow().getKey(), origin.getIndexWithinParent() + (isNext ? 1 : 0), "element");
    }
  } else {
    if (!($isChildCaret(caret) && $isElementNode(origin))) {
      formatDevErrorMessage(`$setPointFromCaret: exhaustiveness check`);
    }
    point.set(origin.getKey(), isNext ? 0 : origin.getChildrenSize(), "element");
  }
}
function $setSelectionFromCaretRange(caretRange) {
  const currentSelection = $getSelection();
  const selection = $isRangeSelection(currentSelection) ? currentSelection : $createRangeSelection();
  $updateRangeSelectionFromCaretRange(selection, caretRange);
  $setSelection(selection);
  return selection;
}
function $updateRangeSelectionFromCaretRange(selection, caretRange) {
  $setPointFromCaret(selection.anchor, caretRange.anchor);
  $setPointFromCaret(selection.focus, caretRange.focus);
}
function $caretRangeFromSelection(selection) {
  const {
    anchor,
    focus
  } = selection;
  const anchorCaret = $caretFromPoint(anchor, "next");
  const focusCaret = $caretFromPoint(focus, "next");
  const direction = $comparePointCaretNext(anchorCaret, focusCaret) <= 0 ? "next" : "previous";
  return $getCaretRange($getCaretInDirection(anchorCaret, direction), $getCaretInDirection(focusCaret, direction));
}
function $rewindSiblingCaret(caret) {
  const {
    direction,
    origin
  } = caret;
  const rewindOrigin = $getSiblingCaret(origin, flipDirection(direction)).getNodeAtCaret();
  return rewindOrigin ? $getSiblingCaret(rewindOrigin, direction) : $getChildCaret(origin.getParentOrThrow(), direction);
}
function $getAnchorCandidates(anchor, rootMode = "root") {
  const carets = [anchor];
  for (let parent = $isChildCaret(anchor) ? anchor.getParentCaret(rootMode) : anchor.getSiblingCaret(); parent !== null; parent = parent.getParentCaret(rootMode)) {
    carets.push($rewindSiblingCaret(parent));
  }
  return carets;
}
function $isCaretAttached(caret) {
  return !!caret && caret.origin.isAttached();
}
function $removeTextFromCaretRange(initialRange, sliceMode = "removeEmptySlices") {
  if (initialRange.isCollapsed()) {
    return initialRange;
  }
  const rootMode = "root";
  const nextDirection = "next";
  let sliceState = sliceMode;
  const range = $getCaretRangeInDirection(initialRange, nextDirection);
  const anchorCandidates = $getAnchorCandidates(range.anchor, rootMode);
  const focusCandidates = $getAnchorCandidates(range.focus.getFlipped(), rootMode);
  const seenStart = /* @__PURE__ */ new Set();
  const removedNodes = [];
  for (const caret of range.iterNodeCarets(rootMode)) {
    if ($isChildCaret(caret)) {
      seenStart.add(caret.origin.getKey());
    } else if ($isSiblingCaret(caret)) {
      const {
        origin
      } = caret;
      if (!$isElementNode(origin) || seenStart.has(origin.getKey())) {
        removedNodes.push(origin);
      }
    }
  }
  for (const node of removedNodes) {
    node.remove();
  }
  for (const slice of range.getTextSlices()) {
    if (!slice) {
      continue;
    }
    const {
      origin
    } = slice.caret;
    const contentSize = origin.getTextContentSize();
    const caretBefore = $rewindSiblingCaret($getSiblingCaret(origin, nextDirection));
    const mode = origin.getMode();
    if (Math.abs(slice.distance) === contentSize && sliceState === "removeEmptySlices" || mode === "token" && slice.distance !== 0) {
      caretBefore.remove();
    } else if (slice.distance !== 0) {
      sliceState = "removeEmptySlices";
      let nextCaret = slice.removeTextSlice();
      const sliceOrigin = slice.caret.origin;
      if (mode === "segmented") {
        const src = nextCaret.origin;
        const plainTextNode = $createTextNode(src.getTextContent()).setStyle(src.getStyle()).setFormat(src.getFormat());
        caretBefore.replaceOrInsert(plainTextNode);
        nextCaret = $getTextPointCaret(plainTextNode, nextDirection, nextCaret.offset);
      }
      if (sliceOrigin.is(anchorCandidates[0].origin)) {
        anchorCandidates[0] = nextCaret;
      }
      if (sliceOrigin.is(focusCandidates[0].origin)) {
        focusCandidates[0] = nextCaret.getFlipped();
      }
    }
  }
  let anchorCandidate;
  let focusCandidate;
  for (const candidate of anchorCandidates) {
    if ($isCaretAttached(candidate)) {
      anchorCandidate = $normalizeCaret(candidate);
      break;
    }
  }
  for (const candidate of focusCandidates) {
    if ($isCaretAttached(candidate)) {
      focusCandidate = $normalizeCaret(candidate);
      break;
    }
  }
  const mergeTargets = $getBlockMergeTargets(anchorCandidate, focusCandidate, seenStart);
  if (mergeTargets) {
    const [anchorBlock, focusBlock] = mergeTargets;
    $getChildCaret(anchorBlock, "previous").splice(0, focusBlock.getChildren());
    let parent = focusBlock.getParent();
    focusBlock.remove(true);
    while (parent && parent.isEmpty()) {
      const element = parent;
      parent = parent.getParent();
      element.remove(true);
    }
  }
  const bestCandidate = [anchorCandidate, focusCandidate, ...anchorCandidates, ...focusCandidates].find($isCaretAttached);
  if (bestCandidate) {
    const anchor = $getCaretInDirection($normalizeCaret(bestCandidate), initialRange.direction);
    return $getCollapsedCaretRange(anchor);
  }
  {
    formatDevErrorMessage(`$removeTextFromCaretRange: selection was lost, could not find a new anchor given candidates with keys: ${JSON.stringify(anchorCandidates.map((n2) => n2.origin.__key))}`);
  }
}
function $getBlockMergeTargets(anchor, focus, seenStart) {
  if (!anchor || !focus) {
    return null;
  }
  const anchorParent = anchor.getParentAtCaret();
  const focusParent = focus.getParentAtCaret();
  if (!anchorParent || !focusParent) {
    return null;
  }
  const anchorElements = anchorParent.getParents().reverse();
  anchorElements.push(anchorParent);
  const focusElements = focusParent.getParents().reverse();
  focusElements.push(focusParent);
  const maxLen = Math.min(anchorElements.length, focusElements.length);
  let commonAncestorCount;
  for (commonAncestorCount = 0; commonAncestorCount < maxLen && anchorElements[commonAncestorCount] === focusElements[commonAncestorCount]; commonAncestorCount++) {
  }
  const $getBlock = (arr, predicate) => {
    let block;
    for (let i2 = commonAncestorCount; i2 < arr.length; i2++) {
      const ancestor = arr[i2];
      if ($isRootOrShadowRoot(ancestor)) {
        return;
      } else if (!block && predicate(ancestor)) {
        block = ancestor;
      }
    }
    return block;
  };
  const anchorBlock = $getBlock(anchorElements, INTERNAL_$isBlock);
  const focusBlock = anchorBlock && $getBlock(focusElements, (node) => seenStart.has(node.getKey()) && INTERNAL_$isBlock(node));
  return anchorBlock && focusBlock ? [anchorBlock, focusBlock] : null;
}
function $getDeepestChildOrSelf(initialCaret) {
  let caret = initialCaret;
  while ($isChildCaret(caret)) {
    const adjacent = $getAdjacentChildCaret(caret);
    if (!$isChildCaret(adjacent)) {
      break;
    }
    caret = adjacent;
  }
  return caret;
}
function $normalizeCaret(initialCaret) {
  const caret = $getDeepestChildOrSelf(initialCaret.getLatest());
  const {
    direction
  } = caret;
  if ($isTextNode(caret.origin)) {
    return $isTextPointCaret(caret) ? caret : $getTextPointCaret(caret.origin, direction, direction);
  }
  const adj = caret.getAdjacentCaret();
  return $isSiblingCaret(adj) && $isTextNode(adj.origin) ? $getTextPointCaret(adj.origin, direction, flipDirection(direction)) : caret;
}
function $isExtendableTextPointCaret(caret) {
  return $isTextPointCaret(caret) && caret.offset !== $getTextNodeOffset(caret.origin, caret.direction);
}
function $getCaretInDirection(caret, direction) {
  return caret.direction === direction ? caret : caret.getFlipped();
}
function $getCaretRangeInDirection(range, direction) {
  if (range.direction === direction) {
    return range;
  }
  return $getCaretRange(
    // focus and anchor get flipped here
    $getCaretInDirection(range.focus, direction),
    $getCaretInDirection(range.anchor, direction)
  );
}
function $getChildCaretAtIndex(parent, index, direction) {
  let caret = $getChildCaret(parent, "next");
  for (let i2 = 0; i2 < index; i2++) {
    const nextCaret = caret.getAdjacentCaret();
    if (nextCaret === null) {
      break;
    }
    caret = nextCaret;
  }
  return $getCaretInDirection(caret, direction);
}
function $getAdjacentSiblingOrParentSiblingCaret(startCaret, rootMode = "root") {
  let depthDiff = 0;
  let caret = startCaret;
  let nextCaret = $getAdjacentChildCaret(caret);
  while (nextCaret === null) {
    depthDiff--;
    nextCaret = caret.getParentCaret(rootMode);
    if (!nextCaret) {
      return null;
    }
    caret = nextCaret;
    nextCaret = $getAdjacentChildCaret(caret);
  }
  return nextCaret && [nextCaret, depthDiff];
}
function $getAdjacentNodes(initialCaret) {
  const siblings = [];
  for (let caret = initialCaret.getAdjacentCaret(); caret; caret = caret.getAdjacentCaret()) {
    siblings.push(caret.origin);
  }
  return siblings;
}
function $splitTextPointCaret(textPointCaret) {
  const {
    origin,
    offset,
    direction
  } = textPointCaret;
  if (offset === $getTextNodeOffset(origin, direction)) {
    return textPointCaret.getSiblingCaret();
  } else if (offset === $getTextNodeOffset(origin, flipDirection(direction))) {
    return $rewindSiblingCaret(textPointCaret.getSiblingCaret());
  }
  const [textNode] = origin.splitText(offset);
  if (!$isTextNode(textNode)) {
    formatDevErrorMessage(`$splitTextPointCaret: splitText must return at least one TextNode`);
  }
  return $getCaretInDirection($getSiblingCaret(textNode, "next"), direction);
}
function $alwaysSplit(_node, _edge) {
  return true;
}
function $splitAtPointCaretNext(pointCaret, {
  $copyElementNode = $copyNode,
  $splitTextPointCaretNext = $splitTextPointCaret,
  rootMode = "shadowRoot",
  $shouldSplit = $alwaysSplit,
  removeEmptyDestination = false
} = {}) {
  if ($isTextPointCaret(pointCaret)) {
    return $splitTextPointCaretNext(pointCaret);
  }
  const parentCaret = pointCaret.getParentCaret(rootMode);
  if (parentCaret) {
    const {
      origin
    } = parentCaret;
    if ($isChildCaret(pointCaret)) {
      const beforeParentCaret = $rewindSiblingCaret(parentCaret);
      if (removeEmptyDestination && origin.isEmpty()) {
        origin.remove();
        return beforeParentCaret;
      }
      if (!(origin.canBeEmpty() && $shouldSplit(origin, "first"))) {
        return beforeParentCaret;
      }
    }
    const siblings = $getAdjacentNodes(pointCaret);
    if (siblings.length > 0 || !removeEmptyDestination && origin.canBeEmpty() && $shouldSplit(origin, "last")) {
      parentCaret.insert($copyElementNode(origin).splice(0, 0, siblings));
    }
  }
  return parentCaret;
}
// @__NO_SIDE_EFFECTS__
function defineExtension(extension) {
  return extension;
}
// @__NO_SIDE_EFFECTS__
function configExtension(...args) {
  return args;
}
// @__NO_SIDE_EFFECTS__
function declarePeerDependency(name, config) {
  return [name, config];
}
// @__NO_SIDE_EFFECTS__
function safeCast(value) {
  return value;
}
function shallowMergeConfig(config, overrides) {
  if (!overrides || config === overrides) {
    return config;
  }
  for (const k in overrides) {
    if (config[k] !== overrides[k]) {
      return {
        ...config,
        ...overrides
      };
    }
  }
  return config;
}
function normalizeClassNames(...classNames) {
  const rval = [];
  for (const className of classNames) {
    if (className && typeof className === "string") {
      for (const [s2] of className.matchAll(/\S+/g)) {
        rval.push(s2);
      }
    }
  }
  return rval;
}
function addClassNamesToElement(element, ...classNames) {
  const classesToAdd = normalizeClassNames(...classNames);
  if (classesToAdd.length > 0) {
    element.classList.add(...classesToAdd);
  }
}
function removeClassNamesFromElement(element, ...classNames) {
  const classesToRemove = normalizeClassNames(...classNames);
  if (classesToRemove.length > 0) {
    element.classList.remove(...classesToRemove);
  }
}
function mergeRegister(...func) {
  return () => {
    for (let i2 = func.length - 1; i2 >= 0; i2--) {
      func[i2]();
    }
    func.length = 0;
  };
}

// node_modules/lexical/Lexical.mjs
var mod = true ? Lexical_dev_exports : Lexical_prod_exports;
var $addUpdateTag2 = mod.$addUpdateTag;
var $applyNodeReplacement2 = mod.$applyNodeReplacement;
var $caretFromPoint2 = mod.$caretFromPoint;
var $caretRangeFromSelection2 = mod.$caretRangeFromSelection;
var $cloneWithProperties2 = mod.$cloneWithProperties;
var $cloneWithPropertiesEphemeral2 = mod.$cloneWithPropertiesEphemeral;
var $comparePointCaretNext2 = mod.$comparePointCaretNext;
var $copyNode2 = mod.$copyNode;
var $create2 = mod.$create;
var $createChildrenArray2 = mod.$createChildrenArray;
var $createLineBreakNode2 = mod.$createLineBreakNode;
var $createNodeSelection2 = mod.$createNodeSelection;
var $createParagraphNode2 = mod.$createParagraphNode;
var $createPoint2 = mod.$createPoint;
var $createRangeSelection2 = mod.$createRangeSelection;
var $createRangeSelectionFromDom2 = mod.$createRangeSelectionFromDom;
var $createTabNode2 = mod.$createTabNode;
var $createTextNode2 = mod.$createTextNode;
var $extendCaretToRange2 = mod.$extendCaretToRange;
var $findMatchingParent2 = mod.$findMatchingParent;
var $getAdjacentChildCaret2 = mod.$getAdjacentChildCaret;
var $getAdjacentNode2 = mod.$getAdjacentNode;
var $getAdjacentSiblingOrParentSiblingCaret2 = mod.$getAdjacentSiblingOrParentSiblingCaret;
var $getCaretInDirection2 = mod.$getCaretInDirection;
var $getCaretRange2 = mod.$getCaretRange;
var $getCaretRangeInDirection2 = mod.$getCaretRangeInDirection;
var $getCharacterOffsets2 = mod.$getCharacterOffsets;
var $getChildCaret2 = mod.$getChildCaret;
var $getChildCaretAtIndex2 = mod.$getChildCaretAtIndex;
var $getChildCaretOrSelf2 = mod.$getChildCaretOrSelf;
var $getCollapsedCaretRange2 = mod.$getCollapsedCaretRange;
var $getCommonAncestor2 = mod.$getCommonAncestor;
var $getCommonAncestorResultBranchOrder2 = mod.$getCommonAncestorResultBranchOrder;
var $getEditor2 = mod.$getEditor;
var $getEditorDOMRenderConfig2 = mod.$getEditorDOMRenderConfig;
var $getNearestNodeFromDOMNode2 = mod.$getNearestNodeFromDOMNode;
var $getNearestRootOrShadowRoot2 = mod.$getNearestRootOrShadowRoot;
var $getNodeByKey2 = mod.$getNodeByKey;
var $getNodeByKeyOrThrow2 = mod.$getNodeByKeyOrThrow;
var $getNodeFromDOMNode2 = mod.$getNodeFromDOMNode;
var $getPreviousSelection2 = mod.$getPreviousSelection;
var $getRoot2 = mod.$getRoot;
var $getSelection2 = mod.$getSelection;
var $getSiblingCaret2 = mod.$getSiblingCaret;
var $getState2 = mod.$getState;
var $getStateChange2 = mod.$getStateChange;
var $getTextContent2 = mod.$getTextContent;
var $getTextNodeOffset2 = mod.$getTextNodeOffset;
var $getTextPointCaret2 = mod.$getTextPointCaret;
var $getTextPointCaretSlice2 = mod.$getTextPointCaretSlice;
var $getWritableNodeState2 = mod.$getWritableNodeState;
var $hasAncestor2 = mod.$hasAncestor;
var $hasUpdateTag2 = mod.$hasUpdateTag;
var $insertNodes2 = mod.$insertNodes;
var $isBlockElementNode2 = mod.$isBlockElementNode;
var $isChildCaret2 = mod.$isChildCaret;
var $isDecoratorNode2 = mod.$isDecoratorNode;
var $isEditorState2 = mod.$isEditorState;
var $isElementNode2 = mod.$isElementNode;
var $isExtendableTextPointCaret2 = mod.$isExtendableTextPointCaret;
var $isInlineElementOrDecoratorNode2 = mod.$isInlineElementOrDecoratorNode;
var $isLeafNode2 = mod.$isLeafNode;
var $isLexicalNode2 = mod.$isLexicalNode;
var $isLineBreakNode2 = mod.$isLineBreakNode;
var $isNodeCaret2 = mod.$isNodeCaret;
var $isNodeSelection2 = mod.$isNodeSelection;
var $isParagraphNode2 = mod.$isParagraphNode;
var $isRangeSelection2 = mod.$isRangeSelection;
var $isRootNode2 = mod.$isRootNode;
var $isRootOrShadowRoot2 = mod.$isRootOrShadowRoot;
var $isSiblingCaret2 = mod.$isSiblingCaret;
var $isTabNode2 = mod.$isTabNode;
var $isTextNode2 = mod.$isTextNode;
var $isTextPointCaret2 = mod.$isTextPointCaret;
var $isTextPointCaretSlice2 = mod.$isTextPointCaretSlice;
var $isTokenOrSegmented2 = mod.$isTokenOrSegmented;
var $isTokenOrTab2 = mod.$isTokenOrTab;
var $nodesOfType2 = mod.$nodesOfType;
var $normalizeCaret2 = mod.$normalizeCaret;
var $normalizeSelection__EXPERIMENTAL = mod.$normalizeSelection__EXPERIMENTAL;
var $onUpdate2 = mod.$onUpdate;
var $parseSerializedNode2 = mod.$parseSerializedNode;
var $removeTextFromCaretRange2 = mod.$removeTextFromCaretRange;
var $rewindSiblingCaret2 = mod.$rewindSiblingCaret;
var $selectAll2 = mod.$selectAll;
var $setCompositionKey2 = mod.$setCompositionKey;
var $setPointFromCaret2 = mod.$setPointFromCaret;
var $setSelection2 = mod.$setSelection;
var $setSelectionFromCaretRange2 = mod.$setSelectionFromCaretRange;
var $setState2 = mod.$setState;
var $splitAtPointCaretNext2 = mod.$splitAtPointCaretNext;
var $splitNode2 = mod.$splitNode;
var $updateRangeSelectionFromCaretRange2 = mod.$updateRangeSelectionFromCaretRange;
var ArtificialNode__DO_NOT_USE2 = mod.ArtificialNode__DO_NOT_USE;
var BEFORE_INPUT_COMMAND2 = mod.BEFORE_INPUT_COMMAND;
var BLUR_COMMAND2 = mod.BLUR_COMMAND;
var CAN_REDO_COMMAND2 = mod.CAN_REDO_COMMAND;
var CAN_UNDO_COMMAND2 = mod.CAN_UNDO_COMMAND;
var CLEAR_EDITOR_COMMAND2 = mod.CLEAR_EDITOR_COMMAND;
var CLEAR_HISTORY_COMMAND2 = mod.CLEAR_HISTORY_COMMAND;
var CLICK_COMMAND2 = mod.CLICK_COMMAND;
var COLLABORATION_TAG2 = mod.COLLABORATION_TAG;
var COMMAND_PRIORITY_BEFORE_CRITICAL2 = mod.COMMAND_PRIORITY_BEFORE_CRITICAL;
var COMMAND_PRIORITY_BEFORE_EDITOR2 = mod.COMMAND_PRIORITY_BEFORE_EDITOR;
var COMMAND_PRIORITY_BEFORE_HIGH2 = mod.COMMAND_PRIORITY_BEFORE_HIGH;
var COMMAND_PRIORITY_BEFORE_LOW2 = mod.COMMAND_PRIORITY_BEFORE_LOW;
var COMMAND_PRIORITY_BEFORE_NORMAL2 = mod.COMMAND_PRIORITY_BEFORE_NORMAL;
var COMMAND_PRIORITY_CRITICAL2 = mod.COMMAND_PRIORITY_CRITICAL;
var COMMAND_PRIORITY_EDITOR2 = mod.COMMAND_PRIORITY_EDITOR;
var COMMAND_PRIORITY_HIGH2 = mod.COMMAND_PRIORITY_HIGH;
var COMMAND_PRIORITY_LOW2 = mod.COMMAND_PRIORITY_LOW;
var COMMAND_PRIORITY_NORMAL2 = mod.COMMAND_PRIORITY_NORMAL;
var COMPOSITION_END_COMMAND2 = mod.COMPOSITION_END_COMMAND;
var COMPOSITION_END_TAG2 = mod.COMPOSITION_END_TAG;
var COMPOSITION_START_COMMAND2 = mod.COMPOSITION_START_COMMAND;
var COMPOSITION_START_TAG2 = mod.COMPOSITION_START_TAG;
var CONTROLLED_TEXT_INSERTION_COMMAND2 = mod.CONTROLLED_TEXT_INSERTION_COMMAND;
var COPY_COMMAND2 = mod.COPY_COMMAND;
var CUT_COMMAND2 = mod.CUT_COMMAND;
var DEFAULT_EDITOR_DOM_CONFIG2 = mod.DEFAULT_EDITOR_DOM_CONFIG;
var DELETE_CHARACTER_COMMAND2 = mod.DELETE_CHARACTER_COMMAND;
var DELETE_LINE_COMMAND2 = mod.DELETE_LINE_COMMAND;
var DELETE_WORD_COMMAND2 = mod.DELETE_WORD_COMMAND;
var DRAGEND_COMMAND2 = mod.DRAGEND_COMMAND;
var DRAGOVER_COMMAND2 = mod.DRAGOVER_COMMAND;
var DRAGSTART_COMMAND2 = mod.DRAGSTART_COMMAND;
var DROP_COMMAND2 = mod.DROP_COMMAND;
var DecoratorNode2 = mod.DecoratorNode;
var ElementNode2 = mod.ElementNode;
var FOCUS_COMMAND2 = mod.FOCUS_COMMAND;
var FORMAT_ELEMENT_COMMAND2 = mod.FORMAT_ELEMENT_COMMAND;
var FORMAT_TEXT_COMMAND2 = mod.FORMAT_TEXT_COMMAND;
var HISTORIC_TAG2 = mod.HISTORIC_TAG;
var HISTORY_MERGE_TAG2 = mod.HISTORY_MERGE_TAG;
var HISTORY_PUSH_TAG2 = mod.HISTORY_PUSH_TAG;
var INDENT_CONTENT_COMMAND2 = mod.INDENT_CONTENT_COMMAND;
var INPUT_COMMAND2 = mod.INPUT_COMMAND;
var INSERT_LINE_BREAK_COMMAND2 = mod.INSERT_LINE_BREAK_COMMAND;
var INSERT_PARAGRAPH_COMMAND2 = mod.INSERT_PARAGRAPH_COMMAND;
var INSERT_TAB_COMMAND2 = mod.INSERT_TAB_COMMAND;
var INTERNAL_$isBlock2 = mod.INTERNAL_$isBlock;
var IS_ALL_FORMATTING2 = mod.IS_ALL_FORMATTING;
var IS_BOLD2 = mod.IS_BOLD;
var IS_CODE2 = mod.IS_CODE;
var IS_HIGHLIGHT2 = mod.IS_HIGHLIGHT;
var IS_ITALIC2 = mod.IS_ITALIC;
var IS_STRIKETHROUGH2 = mod.IS_STRIKETHROUGH;
var IS_SUBSCRIPT2 = mod.IS_SUBSCRIPT;
var IS_SUPERSCRIPT2 = mod.IS_SUPERSCRIPT;
var IS_UNDERLINE2 = mod.IS_UNDERLINE;
var KEY_ARROW_DOWN_COMMAND2 = mod.KEY_ARROW_DOWN_COMMAND;
var KEY_ARROW_LEFT_COMMAND2 = mod.KEY_ARROW_LEFT_COMMAND;
var KEY_ARROW_RIGHT_COMMAND2 = mod.KEY_ARROW_RIGHT_COMMAND;
var KEY_ARROW_UP_COMMAND2 = mod.KEY_ARROW_UP_COMMAND;
var KEY_BACKSPACE_COMMAND2 = mod.KEY_BACKSPACE_COMMAND;
var KEY_DELETE_COMMAND2 = mod.KEY_DELETE_COMMAND;
var KEY_DOWN_COMMAND2 = mod.KEY_DOWN_COMMAND;
var KEY_ENTER_COMMAND2 = mod.KEY_ENTER_COMMAND;
var KEY_ESCAPE_COMMAND2 = mod.KEY_ESCAPE_COMMAND;
var KEY_MODIFIER_COMMAND2 = mod.KEY_MODIFIER_COMMAND;
var KEY_SPACE_COMMAND2 = mod.KEY_SPACE_COMMAND;
var KEY_TAB_COMMAND2 = mod.KEY_TAB_COMMAND;
var LineBreakNode2 = mod.LineBreakNode;
var MOVE_TO_END2 = mod.MOVE_TO_END;
var MOVE_TO_START2 = mod.MOVE_TO_START;
var NODE_STATE_KEY2 = mod.NODE_STATE_KEY;
var OUTDENT_CONTENT_COMMAND2 = mod.OUTDENT_CONTENT_COMMAND;
var PASTE_COMMAND2 = mod.PASTE_COMMAND;
var PASTE_TAG2 = mod.PASTE_TAG;
var ParagraphNode2 = mod.ParagraphNode;
var REDO_COMMAND2 = mod.REDO_COMMAND;
var REMOVE_TEXT_COMMAND2 = mod.REMOVE_TEXT_COMMAND;
var RootNode2 = mod.RootNode;
var SELECTION_CHANGE_COMMAND2 = mod.SELECTION_CHANGE_COMMAND;
var SELECTION_INSERT_CLIPBOARD_NODES_COMMAND2 = mod.SELECTION_INSERT_CLIPBOARD_NODES_COMMAND;
var SELECT_ALL_COMMAND2 = mod.SELECT_ALL_COMMAND;
var SKIP_COLLAB_TAG2 = mod.SKIP_COLLAB_TAG;
var SKIP_DOM_SELECTION_TAG2 = mod.SKIP_DOM_SELECTION_TAG;
var SKIP_SCROLL_INTO_VIEW_TAG2 = mod.SKIP_SCROLL_INTO_VIEW_TAG;
var SKIP_SELECTION_FOCUS_TAG2 = mod.SKIP_SELECTION_FOCUS_TAG;
var TEXT_TYPE_TO_FORMAT2 = mod.TEXT_TYPE_TO_FORMAT;
var TabNode2 = mod.TabNode;
var TextNode2 = mod.TextNode;
var UNDO_COMMAND2 = mod.UNDO_COMMAND;
var addClassNamesToElement2 = mod.addClassNamesToElement;
var buildImportMap2 = mod.buildImportMap;
var configExtension2 = mod.configExtension;
var createCommand2 = mod.createCommand;
var createEditor2 = mod.createEditor;
var createSharedNodeState2 = mod.createSharedNodeState;
var createState2 = mod.createState;
var declarePeerDependency2 = mod.declarePeerDependency;
var defineExtension2 = mod.defineExtension;
var flipDirection2 = mod.flipDirection;
var getDOMOwnerDocument2 = mod.getDOMOwnerDocument;
var getDOMSelection2 = mod.getDOMSelection;
var getDOMSelectionFromTarget2 = mod.getDOMSelectionFromTarget;
var getDOMTextNode2 = mod.getDOMTextNode;
var getEditorPropertyFromDOMNode2 = mod.getEditorPropertyFromDOMNode;
var getNearestEditorFromDOMNode2 = mod.getNearestEditorFromDOMNode;
var getRegisteredNode2 = mod.getRegisteredNode;
var getRegisteredNodeOrThrow2 = mod.getRegisteredNodeOrThrow;
var getStaticNodeConfig2 = mod.getStaticNodeConfig;
var getStyleObjectFromCSS2 = mod.getStyleObjectFromCSS;
var getTextDirection2 = mod.getTextDirection;
var getTransformSetFromKlass2 = mod.getTransformSetFromKlass;
var isBlockDomNode2 = mod.isBlockDomNode;
var isCurrentlyReadOnlyMode2 = mod.isCurrentlyReadOnlyMode;
var isDOMDocumentNode2 = mod.isDOMDocumentNode;
var isDOMNode2 = mod.isDOMNode;
var isDOMTextNode2 = mod.isDOMTextNode;
var isDOMUnmanaged2 = mod.isDOMUnmanaged;
var isDocumentFragment2 = mod.isDocumentFragment;
var isExactShortcutMatch2 = mod.isExactShortcutMatch;
var isHTMLAnchorElement2 = mod.isHTMLAnchorElement;
var isHTMLElement2 = mod.isHTMLElement;
var isInlineDomNode2 = mod.isInlineDomNode;
var isLexicalEditor2 = mod.isLexicalEditor;
var isModifierMatch2 = mod.isModifierMatch;
var isSelectionCapturedInDecoratorInput2 = mod.isSelectionCapturedInDecoratorInput;
var isSelectionWithinEditor2 = mod.isSelectionWithinEditor;
var makeStepwiseIterator2 = mod.makeStepwiseIterator;
var mergeRegister2 = mod.mergeRegister;
var normalizeClassNames2 = mod.normalizeClassNames;
var removeClassNamesFromElement2 = mod.removeClassNamesFromElement;
var removeFromParent2 = mod.removeFromParent;
var resetRandomKey2 = mod.resetRandomKey;
var safeCast2 = mod.safeCast;
var setDOMStyleFromCSS2 = mod.setDOMStyleFromCSS;
var setDOMStyleObject2 = mod.setDOMStyleObject;
var setDOMUnmanaged2 = mod.setDOMUnmanaged;
var setNodeIndentFromDOM2 = mod.setNodeIndentFromDOM;
var shallowMergeConfig2 = mod.shallowMergeConfig;
var toggleTextFormatType2 = mod.toggleTextFormatType;

// node_modules/@lexical/utils/LexicalUtils.dev.mjs
var LexicalUtils_dev_exports = {};
__export(LexicalUtils_dev_exports, {
  $descendantsMatching: () => $descendantsMatching,
  $dfs: () => $dfs,
  $dfsIterator: () => $dfsIterator,
  $filter: () => $filter,
  $findMatchingParent: () => $findMatchingParent2,
  $firstToLastIterator: () => $firstToLastIterator,
  $getAdjacentCaret: () => $getAdjacentCaret,
  $getAdjacentSiblingOrParentSiblingCaret: () => $getAdjacentSiblingOrParentSiblingCaret2,
  $getDepth: () => $getDepth,
  $getNearestBlockElementAncestorOrThrow: () => $getNearestBlockElementAncestorOrThrow,
  $getNearestNodeOfType: () => $getNearestNodeOfType,
  $getNextRightPreorderNode: () => $getNextRightPreorderNode,
  $getNextSiblingOrParentSibling: () => $getNextSiblingOrParentSibling,
  $handleIndentAndOutdent: () => $handleIndentAndOutdent,
  $insertFirst: () => $insertFirst,
  $insertNodeIntoLeaf: () => $insertNodeIntoLeaf,
  $insertNodeToNearestRoot: () => $insertNodeToNearestRoot,
  $insertNodeToNearestRootAtCaret: () => $insertNodeToNearestRootAtCaret,
  $isEditorIsNestedEditor: () => $isEditorIsNestedEditor,
  $lastToFirstIterator: () => $lastToFirstIterator,
  $restoreEditorState: () => $restoreEditorState,
  $reverseDfs: () => $reverseDfs,
  $reverseDfsIterator: () => $reverseDfsIterator,
  $splitNode: () => $splitNode2,
  $unwrapAndFilterDescendants: () => $unwrapAndFilterDescendants,
  $unwrapNode: () => $unwrapNode,
  $wrapNodeInElement: () => $wrapNodeInElement,
  CAN_USE_BEFORE_INPUT: () => CAN_USE_BEFORE_INPUT2,
  CAN_USE_DOM: () => CAN_USE_DOM2,
  IS_ANDROID: () => IS_ANDROID2,
  IS_ANDROID_CHROME: () => IS_ANDROID_CHROME2,
  IS_APPLE: () => IS_APPLE2,
  IS_APPLE_WEBKIT: () => IS_APPLE_WEBKIT2,
  IS_CHROME: () => IS_CHROME2,
  IS_FIREFOX: () => IS_FIREFOX2,
  IS_IOS: () => IS_IOS2,
  IS_SAFARI: () => IS_SAFARI2,
  addClassNamesToElement: () => addClassNamesToElement2,
  calculateZoomLevel: () => calculateZoomLevel,
  isBlockDomNode: () => isBlockDomNode2,
  isHTMLAnchorElement: () => isHTMLAnchorElement2,
  isHTMLElement: () => isHTMLElement2,
  isInlineDomNode: () => isInlineDomNode2,
  isMimeType: () => isMimeType,
  makeStateWrapper: () => makeStateWrapper,
  markSelection: () => markSelection,
  mediaFileReader: () => mediaFileReader,
  mergeRegister: () => mergeRegister2,
  objectKlassEquals: () => objectKlassEquals,
  positionNodeOnRange: () => mlcPositionNodeOnRange,
  registerNestedElementResolver: () => registerNestedElementResolver,
  removeClassNamesFromElement: () => removeClassNamesFromElement2,
  selectionAlwaysOnDisplay: () => selectionAlwaysOnDisplay
});

// node_modules/@lexical/selection/LexicalSelection.dev.mjs
var LexicalSelection_dev_exports = {};
__export(LexicalSelection_dev_exports, {
  $addNodeStyle: () => $addNodeStyle,
  $cloneWithProperties: () => $cloneWithProperties2,
  $copyBlockFormatIndent: () => $copyBlockFormatIndent,
  $ensureForwardRangeSelection: () => $ensureForwardRangeSelection,
  $forEachSelectedTextNode: () => $forEachSelectedTextNode,
  $getComputedStyleForElement: () => $getComputedStyleForElement,
  $getComputedStyleForParent: () => $getComputedStyleForParent,
  $getSelectionStyleValueForProperty: () => $getSelectionStyleValueForProperty,
  $isAtNodeEnd: () => $isAtNodeEnd,
  $isParentElementRTL: () => $isParentElementRTL,
  $isParentRTL: () => $isParentRTL,
  $moveCaretSelection: () => $moveCaretSelection,
  $moveCharacter: () => $moveCharacter,
  $patchStyleText: () => $patchStyleText,
  $selectAll: () => $selectAll2,
  $setBlocksType: () => $setBlocksType,
  $shouldOverrideDefaultCharacterSelection: () => $shouldOverrideDefaultCharacterSelection,
  $sliceSelectedTextNodeContent: () => $sliceSelectedTextNodeContent,
  $trimTextContentFromAnchor: () => $trimTextContentFromAnchor,
  $wrapNodes: () => $wrapNodes,
  createDOMRange: () => createDOMRange,
  createRectsFromDOMRange: () => createRectsFromDOMRange,
  getCSSFromStyleObject: () => getCSSFromStyleObject,
  getStyleObjectFromCSS: () => getStyleObjectFromCSS3,
  trimTextContentFromAnchor: () => trimTextContentFromAnchor
});
function formatDevErrorMessage2(message) {
  throw new Error(message);
}
function warnOnlyOnce2(message) {
  {
    let run = false;
    return () => {
      if (!run) {
        console.warn(message);
      }
      run = true;
    };
  }
}
function getDOMTextNode3(element) {
  let node = element;
  while (node != null) {
    if (node.nodeType === Node.TEXT_NODE) {
      return node;
    }
    node = node.firstChild;
  }
  return null;
}
function getDOMIndexWithinParent(node) {
  const parent = node.parentNode;
  if (parent == null) {
    throw new Error("Should never happen");
  }
  return [parent, Array.from(parent.childNodes).indexOf(node)];
}
function createDOMRange(editor, anchorNode, _anchorOffset, focusNode, _focusOffset) {
  const anchorKey = anchorNode.getKey();
  const focusKey = focusNode.getKey();
  const range = document.createRange();
  let anchorDOM = editor.getElementByKey(anchorKey);
  let focusDOM = editor.getElementByKey(focusKey);
  let anchorOffset = _anchorOffset;
  let focusOffset = _focusOffset;
  if ($isTextNode2(anchorNode)) {
    anchorDOM = getDOMTextNode3(anchorDOM);
  }
  if ($isTextNode2(focusNode)) {
    focusDOM = getDOMTextNode3(focusDOM);
  }
  if (anchorNode === void 0 || focusNode === void 0 || anchorDOM === null || focusDOM === null) {
    return null;
  }
  if (anchorDOM.nodeName === "BR") {
    [anchorDOM, anchorOffset] = getDOMIndexWithinParent(anchorDOM);
  }
  if (focusDOM.nodeName === "BR") {
    [focusDOM, focusOffset] = getDOMIndexWithinParent(focusDOM);
  }
  const firstChild = anchorDOM.firstChild;
  if (anchorDOM === focusDOM && firstChild != null && firstChild.nodeName === "BR" && anchorOffset === 0 && focusOffset === 0) {
    focusOffset = 1;
  }
  try {
    range.setStart(anchorDOM, anchorOffset);
    range.setEnd(focusDOM, focusOffset);
  } catch (_e) {
    return null;
  }
  if (range.collapsed && (anchorOffset !== focusOffset || anchorKey !== focusKey)) {
    range.setStart(focusDOM, focusOffset);
    range.setEnd(anchorDOM, anchorOffset);
  }
  return range;
}
function createRectsFromDOMRange(editor, range) {
  const rootElement = editor.getRootElement();
  if (rootElement === null) {
    return [];
  }
  const rootRect = rootElement.getBoundingClientRect();
  const computedStyle = getComputedStyle(rootElement);
  const rootPadding = parseFloat(computedStyle.paddingLeft) + parseFloat(computedStyle.paddingRight);
  const selectionRects = Array.from(range.getClientRects());
  let selectionRectsLength = selectionRects.length;
  selectionRects.sort((a2, b2) => {
    const top = a2.top - b2.top;
    if (Math.abs(top) <= 3) {
      return a2.left - b2.left;
    }
    return top;
  });
  let prevRect;
  for (let i2 = 0; i2 < selectionRectsLength; i2++) {
    const selectionRect = selectionRects[i2];
    const isOverlappingRect = prevRect && prevRect.top <= selectionRect.top && prevRect.top + prevRect.height > selectionRect.top && prevRect.left + prevRect.width > selectionRect.left;
    const selectionSpansElement = selectionRect.width + rootPadding === rootRect.width;
    if (isOverlappingRect || selectionSpansElement) {
      selectionRects.splice(i2--, 1);
      selectionRectsLength--;
      continue;
    }
    prevRect = selectionRect;
  }
  return selectionRects;
}
function getCSSFromStyleObject(styles) {
  let css = "";
  for (const style in styles) {
    if (style) {
      css += `${style}: ${styles[style]};`;
    }
  }
  return css;
}
function $getComputedStyleForElement(element) {
  const editor = $getEditor2();
  const domElement = editor.getElementByKey(element.getKey());
  if (domElement === null) {
    return null;
  }
  const view = domElement.ownerDocument.defaultView;
  if (view === null) {
    return null;
  }
  return view.getComputedStyle(domElement);
}
function $getComputedStyleForParent(node) {
  const parent = $isRootNode2(node) ? node : node.getParentOrThrow();
  return $getComputedStyleForElement(parent);
}
function $isParentRTL(node) {
  const styles = $getComputedStyleForParent(node);
  return styles !== null && styles.direction === "rtl";
}
function $sliceSelectedTextNodeContent(selection, textNode, mutates = "self") {
  const anchorAndFocus = selection.getStartEndPoints();
  if (textNode.isSelected(selection) && !$isTokenOrSegmented2(textNode) && anchorAndFocus !== null) {
    const [anchor, focus] = anchorAndFocus;
    const isBackward = selection.isBackward();
    const anchorNode = anchor.getNode();
    const focusNode = focus.getNode();
    const isAnchor = textNode.is(anchorNode);
    const isFocus = textNode.is(focusNode);
    if (isAnchor || isFocus) {
      const [anchorOffset, focusOffset] = $getCharacterOffsets2(selection);
      const isSame = anchorNode.is(focusNode);
      const isFirst = textNode.is(isBackward ? focusNode : anchorNode);
      const isLast = textNode.is(isBackward ? anchorNode : focusNode);
      let startOffset = 0;
      let endOffset = void 0;
      if (isSame) {
        startOffset = anchorOffset > focusOffset ? focusOffset : anchorOffset;
        endOffset = anchorOffset > focusOffset ? anchorOffset : focusOffset;
      } else if (isFirst) {
        const offset = isBackward ? focusOffset : anchorOffset;
        startOffset = offset;
        endOffset = void 0;
      } else if (isLast) {
        const offset = isBackward ? anchorOffset : focusOffset;
        startOffset = 0;
        endOffset = offset;
      }
      const text = textNode.__text.slice(startOffset, endOffset);
      if (text !== textNode.__text) {
        if (mutates === "clone") {
          textNode = $cloneWithPropertiesEphemeral2(textNode);
        }
        textNode.__text = text;
      }
    }
  }
  return textNode;
}
function $isAtNodeEnd(point) {
  if (point.type === "text") {
    return point.offset === point.getNode().getTextContentSize();
  }
  const node = point.getNode();
  if (!$isElementNode2(node)) {
    formatDevErrorMessage2(`isAtNodeEnd: node must be a TextNode or ElementNode`);
  }
  return point.offset === node.getChildrenSize();
}
function $trimTextContentFromAnchor(editor, anchor, delCount) {
  let currentNode = anchor.getNode();
  let remaining = delCount;
  if ($isElementNode2(currentNode)) {
    const descendantNode = currentNode.getDescendantByIndex(anchor.offset);
    if (descendantNode !== null) {
      currentNode = descendantNode;
    }
  }
  while (remaining > 0 && currentNode !== null) {
    if ($isElementNode2(currentNode)) {
      const lastDescendant = currentNode.getLastDescendant();
      if (lastDescendant !== null) {
        currentNode = lastDescendant;
      }
    }
    let nextNode = currentNode.getPreviousSibling();
    let additionalElementWhitespace = 0;
    if (nextNode === null) {
      let parent = currentNode.getParentOrThrow();
      let parentSibling = parent.getPreviousSibling();
      while (parentSibling === null) {
        parent = parent.getParent();
        if (parent === null) {
          nextNode = null;
          break;
        }
        parentSibling = parent.getPreviousSibling();
      }
      if (parent !== null) {
        additionalElementWhitespace = parent.isInline() ? 0 : 2;
        nextNode = parentSibling;
      }
    }
    let text = currentNode.getTextContent();
    if (text === "" && $isElementNode2(currentNode) && !currentNode.isInline()) {
      text = "\n\n";
    }
    const currentNodeSize = text.length;
    if (!$isTextNode2(currentNode) || remaining >= currentNodeSize) {
      const parent = currentNode.getParent();
      currentNode.remove();
      if (parent != null && parent.getChildrenSize() === 0 && !$isRootNode2(parent)) {
        parent.remove();
      }
      remaining -= currentNodeSize + additionalElementWhitespace;
      currentNode = nextNode;
    } else {
      const key = currentNode.getKey();
      const prevTextContent = editor.getEditorState().read(() => {
        const prevNode = $getNodeByKey2(key);
        if ($isTextNode2(prevNode) && prevNode.isSimpleText()) {
          return prevNode.getTextContent();
        }
        return null;
      });
      const offset = currentNodeSize - remaining;
      const slicedText = text.slice(0, offset);
      if (prevTextContent !== null && prevTextContent !== text) {
        const prevSelection = $getPreviousSelection2();
        let target = currentNode;
        if (!currentNode.isSimpleText()) {
          const textNode = $createTextNode2(prevTextContent);
          currentNode.replace(textNode);
          target = textNode;
        } else {
          currentNode.setTextContent(prevTextContent);
        }
        if ($isRangeSelection2(prevSelection) && prevSelection.isCollapsed()) {
          const prevOffset = prevSelection.anchor.offset;
          target.select(prevOffset, prevOffset);
        }
      } else if (currentNode.isSimpleText()) {
        const isSelected = anchor.key === key;
        let anchorOffset = anchor.offset;
        if (anchorOffset < remaining) {
          anchorOffset = currentNodeSize;
        }
        const splitStart = isSelected ? anchorOffset - remaining : 0;
        const splitEnd = isSelected ? anchorOffset : offset;
        if (isSelected && splitStart === 0) {
          const [excessNode] = currentNode.splitText(splitStart, splitEnd);
          excessNode.remove();
        } else {
          const [, excessNode] = currentNode.splitText(splitStart, splitEnd);
          excessNode.remove();
        }
      } else {
        const textNode = $createTextNode2(slicedText);
        currentNode.replace(textNode);
      }
      remaining = 0;
    }
  }
}
var $addNodeStyle = warnOnlyOnce2("$addNodeStyle is a deprecated no-op and calls should be removed");
function $patchStyle(target, patch) {
  if (!($isRangeSelection2(target) ? target.isCollapsed() : $isTextNode2(target) || $isElementNode2(target))) {
    formatDevErrorMessage2(`$patchStyle must only be called with a TextNode, ElementNode, or collapsed RangeSelection`);
  }
  const prevStyles = getStyleObjectFromCSS2($isRangeSelection2(target) ? target.style : $isTextNode2(target) ? target.getStyle() : target.getTextStyle());
  const newStyles = Object.entries(patch).reduce((styles, [key, value]) => {
    if (typeof value === "function") {
      styles[key] = value(prevStyles[key], target);
    } else if (value === null) {
      delete styles[key];
    } else {
      styles[key] = value;
    }
    return styles;
  }, {
    ...prevStyles
  });
  const newCSSText = getCSSFromStyleObject(newStyles);
  if ($isRangeSelection2(target) || $isTextNode2(target)) {
    target.setStyle(newCSSText);
  } else {
    target.setTextStyle(newCSSText);
  }
}
function $patchStyleText(selection, patch) {
  if ($isRangeSelection2(selection) && selection.isCollapsed()) {
    $patchStyle(selection, patch);
    const emptyNode = selection.anchor.getNode();
    if ($isElementNode2(emptyNode) && emptyNode.isEmpty()) {
      $patchStyle(emptyNode, patch);
    }
  }
  $forEachSelectedTextNode((textNode) => {
    $patchStyle(textNode, patch);
  });
  const nodes = selection.getNodes();
  if (nodes.length > 0) {
    const patchedElementKeys = /* @__PURE__ */ new Set();
    for (const node of nodes) {
      if (!$isElementNode2(node) || !node.canBeEmpty() || node.getChildrenSize() !== 0) {
        continue;
      }
      const key = node.getKey();
      if (patchedElementKeys.has(key)) {
        continue;
      }
      patchedElementKeys.add(key);
      $patchStyle(node, patch);
    }
  }
}
function $forEachSelectedTextNode(fn) {
  const selection = $getSelection2();
  if (!selection) {
    return;
  }
  const slicedTextNodes = /* @__PURE__ */ new Map();
  const getSliceIndices = (node) => slicedTextNodes.get(node.getKey()) || [0, node.getTextContentSize()];
  if ($isRangeSelection2(selection)) {
    for (const slice of $caretRangeFromSelection2(selection).getTextSlices()) {
      if (slice) {
        slicedTextNodes.set(slice.caret.origin.getKey(), slice.getSliceIndices());
      }
    }
  }
  const selectedNodes = selection.getNodes();
  for (const selectedNode of selectedNodes) {
    if (!($isTextNode2(selectedNode) && selectedNode.canHaveFormat())) {
      continue;
    }
    const [startOffset, endOffset] = getSliceIndices(selectedNode);
    if (endOffset === startOffset) {
      continue;
    }
    if ($isTokenOrSegmented2(selectedNode) || startOffset === 0 && endOffset === selectedNode.getTextContentSize()) {
      fn(selectedNode);
    } else {
      const splitNodes = selectedNode.splitText(startOffset, endOffset);
      const replacement = splitNodes[startOffset === 0 ? 0 : 1];
      fn(replacement);
    }
  }
  if ($isRangeSelection2(selection) && selection.anchor.type === "text" && selection.focus.type === "text" && selection.anchor.key === selection.focus.key) {
    $ensureForwardRangeSelection(selection);
  }
}
function $ensureForwardRangeSelection(selection) {
  if (selection.isBackward()) {
    const {
      anchor,
      focus
    } = selection;
    const {
      key,
      offset,
      type
    } = anchor;
    anchor.set(focus.key, focus.offset, focus.type);
    focus.set(key, offset, type);
  }
}
function $copyBlockFormatIndent(srcNode, destNode) {
  const format = srcNode.getFormatType();
  const indent = srcNode.getIndent();
  if (format !== destNode.getFormatType()) {
    destNode.setFormat(format);
  }
  if (indent !== destNode.getIndent()) {
    destNode.setIndent(indent);
  }
}
function $setBlocksType(selection, $createElement, $afterCreateElement = $copyBlockFormatIndent) {
  if (selection === null) {
    return;
  }
  const anchorAndFocus = selection.getStartEndPoints();
  const blockMap = /* @__PURE__ */ new Map();
  let newSelection = null;
  if (anchorAndFocus) {
    const [anchor, focus] = anchorAndFocus;
    newSelection = $createRangeSelection2();
    newSelection.anchor.set(anchor.key, anchor.offset, anchor.type);
    newSelection.focus.set(focus.key, focus.offset, focus.type);
    const anchorBlock = $findMatchingParent2(anchor.getNode(), INTERNAL_$isBlock2);
    const focusBlock = $findMatchingParent2(focus.getNode(), INTERNAL_$isBlock2);
    if ($isElementNode2(anchorBlock)) {
      blockMap.set(anchorBlock.getKey(), anchorBlock);
    }
    if ($isElementNode2(focusBlock)) {
      blockMap.set(focusBlock.getKey(), focusBlock);
    }
  }
  for (const node of selection.getNodes()) {
    if ($isElementNode2(node) && INTERNAL_$isBlock2(node)) {
      blockMap.set(node.getKey(), node);
    } else if (anchorAndFocus === null) {
      const ancestorBlock = $findMatchingParent2(node, INTERNAL_$isBlock2);
      if ($isElementNode2(ancestorBlock)) {
        blockMap.set(ancestorBlock.getKey(), ancestorBlock);
      }
    }
  }
  for (const [key, prevNode] of blockMap) {
    const element = $createElement();
    $afterCreateElement(prevNode, element);
    prevNode.replace(element, true);
    if (newSelection) {
      if (key === newSelection.anchor.key) {
        newSelection.anchor.set(element.getKey(), newSelection.anchor.offset, newSelection.anchor.type);
      }
      if (key === newSelection.focus.key) {
        newSelection.focus.set(element.getKey(), newSelection.focus.offset, newSelection.focus.type);
      }
    }
  }
  if (newSelection && selection.is($getSelection2())) {
    $setSelection2(newSelection);
  }
}
function isPointAttached(point) {
  return point.getNode().isAttached();
}
function $removeParentEmptyElements(startingNode) {
  let node = startingNode;
  while (node !== null && !$isRootOrShadowRoot2(node)) {
    const latest = node.getLatest();
    const parentNode = node.getParent();
    if (latest.getChildrenSize() === 0) {
      node.remove(true);
    }
    node = parentNode;
  }
}
function $wrapNodes(selection, createElement, wrappingElement = null) {
  const anchorAndFocus = selection.getStartEndPoints();
  const anchor = anchorAndFocus ? anchorAndFocus[0] : null;
  const nodes = selection.getNodes();
  const nodesLength = nodes.length;
  if (anchor !== null && (nodesLength === 0 || nodesLength === 1 && anchor.type === "element" && anchor.getNode().getChildrenSize() === 0)) {
    const target = anchor.type === "text" ? anchor.getNode().getParentOrThrow() : anchor.getNode();
    const children = target.getChildren();
    let element = createElement();
    element.setFormat(target.getFormatType());
    element.setIndent(target.getIndent());
    children.forEach((child) => element.append(child));
    if (wrappingElement) {
      element = wrappingElement.append(element);
    }
    target.replace(element);
    return;
  }
  let topLevelNode = null;
  let descendants = [];
  for (let i2 = 0; i2 < nodesLength; i2++) {
    const node = nodes[i2];
    if ($isRootOrShadowRoot2(node)) {
      $wrapNodesImpl(selection, descendants, descendants.length, createElement, wrappingElement);
      descendants = [];
      topLevelNode = node;
    } else if (topLevelNode === null || topLevelNode !== null && $hasAncestor2(node, topLevelNode)) {
      descendants.push(node);
    } else {
      $wrapNodesImpl(selection, descendants, descendants.length, createElement, wrappingElement);
      descendants = [node];
    }
  }
  $wrapNodesImpl(selection, descendants, descendants.length, createElement, wrappingElement);
}
function $wrapNodesImpl(selection, nodes, nodesLength, createElement, wrappingElement = null) {
  if (nodes.length === 0) {
    return;
  }
  const firstNode = nodes[0];
  const elementMapping = /* @__PURE__ */ new Map();
  const elements = [];
  let target = $isElementNode2(firstNode) ? firstNode : firstNode.getParentOrThrow();
  if (target.isInline()) {
    target = target.getParentOrThrow();
  }
  let targetIsPrevSibling = false;
  while (target !== null) {
    const prevSibling = target.getPreviousSibling();
    if (prevSibling !== null) {
      target = prevSibling;
      targetIsPrevSibling = true;
      break;
    }
    target = target.getParentOrThrow();
    if ($isRootOrShadowRoot2(target)) {
      break;
    }
  }
  const emptyElements = /* @__PURE__ */ new Set();
  for (let i2 = 0; i2 < nodesLength; i2++) {
    const node = nodes[i2];
    if ($isElementNode2(node) && node.getChildrenSize() === 0) {
      emptyElements.add(node.getKey());
    }
  }
  const movedNodes = /* @__PURE__ */ new Set();
  for (let i2 = 0; i2 < nodesLength; i2++) {
    const node = nodes[i2];
    let parent = node.getParent();
    if (parent !== null && parent.isInline()) {
      parent = parent.getParent();
    }
    if (parent !== null && $isLeafNode2(node) && !movedNodes.has(node.getKey())) {
      const parentKey = parent.getKey();
      if (elementMapping.get(parentKey) === void 0) {
        const targetElement = createElement();
        targetElement.setFormat(parent.getFormatType());
        targetElement.setIndent(parent.getIndent());
        elements.push(targetElement);
        elementMapping.set(parentKey, targetElement);
        parent.getChildren().forEach((child) => {
          targetElement.append(child);
          movedNodes.add(child.getKey());
          if ($isElementNode2(child)) {
            child.getChildrenKeys().forEach((key) => movedNodes.add(key));
          }
        });
        $removeParentEmptyElements(parent);
      }
    } else if (emptyElements.has(node.getKey())) {
      if (!$isElementNode2(node)) {
        formatDevErrorMessage2(`Expected node in emptyElements to be an ElementNode`);
      }
      const targetElement = createElement();
      targetElement.setFormat(node.getFormatType());
      targetElement.setIndent(node.getIndent());
      elements.push(targetElement);
      node.remove(true);
    }
  }
  if (wrappingElement !== null) {
    for (let i2 = 0; i2 < elements.length; i2++) {
      const element = elements[i2];
      wrappingElement.append(element);
    }
  }
  let lastElement = null;
  if ($isRootOrShadowRoot2(target)) {
    if (targetIsPrevSibling) {
      if (wrappingElement !== null) {
        target.insertAfter(wrappingElement);
      } else {
        for (let i2 = elements.length - 1; i2 >= 0; i2--) {
          const element = elements[i2];
          target.insertAfter(element);
        }
      }
    } else {
      const firstChild = target.getFirstChild();
      if ($isElementNode2(firstChild)) {
        target = firstChild;
      }
      if (firstChild === null) {
        if (wrappingElement) {
          target.append(wrappingElement);
        } else {
          for (let i2 = 0; i2 < elements.length; i2++) {
            const element = elements[i2];
            target.append(element);
            lastElement = element;
          }
        }
      } else {
        if (wrappingElement !== null) {
          firstChild.insertBefore(wrappingElement);
        } else {
          for (let i2 = 0; i2 < elements.length; i2++) {
            const element = elements[i2];
            firstChild.insertBefore(element);
            lastElement = element;
          }
        }
      }
    }
  } else {
    if (wrappingElement) {
      target.insertAfter(wrappingElement);
    } else {
      for (let i2 = elements.length - 1; i2 >= 0; i2--) {
        const element = elements[i2];
        target.insertAfter(element);
        lastElement = element;
      }
    }
  }
  const prevSelection = $getPreviousSelection2();
  if ($isRangeSelection2(prevSelection) && isPointAttached(prevSelection.anchor) && isPointAttached(prevSelection.focus)) {
    $setSelection2(prevSelection.clone());
  } else if (lastElement !== null) {
    lastElement.selectEnd();
  } else {
    selection.dirty = true;
  }
}
function $isEditorVerticalOrientation(selection) {
  const computedStyle = $getComputedStyle(selection);
  return computedStyle !== null && computedStyle.writingMode === "vertical-rl";
}
function $getComputedStyle(selection) {
  const anchorNode = selection.anchor.getNode();
  if ($isElementNode2(anchorNode)) {
    return $getComputedStyleForElement(anchorNode);
  }
  return $getComputedStyleForParent(anchorNode);
}
function $shouldOverrideDefaultCharacterSelection(selection, isBackward) {
  const isVertical = $isEditorVerticalOrientation(selection);
  let adjustedIsBackward = isVertical ? !isBackward : isBackward;
  if ($isParentElementRTL(selection)) {
    adjustedIsBackward = !adjustedIsBackward;
  }
  const focusCaret = $caretFromPoint2(selection.focus, adjustedIsBackward ? "previous" : "next");
  if ($isExtendableTextPointCaret2(focusCaret)) {
    return false;
  }
  for (const nextCaret of $extendCaretToRange2(focusCaret)) {
    if ($isChildCaret2(nextCaret)) {
      return !nextCaret.origin.isInline();
    } else if ($isElementNode2(nextCaret.origin)) {
      continue;
    } else if ($isDecoratorNode2(nextCaret.origin)) {
      return true;
    }
    break;
  }
  return false;
}
function $moveCaretSelection(selection, isHoldingShift, isBackward, granularity) {
  selection.modify(isHoldingShift ? "extend" : "move", isBackward, granularity);
}
function $isParentElementRTL(selection) {
  const computedStyle = $getComputedStyle(selection);
  return computedStyle !== null && computedStyle.direction === "rtl";
}
function $moveCharacter(selection, isHoldingShift, isBackward) {
  const isRTL = $isParentElementRTL(selection);
  const isVertical = $isEditorVerticalOrientation(selection);
  let adjustedIsBackward;
  if (isVertical) {
    adjustedIsBackward = !isBackward;
  } else if (isRTL) {
    adjustedIsBackward = !isBackward;
  } else {
    adjustedIsBackward = isBackward;
  }
  $moveCaretSelection(selection, isHoldingShift, adjustedIsBackward, "character");
}
function $getNodeStyleValueForProperty(node, styleProperty, defaultValue) {
  const css = node.getStyle();
  const styleObject = getStyleObjectFromCSS2(css);
  if (styleObject !== null) {
    return styleObject[styleProperty] || defaultValue;
  }
  return defaultValue;
}
function $getSelectionStyleValueForProperty(selection, styleProperty, defaultValue = "") {
  let styleValue = null;
  const nodes = selection.getNodes();
  const anchor = selection.anchor;
  const focus = selection.focus;
  const isBackward = selection.isBackward();
  const startNode = isBackward ? focus.getNode() : anchor.getNode();
  const endNode = isBackward ? anchor.getNode() : focus.getNode();
  const startOffset = isBackward ? focus.offset : anchor.offset;
  const endOffset = isBackward ? anchor.offset : focus.offset;
  if ($isRangeSelection2(selection) && selection.isCollapsed() && selection.style !== "") {
    const css = selection.style;
    const styleObject = getStyleObjectFromCSS2(css);
    if (styleObject !== null && styleProperty in styleObject) {
      return styleObject[styleProperty];
    }
  }
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    if (i2 === 0 && node.is(startNode) && $isTextNode2(node) && startOffset === node.getTextContentSize()) {
      continue;
    }
    if (i2 !== 0 && node.is(endNode) && endOffset === 0) {
      continue;
    }
    if ($isTextNode2(node)) {
      const nodeStyleValue = $getNodeStyleValueForProperty(node, styleProperty, defaultValue);
      if (styleValue === null) {
        styleValue = nodeStyleValue;
      } else if (styleValue !== nodeStyleValue) {
        styleValue = "";
        break;
      }
    }
  }
  return styleValue === null ? defaultValue : styleValue;
}
var getStyleObjectFromCSS3 = getStyleObjectFromCSS2;
var trimTextContentFromAnchor = $trimTextContentFromAnchor;

// node_modules/@lexical/selection/LexicalSelection.mjs
var mod2 = true ? LexicalSelection_dev_exports : LexicalSelection_prod_exports;
var $addNodeStyle2 = mod2.$addNodeStyle;
var $cloneWithProperties3 = mod2.$cloneWithProperties;
var $copyBlockFormatIndent2 = mod2.$copyBlockFormatIndent;
var $ensureForwardRangeSelection2 = mod2.$ensureForwardRangeSelection;
var $forEachSelectedTextNode2 = mod2.$forEachSelectedTextNode;
var $getComputedStyleForElement2 = mod2.$getComputedStyleForElement;
var $getComputedStyleForParent2 = mod2.$getComputedStyleForParent;
var $getSelectionStyleValueForProperty2 = mod2.$getSelectionStyleValueForProperty;
var $isAtNodeEnd2 = mod2.$isAtNodeEnd;
var $isParentElementRTL2 = mod2.$isParentElementRTL;
var $isParentRTL2 = mod2.$isParentRTL;
var $moveCaretSelection2 = mod2.$moveCaretSelection;
var $moveCharacter2 = mod2.$moveCharacter;
var $patchStyleText2 = mod2.$patchStyleText;
var $selectAll3 = mod2.$selectAll;
var $setBlocksType2 = mod2.$setBlocksType;
var $shouldOverrideDefaultCharacterSelection2 = mod2.$shouldOverrideDefaultCharacterSelection;
var $sliceSelectedTextNodeContent2 = mod2.$sliceSelectedTextNodeContent;
var $trimTextContentFromAnchor2 = mod2.$trimTextContentFromAnchor;
var $wrapNodes2 = mod2.$wrapNodes;
var createDOMRange2 = mod2.createDOMRange;
var createRectsFromDOMRange2 = mod2.createRectsFromDOMRange;
var getCSSFromStyleObject2 = mod2.getCSSFromStyleObject;
var getStyleObjectFromCSS4 = mod2.getStyleObjectFromCSS;
var trimTextContentFromAnchor2 = mod2.trimTextContentFromAnchor;

// node_modules/@lexical/utils/LexicalUtils.dev.mjs
function formatDevErrorMessage3(message) {
  throw new Error(message);
}
var CAN_USE_DOM$1 = typeof window !== "undefined" && typeof window.document !== "undefined" && typeof window.document.createElement !== "undefined";
var documentMode2 = CAN_USE_DOM$1 && "documentMode" in document ? document.documentMode : null;
var IS_APPLE$1 = CAN_USE_DOM$1 && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
var IS_FIREFOX$1 = CAN_USE_DOM$1 && /^(?!.*Seamonkey)(?=.*Firefox).*/i.test(navigator.userAgent);
var CAN_USE_BEFORE_INPUT$1 = CAN_USE_DOM$1 && "InputEvent" in window && !documentMode2 ? "getTargetRanges" in new window.InputEvent("input") : false;
var IS_IOS$1 = CAN_USE_DOM$1 && /iPad|iPhone|iPod/.test(navigator.userAgent) && !window.MSStream;
var IS_ANDROID$1 = CAN_USE_DOM$1 && /Android/.test(navigator.userAgent);
var IS_SAFARI$1 = CAN_USE_DOM$1 && /Version\/[\d.]+.*Safari/.test(navigator.userAgent) && !IS_ANDROID$1;
var IS_CHROME$1 = CAN_USE_DOM$1 && /^(?=.*Chrome).*/i.test(navigator.userAgent);
var IS_ANDROID_CHROME$1 = CAN_USE_DOM$1 && IS_ANDROID$1 && IS_CHROME$1;
var IS_APPLE_WEBKIT$1 = CAN_USE_DOM$1 && /AppleWebKit\/[\d.]+/.test(navigator.userAgent) && IS_APPLE$1 && !IS_CHROME$1;
function px(value) {
  return `${value}px`;
}
var mutationObserverConfig = {
  attributes: true,
  characterData: true,
  childList: true,
  subtree: true
};
function prependDOMNode(parent, node) {
  parent.insertBefore(node, parent.firstChild);
}
function mlcPositionNodeOnRange(editor, range, onReposition) {
  let rootDOMNode = null;
  let parentDOMNode = null;
  let observer = null;
  let lastNodes = [];
  const wrapperNode = document.createElement("div");
  wrapperNode.style.position = "relative";
  function position() {
    if (!(rootDOMNode !== null)) {
      formatDevErrorMessage3(`Unexpected null rootDOMNode`);
    }
    if (!(parentDOMNode !== null)) {
      formatDevErrorMessage3(`Unexpected null parentDOMNode`);
    }
    const {
      left: parentLeft,
      top: parentTop
    } = parentDOMNode.getBoundingClientRect();
    const rects = createRectsFromDOMRange2(editor, range);
    if (!wrapperNode.isConnected) {
      prependDOMNode(parentDOMNode, wrapperNode);
    }
    let hasRepositioned = false;
    for (let i2 = 0; i2 < rects.length; i2++) {
      const rect = rects[i2];
      const rectNode = lastNodes[i2] || document.createElement("div");
      const rectNodeStyle = rectNode.style;
      if (rectNodeStyle.position !== "absolute") {
        rectNodeStyle.position = "absolute";
        hasRepositioned = true;
      }
      const left = px(rect.left - parentLeft);
      if (rectNodeStyle.left !== left) {
        rectNodeStyle.left = left;
        hasRepositioned = true;
      }
      const top = px(rect.top - parentTop);
      if (rectNodeStyle.top !== top) {
        rectNode.style.top = top;
        hasRepositioned = true;
      }
      const width = px(rect.width);
      if (rectNodeStyle.width !== width) {
        rectNode.style.width = width;
        hasRepositioned = true;
      }
      const height = px(rect.height);
      if (rectNodeStyle.height !== height) {
        rectNode.style.height = height;
        hasRepositioned = true;
      }
      if (rectNode.parentNode !== wrapperNode) {
        wrapperNode.append(rectNode);
        hasRepositioned = true;
      }
      lastNodes[i2] = rectNode;
    }
    while (lastNodes.length > rects.length) {
      lastNodes.pop();
    }
    if (hasRepositioned) {
      onReposition(lastNodes);
    }
  }
  function stop() {
    parentDOMNode = null;
    rootDOMNode = null;
    if (observer !== null) {
      observer.disconnect();
    }
    observer = null;
    wrapperNode.remove();
    for (const node of lastNodes) {
      node.remove();
    }
    lastNodes = [];
  }
  function restart() {
    const currentRootDOMNode = editor.getRootElement();
    if (currentRootDOMNode === null) {
      return stop();
    }
    const currentParentDOMNode = currentRootDOMNode.parentElement;
    if (!isHTMLElement2(currentParentDOMNode)) {
      return stop();
    }
    stop();
    rootDOMNode = currentRootDOMNode;
    parentDOMNode = currentParentDOMNode;
    observer = new MutationObserver((mutations) => {
      const nextRootDOMNode = editor.getRootElement();
      const nextParentDOMNode = nextRootDOMNode && nextRootDOMNode.parentElement;
      if (nextRootDOMNode !== rootDOMNode || nextParentDOMNode !== parentDOMNode) {
        return restart();
      }
      for (const mutation of mutations) {
        if (!wrapperNode.contains(mutation.target)) {
          return position();
        }
      }
    });
    observer.observe(currentParentDOMNode, mutationObserverConfig);
    position();
  }
  const removeRootListener = editor.registerRootListener(restart);
  return () => {
    removeRootListener();
    stop();
  };
}
function $getOrderedSelectionPoints(selection) {
  const points = selection.getStartEndPoints();
  return selection.isBackward() ? [points[1], points[0]] : points;
}
function $rangeTargetFromPoint(point, node, dom) {
  if (point.type === "text" || !$isElementNode2(node)) {
    const textDOM = getDOMTextNode2(dom) || dom;
    return [textDOM, point.offset];
  } else {
    const editor = $getEditor2();
    const slot = $getEditorDOMRenderConfig2(editor).$getDOMSlot(node, dom, editor);
    return [slot.element, slot.getFirstChildOffset() + point.offset];
  }
}
function $rangeFromPoints(editor, start, startNode, startDOM, end, endNode, endDOM) {
  const editorDocument = editor._window ? editor._window.document : document;
  const range = editorDocument.createRange();
  range.setStart(...$rangeTargetFromPoint(start, startNode, startDOM));
  range.setEnd(...$rangeTargetFromPoint(end, endNode, endDOM));
  return range;
}
function defaultOnReposition(domNodes) {
  for (const domNode of domNodes) {
    const domNodeStyle = domNode.style;
    if (domNodeStyle.background !== "Highlight") {
      domNodeStyle.background = "Highlight";
    }
    if (domNodeStyle.color !== "HighlightText") {
      domNodeStyle.color = "HighlightText";
    }
    if (domNodeStyle.marginTop !== px(-1.5)) {
      domNodeStyle.marginTop = px(-1.5);
    }
    if (domNodeStyle.paddingTop !== px(4)) {
      domNodeStyle.paddingTop = px(4);
    }
    if (domNodeStyle.paddingBottom !== px(0)) {
      domNodeStyle.paddingBottom = px(0);
    }
  }
}
function markSelection(editor, onReposition = defaultOnReposition) {
  let previousAnchorNode = null;
  let previousAnchorNodeDOM = null;
  let previousAnchorOffset = null;
  let previousFocusNode = null;
  let previousFocusNodeDOM = null;
  let previousFocusOffset = null;
  let removeRangeListener = () => {
  };
  function compute(editorState) {
    editorState.read(() => {
      const selection = $getSelection2();
      if (!$isRangeSelection2(selection)) {
        previousAnchorNode = null;
        previousAnchorOffset = null;
        previousFocusNode = null;
        previousFocusOffset = null;
        removeRangeListener();
        removeRangeListener = () => {
        };
        return;
      }
      const [start, end] = $getOrderedSelectionPoints(selection);
      const currentStartNode = start.getNode();
      const currentStartNodeKey = currentStartNode.getKey();
      const currentStartOffset = start.offset;
      const currentEndNode = end.getNode();
      const currentEndNodeKey = currentEndNode.getKey();
      const currentEndOffset = end.offset;
      const currentStartNodeDOM = editor.getElementByKey(currentStartNodeKey);
      const currentEndNodeDOM = editor.getElementByKey(currentEndNodeKey);
      const differentStartDOM = previousAnchorNode === null || currentStartNodeDOM !== previousAnchorNodeDOM || currentStartOffset !== previousAnchorOffset || currentStartNodeKey !== previousAnchorNode.getKey();
      const differentEndDOM = previousFocusNode === null || currentEndNodeDOM !== previousFocusNodeDOM || currentEndOffset !== previousFocusOffset || currentEndNodeKey !== previousFocusNode.getKey();
      if ((differentStartDOM || differentEndDOM) && currentStartNodeDOM !== null && currentEndNodeDOM !== null) {
        const range = $rangeFromPoints(editor, start, currentStartNode, currentStartNodeDOM, end, currentEndNode, currentEndNodeDOM);
        removeRangeListener();
        removeRangeListener = mlcPositionNodeOnRange(editor, range, onReposition);
      }
      previousAnchorNode = currentStartNode;
      previousAnchorNodeDOM = currentStartNodeDOM;
      previousAnchorOffset = currentStartOffset;
      previousFocusNode = currentEndNode;
      previousFocusNodeDOM = currentEndNodeDOM;
      previousFocusOffset = currentEndOffset;
    });
  }
  compute(editor.getEditorState());
  return mergeRegister2(editor.registerUpdateListener(({
    editorState
  }) => compute(editorState)), () => {
    removeRangeListener();
  });
}
function selectionAlwaysOnDisplay(editor, onReposition) {
  let removeSelectionMark = null;
  const onSelectionChange2 = () => {
    const domSelection = getSelection();
    const domAnchorNode = domSelection && domSelection.anchorNode;
    const editorRootElement = editor.getRootElement();
    const isSelectionInsideEditor = domAnchorNode !== null && editorRootElement !== null && editorRootElement.contains(domAnchorNode);
    if (isSelectionInsideEditor) {
      if (removeSelectionMark !== null) {
        removeSelectionMark();
        removeSelectionMark = null;
      }
    } else {
      if (removeSelectionMark === null) {
        removeSelectionMark = markSelection(editor, onReposition);
      }
    }
  };
  return editor.registerRootListener((rootElement) => {
    if (rootElement) {
      const document2 = rootElement.ownerDocument;
      document2.addEventListener("selectionchange", onSelectionChange2);
      onSelectionChange2();
      return () => {
        if (removeSelectionMark !== null) {
          removeSelectionMark();
        }
        document2.removeEventListener("selectionchange", onSelectionChange2);
      };
    }
  });
}
var CAN_USE_BEFORE_INPUT2 = CAN_USE_BEFORE_INPUT$1;
var CAN_USE_DOM2 = CAN_USE_DOM$1;
var IS_ANDROID2 = IS_ANDROID$1;
var IS_ANDROID_CHROME2 = IS_ANDROID_CHROME$1;
var IS_APPLE2 = IS_APPLE$1;
var IS_APPLE_WEBKIT2 = IS_APPLE_WEBKIT$1;
var IS_CHROME2 = IS_CHROME$1;
var IS_FIREFOX2 = IS_FIREFOX$1;
var IS_IOS2 = IS_IOS$1;
var IS_SAFARI2 = IS_SAFARI$1;
function isMimeType(file, acceptableMimeTypes) {
  for (const acceptableType of acceptableMimeTypes) {
    if (file.type.startsWith(acceptableType)) {
      return true;
    }
  }
  return false;
}
function mediaFileReader(files, acceptableMimeTypes) {
  const filesIterator = files[Symbol.iterator]();
  return new Promise((resolve, reject) => {
    const processed = [];
    const handleNextFile = () => {
      const {
        done,
        value: file
      } = filesIterator.next();
      if (done) {
        return resolve(processed);
      }
      const fileReader = new FileReader();
      fileReader.addEventListener("error", reject);
      fileReader.addEventListener("load", () => {
        const result = fileReader.result;
        if (typeof result === "string") {
          processed.push({
            file,
            result
          });
        }
        handleNextFile();
      });
      if (isMimeType(file, acceptableMimeTypes)) {
        fileReader.readAsDataURL(file);
      } else {
        handleNextFile();
      }
    };
    handleNextFile();
  });
}
function $dfs(startNode, endNode) {
  return Array.from($dfsIterator(startNode, endNode));
}
function $getAdjacentCaret(caret) {
  return caret ? caret.getAdjacentCaret() : null;
}
function $reverseDfs(startNode, endNode) {
  return Array.from($reverseDfsIterator(startNode, endNode));
}
function $dfsIterator(startNode, endNode) {
  return $dfsCaretIterator("next", startNode, endNode);
}
function $getEndCaret(startNode, direction) {
  const rval = $getAdjacentSiblingOrParentSiblingCaret2($getSiblingCaret2(startNode, direction));
  return rval && rval[0];
}
function $dfsCaretIterator(direction, startNode, endNode) {
  const root = $getRoot2();
  const start = startNode || root;
  const startCaret = $isElementNode2(start) ? $getChildCaret2(start, direction) : $getSiblingCaret2(start, direction);
  const startDepth = $getDepth(start);
  const endCaret = endNode ? $getAdjacentChildCaret2($getChildCaretOrSelf2($getSiblingCaret2(endNode, direction))) || $getEndCaret(endNode, direction) : $getEndCaret(start, direction);
  let depth = startDepth;
  return makeStepwiseIterator2({
    hasNext: (state) => state !== null,
    initial: startCaret,
    map: (state) => ({
      depth,
      node: state.origin
    }),
    step: (state) => {
      if (state.isSameNodeCaret(endCaret)) {
        return null;
      }
      if ($isChildCaret2(state)) {
        depth++;
      }
      const rval = $getAdjacentSiblingOrParentSiblingCaret2(state);
      if (!rval || rval[0].isSameNodeCaret(endCaret)) {
        return null;
      }
      depth += rval[1];
      return rval[0];
    }
  });
}
function $getNextSiblingOrParentSibling(node) {
  const rval = $getAdjacentSiblingOrParentSiblingCaret2($getSiblingCaret2(node, "next"));
  return rval && [rval[0].origin, rval[1]];
}
function $getDepth(node) {
  let depth = -1;
  for (let innerNode = node; innerNode !== null; innerNode = innerNode.getParent()) {
    depth++;
  }
  return depth;
}
function $getNextRightPreorderNode(startingNode) {
  const startCaret = $getChildCaretOrSelf2($getSiblingCaret2(startingNode, "previous"));
  const next = $getAdjacentSiblingOrParentSiblingCaret2(startCaret, "root");
  return next && next[0].origin;
}
function $reverseDfsIterator(startNode, endNode) {
  return $dfsCaretIterator("previous", startNode, endNode);
}
function $getNearestNodeOfType(node, klass) {
  let parent = node;
  while (parent != null) {
    if (parent instanceof klass) {
      return parent;
    }
    parent = parent.getParent();
  }
  return null;
}
function $getNearestBlockElementAncestorOrThrow(startNode) {
  const blockNode = $findMatchingParent2(startNode, (node) => $isElementNode2(node) && !node.isInline());
  if (!$isElementNode2(blockNode)) {
    {
      formatDevErrorMessage3(`Expected node ${startNode.__key} to have closest block element node.`);
    }
  }
  return blockNode;
}
function registerNestedElementResolver(editor, targetNode, cloneNode, handleOverlap) {
  const $isTargetNode = (node) => {
    return node instanceof targetNode;
  };
  const $findMatch = (node) => {
    const children = node.getChildren();
    for (let i2 = 0; i2 < children.length; i2++) {
      const child = children[i2];
      if ($isTargetNode(child)) {
        return null;
      }
    }
    let parentNode = node;
    let childNode = node;
    while (parentNode !== null) {
      childNode = parentNode;
      parentNode = parentNode.getParent();
      if ($isTargetNode(parentNode)) {
        return {
          child: childNode,
          parent: parentNode
        };
      }
    }
    return null;
  };
  const $elementNodeTransform = (node) => {
    const match = $findMatch(node);
    if (match !== null) {
      const {
        child,
        parent
      } = match;
      if (child.is(node)) {
        handleOverlap(parent, node);
        const nextSiblings = child.getNextSiblings();
        const nextSiblingsLength = nextSiblings.length;
        parent.insertAfter(child);
        if (nextSiblingsLength !== 0) {
          const newParent = cloneNode(parent);
          child.insertAfter(newParent);
          for (let i2 = 0; i2 < nextSiblingsLength; i2++) {
            newParent.append(nextSiblings[i2]);
          }
        }
        if (!parent.canBeEmpty() && parent.getChildrenSize() === 0) {
          parent.remove();
        }
      }
    }
  };
  return editor.registerNodeTransform(targetNode, $elementNodeTransform);
}
function $restoreEditorState(editor, editorState) {
  const FULL_RECONCILE2 = 2;
  const nodeMap = /* @__PURE__ */ new Map();
  const activeEditorState2 = editor._pendingEditorState;
  for (const [key, node] of editorState._nodeMap) {
    nodeMap.set(key, $cloneWithProperties2(node));
  }
  if (activeEditorState2) {
    activeEditorState2._nodeMap = nodeMap;
  }
  editor._dirtyType = FULL_RECONCILE2;
  const selection = editorState._selection;
  $setSelection2(selection === null ? null : selection.clone());
}
function $insertNodeToNearestRoot(node) {
  const selection = $getSelection2() || $getPreviousSelection2();
  let initialCaret;
  if ($isRangeSelection2(selection)) {
    initialCaret = $caretFromPoint2(selection.focus, "next");
  } else {
    if (selection != null) {
      const nodes = selection.getNodes();
      const lastNode = nodes[nodes.length - 1];
      if (lastNode) {
        initialCaret = $getSiblingCaret2(lastNode, "next");
      }
    }
    initialCaret = initialCaret || $getChildCaret2($getRoot2(), "previous").getFlipped().insert($createParagraphNode2());
  }
  const insertCaret = $insertNodeToNearestRootAtCaret(node, initialCaret);
  const adjacent = $getAdjacentChildCaret2(insertCaret);
  const selectionCaret = $isChildCaret2(adjacent) ? $normalizeCaret2(adjacent) : insertCaret;
  $setSelectionFromCaretRange2($getCollapsedCaretRange2(selectionCaret));
  return node.getLatest();
}
function $insertNodeToNearestRootAtCaret(node, caret, options) {
  let insertCaret = $getCaretInDirection2(caret, "next");
  if ($isTextPointCaret2(insertCaret)) {
    if (insertCaret.offset === 0) {
      insertCaret = $getSiblingCaret2(insertCaret.origin, "previous").getFlipped();
    } else if (insertCaret.offset === insertCaret.origin.getTextContentSize()) {
      insertCaret = $getSiblingCaret2(insertCaret.origin, "next");
    }
  }
  if (insertCaret.origin.is(node)) {
    if (!$isSiblingCaret2(insertCaret)) {
      formatDevErrorMessage3(`$insertNodeToNearestRootAtCaret node ${node.getKey()} of type ${node.getType()} can not be inserted into itself`);
    }
    insertCaret = $rewindSiblingCaret2(insertCaret);
  }
  if (node.is(insertCaret.getNodeAtCaret()) || node.is(insertCaret.getFlipped().getNodeAtCaret())) {
    node.remove(true);
  }
  for (let nextCaret = insertCaret; nextCaret; nextCaret = $splitAtPointCaretNext2(nextCaret, options)) {
    insertCaret = nextCaret;
  }
  if (!!$isTextPointCaret2(insertCaret)) {
    formatDevErrorMessage3(`$insertNodeToNearestRootAtCaret: An unattached TextNode can not be split`);
  }
  insertCaret.insert(node.isInline() ? $createParagraphNode2().append(node) : node);
  return $getCaretInDirection2($getSiblingCaret2(node.getLatest(), "next"), caret.direction);
}
function $insertNodeIntoLeaf(node) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection)) {
    if (selection) {
      selection.insertNodes([node]);
    }
    return;
  }
  const caretRange = $caretRangeFromSelection2(selection);
  let insertCaret = $getCaretRangeInDirection2($removeTextFromCaretRange2(caretRange), "next").anchor;
  if ($isTextPointCaret2(insertCaret)) {
    const nextAnchor = $splitAtPointCaretNext2(insertCaret);
    if (!nextAnchor) {
      return;
    }
    insertCaret = nextAnchor;
  }
  const focus = insertCaret.getFlipped();
  focus.insert(node);
  $setSelectionFromCaretRange2($getCaretRange2(focus, focus));
}
function $wrapNodeInElement(node, createElementNode) {
  const elementNode = createElementNode();
  node.replace(elementNode);
  elementNode.append(node);
  return elementNode;
}
function objectKlassEquals(object, objectClass) {
  return object !== null ? Object.getPrototypeOf(object).constructor.name === objectClass.name : false;
}
function $filter(nodes, filterFn) {
  const result = [];
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = filterFn(nodes[i2]);
    if (node !== null) {
      result.push(node);
    }
  }
  return result;
}
function $handleIndentAndOutdent(indentOrOutdent) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection)) {
    return false;
  }
  const alreadyHandled = /* @__PURE__ */ new Set();
  const nodes = selection.getNodes();
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    const key = node.getKey();
    if (alreadyHandled.has(key)) {
      continue;
    }
    const parentBlock = $findMatchingParent2(node, (parentNode) => $isElementNode2(parentNode) && !parentNode.isInline());
    if (parentBlock === null) {
      continue;
    }
    const parentKey = parentBlock.getKey();
    if (parentBlock.canIndent() && !alreadyHandled.has(parentKey)) {
      alreadyHandled.add(parentKey);
      indentOrOutdent(parentBlock);
    }
  }
  return alreadyHandled.size > 0;
}
function $insertFirst(parent, node) {
  $getChildCaret2(parent, "next").insert(node);
}
var NEEDS_MANUAL_ZOOM = IS_FIREFOX2 || !CAN_USE_DOM2 ? false : void 0;
function needsManualZoom() {
  if (NEEDS_MANUAL_ZOOM === void 0) {
    const div = document.createElement("div");
    div.style.position = "absolute";
    div.style.opacity = "0";
    div.style.width = "100px";
    div.style.left = "-1000px";
    document.body.appendChild(div);
    const noZoom = div.getBoundingClientRect();
    div.style.setProperty("zoom", "2");
    NEEDS_MANUAL_ZOOM = div.getBoundingClientRect().width === noZoom.width;
    document.body.removeChild(div);
  }
  return NEEDS_MANUAL_ZOOM;
}
function calculateZoomLevel(element, useManualZoom = false) {
  let zoom = 1;
  if (needsManualZoom() || useManualZoom) {
    while (element) {
      zoom *= Number(window.getComputedStyle(element).getPropertyValue("zoom"));
      element = element.parentElement;
    }
  }
  return zoom;
}
function $isEditorIsNestedEditor(editor) {
  return editor._parentEditor !== null;
}
function $unwrapAndFilterDescendants(root, $predicate) {
  return $unwrapAndFilterDescendantsImpl(root, $predicate, null);
}
function $unwrapAndFilterDescendantsImpl(root, $predicate, $onSuccess) {
  let didMutate = false;
  for (const node of $lastToFirstIterator(root)) {
    if ($predicate(node)) {
      if ($onSuccess !== null) {
        $onSuccess(node);
      }
      continue;
    }
    didMutate = true;
    if ($isElementNode2(node)) {
      $unwrapAndFilterDescendantsImpl(node, $predicate, $onSuccess || ((child) => node.insertAfter(child)));
    }
    node.remove();
  }
  return didMutate;
}
function $descendantsMatching(children, $predicate) {
  const result = [];
  const stack = Array.from(children).reverse();
  for (let child = stack.pop(); child !== void 0; child = stack.pop()) {
    if ($predicate(child)) {
      result.push(child);
    } else if ($isElementNode2(child)) {
      for (const grandchild of $lastToFirstIterator(child)) {
        stack.push(grandchild);
      }
    }
  }
  return result;
}
function $firstToLastIterator(node) {
  return $childIterator($getChildCaret2(node, "next"));
}
function $lastToFirstIterator(node) {
  return $childIterator($getChildCaret2(node, "previous"));
}
function $childIterator(startCaret) {
  const seen = /* @__PURE__ */ new Set();
  return makeStepwiseIterator2({
    hasNext: $isSiblingCaret2,
    initial: startCaret.getAdjacentCaret(),
    map: (caret) => {
      const origin = caret.origin.getLatest();
      if (seen !== null) {
        const key = origin.getKey();
        if (!!seen.has(key)) {
          formatDevErrorMessage3(`$childIterator: Cycle detected, node with key ${String(key)} has already been traversed`);
        }
        seen.add(key);
      }
      return origin;
    },
    step: (caret) => caret.getAdjacentCaret()
  });
}
function $unwrapNode(node) {
  $rewindSiblingCaret2($getSiblingCaret2(node, "next")).splice(1, node.getChildren());
}
function makeStateWrapper(stateConfig) {
  const $get = (node) => $getState2(node, stateConfig);
  const $set = (node, valueOrUpdater) => $setState2(node, stateConfig, valueOrUpdater);
  return {
    $get,
    $set,
    accessors: [$get, $set],
    makeGetterMethod: () => function $getter() {
      return $get(this);
    },
    makeSetterMethod: () => function $setter(valueOrUpdater) {
      return $set(this, valueOrUpdater);
    },
    stateConfig
  };
}

// node_modules/@lexical/utils/LexicalUtils.mjs
var mod3 = true ? LexicalUtils_dev_exports : LexicalUtils_prod_exports;
var $descendantsMatching2 = mod3.$descendantsMatching;
var $dfs2 = mod3.$dfs;
var $dfsIterator2 = mod3.$dfsIterator;
var $filter2 = mod3.$filter;
var $findMatchingParent3 = mod3.$findMatchingParent;
var $firstToLastIterator2 = mod3.$firstToLastIterator;
var $getAdjacentCaret2 = mod3.$getAdjacentCaret;
var $getAdjacentSiblingOrParentSiblingCaret3 = mod3.$getAdjacentSiblingOrParentSiblingCaret;
var $getDepth2 = mod3.$getDepth;
var $getNearestBlockElementAncestorOrThrow2 = mod3.$getNearestBlockElementAncestorOrThrow;
var $getNearestNodeOfType2 = mod3.$getNearestNodeOfType;
var $getNextRightPreorderNode2 = mod3.$getNextRightPreorderNode;
var $getNextSiblingOrParentSibling2 = mod3.$getNextSiblingOrParentSibling;
var $handleIndentAndOutdent2 = mod3.$handleIndentAndOutdent;
var $insertFirst2 = mod3.$insertFirst;
var $insertNodeIntoLeaf2 = mod3.$insertNodeIntoLeaf;
var $insertNodeToNearestRoot2 = mod3.$insertNodeToNearestRoot;
var $insertNodeToNearestRootAtCaret2 = mod3.$insertNodeToNearestRootAtCaret;
var $isEditorIsNestedEditor2 = mod3.$isEditorIsNestedEditor;
var $lastToFirstIterator2 = mod3.$lastToFirstIterator;
var $restoreEditorState2 = mod3.$restoreEditorState;
var $reverseDfs2 = mod3.$reverseDfs;
var $reverseDfsIterator2 = mod3.$reverseDfsIterator;
var $splitNode3 = mod3.$splitNode;
var $unwrapAndFilterDescendants2 = mod3.$unwrapAndFilterDescendants;
var $unwrapNode2 = mod3.$unwrapNode;
var $wrapNodeInElement2 = mod3.$wrapNodeInElement;
var CAN_USE_BEFORE_INPUT3 = mod3.CAN_USE_BEFORE_INPUT;
var CAN_USE_DOM3 = mod3.CAN_USE_DOM;
var IS_ANDROID3 = mod3.IS_ANDROID;
var IS_ANDROID_CHROME3 = mod3.IS_ANDROID_CHROME;
var IS_APPLE3 = mod3.IS_APPLE;
var IS_APPLE_WEBKIT3 = mod3.IS_APPLE_WEBKIT;
var IS_CHROME3 = mod3.IS_CHROME;
var IS_FIREFOX3 = mod3.IS_FIREFOX;
var IS_IOS3 = mod3.IS_IOS;
var IS_SAFARI3 = mod3.IS_SAFARI;
var addClassNamesToElement3 = mod3.addClassNamesToElement;
var calculateZoomLevel2 = mod3.calculateZoomLevel;
var isBlockDomNode3 = mod3.isBlockDomNode;
var isHTMLAnchorElement3 = mod3.isHTMLAnchorElement;
var isHTMLElement3 = mod3.isHTMLElement;
var isInlineDomNode3 = mod3.isInlineDomNode;
var isMimeType2 = mod3.isMimeType;
var makeStateWrapper2 = mod3.makeStateWrapper;
var markSelection2 = mod3.markSelection;
var mediaFileReader2 = mod3.mediaFileReader;
var mergeRegister3 = mod3.mergeRegister;
var objectKlassEquals2 = mod3.objectKlassEquals;
var positionNodeOnRange = mod3.positionNodeOnRange;
var registerNestedElementResolver2 = mod3.registerNestedElementResolver;
var removeClassNamesFromElement3 = mod3.removeClassNamesFromElement;
var selectionAlwaysOnDisplay2 = mod3.selectionAlwaysOnDisplay;

// node_modules/@lexical/rich-text/LexicalRichText.dev.mjs
var LexicalRichText_dev_exports = {};
__export(LexicalRichText_dev_exports, {
  $createHeadingNode: () => $createHeadingNode,
  $createQuoteNode: () => $createQuoteNode,
  $isHeadingNode: () => $isHeadingNode,
  $isQuoteNode: () => $isQuoteNode,
  DRAG_DROP_PASTE: () => DRAG_DROP_PASTE,
  HeadingNode: () => HeadingNode,
  QuoteNode: () => QuoteNode,
  RichTextExtension: () => RichTextExtension,
  eventFiles: () => eventFiles,
  registerRichText: () => registerRichText
});

// node_modules/@lexical/clipboard/LexicalClipboard.dev.mjs
var LexicalClipboard_dev_exports = {};
__export(LexicalClipboard_dev_exports, {
  $generateJSONFromSelectedNodes: () => $generateJSONFromSelectedNodes,
  $generateNodesFromSerializedNodes: () => $generateNodesFromSerializedNodes,
  $getClipboardDataFromSelection: () => $getClipboardDataFromSelection,
  $getHtmlContent: () => $getHtmlContent,
  $getLexicalContent: () => $getLexicalContent,
  $handlePlainTextDrop: () => $handlePlainTextDrop,
  $handleRichTextDrop: () => $handleRichTextDrop,
  $insertDataTransferForPlainText: () => $insertDataTransferForPlainText,
  $insertDataTransferForRichText: () => $insertDataTransferForRichText,
  $insertGeneratedNodes: () => $insertGeneratedNodes,
  $writeDragSourceToDataTransfer: () => $writeDragSourceToDataTransfer,
  copyToClipboard: () => copyToClipboard,
  setLexicalClipboardDataTransfer: () => setLexicalClipboardDataTransfer
});

// node_modules/@lexical/extension/LexicalExtension.dev.mjs
var LexicalExtension_dev_exports = {};
__export(LexicalExtension_dev_exports, {
  $createHorizontalRuleNode: () => $createHorizontalRuleNode,
  $isDecoratorTextNode: () => $isDecoratorTextNode,
  $isHorizontalRuleNode: () => $isHorizontalRuleNode,
  AutoFocusExtension: () => AutoFocusExtension,
  ClearEditorExtension: () => ClearEditorExtension,
  DecoratorTextExtension: () => DecoratorTextExtension,
  DecoratorTextNode: () => DecoratorTextNode,
  EditorStateExtension: () => EditorStateExtension,
  HorizontalRuleExtension: () => HorizontalRuleExtension,
  HorizontalRuleNode: () => HorizontalRuleNode,
  INSERT_HORIZONTAL_RULE_COMMAND: () => INSERT_HORIZONTAL_RULE_COMMAND,
  InitialStateExtension: () => InitialStateExtension,
  LexicalBuilder: () => LexicalBuilder,
  NestedEditorExtension: () => NestedEditorExtension,
  NodeSelectionExtension: () => NodeSelectionExtension,
  SelectionAlwaysOnDisplayExtension: () => SelectionAlwaysOnDisplayExtension,
  TabIndentationExtension: () => TabIndentationExtension,
  applyFormatFromStyle: () => applyFormatFromStyle,
  applyFormatToDom: () => applyFormatToDom,
  batch: () => n,
  buildEditorFromExtensions: () => buildEditorFromExtensions,
  computed: () => g,
  configExtension: () => configExtension2,
  declarePeerDependency: () => declarePeerDependency2,
  defineExtension: () => defineExtension2,
  effect: () => j,
  getExtensionDependencyFromEditor: () => getExtensionDependencyFromEditor,
  getKnownTypesAndNodes: () => getKnownTypesAndNodes,
  getPeerDependencyFromEditor: () => getPeerDependencyFromEditor,
  getPeerDependencyFromEditorOrThrow: () => getPeerDependencyFromEditorOrThrow,
  namedSignals: () => namedSignals,
  registerClearEditor: () => registerClearEditor,
  registerTabIndentation: () => registerTabIndentation,
  safeCast: () => safeCast2,
  shallowMergeConfig: () => shallowMergeConfig2,
  signal: () => a,
  untracked: () => h,
  watchedSignal: () => watchedSignal
});
var i = /* @__PURE__ */ Symbol.for("preact-signals");
function t() {
  if (e > 1) {
    e--;
    return;
  }
  let i2, t2 = false;
  !(function() {
    let i3 = r;
    r = void 0;
    while (void 0 !== i3) {
      if (i3.S.v === i3.v) i3.S.i = i3.i;
      i3 = i3.o;
    }
  })();
  while (void 0 !== s) {
    let n2 = s;
    s = void 0;
    u++;
    while (void 0 !== n2) {
      const o2 = n2.u;
      n2.u = void 0;
      n2.f &= -3;
      if (!(8 & n2.f) && w(n2)) try {
        n2.c();
      } catch (n3) {
        if (!t2) {
          i2 = n3;
          t2 = true;
        }
      }
      n2 = o2;
    }
  }
  u = 0;
  e--;
  if (t2) throw i2;
}
function n(i2) {
  if (e > 0) return i2();
  d = ++c;
  e++;
  try {
    return i2();
  } finally {
    t();
  }
}
var o;
var s;
function h(i2) {
  const t2 = o;
  o = void 0;
  try {
    return i2();
  } finally {
    o = t2;
  }
}
var r;
var e = 0;
var u = 0;
var c = 0;
var d = 0;
var v = 0;
function l(i2) {
  if (void 0 === o) return;
  let t2 = i2.n;
  if (void 0 === t2 || t2.t !== o) {
    t2 = { i: 0, S: i2, p: o.s, n: void 0, t: o, e: void 0, x: void 0, r: t2 };
    if (void 0 !== o.s) o.s.n = t2;
    o.s = t2;
    i2.n = t2;
    if (32 & o.f) i2.S(t2);
    return t2;
  } else if (-1 === t2.i) {
    t2.i = 0;
    if (void 0 !== t2.n) {
      t2.n.p = t2.p;
      if (void 0 !== t2.p) t2.p.n = t2.n;
      t2.p = o.s;
      t2.n = void 0;
      o.s.n = t2;
      o.s = t2;
    }
    return t2;
  }
}
function y(i2, t2) {
  this.v = i2;
  this.i = 0;
  this.n = void 0;
  this.t = void 0;
  this.l = 0;
  this.W = null == t2 ? void 0 : t2.watched;
  this.Z = null == t2 ? void 0 : t2.unwatched;
  this.name = null == t2 ? void 0 : t2.name;
}
y.prototype.brand = i;
y.prototype.h = function() {
  return true;
};
y.prototype.S = function(i2) {
  const t2 = this.t;
  if (t2 !== i2 && void 0 === i2.e) {
    i2.x = t2;
    this.t = i2;
    if (void 0 !== t2) t2.e = i2;
    else h(() => {
      var i3;
      null == (i3 = this.W) || i3.call(this);
    });
  }
};
y.prototype.U = function(i2) {
  if (void 0 !== this.t) {
    const t2 = i2.e, n2 = i2.x;
    if (void 0 !== t2) {
      t2.x = n2;
      i2.e = void 0;
    }
    if (void 0 !== n2) {
      n2.e = t2;
      i2.x = void 0;
    }
    if (i2 === this.t) {
      this.t = n2;
      if (void 0 === n2) h(() => {
        var i3;
        null == (i3 = this.Z) || i3.call(this);
      });
    }
  }
};
y.prototype.subscribe = function(i2) {
  return j(() => {
    const t2 = this.value, n2 = o;
    o = void 0;
    try {
      i2(t2);
    } finally {
      o = n2;
    }
  }, { name: "sub" });
};
y.prototype.valueOf = function() {
  return this.value;
};
y.prototype.toString = function() {
  return this.value + "";
};
y.prototype.toJSON = function() {
  return this.value;
};
y.prototype.peek = function() {
  const i2 = o;
  o = void 0;
  try {
    return this.value;
  } finally {
    o = i2;
  }
};
Object.defineProperty(y.prototype, "value", { get() {
  const i2 = l(this);
  if (void 0 !== i2) i2.i = this.i;
  return this.v;
}, set(i2) {
  if (i2 !== this.v) {
    if (u > 100) throw new Error("Cycle detected");
    !(function(i3) {
      if (0 !== e && 0 === u) {
        if (i3.l !== d) {
          i3.l = d;
          r = { S: i3, v: i3.v, i: i3.i, o: r };
        }
      }
    })(this);
    this.v = i2;
    this.i++;
    v++;
    e++;
    try {
      for (let i3 = this.t; void 0 !== i3; i3 = i3.x) i3.t.N();
    } finally {
      t();
    }
  }
} });
function a(i2, t2) {
  return new y(i2, t2);
}
function w(i2) {
  for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) if (t2.S.i !== t2.i || !t2.S.h() || t2.S.i !== t2.i) return true;
  return false;
}
function _(i2) {
  for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) {
    const n2 = t2.S.n;
    if (void 0 !== n2) t2.r = n2;
    t2.S.n = t2;
    t2.i = -1;
    if (void 0 === t2.n) {
      i2.s = t2;
      break;
    }
  }
}
function b(i2) {
  let t2, n2 = i2.s;
  while (void 0 !== n2) {
    const i3 = n2.p;
    if (-1 === n2.i) {
      n2.S.U(n2);
      if (void 0 !== i3) i3.n = n2.n;
      if (void 0 !== n2.n) n2.n.p = i3;
    } else t2 = n2;
    n2.S.n = n2.r;
    if (void 0 !== n2.r) n2.r = void 0;
    n2 = i3;
  }
  i2.s = t2;
}
function p(i2, t2) {
  y.call(this, void 0);
  this.x = i2;
  this.s = void 0;
  this.g = v - 1;
  this.f = 4;
  this.W = null == t2 ? void 0 : t2.watched;
  this.Z = null == t2 ? void 0 : t2.unwatched;
  this.name = null == t2 ? void 0 : t2.name;
}
p.prototype = new y();
p.prototype.h = function() {
  this.f &= -3;
  if (1 & this.f) return false;
  if (32 == (36 & this.f)) return true;
  this.f &= -5;
  if (this.g === v) return true;
  this.g = v;
  this.f |= 1;
  if (this.i > 0 && !w(this)) {
    this.f &= -2;
    return true;
  }
  const i2 = o;
  try {
    _(this);
    o = this;
    const i3 = this.x();
    if (16 & this.f || this.v !== i3 || 0 === this.i) {
      this.v = i3;
      this.f &= -17;
      this.i++;
    }
  } catch (i3) {
    this.v = i3;
    this.f |= 16;
    this.i++;
  }
  o = i2;
  b(this);
  this.f &= -2;
  return true;
};
p.prototype.S = function(i2) {
  if (void 0 === this.t) {
    this.f |= 36;
    for (let i3 = this.s; void 0 !== i3; i3 = i3.n) i3.S.S(i3);
  }
  y.prototype.S.call(this, i2);
};
p.prototype.U = function(i2) {
  if (void 0 !== this.t) {
    y.prototype.U.call(this, i2);
    if (void 0 === this.t) {
      this.f &= -33;
      for (let i3 = this.s; void 0 !== i3; i3 = i3.n) i3.S.U(i3);
    }
  }
};
p.prototype.N = function() {
  if (!(2 & this.f)) {
    this.f |= 6;
    for (let i2 = this.t; void 0 !== i2; i2 = i2.x) i2.t.N();
  }
};
Object.defineProperty(p.prototype, "value", { get() {
  if (1 & this.f) throw new Error("Cycle detected");
  const i2 = l(this);
  this.h();
  if (void 0 !== i2) i2.i = this.i;
  if (16 & this.f) throw this.v;
  return this.v;
} });
function g(i2, t2) {
  return new p(i2, t2);
}
function S(i2) {
  const n2 = i2.m;
  i2.m = void 0;
  if ("function" == typeof n2) {
    e++;
    const s2 = o;
    o = void 0;
    try {
      n2();
    } catch (t2) {
      i2.f &= -2;
      i2.f |= 8;
      m(i2);
      throw t2;
    } finally {
      o = s2;
      t();
    }
  }
}
function m(i2) {
  for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) t2.S.U(t2);
  i2.x = void 0;
  i2.s = void 0;
  S(i2);
}
function x(i2) {
  if (o !== this) throw new Error("Out-of-order effect");
  b(this);
  o = i2;
  this.f &= -2;
  if (8 & this.f) m(this);
  t();
}
function E(i2, t2) {
  this.x = i2;
  this.m = void 0;
  this.s = void 0;
  this.u = void 0;
  this.f = 32;
  this.name = null == t2 ? void 0 : t2.name;
}
E.prototype.c = function() {
  const i2 = this.S();
  try {
    if (8 & this.f) return;
    if (void 0 === this.x) return;
    const t2 = this.x();
    if ("function" == typeof t2) this.m = t2;
  } finally {
    i2();
  }
};
E.prototype.S = function() {
  if (1 & this.f) throw new Error("Cycle detected");
  this.f |= 1;
  this.f &= -9;
  S(this);
  _(this);
  e++;
  const i2 = o;
  o = this;
  return x.bind(this, i2);
};
E.prototype.N = function() {
  if (!(2 & this.f)) {
    this.f |= 2;
    this.u = s;
    s = this;
  }
};
E.prototype.d = function() {
  this.f |= 8;
  if (!(1 & this.f)) m(this);
};
E.prototype.dispose = function() {
  this.d();
};
function j(i2, t2) {
  const n2 = new E(i2, t2);
  try {
    n2.c();
  } catch (i3) {
    n2.d();
    throw i3;
  }
  const o2 = n2.d.bind(n2);
  o2[Symbol.dispose] = o2;
  return o2;
}
function namedSignals(defaults, opts = {}) {
  const initial = {};
  for (const k in defaults) {
    const v2 = opts[k];
    const store = a(v2 === void 0 ? defaults[k] : v2);
    initial[k] = store;
  }
  return initial;
}
var AutoFocusExtension = defineExtension2({
  build: (editor, config, state) => {
    return namedSignals(config);
  },
  config: safeCast2({
    defaultSelection: "rootEnd",
    disabled: false
  }),
  name: "@lexical/extension/AutoFocus",
  register(editor, config, state) {
    const stores = state.getOutput();
    return j(() => stores.disabled.value ? void 0 : editor.registerRootListener((rootElement) => {
      editor.focus(() => {
        const activeElement = document.activeElement;
        if (rootElement !== null && (activeElement === null || !rootElement.contains(activeElement))) {
          rootElement.focus({
            preventScroll: true
          });
        }
      }, {
        defaultSelection: stores.defaultSelection.peek()
      });
    }));
  }
});
function $defaultOnClear() {
  const root = $getRoot2();
  const selection = $getSelection2();
  const paragraph = $createParagraphNode2();
  root.clear();
  root.append(paragraph);
  if (selection !== null) {
    paragraph.select();
  }
  if ($isRangeSelection2(selection)) {
    selection.format = 0;
  }
}
function registerClearEditor(editor, $onClear = $defaultOnClear) {
  return editor.registerCommand(CLEAR_EDITOR_COMMAND2, (payload) => {
    editor.update($onClear);
    return true;
  }, COMMAND_PRIORITY_EDITOR2);
}
var ClearEditorExtension = defineExtension2({
  build(editor, config, state) {
    return namedSignals(config);
  },
  config: safeCast2({
    $onClear: $defaultOnClear
  }),
  name: "@lexical/extension/ClearEditor",
  register(editor, config, state) {
    const {
      $onClear
    } = state.getOutput();
    return j(() => registerClearEditor(editor, $onClear.value));
  }
});
function getKnownTypesAndNodes(config) {
  const types = /* @__PURE__ */ new Set();
  const nodes = /* @__PURE__ */ new Set();
  for (const klassOrReplacement of getNodeConfig(config)) {
    const klass = typeof klassOrReplacement === "function" ? klassOrReplacement : klassOrReplacement.replace;
    void getStaticNodeConfig2(klass);
    types.add(klass.getType());
    nodes.add(klass);
  }
  return {
    nodes,
    types
  };
}
function getNodeConfig(config) {
  return (typeof config.nodes === "function" ? config.nodes() : config.nodes) || [];
}
var formatState = createState2("format", {
  parse: (value) => typeof value === "number" ? value : 0
});
var DecoratorTextNode = class extends DecoratorNode2 {
  $config() {
    return this.config("decorator-text", {
      extends: DecoratorNode2,
      stateConfigs: [{
        flat: true,
        stateConfig: formatState
      }]
    });
  }
  getFormat() {
    return $getState2(this, formatState);
  }
  getFormatFlags(type, alignWithFormat) {
    return toggleTextFormatType2(this.getFormat(), type, alignWithFormat);
  }
  hasFormat(type) {
    const formatFlag = TEXT_TYPE_TO_FORMAT2[type];
    return (this.getFormat() & formatFlag) !== 0;
  }
  setFormat(type) {
    return $setState2(this, formatState, type);
  }
  toggleFormat(type) {
    const format = this.getFormat();
    const newFormat = toggleTextFormatType2(format, type, null);
    return this.setFormat(newFormat);
  }
  isInline() {
    return true;
  }
  createDOM() {
    return document.createElement("span");
  }
  updateDOM() {
    return false;
  }
};
function $isDecoratorTextNode(node) {
  return node instanceof DecoratorTextNode;
}
function applyFormatFromStyle(lexicalNode, style, shouldApply) {
  const fontWeight = style.fontWeight;
  const textDecoration = style.textDecoration.split(" ");
  const hasBoldFontWeight = fontWeight === "700" || fontWeight === "bold";
  const hasLinethroughTextDecoration = textDecoration.includes("line-through");
  const hasItalicFontStyle = style.fontStyle === "italic";
  const hasUnderlineTextDecoration = textDecoration.includes("underline");
  const verticalAlign = style.verticalAlign;
  if (hasBoldFontWeight && !lexicalNode.hasFormat("bold")) {
    lexicalNode.toggleFormat("bold");
  }
  if (hasLinethroughTextDecoration && !lexicalNode.hasFormat("strikethrough")) {
    lexicalNode.toggleFormat("strikethrough");
  }
  if (hasItalicFontStyle && !lexicalNode.hasFormat("italic")) {
    lexicalNode.toggleFormat("italic");
  }
  if (hasUnderlineTextDecoration && !lexicalNode.hasFormat("underline")) {
    lexicalNode.toggleFormat("underline");
  }
  if (verticalAlign === "sub" && !lexicalNode.hasFormat("subscript")) {
    lexicalNode.toggleFormat("subscript");
  }
  if (verticalAlign === "super" && !lexicalNode.hasFormat("superscript")) {
    lexicalNode.toggleFormat("superscript");
  }
  if (shouldApply && !lexicalNode.hasFormat(shouldApply)) {
    lexicalNode.toggleFormat(shouldApply);
  }
  return lexicalNode;
}
function applyFormatToDom(lexicalNode, domNode, tagNameToFormat = DEFAULT_TAG_NAME_TO_FORMAT) {
  for (const [tag, format] of Object.entries(tagNameToFormat)) {
    if (lexicalNode.hasFormat(format)) {
      domNode = wrapElementWith2(domNode, tag);
    }
  }
  return domNode;
}
function wrapElementWith2(element, tag) {
  const el = document.createElement(tag);
  el.appendChild(element);
  return el;
}
var DEFAULT_TAG_NAME_TO_FORMAT = {
  b: "bold",
  code: "code",
  em: "italic",
  i: "italic",
  mark: "highlight",
  s: "strikethrough",
  strong: "bold",
  sub: "subscript",
  sup: "superscript",
  u: "underline"
};
var DecoratorTextExtension = defineExtension2({
  name: "@lexical/extension/DecoratorText",
  nodes: () => [DecoratorTextNode],
  register(editor, config, state) {
    return editor.registerCommand(FORMAT_TEXT_COMMAND2, (formatType) => {
      const selection = $getSelection2();
      if ($isNodeSelection2(selection) || $isRangeSelection2(selection)) {
        for (const node of selection.getNodes()) {
          if ($isDecoratorTextNode(node)) {
            node.toggleFormat(formatType);
          }
        }
      }
      return false;
    }, COMMAND_PRIORITY_LOW2);
  }
});
function watchedSignal(getSnapshot, register) {
  let dispose;
  return a(getSnapshot(), {
    unwatched() {
      if (dispose) {
        dispose();
        dispose = void 0;
      }
    },
    watched() {
      this.value = getSnapshot();
      dispose = register(this);
    }
  });
}
var EditorStateExtension = defineExtension2({
  build(editor) {
    return watchedSignal(() => editor.getEditorState(), (editorStateSignal) => editor.registerUpdateListener((payload) => {
      editorStateSignal.value = payload.editorState;
    }));
  },
  name: "@lexical/extension/EditorState"
});
function formatDevErrorMessage4(message) {
  throw new Error(message);
}
function deepThemeMergeInPlace(a2, b2) {
  if (a2 && b2 && !Array.isArray(b2) && typeof a2 === "object" && typeof b2 === "object") {
    const aObj = a2;
    const bObj = b2;
    for (const k in bObj) {
      aObj[k] = deepThemeMergeInPlace(aObj[k], bObj[k]);
    }
    return a2;
  }
  return b2;
}
var ExtensionRepStateIds = {
  /* eslint-disable sort-keys-fix/sort-keys-fix */
  unmarked: 0,
  temporary: 1,
  permanent: 2,
  configured: 3,
  initialized: 4,
  built: 5,
  registered: 6,
  afterRegistration: 7
  /* eslint-enable sort-keys-fix/sort-keys-fix */
};
function isExactlyUnmarkedExtensionRepState(state) {
  return state.id === ExtensionRepStateIds.unmarked;
}
function isExactlyTemporaryExtensionRepState(state) {
  return state.id === ExtensionRepStateIds.temporary;
}
function isExactlyPermanentExtensionRepState(state) {
  return state.id === ExtensionRepStateIds.permanent;
}
function isConfiguredExtensionRepState(state) {
  return state.id >= ExtensionRepStateIds.configured;
}
function isInitializedExtensionRepState(state) {
  return state.id >= ExtensionRepStateIds.initialized;
}
function isBuiltExtensionRepState(state) {
  return state.id >= ExtensionRepStateIds.built;
}
function isAfterRegistrationState(state) {
  return state.id >= ExtensionRepStateIds.afterRegistration;
}
function applyTemporaryMark(state) {
  if (!isExactlyUnmarkedExtensionRepState(state)) {
    formatDevErrorMessage4(`LexicalBuilder: Can not apply a temporary mark from state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.unmarked)} unmarked)`);
  }
  return Object.assign(state, {
    id: ExtensionRepStateIds.temporary
  });
}
function applyPermanentMark(state) {
  if (!isExactlyTemporaryExtensionRepState(state)) {
    formatDevErrorMessage4(`LexicalBuilder: Can not apply a permanent mark from state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.temporary)} temporary)`);
  }
  return Object.assign(state, {
    id: ExtensionRepStateIds.permanent
  });
}
function applyConfiguredState(state, config, registerState) {
  return Object.assign(state, {
    config,
    id: ExtensionRepStateIds.configured,
    registerState
  });
}
function applyInitializedState(state, initResult, registerState) {
  return Object.assign(state, {
    id: ExtensionRepStateIds.initialized,
    initResult,
    registerState
  });
}
function applyBuiltState(state, output, registerState) {
  return Object.assign(state, {
    id: ExtensionRepStateIds.built,
    output,
    registerState
  });
}
function applyRegisteredState(state) {
  return Object.assign(state, {
    id: ExtensionRepStateIds.registered
  });
}
function applyAfterRegistrationState(state) {
  return Object.assign(state, {
    id: ExtensionRepStateIds.afterRegistration
  });
}
function rollbackToBuiltState(state) {
  return Object.assign(state, {
    id: ExtensionRepStateIds.built
  });
}
var emptySet = /* @__PURE__ */ new Set();
var ExtensionRep = class {
  builder;
  configs;
  _dependency;
  _peerNameSet;
  extension;
  state;
  _signal;
  constructor(builder, extension) {
    this.builder = builder;
    this.extension = extension;
    this.configs = /* @__PURE__ */ new Set();
    this.state = {
      id: ExtensionRepStateIds.unmarked
    };
  }
  mergeConfigs() {
    let config = this.extension.config || {};
    const mergeConfig = this.extension.mergeConfig ? this.extension.mergeConfig.bind(this.extension) : shallowMergeConfig2;
    for (const cfg of this.configs) {
      config = mergeConfig(config, cfg);
    }
    return config;
  }
  init(editorConfig) {
    const initialState = this.state;
    if (!isExactlyPermanentExtensionRepState(initialState)) {
      formatDevErrorMessage4(`ExtensionRep: Can not configure from state id ${String(initialState.id)}`);
    }
    const initState = {
      getDependency: this.getInitDependency.bind(this),
      getDirectDependentNames: this.getDirectDependentNames.bind(this),
      getPeer: this.getInitPeer.bind(this),
      getPeerNameSet: this.getPeerNameSet.bind(this)
    };
    const buildState = {
      ...initState,
      getDependency: this.getDependency.bind(this),
      getInitResult: this.getInitResult.bind(this),
      getPeer: this.getPeer.bind(this)
    };
    const state = applyConfiguredState(initialState, this.mergeConfigs(), initState);
    this.state = state;
    let initResult;
    if (this.extension.init) {
      initResult = this.extension.init(editorConfig, state.config, initState);
    }
    this.state = applyInitializedState(state, initResult, buildState);
  }
  build(editor) {
    const state = this.state;
    if (!(state.id === ExtensionRepStateIds.initialized)) {
      formatDevErrorMessage4(`ExtensionRep: register called in state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.built)} initialized)`);
    }
    let output;
    if (this.extension.build) {
      output = this.extension.build(editor, state.config, state.registerState);
    }
    const registerState = {
      ...state.registerState,
      getOutput: () => output,
      getSignal: this.getSignal.bind(this)
    };
    this.state = applyBuiltState(state, output, registerState);
  }
  register(editor, signal2) {
    this._signal = signal2;
    const state = this.state;
    if (!(state.id === ExtensionRepStateIds.built)) {
      formatDevErrorMessage4(`ExtensionRep: register called in state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.built)} built)`);
    }
    const cleanup = this.extension.register && this.extension.register(editor, state.config, state.registerState);
    this.state = applyRegisteredState(state);
    return () => {
      const afterRegistrationState = this.state;
      if (!(afterRegistrationState.id === ExtensionRepStateIds.afterRegistration)) {
        formatDevErrorMessage4(`ExtensionRep: rollbackToBuiltState called in state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.afterRegistration)} afterRegistration)`);
      }
      this.state = rollbackToBuiltState(afterRegistrationState);
      if (cleanup) {
        cleanup();
      }
    };
  }
  afterRegistration(editor) {
    const state = this.state;
    if (!(state.id === ExtensionRepStateIds.registered)) {
      formatDevErrorMessage4(`ExtensionRep: afterRegistration called in state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.registered)} registered)`);
    }
    let rval;
    if (this.extension.afterRegistration) {
      rval = this.extension.afterRegistration(editor, state.config, state.registerState);
    }
    this.state = applyAfterRegistrationState(state);
    return rval;
  }
  getSignal() {
    if (!(this._signal !== void 0)) {
      formatDevErrorMessage4(`ExtensionRep.getSignal() called before register`);
    }
    return this._signal;
  }
  getInitResult() {
    if (!(this.extension.init !== void 0)) {
      formatDevErrorMessage4(`ExtensionRep: getInitResult() called for Extension ${this.extension.name} that does not define init`);
    }
    const state = this.state;
    if (!isInitializedExtensionRepState(state)) {
      formatDevErrorMessage4(`ExtensionRep: getInitResult() called for ExtensionRep in state id ${String(state.id)} < ${String(ExtensionRepStateIds.initialized)} (initialized)`);
    }
    return state.initResult;
  }
  getInitPeer(name) {
    const rep = this.builder.extensionNameMap.get(name);
    return rep ? rep.getExtensionInitDependency() : void 0;
  }
  getExtensionInitDependency() {
    const state = this.state;
    if (!isConfiguredExtensionRepState(state)) {
      formatDevErrorMessage4(`ExtensionRep: getExtensionInitDependency called in state id ${String(state.id)} (expected >= ${String(ExtensionRepStateIds.configured)} configured)`);
    }
    return {
      config: state.config
    };
  }
  getPeer(name) {
    const rep = this.builder.extensionNameMap.get(name);
    return rep ? rep.getExtensionDependency() : void 0;
  }
  getInitDependency(dep) {
    const rep = this.builder.getExtensionRep(dep);
    if (!(rep !== void 0)) {
      formatDevErrorMessage4(`LexicalExtensionBuilder: Extension ${this.extension.name} missing dependency extension ${dep.name} to be in registry`);
    }
    return rep.getExtensionInitDependency();
  }
  getDependency(dep) {
    const rep = this.builder.getExtensionRep(dep);
    if (!(rep !== void 0)) {
      formatDevErrorMessage4(`LexicalExtensionBuilder: Extension ${this.extension.name} missing dependency extension ${dep.name} to be in registry`);
    }
    return rep.getExtensionDependency();
  }
  getState() {
    const state = this.state;
    if (!isAfterRegistrationState(state)) {
      formatDevErrorMessage4(`ExtensionRep getState called in state id ${String(state.id)} (expected ${String(ExtensionRepStateIds.afterRegistration)} afterRegistration)`);
    }
    return state;
  }
  getDirectDependentNames() {
    return this.builder.incomingEdges.get(this.extension.name) || emptySet;
  }
  getPeerNameSet() {
    let s2 = this._peerNameSet;
    if (!s2) {
      s2 = new Set((this.extension.peerDependencies || []).map(([name]) => name));
      this._peerNameSet = s2;
    }
    return s2;
  }
  getExtensionDependency() {
    if (!this._dependency) {
      const state = this.state;
      if (!isBuiltExtensionRepState(state)) {
        formatDevErrorMessage4(`Extension ${this.extension.name} used as a dependency before build`);
      }
      this._dependency = {
        config: state.config,
        init: state.initResult,
        output: state.output
      };
    }
    return this._dependency;
  }
};
var HISTORY_MERGE_OPTIONS = {
  tag: HISTORY_MERGE_TAG2
};
function $defaultInitializer() {
  const root = $getRoot2();
  if (root.isEmpty()) {
    root.append($createParagraphNode2());
  }
}
var InitialStateExtension = defineExtension2({
  config: safeCast2({
    setOptions: HISTORY_MERGE_OPTIONS,
    updateOptions: HISTORY_MERGE_OPTIONS
  }),
  init({
    $initialEditorState = $defaultInitializer
  }) {
    return {
      $initialEditorState,
      initialized: false
    };
  },
  // eslint-disable-next-line sort-keys-fix/sort-keys-fix -- typescript inference is order dependent here for some reason
  afterRegistration(editor, {
    updateOptions,
    setOptions
  }, state) {
    const initResult = state.getInitResult();
    if (!initResult.initialized) {
      initResult.initialized = true;
      const {
        $initialEditorState
      } = initResult;
      if ($isEditorState2($initialEditorState)) {
        editor.setEditorState($initialEditorState, setOptions);
      } else if (typeof $initialEditorState === "function") {
        editor.update(() => {
          $initialEditorState(editor);
        }, updateOptions);
      } else if ($initialEditorState && (typeof $initialEditorState === "string" || typeof $initialEditorState === "object")) {
        const parsedEditorState = editor.parseEditorState($initialEditorState);
        editor.setEditorState(parsedEditorState, setOptions);
      }
    }
    return () => {
    };
  },
  name: "@lexical/extension/InitialState",
  // These are automatically added by createEditor, we add them here so they are
  // visible during extensionRep.init so extensions can see all known types before the
  // editor is created.
  // (excluding ArtificialNode__DO_NOT_USE because it isn't really public API
  // and shouldn't change anything)
  nodes: [RootNode2, TextNode2, LineBreakNode2, TabNode2, ParagraphNode2]
});
var builderSymbol = /* @__PURE__ */ Symbol.for("@lexical/extension/LexicalBuilder");
function buildEditorFromExtensions(...extensions) {
  return LexicalBuilder.fromExtensions(extensions).buildEditor();
}
function noop() {
}
function defaultOnError(err) {
  throw err;
}
function maybeWithBuilder(editor) {
  return editor;
}
function normalizeExtensionArgument(arg) {
  return Array.isArray(arg) ? arg : [arg];
}
var PACKAGE_VERSION = "0.44.0+dev.esm";
var LexicalBuilder = class _LexicalBuilder {
  roots;
  extensionNameMap;
  outgoingConfigEdges;
  incomingEdges;
  conflicts;
  _sortedExtensionReps;
  PACKAGE_VERSION;
  constructor(roots) {
    this.outgoingConfigEdges = /* @__PURE__ */ new Map();
    this.incomingEdges = /* @__PURE__ */ new Map();
    this.extensionNameMap = /* @__PURE__ */ new Map();
    this.conflicts = /* @__PURE__ */ new Map();
    this.PACKAGE_VERSION = PACKAGE_VERSION;
    this.roots = roots;
    for (const extension of roots) {
      this.addExtension(extension);
    }
  }
  static fromExtensions(extensions) {
    const roots = [normalizeExtensionArgument(InitialStateExtension)];
    for (const extension of extensions) {
      roots.push(normalizeExtensionArgument(extension));
    }
    return new _LexicalBuilder(roots);
  }
  static maybeFromEditor(editor) {
    const builder = maybeWithBuilder(editor)[builderSymbol];
    if (builder) {
      if (!(builder.PACKAGE_VERSION === PACKAGE_VERSION)) {
        formatDevErrorMessage4(`LexicalBuilder.fromEditor: The given editor was created with LexicalBuilder ${builder.PACKAGE_VERSION} but this version is ${PACKAGE_VERSION}. A project should have exactly one copy of LexicalBuilder`);
      }
      if (!(builder instanceof _LexicalBuilder)) {
        formatDevErrorMessage4(`LexicalBuilder.fromEditor: There are multiple copies of the same version of LexicalBuilder in your project, and this editor was created with another one. Your project, or one of its dependencies, has its package.json and/or bundler configured incorrectly.`);
      }
    }
    return builder;
  }
  /** Look up the editor that was created by this LexicalBuilder or throw */
  static fromEditor(editor) {
    const builder = _LexicalBuilder.maybeFromEditor(editor);
    if (!(builder !== void 0)) {
      formatDevErrorMessage4(`LexicalBuilder.fromEditor: The given editor was not created with LexicalBuilder`);
    }
    return builder;
  }
  constructEditor() {
    const {
      $initialEditorState: _$initialEditorState,
      onError,
      ...editorConfig
    } = this.buildCreateEditorArgs();
    const editor = Object.assign(createEditor2({
      ...editorConfig,
      ...onError ? {
        onError: (err) => {
          onError(err, editor);
        }
      } : {}
    }), {
      [builderSymbol]: this
    });
    for (const extensionRep of this.sortedExtensionReps()) {
      extensionRep.build(editor);
    }
    return editor;
  }
  buildEditor() {
    let disposeOnce = noop;
    function dispose() {
      try {
        disposeOnce();
      } finally {
        disposeOnce = noop;
      }
    }
    const editor = Object.assign(this.constructEditor(), {
      dispose,
      [Symbol.dispose]: dispose
    });
    disposeOnce = mergeRegister2(this.registerEditor(editor), () => editor.setRootElement(null));
    return editor;
  }
  hasExtensionByName(name) {
    return this.extensionNameMap.has(name);
  }
  getExtensionRep(extension) {
    const rep = this.extensionNameMap.get(extension.name);
    if (rep) {
      if (!(rep.extension === extension)) {
        formatDevErrorMessage4(`LexicalBuilder: A registered extension with name ${extension.name} exists but does not match the given extension`);
      }
      return rep;
    }
  }
  addEdge(fromExtensionName, toExtensionName, configs) {
    const outgoing = this.outgoingConfigEdges.get(fromExtensionName);
    if (outgoing) {
      outgoing.set(toExtensionName, configs);
    } else {
      this.outgoingConfigEdges.set(fromExtensionName, /* @__PURE__ */ new Map([[toExtensionName, configs]]));
    }
    const incoming = this.incomingEdges.get(toExtensionName);
    if (incoming) {
      incoming.add(fromExtensionName);
    } else {
      this.incomingEdges.set(toExtensionName, /* @__PURE__ */ new Set([fromExtensionName]));
    }
  }
  addExtension(arg) {
    if (!(this._sortedExtensionReps === void 0)) {
      formatDevErrorMessage4(`LexicalBuilder: addExtension called after finalization`);
    }
    const normalized = normalizeExtensionArgument(arg);
    const [extension] = normalized;
    if (!(typeof extension.name === "string")) {
      formatDevErrorMessage4(`LexicalBuilder: extension name must be string, not ${typeof extension.name}`);
    }
    let extensionRep = this.extensionNameMap.get(extension.name);
    if (!(extensionRep === void 0 || extensionRep.extension === extension)) {
      formatDevErrorMessage4(`LexicalBuilder: Multiple extensions registered with name ${extension.name}, names must be unique`);
    }
    if (!extensionRep) {
      extensionRep = new ExtensionRep(this, extension);
      this.extensionNameMap.set(extension.name, extensionRep);
      const hasConflict = this.conflicts.get(extension.name);
      if (typeof hasConflict === "string") {
        {
          formatDevErrorMessage4(`LexicalBuilder: extension ${extension.name} conflicts with ${hasConflict}`);
        }
      }
      for (const name of extension.conflictsWith || []) {
        if (!!this.extensionNameMap.has(name)) {
          formatDevErrorMessage4(`LexicalBuilder: extension ${extension.name} conflicts with ${name}`);
        }
        this.conflicts.set(name, extension.name);
      }
      for (const dep of extension.dependencies || []) {
        const normDep = normalizeExtensionArgument(dep);
        this.addEdge(extension.name, normDep[0].name, normDep.slice(1));
        this.addExtension(normDep);
      }
      for (const [depName, config] of extension.peerDependencies || []) {
        this.addEdge(extension.name, depName, config ? [config] : []);
      }
    }
  }
  sortedExtensionReps() {
    if (this._sortedExtensionReps) {
      return this._sortedExtensionReps;
    }
    const sortedExtensionReps = [];
    const visit = (rep, fromExtensionName) => {
      let mark = rep.state;
      if (isExactlyPermanentExtensionRepState(mark)) {
        return;
      }
      const extensionName = rep.extension.name;
      if (!isExactlyUnmarkedExtensionRepState(mark)) {
        formatDevErrorMessage4(`LexicalBuilder: Circular dependency detected for Extension ${extensionName} from ${fromExtensionName || "[unknown]"}`);
      }
      mark = applyTemporaryMark(mark);
      rep.state = mark;
      const outgoingConfigEdges = this.outgoingConfigEdges.get(extensionName);
      if (outgoingConfigEdges) {
        for (const toExtensionName of outgoingConfigEdges.keys()) {
          const toRep = this.extensionNameMap.get(toExtensionName);
          if (toRep) {
            visit(toRep, extensionName);
          }
        }
      }
      mark = applyPermanentMark(mark);
      rep.state = mark;
      sortedExtensionReps.push(rep);
    };
    for (const rep of this.extensionNameMap.values()) {
      if (isExactlyUnmarkedExtensionRepState(rep.state)) {
        visit(rep);
      }
    }
    for (const rep of sortedExtensionReps) {
      for (const [toExtensionName, configs] of this.outgoingConfigEdges.get(rep.extension.name) || []) {
        if (configs.length > 0) {
          const toRep = this.extensionNameMap.get(toExtensionName);
          if (toRep) {
            for (const config of configs) {
              toRep.configs.add(config);
            }
          }
        }
      }
    }
    for (const [extension, ...configs] of this.roots) {
      if (configs.length > 0) {
        const toRep = this.extensionNameMap.get(extension.name);
        if (!(toRep !== void 0)) {
          formatDevErrorMessage4(`LexicalBuilder: Expecting existing ExtensionRep for ${extension.name}`);
        }
        for (const config of configs) {
          toRep.configs.add(config);
        }
      }
    }
    this._sortedExtensionReps = sortedExtensionReps;
    return this._sortedExtensionReps;
  }
  registerEditor(editor) {
    const extensionReps = this.sortedExtensionReps();
    const controller = new AbortController();
    const cleanups = [() => controller.abort()];
    const signal2 = controller.signal;
    for (const extensionRep of extensionReps) {
      const cleanup = extensionRep.register(editor, signal2);
      if (cleanup) {
        cleanups.push(cleanup);
      }
    }
    for (const extensionRep of extensionReps) {
      const cleanup = extensionRep.afterRegistration(editor);
      if (cleanup) {
        cleanups.push(cleanup);
      }
    }
    return mergeRegister2(...cleanups);
  }
  buildCreateEditorArgs() {
    const config = {};
    const nodes = /* @__PURE__ */ new Set();
    const replacedNodes = /* @__PURE__ */ new Map();
    const htmlExport = /* @__PURE__ */ new Map();
    const htmlImport = {};
    const theme = {};
    const extensionReps = this.sortedExtensionReps();
    for (const extensionRep of extensionReps) {
      const {
        extension
      } = extensionRep;
      if (extension.onError !== void 0) {
        config.onError = extension.onError;
      }
      if (extension.disableEvents !== void 0) {
        config.disableEvents = extension.disableEvents;
      }
      if (extension.parentEditor !== void 0) {
        config.parentEditor = extension.parentEditor;
      }
      if (extension.editable !== void 0) {
        config.editable = extension.editable;
      }
      if (extension.namespace !== void 0) {
        config.namespace = extension.namespace;
      }
      if (extension.$initialEditorState !== void 0) {
        config.$initialEditorState = extension.$initialEditorState;
      }
      if (extension.nodes) {
        for (const node of getNodeConfig(extension)) {
          if (typeof node !== "function") {
            const conflictExtension = replacedNodes.get(node.replace);
            if (conflictExtension) {
              {
                formatDevErrorMessage4(`LexicalBuilder: Extension ${extension.name} can not register replacement for node ${node.replace.name} because ${conflictExtension.extension.name} already did`);
              }
            }
            replacedNodes.set(node.replace, extensionRep);
          }
          nodes.add(node);
        }
      }
      if (extension.html) {
        if (extension.html.export) {
          for (const [k, v2] of extension.html.export.entries()) {
            htmlExport.set(k, v2);
          }
        }
        if (extension.html.import) {
          Object.assign(htmlImport, extension.html.import);
        }
      }
      if (extension.theme) {
        deepThemeMergeInPlace(theme, extension.theme);
      }
    }
    if (Object.keys(theme).length > 0) {
      config.theme = theme;
    }
    if (nodes.size) {
      config.nodes = [...nodes];
    }
    const hasImport = Object.keys(htmlImport).length > 0;
    const hasExport = htmlExport.size > 0;
    if (hasImport || hasExport) {
      config.html = {};
      if (hasImport) {
        config.html.import = htmlImport;
      }
      if (hasExport) {
        config.html.export = htmlExport;
      }
    }
    for (const extensionRep of extensionReps) {
      extensionRep.init(config);
    }
    if (!config.onError) {
      config.onError = defaultOnError;
    }
    return config;
  }
};
function getExtensionDependencyFromEditor(editor, extension) {
  const builder = LexicalBuilder.fromEditor(editor);
  const rep = builder.getExtensionRep(extension);
  if (!(rep !== void 0)) {
    formatDevErrorMessage4(`getExtensionDependencyFromEditor: Extension ${extension.name} was not built when creating this editor`);
  }
  return rep.getExtensionDependency();
}
function getPeerDependencyFromEditor(editor, extensionName) {
  const builder = LexicalBuilder.maybeFromEditor(editor);
  if (!builder) return void 0;
  const peer = builder.extensionNameMap.get(extensionName);
  return peer ? peer.getExtensionDependency() : void 0;
}
function getPeerDependencyFromEditorOrThrow(editor, extensionName) {
  const dep = getPeerDependencyFromEditor(editor, extensionName);
  if (!(dep !== void 0)) {
    formatDevErrorMessage4(`getPeerDependencyFromEditorOrThrow: Editor was not built with Extension ${extensionName}`);
  }
  return dep;
}
var EMPTY_SET = /* @__PURE__ */ new Set();
var NodeSelectionExtension = defineExtension2({
  build(editor, config, state) {
    const editorStateStore = state.getDependency(EditorStateExtension).output;
    const watchedNodeStore = a({
      watchedNodeKeys: /* @__PURE__ */ new Map()
    });
    const selectedNodeKeys = watchedSignal(() => void 0, () => j(() => {
      const prevSelectedNodeKeys = selectedNodeKeys.peek();
      const {
        watchedNodeKeys
      } = watchedNodeStore.value;
      let nextSelectedNodeKeys;
      let didChange = false;
      editorStateStore.value.read(() => {
        const selection = $getSelection2();
        if (selection) {
          for (const [key, listeners] of watchedNodeKeys.entries()) {
            if (listeners.size === 0) {
              watchedNodeKeys.delete(key);
              continue;
            }
            const node = $getNodeByKey2(key);
            const isSelected = node && node.isSelected() || false;
            didChange = didChange || isSelected !== (prevSelectedNodeKeys ? prevSelectedNodeKeys.has(key) : false);
            if (isSelected) {
              nextSelectedNodeKeys = nextSelectedNodeKeys || /* @__PURE__ */ new Set();
              nextSelectedNodeKeys.add(key);
            }
          }
        }
      });
      if (!(!didChange && nextSelectedNodeKeys && prevSelectedNodeKeys && nextSelectedNodeKeys.size === prevSelectedNodeKeys.size)) {
        selectedNodeKeys.value = nextSelectedNodeKeys;
      }
    }));
    function watchNodeKey(key) {
      const watcher = g(() => (selectedNodeKeys.value || EMPTY_SET).has(key));
      const {
        watchedNodeKeys
      } = watchedNodeStore.peek();
      let listeners = watchedNodeKeys.get(key);
      const hadListener = listeners !== void 0;
      listeners = listeners || /* @__PURE__ */ new Set();
      listeners.add(watcher);
      if (!hadListener) {
        watchedNodeKeys.set(key, listeners);
        watchedNodeStore.value = {
          watchedNodeKeys
        };
      }
      return watcher;
    }
    return {
      watchNodeKey
    };
  },
  dependencies: [EditorStateExtension],
  name: "@lexical/extension/NodeSelection"
});
var INSERT_HORIZONTAL_RULE_COMMAND = createCommand2("INSERT_HORIZONTAL_RULE_COMMAND");
var HorizontalRuleNode = class _HorizontalRuleNode extends DecoratorNode2 {
  static getType() {
    return "horizontalrule";
  }
  static clone(node) {
    return new _HorizontalRuleNode(node.__key);
  }
  static importJSON(serializedNode) {
    return $createHorizontalRuleNode().updateFromJSON(serializedNode);
  }
  static importDOM() {
    return {
      hr: () => ({
        conversion: $convertHorizontalRuleElement,
        priority: 0
      })
    };
  }
  exportDOM() {
    return {
      element: document.createElement("hr")
    };
  }
  createDOM(config) {
    const element = document.createElement("hr");
    addClassNamesToElement2(element, config.theme.hr);
    return element;
  }
  getTextContent() {
    return "\n";
  }
  isInline() {
    return false;
  }
  updateDOM() {
    return false;
  }
};
function $convertHorizontalRuleElement() {
  return {
    node: $createHorizontalRuleNode()
  };
}
function $createHorizontalRuleNode() {
  return $create2(HorizontalRuleNode);
}
function $isHorizontalRuleNode(node) {
  return node instanceof HorizontalRuleNode;
}
function $toggleNodeSelection(node, shiftKey = false) {
  const selection = $getSelection2();
  const wasSelected = node.isSelected();
  const key = node.getKey();
  let nodeSelection;
  if (shiftKey && $isNodeSelection2(selection)) {
    nodeSelection = selection;
  } else {
    nodeSelection = $createNodeSelection2();
    $setSelection2(nodeSelection);
  }
  if (wasSelected) {
    nodeSelection.delete(key);
  } else {
    nodeSelection.add(key);
  }
}
var HorizontalRuleExtension = defineExtension2({
  dependencies: [EditorStateExtension, NodeSelectionExtension],
  name: "@lexical/extension/HorizontalRule",
  nodes: () => [HorizontalRuleNode],
  register(editor, config, state) {
    const {
      watchNodeKey
    } = state.getDependency(NodeSelectionExtension).output;
    const nodeSelectionStore = a({
      nodeSelections: /* @__PURE__ */ new Map()
    });
    const isSelectedClassName = editor._config.theme.hrSelected ?? "selected";
    return mergeRegister2(editor.registerCommand(INSERT_HORIZONTAL_RULE_COMMAND, (type) => {
      const selection = $getSelection2();
      if (!$isRangeSelection2(selection)) {
        return false;
      }
      const focusNode = selection.focus.getNode();
      if (focusNode !== null) {
        const horizontalRuleNode = $createHorizontalRuleNode();
        $insertNodeToNearestRoot2(horizontalRuleNode);
      }
      return true;
    }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(CLICK_COMMAND2, (event) => {
      if (isDOMNode2(event.target)) {
        const node = $getNodeFromDOMNode2(event.target);
        if ($isHorizontalRuleNode(node)) {
          $toggleNodeSelection(node, event.shiftKey);
          return true;
        }
      }
      return false;
    }, COMMAND_PRIORITY_LOW2), editor.registerMutationListener(HorizontalRuleNode, (nodes, payload) => {
      n(() => {
        let didChange = false;
        const {
          nodeSelections
        } = nodeSelectionStore.peek();
        for (const [k, v2] of nodes.entries()) {
          if (v2 === "destroyed") {
            nodeSelections.delete(k);
            didChange = true;
          } else {
            const prev = nodeSelections.get(k);
            const dom = editor.getElementByKey(k);
            if (prev) {
              prev.domNode.value = dom;
            } else {
              didChange = true;
              nodeSelections.set(k, {
                domNode: a(dom),
                selectedSignal: watchNodeKey(k)
              });
            }
          }
        }
        if (didChange) {
          nodeSelectionStore.value = {
            nodeSelections
          };
        }
      });
    }), j(() => {
      const effects = [];
      for (const {
        domNode,
        selectedSignal
      } of nodeSelectionStore.value.nodeSelections.values()) {
        effects.push(j(() => {
          const dom = domNode.value;
          if (dom) {
            const isSelected = selectedSignal.value;
            if (isSelected) {
              addClassNamesToElement2(dom, isSelectedClassName);
            } else {
              removeClassNamesFromElement2(dom, isSelectedClassName);
            }
          }
        }));
      }
      return mergeRegister2(...effects);
    }));
  }
});
function $defaultGetParentEditor() {
  const editor = $getEditor2();
  LexicalBuilder.fromEditor(editor);
  return editor;
}
var NestedEditorExtension = defineExtension2({
  build: (editor, config) => namedSignals({
    inheritEditableFromParent: config.inheritEditableFromParent
  }),
  config: safeCast2({
    $getParentEditor: $defaultGetParentEditor,
    inheritEditableFromParent: false
  }),
  init: (editorConfig, config, state) => {
    const parentEditor = config.$getParentEditor();
    editorConfig.parentEditor = parentEditor;
    editorConfig.theme = editorConfig.theme || parentEditor._config.theme;
  },
  name: "@lexical/extension/NestedEditor",
  register: (editor, config, state) => j(() => {
    const parentEditor = editor._parentEditor;
    if (parentEditor) {
      if (state.getOutput().inheritEditableFromParent.value) {
        editor.setEditable(parentEditor.isEditable());
        return parentEditor.registerEditableListener(editor.setEditable.bind(editor));
      }
    }
  })
});
var SelectionAlwaysOnDisplayExtension = defineExtension2({
  build: (editor, config, state) => namedSignals(config),
  config: safeCast2({
    disabled: false,
    onReposition: void 0
  }),
  name: "@lexical/utils/SelectionAlwaysOnDisplay",
  register: (editor, config, state) => {
    const stores = state.getOutput();
    return j(() => {
      if (!stores.disabled.value) {
        return selectionAlwaysOnDisplay2(editor, stores.onReposition.value);
      }
    });
  }
});
function $indentOverTab(selection) {
  const nodes = selection.getNodes();
  const canIndentBlockNodes = nodes.filter((node) => $isBlockElementNode2(node) && node.canIndent());
  if (canIndentBlockNodes.length > 0) {
    return true;
  }
  const anchor = selection.anchor;
  const focus = selection.focus;
  const first = focus.isBefore(anchor) ? focus : anchor;
  const firstNode = first.getNode();
  const firstBlock = $getNearestBlockElementAncestorOrThrow2(firstNode);
  if (firstBlock.canIndent()) {
    const firstBlockKey = firstBlock.getKey();
    let selectionAtStart = $createRangeSelection2();
    selectionAtStart.anchor.set(firstBlockKey, 0, "element");
    selectionAtStart.focus.set(firstBlockKey, 0, "element");
    selectionAtStart = $normalizeSelection__EXPERIMENTAL(selectionAtStart);
    if (selectionAtStart.anchor.is(first)) {
      return true;
    }
  }
  return false;
}
function $defaultCanIndent(node) {
  return node.canBeEmpty();
}
function registerTabIndentation(editor, maxIndent, $canIndent = $defaultCanIndent) {
  return mergeRegister2(editor.registerCommand(KEY_TAB_COMMAND2, (event) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    event.preventDefault();
    const command = $indentOverTab(selection) ? event.shiftKey ? OUTDENT_CONTENT_COMMAND2 : INDENT_CONTENT_COMMAND2 : INSERT_TAB_COMMAND2;
    return editor.dispatchCommand(command, void 0);
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(INDENT_CONTENT_COMMAND2, () => {
    const currentMaxIndent = typeof maxIndent === "number" ? maxIndent : maxIndent ? maxIndent.peek() : null;
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    const $currentCanIndent = typeof $canIndent === "function" ? $canIndent : $canIndent.peek();
    return $handleIndentAndOutdent2((block) => {
      if ($currentCanIndent(block)) {
        const newIndent = block.getIndent() + 1;
        if (!currentMaxIndent || newIndent < currentMaxIndent) {
          block.setIndent(newIndent);
        }
      }
    });
  }, COMMAND_PRIORITY_CRITICAL2));
}
var TabIndentationExtension = defineExtension2({
  build(editor, config, state) {
    return namedSignals(config);
  },
  config: safeCast2({
    $canIndent: $defaultCanIndent,
    disabled: false,
    maxIndent: null
  }),
  name: "@lexical/extension/TabIndentation",
  register(editor, config, state) {
    const {
      disabled,
      maxIndent,
      $canIndent
    } = state.getOutput();
    return j(() => {
      if (!disabled.value) {
        return registerTabIndentation(editor, maxIndent, $canIndent);
      }
    });
  }
});

// node_modules/@lexical/extension/LexicalExtension.mjs
var mod4 = true ? LexicalExtension_dev_exports : LexicalExtension_prod_exports;
var $createHorizontalRuleNode2 = mod4.$createHorizontalRuleNode;
var $isDecoratorTextNode2 = mod4.$isDecoratorTextNode;
var $isHorizontalRuleNode2 = mod4.$isHorizontalRuleNode;
var AutoFocusExtension2 = mod4.AutoFocusExtension;
var ClearEditorExtension2 = mod4.ClearEditorExtension;
var DecoratorTextExtension2 = mod4.DecoratorTextExtension;
var DecoratorTextNode2 = mod4.DecoratorTextNode;
var EditorStateExtension2 = mod4.EditorStateExtension;
var HorizontalRuleExtension2 = mod4.HorizontalRuleExtension;
var HorizontalRuleNode2 = mod4.HorizontalRuleNode;
var INSERT_HORIZONTAL_RULE_COMMAND2 = mod4.INSERT_HORIZONTAL_RULE_COMMAND;
var InitialStateExtension2 = mod4.InitialStateExtension;
var LexicalBuilder2 = mod4.LexicalBuilder;
var NestedEditorExtension2 = mod4.NestedEditorExtension;
var NodeSelectionExtension2 = mod4.NodeSelectionExtension;
var SelectionAlwaysOnDisplayExtension2 = mod4.SelectionAlwaysOnDisplayExtension;
var TabIndentationExtension2 = mod4.TabIndentationExtension;
var applyFormatFromStyle2 = mod4.applyFormatFromStyle;
var applyFormatToDom2 = mod4.applyFormatToDom;
var batch = mod4.batch;
var buildEditorFromExtensions2 = mod4.buildEditorFromExtensions;
var computed = mod4.computed;
var configExtension3 = mod4.configExtension;
var declarePeerDependency3 = mod4.declarePeerDependency;
var defineExtension3 = mod4.defineExtension;
var effect = mod4.effect;
var getExtensionDependencyFromEditor2 = mod4.getExtensionDependencyFromEditor;
var getKnownTypesAndNodes2 = mod4.getKnownTypesAndNodes;
var getPeerDependencyFromEditor2 = mod4.getPeerDependencyFromEditor;
var getPeerDependencyFromEditorOrThrow2 = mod4.getPeerDependencyFromEditorOrThrow;
var namedSignals2 = mod4.namedSignals;
var registerClearEditor2 = mod4.registerClearEditor;
var registerTabIndentation2 = mod4.registerTabIndentation;
var safeCast3 = mod4.safeCast;
var shallowMergeConfig3 = mod4.shallowMergeConfig;
var signal = mod4.signal;
var untracked = mod4.untracked;
var watchedSignal2 = mod4.watchedSignal;

// node_modules/@lexical/html/LexicalHtml.dev.mjs
var LexicalHtml_dev_exports = {};
__export(LexicalHtml_dev_exports, {
  $generateDOMFromNodes: () => $generateDOMFromNodes,
  $generateDOMFromRoot: () => $generateDOMFromRoot,
  $generateHtmlFromNodes: () => $generateHtmlFromNodes,
  $generateNodesFromDOM: () => $generateNodesFromDOM,
  $getRenderContextValue: () => $getRenderContextValue,
  $withRenderContext: () => $withRenderContext,
  DOMRenderExtension: () => DOMRenderExtension,
  RenderContextExport: () => RenderContextExport,
  RenderContextRoot: () => RenderContextRoot,
  contextUpdater: () => contextUpdater,
  contextValue: () => contextValue,
  createRenderState: () => createRenderState,
  domOverride: () => domOverride
});
function formatDevErrorMessage5(message) {
  throw new Error(message);
}
var activeContext;
function getContextValue(contextRecord, cfg) {
  const {
    key
  } = cfg;
  return contextRecord && key in contextRecord ? contextRecord[key] : cfg.defaultValue;
}
function getEditorContext(editor) {
  return activeContext && activeContext.editor === editor ? activeContext : void 0;
}
function getContextRecord(sym, editor) {
  const editorContext = getEditorContext(editor);
  return editorContext && editorContext[sym];
}
function toPair(contextRecord, pairOrUpdater) {
  if ("cfg" in pairOrUpdater) {
    const {
      cfg,
      updater
    } = pairOrUpdater;
    return [cfg, updater(getContextValue(contextRecord, cfg))];
  }
  return pairOrUpdater;
}
function contextFromPairs(pairs, parent) {
  let rval = parent;
  for (const pairOrUpdater of pairs) {
    const [k, v2] = toPair(rval, pairOrUpdater);
    const key = k.key;
    if (rval === parent && getContextValue(rval, k) === v2) {
      continue;
    }
    const ctx = rval || createChildContext(parent);
    ctx[key] = v2;
    rval = ctx;
  }
  return rval;
}
function createChildContext(parent) {
  return Object.create(parent || null);
}
function contextValue(cfg, value) {
  return [cfg, value];
}
function contextUpdater(cfg, updater) {
  return {
    cfg,
    updater
  };
}
// @__NO_SIDE_EFFECTS__
function $withFullContext(sym, contextRecord, f, editor = $getEditor2()) {
  const prevDOMContext = activeContext;
  const parentEditorContext = getEditorContext(editor);
  try {
    activeContext = {
      ...parentEditorContext,
      editor,
      [sym]: contextRecord
    };
    return f();
  } finally {
    activeContext = prevDOMContext;
  }
}
// @__NO_SIDE_EFFECTS__
function $withContext(sym, $defaults = () => void 0) {
  return (cfg, editor = $getEditor2()) => {
    return (f) => {
      const parentEditorContext = getEditorContext(editor);
      const parentContextRecord = parentEditorContext && parentEditorContext[sym];
      const contextRecord = contextFromPairs(cfg, parentContextRecord || $defaults(editor));
      if (!contextRecord || contextRecord === parentContextRecord) {
        return f();
      }
      return /* @__PURE__ */ $withFullContext(sym, contextRecord, f, editor);
    };
  };
}
// @__NO_SIDE_EFFECTS__
function createContextState(tag, name, getDefaultValue, isEqual2) {
  return Object.assign(createState2(Symbol(name), {
    isEqual: isEqual2,
    parse: getDefaultValue
  }), {
    [tag]: true
  });
}
var DOMRenderExtensionName = "@lexical/html/DOM";
var DOMRenderContextSymbol = /* @__PURE__ */ Symbol.for("@lexical/html/DOMExportContext");
var ALWAYS_TRUE = () => true;
function buildTypeTree(editorConfig) {
  const t2 = {};
  const {
    nodes
  } = getKnownTypesAndNodes2(editorConfig);
  for (const klass of nodes) {
    const type = klass.getType();
    t2[type] = {
      klass,
      types: {}
    };
  }
  for (const baseRec of Object.values(t2)) {
    if (baseRec) {
      const baseType = baseRec.klass.getType();
      for (let {
        klass
      } = baseRec; $isLexicalNode2(klass.prototype); klass = Object.getPrototypeOf(klass)) {
        const {
          ownNodeType
        } = getStaticNodeConfig2(klass);
        const superRec = ownNodeType && t2[ownNodeType];
        if (superRec) {
          superRec.types[baseType] = true;
        }
      }
    }
  }
  return t2;
}
function buildNodePredicate(klass) {
  return (node) => node instanceof klass;
}
function getPredicate(typeTree, {
  nodes
}) {
  if (nodes === "*") {
    return ALWAYS_TRUE;
  }
  let types = {};
  const predicates = [];
  for (const klassOrPredicate of nodes) {
    if ("getType" in klassOrPredicate) {
      const type = klassOrPredicate.getType();
      if (types) {
        const tree = typeTree[type];
        if (!(tree !== void 0)) {
          formatDevErrorMessage5(`Node class ${klassOrPredicate.name} with type ${type} not registered in editor`);
        }
        types = Object.assign(types, tree.types);
      }
      predicates.push(buildNodePredicate(klassOrPredicate));
    } else {
      types = void 0;
      predicates.push(klassOrPredicate);
    }
  }
  if (types) {
    return types;
  } else if (predicates.length === 1) {
    return predicates[0];
  }
  return (node) => {
    for (const predicate of predicates) {
      if (predicate(node)) {
        return true;
      }
    }
    return false;
  };
}
function makePrerender() {
  return {
    $createDOM: [],
    $decorateDOM: [],
    $exportDOM: [],
    $extractWithChild: [],
    $getDOMSlot: [],
    $shouldExclude: [],
    $shouldInclude: [],
    $updateDOM: []
  };
}
function ignoreNext2(acc) {
  return (node, _$next, editor) => acc(node, editor);
}
function ignoreNext3(acc) {
  return (node, a2, _$next, editor) => acc(node, a2, editor);
}
function ignoreNext4(acc) {
  return (node, a2, b2, _$next, editor) => acc(node, a2, b2, editor);
}
function ignoreNext5(acc) {
  return (node, a2, b2, c2, _$next, editor) => acc(node, a2, b2, c2, editor);
}
function merge2($acc, $getOverride) {
  return (node, editor) => {
    const $next = () => $acc(node, editor);
    const $override = $getOverride(node);
    return $override ? $override(node, $next, editor) : $next();
  };
}
function merge3(acc, $getOverride) {
  return (node, a2, editor) => {
    const $next = () => acc(node, a2, editor);
    const $override = $getOverride(node);
    return $override ? $override(node, a2, $next, editor) : $next();
  };
}
function merge4($acc, $getOverride) {
  return (node, a2, b2, editor) => {
    const $next = () => $acc(node, a2, b2, editor);
    const $override = $getOverride(node);
    return $override ? $override(node, a2, b2, $next, editor) : $next();
  };
}
function merge5(acc, $getOverride) {
  return (node, a2, b2, c2, editor) => {
    const $next = () => acc(node, a2, b2, c2, editor);
    const $override = $getOverride(node);
    return $override ? $override(node, a2, b2, c2, $next, editor) : $next();
  };
}
function sequence4($acc, $getOverride) {
  return (node, a2, b2, editor) => {
    $acc(node, a2, b2, editor);
    const $override = $getOverride(node);
    if ($override) {
      $override(node, a2, b2, editor);
    }
  };
}
function compilePrerenderKey(prerender, k, defaults, mergeFunction, ignoreNextFunction) {
  let acc = defaults[k];
  for (const pair of prerender[k]) {
    if (typeof pair[0] === "function") {
      const [$predicate, $override] = pair;
      acc = mergeFunction(acc, (node) => $predicate(node) && $override || void 0);
    } else {
      const typeOverrides = pair[1];
      const compiled = {};
      for (const type in typeOverrides) {
        const arr = typeOverrides[type];
        if (arr) {
          compiled[type] = arr.reduce(($acc, $override) => mergeFunction($acc, () => $override), acc);
        }
      }
      acc = mergeFunction(acc, (node) => {
        const f = compiled[node.getType()];
        return f && ignoreNextFunction(f);
      });
    }
  }
  defaults[k] = acc;
}
function addOverride(prerender, k, predicateOrTypes, override) {
  if (!override) {
    return;
  }
  const arr = prerender[k];
  if (typeof predicateOrTypes === "function") {
    arr.push([predicateOrTypes, override]);
  } else {
    const last = arr[arr.length - 1];
    let types;
    if (last && last[0] === "types") {
      types = last[1];
    } else {
      types = {};
      arr.push(["types", types]);
    }
    for (const type in predicateOrTypes) {
      const typeArr = types[type] || [];
      types[type] = typeArr;
      typeArr.push(override);
    }
  }
}
function isWildcard(override) {
  return override.nodes === "*";
}
function sortedOverrides(overrides) {
  const byWildcard = [];
  const byPredicate = [];
  const byNode = [];
  for (const override of overrides) {
    if (isWildcard(override)) {
      byWildcard.push(override);
    } else if (Array.isArray(override.nodes)) {
      for (const klassOrPredicate of override.nodes) {
        if ($isLexicalNode2(klassOrPredicate.prototype)) {
          byNode.push(override.nodes.length === 1 ? override : {
            ...override,
            nodes: [klassOrPredicate]
          });
        } else {
          byPredicate.push(override.nodes.length === 1 ? override : {
            ...override,
            nodes: [klassOrPredicate]
          });
        }
      }
    }
  }
  const depths = /* @__PURE__ */ new Map();
  const depthOf = (klass) => {
    let depth = depths.get(klass);
    if (depth === void 0) {
      depth = 0;
      for (let k = klass; $isLexicalNode2(k.prototype); k = Object.getPrototypeOf(k)) {
        depth++;
      }
      depths.set(klass, depth);
    }
    return depth;
  };
  byNode.sort((a2, b2) => depthOf(a2.nodes[0]) - depthOf(b2.nodes[0]));
  return [...byNode, ...byPredicate, ...byWildcard];
}
function precompileDOMRenderConfigOverrides(editorConfig, overrides) {
  const typeTree = buildTypeTree(editorConfig);
  const prerender = makePrerender();
  for (const override of sortedOverrides(overrides)) {
    const predicateOrTypes = getPredicate(typeTree, override);
    for (const k_ in prerender) {
      const k = k_;
      addOverride(prerender, k, predicateOrTypes, override[k]);
    }
  }
  return prerender;
}
function identity(v2) {
  return v2;
}
function compileDOMRenderConfigOverrides(editorConfig, {
  overrides
}) {
  const prerender = precompileDOMRenderConfigOverrides(editorConfig, overrides);
  const dom = {
    ...DEFAULT_EDITOR_DOM_CONFIG2,
    ...editorConfig.dom
  };
  compilePrerenderKey(prerender, "$createDOM", dom, merge2, ignoreNext2);
  compilePrerenderKey(prerender, "$exportDOM", dom, merge2, ignoreNext2);
  compilePrerenderKey(prerender, "$extractWithChild", dom, merge5, ignoreNext5);
  compilePrerenderKey(prerender, "$getDOMSlot", dom, merge3, ignoreNext3);
  compilePrerenderKey(prerender, "$shouldExclude", dom, merge3, ignoreNext3);
  compilePrerenderKey(prerender, "$shouldInclude", dom, merge3, ignoreNext3);
  compilePrerenderKey(prerender, "$updateDOM", dom, merge4, ignoreNext4);
  compilePrerenderKey(prerender, "$decorateDOM", dom, sequence4, identity);
  return dom;
}
var DOMRenderExtension = defineExtension2({
  build(editor, config, state) {
    return {
      defaults: contextFromPairs(config.contextDefaults, void 0)
    };
  },
  config: {
    contextDefaults: [],
    overrides: []
  },
  html: {
    // Define a RootNode export for $generateDOMFromRoot
    export: /* @__PURE__ */ new Map([[RootNode2, () => {
      const element = document.createElement("div");
      element.role = "textbox";
      return {
        element
      };
    }]])
  },
  init(editorConfig, config) {
    editorConfig.dom = compileDOMRenderConfigOverrides(editorConfig, config);
  },
  mergeConfig(config, partial) {
    const merged = shallowMergeConfig2(config, partial);
    for (const k of ["overrides", "contextDefaults"]) {
      if (partial[k]) {
        merged[k] = [...config[k], ...partial[k]];
      }
    }
    return merged;
  },
  name: DOMRenderExtensionName
});
// @__NO_SIDE_EFFECTS__
function createRenderState(name, getDefaultValue, isEqual2) {
  return /* @__PURE__ */ createContextState(DOMRenderContextSymbol, name, getDefaultValue, isEqual2);
}
var RenderContextRoot = /* @__PURE__ */ createRenderState("root", Boolean);
var RenderContextExport = /* @__PURE__ */ createRenderState("isExport", Boolean);
function getDefaultRenderContext(editor) {
  const builder = LexicalBuilder2.maybeFromEditor(editor);
  return builder && builder.hasExtensionByName(DOMRenderExtensionName) ? getExtensionDependencyFromEditor2(editor, DOMRenderExtension).output.defaults : void 0;
}
function getRenderContext(editor) {
  return getContextRecord(DOMRenderContextSymbol, editor) || getDefaultRenderContext(editor);
}
function $getRenderContextValue(cfg, editor = $getEditor2()) {
  return getContextValue(getRenderContext(editor), cfg);
}
var $withRenderContext = /* @__PURE__ */ $withContext(DOMRenderContextSymbol, getDefaultRenderContext);
// @__NO_SIDE_EFFECTS__
function domOverride(nodes, config) {
  return {
    ...config,
    nodes
  };
}
function isStyleRule(rule) {
  return rule.constructor.name === CSSStyleRule.name;
}
function inlineStylesFromStyleSheets(doc) {
  if (doc.querySelector("style") === null) {
    return;
  }
  const originalInlineStyles = /* @__PURE__ */ new Map();
  function getOriginalInlineProps(el) {
    let props = originalInlineStyles.get(el);
    if (props === void 0) {
      props = /* @__PURE__ */ new Set();
      for (let i2 = 0; i2 < el.style.length; i2++) {
        props.add(el.style[i2]);
      }
      originalInlineStyles.set(el, props);
    }
    return props;
  }
  try {
    for (const sheet of Array.from(doc.styleSheets)) {
      let rules;
      try {
        rules = sheet.cssRules;
      } catch (_unused) {
        continue;
      }
      for (const rule of Array.from(rules)) {
        if (!isStyleRule(rule)) {
          continue;
        }
        let elements;
        try {
          elements = doc.querySelectorAll(rule.selectorText);
        } catch (_unused2) {
          continue;
        }
        for (const el of Array.from(elements)) {
          if (!isHTMLElement2(el)) {
            continue;
          }
          const originalProps = getOriginalInlineProps(el);
          for (let i2 = 0; i2 < rule.style.length; i2++) {
            const prop = rule.style[i2];
            if (!originalProps.has(prop)) {
              el.style.setProperty(prop, rule.style.getPropertyValue(prop), rule.style.getPropertyPriority(prop));
            }
          }
        }
      }
    }
  } catch (_unused3) {
  }
}
var IGNORE_TAGS = /* @__PURE__ */ new Set(["STYLE", "SCRIPT"]);
function $generateNodesFromDOM(editor, dom) {
  if (isDOMDocumentNode2(dom)) {
    inlineStylesFromStyleSheets(dom);
  }
  const elements = isDOMDocumentNode2(dom) ? dom.body.childNodes : dom.childNodes;
  const lexicalNodes = [];
  const allArtificialNodes = [];
  for (const element of elements) {
    if (!IGNORE_TAGS.has(element.nodeName)) {
      const lexicalNode = $createNodesFromDOM(element, editor, allArtificialNodes, false);
      if (lexicalNode !== null) {
        for (const node of lexicalNode) {
          lexicalNodes.push(node);
        }
      }
    }
  }
  $unwrapArtificialNodes(allArtificialNodes);
  return lexicalNodes;
}
function $generateDOMFromNodes(container, selection = null, editor = $getEditor2()) {
  return $withRenderContext([contextValue(RenderContextExport, true)], editor)(() => {
    const root = $getRoot2();
    const domConfig = $getEditorDOMRenderConfig2(editor);
    const parentElementAppend = container.append.bind(container);
    for (const topLevelNode of root.getChildren()) {
      $appendNodesToHTML(editor, topLevelNode, parentElementAppend, selection, domConfig);
    }
    return container;
  });
}
function $generateDOMFromRoot(container, root = $getRoot2()) {
  const editor = $getEditor2();
  return $withRenderContext([contextValue(RenderContextExport, true), contextValue(RenderContextRoot, true)], editor)(() => {
    const selection = null;
    const domConfig = $getEditorDOMRenderConfig2(editor);
    const parentElementAppend = container.append.bind(container);
    $appendNodesToHTML(editor, root, parentElementAppend, selection, domConfig);
    return container;
  });
}
function $generateHtmlFromNodes(editor, selection = null) {
  if (typeof document === "undefined" || typeof window === "undefined" && typeof global.window === "undefined") {
    {
      formatDevErrorMessage5(`To use $generateHtmlFromNodes in headless mode please initialize a headless browser implementation such as JSDom or use withDOM from @lexical/headless/dom before calling this function.`);
    }
  }
  return $generateDOMFromNodes(document.createElement("div"), selection, editor).innerHTML;
}
function $appendNodesToHTML(editor, currentNode, parentElementAppend, selection = null, domConfig = $getEditorDOMRenderConfig2(editor)) {
  let shouldInclude = domConfig.$shouldInclude(currentNode, selection, editor);
  const shouldExclude = domConfig.$shouldExclude(currentNode, selection, editor);
  let target = currentNode;
  if (selection !== null && $isTextNode2(currentNode)) {
    target = $sliceSelectedTextNodeContent2(selection, currentNode, "clone");
  }
  const exportProps = domConfig.$exportDOM(target, editor);
  const {
    element,
    after,
    append: append2,
    $getChildNodes
  } = exportProps;
  if (!element) {
    return false;
  }
  const fragment = document.createDocumentFragment();
  const children = $getChildNodes ? $getChildNodes() : $isElementNode2(target) ? target.getChildren() : [];
  const fragmentAppend = fragment.append.bind(fragment);
  for (const childNode of children) {
    const shouldIncludeChild = $appendNodesToHTML(editor, childNode, fragmentAppend, selection, domConfig);
    if (!shouldInclude && shouldIncludeChild && domConfig.$extractWithChild(currentNode, childNode, selection, "html", editor)) {
      shouldInclude = true;
    }
  }
  if (shouldInclude && !shouldExclude) {
    if (isHTMLElement2(element) || isDocumentFragment2(element)) {
      if (append2) {
        append2(fragment);
      } else {
        element.append(fragment);
      }
    }
    parentElementAppend(element);
    if (after) {
      const newElement = after.call(target, element);
      if (newElement) {
        if (isDocumentFragment2(element)) {
          element.replaceChildren(newElement);
        } else {
          element.replaceWith(newElement);
        }
      }
    }
  } else {
    parentElementAppend(fragment);
  }
  return shouldInclude;
}
function getConversionFunction(domNode, editor) {
  const {
    nodeName
  } = domNode;
  const cachedConversions = editor._htmlConversions.get(nodeName.toLowerCase());
  let currentConversion = null;
  if (cachedConversions !== void 0) {
    for (const cachedConversion of cachedConversions) {
      const domConversion = cachedConversion(domNode);
      if (domConversion !== null && (currentConversion === null || // Given equal priority, prefer the last registered importer
      // which is typically an application custom node or HTMLConfig['import']
      (currentConversion.priority || 0) <= (domConversion.priority || 0))) {
        currentConversion = domConversion;
      }
    }
  }
  return currentConversion !== null ? currentConversion.conversion : null;
}
function $createNodesFromDOM(node, editor, allArtificialNodes, hasBlockAncestorLexicalNode, forChildMap = /* @__PURE__ */ new Map(), parentLexicalNode) {
  const lexicalNodes = [];
  if (IGNORE_TAGS.has(node.nodeName)) {
    return lexicalNodes;
  }
  let currentLexicalNode = null;
  const transformFunction = getConversionFunction(node, editor);
  const transformOutput = transformFunction ? transformFunction(node) : null;
  let postTransform = null;
  if (transformOutput !== null) {
    postTransform = transformOutput.after;
    const transformNodes = transformOutput.node;
    currentLexicalNode = Array.isArray(transformNodes) ? transformNodes[transformNodes.length - 1] : transformNodes;
    if (currentLexicalNode !== null) {
      for (const [, forChildFunction] of forChildMap) {
        currentLexicalNode = forChildFunction(currentLexicalNode, parentLexicalNode);
        if (!currentLexicalNode) {
          break;
        }
      }
      if (currentLexicalNode) {
        lexicalNodes.push(...Array.isArray(transformNodes) ? transformNodes : [currentLexicalNode]);
      }
    }
    if (transformOutput.forChild != null) {
      forChildMap.set(node.nodeName, transformOutput.forChild);
    }
  }
  const children = node.childNodes;
  let childLexicalNodes = [];
  const hasBlockAncestorLexicalNodeForChildren = currentLexicalNode != null && $isRootOrShadowRoot2(currentLexicalNode) ? false : currentLexicalNode != null && $isBlockElementNode2(currentLexicalNode) || hasBlockAncestorLexicalNode;
  for (let i2 = 0; i2 < children.length; i2++) {
    childLexicalNodes.push(...$createNodesFromDOM(children[i2], editor, allArtificialNodes, hasBlockAncestorLexicalNodeForChildren, new Map(forChildMap), currentLexicalNode));
  }
  if (postTransform != null) {
    childLexicalNodes = postTransform(childLexicalNodes);
  }
  if (isBlockDomNode2(node)) {
    if (!hasBlockAncestorLexicalNodeForChildren) {
      childLexicalNodes = wrapContinuousInlines(node, childLexicalNodes, $createParagraphNode2);
    } else {
      childLexicalNodes = wrapContinuousInlines(node, childLexicalNodes, () => {
        const artificialNode = new ArtificialNode__DO_NOT_USE2();
        allArtificialNodes.push(artificialNode);
        return artificialNode;
      });
    }
  }
  if (currentLexicalNode == null) {
    if (childLexicalNodes.length > 0) {
      for (const childNode of childLexicalNodes) {
        lexicalNodes.push(childNode);
      }
    } else {
      if (isBlockDomNode2(node) && isDomNodeBetweenTwoInlineNodes(node)) {
        lexicalNodes.push($createLineBreakNode2());
      }
    }
  } else {
    if ($isElementNode2(currentLexicalNode)) {
      currentLexicalNode.append(...childLexicalNodes);
    }
  }
  return lexicalNodes;
}
function wrapContinuousInlines(domNode, nodes, createWrapperFn) {
  const textAlign = domNode.style.textAlign;
  const out = [];
  let continuousInlines = [];
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    if ($isBlockElementNode2(node)) {
      if (textAlign && !node.getFormat()) {
        node.setFormat(textAlign);
      }
      out.push(node);
    } else {
      continuousInlines.push(node);
      if (i2 === nodes.length - 1 || i2 < nodes.length - 1 && $isBlockElementNode2(nodes[i2 + 1])) {
        const wrapper = createWrapperFn();
        wrapper.setFormat(textAlign);
        wrapper.append(...continuousInlines);
        out.push(wrapper);
        continuousInlines = [];
      }
    }
  }
  return out;
}
function $unwrapArtificialNodes(allArtificialNodes) {
  for (const node of allArtificialNodes) {
    if (node.getParent() && node.getNextSibling() instanceof ArtificialNode__DO_NOT_USE2) {
      node.insertAfter($createLineBreakNode2());
    }
  }
  for (const node of allArtificialNodes) {
    const parent = node.getParent();
    if (parent) {
      parent.splice(node.getIndexWithinParent(), 1, node.getChildren());
    }
  }
}
function isDomNodeBetweenTwoInlineNodes(node) {
  if (node.nextSibling == null || node.previousSibling == null) {
    return false;
  }
  return isInlineDomNode2(node.nextSibling) && isInlineDomNode2(node.previousSibling);
}

// node_modules/@lexical/html/LexicalHtml.mjs
var mod5 = true ? LexicalHtml_dev_exports : LexicalHtml_prod_exports;
var $generateDOMFromNodes2 = mod5.$generateDOMFromNodes;
var $generateDOMFromRoot2 = mod5.$generateDOMFromRoot;
var $generateHtmlFromNodes2 = mod5.$generateHtmlFromNodes;
var $generateNodesFromDOM2 = mod5.$generateNodesFromDOM;
var $getRenderContextValue2 = mod5.$getRenderContextValue;
var $withRenderContext2 = mod5.$withRenderContext;
var DOMRenderExtension2 = mod5.DOMRenderExtension;
var RenderContextExport2 = mod5.RenderContextExport;
var RenderContextRoot2 = mod5.RenderContextRoot;
var contextUpdater2 = mod5.contextUpdater;
var contextValue2 = mod5.contextValue;
var createRenderState2 = mod5.createRenderState;
var domOverride2 = mod5.domOverride;

// node_modules/@lexical/clipboard/LexicalClipboard.dev.mjs
function formatDevErrorMessage6(message) {
  throw new Error(message);
}
function caretFromPoint(x2, y2) {
  if (typeof document.caretRangeFromPoint !== "undefined") {
    const range = document.caretRangeFromPoint(x2, y2);
    if (range === null) {
      return null;
    }
    return {
      node: range.startContainer,
      offset: range.startOffset
    };
  } else if (document.caretPositionFromPoint !== "undefined") {
    const range = document.caretPositionFromPoint(x2, y2);
    if (range === null) {
      return null;
    }
    return {
      node: range.offsetNode,
      offset: range.offset
    };
  } else {
    return null;
  }
}
function $getHtmlContent(editor, selection = $getSelection2()) {
  if (selection == null) {
    {
      formatDevErrorMessage6(`Expected valid LexicalSelection`);
    }
  }
  if ($isRangeSelection2(selection) && selection.isCollapsed() || selection.getNodes().length === 0) {
    return "";
  }
  return $generateHtmlFromNodes2(editor, selection);
}
function $getLexicalContent(editor, selection = $getSelection2()) {
  if (selection == null) {
    {
      formatDevErrorMessage6(`Expected valid LexicalSelection`);
    }
  }
  if ($isRangeSelection2(selection) && selection.isCollapsed() || selection.getNodes().length === 0) {
    return null;
  }
  return JSON.stringify($generateJSONFromSelectedNodes(editor, selection));
}
function $insertDataTransferForPlainText(dataTransfer, selection) {
  const text = dataTransfer.getData("text/plain") || dataTransfer.getData("text/uri-list");
  if (text != null) {
    selection.insertRawText(text);
  }
}
function $insertDataTransferForRichText(dataTransfer, selection, editor) {
  const lexicalString = dataTransfer.getData("application/x-lexical-editor");
  if (lexicalString) {
    try {
      const payload = JSON.parse(lexicalString);
      if (payload.namespace === editor._config.namespace && Array.isArray(payload.nodes)) {
        const nodes = $generateNodesFromSerializedNodes(payload.nodes);
        return $insertGeneratedNodes(editor, nodes, selection);
      }
    } catch (error) {
      console.error(error);
    }
  }
  const htmlString = dataTransfer.getData("text/html");
  const plainString = dataTransfer.getData("text/plain");
  if (htmlString && plainString !== htmlString) {
    try {
      const parser = new DOMParser();
      const dom = parser.parseFromString(trustHTML(htmlString), "text/html");
      const nodes = $generateNodesFromDOM2(editor, dom);
      return $insertGeneratedNodes(editor, nodes, selection);
    } catch (error) {
      console.error(error);
    }
  }
  const text = plainString || dataTransfer.getData("text/uri-list");
  if (text != null) {
    if ($isRangeSelection2(selection)) {
      const parts = text.split(/(\r?\n|\t)/);
      if (parts[parts.length - 1] === "") {
        parts.pop();
      }
      for (let i2 = 0; i2 < parts.length; i2++) {
        const currentSelection = $getSelection2();
        if ($isRangeSelection2(currentSelection)) {
          const part = parts[i2];
          if (part === "\n" || part === "\r\n") {
            currentSelection.insertParagraph();
          } else if (part === "	") {
            currentSelection.insertNodes([$createTabNode2()]);
          } else {
            currentSelection.insertText(part);
          }
        }
      }
    } else {
      selection.insertRawText(text);
    }
  }
}
var LEXICAL_DRAG_MIME_TYPE = "application/x-lexical-drag";
function $writeDragSourceToDataTransfer(dataTransfer, editor) {
  const marker = {
    editorKey: editor.getKey()
  };
  dataTransfer.setData(LEXICAL_DRAG_MIME_TYPE, JSON.stringify(marker));
}
function isLexicalDragMarker(value) {
  return value !== null && typeof value === "object" && "editorKey" in value && typeof value.editorKey === "string";
}
function readDragMarker(dataTransfer) {
  const raw = dataTransfer.getData(LEXICAL_DRAG_MIME_TYPE);
  if (!raw) {
    return null;
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (_unused) {
    return null;
  }
  return isLexicalDragMarker(parsed) ? parsed : null;
}
function findEditorRootByKey(key, doc) {
  const elements = doc.querySelectorAll('[data-lexical-editor="true"]');
  for (const el of Array.from(elements)) {
    const editor = el.__lexicalEditor;
    if (editor && editor.getKey() === key) {
      return el;
    }
  }
  return null;
}
function $resolveDropPointCaret(event) {
  const hit = caretFromPoint(event.clientX, event.clientY);
  if (hit === null) {
    return null;
  }
  const node = $getNearestNodeFromDOMNode2(hit.node);
  if (node === null) {
    return null;
  }
  if ($isTextNode2(node)) {
    return $getTextPointCaret2(node, "next", hit.offset);
  }
  if ($isElementNode2(node)) {
    return $getChildCaretAtIndex2(node, hit.offset, "next");
  }
  const parent = node.getParent();
  if (parent === null) {
    return null;
  }
  return $getChildCaretAtIndex2(parent, node.getIndexWithinParent() + 1, "next");
}
function $isDropCaretInsideSelection(dropCaret, selection) {
  const {
    anchor: start,
    focus: end
  } = $getCaretRangeInDirection2($caretRangeFromSelection2(selection), "next");
  return $comparePointCaretNext2(start, dropCaret) < 0 && $comparePointCaretNext2(dropCaret, end) < 0;
}
function $doDrop(event, editor, $insertDataTransfer) {
  const dataTransfer = event.dataTransfer;
  if (dataTransfer === null) {
    return false;
  }
  const marker = readDragMarker(dataTransfer);
  if (marker === null) {
    return false;
  }
  const dropCaret = $resolveDropPointCaret(event);
  if (dropCaret === null) {
    return false;
  }
  const stableDropCaret = $splitAtPointCaretNext2(dropCaret);
  if (stableDropCaret === null) {
    return false;
  }
  const isSameEditorDrag = marker.editorKey === editor.getKey();
  const currentSelection = $getSelection2();
  if (isSameEditorDrag) {
    if (!$isRangeSelection2(currentSelection) || currentSelection.isCollapsed()) {
      return false;
    }
    if ($isDropCaretInsideSelection(dropCaret, currentSelection)) {
      event.preventDefault();
      return true;
    }
    currentSelection.removeText();
  }
  if (!stableDropCaret.origin.isAttached()) {
    event.preventDefault();
    return true;
  }
  const dropSelection = $setSelectionFromCaretRange2($getCollapsedCaretRange2(stableDropCaret));
  $insertDataTransfer(dataTransfer, dropSelection, editor);
  if (!isSameEditorDrag) {
    const rootElement = editor.getRootElement();
    const doc = rootElement ? rootElement.ownerDocument : null;
    const sourceRoot = doc ? findEditorRootByKey(marker.editorKey, doc) : null;
    if (sourceRoot !== null) {
      sourceRoot.dispatchEvent(new InputEvent("beforeinput", {
        bubbles: true,
        cancelable: true,
        inputType: "deleteByDrag"
      }));
    }
  }
  event.preventDefault();
  return true;
}
function $handleRichTextDrop(event, editor) {
  return $doDrop(event, editor, $insertDataTransferForRichText);
}
function $handlePlainTextDrop(event, editor) {
  return $doDrop(event, editor, (dataTransfer, selection) => $insertDataTransferForPlainText(dataTransfer, selection));
}
function trustHTML(html) {
  if (window.trustedTypes && window.trustedTypes.createPolicy) {
    const policy = window.trustedTypes.createPolicy("lexical", {
      createHTML: (input) => input
    });
    return policy.createHTML(html);
  }
  return html;
}
function $insertGeneratedNodes(editor, nodes, selection) {
  if (!editor.dispatchCommand(SELECTION_INSERT_CLIPBOARD_NODES_COMMAND2, {
    nodes,
    selection
  })) {
    selection.insertNodes(nodes);
    $updateSelectionOnInsert(selection);
  }
  return;
}
function $updateSelectionOnInsert(selection) {
  if ($isRangeSelection2(selection) && selection.isCollapsed()) {
    const anchor = selection.anchor;
    let nodeToInspect = null;
    const anchorCaret = $caretFromPoint2(anchor, "previous");
    if (anchorCaret) {
      if ($isTextPointCaret2(anchorCaret)) {
        nodeToInspect = anchorCaret.origin;
      } else {
        const range = $getCaretRange2(anchorCaret, $getChildCaret2($getRoot2(), "next").getFlipped());
        for (const caret of range) {
          if ($isTextNode2(caret.origin)) {
            nodeToInspect = caret.origin;
            break;
          } else if ($isElementNode2(caret.origin) && !caret.origin.isInline()) {
            break;
          }
        }
      }
    }
    if (nodeToInspect && $isTextNode2(nodeToInspect)) {
      const newFormat = nodeToInspect.getFormat();
      const newStyle = nodeToInspect.getStyle();
      if (selection.format !== newFormat || selection.style !== newStyle) {
        selection.format = newFormat;
        selection.style = newStyle;
        selection.dirty = true;
      }
    }
  }
}
function exportNodeToJSON2(node) {
  const serializedNode = node.exportJSON();
  const nodeClass = node.constructor;
  if (serializedNode.type !== nodeClass.getType()) {
    {
      formatDevErrorMessage6(`LexicalNode: Node ${nodeClass.name} does not implement .exportJSON().`);
    }
  }
  if ($isElementNode2(node)) {
    const serializedChildren = serializedNode.children;
    if (!Array.isArray(serializedChildren)) {
      {
        formatDevErrorMessage6(`LexicalNode: Node ${nodeClass.name} is an element but .exportJSON() does not have a children array.`);
      }
    }
  }
  return serializedNode;
}
function $appendNodesToJSON(editor, selection, currentNode, targetArray = []) {
  let shouldInclude = selection !== null ? currentNode.isSelected(selection) : true;
  const shouldExclude = $isElementNode2(currentNode) && currentNode.excludeFromCopy("html");
  let target = currentNode;
  if (selection !== null && $isTextNode2(target)) {
    target = $sliceSelectedTextNodeContent2(selection, target, "clone");
  }
  const children = $isElementNode2(target) ? target.getChildren() : [];
  const serializedNode = exportNodeToJSON2(target);
  if ($isTextNode2(target) && target.getTextContentSize() === 0) {
    shouldInclude = false;
  }
  for (let i2 = 0; i2 < children.length; i2++) {
    const childNode = children[i2];
    const shouldIncludeChild = $appendNodesToJSON(editor, selection, childNode, serializedNode.children);
    if (!shouldInclude && $isElementNode2(currentNode) && shouldIncludeChild && currentNode.extractWithChild(childNode, selection, "clone")) {
      shouldInclude = true;
    }
  }
  if (shouldInclude && !shouldExclude) {
    targetArray.push(serializedNode);
  } else if (Array.isArray(serializedNode.children)) {
    for (let i2 = 0; i2 < serializedNode.children.length; i2++) {
      const serializedChildNode = serializedNode.children[i2];
      targetArray.push(serializedChildNode);
    }
  }
  return shouldInclude;
}
function $generateJSONFromSelectedNodes(editor, selection) {
  const nodes = [];
  const root = $getRoot2();
  const topLevelChildren = root.getChildren();
  for (let i2 = 0; i2 < topLevelChildren.length; i2++) {
    const topLevelNode = topLevelChildren[i2];
    $appendNodesToJSON(editor, selection, topLevelNode, nodes);
  }
  return {
    namespace: editor._config.namespace,
    nodes
  };
}
function $generateNodesFromSerializedNodes(serializedNodes) {
  const nodes = [];
  for (let i2 = 0; i2 < serializedNodes.length; i2++) {
    const serializedNode = serializedNodes[i2];
    const node = $parseSerializedNode2(serializedNode);
    if ($isTextNode2(node)) {
      $addNodeStyle2(node);
    }
    nodes.push(node);
  }
  return nodes;
}
var EVENT_LATENCY = 50;
var clipboardEventTimeout = null;
async function copyToClipboard(editor, event, data) {
  if (clipboardEventTimeout !== null) {
    return false;
  }
  if (event !== null) {
    return new Promise((resolve, reject) => {
      editor.update(() => {
        resolve($copyToClipboardEvent(editor, event, data));
      });
    });
  }
  const rootElement = editor.getRootElement();
  const editorWindow = editor._window || window;
  const windowDocument = editorWindow.document;
  const domSelection = getDOMSelection2(editorWindow);
  if (rootElement === null || domSelection === null) {
    return false;
  }
  const element = windowDocument.createElement("span");
  element.style.position = "fixed";
  element.style.top = "-1000px";
  element.append(windowDocument.createTextNode("#"));
  rootElement.append(element);
  const range = new Range();
  range.setStart(element, 0);
  range.setEnd(element, 1);
  domSelection.removeAllRanges();
  domSelection.addRange(range);
  return new Promise((resolve, reject) => {
    const removeListener = editor.registerCommand(COPY_COMMAND2, (secondEvent) => {
      if (objectKlassEquals2(secondEvent, ClipboardEvent)) {
        removeListener();
        if (clipboardEventTimeout !== null) {
          editorWindow.clearTimeout(clipboardEventTimeout);
          clipboardEventTimeout = null;
        }
        resolve($copyToClipboardEvent(editor, secondEvent, data));
      }
      return true;
    }, COMMAND_PRIORITY_CRITICAL2);
    clipboardEventTimeout = editorWindow.setTimeout(() => {
      removeListener();
      clipboardEventTimeout = null;
      resolve(false);
    }, EVENT_LATENCY);
    windowDocument.execCommand("copy");
    element.remove();
  });
}
function $copyToClipboardEvent(editor, event, data) {
  if (data === void 0) {
    const domSelection = getDOMSelection2(editor._window);
    const selection = $getSelection2();
    if (!selection || selection.isCollapsed()) {
      return false;
    }
    if (!domSelection) {
      return false;
    }
    const anchorDOM = domSelection.anchorNode;
    const focusDOM = domSelection.focusNode;
    if (anchorDOM !== null && focusDOM !== null && !isSelectionWithinEditor2(editor, anchorDOM, focusDOM)) {
      return false;
    }
    data = $getClipboardDataFromSelection(selection);
  }
  event.preventDefault();
  const clipboardData = event.clipboardData;
  if (clipboardData === null) {
    return false;
  }
  setLexicalClipboardDataTransfer(clipboardData, data);
  return true;
}
var clipboardDataFunctions = [["text/html", $getHtmlContent], ["application/x-lexical-editor", $getLexicalContent]];
function $getClipboardDataFromSelection(selection = $getSelection2()) {
  return $getClipboardDataWithConfigFromSelection($getExportConfig(), selection);
}
function setLexicalClipboardDataTransfer(clipboardData, data) {
  for (const [k] of clipboardDataFunctions) {
    if (data[k] === void 0) {
      clipboardData.setData(k, "");
    }
  }
  for (const k in data) {
    const v2 = data[k];
    if (v2 !== void 0) {
      clipboardData.setData(k, v2);
    }
  }
}
function $getExportConfig() {
  const editor = $getEditor2();
  const builder = LexicalBuilder2.maybeFromEditor(editor);
  if (builder && builder.hasExtensionByName(GetClipboardDataExtension.name)) {
    return getExtensionDependencyFromEditor2(editor, GetClipboardDataExtension).output;
  }
  return DEFAULT_EXPORT_MIME_TYPE;
}
var DEFAULT_EXPORT_MIME_TYPE = {
  "application/x-lexical-editor": [(sel, next) => sel ? $getLexicalContent($getEditor2(), sel) : next()],
  "text/html": [(sel, next) => sel ? $getHtmlContent($getEditor2(), sel) : next()],
  "text/plain": [(sel, next) => sel ? sel.getTextContent() : next()]
};
function $getClipboardDataWithConfigFromSelection($exportMimeType, selection) {
  const clipboardData = {
    "text/plain": ""
  };
  for (const [k, fns] of Object.entries($exportMimeType)) {
    const v2 = callExportMimeTypeFunctionStack(fns, selection);
    if (v2 !== null) {
      clipboardData[k] = v2;
    }
  }
  return clipboardData;
}
function callExportMimeTypeFunctionStack(fns, selection) {
  const callAt = (i2) => fns[i2] ? fns[i2](selection, callAt.bind(null, i2 - 1)) : null;
  return callAt(fns.length - 1);
}
var GetClipboardDataExtension = defineExtension2({
  build(editor, config, state) {
    return config.$exportMimeType;
  },
  config: safeCast2({
    $exportMimeType: DEFAULT_EXPORT_MIME_TYPE
  }),
  mergeConfig(config, partial) {
    const merged = shallowMergeConfig2(config, partial);
    if (partial.$exportMimeType) {
      const $exportMimeType = {
        ...config.$exportMimeType
      };
      for (const [k, v2] of Object.entries(partial.$exportMimeType)) {
        $exportMimeType[k] = [...$exportMimeType[k], ...v2];
      }
      merged.$exportMimeType = $exportMimeType;
    }
    return merged;
  },
  name: "@lexical/clipboard/GetClipboardData"
});

// node_modules/@lexical/clipboard/LexicalClipboard.mjs
var mod6 = true ? LexicalClipboard_dev_exports : LexicalClipboard_prod_exports;
var $generateJSONFromSelectedNodes2 = mod6.$generateJSONFromSelectedNodes;
var $generateNodesFromSerializedNodes2 = mod6.$generateNodesFromSerializedNodes;
var $getClipboardDataFromSelection2 = mod6.$getClipboardDataFromSelection;
var $getHtmlContent2 = mod6.$getHtmlContent;
var $getLexicalContent2 = mod6.$getLexicalContent;
var $handlePlainTextDrop2 = mod6.$handlePlainTextDrop;
var $handleRichTextDrop2 = mod6.$handleRichTextDrop;
var $insertDataTransferForPlainText2 = mod6.$insertDataTransferForPlainText;
var $insertDataTransferForRichText2 = mod6.$insertDataTransferForRichText;
var $insertGeneratedNodes2 = mod6.$insertGeneratedNodes;
var $writeDragSourceToDataTransfer2 = mod6.$writeDragSourceToDataTransfer;
var copyToClipboard2 = mod6.copyToClipboard;
var setLexicalClipboardDataTransfer2 = mod6.setLexicalClipboardDataTransfer;

// node_modules/@lexical/dragon/LexicalDragon.dev.mjs
var LexicalDragon_dev_exports = {};
__export(LexicalDragon_dev_exports, {
  DragonExtension: () => DragonExtension,
  registerDragonSupport: () => registerDragonSupport
});
function registerDragonSupport(editor) {
  const origin = window.location.origin;
  const handler = (event) => {
    if (event.origin !== origin) {
      return;
    }
    const rootElement = editor.getRootElement();
    if (document.activeElement !== rootElement) {
      return;
    }
    const data = event.data;
    if (typeof data === "string") {
      let parsedData;
      try {
        parsedData = JSON.parse(data);
      } catch (_e) {
        return;
      }
      if (parsedData && parsedData.protocol === "nuanria_messaging" && parsedData.type === "request") {
        const payload = parsedData.payload;
        if (payload && payload.functionId === "makeChanges") {
          const args = payload.args;
          if (args) {
            const [elementStart, elementLength, text, selStart, selLength] = args;
            editor.update(() => {
              const selection = $getSelection2();
              if ($isRangeSelection2(selection)) {
                const anchor = selection.anchor;
                let anchorNode = anchor.getNode();
                let setSelStart = 0;
                let setSelEnd = 0;
                if ($isTextNode2(anchorNode)) {
                  if (elementStart >= 0 && elementLength >= 0) {
                    setSelStart = elementStart;
                    setSelEnd = elementStart + elementLength;
                    selection.setTextNodeRange(anchorNode, setSelStart, anchorNode, setSelEnd);
                  }
                }
                if (setSelStart !== setSelEnd || text !== "") {
                  selection.insertRawText(text);
                  anchorNode = anchor.getNode();
                }
                if ($isTextNode2(anchorNode)) {
                  setSelStart = selStart;
                  setSelEnd = selStart + selLength;
                  const anchorNodeTextLength = anchorNode.getTextContentSize();
                  setSelStart = setSelStart > anchorNodeTextLength ? anchorNodeTextLength : setSelStart;
                  setSelEnd = setSelEnd > anchorNodeTextLength ? anchorNodeTextLength : setSelEnd;
                  selection.setTextNodeRange(anchorNode, setSelStart, anchorNode, setSelEnd);
                }
                event.stopImmediatePropagation();
              }
            });
          }
        }
      }
    }
  };
  window.addEventListener("message", handler, true);
  return () => {
    window.removeEventListener("message", handler, true);
  };
}
var DragonExtension = defineExtension2({
  build: (editor, config, state) => namedSignals2(config),
  config: safeCast2({
    disabled: typeof window === "undefined"
  }),
  name: "@lexical/dragon",
  register: (editor, config, state) => effect(() => state.getOutput().disabled.value ? void 0 : registerDragonSupport(editor))
});

// node_modules/@lexical/dragon/LexicalDragon.mjs
var mod7 = true ? LexicalDragon_dev_exports : LexicalDragon_prod_exports;
var DragonExtension2 = mod7.DragonExtension;
var registerDragonSupport2 = mod7.registerDragonSupport;

// node_modules/@lexical/rich-text/LexicalRichText.dev.mjs
function caretFromPoint2(x2, y2) {
  if (typeof document.caretRangeFromPoint !== "undefined") {
    const range = document.caretRangeFromPoint(x2, y2);
    if (range === null) {
      return null;
    }
    return {
      node: range.startContainer,
      offset: range.startOffset
    };
  } else if (document.caretPositionFromPoint !== "undefined") {
    const range = document.caretPositionFromPoint(x2, y2);
    if (range === null) {
      return null;
    }
    return {
      node: range.offsetNode,
      offset: range.offset
    };
  } else {
    return null;
  }
}
var CAN_USE_DOM4 = typeof window !== "undefined" && typeof window.document !== "undefined" && typeof window.document.createElement !== "undefined";
var documentMode3 = CAN_USE_DOM4 && "documentMode" in document ? document.documentMode : null;
var IS_APPLE4 = CAN_USE_DOM4 && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
var CAN_USE_BEFORE_INPUT4 = CAN_USE_DOM4 && "InputEvent" in window && !documentMode3 ? "getTargetRanges" in new window.InputEvent("input") : false;
var IS_IOS4 = CAN_USE_DOM4 && /iPad|iPhone|iPod/.test(navigator.userAgent) && !window.MSStream;
var IS_ANDROID4 = CAN_USE_DOM4 && /Android/.test(navigator.userAgent);
var IS_SAFARI4 = CAN_USE_DOM4 && /Version\/[\d.]+.*Safari/.test(navigator.userAgent) && !IS_ANDROID4;
var IS_CHROME4 = CAN_USE_DOM4 && /^(?=.*Chrome).*/i.test(navigator.userAgent);
var IS_APPLE_WEBKIT4 = CAN_USE_DOM4 && /AppleWebKit\/[\d.]+/.test(navigator.userAgent) && IS_APPLE4 && !IS_CHROME4;
var DRAG_DROP_PASTE = createCommand2("DRAG_DROP_PASTE_FILE");
var QuoteNode = class _QuoteNode extends ElementNode2 {
  static getType() {
    return "quote";
  }
  static clone(node) {
    return new _QuoteNode(node.__key);
  }
  // View
  createDOM(config) {
    const element = document.createElement("blockquote");
    addClassNamesToElement3(element, config.theme.quote);
    return element;
  }
  updateDOM(prevNode, dom) {
    return false;
  }
  static importDOM() {
    return {
      blockquote: (node) => ({
        conversion: $convertBlockquoteElement,
        priority: 0
      })
    };
  }
  exportDOM(editor) {
    const {
      element
    } = super.exportDOM(editor);
    if (isHTMLElement3(element)) {
      if (this.isEmpty()) {
        element.append(document.createElement("br"));
      }
      const formatType = this.getFormatType();
      if (formatType) {
        element.style.textAlign = formatType;
      }
      const direction = this.getDirection();
      if (direction) {
        element.dir = direction;
      }
    }
    return {
      element
    };
  }
  static importJSON(serializedNode) {
    return $createQuoteNode().updateFromJSON(serializedNode);
  }
  // Mutation
  insertNewAfter(_2, restoreSelection) {
    const newBlock = $createParagraphNode2();
    const direction = this.getDirection();
    newBlock.setDirection(direction);
    this.insertAfter(newBlock, restoreSelection);
    return newBlock;
  }
  collapseAtStart() {
    const paragraph = $createParagraphNode2();
    const children = this.getChildren();
    children.forEach((child) => paragraph.append(child));
    this.replace(paragraph);
    return true;
  }
  canMergeWhenEmpty() {
    return true;
  }
};
function $createQuoteNode() {
  return $applyNodeReplacement2(new QuoteNode());
}
function $isQuoteNode(node) {
  return node instanceof QuoteNode;
}
var HeadingNode = class _HeadingNode extends ElementNode2 {
  /** @internal */
  __tag;
  static getType() {
    return "heading";
  }
  static clone(node) {
    return new _HeadingNode(node.__tag, node.__key);
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__tag = prevNode.__tag;
  }
  constructor(tag = "h1", key) {
    super(key);
    this.__tag = tag;
  }
  getTag() {
    return this.getLatest().__tag;
  }
  setTag(tag) {
    const self2 = this.getWritable();
    self2.__tag = tag;
    return self2;
  }
  // View
  createDOM(config) {
    const tag = this.__tag;
    const element = document.createElement(tag);
    const theme = config.theme;
    const classNames = theme.heading;
    if (classNames !== void 0) {
      const className = classNames[tag];
      addClassNamesToElement3(element, className);
    }
    return element;
  }
  updateDOM(prevNode, dom, config) {
    return prevNode.__tag !== this.__tag;
  }
  static importDOM() {
    return {
      h1: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      h2: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      h3: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      h4: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      h5: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      h6: (node) => ({
        conversion: $convertHeadingElement,
        priority: 0
      }),
      p: (node) => {
        const paragraph = node;
        const firstChild = paragraph.firstChild;
        if (firstChild !== null && isGoogleDocsTitle(firstChild)) {
          return {
            conversion: () => ({
              node: null
            }),
            priority: 3
          };
        }
        return null;
      },
      span: (node) => {
        if (isGoogleDocsTitle(node)) {
          return {
            conversion: (domNode) => {
              return {
                node: $createHeadingNode("h1")
              };
            },
            priority: 3
          };
        }
        return null;
      }
    };
  }
  exportDOM(editor) {
    const {
      element
    } = super.exportDOM(editor);
    if (isHTMLElement3(element)) {
      if (this.isEmpty()) {
        element.append(document.createElement("br"));
      }
      const formatType = this.getFormatType();
      if (formatType) {
        element.style.textAlign = formatType;
      }
      const direction = this.getDirection();
      if (direction) {
        element.dir = direction;
      }
    }
    return {
      element
    };
  }
  static importJSON(serializedNode) {
    return $createHeadingNode(serializedNode.tag).updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setTag(serializedNode.tag);
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      tag: this.getTag()
    };
  }
  // Mutation
  insertNewAfter(selection, restoreSelection = true) {
    const anchorOffet = selection ? selection.anchor.offset : 0;
    const lastDesc = this.getLastDescendant();
    const isAtEnd = !lastDesc || selection && selection.anchor.key === lastDesc.getKey() && anchorOffet === lastDesc.getTextContentSize();
    const newElement = isAtEnd || !selection ? $createParagraphNode2() : $createHeadingNode(this.getTag());
    const direction = this.getDirection();
    newElement.setDirection(direction);
    this.insertAfter(newElement, restoreSelection);
    if (anchorOffet === 0 && !this.isEmpty() && selection) {
      const paragraph = $createParagraphNode2();
      paragraph.select();
      this.replace(paragraph, true);
    }
    return newElement;
  }
  collapseAtStart() {
    const newElement = !this.isEmpty() ? $createHeadingNode(this.getTag()) : $createParagraphNode2();
    const children = this.getChildren();
    children.forEach((child) => newElement.append(child));
    this.replace(newElement);
    return true;
  }
  extractWithChild() {
    return true;
  }
};
function isGoogleDocsTitle(domNode) {
  if (domNode.nodeName.toLowerCase() === "span") {
    return domNode.style.fontSize === "26pt";
  }
  return false;
}
function $convertHeadingElement(element) {
  const nodeName = element.nodeName.toLowerCase();
  let node = null;
  if (nodeName === "h1" || nodeName === "h2" || nodeName === "h3" || nodeName === "h4" || nodeName === "h5" || nodeName === "h6") {
    node = $createHeadingNode(nodeName);
    if (element.style !== null) {
      setNodeIndentFromDOM2(element, node);
      node.setFormat(element.style.textAlign);
    }
  }
  return {
    node
  };
}
function $convertBlockquoteElement(element) {
  const node = $createQuoteNode();
  if (element.style !== null) {
    node.setFormat(element.style.textAlign);
    setNodeIndentFromDOM2(element, node);
  }
  return {
    node
  };
}
function $createHeadingNode(headingTag = "h1") {
  return $applyNodeReplacement2(new HeadingNode(headingTag));
}
function $isHeadingNode(node) {
  return node instanceof HeadingNode;
}
function onPasteForRichText(event, editor) {
  event.preventDefault();
  editor.update(() => {
    const selection = $getSelection2();
    const clipboardData = objectKlassEquals2(event, InputEvent) || objectKlassEquals2(event, KeyboardEvent) ? null : event.clipboardData;
    if (clipboardData != null && selection !== null) {
      $insertDataTransferForRichText2(clipboardData, selection, editor);
    }
  }, {
    tag: PASTE_TAG2
  });
}
async function onCutForRichText(event, editor) {
  await copyToClipboard2(editor, objectKlassEquals2(event, ClipboardEvent) ? event : null);
  editor.update(() => {
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      selection.removeText();
    } else if ($isNodeSelection2(selection)) {
      selection.getNodes().forEach((node) => node.remove());
    }
  });
}
function eventFiles(event) {
  let dataTransfer = null;
  if (objectKlassEquals2(event, DragEvent)) {
    dataTransfer = event.dataTransfer;
  } else if (objectKlassEquals2(event, ClipboardEvent)) {
    dataTransfer = event.clipboardData;
  }
  if (dataTransfer === null) {
    return [false, [], false];
  }
  const types = dataTransfer.types;
  const hasFiles = types.includes("Files");
  const hasContent = types.includes("text/html") || types.includes("text/plain");
  return [hasFiles, Array.from(dataTransfer.files), hasContent];
}
function $isTargetWithinDecorator(target) {
  const node = $getNearestNodeFromDOMNode2(target);
  return $isDecoratorNode2(node);
}
function $isSelectionAtEndOfRoot(selection) {
  const focus = selection.focus;
  return focus.key === "root" && focus.offset === $getRoot2().getChildrenSize();
}
function $isSelectionCollapsedAtFrontOfIndentedBlock(selection) {
  if (!selection.isCollapsed()) {
    return false;
  }
  const {
    anchor
  } = selection;
  if (anchor.offset !== 0) {
    return false;
  }
  const anchorNode = anchor.getNode();
  if ($isRootNode2(anchorNode)) {
    return false;
  }
  const element = $getNearestBlockElementAncestorOrThrow2(anchorNode);
  return element.getIndent() > 0 && (element.is(anchorNode) || anchorNode.is(element.getFirstDescendant()));
}
function $resetCapitalization(selection) {
  for (const format of ["lowercase", "uppercase", "capitalize"]) {
    if (selection.hasFormat(format)) {
      selection.toggleFormat(format);
    }
  }
}
function registerRichText(editor) {
  const removeListener = mergeRegister3(editor.registerCommand(CLICK_COMMAND2, (payload) => {
    const selection = $getSelection2();
    if ($isNodeSelection2(selection)) {
      selection.clear();
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DELETE_CHARACTER_COMMAND2, (isBackward) => {
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      selection.deleteCharacter(isBackward);
      return true;
    } else if ($isNodeSelection2(selection)) {
      selection.deleteNodes();
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DELETE_WORD_COMMAND2, (isBackward) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.deleteWord(isBackward);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DELETE_LINE_COMMAND2, (isBackward) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.deleteLine(isBackward);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(CONTROLLED_TEXT_INSERTION_COMMAND2, (eventOrText) => {
    const selection = $getSelection2();
    if (typeof eventOrText === "string") {
      if (selection !== null) {
        selection.insertText(eventOrText);
      }
    } else {
      if (selection === null) {
        return false;
      }
      const dataTransfer = eventOrText.dataTransfer;
      if (dataTransfer != null) {
        $insertDataTransferForRichText2(dataTransfer, selection, editor);
      } else if ($isRangeSelection2(selection)) {
        const data = eventOrText.data;
        if (data) {
          selection.insertText(data);
        }
        return true;
      }
    }
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(REMOVE_TEXT_COMMAND2, () => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.removeText();
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(FORMAT_TEXT_COMMAND2, (format) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.formatText(format);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(FORMAT_ELEMENT_COMMAND2, (format) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection) && !$isNodeSelection2(selection)) {
      return false;
    }
    const nodes = selection.getNodes();
    for (const node of nodes) {
      const element = $findMatchingParent3(node, (parentNode) => $isElementNode2(parentNode) && !parentNode.isInline());
      if (element !== null) {
        element.setFormat(format);
      }
    }
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(INSERT_LINE_BREAK_COMMAND2, (selectStart) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.insertLineBreak(selectStart);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(INSERT_PARAGRAPH_COMMAND2, () => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    selection.insertParagraph();
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(INSERT_TAB_COMMAND2, () => {
    const tabNode = $createTabNode2();
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      tabNode.setFormat(selection.format);
      tabNode.setStyle(selection.style);
    }
    $insertNodes2([tabNode]);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(INDENT_CONTENT_COMMAND2, () => {
    return $handleIndentAndOutdent2((block) => {
      const indent = block.getIndent();
      block.setIndent(indent + 1);
    });
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(OUTDENT_CONTENT_COMMAND2, () => {
    return $handleIndentAndOutdent2((block) => {
      const indent = block.getIndent();
      if (indent > 0) {
        block.setIndent(Math.max(0, indent - 1));
      }
    });
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ARROW_UP_COMMAND2, (event) => {
    const selection = $getSelection2();
    if ($isNodeSelection2(selection)) {
      const nodes = selection.getNodes();
      if (nodes.length > 0) {
        event.preventDefault();
        nodes[0].selectPrevious();
        return true;
      }
    } else if ($isRangeSelection2(selection)) {
      const possibleNode = $getAdjacentNode2(selection.focus, true);
      if (!event.shiftKey && $isDecoratorNode2(possibleNode) && !possibleNode.isIsolated() && !possibleNode.isInline()) {
        possibleNode.selectPrevious();
        event.preventDefault();
        return true;
      }
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ARROW_DOWN_COMMAND2, (event) => {
    const selection = $getSelection2();
    if ($isNodeSelection2(selection)) {
      const nodes = selection.getNodes();
      if (nodes.length > 0) {
        event.preventDefault();
        nodes[0].selectNext(0, 0);
        return true;
      }
    } else if ($isRangeSelection2(selection)) {
      if ($isSelectionAtEndOfRoot(selection)) {
        event.preventDefault();
        return true;
      }
      const possibleNode = $getAdjacentNode2(selection.focus, false);
      if (!event.shiftKey && $isDecoratorNode2(possibleNode) && !possibleNode.isIsolated() && !possibleNode.isInline()) {
        possibleNode.selectNext();
        event.preventDefault();
        return true;
      }
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ARROW_LEFT_COMMAND2, (event) => {
    const selection = $getSelection2();
    if ($isNodeSelection2(selection)) {
      const nodes = selection.getNodes();
      if (nodes.length > 0) {
        event.preventDefault();
        if ($isParentRTL2(nodes[0])) {
          nodes[0].selectNext(0, 0);
        } else {
          nodes[0].selectPrevious();
        }
        return true;
      }
    }
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    if ($shouldOverrideDefaultCharacterSelection2(selection, true)) {
      const isHoldingShift = event.shiftKey;
      event.preventDefault();
      $moveCharacter2(selection, isHoldingShift, true);
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ARROW_RIGHT_COMMAND2, (event) => {
    const selection = $getSelection2();
    if ($isNodeSelection2(selection)) {
      const nodes = selection.getNodes();
      if (nodes.length > 0) {
        event.preventDefault();
        if ($isParentRTL2(nodes[0])) {
          nodes[0].selectPrevious();
        } else {
          nodes[0].selectNext(0, 0);
        }
        return true;
      }
    }
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    const isHoldingShift = event.shiftKey;
    if ($shouldOverrideDefaultCharacterSelection2(selection, false)) {
      event.preventDefault();
      $moveCharacter2(selection, isHoldingShift, false);
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_BACKSPACE_COMMAND2, (event) => {
    if ($isTargetWithinDecorator(event.target)) {
      return false;
    }
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      if ($isSelectionCollapsedAtFrontOfIndentedBlock(selection)) {
        event.preventDefault();
        return editor.dispatchCommand(OUTDENT_CONTENT_COMMAND2, void 0);
      }
      if (IS_IOS4 && navigator.language === "ko-KR") {
        return false;
      }
    } else if (!$isNodeSelection2(selection)) {
      return false;
    }
    event.preventDefault();
    return editor.dispatchCommand(DELETE_CHARACTER_COMMAND2, true);
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_DELETE_COMMAND2, (event) => {
    if ($isTargetWithinDecorator(event.target)) {
      return false;
    }
    const selection = $getSelection2();
    if (!($isRangeSelection2(selection) || $isNodeSelection2(selection))) {
      return false;
    }
    event.preventDefault();
    return editor.dispatchCommand(DELETE_CHARACTER_COMMAND2, false);
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ENTER_COMMAND2, (event) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    $resetCapitalization(selection);
    if (event !== null) {
      if ((IS_IOS4 || IS_SAFARI4 || IS_APPLE_WEBKIT4) && CAN_USE_BEFORE_INPUT4) {
        return false;
      }
      event.preventDefault();
      if (event.shiftKey) {
        return editor.dispatchCommand(INSERT_LINE_BREAK_COMMAND2, false);
      }
    }
    return editor.dispatchCommand(INSERT_PARAGRAPH_COMMAND2, void 0);
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_ESCAPE_COMMAND2, () => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    editor.blur();
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DROP_COMMAND2, (event) => {
    const [, files] = eventFiles(event);
    if (files.length > 0) {
      const x2 = event.clientX;
      const y2 = event.clientY;
      const eventRange = caretFromPoint2(x2, y2);
      if (eventRange !== null) {
        const {
          offset: domOffset,
          node: domNode
        } = eventRange;
        const node = $getNearestNodeFromDOMNode2(domNode);
        if (node !== null) {
          const selection = $createRangeSelection2();
          if ($isTextNode2(node)) {
            selection.anchor.set(node.getKey(), domOffset, "text");
            selection.focus.set(node.getKey(), domOffset, "text");
          } else {
            const parentKey = node.getParentOrThrow().getKey();
            const offset = node.getIndexWithinParent() + 1;
            selection.anchor.set(parentKey, offset, "element");
            selection.focus.set(parentKey, offset, "element");
          }
          const normalizedSelection = $normalizeSelection__EXPERIMENTAL(selection);
          $setSelection2(normalizedSelection);
        }
        editor.dispatchCommand(DRAG_DROP_PASTE, files);
      }
      event.preventDefault();
      return true;
    }
    return $handleRichTextDrop2(event, editor);
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DRAGSTART_COMMAND2, (event) => {
    const [isFileTransfer] = eventFiles(event);
    const selection = $getSelection2();
    if (isFileTransfer && !$isRangeSelection2(selection)) {
      return false;
    }
    if ($isRangeSelection2(selection) && !selection.isCollapsed() && event.dataTransfer !== null) {
      setLexicalClipboardDataTransfer2(event.dataTransfer, $getClipboardDataFromSelection2(selection));
      $writeDragSourceToDataTransfer2(event.dataTransfer, editor);
    }
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(DRAGOVER_COMMAND2, (event) => {
    const [isFileTransfer] = eventFiles(event);
    const selection = $getSelection2();
    if (isFileTransfer && !$isRangeSelection2(selection)) {
      return false;
    }
    const x2 = event.clientX;
    const y2 = event.clientY;
    const eventRange = caretFromPoint2(x2, y2);
    if (eventRange !== null) {
      const node = $getNearestNodeFromDOMNode2(eventRange.node);
      if ($isDecoratorNode2(node)) {
        event.preventDefault();
      }
    }
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(SELECT_ALL_COMMAND2, () => {
    $selectAll2();
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(COPY_COMMAND2, (event) => {
    copyToClipboard2(editor, objectKlassEquals2(event, ClipboardEvent) ? event : null);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(CUT_COMMAND2, (event) => {
    onCutForRichText(event, editor);
    return true;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(PASTE_COMMAND2, (event) => {
    const [, files, hasTextContent] = eventFiles(event);
    if (files.length > 0 && !hasTextContent) {
      editor.dispatchCommand(DRAG_DROP_PASTE, files);
      return true;
    }
    if (isDOMNode2(event.target) && isSelectionCapturedInDecoratorInput2(event.target)) {
      return false;
    }
    const selection = $getSelection2();
    if (selection !== null) {
      onPasteForRichText(event, editor);
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_SPACE_COMMAND2, (_2) => {
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      $resetCapitalization(selection);
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2), editor.registerCommand(KEY_TAB_COMMAND2, (_2) => {
    const selection = $getSelection2();
    if ($isRangeSelection2(selection)) {
      $resetCapitalization(selection);
    }
    return false;
  }, COMMAND_PRIORITY_EDITOR2));
  return removeListener;
}
var RichTextExtension = defineExtension2({
  conflictsWith: ["@lexical/plain-text"],
  dependencies: [DragonExtension2],
  name: "@lexical/rich-text",
  nodes: () => [HeadingNode, QuoteNode],
  register: registerRichText
});

// node_modules/@lexical/rich-text/LexicalRichText.mjs
var mod8 = true ? LexicalRichText_dev_exports : LexicalRichText_prod_exports;
var $createHeadingNode2 = mod8.$createHeadingNode;
var $createQuoteNode2 = mod8.$createQuoteNode;
var $isHeadingNode2 = mod8.$isHeadingNode;
var $isQuoteNode2 = mod8.$isQuoteNode;
var DRAG_DROP_PASTE2 = mod8.DRAG_DROP_PASTE;
var HeadingNode2 = mod8.HeadingNode;
var QuoteNode2 = mod8.QuoteNode;
var RichTextExtension2 = mod8.RichTextExtension;
var eventFiles2 = mod8.eventFiles;
var registerRichText2 = mod8.registerRichText;

// node_modules/@lexical/list/LexicalList.dev.mjs
var LexicalList_dev_exports = {};
__export(LexicalList_dev_exports, {
  $createListItemNode: () => $createListItemNode,
  $createListNode: () => $createListNode,
  $getListDepth: () => $getListDepth,
  $handleListInsertParagraph: () => $handleListInsertParagraph,
  $insertList: () => $insertList,
  $isListItemNode: () => $isListItemNode,
  $isListNode: () => $isListNode,
  $removeList: () => $removeList,
  CheckListExtension: () => CheckListExtension,
  INSERT_CHECK_LIST_COMMAND: () => INSERT_CHECK_LIST_COMMAND,
  INSERT_ORDERED_LIST_COMMAND: () => INSERT_ORDERED_LIST_COMMAND,
  INSERT_UNORDERED_LIST_COMMAND: () => INSERT_UNORDERED_LIST_COMMAND,
  ListExtension: () => ListExtension,
  ListItemNode: () => ListItemNode,
  ListNode: () => ListNode,
  REMOVE_LIST_COMMAND: () => REMOVE_LIST_COMMAND,
  UPDATE_LIST_START_COMMAND: () => UPDATE_LIST_START_COMMAND,
  insertList: () => insertList,
  registerCheckList: () => registerCheckList,
  registerList: () => registerList,
  registerListStrictIndentTransform: () => registerListStrictIndentTransform,
  removeList: () => removeList
});
function formatDevErrorMessage7(message) {
  throw new Error(message);
}
function $getListDepth(listNode) {
  let depth = 1;
  let parent = listNode.getParent();
  while (parent != null) {
    if ($isListItemNode(parent)) {
      const parentList = parent.getParent();
      if ($isListNode(parentList)) {
        depth++;
        parent = parentList.getParent();
        continue;
      }
      {
        formatDevErrorMessage7(`A ListItemNode must have a ListNode for a parent.`);
      }
    }
    return depth;
  }
  return depth;
}
function $getTopListNode(listItem) {
  let list = listItem.getParent();
  if (!$isListNode(list)) {
    {
      formatDevErrorMessage7(`A ListItemNode must have a ListNode for a parent.`);
    }
  }
  let parent = list;
  while (parent !== null) {
    parent = parent.getParent();
    if ($isListNode(parent)) {
      list = parent;
    }
  }
  return list;
}
function $getAllListItems(node) {
  let listItemNodes = [];
  const listChildren = node.getChildren().filter($isListItemNode);
  for (let i2 = 0; i2 < listChildren.length; i2++) {
    const listItemNode = listChildren[i2];
    const firstChild = listItemNode.getFirstChild();
    if ($isListNode(firstChild)) {
      listItemNodes = listItemNodes.concat($getAllListItems(firstChild));
    } else {
      listItemNodes.push(listItemNode);
    }
  }
  return listItemNodes;
}
function isNestedListNode(node) {
  return $isListItemNode(node) && $isListNode(node.getFirstChild());
}
function $removeHighestEmptyListParent(sublist) {
  let emptyListPtr = sublist;
  while (emptyListPtr.getNextSibling() == null && emptyListPtr.getPreviousSibling() == null) {
    const parent = emptyListPtr.getParent();
    if (parent == null || !($isListItemNode(parent) || $isListNode(parent))) {
      break;
    }
    emptyListPtr = parent;
  }
  emptyListPtr.remove();
}
function $wrapInListItem(node) {
  const listItemWrapper = $createListItemNode();
  return listItemWrapper.append(node);
}
function $getNewListStart(list, listItem) {
  return list.getStart() + listItem.getIndexWithinParent();
}
function $isSelectingEmptyListItem(anchorNode, nodes) {
  return $isListItemNode(anchorNode) && (nodes.length === 0 || nodes.length === 1 && anchorNode.is(nodes[0]) && anchorNode.getChildrenSize() === 0);
}
function $insertList(listType) {
  const selection = $getSelection2();
  if (selection !== null) {
    let nodes = selection.getNodes();
    if ($isRangeSelection2(selection)) {
      const anchorAndFocus = selection.getStartEndPoints();
      if (!(anchorAndFocus !== null)) {
        formatDevErrorMessage7(`insertList: anchor should be defined`);
      }
      const [anchor] = anchorAndFocus;
      const anchorNode = anchor.getNode();
      const anchorNodeParent = anchorNode.getParent();
      if ($isRootOrShadowRoot2(anchorNode)) {
        const firstChild = anchorNode.getFirstChild();
        if (firstChild) {
          nodes = firstChild.selectStart().getNodes();
        } else {
          const paragraph = $createParagraphNode2();
          anchorNode.append(paragraph);
          nodes = paragraph.select().getNodes();
        }
      } else if ($isSelectingEmptyListItem(anchorNode, nodes)) {
        const list = $createListNode(listType);
        if ($isRootOrShadowRoot2(anchorNodeParent)) {
          anchorNode.replace(list);
          const listItem = $createListItemNode();
          if ($isElementNode2(anchorNode)) {
            listItem.setFormat(anchorNode.getFormatType());
            listItem.setIndent(anchorNode.getIndent());
          }
          list.append(listItem);
        } else if ($isListItemNode(anchorNode)) {
          const parent = anchorNode.getParentOrThrow();
          append(list, parent.getChildren());
          parent.replace(list);
        }
        return;
      }
    }
    const handled = /* @__PURE__ */ new Set();
    for (let i2 = 0; i2 < nodes.length; i2++) {
      const node = nodes[i2];
      if ($isElementNode2(node) && node.isEmpty() && !$isListItemNode(node) && !handled.has(node.getKey())) {
        $createListOrMerge(node, listType);
        continue;
      }
      let parent = $isLeafNode2(node) ? node.getParent() : $isListItemNode(node) && node.isEmpty() ? node : null;
      while (parent != null) {
        const parentKey = parent.getKey();
        if ($isListNode(parent)) {
          if (!handled.has(parentKey)) {
            const newListNode = $createListNode(listType);
            append(newListNode, parent.getChildren());
            parent.replace(newListNode);
            handled.add(parentKey);
          }
          break;
        } else {
          const nextParent = parent.getParent();
          if ($isRootOrShadowRoot2(nextParent) && !handled.has(parentKey)) {
            handled.add(parentKey);
            $createListOrMerge(parent, listType);
            break;
          }
          parent = nextParent;
        }
      }
    }
  }
}
function append(node, nodesToAppend) {
  node.splice(node.getChildrenSize(), 0, nodesToAppend);
}
function $createListOrMerge(node, listType) {
  if ($isListNode(node)) {
    return node;
  }
  const previousSibling = node.getPreviousSibling();
  const nextSibling = node.getNextSibling();
  const listItem = $createListItemNode();
  append(listItem, node.getChildren());
  let targetList;
  if ($isListNode(previousSibling) && listType === previousSibling.getListType()) {
    previousSibling.append(listItem);
    if ($isListNode(nextSibling) && listType === nextSibling.getListType()) {
      append(previousSibling, nextSibling.getChildren());
      nextSibling.remove();
    }
    targetList = previousSibling;
  } else if ($isListNode(nextSibling) && listType === nextSibling.getListType()) {
    nextSibling.getFirstChildOrThrow().insertBefore(listItem);
    targetList = nextSibling;
  } else {
    const list = $createListNode(listType);
    list.append(listItem);
    node.replace(list);
    targetList = list;
  }
  listItem.setFormat(node.getFormatType());
  listItem.setIndent(node.getIndent());
  const selection = $getSelection2();
  if ($isRangeSelection2(selection)) {
    if (targetList.getKey() === selection.anchor.key) {
      selection.anchor.set(listItem.getKey(), selection.anchor.offset, "element");
    }
    if (targetList.getKey() === selection.focus.key) {
      selection.focus.set(listItem.getKey(), selection.focus.offset, "element");
    }
  }
  node.remove();
  return targetList;
}
function mergeLists(list1, list2) {
  const listItem1 = list1.getLastChild();
  const listItem2 = list2.getFirstChild();
  if (listItem1 && listItem2 && isNestedListNode(listItem1) && isNestedListNode(listItem2)) {
    mergeLists(listItem1.getFirstChild(), listItem2.getFirstChild());
    listItem2.remove();
  }
  const toMerge = list2.getChildren();
  if (toMerge.length > 0) {
    list1.append(...toMerge);
  }
  list2.remove();
}
function $removeList() {
  const selection = $getSelection2();
  if ($isRangeSelection2(selection)) {
    const listNodes = /* @__PURE__ */ new Set();
    const nodes = selection.getNodes();
    const anchorNode = selection.anchor.getNode();
    if ($isSelectingEmptyListItem(anchorNode, nodes)) {
      listNodes.add($getTopListNode(anchorNode));
    } else {
      for (let i2 = 0; i2 < nodes.length; i2++) {
        const node = nodes[i2];
        if ($isLeafNode2(node)) {
          const listItemNode = $getNearestNodeOfType2(node, ListItemNode);
          if (listItemNode != null) {
            listNodes.add($getTopListNode(listItemNode));
          }
        }
      }
    }
    for (const listNode of listNodes) {
      let insertionPoint = listNode;
      const listItems = $getAllListItems(listNode);
      for (const listItemNode of listItems) {
        const paragraph = $createParagraphNode2().setTextStyle(selection.style).setTextFormat(selection.format);
        append(paragraph, listItemNode.getChildren());
        insertionPoint.insertAfter(paragraph);
        insertionPoint = paragraph;
        if (listItemNode.__key === selection.anchor.key) {
          $setPointFromCaret2(selection.anchor, $normalizeCaret2($getChildCaret2(paragraph, "next")));
        }
        if (listItemNode.__key === selection.focus.key) {
          $setPointFromCaret2(selection.focus, $normalizeCaret2($getChildCaret2(paragraph, "next")));
        }
        listItemNode.remove();
      }
      listNode.remove();
    }
  }
}
function updateChildrenListItemValue(list) {
  const isNotChecklist = list.getListType() !== "check";
  let value = list.getStart();
  for (const child of list.getChildren()) {
    if ($isListItemNode(child)) {
      if (child.getValue() !== value) {
        child.setValue(value);
      }
      if (isNotChecklist && child.getLatest().__checked != null) {
        child.setChecked(void 0);
      }
      if (!$isListNode(child.getFirstChild())) {
        value++;
      }
    }
  }
}
function mergeNextSiblingListIfSameType(list) {
  const nextSibling = list.getNextSibling();
  if ($isListNode(nextSibling) && list.getListType() === nextSibling.getListType()) {
    mergeLists(list, nextSibling);
  }
}
function $handleIndent(listItemNode) {
  const removed = /* @__PURE__ */ new Set();
  if (isNestedListNode(listItemNode) || removed.has(listItemNode.getKey())) {
    return;
  }
  const parent = listItemNode.getParent();
  const nextSibling = listItemNode.getNextSibling();
  const previousSibling = listItemNode.getPreviousSibling();
  if (isNestedListNode(nextSibling) && isNestedListNode(previousSibling)) {
    const innerList = previousSibling.getFirstChild();
    if ($isListNode(innerList)) {
      innerList.append(listItemNode);
      const nextInnerList = nextSibling.getFirstChild();
      if ($isListNode(nextInnerList)) {
        const children = nextInnerList.getChildren();
        append(innerList, children);
        nextSibling.remove();
        removed.add(nextSibling.getKey());
      }
    }
  } else if (isNestedListNode(nextSibling)) {
    const innerList = nextSibling.getFirstChild();
    if ($isListNode(innerList)) {
      const firstChild = innerList.getFirstChild();
      if (firstChild !== null) {
        firstChild.insertBefore(listItemNode);
      }
    }
  } else if (isNestedListNode(previousSibling)) {
    const innerList = previousSibling.getFirstChild();
    if ($isListNode(innerList)) {
      innerList.append(listItemNode);
    }
  } else {
    if ($isListNode(parent)) {
      const newListItem = $copyNode2(listItemNode);
      const newList = $copyNode2(parent);
      newListItem.append(newList);
      newList.append(listItemNode);
      if (previousSibling) {
        previousSibling.insertAfter(newListItem);
      } else if (nextSibling) {
        nextSibling.insertBefore(newListItem);
      } else {
        parent.append(newListItem);
      }
    }
  }
}
function $handleOutdent(listItemNode) {
  if (isNestedListNode(listItemNode)) {
    return;
  }
  const parentList = listItemNode.getParent();
  const grandparentListItem = parentList ? parentList.getParent() : void 0;
  const greatGrandparentList = grandparentListItem ? grandparentListItem.getParent() : void 0;
  if ($isListNode(greatGrandparentList) && $isListItemNode(grandparentListItem) && $isListNode(parentList)) {
    const firstChild = parentList ? parentList.getFirstChild() : void 0;
    const lastChild = parentList ? parentList.getLastChild() : void 0;
    if (listItemNode.is(firstChild)) {
      grandparentListItem.insertBefore(listItemNode);
      if (parentList.isEmpty()) {
        grandparentListItem.remove();
      }
    } else if (listItemNode.is(lastChild)) {
      grandparentListItem.insertAfter(listItemNode);
      if (parentList.isEmpty()) {
        grandparentListItem.remove();
      }
    } else {
      const previousSiblingsListItem = $copyNode2(listItemNode);
      const previousSiblingsList = $copyNode2(parentList);
      previousSiblingsListItem.append(previousSiblingsList);
      listItemNode.getPreviousSiblings().forEach((sibling) => previousSiblingsList.append(sibling));
      const nextSiblingsListItem = $copyNode2(listItemNode);
      const nextSiblingsList = $copyNode2(parentList);
      nextSiblingsListItem.append(nextSiblingsList);
      append(nextSiblingsList, listItemNode.getNextSiblings());
      grandparentListItem.insertBefore(previousSiblingsListItem);
      grandparentListItem.insertAfter(nextSiblingsListItem);
      grandparentListItem.replace(listItemNode);
    }
  }
}
function $handleListInsertParagraph(restoreNumbering = false) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection) || !selection.isCollapsed()) {
    return false;
  }
  const anchor = selection.anchor.getNode();
  let listItem = null;
  if ($isListItemNode(anchor) && anchor.getChildrenSize() === 0) {
    listItem = anchor;
  } else if ($isTextNode2(anchor)) {
    const parentListItem = anchor.getParent();
    if ($isListItemNode(parentListItem) && parentListItem.getChildren().every((node) => $isTextNode2(node) && node.getTextContent().trim() === "")) {
      listItem = parentListItem;
    }
  }
  if (listItem === null) {
    return false;
  }
  const topListNode = $getTopListNode(listItem);
  const parent = listItem.getParent();
  if (!$isListNode(parent)) {
    formatDevErrorMessage7(`A ListItemNode must have a ListNode for a parent.`);
  }
  const grandparent = parent.getParent();
  let replacementNode;
  if ($isRootOrShadowRoot2(grandparent)) {
    replacementNode = $createParagraphNode2();
    topListNode.insertAfter(replacementNode);
  } else if ($isListItemNode(grandparent)) {
    replacementNode = $copyNode2(grandparent);
    grandparent.insertAfter(replacementNode);
  } else {
    return false;
  }
  replacementNode.setTextStyle(selection.style).setTextFormat(selection.format).select();
  const nextSiblings = listItem.getNextSiblings();
  if (nextSiblings.length > 0) {
    const newStart = restoreNumbering ? $getNewListStart(parent, listItem) : 1;
    const newList = $copyNode2(parent).setStart(newStart);
    if ($isListItemNode(replacementNode)) {
      const newListItem = $copyNode2(replacementNode);
      newListItem.append(newList);
      replacementNode.insertAfter(newListItem);
    } else {
      replacementNode.insertAfter(newList);
    }
    newList.append(...nextSiblings);
  }
  $removeHighestEmptyListParent(listItem);
  return true;
}
function applyMarkerStyles(dom, node, prevNode) {
  const nextTextStyle = node.__textStyle;
  const prevTextStyle = prevNode ? prevNode.__textStyle : "";
  if (prevNode !== null && prevTextStyle === nextTextStyle) {
    return;
  }
  const styles = getStyleObjectFromCSS2(nextTextStyle);
  for (const k in styles) {
    dom.style.setProperty(`--listitem-marker-${k}`, styles[k]);
  }
  if (prevTextStyle !== "") {
    for (const k in getStyleObjectFromCSS2(prevTextStyle)) {
      if (!(k in styles)) {
        dom.style.removeProperty(`--listitem-marker-${k}`);
      }
    }
  }
}
var ListItemNode = class extends ElementNode2 {
  /** @internal */
  __value;
  /** @internal */
  __checked;
  /** @internal */
  $config() {
    return this.config("listitem", {
      $transform: (node) => {
        const parent = node.getParent();
        if ($isListNode(parent)) {
          if (parent.getListType() !== "check" && node.getChecked() != null) {
            node.setChecked(void 0);
          }
        } else if (parent) {
          const newParent = node.createParentElementNode();
          if (!$isListNode(newParent)) {
            formatDevErrorMessage7(`ListItemNode.createParentElementNode() must return a ListNode`);
          }
          const children = [node];
          for (const dir of ["previous", "next"]) {
            children.reverse();
            for (const {
              origin
            } of $getSiblingCaret2(node, dir)) {
              if (!$isListItemNode(origin)) {
                break;
              }
              children.push(origin);
            }
          }
          node.insertBefore(newParent);
          newParent.splice(0, 0, children);
          if (!$isRootOrShadowRoot2(parent)) {
            $insertNodeToNearestRootAtCaret2(newParent, $rewindSiblingCaret2($getSiblingCaret2(newParent, "next")), {
              $shouldSplit: () => false,
              removeEmptyDestination: true
            });
            if (parent.isEmpty() && parent.isAttached()) {
              parent.remove();
            }
          }
        }
      },
      extends: ElementNode2,
      importDOM: buildImportMap2({
        li: () => ({
          conversion: $convertListItemElement,
          priority: 0
        })
      })
    });
  }
  constructor(value = 1, checked = void 0, key) {
    super(key);
    this.__value = value === void 0 ? 1 : value;
    this.__checked = checked;
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__value = prevNode.__value;
    this.__checked = prevNode.__checked;
  }
  createDOM(config) {
    const element = document.createElement("li");
    this.updateListItemDOM(null, element, config);
    return element;
  }
  updateListItemDOM(prevNode, dom, config) {
    updateListItemChecked(dom, this, prevNode);
    dom.value = this.__value;
    $setListItemThemeClassNames(dom, config.theme, this);
    const prevStyle = prevNode ? prevNode.__style : "";
    const nextStyle = this.__style;
    if (prevStyle !== nextStyle) {
      setDOMStyleFromCSS2(dom.style, nextStyle, prevStyle);
    }
    applyMarkerStyles(dom, this, prevNode);
  }
  updateDOM(prevNode, dom, config) {
    const element = dom;
    this.updateListItemDOM(prevNode, element, config);
    return false;
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setValue(serializedNode.value).setChecked(serializedNode.checked);
  }
  exportDOM(editor) {
    const element = this.createDOM(editor._config);
    const formatType = this.getFormatType();
    if (formatType) {
      element.style.textAlign = formatType;
    }
    const direction = this.getDirection();
    if (direction) {
      element.dir = direction;
    }
    if (isNestedListNode(this)) {
      return {
        after(containerElement) {
          if (isHTMLElement2(containerElement)) {
            const prevSibling = containerElement.previousElementSibling;
            if (isHTMLElement2(prevSibling) && prevSibling.nodeName === "LI") {
              while (containerElement.firstChild) {
                prevSibling.append(containerElement.firstChild);
              }
              containerElement.remove();
            }
          }
          return containerElement;
        },
        element
      };
    }
    return {
      element
    };
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      checked: this.getChecked(),
      value: this.getValue()
    };
  }
  append(...nodes) {
    for (let i2 = 0; i2 < nodes.length; i2++) {
      const node = nodes[i2];
      if ($isElementNode2(node) && this.canMergeWith(node)) {
        const children = node.getChildren();
        this.append(...children);
        node.remove();
      } else {
        super.append(node);
      }
    }
    return this;
  }
  replace(replaceWithNode, includeChildren) {
    if ($isListItemNode(replaceWithNode)) {
      return super.replace(replaceWithNode);
    }
    this.setIndent(0);
    const list = this.getParentOrThrow();
    if (!$isListNode(list)) {
      return replaceWithNode;
    }
    if (list.__first === this.getKey()) {
      list.insertBefore(replaceWithNode);
    } else if (list.__last === this.getKey()) {
      list.insertAfter(replaceWithNode);
    } else {
      const newList = $copyNode2(list);
      let nextSibling = this.getNextSibling();
      while (nextSibling) {
        const nodeToAppend = nextSibling;
        nextSibling = nextSibling.getNextSibling();
        newList.append(nodeToAppend);
      }
      list.insertAfter(replaceWithNode);
      replaceWithNode.insertAfter(newList);
    }
    if (includeChildren) {
      if (!$isElementNode2(replaceWithNode)) {
        formatDevErrorMessage7(`includeChildren should only be true for ElementNodes`);
      }
      this.getChildren().forEach((child) => {
        replaceWithNode.append(child);
      });
    }
    this.remove();
    if (list.getChildrenSize() === 0) {
      list.remove();
    }
    return replaceWithNode;
  }
  insertAfter(node, restoreSelection = true) {
    const listNode = this.getParentOrThrow();
    if (!$isListNode(listNode)) {
      {
        formatDevErrorMessage7(`insertAfter: list node is not parent of list item node`);
      }
    }
    if ($isListItemNode(node)) {
      return super.insertAfter(node, restoreSelection);
    }
    const siblings = this.getNextSiblings();
    listNode.insertAfter(node, restoreSelection);
    if (siblings.length !== 0) {
      const newListNode = $copyNode2(listNode);
      siblings.forEach((sibling) => newListNode.append(sibling));
      node.insertAfter(newListNode, restoreSelection);
    }
    return node;
  }
  remove(preserveEmptyParent) {
    const prevSibling = this.getPreviousSibling();
    const nextSibling = this.getNextSibling();
    super.remove(preserveEmptyParent);
    if (prevSibling && nextSibling && isNestedListNode(prevSibling) && isNestedListNode(nextSibling)) {
      mergeLists(prevSibling.getFirstChild(), nextSibling.getFirstChild());
      nextSibling.remove();
    }
  }
  resetOnCopyNodeFrom(original) {
    super.resetOnCopyNodeFrom(original);
    if (original.getChecked()) {
      this.setChecked(false);
    }
  }
  insertNewAfter(_2, restoreSelection = true) {
    const newElement = $copyNode2(this);
    this.insertAfter(newElement, restoreSelection);
    return newElement;
  }
  collapseAtStart(selection) {
    const paragraph = $createParagraphNode2();
    const children = this.getChildren();
    children.forEach((child) => paragraph.append(child));
    const listNode = this.getParentOrThrow();
    const listNodeParent = listNode.getParentOrThrow();
    const isIndented = $isListItemNode(listNodeParent);
    if (listNode.getChildrenSize() === 1) {
      if (isIndented) {
        listNode.remove();
        listNodeParent.select();
      } else {
        listNode.insertBefore(paragraph);
        listNode.remove();
        const anchor = selection.anchor;
        const focus = selection.focus;
        const key = paragraph.getKey();
        if (anchor.type === "element" && anchor.getNode().is(this)) {
          anchor.set(key, anchor.offset, "element");
        }
        if (focus.type === "element" && focus.getNode().is(this)) {
          focus.set(key, focus.offset, "element");
        }
      }
    } else {
      listNode.insertBefore(paragraph);
      this.remove();
    }
    return true;
  }
  getValue() {
    const self2 = this.getLatest();
    return self2.__value;
  }
  setValue(value) {
    const self2 = this.getWritable();
    self2.__value = value;
    return self2;
  }
  getChecked() {
    const self2 = this.getLatest();
    let listType;
    const parent = this.getParent();
    if ($isListNode(parent)) {
      listType = parent.getListType();
    }
    return listType === "check" ? Boolean(self2.__checked) : void 0;
  }
  setChecked(checked) {
    const self2 = this.getWritable();
    self2.__checked = checked;
    return self2;
  }
  toggleChecked() {
    const self2 = this.getWritable();
    return self2.setChecked(!self2.__checked);
  }
  getIndent() {
    const parent = this.getParent();
    if (parent === null || !this.isAttached()) {
      return this.getLatest().__indent;
    }
    let listNodeParent = parent.getParentOrThrow();
    let indentLevel = 0;
    while ($isListItemNode(listNodeParent)) {
      listNodeParent = listNodeParent.getParentOrThrow().getParentOrThrow();
      indentLevel++;
    }
    return indentLevel;
  }
  setIndent(indent) {
    if (!(typeof indent === "number")) {
      formatDevErrorMessage7(`Invalid indent value.`);
    }
    indent = Math.floor(indent);
    if (!(indent >= 0)) {
      formatDevErrorMessage7(`Indent value must be non-negative.`);
    }
    let currentIndent = this.getIndent();
    while (currentIndent !== indent) {
      if (currentIndent < indent) {
        $handleIndent(this);
        currentIndent++;
      } else {
        $handleOutdent(this);
        currentIndent--;
      }
    }
    return this;
  }
  /** @deprecated @internal */
  canInsertAfter(node) {
    return $isListItemNode(node);
  }
  /** @deprecated @internal */
  canReplaceWith(replacement) {
    return $isListItemNode(replacement);
  }
  canMergeWith(node) {
    return $isListItemNode(node) || $isParagraphNode2(node);
  }
  extractWithChild(child, selection) {
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    const anchorNode = selection.anchor.getNode();
    const focusNode = selection.focus.getNode();
    return this.isParentOf(anchorNode) && this.isParentOf(focusNode) && this.getTextContent().length === selection.getTextContent().length;
  }
  isParentRequired() {
    return true;
  }
  createParentElementNode() {
    return $createListNode("bullet");
  }
  canMergeWhenEmpty() {
    return true;
  }
};
function $setListItemThemeClassNames(dom, editorThemeClasses, node) {
  const classesToAdd = [];
  const classesToRemove = [];
  const listTheme = editorThemeClasses.list;
  const listItemClassName = listTheme ? listTheme.listitem : void 0;
  let nestedListItemClassName;
  if (listTheme && listTheme.nested) {
    nestedListItemClassName = listTheme.nested.listitem;
  }
  if (listItemClassName !== void 0) {
    classesToAdd.push(...normalizeClassNames2(listItemClassName));
  }
  if (listTheme) {
    const parentNode = node.getParent();
    const isCheckList = $isListNode(parentNode) && parentNode.getListType() === "check";
    const checked = node.getChecked();
    if (!isCheckList || checked) {
      classesToRemove.push(listTheme.listitemUnchecked);
    }
    if (!isCheckList || !checked) {
      classesToRemove.push(listTheme.listitemChecked);
    }
    if (isCheckList) {
      classesToAdd.push(checked ? listTheme.listitemChecked : listTheme.listitemUnchecked);
    }
  }
  if (nestedListItemClassName !== void 0) {
    const nestedListItemClasses = normalizeClassNames2(nestedListItemClassName);
    if (node.getChildren().some((child) => $isListNode(child))) {
      classesToAdd.push(...nestedListItemClasses);
    } else {
      classesToRemove.push(...nestedListItemClasses);
    }
  }
  if (classesToRemove.length > 0) {
    removeClassNamesFromElement3(dom, ...classesToRemove);
  }
  if (classesToAdd.length > 0) {
    addClassNamesToElement3(dom, ...classesToAdd);
  }
}
function updateListItemChecked(dom, listItemNode, prevListItemNode) {
  const parent = listItemNode.getParent();
  const isCheckbox = $isListNode(parent) && parent.getListType() === "check" && // Only add attributes for leaf list items
  !$isListNode(listItemNode.getFirstChild());
  if (!isCheckbox) {
    dom.removeAttribute("role");
    dom.removeAttribute("tabIndex");
    dom.removeAttribute("aria-checked");
  } else {
    dom.setAttribute("role", "checkbox");
    dom.setAttribute("tabIndex", "-1");
    if (!prevListItemNode || listItemNode.__checked !== prevListItemNode.__checked) {
      dom.setAttribute("aria-checked", listItemNode.getChecked() ? "true" : "false");
    }
  }
}
function $convertListItemElement(domNode) {
  const isGitHubCheckList = domNode.classList.contains("task-list-item");
  if (isGitHubCheckList) {
    for (const child of domNode.children) {
      if (child.tagName === "INPUT") {
        return $convertCheckboxInput(child);
      }
    }
  }
  const isJoplinCheckList = domNode.classList.contains("joplin-checkbox");
  if (isJoplinCheckList) {
    for (const child of domNode.children) {
      if (child.classList.contains("checkbox-wrapper") && child.children.length > 0 && child.children[0].tagName === "INPUT") {
        return $convertCheckboxInput(child.children[0]);
      }
    }
  }
  const ariaCheckedAttr = domNode.getAttribute("aria-checked");
  const checked = ariaCheckedAttr === "true" ? true : ariaCheckedAttr === "false" ? false : void 0;
  return {
    node: $createListItemNode(checked)
  };
}
function $convertCheckboxInput(domNode) {
  const isCheckboxInput = domNode.getAttribute("type") === "checkbox";
  if (!isCheckboxInput) {
    return {
      node: null
    };
  }
  const checked = domNode.hasAttribute("checked");
  return {
    node: $createListItemNode(checked)
  };
}
function $createListItemNode(checked) {
  return $applyNodeReplacement2(new ListItemNode(void 0, checked));
}
function $isListItemNode(node) {
  return node instanceof ListItemNode;
}
var ListNode = class extends ElementNode2 {
  /** @internal */
  __tag;
  /** @internal */
  __start;
  /** @internal */
  __listType;
  /** @internal */
  $config() {
    return this.config("list", {
      $transform: (node) => {
        mergeNextSiblingListIfSameType(node);
        updateChildrenListItemValue(node);
      },
      extends: ElementNode2,
      importDOM: buildImportMap2({
        ol: () => ({
          conversion: $convertListNode,
          priority: 0
        }),
        ul: () => ({
          conversion: $convertListNode,
          priority: 0
        })
      })
    });
  }
  constructor(listType = "number", start = 1, key) {
    super(key);
    const _listType = TAG_TO_LIST_TYPE[listType] || listType;
    this.__listType = _listType;
    this.__tag = _listType === "number" ? "ol" : "ul";
    this.__start = start;
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__listType = prevNode.__listType;
    this.__tag = prevNode.__tag;
    this.__start = prevNode.__start;
  }
  getTag() {
    return this.getLatest().__tag;
  }
  setListType(type) {
    const writable = this.getWritable();
    writable.__listType = type;
    writable.__tag = type === "number" ? "ol" : "ul";
    return writable;
  }
  getListType() {
    return this.getLatest().__listType;
  }
  getStart() {
    return this.getLatest().__start;
  }
  setStart(start) {
    const self2 = this.getWritable();
    self2.__start = start;
    return self2;
  }
  // View
  createDOM(config, _editor) {
    const tag = this.__tag;
    const dom = document.createElement(tag);
    if (this.__start !== 1) {
      dom.setAttribute("start", String(this.__start));
    }
    dom.__lexicalListType = this.__listType;
    $setListThemeClassNames(dom, config.theme, this);
    return dom;
  }
  updateDOM(prevNode, dom, config) {
    if (prevNode.__tag !== this.__tag || prevNode.__listType !== this.__listType) {
      return true;
    }
    $setListThemeClassNames(dom, config.theme, this);
    if (prevNode.__start !== this.__start) {
      dom.setAttribute("start", String(this.__start));
    }
    return false;
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setListType(serializedNode.listType).setStart(serializedNode.start);
  }
  exportDOM(editor) {
    const element = this.createDOM(editor._config, editor);
    if (isHTMLElement3(element)) {
      if (this.__start !== 1) {
        element.setAttribute("start", String(this.__start));
      }
      if (this.__listType === "check") {
        element.setAttribute("__lexicalListType", "check");
      }
    }
    return {
      element
    };
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      listType: this.getListType(),
      start: this.getStart(),
      tag: this.getTag()
    };
  }
  canBeEmpty() {
    return false;
  }
  canIndent() {
    return false;
  }
  splice(start, deleteCount, nodesToInsert) {
    const exampleListItem = nodesToInsert.find($isListItemNode) ?? this.getChildren().find($isListItemNode);
    const $newListItem = exampleListItem ? () => $copyNode2(exampleListItem) : $createListItemNode;
    let listItemNodesToInsert = nodesToInsert;
    for (let i2 = 0; i2 < nodesToInsert.length; i2++) {
      const node = nodesToInsert[i2];
      if (!$isListItemNode(node)) {
        if (listItemNodesToInsert === nodesToInsert) {
          listItemNodesToInsert = [...nodesToInsert];
        }
        listItemNodesToInsert[i2] = $newListItem().append($isElementNode2(node) && !($isListNode(node) || node.isInline()) ? $createTextNode2(node.getTextContent()) : node);
      }
    }
    return super.splice(start, deleteCount, listItemNodesToInsert);
  }
  extractWithChild(child) {
    return $isListItemNode(child);
  }
};
function $setListThemeClassNames(dom, editorThemeClasses, node) {
  const classesToAdd = [];
  const classesToRemove = [];
  const listTheme = editorThemeClasses.list;
  if (listTheme !== void 0) {
    const listLevelsClassNames = listTheme[`${node.__tag}Depth`] || [];
    const listDepth = $getListDepth(node) - 1;
    const normalizedListDepth = listDepth % listLevelsClassNames.length;
    const listLevelClassName = listLevelsClassNames[normalizedListDepth];
    const listClassName = listTheme[node.__tag];
    let nestedListClassName;
    const nestedListTheme = listTheme.nested;
    const checklistClassName = listTheme.checklist;
    if (nestedListTheme !== void 0 && nestedListTheme.list) {
      nestedListClassName = nestedListTheme.list;
    }
    if (listClassName !== void 0) {
      classesToAdd.push(listClassName);
    }
    if (checklistClassName !== void 0 && node.__listType === "check") {
      classesToAdd.push(checklistClassName);
    }
    if (listLevelClassName !== void 0) {
      classesToAdd.push(...normalizeClassNames2(listLevelClassName));
      for (let i2 = 0; i2 < listLevelsClassNames.length; i2++) {
        if (i2 !== normalizedListDepth) {
          classesToRemove.push(node.__tag + i2);
        }
      }
    }
    if (nestedListClassName !== void 0) {
      const nestedListItemClasses = normalizeClassNames2(nestedListClassName);
      if (listDepth > 1) {
        classesToAdd.push(...nestedListItemClasses);
      } else {
        classesToRemove.push(...nestedListItemClasses);
      }
    }
  }
  if (classesToRemove.length > 0) {
    removeClassNamesFromElement3(dom, ...classesToRemove);
  }
  if (classesToAdd.length > 0) {
    addClassNamesToElement3(dom, ...classesToAdd);
  }
}
function $normalizeChildren(nodes) {
  const normalizedListItems = [];
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    if ($isListItemNode(node)) {
      normalizedListItems.push(node);
      const children = node.getChildren();
      if (children.length > 1) {
        children.forEach((child) => {
          if ($isListNode(child)) {
            normalizedListItems.push($wrapInListItem(child));
          }
        });
      }
    } else {
      normalizedListItems.push($wrapInListItem(node));
    }
  }
  return normalizedListItems;
}
function isDomChecklist(domNode) {
  if (domNode.getAttribute("__lexicallisttype") === "check" || // is github checklist
  domNode.classList.contains("contains-task-list") || // is joplin checklist
  domNode.getAttribute("data-is-checklist") === "1") {
    return true;
  }
  for (const child of domNode.childNodes) {
    if (isHTMLElement3(child) && child.hasAttribute("aria-checked")) {
      return true;
    }
  }
  return false;
}
function $convertListNode(domNode) {
  const nodeName = domNode.nodeName.toLowerCase();
  let node = null;
  if (nodeName === "ol") {
    const start = domNode.start;
    node = $createListNode("number", start);
  } else if (nodeName === "ul") {
    if (isDomChecklist(domNode)) {
      node = $createListNode("check");
    } else {
      node = $createListNode("bullet");
    }
  }
  return {
    after: $normalizeChildren,
    node
  };
}
var TAG_TO_LIST_TYPE = {
  ol: "number",
  ul: "bullet"
};
function $createListNode(listType = "number", start = 1) {
  return $applyNodeReplacement2(new ListNode(listType, start));
}
function $isListNode(node) {
  return node instanceof ListNode;
}
var INSERT_CHECK_LIST_COMMAND = createCommand2("INSERT_CHECK_LIST_COMMAND");
function registerCheckList(editor, options) {
  const disableTakeFocusOnClick = options && options.disableTakeFocusOnClick || false;
  const peekDisableTakeFocusOnClick = typeof disableTakeFocusOnClick === "boolean" ? () => disableTakeFocusOnClick : disableTakeFocusOnClick.peek.bind(disableTakeFocusOnClick);
  const DEDUP_WINDOW_MS = 500;
  const isWithinDedupWindow = (event) => {
    const target = event.target;
    if (!isHTMLElement3(target)) {
      return false;
    }
    const last = target.__lexicalCheckListLastHandled;
    return last !== void 0 && event.timeStamp - last < DEDUP_WINDOW_MS;
  };
  const recordHandled = (event) => {
    const target = event.target;
    if (isHTMLElement3(target)) {
      target.__lexicalCheckListLastHandled = event.timeStamp;
    }
  };
  const configHandleClick = (event) => {
    if (isWithinDedupWindow(event)) {
      return;
    }
    recordHandled(event);
    handleClick(event, peekDisableTakeFocusOnClick());
  };
  const configHandlePointerUp = (event) => {
    if (event.pointerType !== "touch") {
      return;
    }
    if (isWithinDedupWindow(event)) {
      return;
    }
    recordHandled(event);
    handleClick(event, peekDisableTakeFocusOnClick());
  };
  const configHandleSelectDefaults = (event) => {
    handleSelectDefaults(event, peekDisableTakeFocusOnClick());
  };
  return mergeRegister3(editor.registerCommand(INSERT_CHECK_LIST_COMMAND, () => {
    $insertList("check");
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ARROW_DOWN_COMMAND2, (event) => {
    return handleArrowUpOrDown(event, editor, false);
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ARROW_UP_COMMAND2, (event) => {
    return handleArrowUpOrDown(event, editor, true);
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ESCAPE_COMMAND2, () => {
    const activeItem = getActiveCheckListItem();
    if (activeItem != null) {
      const rootElement = editor.getRootElement();
      if (rootElement != null) {
        rootElement.focus();
      }
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_SPACE_COMMAND2, (event) => {
    const activeItem = getActiveCheckListItem();
    if (activeItem != null && editor.isEditable()) {
      editor.update(() => {
        const listItemNode = $getNearestNodeFromDOMNode2(activeItem);
        if ($isListItemNode(listItemNode)) {
          event.preventDefault();
          listItemNode.toggleChecked();
        }
      });
      return true;
    }
    return false;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ARROW_LEFT_COMMAND2, (event) => {
    return editor.getEditorState().read(() => {
      const selection = $getSelection2();
      if ($isRangeSelection2(selection) && selection.isCollapsed()) {
        const {
          anchor
        } = selection;
        const isElement = anchor.type === "element";
        if (isElement || anchor.offset === 0) {
          const anchorNode = anchor.getNode();
          const elementNode = $findMatchingParent3(anchorNode, (node) => $isElementNode2(node) && !node.isInline());
          if ($isListItemNode(elementNode)) {
            const parent = elementNode.getParent();
            if ($isListNode(parent) && parent.getListType() === "check" && (isElement || elementNode.getFirstDescendant() === anchorNode)) {
              const domNode = editor.getElementByKey(elementNode.__key);
              if (domNode != null && document.activeElement !== domNode) {
                domNode.focus();
                event.preventDefault();
                return true;
              }
            }
          }
        }
      }
      return false;
    });
  }, COMMAND_PRIORITY_LOW2), editor.registerRootListener((rootElement) => {
    if (rootElement !== null) {
      rootElement.addEventListener("click", configHandleClick);
      rootElement.addEventListener("pointerup", configHandlePointerUp);
      rootElement.addEventListener("pointerdown", configHandleSelectDefaults, {
        capture: true
      });
      rootElement.addEventListener("mousedown", configHandleSelectDefaults, {
        capture: true
      });
      rootElement.addEventListener("touchstart", configHandleSelectDefaults, {
        capture: true,
        passive: false
      });
      return () => {
        rootElement.removeEventListener("click", configHandleClick);
        rootElement.removeEventListener("pointerup", configHandlePointerUp);
        rootElement.removeEventListener("pointerdown", configHandleSelectDefaults, {
          capture: true
        });
        rootElement.removeEventListener("mousedown", configHandleSelectDefaults, {
          capture: true
        });
        rootElement.removeEventListener("touchstart", configHandleSelectDefaults, {
          capture: true
        });
      };
    }
  }));
}
function handleCheckItemEvent(event, callback) {
  const target = event.target;
  if (!isHTMLElement3(target)) {
    return;
  }
  const firstChild = target.firstChild;
  if (isHTMLElement3(firstChild) && (firstChild.tagName === "UL" || firstChild.tagName === "OL")) {
    return;
  }
  const parentNode = target.parentNode;
  if (!parentNode || parentNode.__lexicalListType !== "check") {
    return;
  }
  let clientX = null;
  let pointerType = null;
  if ("clientX" in event) {
    clientX = event.clientX;
  } else if ("touches" in event) {
    const touches = event.touches;
    if (touches.length > 0) {
      clientX = touches[0].clientX;
      pointerType = "touch";
    }
  }
  if (clientX == null) {
    return;
  }
  const rect = target.getBoundingClientRect();
  const zoom = calculateZoomLevel2(target);
  const clientXInPixels = clientX / zoom;
  const beforeStyles = window.getComputedStyle ? window.getComputedStyle(target, "::before") : {
    width: "0px"
  };
  const beforeWidthInPixels = parseFloat(beforeStyles.width);
  const isTouchEvent = pointerType === "touch" || "pointerType" in event && event.pointerType === "touch";
  const clickAreaPadding = isTouchEvent ? 32 : 0;
  if (target.dir === "rtl" ? clientXInPixels < rect.right + clickAreaPadding && clientXInPixels > rect.right - beforeWidthInPixels - clickAreaPadding : clientXInPixels > rect.left - clickAreaPadding && clientXInPixels < rect.left + beforeWidthInPixels + clickAreaPadding) {
    callback();
  }
}
function handleClick(event, disableFocusOnClick) {
  handleCheckItemEvent(event, () => {
    if (isHTMLElement3(event.target)) {
      const domNode = event.target;
      const editor = getNearestEditorFromDOMNode2(domNode);
      if (editor != null && editor.isEditable()) {
        editor.update(() => {
          const node = $getNearestNodeFromDOMNode2(domNode);
          if ($isListItemNode(node)) {
            if (disableFocusOnClick) {
              $addUpdateTag2(SKIP_SELECTION_FOCUS_TAG2);
              $addUpdateTag2(SKIP_DOM_SELECTION_TAG2);
            } else {
              domNode.focus();
            }
            node.toggleChecked();
          }
        });
      }
    }
  });
}
function handleSelectDefaults(event, disableTakeFocusOnClick) {
  handleCheckItemEvent(event, () => {
    event.preventDefault();
    if (disableTakeFocusOnClick) {
      event.stopPropagation();
    }
  });
}
function getActiveCheckListItem() {
  const activeElement = document.activeElement;
  return isHTMLElement3(activeElement) && activeElement.tagName === "LI" && activeElement.parentNode != null && // @ts-ignore internal field
  activeElement.parentNode.__lexicalListType === "check" ? activeElement : null;
}
function findCheckListItemSibling(node, backward) {
  let sibling = backward ? node.getPreviousSibling() : node.getNextSibling();
  let parent = node;
  while (sibling == null && $isListItemNode(parent)) {
    parent = parent.getParentOrThrow().getParent();
    if (parent != null) {
      sibling = backward ? parent.getPreviousSibling() : parent.getNextSibling();
    }
  }
  while ($isListItemNode(sibling)) {
    const firstChild = backward ? sibling.getLastChild() : sibling.getFirstChild();
    if (!$isListNode(firstChild)) {
      return sibling;
    }
    sibling = backward ? firstChild.getLastChild() : firstChild.getFirstChild();
  }
  return null;
}
function handleArrowUpOrDown(event, editor, backward) {
  const activeItem = getActiveCheckListItem();
  if (activeItem != null) {
    editor.update(() => {
      const listItem = $getNearestNodeFromDOMNode2(activeItem);
      if (!$isListItemNode(listItem)) {
        return;
      }
      const nextListItem = findCheckListItemSibling(listItem, backward);
      if (nextListItem != null) {
        nextListItem.selectStart();
        const dom = editor.getElementByKey(nextListItem.__key);
        if (dom != null) {
          event.preventDefault();
          setTimeout(() => {
            dom.focus();
          }, 0);
        }
      }
    });
  }
  return false;
}
var UPDATE_LIST_START_COMMAND = createCommand2("UPDATE_LIST_START_COMMAND");
var INSERT_UNORDERED_LIST_COMMAND = createCommand2("INSERT_UNORDERED_LIST_COMMAND");
var INSERT_ORDERED_LIST_COMMAND = createCommand2("INSERT_ORDERED_LIST_COMMAND");
var REMOVE_LIST_COMMAND = createCommand2("REMOVE_LIST_COMMAND");
function registerList(editor, options) {
  const removeListener = mergeRegister3(editor.registerCommand(INSERT_ORDERED_LIST_COMMAND, () => {
    $insertList("number");
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(UPDATE_LIST_START_COMMAND, (payload) => {
    const {
      listNodeKey,
      newStart
    } = payload;
    const listNode = $getNodeByKey2(listNodeKey);
    if (!$isListNode(listNode)) {
      return false;
    }
    if (listNode.getListType() === "number") {
      listNode.setStart(newStart);
      updateChildrenListItemValue(listNode);
    }
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(INSERT_UNORDERED_LIST_COMMAND, () => {
    $insertList("bullet");
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(REMOVE_LIST_COMMAND, () => {
    $removeList();
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(INSERT_PARAGRAPH_COMMAND2, () => {
    const shouldRestore = options && options.restoreNumbering;
    return $handleListInsertParagraph(!!shouldRestore);
  }, COMMAND_PRIORITY_LOW2), editor.registerNodeTransform(ListItemNode, (node) => {
    const firstChild = node.getFirstChild();
    if (firstChild) {
      if ($isTextNode2(firstChild)) {
        const style = firstChild.getStyle();
        const format = firstChild.getFormat();
        if (node.getTextStyle() !== style) {
          node.setTextStyle(style);
        }
        if (node.getTextFormat() !== format) {
          node.setTextFormat(format);
        }
      }
    } else {
      const selection = $getSelection2();
      if ($isRangeSelection2(selection) && (selection.style !== node.getTextStyle() || selection.format !== node.getTextFormat()) && selection.isCollapsed() && node.is(selection.anchor.getNode())) {
        node.setTextStyle(selection.style).setTextFormat(selection.format);
      }
    }
  }), editor.registerNodeTransform(TextNode2, (node) => {
    const listItemParentNode = node.getParent();
    if ($isListItemNode(listItemParentNode) && node.is(listItemParentNode.getFirstChild())) {
      const style = node.getStyle();
      const format = node.getFormat();
      if (style !== listItemParentNode.getTextStyle() || format !== listItemParentNode.getTextFormat()) {
        listItemParentNode.setTextStyle(style).setTextFormat(format);
      }
    }
  }));
  return removeListener;
}
function registerListStrictIndentTransform(editor) {
  const $formatListIndentStrict = (listItemNode) => {
    const listNode = listItemNode.getParent();
    if ($isListNode(listItemNode.getFirstChild()) || !$isListNode(listNode)) {
      return;
    }
    const startingListItemNode = $findMatchingParent3(listItemNode, (node) => $isListItemNode(node) && $isListNode(node.getParent()) && $isListItemNode(node.getPreviousSibling()));
    if (startingListItemNode === null && listItemNode.getIndent() > 0) {
      listItemNode.setIndent(0);
    } else if ($isListItemNode(startingListItemNode)) {
      const prevListItemNode = startingListItemNode.getPreviousSibling();
      if ($isListItemNode(prevListItemNode)) {
        const endListItemNode = $findChildrenEndListItemNode(prevListItemNode);
        const endListNode = endListItemNode.getParent();
        if ($isListNode(endListNode)) {
          const prevDepth = $getListDepth(endListNode);
          const depth = $getListDepth(listNode);
          if (prevDepth + 1 < depth) {
            listItemNode.setIndent(prevDepth);
          }
        }
      }
    }
  };
  const $processListWithStrictIndent = (listNode) => {
    const queue = [listNode];
    while (queue.length > 0) {
      const node = queue.shift();
      if (!$isListNode(node)) {
        continue;
      }
      for (const child of node.getChildren()) {
        if ($isListItemNode(child)) {
          $formatListIndentStrict(child);
          const firstChild = child.getFirstChild();
          if ($isListNode(firstChild)) {
            queue.push(firstChild);
          }
        }
      }
    }
  };
  return editor.registerNodeTransform(ListNode, $processListWithStrictIndent);
}
function $findChildrenEndListItemNode(listItemNode) {
  let current = listItemNode;
  let firstChild = current.getFirstChild();
  while ($isListNode(firstChild)) {
    const lastChild = firstChild.getLastChild();
    if ($isListItemNode(lastChild)) {
      current = lastChild;
      firstChild = current.getFirstChild();
    } else {
      break;
    }
  }
  return current;
}
function insertList(editor, listType) {
  editor.update(() => $insertList(listType));
}
function removeList(editor) {
  editor.update(() => $removeList());
}
var ListExtension = defineExtension2({
  build(editor, config, state) {
    return namedSignals2(config);
  },
  config: safeCast2({
    hasStrictIndent: false,
    shouldPreserveNumbering: false
  }),
  name: "@lexical/list/List",
  nodes: () => [ListNode, ListItemNode],
  register(editor, config, state) {
    const stores = state.getOutput();
    return mergeRegister3(effect(() => {
      return registerList(editor, {
        restoreNumbering: stores.shouldPreserveNumbering.value
      });
    }), effect(() => stores.hasStrictIndent.value ? registerListStrictIndentTransform(editor) : void 0));
  }
});
var CheckListExtension = defineExtension2({
  build: (editor, config) => namedSignals2(config),
  config: safeCast2({
    disableTakeFocusOnClick: false
  }),
  dependencies: [ListExtension],
  name: "@lexical/list/CheckList",
  register: (editor, config, state) => registerCheckList(editor, state.getOutput())
});

// node_modules/@lexical/list/LexicalList.mjs
var mod9 = true ? LexicalList_dev_exports : LexicalList_prod_exports;
var $createListItemNode2 = mod9.$createListItemNode;
var $createListNode2 = mod9.$createListNode;
var $getListDepth2 = mod9.$getListDepth;
var $handleListInsertParagraph2 = mod9.$handleListInsertParagraph;
var $insertList2 = mod9.$insertList;
var $isListItemNode2 = mod9.$isListItemNode;
var $isListNode2 = mod9.$isListNode;
var $removeList2 = mod9.$removeList;
var CheckListExtension2 = mod9.CheckListExtension;
var INSERT_CHECK_LIST_COMMAND2 = mod9.INSERT_CHECK_LIST_COMMAND;
var INSERT_ORDERED_LIST_COMMAND2 = mod9.INSERT_ORDERED_LIST_COMMAND;
var INSERT_UNORDERED_LIST_COMMAND2 = mod9.INSERT_UNORDERED_LIST_COMMAND;
var ListExtension2 = mod9.ListExtension;
var ListItemNode2 = mod9.ListItemNode;
var ListNode2 = mod9.ListNode;
var REMOVE_LIST_COMMAND2 = mod9.REMOVE_LIST_COMMAND;
var UPDATE_LIST_START_COMMAND2 = mod9.UPDATE_LIST_START_COMMAND;
var insertList2 = mod9.insertList;
var registerCheckList2 = mod9.registerCheckList;
var registerList2 = mod9.registerList;
var registerListStrictIndentTransform2 = mod9.registerListStrictIndentTransform;
var removeList2 = mod9.removeList;

// node_modules/@lexical/link/LexicalLink.dev.mjs
var LexicalLink_dev_exports = {};
__export(LexicalLink_dev_exports, {
  $createAutoLinkNode: () => $createAutoLinkNode,
  $createLinkNode: () => $createLinkNode,
  $isAutoLinkNode: () => $isAutoLinkNode,
  $isLinkNode: () => $isLinkNode,
  $toggleLink: () => $toggleLink,
  AutoLinkExtension: () => AutoLinkExtension,
  AutoLinkNode: () => AutoLinkNode,
  ClickableLinkExtension: () => ClickableLinkExtension,
  LinkExtension: () => LinkExtension,
  LinkNode: () => LinkNode,
  TOGGLE_LINK_COMMAND: () => TOGGLE_LINK_COMMAND,
  createLinkMatcherWithRegExp: () => createLinkMatcherWithRegExp,
  formatUrl: () => formatUrl,
  registerAutoLink: () => registerAutoLink,
  registerClickableLink: () => registerClickableLink,
  registerLink: () => registerLink,
  toggleLink: () => toggleLink
});
function formatDevErrorMessage8(message) {
  throw new Error(message);
}
var SUPPORTED_URL_PROTOCOLS = /* @__PURE__ */ new Set(["http:", "https:", "mailto:", "sms:", "tel:"]);
var LinkNode = class _LinkNode extends ElementNode2 {
  /** @internal */
  __url;
  /** @internal */
  __target;
  /** @internal */
  __rel;
  /** @internal */
  __title;
  static getType() {
    return "link";
  }
  static clone(node) {
    return new _LinkNode(node.__url, {
      rel: node.__rel,
      target: node.__target,
      title: node.__title
    }, node.__key);
  }
  constructor(url = "", attributes = {}, key) {
    super(key);
    const {
      target = null,
      rel = null,
      title = null
    } = attributes;
    this.__url = url;
    this.__target = target;
    this.__rel = rel;
    this.__title = title;
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__url = prevNode.__url;
    this.__rel = prevNode.__rel;
    this.__target = prevNode.__target;
    this.__title = prevNode.__title;
  }
  createDOM(config) {
    const element = document.createElement("a");
    this.updateLinkDOM(null, element, config);
    addClassNamesToElement3(element, config.theme.link);
    return element;
  }
  updateLinkDOM(prevNode, anchor, config) {
    if (isHTMLAnchorElement3(anchor)) {
      if (!prevNode || prevNode.__url !== this.__url) {
        anchor.href = this.sanitizeUrl(this.__url);
      }
      for (const attr of ["target", "rel", "title"]) {
        const key = `__${attr}`;
        const value = this[key];
        if (!prevNode || prevNode[key] !== value) {
          if (value) {
            anchor[attr] = value;
          } else {
            anchor.removeAttribute(attr);
          }
        }
      }
    }
  }
  updateDOM(prevNode, anchor, config) {
    this.updateLinkDOM(prevNode, anchor, config);
    return false;
  }
  static importDOM() {
    return {
      a: (node) => ({
        conversion: $convertAnchorElement,
        priority: 1
      })
    };
  }
  static importJSON(serializedNode) {
    return $createLinkNode().updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setURL(serializedNode.url).setRel(serializedNode.rel || null).setTarget(serializedNode.target || null).setTitle(serializedNode.title || null);
  }
  sanitizeUrl(url) {
    url = formatUrl(url);
    try {
      const parsedUrl = new URL(formatUrl(url));
      if (!SUPPORTED_URL_PROTOCOLS.has(parsedUrl.protocol)) {
        return "about:blank";
      }
    } catch (_unused) {
      return url;
    }
    return url;
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      rel: this.getRel(),
      target: this.getTarget(),
      title: this.getTitle(),
      url: this.getURL()
    };
  }
  getURL() {
    return this.getLatest().__url;
  }
  setURL(url) {
    const writable = this.getWritable();
    writable.__url = url;
    return writable;
  }
  getTarget() {
    return this.getLatest().__target;
  }
  setTarget(target) {
    const writable = this.getWritable();
    writable.__target = target;
    return writable;
  }
  getRel() {
    return this.getLatest().__rel;
  }
  setRel(rel) {
    const writable = this.getWritable();
    writable.__rel = rel;
    return writable;
  }
  getTitle() {
    return this.getLatest().__title;
  }
  setTitle(title) {
    const writable = this.getWritable();
    writable.__title = title;
    return writable;
  }
  insertNewAfter(_2, restoreSelection = true) {
    const linkNode = $copyNode2(this);
    this.insertAfter(linkNode, restoreSelection);
    return linkNode;
  }
  canInsertTextBefore() {
    return false;
  }
  canInsertTextAfter() {
    return false;
  }
  canBeEmpty() {
    return false;
  }
  isInline() {
    return true;
  }
  extractWithChild(child, selection, destination) {
    if (!$isRangeSelection2(selection)) {
      return false;
    }
    const anchorNode = selection.anchor.getNode();
    const focusNode = selection.focus.getNode();
    return this.isParentOf(anchorNode) && this.isParentOf(focusNode) && selection.getTextContent().length > 0;
  }
  isEmailURI() {
    return this.__url.startsWith("mailto:");
  }
  isWebSiteURI() {
    return this.__url.startsWith("https://") || this.__url.startsWith("http://");
  }
  shouldMergeAdjacentLink(otherLink) {
    return this.getType() === otherLink.getType() && this.__url === otherLink.__url && this.__target === otherLink.__target && this.__rel === otherLink.__rel && this.__title === otherLink.__title;
  }
};
function $saveCaretPair(point) {
  const next = $caretFromPoint2(point, "next");
  return [next, next.getFlipped()];
}
function $restoreCaretPair(point, pair) {
  for (const caret of pair) {
    if (caret.origin.isAttached()) {
      const normalized = $normalizeCaret2(caret);
      $setPointFromCaret2(point, normalized);
      return;
    }
  }
}
function $linkNodeTransform(link) {
  const selection = $getSelection2();
  let anchorPair = null;
  let focusPair = null;
  if ($isRangeSelection2(selection)) {
    anchorPair = $saveCaretPair(selection.anchor);
    focusPair = $saveCaretPair(selection.focus);
  }
  function $restoreSelection() {
    if ($isRangeSelection2(selection)) {
      $restoreCaretPair(selection.anchor, anchorPair);
      $restoreCaretPair(selection.focus, focusPair);
      $normalizeSelection__EXPERIMENTAL(selection);
    }
  }
  let transformed = false;
  for (const caret of $getChildCaret2(link, "next")) {
    const node = caret.origin;
    if ($isElementNode2(node) && !node.isInline()) {
      const blockChildren = node.getChildren();
      if (blockChildren.length > 0) {
        const innerLink = $copyNode2(link);
        innerLink.append(...blockChildren);
        node.append(innerLink);
        transformed = true;
      }
      $insertNodeToNearestRootAtCaret2(node, $rewindSiblingCaret2(caret), {
        $shouldSplit: () => false
      });
    }
  }
  function $fixMergeBoundaryCaret(pair, absorbingLink, mergingLink) {
    const [next, prev] = pair;
    const $isAffected = (caret) => $isSiblingCaret2(caret) && caret.origin.is(absorbingLink);
    if (!$isAffected(next) && !$isAffected(prev)) {
      return pair;
    }
    const fixed = $normalizeCaret2($getChildCaret2(mergingLink, "next"));
    return [fixed, fixed.getFlipped()];
  }
  if (link.isAttached()) {
    const prevSibling = link.getPreviousSibling();
    if ($isLinkNode(prevSibling) && prevSibling.shouldMergeAdjacentLink(link)) {
      if (anchorPair) {
        anchorPair = $fixMergeBoundaryCaret(anchorPair, prevSibling, link);
      }
      if (focusPair) {
        focusPair = $fixMergeBoundaryCaret(focusPair, prevSibling, link);
      }
      prevSibling.append(...link.getChildren());
      link.remove();
      $restoreSelection();
      return;
    }
    const nextSibling = link.getNextSibling();
    if ($isLinkNode(nextSibling) && link.shouldMergeAdjacentLink(nextSibling)) {
      if (anchorPair) {
        anchorPair = $fixMergeBoundaryCaret(anchorPair, link, nextSibling);
      }
      if (focusPair) {
        focusPair = $fixMergeBoundaryCaret(focusPair, link, nextSibling);
      }
      link.append(...nextSibling.getChildren());
      nextSibling.remove();
      transformed = true;
    }
  }
  if (!transformed) {
    return;
  }
  if (!link.canBeEmpty() && link.isEmpty()) {
    const parent = link.getParent();
    link.remove();
    if (parent && parent.isEmpty()) {
      parent.remove();
    }
  }
  $restoreSelection();
}
function $convertAnchorElement(domNode) {
  let node = null;
  if (isHTMLAnchorElement3(domNode)) {
    const content = domNode.textContent;
    if (content !== null && content !== "" || domNode.children.length > 0) {
      node = $createLinkNode(domNode.getAttribute("href") || "", {
        rel: domNode.getAttribute("rel"),
        target: domNode.getAttribute("target"),
        title: domNode.getAttribute("title")
      });
    }
  }
  return {
    node
  };
}
function $createLinkNode(url = "", attributes) {
  return $applyNodeReplacement2(new LinkNode(url, attributes));
}
function $isLinkNode(node) {
  return node instanceof LinkNode;
}
var AutoLinkNode = class _AutoLinkNode extends LinkNode {
  /** @internal */
  /** Indicates whether the autolink was ever unlinked. **/
  __isUnlinked;
  constructor(url = "", attributes = {}, key) {
    super(url, attributes, key);
    this.__isUnlinked = attributes.isUnlinked !== void 0 && attributes.isUnlinked !== null ? attributes.isUnlinked : false;
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__isUnlinked = prevNode.__isUnlinked;
  }
  static getType() {
    return "autolink";
  }
  static clone(node) {
    return new _AutoLinkNode(node.__url, {
      isUnlinked: node.__isUnlinked,
      rel: node.__rel,
      target: node.__target,
      title: node.__title
    }, node.__key);
  }
  shouldMergeAdjacentLink(_otherLink) {
    return false;
  }
  getIsUnlinked() {
    return this.__isUnlinked;
  }
  setIsUnlinked(value) {
    const self2 = this.getWritable();
    self2.__isUnlinked = value;
    return self2;
  }
  createDOM(config) {
    if (this.__isUnlinked) {
      return document.createElement("span");
    } else {
      return super.createDOM(config);
    }
  }
  updateDOM(prevNode, anchor, config) {
    return super.updateDOM(prevNode, anchor, config) || prevNode.__isUnlinked !== this.__isUnlinked;
  }
  static importJSON(serializedNode) {
    return $createAutoLinkNode().updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setIsUnlinked(serializedNode.isUnlinked || false);
  }
  static importDOM() {
    return null;
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      isUnlinked: this.__isUnlinked
    };
  }
  insertNewAfter(_2, restoreSelection = true) {
    const linkNode = $createAutoLinkNode(this.__url, {
      isUnlinked: this.__isUnlinked,
      rel: this.__rel,
      target: this.__target,
      title: this.__title
    });
    this.insertAfter(linkNode, restoreSelection);
    return linkNode;
  }
};
function $createAutoLinkNode(url = "", attributes) {
  return $applyNodeReplacement2(new AutoLinkNode(url, attributes));
}
function $isAutoLinkNode(node) {
  return node instanceof AutoLinkNode;
}
var TOGGLE_LINK_COMMAND = createCommand2("TOGGLE_LINK_COMMAND");
function $getPointNode(point, offset) {
  if (point.type === "element") {
    const node = point.getNode();
    if (!$isElementNode2(node)) {
      formatDevErrorMessage8(`$getPointNode: element point is not an ElementNode`);
    }
    const childNode = node.getChildren()[point.offset + offset];
    return childNode || null;
  }
  return null;
}
function $withSelectedNodes($fn) {
  const initialSelection = $getSelection2();
  if (!$isRangeSelection2(initialSelection)) {
    return $fn();
  }
  const normalized = $normalizeSelection__EXPERIMENTAL(initialSelection);
  const isBackwards = normalized.isBackward();
  const anchorNode = $getPointNode(normalized.anchor, isBackwards ? -1 : 0);
  const focusNode = $getPointNode(normalized.focus, isBackwards ? 0 : -1);
  const rval = $fn();
  if (anchorNode || focusNode) {
    const updatedSelection = $getSelection2();
    if ($isRangeSelection2(updatedSelection)) {
      const finalSelection = updatedSelection.clone();
      if (anchorNode) {
        const anchorParent = anchorNode.getParent();
        if (anchorParent) {
          finalSelection.anchor.set(anchorParent.getKey(), anchorNode.getIndexWithinParent() + (isBackwards ? 1 : 0), "element");
        }
      }
      if (focusNode) {
        const focusParent = focusNode.getParent();
        if (focusParent) {
          finalSelection.focus.set(focusParent.getKey(), focusNode.getIndexWithinParent() + (isBackwards ? 0 : 1), "element");
        }
      }
      $setSelection2($normalizeSelection__EXPERIMENTAL(finalSelection));
    }
  }
  return rval;
}
function $splitLinkAtSelection(parentLink, extractedNodes) {
  const extractedKeys = new Set(extractedNodes.filter((n2) => parentLink.isParentOf(n2)).map((n2) => n2.getKey()));
  const allChildren = parentLink.getChildren();
  const isExtractedChild = (child) => extractedKeys.has(child.getKey()) || $isElementNode2(child) && extractedNodes.some((n2) => parentLink.isParentOf(n2) && child.isParentOf(n2));
  const extractedChildren = allChildren.filter(isExtractedChild);
  if (extractedChildren.length === allChildren.length) {
    allChildren.forEach((child) => parentLink.insertBefore(child));
    parentLink.remove();
    return;
  }
  const firstExtractedIndex = allChildren.findIndex(isExtractedChild);
  const lastExtractedIndex = allChildren.findLastIndex(isExtractedChild);
  const isAtStart = firstExtractedIndex === 0;
  const isAtEnd = lastExtractedIndex === allChildren.length - 1;
  if (isAtStart) {
    extractedChildren.forEach((child) => parentLink.insertBefore(child));
  } else if (isAtEnd) {
    for (let i2 = extractedChildren.length - 1; i2 >= 0; i2--) {
      parentLink.insertAfter(extractedChildren[i2]);
    }
  } else {
    for (let i2 = extractedChildren.length - 1; i2 >= 0; i2--) {
      parentLink.insertAfter(extractedChildren[i2]);
    }
    const trailingChildren = allChildren.slice(lastExtractedIndex + 1);
    if (trailingChildren.length > 0) {
      const newLink = $copyNode2(parentLink);
      extractedChildren[extractedChildren.length - 1].insertAfter(newLink);
      trailingChildren.forEach((child) => newLink.append(child));
    }
  }
}
function $toggleLink(urlOrAttributes, attributes = {}) {
  let url;
  if (urlOrAttributes && typeof urlOrAttributes === "object") {
    const {
      url: urlProp,
      ...rest
    } = urlOrAttributes;
    url = urlProp;
    attributes = {
      ...rest,
      ...attributes
    };
  } else {
    url = urlOrAttributes;
  }
  const {
    target,
    title
  } = attributes;
  const rel = attributes.rel === void 0 ? "noreferrer" : attributes.rel;
  const selection = $getSelection2();
  if (selection === null || !$isRangeSelection2(selection) && !$isNodeSelection2(selection)) {
    return;
  }
  if ($isNodeSelection2(selection)) {
    const nodes2 = selection.getNodes();
    if (nodes2.length === 0) {
      return;
    }
    nodes2.forEach((node) => {
      if (url === null) {
        const linkParent = $findMatchingParent3(node, (parent) => !$isAutoLinkNode(parent) && $isLinkNode(parent));
        if (linkParent) {
          linkParent.insertBefore(node);
          if (linkParent.getChildren().length === 0) {
            linkParent.remove();
          }
        }
      } else {
        const existingLink = $findMatchingParent3(node, (parent) => !$isAutoLinkNode(parent) && $isLinkNode(parent));
        if (existingLink) {
          existingLink.setURL(url);
          if (target !== void 0) {
            existingLink.setTarget(target);
          }
          if (rel !== void 0) {
            existingLink.setRel(rel);
          }
        } else {
          const linkNode = $createLinkNode(url, {
            rel,
            target
          });
          node.insertBefore(linkNode);
          linkNode.append(node);
        }
      }
    });
    return;
  }
  if (selection.isCollapsed() && url === null) {
    for (const node of selection.getNodes()) {
      const parentLink = $findMatchingParent3(node, (parent) => !$isAutoLinkNode(parent) && $isLinkNode(parent));
      if (parentLink !== null) {
        parentLink.getChildren().forEach((child) => {
          parentLink.insertBefore(child);
        });
        parentLink.remove();
      }
      return;
    }
  }
  const nodes = selection.extract();
  if (url === null) {
    const processedLinks = /* @__PURE__ */ new Set();
    nodes.forEach((node) => {
      const parentLink = $findMatchingParent3(node, (parent) => !$isAutoLinkNode(parent) && $isLinkNode(parent));
      if (parentLink !== null) {
        const linkKey = parentLink.getKey();
        if (processedLinks.has(linkKey)) {
          return;
        }
        $splitLinkAtSelection(parentLink, nodes);
        processedLinks.add(linkKey);
      }
    });
    return;
  }
  const updatedNodes = /* @__PURE__ */ new Set();
  const updateLinkNode = (linkNode) => {
    if (updatedNodes.has(linkNode.getKey())) {
      return;
    }
    updatedNodes.add(linkNode.getKey());
    linkNode.setURL(url);
    if (target !== void 0) {
      linkNode.setTarget(target);
    }
    if (rel !== void 0) {
      linkNode.setRel(rel);
    }
    if (title !== void 0) {
      linkNode.setTitle(title);
    }
  };
  if (nodes.length === 1) {
    const firstNode = nodes[0];
    const linkNode = $findMatchingParent3(firstNode, $isLinkNode);
    if (linkNode !== null) {
      return updateLinkNode(linkNode);
    }
  }
  $withSelectedNodes(() => {
    let linkNode = null;
    for (const node of nodes) {
      if (!node.isAttached()) {
        continue;
      }
      const parentLinkNode = $findMatchingParent3(node, $isLinkNode);
      if (parentLinkNode) {
        updateLinkNode(parentLinkNode);
        continue;
      }
      if ($isElementNode2(node)) {
        if (!node.isInline()) {
          continue;
        }
        if ($isLinkNode(node)) {
          if (!$isAutoLinkNode(node) && (linkNode === null || !linkNode.getParentOrThrow().isParentOf(node))) {
            updateLinkNode(node);
            linkNode = node;
            continue;
          }
          for (const child of node.getChildren()) {
            node.insertBefore(child);
          }
          node.remove();
          continue;
        }
      }
      const prevLinkNode = node.getPreviousSibling();
      if ($isLinkNode(prevLinkNode) && prevLinkNode.is(linkNode)) {
        prevLinkNode.append(node);
        continue;
      }
      linkNode = $createLinkNode(url, {
        rel,
        target,
        title
      });
      node.insertAfter(linkNode);
      linkNode.append(node);
    }
  });
}
var PHONE_NUMBER_REGEX = /^\+?[0-9\s()-]{5,}$/;
function formatUrl(url) {
  if (url.match(/^[a-z][a-z0-9+.-]*:/i)) {
    return url;
  } else if (url.match(/^[/#.]/)) {
    return url;
  } else if (url.includes("@")) {
    return `mailto:${url}`;
  } else if (PHONE_NUMBER_REGEX.test(url)) {
    return `tel:${url}`;
  }
  return `https://${url}`;
}
var defaultProps = {
  attributes: void 0,
  validateUrl: void 0
};
function registerLink(editor, stores) {
  return mergeRegister3(editor.registerNodeTransform(LinkNode, $linkNodeTransform), editor.registerCommand(TOGGLE_LINK_COMMAND, (payload) => {
    const validateUrl = stores.validateUrl.peek();
    const attributes = stores.attributes.peek();
    if (payload === null) {
      $toggleLink(null);
      return true;
    } else if (typeof payload === "string") {
      if (validateUrl === void 0 || validateUrl(payload)) {
        $toggleLink(payload, attributes);
        return true;
      }
      return false;
    } else {
      const {
        url,
        target,
        rel,
        title
      } = payload;
      $toggleLink(url, {
        ...attributes,
        rel,
        target,
        title
      });
      return true;
    }
  }, COMMAND_PRIORITY_EDITOR2), effect(() => {
    const validateUrl = stores.validateUrl.value;
    if (!validateUrl) {
      return;
    }
    const attributes = stores.attributes.value;
    return editor.registerCommand(PASTE_COMMAND2, (event) => {
      const selection = $getSelection2();
      if (!$isRangeSelection2(selection) || selection.isCollapsed() || !objectKlassEquals2(event, ClipboardEvent)) {
        return false;
      }
      if (event.clipboardData === null) {
        return false;
      }
      const clipboardText = event.clipboardData.getData("text");
      if (!validateUrl(clipboardText)) {
        return false;
      }
      if (!selection.getNodes().some((node) => $isElementNode2(node))) {
        editor.dispatchCommand(TOGGLE_LINK_COMMAND, {
          ...attributes,
          url: clipboardText
        });
        event.preventDefault();
        return true;
      }
      return false;
    }, COMMAND_PRIORITY_LOW2);
  }));
}
var LinkExtension = defineExtension2({
  build(editor, config, state) {
    return namedSignals2(config);
  },
  config: defaultProps,
  mergeConfig(config, overrides) {
    const merged = shallowMergeConfig2(config, overrides);
    if (config.attributes) {
      merged.attributes = shallowMergeConfig2(config.attributes, merged.attributes);
    }
    return merged;
  },
  name: "@lexical/link/Link",
  nodes: () => [LinkNode],
  register(editor, config, state) {
    return registerLink(editor, state.getOutput());
  }
});
function findMatchingDOM(startNode, predicate) {
  let node = startNode;
  while (node != null) {
    if (predicate(node)) {
      return node;
    }
    node = node.parentNode;
  }
  return null;
}
function registerClickableLink(editor, stores, eventOptions = {}) {
  const onClick2 = (event) => {
    const target = event.target;
    if (!isDOMNode2(target)) {
      return;
    }
    const nearestEditor = getNearestEditorFromDOMNode2(target);
    if (nearestEditor === null) {
      return;
    }
    let url = null;
    let urlTarget = null;
    nearestEditor.update(() => {
      const clickedNode = $getNearestNodeFromDOMNode2(target);
      if (clickedNode !== null) {
        const maybeLinkNode = $findMatchingParent3(clickedNode, $isElementNode2);
        if (!stores.disabled.peek()) {
          if ($isLinkNode(maybeLinkNode)) {
            url = maybeLinkNode.sanitizeUrl(maybeLinkNode.getURL());
            urlTarget = maybeLinkNode.getTarget();
          } else {
            const a2 = findMatchingDOM(target, isHTMLAnchorElement3);
            if (a2 !== null) {
              url = a2.href;
              urlTarget = a2.target;
            }
          }
        }
      }
    });
    if (url === null || url === "") {
      return;
    }
    const selection = editor.getEditorState().read($getSelection2, {
      editor
    });
    if ($isRangeSelection2(selection) && !selection.isCollapsed()) {
      event.preventDefault();
      return;
    }
    const isMiddle = event.type === "auxclick" && event.button === 1;
    window.open(url, stores.newTab.peek() || isMiddle || event.metaKey || event.ctrlKey || urlTarget === "_blank" ? "_blank" : "_self");
    event.preventDefault();
  };
  const onMouseUp = (event) => {
    if (event.button === 1) {
      onClick2(event);
    }
  };
  return editor.registerRootListener((rootElement) => {
    if (rootElement) {
      rootElement.addEventListener("click", onClick2, eventOptions);
      rootElement.addEventListener("mouseup", onMouseUp, eventOptions);
      return () => {
        rootElement.removeEventListener("click", onClick2);
        rootElement.removeEventListener("mouseup", onMouseUp);
      };
    }
  });
}
var ClickableLinkExtension = defineExtension2({
  build(editor, config, state) {
    return namedSignals2(config);
  },
  config: safeCast2({
    disabled: false,
    newTab: false
  }),
  dependencies: [LinkExtension],
  name: "@lexical/link/ClickableLink",
  register(editor, config, state) {
    return registerClickableLink(editor, state.getOutput());
  }
});
function createLinkMatcherWithRegExp(regExp, urlTransformer = (text) => text) {
  return (text) => {
    const match = regExp.exec(text);
    if (match === null) {
      return null;
    }
    return {
      index: match.index,
      length: match[0].length,
      text: match[0],
      url: urlTransformer(match[0])
    };
  };
}
function findFirstMatch(text, matchers) {
  for (let i2 = 0; i2 < matchers.length; i2++) {
    const match = matchers[i2](text);
    if (match) {
      return match;
    }
  }
  return null;
}
var PUNCTUATION_OR_SPACE = /[.,;\s]/;
function isSeparator(char, separatorRegex) {
  return separatorRegex.test(char);
}
function endsWithSeparator(textContent, separatorRegex) {
  return isSeparator(textContent[textContent.length - 1], separatorRegex);
}
function startsWithSeparator(textContent, separatorRegex) {
  return isSeparator(textContent[0], separatorRegex);
}
function startsWithTLD(textContent, isEmail) {
  if (isEmail) {
    return /^\.[a-zA-Z]{2,}/.test(textContent);
  } else {
    return /^\.[a-zA-Z0-9]{1,}/.test(textContent);
  }
}
function isPreviousNodeValid(node, separatorRegex) {
  let previousNode = node.getPreviousSibling();
  if ($isElementNode2(previousNode)) {
    previousNode = previousNode.getLastDescendant();
  }
  return previousNode === null || $isLineBreakNode2(previousNode) || $isTextNode2(previousNode) && endsWithSeparator(previousNode.getTextContent(), separatorRegex);
}
function isNextNodeValid(node, separatorRegex) {
  let nextNode = node.getNextSibling();
  if ($isElementNode2(nextNode)) {
    nextNode = nextNode.getFirstDescendant();
  }
  return nextNode === null || $isLineBreakNode2(nextNode) || $isTextNode2(nextNode) && startsWithSeparator(nextNode.getTextContent(), separatorRegex);
}
function isContentAroundIsValid(matchStart, matchEnd, separatorRegex, text, nodes) {
  const contentBeforeIsValid = matchStart > 0 ? isSeparator(text[matchStart - 1], separatorRegex) : isPreviousNodeValid(nodes[0], separatorRegex);
  if (!contentBeforeIsValid) {
    return false;
  }
  const contentAfterIsValid = matchEnd < text.length ? isSeparator(text[matchEnd], separatorRegex) : isNextNodeValid(nodes[nodes.length - 1], separatorRegex);
  return contentAfterIsValid;
}
function extractMatchingNodes(nodes, startIndex, endIndex) {
  const unmodifiedBeforeNodes = [];
  const matchingNodes = [];
  const unmodifiedAfterNodes = [];
  let matchingOffset = 0;
  let currentOffset = 0;
  const currentNodes = [...nodes];
  while (currentNodes.length > 0) {
    const currentNode = currentNodes[0];
    const currentNodeText = currentNode.getTextContent();
    const currentNodeLength = currentNodeText.length;
    const currentNodeStart = currentOffset;
    const currentNodeEnd = currentOffset + currentNodeLength;
    if (currentNodeEnd <= startIndex) {
      unmodifiedBeforeNodes.push(currentNode);
      matchingOffset += currentNodeLength;
    } else if (currentNodeStart >= endIndex) {
      unmodifiedAfterNodes.push(currentNode);
    } else {
      matchingNodes.push(currentNode);
    }
    currentOffset += currentNodeLength;
    currentNodes.shift();
  }
  return [matchingOffset, unmodifiedBeforeNodes, matchingNodes, unmodifiedAfterNodes];
}
function $createAutoLinkNode_(nodes, startIndex, endIndex, match) {
  const linkNode = $createAutoLinkNode(match.url, match.attributes);
  if (nodes.length === 1) {
    let remainingTextNode = nodes[0];
    let linkTextNode;
    if (startIndex === 0) {
      [linkTextNode, remainingTextNode] = remainingTextNode.splitText(endIndex);
    } else {
      [, linkTextNode, remainingTextNode] = remainingTextNode.splitText(startIndex, endIndex);
    }
    const textNode = $createTextNode2(match.text);
    textNode.setFormat(linkTextNode.getFormat());
    textNode.setDetail(linkTextNode.getDetail());
    textNode.setStyle(linkTextNode.getStyle());
    linkNode.append(textNode);
    linkTextNode.replace(linkNode);
    return remainingTextNode;
  } else if (nodes.length > 1) {
    const firstTextNode = nodes[0];
    let offset = firstTextNode.getTextContent().length;
    let firstLinkTextNode;
    if (startIndex === 0) {
      firstLinkTextNode = firstTextNode;
    } else {
      [, firstLinkTextNode] = firstTextNode.splitText(startIndex);
    }
    const linkNodes = [];
    let remainingTextNode;
    for (let i2 = 1; i2 < nodes.length; i2++) {
      const currentNode = nodes[i2];
      const currentNodeText = currentNode.getTextContent();
      const currentNodeLength = currentNodeText.length;
      const currentNodeStart = offset;
      const currentNodeEnd = offset + currentNodeLength;
      if (currentNodeStart < endIndex) {
        if (currentNodeEnd <= endIndex) {
          linkNodes.push(currentNode);
        } else {
          const [linkTextNode, endNode] = currentNode.splitText(endIndex - currentNodeStart);
          linkNodes.push(linkTextNode);
          remainingTextNode = endNode;
        }
      }
      offset += currentNodeLength;
    }
    const selection = $getSelection2();
    const selectedTextNode = selection ? selection.getNodes().find($isTextNode2) : void 0;
    const textNode = $createTextNode2(firstLinkTextNode.getTextContent());
    textNode.setFormat(firstLinkTextNode.getFormat());
    textNode.setDetail(firstLinkTextNode.getDetail());
    textNode.setStyle(firstLinkTextNode.getStyle());
    linkNode.append(textNode, ...linkNodes);
    if (selectedTextNode && selectedTextNode === firstLinkTextNode) {
      if ($isRangeSelection2(selection)) {
        textNode.select(selection.anchor.offset, selection.focus.offset);
      } else if ($isNodeSelection2(selection)) {
        textNode.select(0, textNode.getTextContent().length);
      }
    }
    firstLinkTextNode.replace(linkNode);
    return remainingTextNode;
  }
  return void 0;
}
function $handleLinkCreation(nodes, matchers, onChange, separatorRegex) {
  for (const node of nodes) {
    const parent = node.getParent();
    if ($isAutoLinkNode(parent) && !parent.getIsUnlinked()) {
      return;
    }
  }
  let currentNodes = [...nodes];
  const initialText = currentNodes.map((node) => node.getTextContent()).join("");
  let text = initialText;
  let match;
  let invalidMatchEnd = 0;
  while ((match = findFirstMatch(text, matchers)) && match !== null) {
    const matchStart = match.index;
    const matchLength = match.length;
    const matchEnd = matchStart + matchLength;
    const isValid = isContentAroundIsValid(invalidMatchEnd + matchStart, invalidMatchEnd + matchEnd, separatorRegex, initialText, currentNodes);
    if (isValid) {
      const [matchingOffset, , matchingNodes, unmodifiedAfterNodes] = extractMatchingNodes(currentNodes, invalidMatchEnd + matchStart, invalidMatchEnd + matchEnd);
      let alreadyLinked = false;
      for (const node of matchingNodes) {
        const parent = node.getParent();
        if ($isAutoLinkNode(parent) && !parent.getIsUnlinked()) {
          alreadyLinked = true;
          break;
        }
      }
      if (alreadyLinked) {
        invalidMatchEnd += matchEnd;
        text = text.substring(matchEnd);
        continue;
      }
      const actualMatchStart = invalidMatchEnd + matchStart - matchingOffset;
      const actualMatchEnd = invalidMatchEnd + matchEnd - matchingOffset;
      const remainingTextNode = $createAutoLinkNode_(matchingNodes, actualMatchStart, actualMatchEnd, match);
      currentNodes = remainingTextNode ? [remainingTextNode, ...unmodifiedAfterNodes] : unmodifiedAfterNodes;
      onChange(match.url, null);
      invalidMatchEnd = 0;
    } else {
      invalidMatchEnd += matchEnd;
    }
    text = text.substring(matchEnd);
  }
}
function handleLinkEdit(linkNode, matchers, onChange, separatorRegex) {
  const children = linkNode.getChildren();
  const childrenLength = children.length;
  for (let i2 = 0; i2 < childrenLength; i2++) {
    const child = children[i2];
    if (!$isTextNode2(child) || !child.isSimpleText()) {
      replaceWithChildren(linkNode);
      onChange(null, linkNode.getURL());
      return;
    }
  }
  const text = linkNode.getTextContent();
  const match = findFirstMatch(text, matchers);
  if (match === null || match.text !== text) {
    replaceWithChildren(linkNode);
    onChange(null, linkNode.getURL());
    return;
  }
  if (!isPreviousNodeValid(linkNode, separatorRegex) || !isNextNodeValid(linkNode, separatorRegex)) {
    replaceWithChildren(linkNode);
    onChange(null, linkNode.getURL());
    return;
  }
  const url = linkNode.getURL();
  if (url !== match.url) {
    linkNode.setURL(match.url);
    onChange(match.url, url);
  }
  if (match.attributes) {
    const rel = linkNode.getRel();
    if (rel !== match.attributes.rel) {
      linkNode.setRel(match.attributes.rel || null);
      onChange(match.attributes.rel || null, rel);
    }
    const target = linkNode.getTarget();
    if (target !== match.attributes.target) {
      linkNode.setTarget(match.attributes.target || null);
      onChange(match.attributes.target || null, target);
    }
  }
}
function handleBadNeighbors(textNode, matchers, onChange, separatorRegex) {
  const parent = textNode.getParent();
  const previousSibling = textNode.getPreviousSibling();
  const nextSibling = textNode.getNextSibling();
  const text = textNode.getTextContent();
  if ($isAutoLinkNode(parent) && !parent.getIsUnlinked()) {
    return;
  }
  if ($isAutoLinkNode(previousSibling) && !previousSibling.getIsUnlinked()) {
    if (previousSibling.is(textNode.getPreviousSibling()) && textNode.getParent() === previousSibling.getParent()) {
      if (!startsWithSeparator(text, separatorRegex)) {
        replaceWithChildren(previousSibling);
        onChange(null, previousSibling.getURL());
        return;
      }
      if (startsWithTLD(text, previousSibling.isEmailURI())) {
        const combinedText = previousSibling.getTextContent() + text;
        const match = findFirstMatch(combinedText, matchers);
        if (match !== null && match.text === combinedText) {
          previousSibling.append(textNode);
          handleLinkEdit(previousSibling, matchers, onChange, separatorRegex);
          onChange(null, previousSibling.getURL());
        }
      }
    }
  }
  if ($isAutoLinkNode(nextSibling) && !nextSibling.getIsUnlinked() && !endsWithSeparator(text, separatorRegex)) {
    if (nextSibling.is(textNode.getNextSibling()) && textNode.getParent() === nextSibling.getParent()) {
      replaceWithChildren(nextSibling);
      onChange(null, nextSibling.getURL());
    }
  }
}
function replaceWithChildren(node) {
  const children = node.getChildren();
  const childrenLength = children.length;
  for (let j2 = childrenLength - 1; j2 >= 0; j2--) {
    node.insertAfter(children[j2]);
  }
  node.remove();
  return children.map((child) => child.getLatest());
}
function getTextNodesToMatch(textNode) {
  const textNodesToMatch = [textNode];
  let nextSibling = textNode.getNextSibling();
  while (nextSibling !== null && $isTextNode2(nextSibling) && nextSibling.isSimpleText()) {
    textNodesToMatch.push(nextSibling);
    if (/[\s]/.test(nextSibling.getTextContent())) {
      break;
    }
    nextSibling = nextSibling.getNextSibling();
  }
  return textNodesToMatch;
}
var defaultConfig = {
  changeHandlers: [],
  excludeParents: [],
  matchers: [],
  separatorRegex: PUNCTUATION_OR_SPACE
};
function registerAutoLink(editor, config = defaultConfig) {
  const {
    matchers,
    changeHandlers,
    excludeParents,
    separatorRegex = PUNCTUATION_OR_SPACE
  } = config;
  const onChange = (url, prevUrl) => {
    for (const handler of changeHandlers) {
      handler(url, prevUrl);
    }
  };
  return mergeRegister3(editor.registerNodeTransform(TextNode2, (textNode) => {
    const parent = textNode.getParentOrThrow();
    const previous = textNode.getPreviousSibling();
    if ($isAutoLinkNode(parent)) {
      handleLinkEdit(parent, matchers, onChange, separatorRegex);
    } else if (!$isLinkNode(parent) && !excludeParents.some((pred) => pred(parent))) {
      if (textNode.isSimpleText() && (startsWithSeparator(textNode.getTextContent(), separatorRegex) || !$isAutoLinkNode(previous))) {
        const textNodesToMatch = getTextNodesToMatch(textNode);
        $handleLinkCreation(textNodesToMatch, matchers, onChange, separatorRegex);
      }
      handleBadNeighbors(textNode, matchers, onChange, separatorRegex);
    }
  }), editor.registerCommand(
    TOGGLE_LINK_COMMAND,
    (payload) => {
      const selection = $getSelection2();
      if (payload !== null || !$isRangeSelection2(selection)) {
        return false;
      }
      const nodes = selection.extract();
      nodes.forEach((node) => {
        const parent = node.getParent();
        if ($isAutoLinkNode(parent)) {
          parent.setIsUnlinked(!parent.getIsUnlinked());
          parent.markDirty();
        }
      });
      return false;
    },
    // Has to be higher than TOGGLE_LINK_COMMAND in LinkExtension
    COMMAND_PRIORITY_LOW2
  ));
}
var AutoLinkExtension = defineExtension2({
  config: defaultConfig,
  dependencies: [LinkExtension],
  mergeConfig(config, overrides) {
    const merged = shallowMergeConfig2(config, overrides);
    for (const k of ["matchers", "changeHandlers", "excludeParents"]) {
      const v2 = overrides[k];
      if (Array.isArray(v2)) {
        merged[k] = [...config[k], ...v2];
      }
    }
    return merged;
  },
  name: "@lexical/link/AutoLink",
  nodes: [AutoLinkNode],
  register: registerAutoLink
});
var toggleLink = $toggleLink;

// node_modules/@lexical/link/LexicalLink.mjs
var mod10 = true ? LexicalLink_dev_exports : LexicalLink_prod_exports;
var $createAutoLinkNode2 = mod10.$createAutoLinkNode;
var $createLinkNode2 = mod10.$createLinkNode;
var $isAutoLinkNode2 = mod10.$isAutoLinkNode;
var $isLinkNode2 = mod10.$isLinkNode;
var $toggleLink2 = mod10.$toggleLink;
var AutoLinkExtension2 = mod10.AutoLinkExtension;
var AutoLinkNode2 = mod10.AutoLinkNode;
var ClickableLinkExtension2 = mod10.ClickableLinkExtension;
var LinkExtension2 = mod10.LinkExtension;
var LinkNode2 = mod10.LinkNode;
var TOGGLE_LINK_COMMAND2 = mod10.TOGGLE_LINK_COMMAND;
var createLinkMatcherWithRegExp2 = mod10.createLinkMatcherWithRegExp;
var formatUrl2 = mod10.formatUrl;
var registerAutoLink2 = mod10.registerAutoLink;
var registerClickableLink2 = mod10.registerClickableLink;
var registerLink2 = mod10.registerLink;
var toggleLink2 = mod10.toggleLink;

// node_modules/@lexical/code/LexicalCode.dev.mjs
var LexicalCode_dev_exports = {};
__export(LexicalCode_dev_exports, {
  $createCodeHighlightNode: () => $createCodeHighlightNode2,
  $createCodeNode: () => $createCodeNode2,
  $getCodeLineDirection: () => $getCodeLineDirection2,
  $getEndOfCodeInLine: () => $getEndOfCodeInLine2,
  $getFirstCodeNodeOfLine: () => $getFirstCodeNodeOfLine2,
  $getLastCodeNodeOfLine: () => $getLastCodeNodeOfLine2,
  $getStartOfCodeInLine: () => $getStartOfCodeInLine2,
  $isCodeHighlightNode: () => $isCodeHighlightNode2,
  $isCodeNode: () => $isCodeNode2,
  CODE_LANGUAGE_FRIENDLY_NAME_MAP: () => CODE_LANGUAGE_FRIENDLY_NAME_MAP3,
  CODE_LANGUAGE_MAP: () => CODE_LANGUAGE_MAP3,
  CodeExtension: () => CodeExtension2,
  CodeHighlightNode: () => CodeHighlightNode2,
  CodeNode: () => CodeNode2,
  DEFAULT_CODE_LANGUAGE: () => DEFAULT_CODE_LANGUAGE2,
  PrismTokenizer: () => PrismTokenizer3,
  getCodeLanguageOptions: () => getCodeLanguageOptions3,
  getCodeLanguages: () => getCodeLanguages3,
  getCodeThemeOptions: () => getCodeThemeOptions3,
  getDefaultCodeLanguage: () => getDefaultCodeLanguage2,
  getLanguageFriendlyName: () => getLanguageFriendlyName3,
  normalizeCodeLang: () => normalizeCodeLang,
  normalizeCodeLanguage: () => normalizeCodeLanguage3,
  registerCodeHighlighting: () => registerCodeHighlighting3
});

// node_modules/@lexical/code-prism/LexicalCodePrism.dev.mjs
var LexicalCodePrism_dev_exports = {};
__export(LexicalCodePrism_dev_exports, {
  CODE_LANGUAGE_FRIENDLY_NAME_MAP: () => CODE_LANGUAGE_FRIENDLY_NAME_MAP,
  CODE_LANGUAGE_MAP: () => CODE_LANGUAGE_MAP,
  CodePrismExtension: () => CodePrismExtension,
  PrismTokenizer: () => PrismTokenizer,
  getCodeLanguageOptions: () => getCodeLanguageOptions,
  getCodeLanguages: () => getCodeLanguages,
  getCodeThemeOptions: () => getCodeThemeOptions,
  getLanguageFriendlyName: () => getLanguageFriendlyName,
  isCodeLanguageLoaded: () => isCodeLanguageLoaded,
  loadCodeLanguage: () => loadCodeLanguage,
  normalizeCodeLanguage: () => normalizeCodeLanguage,
  registerCodeHighlighting: () => registerCodeHighlighting
});

// node_modules/@lexical/code-core/LexicalCodeCore.dev.mjs
var LexicalCodeCore_dev_exports = {};
__export(LexicalCodeCore_dev_exports, {
  $createCodeHighlightNode: () => $createCodeHighlightNode,
  $createCodeNode: () => $createCodeNode,
  $getCodeLineDirection: () => $getCodeLineDirection,
  $getEndOfCodeInLine: () => $getEndOfCodeInLine,
  $getFirstCodeNodeOfLine: () => $getFirstCodeNodeOfLine,
  $getLastCodeNodeOfLine: () => $getLastCodeNodeOfLine,
  $getStartOfCodeInLine: () => $getStartOfCodeInLine,
  $isCodeHighlightNode: () => $isCodeHighlightNode,
  $isCodeNode: () => $isCodeNode,
  CodeExtension: () => CodeExtension,
  CodeHighlightNode: () => CodeHighlightNode,
  CodeNode: () => CodeNode,
  DEFAULT_CODE_LANGUAGE: () => DEFAULT_CODE_LANGUAGE,
  getDefaultCodeLanguage: () => getDefaultCodeLanguage
});
function warnOnlyOnce3(message) {
  {
    let run = false;
    return () => {
      if (!run) {
        console.warn(message);
      }
      run = true;
    };
  }
}
function formatDevErrorMessage9(message) {
  throw new Error(message);
}
function $getLastMatchingCodeNode(anchor, direction) {
  let matchingNode = anchor;
  for (let caret = $getSiblingCaret2(anchor, direction); caret && ($isCodeHighlightNode(caret.origin) || $isTabNode2(caret.origin)); caret = caret.getAdjacentCaret()) {
    matchingNode = caret.origin;
  }
  return matchingNode;
}
function $getFirstCodeNodeOfLine(anchor) {
  return $getLastMatchingCodeNode(anchor, "previous");
}
function $getLastCodeNodeOfLine(anchor) {
  return $getLastMatchingCodeNode(anchor, "next");
}
function $getCodeLineDirection(anchor) {
  const start = $getFirstCodeNodeOfLine(anchor);
  const end = $getLastCodeNodeOfLine(anchor);
  let node = start;
  while (node !== null) {
    if ($isCodeHighlightNode(node)) {
      const direction = getTextDirection2(node.getTextContent());
      if (direction !== null) {
        return direction;
      }
    }
    if (node === end) {
      break;
    }
    node = node.getNextSibling();
  }
  const parent = start.getParent();
  if ($isElementNode2(parent)) {
    const parentDirection = parent.getDirection();
    if (parentDirection === "ltr" || parentDirection === "rtl") {
      return parentDirection;
    }
  }
  return null;
}
function $getStartOfCodeInLine(anchor, offset) {
  let last = null;
  let lastNonBlank = null;
  let node = anchor;
  let nodeOffset = offset;
  let nodeTextContent = anchor.getTextContent();
  while (true) {
    if (nodeOffset === 0) {
      node = node.getPreviousSibling();
      if (node === null) {
        break;
      }
      if (!($isCodeHighlightNode(node) || $isTabNode2(node) || $isLineBreakNode2(node))) {
        formatDevErrorMessage9(`Expected a valid Code Node: CodeHighlightNode, TabNode, LineBreakNode`);
      }
      if ($isLineBreakNode2(node)) {
        last = {
          node,
          offset: 1
        };
        break;
      }
      nodeOffset = Math.max(0, node.getTextContentSize() - 1);
      nodeTextContent = node.getTextContent();
    } else {
      nodeOffset--;
    }
    const character = nodeTextContent[nodeOffset];
    if ($isCodeHighlightNode(node) && character !== " ") {
      lastNonBlank = {
        node,
        offset: nodeOffset
      };
    }
  }
  if (lastNonBlank !== null) {
    return lastNonBlank;
  }
  let codeCharacterAtAnchorOffset = null;
  if (offset < anchor.getTextContentSize()) {
    if ($isCodeHighlightNode(anchor)) {
      codeCharacterAtAnchorOffset = anchor.getTextContent()[offset];
    }
  } else {
    const nextSibling = anchor.getNextSibling();
    if ($isCodeHighlightNode(nextSibling)) {
      codeCharacterAtAnchorOffset = nextSibling.getTextContent()[0];
    }
  }
  if (codeCharacterAtAnchorOffset !== null && codeCharacterAtAnchorOffset !== " ") {
    return last;
  } else {
    const nextNonBlank = findNextNonBlankInLine(anchor, offset);
    if (nextNonBlank !== null) {
      return nextNonBlank;
    } else {
      return last;
    }
  }
}
function findNextNonBlankInLine(anchor, offset) {
  let node = anchor;
  let nodeOffset = offset;
  let nodeTextContent = anchor.getTextContent();
  let nodeTextContentSize = anchor.getTextContentSize();
  while (true) {
    if (!$isCodeHighlightNode(node) || nodeOffset === nodeTextContentSize) {
      node = node.getNextSibling();
      if (node === null || $isLineBreakNode2(node)) {
        return null;
      }
      if ($isCodeHighlightNode(node)) {
        nodeOffset = 0;
        nodeTextContent = node.getTextContent();
        nodeTextContentSize = node.getTextContentSize();
      }
    }
    if ($isCodeHighlightNode(node)) {
      if (nodeTextContent[nodeOffset] !== " ") {
        return {
          node,
          offset: nodeOffset
        };
      }
      nodeOffset++;
    }
  }
}
function $getEndOfCodeInLine(anchor) {
  const lastNode = $getLastCodeNodeOfLine(anchor);
  if (!!$isLineBreakNode2(lastNode)) {
    formatDevErrorMessage9(`Unexpected lineBreakNode in getEndOfCodeInLine`);
  }
  return lastNode;
}
var DEFAULT_CODE_LANGUAGE = "javascript";
var getDefaultCodeLanguage = () => DEFAULT_CODE_LANGUAGE;
function hasChildDOMNodeTag(node, tagName) {
  for (const child of node.childNodes) {
    if (isHTMLElement2(child) && child.tagName === tagName) {
      return true;
    }
    hasChildDOMNodeTag(child, tagName);
  }
  return false;
}
var LANGUAGE_DATA_ATTRIBUTE = "data-language";
var HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE = "data-highlight-language";
var THEME_DATA_ATTRIBUTE = "data-theme";
var noExtensionDeprecation = warnOnlyOnce3("Using CodeNode without CodeExtension is deprecated");
var CodeNode = class _CodeNode extends ElementNode2 {
  /** @internal */
  __language;
  /** @internal */
  __theme;
  /** @internal */
  __isSyntaxHighlightSupported;
  static getType() {
    return "code";
  }
  static clone(node) {
    return new _CodeNode(node.__language, node.__key);
  }
  constructor(language, key) {
    super(key);
    this.__language = language || void 0;
    this.__isSyntaxHighlightSupported = false;
    this.__theme = void 0;
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__language = prevNode.__language;
    this.__theme = prevNode.__theme;
    this.__isSyntaxHighlightSupported = prevNode.__isSyntaxHighlightSupported;
  }
  // View
  createDOM(config) {
    const element = document.createElement("code");
    addClassNamesToElement2(element, config.theme.code);
    element.setAttribute("spellcheck", "false");
    const language = this.getLanguage();
    if (language) {
      element.setAttribute(LANGUAGE_DATA_ATTRIBUTE, language);
      if (this.getIsSyntaxHighlightSupported()) {
        element.setAttribute(HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE, language);
      }
    }
    const theme = this.getTheme();
    if (theme) {
      element.setAttribute(THEME_DATA_ATTRIBUTE, theme);
    }
    const style = this.getStyle();
    if (style) {
      setDOMStyleFromCSS2(element.style, style);
    }
    return element;
  }
  updateDOM(prevNode, dom, config) {
    const language = this.__language;
    const prevLanguage = prevNode.__language;
    if (language) {
      if (language !== prevLanguage) {
        dom.setAttribute(LANGUAGE_DATA_ATTRIBUTE, language);
      }
    } else if (prevLanguage) {
      dom.removeAttribute(LANGUAGE_DATA_ATTRIBUTE);
    }
    const isSyntaxHighlightSupported = this.__isSyntaxHighlightSupported;
    const prevIsSyntaxHighlightSupported = prevNode.__isSyntaxHighlightSupported;
    if (prevIsSyntaxHighlightSupported && prevLanguage) {
      if (isSyntaxHighlightSupported && language) {
        if (language !== prevLanguage) {
          dom.setAttribute(HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE, language);
        }
      } else {
        dom.removeAttribute(HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE);
      }
    } else if (isSyntaxHighlightSupported && language) {
      dom.setAttribute(HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE, language);
    }
    const theme = this.__theme;
    const prevTheme = prevNode.__theme;
    if (theme) {
      if (theme !== prevTheme) {
        dom.setAttribute(THEME_DATA_ATTRIBUTE, theme);
      }
    } else if (prevTheme) {
      dom.removeAttribute(THEME_DATA_ATTRIBUTE);
    }
    const style = this.__style;
    const prevStyle = prevNode.__style;
    if (style !== prevStyle) {
      setDOMStyleFromCSS2(dom.style, style, prevStyle);
    }
    return false;
  }
  exportDOM(editor) {
    const element = document.createElement("pre");
    addClassNamesToElement2(element, editor._config.theme.code);
    element.setAttribute("spellcheck", "false");
    const language = this.getLanguage();
    if (language) {
      element.setAttribute(LANGUAGE_DATA_ATTRIBUTE, language);
      if (this.getIsSyntaxHighlightSupported()) {
        element.setAttribute(HIGHLIGHT_LANGUAGE_DATA_ATTRIBUTE, language);
      }
    }
    const theme = this.getTheme();
    if (theme) {
      element.setAttribute(THEME_DATA_ATTRIBUTE, theme);
    }
    const style = this.getStyle();
    if (style) {
      setDOMStyleFromCSS2(element.style, style);
    }
    return {
      element
    };
  }
  static importDOM() {
    return {
      // Typically <pre> is used for code blocks, and <code> for inline code styles
      // but if it's a multi line <code> we'll create a block. Pass through to
      // inline format handled by TextNode otherwise.
      code: (node) => {
        const isMultiLine = node.textContent != null && (/\r?\n/.test(node.textContent) || hasChildDOMNodeTag(node, "BR"));
        return isMultiLine ? {
          conversion: $convertPreElement,
          priority: 1
        } : null;
      },
      div: () => ({
        conversion: $convertDivElement,
        priority: 1
      }),
      pre: () => ({
        conversion: $convertPreElement,
        priority: 0
      }),
      table: (node) => {
        const table = node;
        if (isGitHubCodeTable(table)) {
          return {
            conversion: $convertTableElement,
            priority: 3
          };
        }
        return null;
      },
      td: (node) => {
        const td = node;
        const table = td.closest("table");
        if (isGitHubCodeCell(td) || table && isGitHubCodeTable(table)) {
          return {
            conversion: convertCodeNoop,
            priority: 3
          };
        }
        return null;
      },
      tr: (node) => {
        const tr = node;
        const table = tr.closest("table");
        if (table && isGitHubCodeTable(table)) {
          return {
            conversion: convertCodeNoop,
            priority: 3
          };
        }
        return null;
      }
    };
  }
  static importJSON(serializedNode) {
    return $createCodeNode().updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setLanguage(serializedNode.language).setTheme(serializedNode.theme);
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      language: this.getLanguage(),
      theme: this.getTheme()
    };
  }
  // Mutation
  insertNewAfter(selection, restoreSelection = true) {
    if (!getPeerDependencyFromEditor2($getEditor2(), "@lexical/code")) {
      noExtensionDeprecation();
      const el = $exitCodeNodeOnEnter(selection);
      if (el) {
        return el;
      }
    }
    const {
      anchor,
      focus
    } = selection;
    const firstPoint = anchor.isBefore(focus) ? anchor : focus;
    const firstSelectionNode = firstPoint.getNode();
    if ($isTextNode2(firstSelectionNode)) {
      let node = $getFirstCodeNodeOfLine(firstSelectionNode);
      const insertNodes = [];
      while (true) {
        if ($isTabNode2(node)) {
          insertNodes.push($createTabNode2());
          node = node.getNextSibling();
        } else if ($isCodeHighlightNode(node)) {
          let spaces = 0;
          const text = node.getTextContent();
          const textSize = node.getTextContentSize();
          while (spaces < textSize && text[spaces] === " ") {
            spaces++;
          }
          if (spaces !== 0) {
            insertNodes.push($createCodeHighlightNode(" ".repeat(spaces)));
          }
          if (spaces !== textSize) {
            break;
          }
          node = node.getNextSibling();
        } else {
          break;
        }
      }
      const split = firstSelectionNode.splitText(anchor.offset)[0];
      const x2 = anchor.offset === 0 ? 0 : 1;
      const index = split.getIndexWithinParent() + x2;
      const codeNode = firstSelectionNode.getParentOrThrow();
      const nodesToInsert = [$createLineBreakNode2(), ...insertNodes];
      codeNode.splice(index, 0, nodesToInsert);
      const last = insertNodes[insertNodes.length - 1];
      if (last) {
        last.select();
      } else if (anchor.offset === 0) {
        split.selectPrevious();
      } else {
        split.getNextSibling().selectNext(0, 0);
      }
    }
    if ($isCodeNode(firstSelectionNode)) {
      const {
        offset
      } = selection.anchor;
      firstSelectionNode.splice(offset, 0, [$createLineBreakNode2()]);
      firstSelectionNode.select(offset + 1, offset + 1);
    }
    return null;
  }
  canIndent() {
    return false;
  }
  collapseAtStart() {
    const paragraph = $createParagraphNode2();
    const children = this.getChildren();
    children.forEach((child) => paragraph.append(child));
    this.replace(paragraph);
    return true;
  }
  setLanguage(language) {
    const writable = this.getWritable();
    writable.__language = language || void 0;
    return writable;
  }
  getLanguage() {
    return this.getLatest().__language;
  }
  setIsSyntaxHighlightSupported(isSupported) {
    const writable = this.getWritable();
    writable.__isSyntaxHighlightSupported = isSupported;
    return writable;
  }
  getIsSyntaxHighlightSupported() {
    return this.getLatest().__isSyntaxHighlightSupported;
  }
  setTheme(theme) {
    const writable = this.getWritable();
    writable.__theme = theme || void 0;
    return writable;
  }
  getTheme() {
    return this.getLatest().__theme;
  }
};
function $createCodeNode(language, theme) {
  return $create2(CodeNode).setLanguage(language).setTheme(theme);
}
function $isCodeNode(node) {
  return node instanceof CodeNode;
}
function $convertPreElement(domNode) {
  const language = domNode.getAttribute(LANGUAGE_DATA_ATTRIBUTE);
  return {
    node: $createCodeNode(language)
  };
}
function $convertDivElement(domNode) {
  const div = domNode;
  const isCode = isCodeElement(div);
  if (!isCode && !isCodeChildElement(div)) {
    return {
      node: null
    };
  }
  return {
    node: isCode ? $createCodeNode() : null
  };
}
function $convertTableElement() {
  return {
    node: $createCodeNode()
  };
}
function convertCodeNoop() {
  return {
    node: null
  };
}
function isCodeElement(div) {
  return div.style.fontFamily.match("monospace") !== null;
}
function isCodeChildElement(node) {
  let parent = node.parentElement;
  while (parent !== null) {
    if (isCodeElement(parent)) {
      return true;
    }
    parent = parent.parentElement;
  }
  return false;
}
function isGitHubCodeCell(cell) {
  return cell.classList.contains("js-file-line");
}
function isGitHubCodeTable(table) {
  return table.classList.contains("js-file-line-container");
}
function $exitCodeNodeOnEnter(selection) {
  const {
    anchor
  } = selection;
  if (selection.isCollapsed() && anchor.type === "element") {
    const codeNode = anchor.getNode();
    if ($isCodeNode(codeNode)) {
      const childrenSize = codeNode.getChildrenSize();
      if (childrenSize >= 2 && anchor.offset === childrenSize) {
        const lastChild = codeNode.getLastChild();
        if ($isLineBreakNode2(lastChild) && $isLineBreakNode2(lastChild.getPreviousSibling())) {
          const newElement = $createParagraphNode2();
          codeNode.splice(childrenSize - 2, 2, []).insertAfter(newElement, false);
          newElement.select();
          return newElement;
        }
      }
    }
  }
  return null;
}
var CodeHighlightNode = class _CodeHighlightNode extends TextNode2 {
  /** @internal */
  __highlightType;
  constructor(text = "", highlightType, key) {
    super(text, key);
    this.__highlightType = highlightType;
  }
  static getType() {
    return "code-highlight";
  }
  static clone(node) {
    return new _CodeHighlightNode(node.__text, node.__highlightType || void 0, node.__key);
  }
  afterCloneFrom(prevNode) {
    super.afterCloneFrom(prevNode);
    this.__highlightType = prevNode.__highlightType;
  }
  getHighlightType() {
    const self2 = this.getLatest();
    return self2.__highlightType;
  }
  setHighlightType(highlightType) {
    const self2 = this.getWritable();
    self2.__highlightType = highlightType || void 0;
    return self2;
  }
  canHaveFormat() {
    return false;
  }
  createDOM(config) {
    const element = super.createDOM(config);
    const className = getHighlightThemeClass(config.theme, this.__highlightType);
    addClassNamesToElement2(element, className);
    return element;
  }
  updateDOM(prevNode, dom, config) {
    const update = super.updateDOM(prevNode, dom, config);
    const prevClassName = getHighlightThemeClass(config.theme, prevNode.__highlightType);
    const nextClassName = getHighlightThemeClass(config.theme, this.__highlightType);
    if (prevClassName !== nextClassName) {
      if (prevClassName) {
        removeClassNamesFromElement2(dom, prevClassName);
      }
      if (nextClassName) {
        addClassNamesToElement2(dom, nextClassName);
      }
    }
    return update;
  }
  static importJSON(serializedNode) {
    return $createCodeHighlightNode().updateFromJSON(serializedNode);
  }
  updateFromJSON(serializedNode) {
    return super.updateFromJSON(serializedNode).setHighlightType(serializedNode.highlightType);
  }
  exportJSON() {
    return {
      ...super.exportJSON(),
      highlightType: this.getHighlightType()
    };
  }
  // Prevent formatting (bold, underline, etc)
  setFormat(format) {
    return this;
  }
  isParentRequired() {
    return true;
  }
  createParentElementNode() {
    return $createCodeNode();
  }
};
function getHighlightThemeClass(theme, highlightType) {
  return highlightType && theme && theme.codeHighlight && theme.codeHighlight[highlightType];
}
function $createCodeHighlightNode(text = "", highlightType) {
  return $applyNodeReplacement2(new CodeHighlightNode(text, highlightType));
}
function $isCodeHighlightNode(node) {
  return node instanceof CodeHighlightNode;
}
var CodeExtension = defineExtension2({
  name: "@lexical/code",
  nodes: () => [CodeNode, CodeHighlightNode],
  register(editor) {
    return editor.registerCommand(KEY_ENTER_COMMAND2, (event) => {
      const selection = $getSelection2();
      if ($isRangeSelection2(selection) && $exitCodeNodeOnEnter(selection)) {
        event.preventDefault();
        return true;
      }
      return false;
    }, COMMAND_PRIORITY_LOW2);
  }
});

// node_modules/@lexical/code-core/LexicalCodeCore.mjs
var mod11 = true ? LexicalCodeCore_dev_exports : LexicalCodeCore_prod_exports;
var $createCodeHighlightNode2 = mod11.$createCodeHighlightNode;
var $createCodeNode2 = mod11.$createCodeNode;
var $getCodeLineDirection2 = mod11.$getCodeLineDirection;
var $getEndOfCodeInLine2 = mod11.$getEndOfCodeInLine;
var $getFirstCodeNodeOfLine2 = mod11.$getFirstCodeNodeOfLine;
var $getLastCodeNodeOfLine2 = mod11.$getLastCodeNodeOfLine;
var $getStartOfCodeInLine2 = mod11.$getStartOfCodeInLine;
var $isCodeHighlightNode2 = mod11.$isCodeHighlightNode;
var $isCodeNode2 = mod11.$isCodeNode;
var CodeExtension2 = mod11.CodeExtension;
var CodeHighlightNode2 = mod11.CodeHighlightNode;
var CodeNode2 = mod11.CodeNode;
var DEFAULT_CODE_LANGUAGE2 = mod11.DEFAULT_CODE_LANGUAGE;
var getDefaultCodeLanguage2 = mod11.getDefaultCodeLanguage;

// node_modules/@lexical/code-prism/LexicalCodePrism.dev.mjs
var import_prismjs = __toESM(require_prism(), 1);

// node_modules/prismjs/components/prism-clike.js
Prism.languages.clike = {
  "comment": [
    {
      pattern: /(^|[^\\])\/\*[\s\S]*?(?:\*\/|$)/,
      lookbehind: true,
      greedy: true
    },
    {
      pattern: /(^|[^\\:])\/\/.*/,
      lookbehind: true,
      greedy: true
    }
  ],
  "string": {
    pattern: /(["'])(?:\\(?:\r\n|[\s\S])|(?!\1)[^\\\r\n])*\1/,
    greedy: true
  },
  "class-name": {
    pattern: /(\b(?:class|extends|implements|instanceof|interface|new|trait)\s+|\bcatch\s+\()[\w.\\]+/i,
    lookbehind: true,
    inside: {
      "punctuation": /[.\\]/
    }
  },
  "keyword": /\b(?:break|catch|continue|do|else|finally|for|function|if|in|instanceof|new|null|return|throw|try|while)\b/,
  "boolean": /\b(?:false|true)\b/,
  "function": /\b\w+(?=\()/,
  "number": /\b0x[\da-f]+\b|(?:\b\d+(?:\.\d*)?|\B\.\d+)(?:e[+-]?\d+)?/i,
  "operator": /[<>]=?|[!=]=?=?|--?|\+\+?|&&?|\|\|?|[?*/~^%]/,
  "punctuation": /[{}[\];(),.:]/
};

// node_modules/prismjs/components/prism-javascript.js
Prism.languages.javascript = Prism.languages.extend("clike", {
  "class-name": [
    Prism.languages.clike["class-name"],
    {
      pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$A-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\.(?:constructor|prototype))/,
      lookbehind: true
    }
  ],
  "keyword": [
    {
      pattern: /((?:^|\})\s*)catch\b/,
      lookbehind: true
    },
    {
      pattern: /(^|[^.]|\.\.\.\s*)\b(?:as|assert(?=\s*\{)|async(?=\s*(?:function\b|\(|[$\w\xA0-\uFFFF]|$))|await|break|case|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally(?=\s*(?:\{|$))|for|from(?=\s*(?:['"]|$))|function|(?:get|set)(?=\s*(?:[#\[$\w\xA0-\uFFFF]|$))|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)\b/,
      lookbehind: true
    }
  ],
  // Allow for all non-ASCII characters (See http://stackoverflow.com/a/2008444)
  "function": /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*(?:\.\s*(?:apply|bind|call)\s*)?\()/,
  "number": {
    pattern: RegExp(
      /(^|[^\w$])/.source + "(?:" + // constant
      (/NaN|Infinity/.source + "|" + // binary integer
      /0[bB][01]+(?:_[01]+)*n?/.source + "|" + // octal integer
      /0[oO][0-7]+(?:_[0-7]+)*n?/.source + "|" + // hexadecimal integer
      /0[xX][\dA-Fa-f]+(?:_[\dA-Fa-f]+)*n?/.source + "|" + // decimal bigint
      /\d+(?:_\d+)*n/.source + "|" + // decimal number (integer or float) but no bigint
      /(?:\d+(?:_\d+)*(?:\.(?:\d+(?:_\d+)*)?)?|\.\d+(?:_\d+)*)(?:[Ee][+-]?\d+(?:_\d+)*)?/.source) + ")" + /(?![\w$])/.source
    ),
    lookbehind: true
  },
  "operator": /--|\+\+|\*\*=?|=>|&&=?|\|\|=?|[!=]==|<<=?|>>>?=?|[-+*/%&|^!=<>]=?|\.{3}|\?\?=?|\?\.?|[~:]/
});
Prism.languages.javascript["class-name"][0].pattern = /(\b(?:class|extends|implements|instanceof|interface|new)\s+)[\w.\\]+/;
Prism.languages.insertBefore("javascript", "keyword", {
  "regex": {
    pattern: RegExp(
      // lookbehind
      // eslint-disable-next-line regexp/no-dupe-characters-character-class
      /((?:^|[^$\w\xA0-\uFFFF."'\])\s]|\b(?:return|yield))\s*)/.source + // Regex pattern:
      // There are 2 regex patterns here. The RegExp set notation proposal added support for nested character
      // classes if the `v` flag is present. Unfortunately, nested CCs are both context-free and incompatible
      // with the only syntax, so we have to define 2 different regex patterns.
      /\//.source + "(?:" + /(?:\[(?:[^\]\\\r\n]|\\.)*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}/.source + "|" + // `v` flag syntax. This supports 3 levels of nested character classes.
      /(?:\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.)*\])*\])*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}v[dgimyus]{0,7}/.source + ")" + // lookahead
      /(?=(?:\s|\/\*(?:[^*]|\*(?!\/))*\*\/)*(?:$|[\r\n,.;:})\]]|\/\/))/.source
    ),
    lookbehind: true,
    greedy: true,
    inside: {
      "regex-source": {
        pattern: /^(\/)[\s\S]+(?=\/[a-z]*$)/,
        lookbehind: true,
        alias: "language-regex",
        inside: Prism.languages.regex
      },
      "regex-delimiter": /^\/|\/$/,
      "regex-flags": /^[a-z]+$/
    }
  },
  // This must be declared before keyword because we use "function" inside the look-forward
  "function-variable": {
    pattern: /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*[=:]\s*(?:async\s*)?(?:\bfunction\b|(?:\((?:[^()]|\([^()]*\))*\)|(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)\s*=>))/,
    alias: "function"
  },
  "parameter": [
    {
      pattern: /(function(?:\s+(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)?\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\))/,
      lookbehind: true,
      inside: Prism.languages.javascript
    },
    {
      pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$a-z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*=>)/i,
      lookbehind: true,
      inside: Prism.languages.javascript
    },
    {
      pattern: /(\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*=>)/,
      lookbehind: true,
      inside: Prism.languages.javascript
    },
    {
      pattern: /((?:\b|\s|^)(?!(?:as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally|for|from|function|get|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|set|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)(?![$\w\xA0-\uFFFF]))(?:(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*\s*)\(\s*|\]\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*\{)/,
      lookbehind: true,
      inside: Prism.languages.javascript
    }
  ],
  "constant": /\b[A-Z](?:[A-Z_]|\dx?)*\b/
});
Prism.languages.insertBefore("javascript", "string", {
  "hashbang": {
    pattern: /^#!.*/,
    greedy: true,
    alias: "comment"
  },
  "template-string": {
    pattern: /`(?:\\[\s\S]|\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}|(?!\$\{)[^\\`])*`/,
    greedy: true,
    inside: {
      "template-punctuation": {
        pattern: /^`|`$/,
        alias: "string"
      },
      "interpolation": {
        pattern: /((?:^|[^\\])(?:\\{2})*)\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}/,
        lookbehind: true,
        inside: {
          "interpolation-punctuation": {
            pattern: /^\$\{|\}$/,
            alias: "punctuation"
          },
          rest: Prism.languages.javascript
        }
      },
      "string": /[\s\S]+/
    }
  },
  "string-property": {
    pattern: /((?:^|[,{])[ \t]*)(["'])(?:\\(?:\r\n|[\s\S])|(?!\2)[^\\\r\n])*\2(?=\s*:)/m,
    lookbehind: true,
    greedy: true,
    alias: "property"
  }
});
Prism.languages.insertBefore("javascript", "operator", {
  "literal-property": {
    pattern: /((?:^|[,{])[ \t]*)(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*:)/m,
    lookbehind: true,
    alias: "property"
  }
});
if (Prism.languages.markup) {
  Prism.languages.markup.tag.addInlined("script", "javascript");
  Prism.languages.markup.tag.addAttribute(
    /on(?:abort|blur|change|click|composition(?:end|start|update)|dblclick|error|focus(?:in|out)?|key(?:down|up)|load|mouse(?:down|enter|leave|move|out|over|up)|reset|resize|scroll|select|slotchange|submit|unload|wheel)/.source,
    "javascript"
  );
}
Prism.languages.js = Prism.languages.javascript;

// node_modules/prismjs/components/prism-markup.js
Prism.languages.markup = {
  "comment": {
    pattern: /<!--(?:(?!<!--)[\s\S])*?-->/,
    greedy: true
  },
  "prolog": {
    pattern: /<\?[\s\S]+?\?>/,
    greedy: true
  },
  "doctype": {
    // https://www.w3.org/TR/xml/#NT-doctypedecl
    pattern: /<!DOCTYPE(?:[^>"'[\]]|"[^"]*"|'[^']*')+(?:\[(?:[^<"'\]]|"[^"]*"|'[^']*'|<(?!!--)|<!--(?:[^-]|-(?!->))*-->)*\]\s*)?>/i,
    greedy: true,
    inside: {
      "internal-subset": {
        pattern: /(^[^\[]*\[)[\s\S]+(?=\]>$)/,
        lookbehind: true,
        greedy: true,
        inside: null
        // see below
      },
      "string": {
        pattern: /"[^"]*"|'[^']*'/,
        greedy: true
      },
      "punctuation": /^<!|>$|[[\]]/,
      "doctype-tag": /^DOCTYPE/i,
      "name": /[^\s<>'"]+/
    }
  },
  "cdata": {
    pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
    greedy: true
  },
  "tag": {
    pattern: /<\/?(?!\d)[^\s>\/=$<%]+(?:\s(?:\s*[^\s>\/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))|(?=[\s/>])))+)?\s*\/?>/,
    greedy: true,
    inside: {
      "tag": {
        pattern: /^<\/?[^\s>\/]+/,
        inside: {
          "punctuation": /^<\/?/,
          "namespace": /^[^\s>\/:]+:/
        }
      },
      "special-attr": [],
      "attr-value": {
        pattern: /=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+)/,
        inside: {
          "punctuation": [
            {
              pattern: /^=/,
              alias: "attr-equals"
            },
            {
              pattern: /^(\s*)["']|["']$/,
              lookbehind: true
            }
          ]
        }
      },
      "punctuation": /\/?>/,
      "attr-name": {
        pattern: /[^\s>\/]+/,
        inside: {
          "namespace": /^[^\s>\/:]+:/
        }
      }
    }
  },
  "entity": [
    {
      pattern: /&[\da-z]{1,8};/i,
      alias: "named-entity"
    },
    /&#x?[\da-f]{1,8};/i
  ]
};
Prism.languages.markup["tag"].inside["attr-value"].inside["entity"] = Prism.languages.markup["entity"];
Prism.languages.markup["doctype"].inside["internal-subset"].inside = Prism.languages.markup;
Prism.hooks.add("wrap", function(env) {
  if (env.type === "entity") {
    env.attributes["title"] = env.content.replace(/&amp;/, "&");
  }
});
Object.defineProperty(Prism.languages.markup.tag, "addInlined", {
  /**
   * Adds an inlined language to markup.
   *
   * An example of an inlined language is CSS with `<style>` tags.
   *
   * @param {string} tagName The name of the tag that contains the inlined language. This name will be treated as
   * case insensitive.
   * @param {string} lang The language key.
   * @example
   * addInlined('style', 'css');
   */
  value: function addInlined(tagName, lang) {
    var includedCdataInside = {};
    includedCdataInside["language-" + lang] = {
      pattern: /(^<!\[CDATA\[)[\s\S]+?(?=\]\]>$)/i,
      lookbehind: true,
      inside: Prism.languages[lang]
    };
    includedCdataInside["cdata"] = /^<!\[CDATA\[|\]\]>$/i;
    var inside = {
      "included-cdata": {
        pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
        inside: includedCdataInside
      }
    };
    inside["language-" + lang] = {
      pattern: /[\s\S]+/,
      inside: Prism.languages[lang]
    };
    var def = {};
    def[tagName] = {
      pattern: RegExp(/(<__[^>]*>)(?:<!\[CDATA\[(?:[^\]]|\](?!\]>))*\]\]>|(?!<!\[CDATA\[)[\s\S])*?(?=<\/__>)/.source.replace(/__/g, function() {
        return tagName;
      }), "i"),
      lookbehind: true,
      greedy: true,
      inside
    };
    Prism.languages.insertBefore("markup", "cdata", def);
  }
});
Object.defineProperty(Prism.languages.markup.tag, "addAttribute", {
  /**
   * Adds an pattern to highlight languages embedded in HTML attributes.
   *
   * An example of an inlined language is CSS with `style` attributes.
   *
   * @param {string} attrName The name of the tag that contains the inlined language. This name will be treated as
   * case insensitive.
   * @param {string} lang The language key.
   * @example
   * addAttribute('style', 'css');
   */
  value: function(attrName, lang) {
    Prism.languages.markup.tag.inside["special-attr"].push({
      pattern: RegExp(
        /(^|["'\s])/.source + "(?:" + attrName + ")" + /\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))/.source,
        "i"
      ),
      lookbehind: true,
      inside: {
        "attr-name": /^[^\s=]+/,
        "attr-value": {
          pattern: /=[\s\S]+/,
          inside: {
            "value": {
              pattern: /(^=\s*(["']|(?!["'])))\S[\s\S]*(?=\2$)/,
              lookbehind: true,
              alias: [lang, "language-" + lang],
              inside: Prism.languages[lang]
            },
            "punctuation": [
              {
                pattern: /^=/,
                alias: "attr-equals"
              },
              /"|'/
            ]
          }
        }
      }
    });
  }
});
Prism.languages.html = Prism.languages.markup;
Prism.languages.mathml = Prism.languages.markup;
Prism.languages.svg = Prism.languages.markup;
Prism.languages.xml = Prism.languages.extend("markup", {});
Prism.languages.ssml = Prism.languages.xml;
Prism.languages.atom = Prism.languages.xml;
Prism.languages.rss = Prism.languages.xml;

// node_modules/prismjs/components/prism-markdown.js
(function(Prism2) {
  var inner = /(?:\\.|[^\\\n\r]|(?:\n|\r\n?)(?![\r\n]))/.source;
  function createInline(pattern) {
    pattern = pattern.replace(/<inner>/g, function() {
      return inner;
    });
    return RegExp(/((?:^|[^\\])(?:\\{2})*)/.source + "(?:" + pattern + ")");
  }
  var tableCell = /(?:\\.|``(?:[^`\r\n]|`(?!`))+``|`[^`\r\n]+`|[^\\|\r\n`])+/.source;
  var tableRow = /\|?__(?:\|__)+\|?(?:(?:\n|\r\n?)|(?![\s\S]))/.source.replace(/__/g, function() {
    return tableCell;
  });
  var tableLine = /\|?[ \t]*:?-{3,}:?[ \t]*(?:\|[ \t]*:?-{3,}:?[ \t]*)+\|?(?:\n|\r\n?)/.source;
  Prism2.languages.markdown = Prism2.languages.extend("markup", {});
  Prism2.languages.insertBefore("markdown", "prolog", {
    "front-matter-block": {
      pattern: /(^(?:\s*[\r\n])?)---(?!.)[\s\S]*?[\r\n]---(?!.)/,
      lookbehind: true,
      greedy: true,
      inside: {
        "punctuation": /^---|---$/,
        "front-matter": {
          pattern: /\S+(?:\s+\S+)*/,
          alias: ["yaml", "language-yaml"],
          inside: Prism2.languages.yaml
        }
      }
    },
    "blockquote": {
      // > ...
      pattern: /^>(?:[\t ]*>)*/m,
      alias: "punctuation"
    },
    "table": {
      pattern: RegExp("^" + tableRow + tableLine + "(?:" + tableRow + ")*", "m"),
      inside: {
        "table-data-rows": {
          pattern: RegExp("^(" + tableRow + tableLine + ")(?:" + tableRow + ")*$"),
          lookbehind: true,
          inside: {
            "table-data": {
              pattern: RegExp(tableCell),
              inside: Prism2.languages.markdown
            },
            "punctuation": /\|/
          }
        },
        "table-line": {
          pattern: RegExp("^(" + tableRow + ")" + tableLine + "$"),
          lookbehind: true,
          inside: {
            "punctuation": /\||:?-{3,}:?/
          }
        },
        "table-header-row": {
          pattern: RegExp("^" + tableRow + "$"),
          inside: {
            "table-header": {
              pattern: RegExp(tableCell),
              alias: "important",
              inside: Prism2.languages.markdown
            },
            "punctuation": /\|/
          }
        }
      }
    },
    "code": [
      {
        // Prefixed by 4 spaces or 1 tab and preceded by an empty line
        pattern: /((?:^|\n)[ \t]*\n|(?:^|\r\n?)[ \t]*\r\n?)(?: {4}|\t).+(?:(?:\n|\r\n?)(?: {4}|\t).+)*/,
        lookbehind: true,
        alias: "keyword"
      },
      {
        // ```optional language
        // code block
        // ```
        pattern: /^```[\s\S]*?^```$/m,
        greedy: true,
        inside: {
          "code-block": {
            pattern: /^(```.*(?:\n|\r\n?))[\s\S]+?(?=(?:\n|\r\n?)^```$)/m,
            lookbehind: true
          },
          "code-language": {
            pattern: /^(```).+/,
            lookbehind: true
          },
          "punctuation": /```/
        }
      }
    ],
    "title": [
      {
        // title 1
        // =======
        // title 2
        // -------
        pattern: /\S.*(?:\n|\r\n?)(?:==+|--+)(?=[ \t]*$)/m,
        alias: "important",
        inside: {
          punctuation: /==+$|--+$/
        }
      },
      {
        // # title 1
        // ###### title 6
        pattern: /(^\s*)#.+/m,
        lookbehind: true,
        alias: "important",
        inside: {
          punctuation: /^#+|#+$/
        }
      }
    ],
    "hr": {
      // ***
      // ---
      // * * *
      // -----------
      pattern: /(^\s*)([*-])(?:[\t ]*\2){2,}(?=\s*$)/m,
      lookbehind: true,
      alias: "punctuation"
    },
    "list": {
      // * item
      // + item
      // - item
      // 1. item
      pattern: /(^\s*)(?:[*+-]|\d+\.)(?=[\t ].)/m,
      lookbehind: true,
      alias: "punctuation"
    },
    "url-reference": {
      // [id]: http://example.com "Optional title"
      // [id]: http://example.com 'Optional title'
      // [id]: http://example.com (Optional title)
      // [id]: <http://example.com> "Optional title"
      pattern: /!?\[[^\]]+\]:[\t ]+(?:\S+|<(?:\\.|[^>\\])+>)(?:[\t ]+(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\((?:\\.|[^)\\])*\)))?/,
      inside: {
        "variable": {
          pattern: /^(!?\[)[^\]]+/,
          lookbehind: true
        },
        "string": /(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\((?:\\.|[^)\\])*\))$/,
        "punctuation": /^[\[\]!:]|[<>]/
      },
      alias: "url"
    },
    "bold": {
      // **strong**
      // __strong__
      // allow one nested instance of italic text using the same delimiter
      pattern: createInline(/\b__(?:(?!_)<inner>|_(?:(?!_)<inner>)+_)+__\b|\*\*(?:(?!\*)<inner>|\*(?:(?!\*)<inner>)+\*)+\*\*/.source),
      lookbehind: true,
      greedy: true,
      inside: {
        "content": {
          pattern: /(^..)[\s\S]+(?=..$)/,
          lookbehind: true,
          inside: {}
          // see below
        },
        "punctuation": /\*\*|__/
      }
    },
    "italic": {
      // *em*
      // _em_
      // allow one nested instance of bold text using the same delimiter
      pattern: createInline(/\b_(?:(?!_)<inner>|__(?:(?!_)<inner>)+__)+_\b|\*(?:(?!\*)<inner>|\*\*(?:(?!\*)<inner>)+\*\*)+\*/.source),
      lookbehind: true,
      greedy: true,
      inside: {
        "content": {
          pattern: /(^.)[\s\S]+(?=.$)/,
          lookbehind: true,
          inside: {}
          // see below
        },
        "punctuation": /[*_]/
      }
    },
    "strike": {
      // ~~strike through~~
      // ~strike~
      // eslint-disable-next-line regexp/strict
      pattern: createInline(/(~~?)(?:(?!~)<inner>)+\2/.source),
      lookbehind: true,
      greedy: true,
      inside: {
        "content": {
          pattern: /(^~~?)[\s\S]+(?=\1$)/,
          lookbehind: true,
          inside: {}
          // see below
        },
        "punctuation": /~~?/
      }
    },
    "code-snippet": {
      // `code`
      // ``code``
      pattern: /(^|[^\\`])(?:``[^`\r\n]+(?:`[^`\r\n]+)*``(?!`)|`[^`\r\n]+`(?!`))/,
      lookbehind: true,
      greedy: true,
      alias: ["code", "keyword"]
    },
    "url": {
      // [example](http://example.com "Optional title")
      // [example][id]
      // [example] [id]
      pattern: createInline(/!?\[(?:(?!\])<inner>)+\](?:\([^\s)]+(?:[\t ]+"(?:\\.|[^"\\])*")?\)|[ \t]?\[(?:(?!\])<inner>)+\])/.source),
      lookbehind: true,
      greedy: true,
      inside: {
        "operator": /^!/,
        "content": {
          pattern: /(^\[)[^\]]+(?=\])/,
          lookbehind: true,
          inside: {}
          // see below
        },
        "variable": {
          pattern: /(^\][ \t]?\[)[^\]]+(?=\]$)/,
          lookbehind: true
        },
        "url": {
          pattern: /(^\]\()[^\s)]+/,
          lookbehind: true
        },
        "string": {
          pattern: /(^[ \t]+)"(?:\\.|[^"\\])*"(?=\)$)/,
          lookbehind: true
        }
      }
    }
  });
  ["url", "bold", "italic", "strike"].forEach(function(token) {
    ["url", "bold", "italic", "strike", "code-snippet"].forEach(function(inside) {
      if (token !== inside) {
        Prism2.languages.markdown[token].inside.content.inside[inside] = Prism2.languages.markdown[inside];
      }
    });
  });
  Prism2.hooks.add("after-tokenize", function(env) {
    if (env.language !== "markdown" && env.language !== "md") {
      return;
    }
    function walkTokens(tokens) {
      if (!tokens || typeof tokens === "string") {
        return;
      }
      for (var i2 = 0, l2 = tokens.length; i2 < l2; i2++) {
        var token = tokens[i2];
        if (token.type !== "code") {
          walkTokens(token.content);
          continue;
        }
        var codeLang = token.content[1];
        var codeBlock = token.content[3];
        if (codeLang && codeBlock && codeLang.type === "code-language" && codeBlock.type === "code-block" && typeof codeLang.content === "string") {
          var lang = codeLang.content.replace(/\b#/g, "sharp").replace(/\b\+\+/g, "pp");
          lang = (/[a-z][\w-]*/i.exec(lang) || [""])[0].toLowerCase();
          var alias = "language-" + lang;
          if (!codeBlock.alias) {
            codeBlock.alias = [alias];
          } else if (typeof codeBlock.alias === "string") {
            codeBlock.alias = [codeBlock.alias, alias];
          } else {
            codeBlock.alias.push(alias);
          }
        }
      }
    }
    walkTokens(env.tokens);
  });
  Prism2.hooks.add("wrap", function(env) {
    if (env.type !== "code-block") {
      return;
    }
    var codeLang = "";
    for (var i2 = 0, l2 = env.classes.length; i2 < l2; i2++) {
      var cls = env.classes[i2];
      var match = /language-(.+)/.exec(cls);
      if (match) {
        codeLang = match[1];
        break;
      }
    }
    var grammar = Prism2.languages[codeLang];
    if (!grammar) {
      if (codeLang && codeLang !== "none" && Prism2.plugins.autoloader) {
        var id = "md-" + (/* @__PURE__ */ new Date()).valueOf() + "-" + Math.floor(Math.random() * 1e16);
        env.attributes["id"] = id;
        Prism2.plugins.autoloader.loadLanguages(codeLang, function() {
          var ele = document.getElementById(id);
          if (ele) {
            ele.innerHTML = Prism2.highlight(ele.textContent, Prism2.languages[codeLang], codeLang);
          }
        });
      }
    } else {
      env.content = Prism2.highlight(textContent(env.content), grammar, codeLang);
    }
  });
  var tagPattern = RegExp(Prism2.languages.markup.tag.pattern.source, "gi");
  var KNOWN_ENTITY_NAMES = {
    "amp": "&",
    "lt": "<",
    "gt": ">",
    "quot": '"'
  };
  var fromCodePoint = String.fromCodePoint || String.fromCharCode;
  function textContent(html) {
    var text = html.replace(tagPattern, "");
    text = text.replace(/&(\w{1,8}|#x?[\da-f]{1,8});/gi, function(m2, code) {
      code = code.toLowerCase();
      if (code[0] === "#") {
        var value;
        if (code[1] === "x") {
          value = parseInt(code.slice(2), 16);
        } else {
          value = Number(code.slice(1));
        }
        return fromCodePoint(value);
      } else {
        var known = KNOWN_ENTITY_NAMES[code];
        if (known) {
          return known;
        }
        return m2;
      }
    });
    return text;
  }
  Prism2.languages.md = Prism2.languages.markdown;
})(Prism);

// node_modules/prismjs/components/prism-c.js
Prism.languages.c = Prism.languages.extend("clike", {
  "comment": {
    pattern: /\/\/(?:[^\r\n\\]|\\(?:\r\n?|\n|(?![\r\n])))*|\/\*[\s\S]*?(?:\*\/|$)/,
    greedy: true
  },
  "string": {
    // https://en.cppreference.com/w/c/language/string_literal
    pattern: /"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"/,
    greedy: true
  },
  "class-name": {
    pattern: /(\b(?:enum|struct)\s+(?:__attribute__\s*\(\([\s\S]*?\)\)\s*)?)\w+|\b[a-z]\w*_t\b/,
    lookbehind: true
  },
  "keyword": /\b(?:_Alignas|_Alignof|_Atomic|_Bool|_Complex|_Generic|_Imaginary|_Noreturn|_Static_assert|_Thread_local|__attribute__|asm|auto|break|case|char|const|continue|default|do|double|else|enum|extern|float|for|goto|if|inline|int|long|register|return|short|signed|sizeof|static|struct|switch|typedef|typeof|union|unsigned|void|volatile|while)\b/,
  "function": /\b[a-z_]\w*(?=\s*\()/i,
  "number": /(?:\b0x(?:[\da-f]+(?:\.[\da-f]*)?|\.[\da-f]+)(?:p[+-]?\d+)?|(?:\b\d+(?:\.\d*)?|\B\.\d+)(?:e[+-]?\d+)?)[ful]{0,4}/i,
  "operator": />>=?|<<=?|->|([-+&|:])\1|[?:~]|[-+*/%&|^!=<>]=?/
});
Prism.languages.insertBefore("c", "string", {
  "char": {
    // https://en.cppreference.com/w/c/language/character_constant
    pattern: /'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n]){0,32}'/,
    greedy: true
  }
});
Prism.languages.insertBefore("c", "string", {
  "macro": {
    // allow for multiline macro definitions
    // spaces after the # character compile fine with gcc
    pattern: /(^[\t ]*)#\s*[a-z](?:[^\r\n\\/]|\/(?!\*)|\/\*(?:[^*]|\*(?!\/))*\*\/|\\(?:\r\n|[\s\S]))*/im,
    lookbehind: true,
    greedy: true,
    alias: "property",
    inside: {
      "string": [
        {
          // highlight the path of the include statement as a string
          pattern: /^(#\s*include\s*)<[^>]+>/,
          lookbehind: true
        },
        Prism.languages.c["string"]
      ],
      "char": Prism.languages.c["char"],
      "comment": Prism.languages.c["comment"],
      "macro-name": [
        {
          pattern: /(^#\s*define\s+)\w+\b(?!\()/i,
          lookbehind: true
        },
        {
          pattern: /(^#\s*define\s+)\w+\b(?=\()/i,
          lookbehind: true,
          alias: "function"
        }
      ],
      // highlight macro directives as keywords
      "directive": {
        pattern: /^(#\s*)[a-z]+/,
        lookbehind: true,
        alias: "keyword"
      },
      "directive-hash": /^#/,
      "punctuation": /##|\\(?=[\r\n])/,
      "expression": {
        pattern: /\S[\s\S]*/,
        inside: Prism.languages.c
      }
    }
  }
});
Prism.languages.insertBefore("c", "function", {
  // highlight predefined macros as constants
  "constant": /\b(?:EOF|NULL|SEEK_CUR|SEEK_END|SEEK_SET|__DATE__|__FILE__|__LINE__|__TIMESTAMP__|__TIME__|__func__|stderr|stdin|stdout)\b/
});
delete Prism.languages.c["boolean"];

// node_modules/prismjs/components/prism-css.js
(function(Prism2) {
  var string = /(?:"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')/;
  Prism2.languages.css = {
    "comment": /\/\*[\s\S]*?\*\//,
    "atrule": {
      pattern: RegExp("@[\\w-](?:" + /[^;{\s"']|\s+(?!\s)/.source + "|" + string.source + ")*?" + /(?:;|(?=\s*\{))/.source),
      inside: {
        "rule": /^@[\w-]+/,
        "selector-function-argument": {
          pattern: /(\bselector\s*\(\s*(?![\s)]))(?:[^()\s]|\s+(?![\s)])|\((?:[^()]|\([^()]*\))*\))+(?=\s*\))/,
          lookbehind: true,
          alias: "selector"
        },
        "keyword": {
          pattern: /(^|[^\w-])(?:and|not|only|or)(?![\w-])/,
          lookbehind: true
        }
        // See rest below
      }
    },
    "url": {
      // https://drafts.csswg.org/css-values-3/#urls
      pattern: RegExp("\\burl\\((?:" + string.source + "|" + /(?:[^\\\r\n()"']|\\[\s\S])*/.source + ")\\)", "i"),
      greedy: true,
      inside: {
        "function": /^url/i,
        "punctuation": /^\(|\)$/,
        "string": {
          pattern: RegExp("^" + string.source + "$"),
          alias: "url"
        }
      }
    },
    "selector": {
      pattern: RegExp(`(^|[{}\\s])[^{}\\s](?:[^{};"'\\s]|\\s+(?![\\s{])|` + string.source + ")*(?=\\s*\\{)"),
      lookbehind: true
    },
    "string": {
      pattern: string,
      greedy: true
    },
    "property": {
      pattern: /(^|[^-\w\xA0-\uFFFF])(?!\s)[-_a-z\xA0-\uFFFF](?:(?!\s)[-\w\xA0-\uFFFF])*(?=\s*:)/i,
      lookbehind: true
    },
    "important": /!important\b/i,
    "function": {
      pattern: /(^|[^-a-z0-9])[-a-z0-9]+(?=\()/i,
      lookbehind: true
    },
    "punctuation": /[(){};:,]/
  };
  Prism2.languages.css["atrule"].inside.rest = Prism2.languages.css;
  var markup = Prism2.languages.markup;
  if (markup) {
    markup.tag.addInlined("style", "css");
    markup.tag.addAttribute("style", "css");
  }
})(Prism);

// node_modules/prismjs/components/prism-objectivec.js
Prism.languages.objectivec = Prism.languages.extend("c", {
  "string": {
    pattern: /@?"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"/,
    greedy: true
  },
  "keyword": /\b(?:asm|auto|break|case|char|const|continue|default|do|double|else|enum|extern|float|for|goto|if|in|inline|int|long|register|return|self|short|signed|sizeof|static|struct|super|switch|typedef|typeof|union|unsigned|void|volatile|while)\b|(?:@interface|@end|@implementation|@protocol|@class|@public|@protected|@private|@property|@try|@catch|@finally|@throw|@synthesize|@dynamic|@selector)\b/,
  "operator": /-[->]?|\+\+?|!=?|<<?=?|>>?=?|==?|&&?|\|\|?|[~^%?*\/@]/
});
delete Prism.languages.objectivec["class-name"];
Prism.languages.objc = Prism.languages.objectivec;

// node_modules/prismjs/components/prism-sql.js
Prism.languages.sql = {
  "comment": {
    pattern: /(^|[^\\])(?:\/\*[\s\S]*?\*\/|(?:--|\/\/|#).*)/,
    lookbehind: true
  },
  "variable": [
    {
      pattern: /@(["'`])(?:\\[\s\S]|(?!\1)[^\\])+\1/,
      greedy: true
    },
    /@[\w.$]+/
  ],
  "string": {
    pattern: /(^|[^@\\])("|')(?:\\[\s\S]|(?!\2)[^\\]|\2\2)*\2/,
    greedy: true,
    lookbehind: true
  },
  "identifier": {
    pattern: /(^|[^@\\])`(?:\\[\s\S]|[^`\\]|``)*`/,
    greedy: true,
    lookbehind: true,
    inside: {
      "punctuation": /^`|`$/
    }
  },
  "function": /\b(?:AVG|COUNT|FIRST|FORMAT|LAST|LCASE|LEN|MAX|MID|MIN|MOD|NOW|ROUND|SUM|UCASE)(?=\s*\()/i,
  // Should we highlight user defined functions too?
  "keyword": /\b(?:ACTION|ADD|AFTER|ALGORITHM|ALL|ALTER|ANALYZE|ANY|APPLY|AS|ASC|AUTHORIZATION|AUTO_INCREMENT|BACKUP|BDB|BEGIN|BERKELEYDB|BIGINT|BINARY|BIT|BLOB|BOOL|BOOLEAN|BREAK|BROWSE|BTREE|BULK|BY|CALL|CASCADED?|CASE|CHAIN|CHAR(?:ACTER|SET)?|CHECK(?:POINT)?|CLOSE|CLUSTERED|COALESCE|COLLATE|COLUMNS?|COMMENT|COMMIT(?:TED)?|COMPUTE|CONNECT|CONSISTENT|CONSTRAINT|CONTAINS(?:TABLE)?|CONTINUE|CONVERT|CREATE|CROSS|CURRENT(?:_DATE|_TIME|_TIMESTAMP|_USER)?|CURSOR|CYCLE|DATA(?:BASES?)?|DATE(?:TIME)?|DAY|DBCC|DEALLOCATE|DEC|DECIMAL|DECLARE|DEFAULT|DEFINER|DELAYED|DELETE|DELIMITERS?|DENY|DESC|DESCRIBE|DETERMINISTIC|DISABLE|DISCARD|DISK|DISTINCT|DISTINCTROW|DISTRIBUTED|DO|DOUBLE|DROP|DUMMY|DUMP(?:FILE)?|DUPLICATE|ELSE(?:IF)?|ENABLE|ENCLOSED|END|ENGINE|ENUM|ERRLVL|ERRORS|ESCAPED?|EXCEPT|EXEC(?:UTE)?|EXISTS|EXIT|EXPLAIN|EXTENDED|FETCH|FIELDS|FILE|FILLFACTOR|FIRST|FIXED|FLOAT|FOLLOWING|FOR(?: EACH ROW)?|FORCE|FOREIGN|FREETEXT(?:TABLE)?|FROM|FULL|FUNCTION|GEOMETRY(?:COLLECTION)?|GLOBAL|GOTO|GRANT|GROUP|HANDLER|HASH|HAVING|HOLDLOCK|HOUR|IDENTITY(?:COL|_INSERT)?|IF|IGNORE|IMPORT|INDEX|INFILE|INNER|INNODB|INOUT|INSERT|INT|INTEGER|INTERSECT|INTERVAL|INTO|INVOKER|ISOLATION|ITERATE|JOIN|KEYS?|KILL|LANGUAGE|LAST|LEAVE|LEFT|LEVEL|LIMIT|LINENO|LINES|LINESTRING|LOAD|LOCAL|LOCK|LONG(?:BLOB|TEXT)|LOOP|MATCH(?:ED)?|MEDIUM(?:BLOB|INT|TEXT)|MERGE|MIDDLEINT|MINUTE|MODE|MODIFIES|MODIFY|MONTH|MULTI(?:LINESTRING|POINT|POLYGON)|NATIONAL|NATURAL|NCHAR|NEXT|NO|NONCLUSTERED|NULLIF|NUMERIC|OFF?|OFFSETS?|ON|OPEN(?:DATASOURCE|QUERY|ROWSET)?|OPTIMIZE|OPTION(?:ALLY)?|ORDER|OUT(?:ER|FILE)?|OVER|PARTIAL|PARTITION|PERCENT|PIVOT|PLAN|POINT|POLYGON|PRECEDING|PRECISION|PREPARE|PREV|PRIMARY|PRINT|PRIVILEGES|PROC(?:EDURE)?|PUBLIC|PURGE|QUICK|RAISERROR|READS?|REAL|RECONFIGURE|REFERENCES|RELEASE|RENAME|REPEAT(?:ABLE)?|REPLACE|REPLICATION|REQUIRE|RESIGNAL|RESTORE|RESTRICT|RETURN(?:ING|S)?|REVOKE|RIGHT|ROLLBACK|ROUTINE|ROW(?:COUNT|GUIDCOL|S)?|RTREE|RULE|SAVE(?:POINT)?|SCHEMA|SECOND|SELECT|SERIAL(?:IZABLE)?|SESSION(?:_USER)?|SET(?:USER)?|SHARE|SHOW|SHUTDOWN|SIMPLE|SMALLINT|SNAPSHOT|SOME|SONAME|SQL|START(?:ING)?|STATISTICS|STATUS|STRIPED|SYSTEM_USER|TABLES?|TABLESPACE|TEMP(?:ORARY|TABLE)?|TERMINATED|TEXT(?:SIZE)?|THEN|TIME(?:STAMP)?|TINY(?:BLOB|INT|TEXT)|TOP?|TRAN(?:SACTIONS?)?|TRIGGER|TRUNCATE|TSEQUAL|TYPES?|UNBOUNDED|UNCOMMITTED|UNDEFINED|UNION|UNIQUE|UNLOCK|UNPIVOT|UNSIGNED|UPDATE(?:TEXT)?|USAGE|USE|USER|USING|VALUES?|VAR(?:BINARY|CHAR|CHARACTER|YING)|VIEW|WAITFOR|WARNINGS|WHEN|WHERE|WHILE|WITH(?: ROLLUP|IN)?|WORK|WRITE(?:TEXT)?|YEAR)\b/i,
  "boolean": /\b(?:FALSE|NULL|TRUE)\b/i,
  "number": /\b0x[\da-f]+\b|\b\d+(?:\.\d*)?|\B\.\d+\b/i,
  "operator": /[-+*\/=%^~]|&&?|\|\|?|!=?|<(?:=>?|<|>)?|>[>=]?|\b(?:AND|BETWEEN|DIV|ILIKE|IN|IS|LIKE|NOT|OR|REGEXP|RLIKE|SOUNDS LIKE|XOR)\b/i,
  "punctuation": /[;[\]()`,.]/
};

// node_modules/prismjs/components/prism-powershell.js
(function(Prism2) {
  var powershell = Prism2.languages.powershell = {
    "comment": [
      {
        pattern: /(^|[^`])<#[\s\S]*?#>/,
        lookbehind: true
      },
      {
        pattern: /(^|[^`])#.*/,
        lookbehind: true
      }
    ],
    "string": [
      {
        pattern: /"(?:`[\s\S]|[^`"])*"/,
        greedy: true,
        inside: null
        // see below
      },
      {
        pattern: /'(?:[^']|'')*'/,
        greedy: true
      }
    ],
    // Matches name spaces as well as casts, attribute decorators. Force starting with letter to avoid matching array indices
    // Supports two levels of nested brackets (e.g. `[OutputType([System.Collections.Generic.List[int]])]`)
    "namespace": /\[[a-z](?:\[(?:\[[^\]]*\]|[^\[\]])*\]|[^\[\]])*\]/i,
    "boolean": /\$(?:false|true)\b/i,
    "variable": /\$\w+\b/,
    // Cmdlets and aliases. Aliases should come last, otherwise "write" gets preferred over "write-host" for example
    // Get-Command | ?{ $_.ModuleName -match "Microsoft.PowerShell.(Util|Core|Management)" }
    // Get-Alias | ?{ $_.ReferencedCommand.Module.Name -match "Microsoft.PowerShell.(Util|Core|Management)" }
    "function": [
      /\b(?:Add|Approve|Assert|Backup|Block|Checkpoint|Clear|Close|Compare|Complete|Compress|Confirm|Connect|Convert|ConvertFrom|ConvertTo|Copy|Debug|Deny|Disable|Disconnect|Dismount|Edit|Enable|Enter|Exit|Expand|Export|Find|ForEach|Format|Get|Grant|Group|Hide|Import|Initialize|Install|Invoke|Join|Limit|Lock|Measure|Merge|Move|New|Open|Optimize|Out|Ping|Pop|Protect|Publish|Push|Read|Receive|Redo|Register|Remove|Rename|Repair|Request|Reset|Resize|Resolve|Restart|Restore|Resume|Revoke|Save|Search|Select|Send|Set|Show|Skip|Sort|Split|Start|Step|Stop|Submit|Suspend|Switch|Sync|Tee|Test|Trace|Unblock|Undo|Uninstall|Unlock|Unprotect|Unpublish|Unregister|Update|Use|Wait|Watch|Where|Write)-[a-z]+\b/i,
      /\b(?:ac|cat|chdir|clc|cli|clp|clv|compare|copy|cp|cpi|cpp|cvpa|dbp|del|diff|dir|ebp|echo|epal|epcsv|epsn|erase|fc|fl|ft|fw|gal|gbp|gc|gci|gcs|gdr|gi|gl|gm|gp|gps|group|gsv|gu|gv|gwmi|iex|ii|ipal|ipcsv|ipsn|irm|iwmi|iwr|kill|lp|ls|measure|mi|mount|move|mp|mv|nal|ndr|ni|nv|ogv|popd|ps|pushd|pwd|rbp|rd|rdr|ren|ri|rm|rmdir|rni|rnp|rp|rv|rvpa|rwmi|sal|saps|sasv|sbp|sc|select|set|shcm|si|sl|sleep|sls|sort|sp|spps|spsv|start|sv|swmi|tee|trcm|type|write)\b/i
    ],
    // per http://technet.microsoft.com/en-us/library/hh847744.aspx
    "keyword": /\b(?:Begin|Break|Catch|Class|Continue|Data|Define|Do|DynamicParam|Else|ElseIf|End|Exit|Filter|Finally|For|ForEach|From|Function|If|InlineScript|Parallel|Param|Process|Return|Sequence|Switch|Throw|Trap|Try|Until|Using|Var|While|Workflow)\b/i,
    "operator": {
      pattern: /(^|\W)(?:!|-(?:b?(?:and|x?or)|as|(?:Not)?(?:Contains|In|Like|Match)|eq|ge|gt|is(?:Not)?|Join|le|lt|ne|not|Replace|sh[lr])\b|-[-=]?|\+[+=]?|[*\/%]=?)/i,
      lookbehind: true
    },
    "punctuation": /[|{}[\];(),.]/
  };
  powershell.string[0].inside = {
    "function": {
      // Allow for one level of nesting
      pattern: /(^|[^`])\$\((?:\$\([^\r\n()]*\)|(?!\$\()[^\r\n)])*\)/,
      lookbehind: true,
      inside: powershell
    },
    "boolean": powershell.boolean,
    "variable": powershell.variable
  };
})(Prism);

// node_modules/prismjs/components/prism-python.js
Prism.languages.python = {
  "comment": {
    pattern: /(^|[^\\])#.*/,
    lookbehind: true,
    greedy: true
  },
  "string-interpolation": {
    pattern: /(?:f|fr|rf)(?:("""|''')[\s\S]*?\1|("|')(?:\\.|(?!\2)[^\\\r\n])*\2)/i,
    greedy: true,
    inside: {
      "interpolation": {
        // "{" <expression> <optional "!s", "!r", or "!a"> <optional ":" format specifier> "}"
        pattern: /((?:^|[^{])(?:\{\{)*)\{(?!\{)(?:[^{}]|\{(?!\{)(?:[^{}]|\{(?!\{)(?:[^{}])+\})+\})+\}/,
        lookbehind: true,
        inside: {
          "format-spec": {
            pattern: /(:)[^:(){}]+(?=\}$)/,
            lookbehind: true
          },
          "conversion-option": {
            pattern: /![sra](?=[:}]$)/,
            alias: "punctuation"
          },
          rest: null
        }
      },
      "string": /[\s\S]+/
    }
  },
  "triple-quoted-string": {
    pattern: /(?:[rub]|br|rb)?("""|''')[\s\S]*?\1/i,
    greedy: true,
    alias: "string"
  },
  "string": {
    pattern: /(?:[rub]|br|rb)?("|')(?:\\.|(?!\1)[^\\\r\n])*\1/i,
    greedy: true
  },
  "function": {
    pattern: /((?:^|\s)def[ \t]+)[a-zA-Z_]\w*(?=\s*\()/g,
    lookbehind: true
  },
  "class-name": {
    pattern: /(\bclass\s+)\w+/i,
    lookbehind: true
  },
  "decorator": {
    pattern: /(^[\t ]*)@\w+(?:\.\w+)*/m,
    lookbehind: true,
    alias: ["annotation", "punctuation"],
    inside: {
      "punctuation": /\./
    }
  },
  "keyword": /\b(?:_(?=\s*:)|and|as|assert|async|await|break|case|class|continue|def|del|elif|else|except|exec|finally|for|from|global|if|import|in|is|lambda|match|nonlocal|not|or|pass|print|raise|return|try|while|with|yield)\b/,
  "builtin": /\b(?:__import__|abs|all|any|apply|ascii|basestring|bin|bool|buffer|bytearray|bytes|callable|chr|classmethod|cmp|coerce|compile|complex|delattr|dict|dir|divmod|enumerate|eval|execfile|file|filter|float|format|frozenset|getattr|globals|hasattr|hash|help|hex|id|input|int|intern|isinstance|issubclass|iter|len|list|locals|long|map|max|memoryview|min|next|object|oct|open|ord|pow|property|range|raw_input|reduce|reload|repr|reversed|round|set|setattr|slice|sorted|staticmethod|str|sum|super|tuple|type|unichr|unicode|vars|xrange|zip)\b/,
  "boolean": /\b(?:False|None|True)\b/,
  "number": /\b0(?:b(?:_?[01])+|o(?:_?[0-7])+|x(?:_?[a-f0-9])+)\b|(?:\b\d+(?:_\d+)*(?:\.(?:\d+(?:_\d+)*)?)?|\B\.\d+(?:_\d+)*)(?:e[+-]?\d+(?:_\d+)*)?j?(?!\w)/i,
  "operator": /[-+%=]=?|!=|:=|\*\*?=?|\/\/?=?|<[<=>]?|>[=>]?|[&|^~]/,
  "punctuation": /[{}[\];(),.:]/
};
Prism.languages.python["string-interpolation"].inside["interpolation"].inside.rest = Prism.languages.python;
Prism.languages.py = Prism.languages.python;

// node_modules/prismjs/components/prism-rust.js
(function(Prism2) {
  var multilineComment = /\/\*(?:[^*/]|\*(?!\/)|\/(?!\*)|<self>)*\*\//.source;
  for (var i2 = 0; i2 < 2; i2++) {
    multilineComment = multilineComment.replace(/<self>/g, function() {
      return multilineComment;
    });
  }
  multilineComment = multilineComment.replace(/<self>/g, function() {
    return /[^\s\S]/.source;
  });
  Prism2.languages.rust = {
    "comment": [
      {
        pattern: RegExp(/(^|[^\\])/.source + multilineComment),
        lookbehind: true,
        greedy: true
      },
      {
        pattern: /(^|[^\\:])\/\/.*/,
        lookbehind: true,
        greedy: true
      }
    ],
    "string": {
      pattern: /b?"(?:\\[\s\S]|[^\\"])*"|b?r(#*)"(?:[^"]|"(?!\1))*"\1/,
      greedy: true
    },
    "char": {
      pattern: /b?'(?:\\(?:x[0-7][\da-fA-F]|u\{(?:[\da-fA-F]_*){1,6}\}|.)|[^\\\r\n\t'])'/,
      greedy: true
    },
    "attribute": {
      pattern: /#!?\[(?:[^\[\]"]|"(?:\\[\s\S]|[^\\"])*")*\]/,
      greedy: true,
      alias: "attr-name",
      inside: {
        "string": null
        // see below
      }
    },
    // Closure params should not be confused with bitwise OR |
    "closure-params": {
      pattern: /([=(,:]\s*|\bmove\s*)\|[^|]*\||\|[^|]*\|(?=\s*(?:\{|->))/,
      lookbehind: true,
      greedy: true,
      inside: {
        "closure-punctuation": {
          pattern: /^\||\|$/,
          alias: "punctuation"
        },
        rest: null
        // see below
      }
    },
    "lifetime-annotation": {
      pattern: /'\w+/,
      alias: "symbol"
    },
    "fragment-specifier": {
      pattern: /(\$\w+:)[a-z]+/,
      lookbehind: true,
      alias: "punctuation"
    },
    "variable": /\$\w+/,
    "function-definition": {
      pattern: /(\bfn\s+)\w+/,
      lookbehind: true,
      alias: "function"
    },
    "type-definition": {
      pattern: /(\b(?:enum|struct|trait|type|union)\s+)\w+/,
      lookbehind: true,
      alias: "class-name"
    },
    "module-declaration": [
      {
        pattern: /(\b(?:crate|mod)\s+)[a-z][a-z_\d]*/,
        lookbehind: true,
        alias: "namespace"
      },
      {
        pattern: /(\b(?:crate|self|super)\s*)::\s*[a-z][a-z_\d]*\b(?:\s*::(?:\s*[a-z][a-z_\d]*\s*::)*)?/,
        lookbehind: true,
        alias: "namespace",
        inside: {
          "punctuation": /::/
        }
      }
    ],
    "keyword": [
      // https://github.com/rust-lang/reference/blob/master/src/keywords.md
      /\b(?:Self|abstract|as|async|await|become|box|break|const|continue|crate|do|dyn|else|enum|extern|final|fn|for|if|impl|in|let|loop|macro|match|mod|move|mut|override|priv|pub|ref|return|self|static|struct|super|trait|try|type|typeof|union|unsafe|unsized|use|virtual|where|while|yield)\b/,
      // primitives and str
      // https://doc.rust-lang.org/stable/rust-by-example/primitives.html
      /\b(?:bool|char|f(?:32|64)|[ui](?:8|16|32|64|128|size)|str)\b/
    ],
    // functions can technically start with an upper-case letter, but this will introduce a lot of false positives
    // and Rust's naming conventions recommend snake_case anyway.
    // https://doc.rust-lang.org/1.0.0/style/style/naming/README.html
    "function": /\b[a-z_]\w*(?=\s*(?:::\s*<|\())/,
    "macro": {
      pattern: /\b\w+!/,
      alias: "property"
    },
    "constant": /\b[A-Z_][A-Z_\d]+\b/,
    "class-name": /\b[A-Z]\w*\b/,
    "namespace": {
      pattern: /(?:\b[a-z][a-z_\d]*\s*::\s*)*\b[a-z][a-z_\d]*\s*::(?!\s*<)/,
      inside: {
        "punctuation": /::/
      }
    },
    // Hex, oct, bin, dec numbers with visual separators and type suffix
    "number": /\b(?:0x[\dA-Fa-f](?:_?[\dA-Fa-f])*|0o[0-7](?:_?[0-7])*|0b[01](?:_?[01])*|(?:(?:\d(?:_?\d)*)?\.)?\d(?:_?\d)*(?:[Ee][+-]?\d+)?)(?:_?(?:f32|f64|[iu](?:8|16|32|64|size)?))?\b/,
    "boolean": /\b(?:false|true)\b/,
    "punctuation": /->|\.\.=|\.{1,3}|::|[{}[\];(),:]/,
    "operator": /[-+*\/%!^]=?|=[=>]?|&[&=]?|\|[|=]?|<<?=?|>>?=?|[@?]/
  };
  Prism2.languages.rust["closure-params"].inside.rest = Prism2.languages.rust;
  Prism2.languages.rust["attribute"].inside["string"] = Prism2.languages.rust["string"];
})(Prism);

// node_modules/prismjs/components/prism-swift.js
Prism.languages.swift = {
  "comment": {
    // Nested comments are supported up to 2 levels
    pattern: /(^|[^\\:])(?:\/\/.*|\/\*(?:[^/*]|\/(?!\*)|\*(?!\/)|\/\*(?:[^*]|\*(?!\/))*\*\/)*\*\/)/,
    lookbehind: true,
    greedy: true
  },
  "string-literal": [
    // https://docs.swift.org/swift-book/LanguageGuide/StringsAndCharacters.html
    {
      pattern: RegExp(
        /(^|[^"#])/.source + "(?:" + /"(?:\\(?:\((?:[^()]|\([^()]*\))*\)|\r\n|[^(])|[^\\\r\n"])*"/.source + "|" + /"""(?:\\(?:\((?:[^()]|\([^()]*\))*\)|[^(])|[^\\"]|"(?!""))*"""/.source + ")" + /(?!["#])/.source
      ),
      lookbehind: true,
      greedy: true,
      inside: {
        "interpolation": {
          pattern: /(\\\()(?:[^()]|\([^()]*\))*(?=\))/,
          lookbehind: true,
          inside: null
          // see below
        },
        "interpolation-punctuation": {
          pattern: /^\)|\\\($/,
          alias: "punctuation"
        },
        "punctuation": /\\(?=[\r\n])/,
        "string": /[\s\S]+/
      }
    },
    {
      pattern: RegExp(
        /(^|[^"#])(#+)/.source + "(?:" + /"(?:\\(?:#+\((?:[^()]|\([^()]*\))*\)|\r\n|[^#])|[^\\\r\n])*?"/.source + "|" + /"""(?:\\(?:#+\((?:[^()]|\([^()]*\))*\)|[^#])|[^\\])*?"""/.source + ")\\2"
      ),
      lookbehind: true,
      greedy: true,
      inside: {
        "interpolation": {
          pattern: /(\\#+\()(?:[^()]|\([^()]*\))*(?=\))/,
          lookbehind: true,
          inside: null
          // see below
        },
        "interpolation-punctuation": {
          pattern: /^\)|\\#+\($/,
          alias: "punctuation"
        },
        "string": /[\s\S]+/
      }
    }
  ],
  "directive": {
    // directives with conditions
    pattern: RegExp(
      /#/.source + "(?:" + (/(?:elseif|if)\b/.source + "(?:[ 	]*" + /(?:![ \t]*)?(?:\b\w+\b(?:[ \t]*\((?:[^()]|\([^()]*\))*\))?|\((?:[^()]|\([^()]*\))*\))(?:[ \t]*(?:&&|\|\|))?/.source + ")+") + "|" + /(?:else|endif)\b/.source + ")"
    ),
    alias: "property",
    inside: {
      "directive-name": /^#\w+/,
      "boolean": /\b(?:false|true)\b/,
      "number": /\b\d+(?:\.\d+)*\b/,
      "operator": /!|&&|\|\||[<>]=?/,
      "punctuation": /[(),]/
    }
  },
  "literal": {
    pattern: /#(?:colorLiteral|column|dsohandle|file(?:ID|Literal|Path)?|function|imageLiteral|line)\b/,
    alias: "constant"
  },
  "other-directive": {
    pattern: /#\w+\b/,
    alias: "property"
  },
  "attribute": {
    pattern: /@\w+/,
    alias: "atrule"
  },
  "function-definition": {
    pattern: /(\bfunc\s+)\w+/,
    lookbehind: true,
    alias: "function"
  },
  "label": {
    // https://docs.swift.org/swift-book/LanguageGuide/ControlFlow.html#ID141
    pattern: /\b(break|continue)\s+\w+|\b[a-zA-Z_]\w*(?=\s*:\s*(?:for|repeat|while)\b)/,
    lookbehind: true,
    alias: "important"
  },
  "keyword": /\b(?:Any|Protocol|Self|Type|actor|as|assignment|associatedtype|associativity|async|await|break|case|catch|class|continue|convenience|default|defer|deinit|didSet|do|dynamic|else|enum|extension|fallthrough|fileprivate|final|for|func|get|guard|higherThan|if|import|in|indirect|infix|init|inout|internal|is|isolated|lazy|left|let|lowerThan|mutating|none|nonisolated|nonmutating|open|operator|optional|override|postfix|precedencegroup|prefix|private|protocol|public|repeat|required|rethrows|return|right|safe|self|set|some|static|struct|subscript|super|switch|throw|throws|try|typealias|unowned|unsafe|var|weak|where|while|willSet)\b/,
  "boolean": /\b(?:false|true)\b/,
  "nil": {
    pattern: /\bnil\b/,
    alias: "constant"
  },
  "short-argument": /\$\d+\b/,
  "omit": {
    pattern: /\b_\b/,
    alias: "keyword"
  },
  "number": /\b(?:[\d_]+(?:\.[\de_]+)?|0x[a-f0-9_]+(?:\.[a-f0-9p_]+)?|0b[01_]+|0o[0-7_]+)\b/i,
  // A class name must start with an upper-case letter and be either 1 letter long or contain a lower-case letter.
  "class-name": /\b[A-Z](?:[A-Z_\d]*[a-z]\w*)?\b/,
  "function": /\b[a-z_]\w*(?=\s*\()/i,
  "constant": /\b(?:[A-Z_]{2,}|k[A-Z][A-Za-z_]+)\b/,
  // Operators are generic in Swift. Developers can even create new operators (e.g. +++).
  // https://docs.swift.org/swift-book/ReferenceManual/zzSummaryOfTheGrammar.html#ID481
  // This regex only supports ASCII operators.
  "operator": /[-+*/%=!<>&|^~?]+|\.[.\-+*/%=!<>&|^~?]+/,
  "punctuation": /[{}[\]();,.:\\]/
};
Prism.languages.swift["string-literal"].forEach(function(rule) {
  rule.inside["interpolation"].inside = Prism.languages.swift;
});

// node_modules/prismjs/components/prism-typescript.js
(function(Prism2) {
  Prism2.languages.typescript = Prism2.languages.extend("javascript", {
    "class-name": {
      pattern: /(\b(?:class|extends|implements|instanceof|interface|new|type)\s+)(?!keyof\b)(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?:\s*<(?:[^<>]|<(?:[^<>]|<[^<>]*>)*>)*>)?/,
      lookbehind: true,
      greedy: true,
      inside: null
      // see below
    },
    "builtin": /\b(?:Array|Function|Promise|any|boolean|console|never|number|string|symbol|unknown)\b/
  });
  Prism2.languages.typescript.keyword.push(
    /\b(?:abstract|declare|is|keyof|readonly|require)\b/,
    // keywords that have to be followed by an identifier
    /\b(?:asserts|infer|interface|module|namespace|type)\b(?=\s*(?:[{_$a-zA-Z\xA0-\uFFFF]|$))/,
    // This is for `import type *, {}`
    /\btype\b(?=\s*(?:[\{*]|$))/
  );
  delete Prism2.languages.typescript["parameter"];
  delete Prism2.languages.typescript["literal-property"];
  var typeInside = Prism2.languages.extend("typescript", {});
  delete typeInside["class-name"];
  Prism2.languages.typescript["class-name"].inside = typeInside;
  Prism2.languages.insertBefore("typescript", "function", {
    "decorator": {
      pattern: /@[$\w\xA0-\uFFFF]+/,
      inside: {
        "at": {
          pattern: /^@/,
          alias: "operator"
        },
        "function": /^[\s\S]+/
      }
    },
    "generic-function": {
      // e.g. foo<T extends "bar" | "baz">( ...
      pattern: /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*\s*<(?:[^<>]|<(?:[^<>]|<[^<>]*>)*>)*>(?=\s*\()/,
      greedy: true,
      inside: {
        "function": /^#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*/,
        "generic": {
          pattern: /<[\s\S]+/,
          // everything after the first <
          alias: "class-name",
          inside: typeInside
        }
      }
    }
  });
  Prism2.languages.ts = Prism2.languages.typescript;
})(Prism);

// node_modules/prismjs/components/prism-java.js
(function(Prism2) {
  var keywords = /\b(?:abstract|assert|boolean|break|byte|case|catch|char|class|const|continue|default|do|double|else|enum|exports|extends|final|finally|float|for|goto|if|implements|import|instanceof|int|interface|long|module|native|new|non-sealed|null|open|opens|package|permits|private|protected|provides|public|record(?!\s*[(){}[\]<>=%~.:,;?+\-*/&|^])|requires|return|sealed|short|static|strictfp|super|switch|synchronized|this|throw|throws|to|transient|transitive|try|uses|var|void|volatile|while|with|yield)\b/;
  var classNamePrefix = /(?:[a-z]\w*\s*\.\s*)*(?:[A-Z]\w*\s*\.\s*)*/.source;
  var className = {
    pattern: RegExp(/(^|[^\w.])/.source + classNamePrefix + /[A-Z](?:[\d_A-Z]*[a-z]\w*)?\b/.source),
    lookbehind: true,
    inside: {
      "namespace": {
        pattern: /^[a-z]\w*(?:\s*\.\s*[a-z]\w*)*(?:\s*\.)?/,
        inside: {
          "punctuation": /\./
        }
      },
      "punctuation": /\./
    }
  };
  Prism2.languages.java = Prism2.languages.extend("clike", {
    "string": {
      pattern: /(^|[^\\])"(?:\\.|[^"\\\r\n])*"/,
      lookbehind: true,
      greedy: true
    },
    "class-name": [
      className,
      {
        // variables, parameters, and constructor references
        // this to support class names (or generic parameters) which do not contain a lower case letter (also works for methods)
        pattern: RegExp(/(^|[^\w.])/.source + classNamePrefix + /[A-Z]\w*(?=\s+\w+\s*[;,=()]|\s*(?:\[[\s,]*\]\s*)?::\s*new\b)/.source),
        lookbehind: true,
        inside: className.inside
      },
      {
        // class names based on keyword
        // this to support class names (or generic parameters) which do not contain a lower case letter (also works for methods)
        pattern: RegExp(/(\b(?:class|enum|extends|implements|instanceof|interface|new|record|throws)\s+)/.source + classNamePrefix + /[A-Z]\w*\b/.source),
        lookbehind: true,
        inside: className.inside
      }
    ],
    "keyword": keywords,
    "function": [
      Prism2.languages.clike.function,
      {
        pattern: /(::\s*)[a-z_]\w*/,
        lookbehind: true
      }
    ],
    "number": /\b0b[01][01_]*L?\b|\b0x(?:\.[\da-f_p+-]+|[\da-f_]+(?:\.[\da-f_p+-]+)?)\b|(?:\b\d[\d_]*(?:\.[\d_]*)?|\B\.\d[\d_]*)(?:e[+-]?\d[\d_]*)?[dfl]?/i,
    "operator": {
      pattern: /(^|[^.])(?:<<=?|>>>?=?|->|--|\+\+|&&|\|\||::|[?:~]|[-+*/%&|^!=<>]=?)/m,
      lookbehind: true
    },
    "constant": /\b[A-Z][A-Z_\d]+\b/
  });
  Prism2.languages.insertBefore("java", "string", {
    "triple-quoted-string": {
      // http://openjdk.java.net/jeps/355#Description
      pattern: /"""[ \t]*[\r\n](?:(?:"|"")?(?:\\.|[^"\\]))*"""/,
      greedy: true,
      alias: "string"
    },
    "char": {
      pattern: /'(?:\\.|[^'\\\r\n]){1,6}'/,
      greedy: true
    }
  });
  Prism2.languages.insertBefore("java", "class-name", {
    "annotation": {
      pattern: /(^|[^.])@\w+(?:\s*\.\s*\w+)*/,
      lookbehind: true,
      alias: "punctuation"
    },
    "generics": {
      pattern: /<(?:[\w\s,.?]|&(?!&)|<(?:[\w\s,.?]|&(?!&)|<(?:[\w\s,.?]|&(?!&)|<(?:[\w\s,.?]|&(?!&))*>)*>)*>)*>/,
      inside: {
        "class-name": className,
        "keyword": keywords,
        "punctuation": /[<>(),.:]/,
        "operator": /[?&|]/
      }
    },
    "import": [
      {
        pattern: RegExp(/(\bimport\s+)/.source + classNamePrefix + /(?:[A-Z]\w*|\*)(?=\s*;)/.source),
        lookbehind: true,
        inside: {
          "namespace": className.inside.namespace,
          "punctuation": /\./,
          "operator": /\*/,
          "class-name": /\w+/
        }
      },
      {
        pattern: RegExp(/(\bimport\s+static\s+)/.source + classNamePrefix + /(?:\w+|\*)(?=\s*;)/.source),
        lookbehind: true,
        alias: "static",
        inside: {
          "namespace": className.inside.namespace,
          "static": /\b\w+$/,
          "punctuation": /\./,
          "operator": /\*/,
          "class-name": /\w+/
        }
      }
    ],
    "namespace": {
      pattern: RegExp(
        /(\b(?:exports|import(?:\s+static)?|module|open|opens|package|provides|requires|to|transitive|uses|with)\s+)(?!<keyword>)[a-z]\w*(?:\.[a-z]\w*)*\.?/.source.replace(/<keyword>/g, function() {
          return keywords.source;
        })
      ),
      lookbehind: true,
      inside: {
        "punctuation": /\./
      }
    }
  });
})(Prism);

// node_modules/prismjs/components/prism-cpp.js
(function(Prism2) {
  var keyword = /\b(?:alignas|alignof|asm|auto|bool|break|case|catch|char|char16_t|char32_t|char8_t|class|co_await|co_return|co_yield|compl|concept|const|const_cast|consteval|constexpr|constinit|continue|decltype|default|delete|do|double|dynamic_cast|else|enum|explicit|export|extern|final|float|for|friend|goto|if|import|inline|int|int16_t|int32_t|int64_t|int8_t|long|module|mutable|namespace|new|noexcept|nullptr|operator|override|private|protected|public|register|reinterpret_cast|requires|return|short|signed|sizeof|static|static_assert|static_cast|struct|switch|template|this|thread_local|throw|try|typedef|typeid|typename|uint16_t|uint32_t|uint64_t|uint8_t|union|unsigned|using|virtual|void|volatile|wchar_t|while)\b/;
  var modName = /\b(?!<keyword>)\w+(?:\s*\.\s*\w+)*\b/.source.replace(/<keyword>/g, function() {
    return keyword.source;
  });
  Prism2.languages.cpp = Prism2.languages.extend("c", {
    "class-name": [
      {
        pattern: RegExp(/(\b(?:class|concept|enum|struct|typename)\s+)(?!<keyword>)\w+/.source.replace(/<keyword>/g, function() {
          return keyword.source;
        })),
        lookbehind: true
      },
      // This is intended to capture the class name of method implementations like:
      //   void foo::bar() const {}
      // However! The `foo` in the above example could also be a namespace, so we only capture the class name if
      // it starts with an uppercase letter. This approximation should give decent results.
      /\b[A-Z]\w*(?=\s*::\s*\w+\s*\()/,
      // This will capture the class name before destructors like:
      //   Foo::~Foo() {}
      /\b[A-Z_]\w*(?=\s*::\s*~\w+\s*\()/i,
      // This also intends to capture the class name of method implementations but here the class has template
      // parameters, so it can't be a namespace (until C++ adds generic namespaces).
      /\b\w+(?=\s*<(?:[^<>]|<(?:[^<>]|<[^<>]*>)*>)*>\s*::\s*\w+\s*\()/
    ],
    "keyword": keyword,
    "number": {
      pattern: /(?:\b0b[01']+|\b0x(?:[\da-f']+(?:\.[\da-f']*)?|\.[\da-f']+)(?:p[+-]?[\d']+)?|(?:\b[\d']+(?:\.[\d']*)?|\B\.[\d']+)(?:e[+-]?[\d']+)?)[ful]{0,4}/i,
      greedy: true
    },
    "operator": />>=?|<<=?|->|--|\+\+|&&|\|\||[?:~]|<=>|[-+*/%&|^!=<>]=?|\b(?:and|and_eq|bitand|bitor|not|not_eq|or|or_eq|xor|xor_eq)\b/,
    "boolean": /\b(?:false|true)\b/
  });
  Prism2.languages.insertBefore("cpp", "string", {
    "module": {
      // https://en.cppreference.com/w/cpp/language/modules
      pattern: RegExp(
        /(\b(?:import|module)\s+)/.source + "(?:" + // header-name
        /"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|<[^<>\r\n]*>/.source + "|" + // module name or partition or both
        /<mod-name>(?:\s*:\s*<mod-name>)?|:\s*<mod-name>/.source.replace(/<mod-name>/g, function() {
          return modName;
        }) + ")"
      ),
      lookbehind: true,
      greedy: true,
      inside: {
        "string": /^[<"][\s\S]+/,
        "operator": /:/,
        "punctuation": /\./
      }
    },
    "raw-string": {
      pattern: /R"([^()\\ ]{0,16})\([\s\S]*?\)\1"/,
      alias: "string",
      greedy: true
    }
  });
  Prism2.languages.insertBefore("cpp", "keyword", {
    "generic-function": {
      pattern: /\b(?!operator\b)[a-z_]\w*\s*<(?:[^<>]|<[^<>]*>)*>(?=\s*\()/i,
      inside: {
        "function": /^\w+/,
        "generic": {
          pattern: /<[\s\S]+/,
          alias: "class-name",
          inside: Prism2.languages.cpp
        }
      }
    }
  });
  Prism2.languages.insertBefore("cpp", "operator", {
    "double-colon": {
      pattern: /::/,
      alias: "punctuation"
    }
  });
  Prism2.languages.insertBefore("cpp", "class-name", {
    // the base clause is an optional list of parent classes
    // https://en.cppreference.com/w/cpp/language/class
    "base-clause": {
      pattern: /(\b(?:class|struct)\s+\w+\s*:\s*)[^;{}"'\s]+(?:\s+[^;{}"'\s]+)*(?=\s*[;{])/,
      lookbehind: true,
      greedy: true,
      inside: Prism2.languages.extend("cpp", {})
    }
  });
  Prism2.languages.insertBefore("inside", "double-colon", {
    // All untokenized words that are not namespaces should be class names
    "class-name": /\b[a-z_]\w*\b(?!\s*::)/i
  }, Prism2.languages.cpp["base-clause"]);
})(Prism);

// node_modules/@lexical/code-prism/LexicalCodePrism.dev.mjs
function formatDevErrorMessage10(message) {
  throw new Error(message);
}
(function(Prism2) {
  Prism2.languages.diff = {
    "coord": [
      // Match all kinds of coord lines (prefixed by "+++", "---" or "***").
      /^(?:\*{3}|-{3}|\+{3}).*$/m,
      // Match "@@ ... @@" coord lines in unified diff.
      /^@@.*@@$/m,
      // Match coord lines in normal diff (starts with a number).
      /^\d.*$/m
    ]
    // deleted, inserted, unchanged, diff
  };
  var PREFIXES = {
    "deleted-sign": "-",
    "deleted-arrow": "<",
    "inserted-sign": "+",
    "inserted-arrow": ">",
    "unchanged": " ",
    "diff": "!"
  };
  Object.keys(PREFIXES).forEach(function(name) {
    var prefix = PREFIXES[name];
    var alias = [];
    if (!/^\w+$/.test(name)) {
      alias.push(/\w+/.exec(name)[0]);
    }
    if (name === "diff") {
      alias.push("bold");
    }
    Prism2.languages.diff[name] = {
      pattern: RegExp("^(?:[" + prefix + "].*(?:\r\n?|\n|(?![\\s\\S])))+", "m"),
      alias,
      inside: {
        "line": {
          pattern: /(.)(?=[\s\S]).*(?:\r\n?|\n)?/,
          lookbehind: true
        },
        "prefix": {
          pattern: /[\s\S]/,
          alias: /\w+/.exec(name)[0]
        }
      }
    };
  });
  Object.defineProperty(Prism2.languages.diff, "PREFIXES", {
    value: PREFIXES
  });
})(Prism);
var Prism$1 = globalThis.Prism || window.Prism;
var CODE_LANGUAGE_FRIENDLY_NAME_MAP = {
  c: "C",
  clike: "C-like",
  cpp: "C++",
  css: "CSS",
  html: "HTML",
  java: "Java",
  js: "JavaScript",
  markdown: "Markdown",
  objc: "Objective-C",
  plain: "Plain Text",
  powershell: "PowerShell",
  py: "Python",
  rust: "Rust",
  sql: "SQL",
  swift: "Swift",
  typescript: "TypeScript",
  xml: "XML"
};
var CODE_LANGUAGE_MAP = {
  cpp: "cpp",
  java: "java",
  javascript: "js",
  md: "markdown",
  plaintext: "plain",
  python: "py",
  text: "plain",
  ts: "typescript"
};
function normalizeCodeLanguage(lang) {
  return CODE_LANGUAGE_MAP[lang] || lang;
}
function getLanguageFriendlyName(lang) {
  const _lang = normalizeCodeLanguage(lang);
  return CODE_LANGUAGE_FRIENDLY_NAME_MAP[_lang] || _lang;
}
var getCodeLanguages = () => Object.keys(Prism$1.languages).filter(
  // Prism has several language helpers mixed into languages object
  // so filtering them out here to get langs list
  (language) => typeof Prism$1.languages[language] !== "function"
).sort();
function getCodeLanguageOptions() {
  const options = [];
  for (const [lang, friendlyName] of Object.entries(CODE_LANGUAGE_FRIENDLY_NAME_MAP)) {
    options.push([lang, friendlyName]);
  }
  return options;
}
function getCodeThemeOptions() {
  const options = [];
  return options;
}
function getDiffedLanguage(language) {
  const DIFF_LANGUAGE_REGEX = /^diff-([\w-]+)/i;
  const diffLanguageMatch = DIFF_LANGUAGE_REGEX.exec(language);
  return diffLanguageMatch ? diffLanguageMatch[1] : null;
}
function isCodeLanguageLoaded(language) {
  const diffedLanguage = getDiffedLanguage(language);
  const langId = diffedLanguage ? diffedLanguage : language;
  try {
    return langId ? Prism$1.languages.hasOwnProperty(langId) : false;
  } catch (_unused) {
    return false;
  }
}
async function loadCodeLanguage(language, editor, codeNodeKey) {
}
function getTextContent(token) {
  if (typeof token === "string") {
    return token;
  } else if (Array.isArray(token)) {
    return token.map(getTextContent).join("");
  } else {
    return getTextContent(token.content);
  }
}
function tokenizeDiffHighlight(tokens, language) {
  const diffLanguage = language;
  const diffGrammar = Prism$1.languages[diffLanguage];
  const env = {
    tokens
  };
  const PREFIXES = Prism$1.languages.diff.PREFIXES;
  for (const token of env.tokens) {
    if (typeof token === "string" || !(token.type in PREFIXES) || !Array.isArray(token.content)) {
      continue;
    }
    const type = token.type;
    let insertedPrefixes = 0;
    const getPrefixToken = () => {
      insertedPrefixes++;
      return new Prism$1.Token("prefix", PREFIXES[type], type.replace(/^(\w+).*/, "$1"));
    };
    const withoutPrefixes = token.content.filter((t2) => typeof t2 === "string" || t2.type !== "prefix");
    const prefixCount = token.content.length - withoutPrefixes.length;
    const diffTokens = Prism$1.tokenize(getTextContent(withoutPrefixes), diffGrammar);
    diffTokens.unshift(getPrefixToken());
    const LINE_BREAK = /\r\n|\n/g;
    const insertAfterLineBreakString = (text) => {
      const result = [];
      LINE_BREAK.lastIndex = 0;
      let last = 0;
      let m2;
      while (insertedPrefixes < prefixCount && (m2 = LINE_BREAK.exec(text))) {
        const end = m2.index + m2[0].length;
        result.push(text.slice(last, end));
        last = end;
        result.push(getPrefixToken());
      }
      if (result.length === 0) {
        return void 0;
      }
      if (last < text.length) {
        result.push(text.slice(last));
      }
      return result;
    };
    const insertAfterLineBreak = (toks) => {
      for (let i2 = 0; i2 < toks.length && insertedPrefixes < prefixCount; i2++) {
        const tok = toks[i2];
        if (typeof tok === "string") {
          const inserted = insertAfterLineBreakString(tok);
          if (inserted) {
            toks.splice(i2, 1, ...inserted);
            i2 += inserted.length - 1;
          }
        } else if (typeof tok.content === "string") {
          const inserted = insertAfterLineBreakString(tok.content);
          if (inserted) {
            tok.content = inserted;
          }
        } else if (Array.isArray(tok.content)) {
          insertAfterLineBreak(tok.content);
        } else {
          insertAfterLineBreak([tok.content]);
        }
      }
    };
    insertAfterLineBreak(diffTokens);
    if (insertedPrefixes < prefixCount) {
      diffTokens.push(getPrefixToken());
    }
    token.content = diffTokens;
  }
  return env.tokens;
}
function $getHighlightNodes(codeNode, language) {
  const DIFF_LANGUAGE_REGEX = /^diff-([\w-]+)/i;
  const diffLanguageMatch = DIFF_LANGUAGE_REGEX.exec(language);
  const code = codeNode.getTextContent();
  let tokens = Prism$1.tokenize(code, Prism$1.languages[diffLanguageMatch ? "diff" : language]);
  if (diffLanguageMatch) {
    tokens = tokenizeDiffHighlight(tokens, diffLanguageMatch[1]);
  }
  return $mapTokensToLexicalStructure(tokens);
}
function $mapTokensToLexicalStructure(tokens, type) {
  const nodes = [];
  for (const token of tokens) {
    if (typeof token === "string") {
      const partials = token.split(/(\n|\t)/);
      const partialsLength = partials.length;
      for (let i2 = 0; i2 < partialsLength; i2++) {
        const part = partials[i2];
        if (part === "\n" || part === "\r\n") {
          nodes.push($createLineBreakNode2());
        } else if (part === "	") {
          nodes.push($createTabNode2());
        } else if (part.length > 0) {
          nodes.push($createCodeHighlightNode2(part, type));
        }
      }
    } else {
      const {
        content,
        alias
      } = token;
      if (typeof content === "string") {
        nodes.push(...$mapTokensToLexicalStructure([content], token.type === "prefix" && typeof alias === "string" ? alias : token.type));
      } else if (Array.isArray(content)) {
        nodes.push(...$mapTokensToLexicalStructure(content, token.type === "unchanged" ? void 0 : token.type));
      }
    }
  }
  return nodes;
}
var PrismTokenizer = {
  $tokenize(codeNode, language) {
    return $getHighlightNodes(codeNode, language || this.defaultLanguage);
  },
  defaultLanguage: DEFAULT_CODE_LANGUAGE2,
  tokenize(code, language) {
    return Prism$1.tokenize(code, Prism$1.languages[language || ""] || Prism$1.languages[this.defaultLanguage]);
  }
};
function $textNodeTransform(editor, tokenizer, transformState, node) {
  const parentNode = node.getParent();
  if ($isCodeNode2(parentNode)) {
    $codeNodeTransform(editor, tokenizer, transformState, parentNode);
  } else if ($isCodeHighlightNode2(node)) {
    node.replace($createTextNode2(node.__text));
  }
}
function updateCodeGutter(node, editor) {
  const codeElement = editor.getElementByKey(node.getKey());
  if (codeElement === null) {
    return;
  }
  const children = node.getChildren();
  const childrenLength = children.length;
  if (childrenLength === codeElement.__cachedChildrenLength) {
    return;
  }
  codeElement.__cachedChildrenLength = childrenLength;
  let gutter = "1";
  let count = 1;
  for (let i2 = 0; i2 < childrenLength; i2++) {
    if ($isLineBreakNode2(children[i2])) {
      gutter += "\n" + ++count;
    }
  }
  codeElement.setAttribute("data-gutter", gutter);
}
function $codeNodeTransform(editor, tokenizer, transformState, node) {
  const {
    nodesCurrentlyHighlighting
  } = transformState;
  const nodeKey = node.getKey();
  if (node.getLanguage() === void 0) {
    node.setLanguage(tokenizer.defaultLanguage);
  }
  const language = node.getLanguage() || tokenizer.defaultLanguage;
  if (isCodeLanguageLoaded(language)) {
    if (!node.getIsSyntaxHighlightSupported()) {
      node.setIsSyntaxHighlightSupported(true);
    }
  } else {
    if (node.getIsSyntaxHighlightSupported()) {
      node.setIsSyntaxHighlightSupported(false);
    }
    loadCodeLanguage(language, editor, nodeKey);
    return;
  }
  if (nodesCurrentlyHighlighting.has(nodeKey)) {
    return;
  }
  nodesCurrentlyHighlighting.add(nodeKey);
  if (!transformState.didTransform) {
    transformState.didTransform = true;
    $onUpdate2(() => {
      transformState.didTransform = false;
      nodesCurrentlyHighlighting.clear();
    });
  }
  $updateAndRetainSelection(nodeKey, () => {
    const currentNode = $getNodeByKey2(nodeKey);
    if (!$isCodeNode2(currentNode) || !currentNode.isAttached()) {
      return false;
    }
    const currentLanguage = currentNode.getLanguage() || tokenizer.defaultLanguage;
    const highlightNodes = tokenizer.$tokenize(currentNode, currentLanguage);
    const diffRange = getDiffRange(currentNode.getChildren(), highlightNodes);
    const {
      from,
      to,
      nodesForReplacement
    } = diffRange;
    if (from !== to || nodesForReplacement.length) {
      node.splice(from, to - from, nodesForReplacement);
      return true;
    }
    return false;
  });
}
function $updateAndRetainSelection(nodeKey, updateFn) {
  const node = $getNodeByKey2(nodeKey);
  if (!$isCodeNode2(node) || !node.isAttached()) {
    return;
  }
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection)) {
    updateFn();
    return;
  }
  const anchor = selection.anchor;
  const anchorOffset = anchor.offset;
  const isNewLineAnchor = anchor.type === "element" && $isLineBreakNode2(node.getChildAtIndex(anchor.offset - 1));
  let textOffset = 0;
  if (!isNewLineAnchor) {
    const anchorNode = anchor.getNode();
    textOffset = anchorOffset + anchorNode.getPreviousSiblings().reduce((offset, _node) => {
      return offset + _node.getTextContentSize();
    }, 0);
  }
  const hasChanges = updateFn();
  if (!hasChanges) {
    return;
  }
  if (isNewLineAnchor) {
    anchor.getNode().select(anchorOffset, anchorOffset);
    return;
  }
  node.getChildren().some((_node) => {
    const isText = $isTextNode2(_node);
    if (isText || $isLineBreakNode2(_node)) {
      const textContentSize = _node.getTextContentSize();
      if (isText && textContentSize >= textOffset) {
        _node.select(textOffset, textOffset);
        return true;
      }
      textOffset -= textContentSize;
    }
    return false;
  });
}
function getDiffRange(prevNodes, nextNodes) {
  let leadingMatch = 0;
  while (leadingMatch < prevNodes.length) {
    if (!isEqual(prevNodes[leadingMatch], nextNodes[leadingMatch])) {
      break;
    }
    leadingMatch++;
  }
  const prevNodesLength = prevNodes.length;
  const nextNodesLength = nextNodes.length;
  const maxTrailingMatch = Math.min(prevNodesLength, nextNodesLength) - leadingMatch;
  let trailingMatch = 0;
  while (trailingMatch < maxTrailingMatch) {
    trailingMatch++;
    if (!isEqual(prevNodes[prevNodesLength - trailingMatch], nextNodes[nextNodesLength - trailingMatch])) {
      trailingMatch--;
      break;
    }
  }
  const from = leadingMatch;
  const to = prevNodesLength - trailingMatch;
  const nodesForReplacement = nextNodes.slice(leadingMatch, nextNodesLength - trailingMatch);
  return {
    from,
    nodesForReplacement,
    to
  };
}
function isEqual(nodeA, nodeB) {
  return $isCodeHighlightNode2(nodeA) && $isCodeHighlightNode2(nodeB) && nodeA.__text === nodeB.__text && nodeA.__highlightType === nodeB.__highlightType || $isTabNode2(nodeA) && $isTabNode2(nodeB) || $isLineBreakNode2(nodeA) && $isLineBreakNode2(nodeB);
}
function $isSelectionInCode(selection) {
  if (!$isRangeSelection2(selection)) {
    return false;
  }
  const anchorNode = selection.anchor.getNode();
  const maybeAnchorCodeNode = $isCodeNode2(anchorNode) ? anchorNode : anchorNode.getParent();
  const focusNode = selection.focus.getNode();
  const maybeFocusCodeNode = $isCodeNode2(focusNode) ? focusNode : focusNode.getParent();
  return $isCodeNode2(maybeAnchorCodeNode) && maybeAnchorCodeNode.is(maybeFocusCodeNode);
}
function $getCodeLines(selection) {
  const nodes = selection.getNodes();
  const lines = [];
  if (nodes.length === 1 && $isCodeNode2(nodes[0])) {
    return lines;
  }
  let lastLine = [];
  for (let i2 = 0; i2 < nodes.length; i2++) {
    const node = nodes[i2];
    if (!($isCodeHighlightNode2(node) || $isTabNode2(node) || $isLineBreakNode2(node))) {
      formatDevErrorMessage10(`Expected selection to be inside CodeBlock and consisting of CodeHighlightNode, TabNode and LineBreakNode`);
    }
    if ($isLineBreakNode2(node)) {
      if (lastLine.length > 0) {
        lines.push(lastLine);
        lastLine = [];
      }
    } else {
      lastLine.push(node);
    }
  }
  if (lastLine.length > 0) {
    const selectionEnd = selection.isBackward() ? selection.anchor : selection.focus;
    const lastPoint = $createPoint2(lastLine[0].getKey(), 0, "text");
    if (!selectionEnd.is(lastPoint)) {
      lines.push(lastLine);
    }
  }
  return lines;
}
function $handleTab(shiftKey) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection) || !$isSelectionInCode(selection)) {
    return null;
  }
  const indentOrOutdent = !shiftKey ? INDENT_CONTENT_COMMAND2 : OUTDENT_CONTENT_COMMAND2;
  const tabOrOutdent = !shiftKey ? INSERT_TAB_COMMAND2 : OUTDENT_CONTENT_COMMAND2;
  const anchor = selection.anchor;
  const focus = selection.focus;
  if (anchor.is(focus)) {
    return tabOrOutdent;
  }
  const codeLines = $getCodeLines(selection);
  if (codeLines.length !== 1) {
    return indentOrOutdent;
  }
  const codeLine = codeLines[0];
  const codeLineLength = codeLine.length;
  if (!(codeLineLength !== 0)) {
    formatDevErrorMessage10(`$getCodeLines only extracts non-empty lines`);
  }
  let selectionFirst;
  let selectionLast;
  if (selection.isBackward()) {
    selectionFirst = focus;
    selectionLast = anchor;
  } else {
    selectionFirst = anchor;
    selectionLast = focus;
  }
  const firstOfLine = $getFirstCodeNodeOfLine2(codeLine[0]);
  const lastOfLine = $getLastCodeNodeOfLine2(codeLine[0]);
  const anchorOfLine = $createPoint2(firstOfLine.getKey(), 0, "text");
  const focusOfLine = $createPoint2(lastOfLine.getKey(), lastOfLine.getTextContentSize(), "text");
  if (selectionFirst.isBefore(anchorOfLine)) {
    return indentOrOutdent;
  }
  if (focusOfLine.isBefore(selectionLast)) {
    return indentOrOutdent;
  }
  if (anchorOfLine.isBefore(selectionFirst) || selectionLast.isBefore(focusOfLine)) {
    return tabOrOutdent;
  }
  return indentOrOutdent;
}
function $handleMultilineIndent(type) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection) || !$isSelectionInCode(selection)) {
    return false;
  }
  const codeLines = $getCodeLines(selection);
  const codeLinesLength = codeLines.length;
  if (codeLinesLength === 0 && selection.isCollapsed()) {
    if (type === INDENT_CONTENT_COMMAND2) {
      selection.insertNodes([$createTabNode2()]);
    }
    return true;
  }
  if (codeLinesLength === 0 && type === INDENT_CONTENT_COMMAND2 && selection.getTextContent() === "\n") {
    const tabNode = $createTabNode2();
    const lineBreakNode = $createLineBreakNode2();
    const direction = selection.isBackward() ? "previous" : "next";
    selection.insertNodes([tabNode, lineBreakNode]);
    $setSelectionFromCaretRange2($getCaretRangeInDirection2($getCaretRange2($getTextPointCaret2(tabNode, "next", 0), $normalizeCaret2($getSiblingCaret2(lineBreakNode, "next"))), direction));
    return true;
  }
  for (let i2 = 0; i2 < codeLinesLength; i2++) {
    const line = codeLines[i2];
    if (line.length > 0) {
      let firstOfLine = line[0];
      if (i2 === 0) {
        firstOfLine = $getFirstCodeNodeOfLine2(firstOfLine);
      }
      if (type === INDENT_CONTENT_COMMAND2) {
        const tabNode = $createTabNode2();
        firstOfLine.insertBefore(tabNode);
        if (i2 === 0) {
          const anchorKey = selection.isBackward() ? "focus" : "anchor";
          const anchorLine = $createPoint2(firstOfLine.getKey(), 0, "text");
          if (selection[anchorKey].is(anchorLine)) {
            selection[anchorKey].set(tabNode.getKey(), 0, "text");
          }
        }
      } else if ($isTabNode2(firstOfLine)) {
        firstOfLine.remove();
      }
    }
  }
  return true;
}
function $handleShiftLines(type, event) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection)) {
    return false;
  }
  const {
    anchor,
    focus
  } = selection;
  const anchorOffset = anchor.offset;
  const focusOffset = focus.offset;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  const arrowIsUp = type === KEY_ARROW_UP_COMMAND2;
  if (!$isSelectionInCode(selection) || !($isCodeHighlightNode2(anchorNode) || $isTabNode2(anchorNode)) || !($isCodeHighlightNode2(focusNode) || $isTabNode2(focusNode))) {
    return false;
  }
  if (!event.altKey) {
    if (selection.isCollapsed()) {
      const codeNode = anchorNode.getParentOrThrow();
      if (arrowIsUp && anchorOffset === 0 && anchorNode.getPreviousSibling() === null) {
        const codeNodeSibling = codeNode.getPreviousSibling();
        if (codeNodeSibling === null) {
          codeNode.selectPrevious();
          event.preventDefault();
          return true;
        }
      } else if (!arrowIsUp && anchorOffset === anchorNode.getTextContentSize() && anchorNode.getNextSibling() === null) {
        const codeNodeSibling = codeNode.getNextSibling();
        if (codeNodeSibling === null) {
          codeNode.selectNext();
          event.preventDefault();
          return true;
        }
      }
    }
    return false;
  }
  let start;
  let end;
  if (anchorNode.isBefore(focusNode)) {
    start = $getFirstCodeNodeOfLine2(anchorNode);
    end = $getLastCodeNodeOfLine2(focusNode);
  } else {
    start = $getFirstCodeNodeOfLine2(focusNode);
    end = $getLastCodeNodeOfLine2(anchorNode);
  }
  if (start == null || end == null) {
    return false;
  }
  const range = start.getNodesBetween(end);
  for (let i2 = 0; i2 < range.length; i2++) {
    const node = range[i2];
    if (!$isCodeHighlightNode2(node) && !$isTabNode2(node) && !$isLineBreakNode2(node)) {
      return false;
    }
  }
  event.preventDefault();
  event.stopPropagation();
  const linebreak = arrowIsUp ? start.getPreviousSibling() : end.getNextSibling();
  if (!$isLineBreakNode2(linebreak)) {
    return true;
  }
  const sibling = arrowIsUp ? linebreak.getPreviousSibling() : linebreak.getNextSibling();
  if (sibling == null) {
    return true;
  }
  const maybeInsertionPoint = $isCodeHighlightNode2(sibling) || $isTabNode2(sibling) || $isLineBreakNode2(sibling) ? arrowIsUp ? $getFirstCodeNodeOfLine2(sibling) : $getLastCodeNodeOfLine2(sibling) : null;
  let insertionPoint = maybeInsertionPoint != null ? maybeInsertionPoint : sibling;
  linebreak.remove();
  range.forEach((node) => node.remove());
  if (type === KEY_ARROW_UP_COMMAND2) {
    range.forEach((node) => insertionPoint.insertBefore(node));
    insertionPoint.insertBefore(linebreak);
  } else {
    insertionPoint.insertAfter(linebreak);
    insertionPoint = linebreak;
    range.forEach((node) => {
      insertionPoint.insertAfter(node);
      insertionPoint = node;
    });
  }
  selection.setTextNodeRange(anchorNode, anchorOffset, focusNode, focusOffset);
  return true;
}
function $handleMoveTo(type, event) {
  const selection = $getSelection2();
  if (!$isRangeSelection2(selection)) {
    return false;
  }
  const {
    anchor,
    focus
  } = selection;
  const anchorNode = anchor.getNode();
  const focusNode = focus.getNode();
  const isMoveToStart2 = type === MOVE_TO_START2;
  if (!$isSelectionInCode(selection) || !($isCodeHighlightNode2(anchorNode) || $isTabNode2(anchorNode)) || !($isCodeHighlightNode2(focusNode) || $isTabNode2(focusNode))) {
    return false;
  }
  const focusLineNode = focusNode;
  const direction = $getCodeLineDirection2(focusLineNode);
  const moveToStart = direction === "rtl" ? !isMoveToStart2 : isMoveToStart2;
  if (moveToStart) {
    const start = $getStartOfCodeInLine2(focusLineNode, focus.offset);
    if (start !== null) {
      const {
        node,
        offset
      } = start;
      if ($isLineBreakNode2(node)) {
        node.selectNext(0, 0);
      } else {
        selection.setTextNodeRange(node, offset, node, offset);
      }
    } else {
      focusLineNode.getParentOrThrow().selectStart();
    }
  } else {
    const node = $getEndOfCodeInLine2(focusLineNode);
    node.select();
  }
  event.preventDefault();
  event.stopPropagation();
  return true;
}
function registerCodeHighlighting(editor, tokenizer) {
  if (!editor.hasNodes([CodeNode2, CodeHighlightNode2])) {
    throw new Error("CodeHighlightPlugin: CodeNode or CodeHighlightNode not registered on editor");
  }
  if (tokenizer == null) {
    tokenizer = PrismTokenizer;
  }
  const registrations = [];
  if (editor._headless !== true) {
    registrations.push(editor.registerMutationListener(CodeNode2, (mutations) => {
      editor.getEditorState().read(() => {
        for (const [key, type] of mutations) {
          if (type !== "destroyed") {
            const node = $getNodeByKey2(key);
            if (node !== null) {
              updateCodeGutter(node, editor);
            }
          }
        }
      });
    }, {
      skipInitialization: false
    }));
  }
  const transformState = {
    didTransform: false,
    nodesCurrentlyHighlighting: /* @__PURE__ */ new Set()
  };
  registrations.push(editor.registerNodeTransform(CodeNode2, $codeNodeTransform.bind(null, editor, tokenizer, transformState)), editor.registerNodeTransform(TextNode2, $textNodeTransform.bind(null, editor, tokenizer, transformState)), editor.registerNodeTransform(CodeHighlightNode2, $textNodeTransform.bind(null, editor, tokenizer, transformState)), editor.registerCommand(KEY_TAB_COMMAND2, (event) => {
    const command = $handleTab(event.shiftKey);
    if (command === null) {
      return false;
    }
    event.preventDefault();
    editor.dispatchCommand(command, void 0);
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(INSERT_TAB_COMMAND2, () => {
    const selection = $getSelection2();
    if (!$isSelectionInCode(selection)) {
      return false;
    }
    $insertNodes2([$createTabNode2()]);
    return true;
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(INDENT_CONTENT_COMMAND2, (payload) => $handleMultilineIndent(INDENT_CONTENT_COMMAND2), COMMAND_PRIORITY_LOW2), editor.registerCommand(OUTDENT_CONTENT_COMMAND2, (payload) => $handleMultilineIndent(OUTDENT_CONTENT_COMMAND2), COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ARROW_UP_COMMAND2, (event) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection) || !$isSelectionInCode(selection)) {
      return false;
    }
    const firstNode = $getRoot2().getFirstDescendant();
    const {
      anchor
    } = selection;
    const anchorNode = anchor.getNode();
    if (firstNode && anchorNode && firstNode.getKey() === anchorNode.getKey()) {
      return false;
    }
    return $handleShiftLines(KEY_ARROW_UP_COMMAND2, event);
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(KEY_ARROW_DOWN_COMMAND2, (event) => {
    const selection = $getSelection2();
    if (!$isRangeSelection2(selection) || !$isSelectionInCode(selection)) {
      return false;
    }
    const lastNode = $getRoot2().getLastDescendant();
    const {
      anchor
    } = selection;
    const anchorNode = anchor.getNode();
    if (lastNode && anchorNode && lastNode.getKey() === anchorNode.getKey()) {
      return false;
    }
    return $handleShiftLines(KEY_ARROW_DOWN_COMMAND2, event);
  }, COMMAND_PRIORITY_LOW2), editor.registerCommand(MOVE_TO_START2, (event) => $handleMoveTo(MOVE_TO_START2, event), COMMAND_PRIORITY_LOW2), editor.registerCommand(MOVE_TO_END2, (event) => $handleMoveTo(MOVE_TO_END2, event), COMMAND_PRIORITY_LOW2));
  return mergeRegister2(...registrations);
}
var CodePrismExtension = defineExtension2({
  build: (editor, config) => namedSignals2(config),
  config: safeCast2({
    disabled: false,
    tokenizer: PrismTokenizer
  }),
  dependencies: [CodeExtension2],
  name: "@lexical/code-prism",
  register: (editor, config, state) => {
    const stores = state.getOutput();
    return effect(() => {
      if (stores.disabled.value) {
        return;
      }
      return registerCodeHighlighting(editor, stores.tokenizer.value);
    });
  }
});

// node_modules/@lexical/code-prism/LexicalCodePrism.prod.mjs
var import_prismjs2 = __toESM(require_prism(), 1);
function J(e2, ...t2) {
  const n2 = new URL("https://lexical.dev/docs/error"), r2 = new URLSearchParams();
  r2.append("code", e2);
  for (const e3 of t2) r2.append("v", e3);
  throw n2.search = r2.toString(), Error(`Minified Lexical error #${e2}; visit ${n2.toString()} for the full message or use the non-minified dev environment for full errors and additional helpful warnings.`);
}
!(function(e2) {
  e2.languages.diff = { coord: [/^(?:\*{3}|-{3}|\+{3}).*$/m, /^@@.*@@$/m, /^\d.*$/m] };
  var t2 = { "deleted-sign": "-", "deleted-arrow": "<", "inserted-sign": "+", "inserted-arrow": ">", unchanged: " ", diff: "!" };
  Object.keys(t2).forEach(function(n2) {
    var r2 = t2[n2], o2 = [];
    /^\w+$/.test(n2) || o2.push(/\w+/.exec(n2)[0]), "diff" === n2 && o2.push("bold"), e2.languages.diff[n2] = { pattern: RegExp("^(?:[" + r2 + "].*(?:\r\n?|\n|(?![\\s\\S])))+", "m"), alias: o2, inside: { line: { pattern: /(.)(?=[\s\S]).*(?:\r\n?|\n)?/, lookbehind: true }, prefix: { pattern: /[\s\S]/, alias: /\w+/.exec(n2)[0] } } };
  }), Object.defineProperty(e2.languages.diff, "PREFIXES", { value: t2 });
})(Prism);
var U = globalThis.Prism || window.Prism;
function te(e2) {
  const t2 = (function(e3) {
    const t3 = /^diff-([\w-]+)/i.exec(e3);
    return t3 ? t3[1] : null;
  })(e2), n2 = t2 || e2;
  try {
    return !!n2 && U.languages.hasOwnProperty(n2);
  } catch (e3) {
    return false;
  }
}
async function ne(e2, t2, n2) {
}
function re(e2) {
  return "string" == typeof e2 ? e2 : Array.isArray(e2) ? e2.map(re).join("") : re(e2.content);
}
function oe(e2, t2) {
  const n2 = /^diff-([\w-]+)/i.exec(t2), r2 = e2.getTextContent();
  let o2 = U.tokenize(r2, U.languages[n2 ? "diff" : t2]);
  return n2 && (o2 = (function(e3, t3) {
    const n3 = t3, r3 = U.languages[n3], o3 = { tokens: e3 }, s2 = U.languages.diff.PREFIXES;
    for (const e4 of o3.tokens) {
      if ("string" == typeof e4 || !(e4.type in s2) || !Array.isArray(e4.content)) continue;
      const t4 = e4.type;
      let n4 = 0;
      const o4 = () => (n4++, new U.Token("prefix", s2[t4], t4.replace(/^(\w+).*/, "$1"))), i2 = e4.content.filter((e5) => "string" == typeof e5 || "prefix" !== e5.type), c2 = e4.content.length - i2.length, l2 = U.tokenize(re(i2), r3);
      l2.unshift(o4());
      const a2 = /\r\n|\n/g, f = (e5) => {
        const t5 = [];
        a2.lastIndex = 0;
        let r4, s3 = 0;
        for (; n4 < c2 && (r4 = a2.exec(e5)); ) {
          const n5 = r4.index + r4[0].length;
          t5.push(e5.slice(s3, n5)), s3 = n5, t5.push(o4());
        }
        if (0 !== t5.length) return s3 < e5.length && t5.push(e5.slice(s3)), t5;
      }, u2 = (e5) => {
        for (let t5 = 0; t5 < e5.length && n4 < c2; t5++) {
          const n5 = e5[t5];
          if ("string" == typeof n5) {
            const r4 = f(n5);
            r4 && (e5.splice(t5, 1, ...r4), t5 += r4.length - 1);
          } else if ("string" == typeof n5.content) {
            const e6 = f(n5.content);
            e6 && (n5.content = e6);
          } else Array.isArray(n5.content) ? u2(n5.content) : u2([n5.content]);
        }
      };
      u2(l2), n4 < c2 && l2.push(o4()), e4.content = l2;
    }
    return o3.tokens;
  })(o2, n2[1])), se(o2);
}
function se(t2, n2) {
  const r2 = [];
  for (const o2 of t2) if ("string" == typeof o2) {
    const t3 = o2.split(/(\n|\t)/), s2 = t3.length;
    for (let o3 = 0; o3 < s2; o3++) {
      const s3 = t3[o3];
      "\n" === s3 || "\r\n" === s3 ? r2.push($createLineBreakNode2()) : "	" === s3 ? r2.push($createTabNode2()) : s3.length > 0 && r2.push($createCodeHighlightNode2(s3, n2));
    }
  } else {
    const { content: e2, alias: t3 } = o2;
    "string" == typeof e2 ? r2.push(...se([e2], "prefix" === o2.type && "string" == typeof t3 ? t3 : o2.type)) : Array.isArray(e2) && r2.push(...se(e2, "unchanged" === o2.type ? void 0 : o2.type));
  }
  return r2;
}
var ie = { $tokenize(e2, t2) {
  return oe(e2, t2 || this.defaultLanguage);
}, defaultLanguage: DEFAULT_CODE_LANGUAGE2, tokenize(e2, t2) {
  return U.tokenize(e2, U.languages[t2 || ""] || U.languages[this.defaultLanguage]);
} };
function ce(e2, t2, o2, s2) {
  const i2 = s2.getParent();
  $isCodeNode2(i2) ? ae(e2, t2, o2, i2) : $isCodeHighlightNode2(s2) && s2.replace($createTextNode2(s2.__text));
}
function le(e2, t2) {
  const n2 = t2.getElementByKey(e2.getKey());
  if (null === n2) return;
  const r2 = e2.getChildren(), o2 = r2.length;
  if (o2 === n2.__cachedChildrenLength) return;
  n2.__cachedChildrenLength = o2;
  let s2 = "1", i2 = 1;
  for (let e3 = 0; e3 < o2; e3++) $isLineBreakNode2(r2[e3]) && (s2 += "\n" + ++i2);
  n2.setAttribute("data-gutter", s2);
}
function ae(e2, t2, r2, o2) {
  const { nodesCurrentlyHighlighting: s2 } = r2, i2 = o2.getKey();
  void 0 === o2.getLanguage() && o2.setLanguage(t2.defaultLanguage);
  const c2 = o2.getLanguage() || t2.defaultLanguage;
  if (!te(c2)) return o2.getIsSyntaxHighlightSupported() && o2.setIsSyntaxHighlightSupported(false), void ne();
  o2.getIsSyntaxHighlightSupported() || o2.setIsSyntaxHighlightSupported(true), s2.has(i2) || (s2.add(i2), r2.didTransform || (r2.didTransform = true, $onUpdate2(() => {
    r2.didTransform = false, s2.clear();
  })), (function(e3, t3) {
    const r3 = $getNodeByKey2(e3);
    if (!$isCodeNode2(r3) || !r3.isAttached()) return;
    const o3 = $getSelection2();
    if (!$isRangeSelection2(o3)) return void t3();
    const s3 = o3.anchor, i3 = s3.offset, c3 = "element" === s3.type && $isLineBreakNode2(r3.getChildAtIndex(s3.offset - 1));
    let l2 = 0;
    if (!c3) {
      const e4 = s3.getNode();
      l2 = i3 + e4.getPreviousSiblings().reduce((e5, t4) => e5 + t4.getTextContentSize(), 0);
    }
    if (!t3()) return;
    if (c3) return void s3.getNode().select(i3, i3);
    r3.getChildren().some((e4) => {
      const t4 = $isTextNode2(e4);
      if (t4 || $isLineBreakNode2(e4)) {
        const n2 = e4.getTextContentSize();
        if (t4 && n2 >= l2) return e4.select(l2, l2), true;
        l2 -= n2;
      }
      return false;
    });
  })(i2, () => {
    const e3 = $getNodeByKey2(i2);
    if (!$isCodeNode2(e3) || !e3.isAttached()) return false;
    const r3 = e3.getLanguage() || t2.defaultLanguage, s3 = t2.$tokenize(e3, r3), c3 = (function(e4, t3) {
      let n2 = 0;
      for (; n2 < e4.length && fe(e4[n2], t3[n2]); ) n2++;
      const r4 = e4.length, o3 = t3.length, s4 = Math.min(r4, o3) - n2;
      let i3 = 0;
      for (; i3 < s4; ) if (i3++, !fe(e4[r4 - i3], t3[o3 - i3])) {
        i3--;
        break;
      }
      const c4 = n2, l3 = r4 - i3, a3 = t3.slice(n2, o3 - i3);
      return { from: c4, nodesForReplacement: a3, to: l3 };
    })(e3.getChildren(), s3), { from: l2, to: a2, nodesForReplacement: f } = c3;
    return !(l2 === a2 && !f.length) && (o2.splice(l2, a2 - l2, f), true);
  }));
}
function fe(e2, t2) {
  return $isCodeHighlightNode2(e2) && $isCodeHighlightNode2(t2) && e2.__text === t2.__text && e2.__highlightType === t2.__highlightType || $isTabNode2(e2) && $isTabNode2(t2) || $isLineBreakNode2(e2) && $isLineBreakNode2(t2);
}
function ue(e2) {
  if (!$isRangeSelection2(e2)) return false;
  const t2 = e2.anchor.getNode(), r2 = $isCodeNode2(t2) ? t2 : t2.getParent(), o2 = e2.focus.getNode(), s2 = $isCodeNode2(o2) ? o2 : o2.getParent();
  return $isCodeNode2(r2) && r2.is(s2);
}
function ge(e2) {
  const t2 = e2.getNodes(), o2 = [];
  if (1 === t2.length && $isCodeNode2(t2[0])) return o2;
  let s2 = [];
  for (let e3 = 0; e3 < t2.length; e3++) {
    const n2 = t2[e3];
    $isCodeHighlightNode2(n2) || $isTabNode2(n2) || $isLineBreakNode2(n2) || J(169), $isLineBreakNode2(n2) ? s2.length > 0 && (o2.push(s2), s2 = []) : s2.push(n2);
  }
  if (s2.length > 0) {
    const t3 = e2.isBackward() ? e2.anchor : e2.focus, n2 = $createPoint2(s2[0].getKey(), 0, "text");
    t3.is(n2) || o2.push(s2);
  }
  return o2;
}
function pe(e2) {
  const t2 = $getSelection2();
  if (!$isRangeSelection2(t2) || !ue(t2)) return false;
  const n2 = ge(t2), r2 = n2.length;
  if (0 === r2 && t2.isCollapsed()) return e2 === INDENT_CONTENT_COMMAND2 && t2.insertNodes([$createTabNode2()]), true;
  if (0 === r2 && e2 === INDENT_CONTENT_COMMAND2 && "\n" === t2.getTextContent()) {
    const e3 = $createTabNode2(), n3 = $createLineBreakNode2(), r3 = t2.isBackward() ? "previous" : "next";
    return t2.insertNodes([e3, n3]), $setSelectionFromCaretRange2($getCaretRangeInDirection2($getCaretRange2($getTextPointCaret2(e3, "next", 0), $normalizeCaret2($getSiblingCaret2(n3, "next"))), r3)), true;
  }
  for (let s2 = 0; s2 < r2; s2++) {
    const r3 = n2[s2];
    if (r3.length > 0) {
      let n3 = r3[0];
      if (0 === s2 && (n3 = $getFirstCodeNodeOfLine2(n3)), e2 === INDENT_CONTENT_COMMAND2) {
        const e3 = $createTabNode2();
        if (n3.insertBefore(e3), 0 === s2) {
          const r4 = t2.isBackward() ? "focus" : "anchor", o2 = $createPoint2(n3.getKey(), 0, "text");
          t2[r4].is(o2) && t2[r4].set(e3.getKey(), 0, "text");
        }
      } else $isTabNode2(n3) && n3.remove();
    }
  }
  return true;
}
function de(e2, t2) {
  const n2 = $getSelection2();
  if (!$isRangeSelection2(n2)) return false;
  const { anchor: i2, focus: c2 } = n2, l2 = i2.offset, a2 = c2.offset, f = i2.getNode(), u2 = c2.getNode(), g2 = e2 === KEY_ARROW_UP_COMMAND2;
  if (!ue(n2) || !$isCodeHighlightNode2(f) && !$isTabNode2(f) || !$isCodeHighlightNode2(u2) && !$isTabNode2(u2)) return false;
  if (!t2.altKey) {
    if (n2.isCollapsed()) {
      const e3 = f.getParentOrThrow();
      if (g2 && 0 === l2 && null === f.getPreviousSibling()) {
        if (null === e3.getPreviousSibling()) return e3.selectPrevious(), t2.preventDefault(), true;
      } else if (!g2 && l2 === f.getTextContentSize() && null === f.getNextSibling()) {
        if (null === e3.getNextSibling()) return e3.selectNext(), t2.preventDefault(), true;
      }
    }
    return false;
  }
  let p2, d2;
  if (f.isBefore(u2) ? (p2 = $getFirstCodeNodeOfLine2(f), d2 = $getLastCodeNodeOfLine2(u2)) : (p2 = $getFirstCodeNodeOfLine2(u2), d2 = $getLastCodeNodeOfLine2(f)), null == p2 || null == d2) return false;
  const m2 = p2.getNodesBetween(d2);
  for (let e3 = 0; e3 < m2.length; e3++) {
    const t3 = m2[e3];
    if (!$isCodeHighlightNode2(t3) && !$isTabNode2(t3) && !$isLineBreakNode2(t3)) return false;
  }
  t2.preventDefault(), t2.stopPropagation();
  const h2 = g2 ? p2.getPreviousSibling() : d2.getNextSibling();
  if (!$isLineBreakNode2(h2)) return true;
  const x2 = g2 ? h2.getPreviousSibling() : h2.getNextSibling();
  if (null == x2) return true;
  const j2 = $isCodeHighlightNode2(x2) || $isTabNode2(x2) || $isLineBreakNode2(x2) ? g2 ? $getFirstCodeNodeOfLine2(x2) : $getLastCodeNodeOfLine2(x2) : null;
  let w2 = null != j2 ? j2 : x2;
  return h2.remove(), m2.forEach((e3) => e3.remove()), e2 === KEY_ARROW_UP_COMMAND2 ? (m2.forEach((e3) => w2.insertBefore(e3)), w2.insertBefore(h2)) : (w2.insertAfter(h2), w2 = h2, m2.forEach((e3) => {
    w2.insertAfter(e3), w2 = e3;
  })), n2.setTextNodeRange(f, l2, u2, a2), true;
}
function me(e2, t2) {
  const n2 = $getSelection2();
  if (!$isRangeSelection2(n2)) return false;
  const { anchor: o2, focus: s2 } = n2, a2 = o2.getNode(), f = s2.getNode(), u2 = e2 === MOVE_TO_START2;
  if (!ue(n2) || !$isCodeHighlightNode2(a2) && !$isTabNode2(a2) || !$isCodeHighlightNode2(f) && !$isTabNode2(f)) return false;
  const g2 = f;
  if ("rtl" === $getCodeLineDirection2(g2) ? !u2 : u2) {
    const e3 = $getStartOfCodeInLine2(g2, s2.offset);
    if (null !== e3) {
      const { node: t3, offset: r2 } = e3;
      $isLineBreakNode2(t3) ? t3.selectNext(0, 0) : n2.setTextNodeRange(t3, r2, t3, r2);
    } else g2.getParentOrThrow().selectStart();
  } else {
    $getEndOfCodeInLine2(g2).select();
  }
  return t2.preventDefault(), t2.stopPropagation(), true;
}
function he(e2, t2) {
  if (!e2.hasNodes([CodeNode2, CodeHighlightNode2])) throw new Error("CodeHighlightPlugin: CodeNode or CodeHighlightNode not registered on editor");
  null == t2 && (t2 = ie);
  const n2 = [];
  true !== e2._headless && n2.push(e2.registerMutationListener(CodeNode2, (t3) => {
    e2.getEditorState().read(() => {
      for (const [n3, r3] of t3) if ("destroyed" !== r3) {
        const t4 = $getNodeByKey2(n3);
        null !== t4 && le(t4, e2);
      }
    });
  }, { skipInitialization: false }));
  const r2 = { didTransform: false, nodesCurrentlyHighlighting: /* @__PURE__ */ new Set() };
  return n2.push(e2.registerNodeTransform(CodeNode2, ae.bind(null, e2, t2, r2)), e2.registerNodeTransform(TextNode2, ce.bind(null, e2, t2, r2)), e2.registerNodeTransform(CodeHighlightNode2, ce.bind(null, e2, t2, r2)), e2.registerCommand(KEY_TAB_COMMAND2, (t3) => {
    const n3 = (function(e3) {
      const t4 = $getSelection2();
      if (!$isRangeSelection2(t4) || !ue(t4)) return null;
      const n4 = e3 ? OUTDENT_CONTENT_COMMAND2 : INDENT_CONTENT_COMMAND2, r3 = e3 ? OUTDENT_CONTENT_COMMAND2 : INSERT_TAB_COMMAND2, i2 = t4.anchor, c2 = t4.focus;
      if (i2.is(c2)) return r3;
      const l2 = ge(t4);
      if (1 !== l2.length) return n4;
      const a2 = l2[0];
      let f, u2;
      0 === a2.length && J(285), t4.isBackward() ? (f = c2, u2 = i2) : (f = i2, u2 = c2);
      const g2 = $getFirstCodeNodeOfLine2(a2[0]), p2 = $getLastCodeNodeOfLine2(a2[0]), d2 = $createPoint2(g2.getKey(), 0, "text"), m2 = $createPoint2(p2.getKey(), p2.getTextContentSize(), "text");
      return f.isBefore(d2) || m2.isBefore(u2) ? n4 : d2.isBefore(f) || u2.isBefore(m2) ? r3 : n4;
    })(t3.shiftKey);
    return null !== n3 && (t3.preventDefault(), e2.dispatchCommand(n3, void 0), true);
  }, COMMAND_PRIORITY_LOW2), e2.registerCommand(INSERT_TAB_COMMAND2, () => !!ue($getSelection2()) && ($insertNodes2([$createTabNode2()]), true), COMMAND_PRIORITY_LOW2), e2.registerCommand(INDENT_CONTENT_COMMAND2, (e3) => pe(INDENT_CONTENT_COMMAND2), COMMAND_PRIORITY_LOW2), e2.registerCommand(OUTDENT_CONTENT_COMMAND2, (e3) => pe(OUTDENT_CONTENT_COMMAND2), COMMAND_PRIORITY_LOW2), e2.registerCommand(KEY_ARROW_UP_COMMAND2, (e3) => {
    const t3 = $getSelection2();
    if (!$isRangeSelection2(t3) || !ue(t3)) return false;
    const n3 = $getRoot2().getFirstDescendant(), { anchor: r3 } = t3, o2 = r3.getNode();
    return (!n3 || !o2 || n3.getKey() !== o2.getKey()) && de(KEY_ARROW_UP_COMMAND2, e3);
  }, COMMAND_PRIORITY_LOW2), e2.registerCommand(KEY_ARROW_DOWN_COMMAND2, (e3) => {
    const t3 = $getSelection2();
    if (!$isRangeSelection2(t3) || !ue(t3)) return false;
    const n3 = $getRoot2().getLastDescendant(), { anchor: r3 } = t3, o2 = r3.getNode();
    return (!n3 || !o2 || n3.getKey() !== o2.getKey()) && de(KEY_ARROW_DOWN_COMMAND2, e3);
  }, COMMAND_PRIORITY_LOW2), e2.registerCommand(MOVE_TO_START2, (e3) => me(MOVE_TO_START2, e3), COMMAND_PRIORITY_LOW2), e2.registerCommand(MOVE_TO_END2, (e3) => me(MOVE_TO_END2, e3), COMMAND_PRIORITY_LOW2)), mergeRegister2(...n2);
}
var ye = defineExtension2({ build: (e2, t2) => namedSignals2(t2), config: safeCast2({ disabled: false, tokenizer: ie }), dependencies: [CodeExtension2], name: "@lexical/code-prism", register: (e2, t2, n2) => {
  const r2 = n2.getOutput();
  return effect(() => {
    if (!r2.disabled.value) return he(e2, r2.tokenizer.value);
  });
} });

// node_modules/@lexical/code-prism/LexicalCodePrism.mjs
var mod12 = true ? LexicalCodePrism_dev_exports : LexicalCodePrism_prod_exports;
var CODE_LANGUAGE_FRIENDLY_NAME_MAP2 = mod12.CODE_LANGUAGE_FRIENDLY_NAME_MAP;
var CODE_LANGUAGE_MAP2 = mod12.CODE_LANGUAGE_MAP;
var CodePrismExtension2 = mod12.CodePrismExtension;
var PrismTokenizer2 = mod12.PrismTokenizer;
var getCodeLanguageOptions2 = mod12.getCodeLanguageOptions;
var getCodeLanguages2 = mod12.getCodeLanguages;
var getCodeThemeOptions2 = mod12.getCodeThemeOptions;
var getLanguageFriendlyName2 = mod12.getLanguageFriendlyName;
var isCodeLanguageLoaded2 = mod12.isCodeLanguageLoaded;
var loadCodeLanguage2 = mod12.loadCodeLanguage;
var normalizeCodeLanguage2 = mod12.normalizeCodeLanguage;
var registerCodeHighlighting2 = mod12.registerCodeHighlighting;

// node_modules/@lexical/code/LexicalCode.dev.mjs
var CODE_LANGUAGE_FRIENDLY_NAME_MAP3 = CODE_LANGUAGE_FRIENDLY_NAME_MAP2;
var CODE_LANGUAGE_MAP3 = CODE_LANGUAGE_MAP2;
var getCodeLanguageOptions3 = getCodeLanguageOptions2;
var getCodeLanguages3 = getCodeLanguages2;
var getCodeThemeOptions3 = getCodeThemeOptions2;
var getLanguageFriendlyName3 = getLanguageFriendlyName2;
var normalizeCodeLang = normalizeCodeLanguage2;
var normalizeCodeLanguage3 = normalizeCodeLanguage2;
var PrismTokenizer3 = PrismTokenizer2;
var registerCodeHighlighting3 = registerCodeHighlighting2;

// node_modules/@lexical/code/LexicalCode.mjs
var mod13 = true ? LexicalCode_dev_exports : LexicalCode_prod_exports;
var $createCodeHighlightNode3 = mod13.$createCodeHighlightNode;
var $createCodeNode3 = mod13.$createCodeNode;
var $getCodeLineDirection3 = mod13.$getCodeLineDirection;
var $getEndOfCodeInLine3 = mod13.$getEndOfCodeInLine;
var $getFirstCodeNodeOfLine3 = mod13.$getFirstCodeNodeOfLine;
var $getLastCodeNodeOfLine3 = mod13.$getLastCodeNodeOfLine;
var $getStartOfCodeInLine3 = mod13.$getStartOfCodeInLine;
var $isCodeHighlightNode3 = mod13.$isCodeHighlightNode;
var $isCodeNode3 = mod13.$isCodeNode;
var CODE_LANGUAGE_FRIENDLY_NAME_MAP4 = mod13.CODE_LANGUAGE_FRIENDLY_NAME_MAP;
var CODE_LANGUAGE_MAP4 = mod13.CODE_LANGUAGE_MAP;
var CodeExtension3 = mod13.CodeExtension;
var CodeHighlightNode3 = mod13.CodeHighlightNode;
var CodeNode3 = mod13.CodeNode;
var DEFAULT_CODE_LANGUAGE3 = mod13.DEFAULT_CODE_LANGUAGE;
var PrismTokenizer4 = mod13.PrismTokenizer;
var getCodeLanguageOptions4 = mod13.getCodeLanguageOptions;
var getCodeLanguages4 = mod13.getCodeLanguages;
var getCodeThemeOptions4 = mod13.getCodeThemeOptions;
var getDefaultCodeLanguage3 = mod13.getDefaultCodeLanguage;
var getLanguageFriendlyName4 = mod13.getLanguageFriendlyName;
var normalizeCodeLang2 = mod13.normalizeCodeLang;
var normalizeCodeLanguage4 = mod13.normalizeCodeLanguage;
var registerCodeHighlighting4 = mod13.registerCodeHighlighting;
export {
  $createHeadingNode2 as $createHeadingNode,
  $createParagraphNode2 as $createParagraphNode,
  $createQuoteNode2 as $createQuoteNode,
  $createTextNode2 as $createTextNode,
  $generateHtmlFromNodes2 as $generateHtmlFromNodes,
  $generateNodesFromDOM2 as $generateNodesFromDOM,
  $getRoot2 as $getRoot,
  $getSelection2 as $getSelection,
  $isElementNode2 as $isElementNode,
  $isHeadingNode2 as $isHeadingNode,
  $isListItemNode2 as $isListItemNode,
  $isListNode2 as $isListNode,
  $isQuoteNode2 as $isQuoteNode,
  $isRangeSelection2 as $isRangeSelection,
  $patchStyleText2 as $patchStyleText,
  $setBlocksType2 as $setBlocksType,
  AutoLinkNode2 as AutoLinkNode,
  CAN_REDO_COMMAND2 as CAN_REDO_COMMAND,
  CAN_UNDO_COMMAND2 as CAN_UNDO_COMMAND,
  COMMAND_PRIORITY_LOW2 as COMMAND_PRIORITY_LOW,
  CodeNode3 as CodeNode,
  FORMAT_ELEMENT_COMMAND2 as FORMAT_ELEMENT_COMMAND,
  FORMAT_TEXT_COMMAND2 as FORMAT_TEXT_COMMAND,
  HeadingNode2 as HeadingNode,
  LinkNode2 as LinkNode,
  ListItemNode2 as ListItemNode,
  ListNode2 as ListNode,
  QuoteNode2 as QuoteNode,
  REDO_COMMAND2 as REDO_COMMAND,
  SELECTION_CHANGE_COMMAND2 as SELECTION_CHANGE_COMMAND,
  UNDO_COMMAND2 as UNDO_COMMAND,
  createEditor2 as createEditor,
  mergeRegister3 as mergeRegister,
  registerRichText
};
/*! Bundled license information:

prismjs/prism.js:
  (**
   * Prism: Lightweight, robust, elegant syntax highlighting
   *
   * @license MIT <https://opensource.org/licenses/MIT>
   * @author Lea Verou <https://lea.verou.me>
   * @namespace
   * @public
   *)
*/
