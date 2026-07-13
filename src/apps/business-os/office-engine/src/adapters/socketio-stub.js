/* The CTOX product forks deliberately exclude the inherited coauthoring transport. */
(function (global) {
  'use strict';
  const unavailable = function () {
    throw new Error('Coauthoring socket transport is disabled in the CTOX product fork');
  };
  unavailable.connect = unavailable;
  global.io = unavailable;
  if (typeof global.define === 'function' && global.define.amd) global.define('socketio', [], function () { return unavailable; });
})(window);
