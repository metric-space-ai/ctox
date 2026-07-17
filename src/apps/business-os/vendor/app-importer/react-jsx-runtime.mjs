import * as __react from 'react';
const __reactM = __react.default ?? __react;
const require = (m) => { if (m === 'react') return __reactM; throw new Error('unresolved require: ' + m); };

var v=Object.create;var u=Object.defineProperty;var a=Object.getOwnPropertyDescriptor;var k=Object.getOwnPropertyNames;var T=Object.getPrototypeOf,c=Object.prototype.hasOwnProperty;var j=(e,r)=>()=>{try{return r||e((r={exports:{}}).exports,r),r.exports}catch(t){throw r=0,t}};var f=(e,r,t,o)=>{if(r&&typeof r=="object"||typeof r=="function")for(let s of k(r))!c.call(e,s)&&s!==t&&u(e,s,{get:()=>r[s],enumerable:!(o=a(r,s))||o.enumerable});return e};var m=(e,r,t)=>(t=e!=null?v(T(e)):{},f(r||!e||!e.__esModule?u(t,"default",{value:e,enumerable:!0}):t,e));var E=j(n=>{"use strict";var _=Symbol.for("react.transitional.element"),F=Symbol.for("react.fragment");function p(e,r,t){var o=null;if(t!==void 0&&(o=""+t),r.key!==void 0&&(o=""+r.key),"key"in r){t={};for(var s in r)s!=="key"&&(t[s]=r[s])}else t=r;return r=t.ref,{$$typeof:_,type:e,key:o,ref:r!==void 0?r:null,props:t}}n.Fragment=F;n.jsx=p;n.jsxs=p});var i=j((R,d)=>{"use strict";d.exports=E()});var x=m(i()),l=x.default??x,q=l,C=l.jsx,M=l.jsxs,N=l.Fragment;export{N as Fragment,q as default,C as jsx,M as jsxs};
/*! Bundled license information:

react/cjs/react-jsx-runtime.production.js:
  (**
   * @license React
   * react-jsx-runtime.production.js
   *
   * Copyright (c) Meta Platforms, Inc. and affiliates.
   *
   * This source code is licensed under the MIT license found in the
   * LICENSE file in the root directory of this source tree.
   *)
*/
