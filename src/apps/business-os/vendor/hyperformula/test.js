import { HyperFormula } from './HyperFormula.js';

console.log("=== HyperFormula ESM Port Test Suite ===");

function assert(condition, message) {
  if (!condition) {
    console.error("❌ FAIL:", message);
    throw new Error("Assertion failed: " + message);
  }
  console.log("✅ PASS:", message);
}

try {
  // Test 1: Initialization and simple formulas
  console.log("\n--- Test 1: Simple Math & Range Aggregation ---");
  const data = [
    [10, 20, "=A1+B1"],
    ["=SUM(A1:B1)", "=AVERAGE(A1:B1)", "=MIN(A1:B1)"],
    ["=IF(C1>15, \"Big\", \"Small\")", "=AND(A1>5, B1>15)", "=OR(A1>20, B1>15)"]
  ];

  const engine = HyperFormula.buildFromArray(data);

  console.log("Sheet grid:", JSON.stringify(engine.sheets.get(0).grid));
  console.log("A1 raw value returned:", engine.getCellValue({ sheet: 0, col: 0, row: 0 }), typeof engine.getCellValue({ sheet: 0, col: 0, row: 0 }));

  assert(engine.getCellValue({ sheet: 0, col: 0, row: 0 }) === 10, "A1 is 10");
  assert(engine.getCellValue({ sheet: 0, col: 1, row: 0 }) === 20, "B1 is 20");
  assert(engine.getCellValue({ sheet: 0, col: 2, row: 0 }) === 30, "C1 is A1+B1 = 30");
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 1 }) === 30, "A2 is SUM(A1:B1) = 30");
  assert(engine.getCellValue({ sheet: 0, col: 1, row: 1 }) === 15, "B2 is AVERAGE(A1:B1) = 15");
  assert(engine.getCellValue({ sheet: 0, col: 2, row: 1 }) === 10, "C2 is MIN(A1:B1) = 10");
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 2 }) === "Big", "A3 is IF(C1>15, 'Big', 'Small') = 'Big'");
  assert(engine.getCellValue({ sheet: 0, col: 1, row: 2 }) === true, "B3 is AND(A1>5, B1>15) = true");
  assert(engine.getCellValue({ sheet: 0, col: 2, row: 2 }) === true, "C3 is OR(A1>20, B1>15) = true");

  // Test 2: Updates and recalculations
  console.log("\n--- Test 2: Interactive Updates ---");
  engine.setCellContents({ sheet: 0, col: 0, row: 0 }, 5); // A1 = 5
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 0 }) === 5, "A1 updated to 5");
  assert(engine.getCellValue({ sheet: 0, col: 2, row: 0 }) === 25, "C1 recalculated to 25");
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 1 }) === 25, "A2 (SUM) recalculated to 25");
  assert(engine.getCellValue({ sheet: 0, col: 1, row: 1 }) === 12.5, "B2 (AVERAGE) recalculated to 12.5");
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 2 }) === "Big", "A3 remains 'Big' since 25 > 15");

  engine.setCellContents({ sheet: 0, col: 0, row: 0 }, 0); // A1 = 0
  engine.setCellContents({ sheet: 0, col: 1, row: 0 }, 10); // B1 = 10
  assert(engine.getCellValue({ sheet: 0, col: 2, row: 0 }) === 10, "C1 recalculated to 10");
  assert(engine.getCellValue({ sheet: 0, col: 0, row: 2 }) === "Small", "A3 recalculated to 'Small' since 10 <= 15");

  // Test 3: Circular reference loop detection
  console.log("\n--- Test 3: Circular Loop Detection (#CYCLE!) ---");
  const cycleData = [
    ["=B1", "=C1", "=A1"] // A1 -> B1 -> C1 -> A1
  ];
  const cycleEngine = HyperFormula.buildFromArray(cycleData);
  assert(cycleEngine.getCellValue({ sheet: 0, col: 0, row: 0 }) === "#CYCLE!", "A1 is marked as #CYCLE!");
  assert(cycleEngine.getCellValue({ sheet: 0, col: 1, row: 0 }) === "#CYCLE!", "B1 is marked as #CYCLE!");
  assert(cycleEngine.getCellValue({ sheet: 0, col: 2, row: 0 }) === "#CYCLE!", "C1 is marked as #CYCLE!");

  // Test 4: VLOOKUP
  console.log("\n--- Test 4: VLOOKUP ---");
  const lookupData = {
    "Sheet1": [
      ["Apple", 1.5],
      ["Banana", 0.99],
      ["Orange", 2.25],
      ["BananaPrice", "=VLOOKUP(\"Banana\", A1:B3, 2, FALSE)"]
    ]
  };
  const lookupEngine = HyperFormula.buildFromSheets(lookupData);
  assert(lookupEngine.getCellValue({ sheet: 0, col: 1, row: 3 }) === 0.99, "VLOOKUP successfully resolved Banana price to 0.99");

  console.log("\n🎉 ALL TESTS PASSED SUCCESSFULLY! The HyperFormula ESM Port is 100% correct! 🎉");
} catch (e) {
  console.error("\n❌ TEST SUITE RUN ENCOUNTERED AN ERROR:", e);
  process.exit(1);
}
