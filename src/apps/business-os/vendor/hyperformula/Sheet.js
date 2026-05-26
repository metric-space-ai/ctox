/**
 * CTOX Business OS - HyperFormula ESM Port
 * Sheet.js — Two-dimensional grid data storage and representation.
 *
 * ref: hyperformula/src/Sheet.ts:1-150
 */

export class Sheet {
  // ref: hyperformula/src/Sheet.ts:155-180
  constructor(id, name, data = []) {
    this.id = id;
    this.name = name;
    this.grid = [];

    // Copy the 2D grid
    for (let r = 0; r < data.length; r++) {
      this.grid[r] = [];
      const rowData = data[r] || [];
      for (let c = 0; c < rowData.length; c++) {
        this.grid[r][c] = rowData[c] !== undefined ? rowData[c] : "";
      }
    }
  }

  // ref: hyperformula/src/Sheet.ts:210-230
  getCell(col, row) {
    if (this.grid[row] === undefined) return "";
    const val = this.grid[row][col];
    return val !== undefined ? val : "";
  }

  // ref: hyperformula/src/Sheet.ts:235-255
  setCell(col, row, value) {
    if (this.grid[row] === undefined) {
      // expand rows
      for (let i = this.grid.length; i <= row; i++) {
        this.grid[i] = [];
      }
    }
    this.grid[row][col] = value;
  }

  // ref: hyperformula/src/Sheet.ts:280-310
  getData() {
    return this.grid;
  }

  // ref: hyperformula/src/Sheet.ts:320-350
  getDimensions() {
    const rows = this.grid.length;
    let cols = 0;
    for (let r = 0; r < rows; r++) {
      if (this.grid[r] && this.grid[r].length > cols) {
        cols = this.grid[r].length;
      }
    }
    return { width: cols, height: rows };
  }
}
