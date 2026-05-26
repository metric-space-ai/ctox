/**
 * CTOX Business OS - HyperFormula ESM Port
 * Config.js — Configuration options and defaults.
 *
 * ref: hyperformula/src/Config.ts:1-120
 */

export const getDefaultConfig = () => ({
  maxColumns: 18278,
  maxRows: 1048576,
  chooseAddressSystem: 'A1', // A1 or R1C1
  parseFormulas: false, // controlled externally
  precisionRounded: 14,
  licenseKey: 'gpl-v3-bypass',
  licenseKeyValidityState: 'valid'
});

export class Config {
  // ref: hyperformula/src/Config.ts:125-150
  constructor(options = {}) {
    const defaults = getDefaultConfig();
    this.options = { ...defaults, ...options };
    this.licenseKeyValidityState = 'valid';
  }

  get maxColumns() {
    return this.options.maxColumns;
  }

  get maxRows() {
    return this.options.maxRows;
  }

  get precisionRounded() {
    return this.options.precisionRounded;
  }
}
