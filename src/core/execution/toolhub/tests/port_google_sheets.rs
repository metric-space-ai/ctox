//! Recorded port tests for the Google Sheets toolkit (one case per tool).
//! Expectations are derived from the Activepieces `run()` code (Sheets API v4 +
//! the Drive `files` endpoints the piece uses); responses are realistic
//! Google API payloads.
mod support;

use ctox_toolhub::toolkits::google_sheets::GoogleSheets;
use ctox_toolhub::toolkits::ToolkitAdapter;
use ctox_toolhub::AuthScheme;
use serde_json::{json, Value};
use support::Case;


/// Leak the expectation arrays so a `Case<'static>` can own `json!` values.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn cases() -> Vec<Case<'static>> {
    vec![
        // ---------------- read / search ----------------
        // AP: src/lib/actions/sheets-get-spreadsheet.ts
        Case {
            tool: "GOOGLESHEETS_GET_SPREADSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "include_grid_data": false}),
            method: "GET",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms?includeGridData=false"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "properties": {"title": "Budget 2026", "locale": "en_US", "timeZone": "Europe/Berlin",
                    "autoRecalc": "ON_CHANGE", "defaultFormat": {}},
                "sheets": [
                    {"properties": {"sheetId": 0, "title": "Sheet1", "index": 0, "sheetType": "GRID",
                        "gridProperties": {"rowCount": 1000, "columnCount": 26}}},
                    {"properties": {"sheetId": 123456789, "title": "Expenses", "index": 1, "sheetType": "GRID",
                        "gridProperties": {"rowCount": 500, "columnCount": 20}}}
                ],
                "spreadsheetUrl": "https://docs.google.com/spreadsheets/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/edit"}),
            result_eq: ptrs([
                ("/spreadsheet_id", json!("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")),
                ("/title", json!("Budget 2026")),
                ("/locale", json!("en_US")),
                ("/worksheet_count", json!(2)),
                ("/worksheets/0/sheet_id", json!(0)),
                ("/worksheets/1/sheet_id", json!(123456789)),
                ("/worksheets/1/title", json!("Expenses")),
                ("/worksheets/1/row_count", json!(500)),
            ]),
        },
        // AP: src/lib/actions/sheets-find-worksheet.ts
        Case {
            tool: "GOOGLESHEETS_FIND_WORKSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "title": "Exp"}),
            method: "GET",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "properties": {"title": "Budget 2026"},
                "sheets": [
                    {"properties": {"sheetId": 0, "title": "Sheet1", "index": 0, "sheetType": "GRID",
                        "gridProperties": {"rowCount": 1000, "columnCount": 26}}},
                    {"properties": {"sheetId": 123456789, "title": "Expenses", "index": 1, "sheetType": "GRID",
                        "gridProperties": {"rowCount": 500, "columnCount": 20}}},
                    {"properties": {"sheetId": 987654321, "title": "Expenses 2025", "index": 2, "sheetType": "GRID",
                        "gridProperties": {"rowCount": 500, "columnCount": 20}}}
                ]}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/worksheets/0/properties/sheetId", json!(123456789)),
                ("/worksheets/0/properties/title", json!("Expenses")),
                ("/worksheets/1/properties/sheetId", json!(987654321)),
            ]),
        },
        // AP: src/lib/actions/sheets-search-spreadsheets.ts
        Case {
            tool: "GOOGLESHEETS_SEARCH_SPREADSHEETS",
            args: json!({"spreadsheet_name": "Q4 Budget"}),
            method: "GET",
            url_contains: &[
                "/drive/v3/files?",
                "q=mimeType%3D%27application%2Fvnd.google-apps.spreadsheet%27+and+trashed%3Dfalse+and+name+contains+%27Q4+Budget%27",
                "includeItemsFromAllDrives=false",
                "supportsAllDrives=true",
                "corpora=user",
                "fields=files%28id%2Cname%2CwebViewLink%2CcreatedTime%2CmodifiedTime%29%2CnextPageToken",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"kind": "drive#fileList", "files": [
                {"id": "1Q4BudgetAbc123", "name": "Q4 Budget 2026",
                 "webViewLink": "https://docs.google.com/spreadsheets/d/1Q4BudgetAbc123/edit",
                 "createdTime": "2026-09-30T08:15:00.000Z", "modifiedTime": "2026-10-01T11:02:00.000Z"}]}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/spreadsheets/0/id", json!("1Q4BudgetAbc123")),
                ("/spreadsheets/0/name", json!("Q4 Budget 2026")),
            ]),
        },
        // AP: src/lib/actions/sheets-get-values.ts
        Case {
            tool: "GOOGLESHEETS_GET_VALUES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Budget 2026", "range": "A1:D10",
                "major_dimension": "ROWS", "value_render_option": "FORMATTED_VALUE"}),
            method: "GET",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Budget%202026!A1%3AD10",
                "majorDimension=ROWS",
                "valueRenderOption=FORMATTED_VALUE",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"range": "Budget 2026!A1:D10", "majorDimension": "ROWS",
                "values": [["Month", "Amount"], ["Jan", "1200"], ["Feb", "900"]]}),
            result_eq: ptrs([
                ("/range", json!("Budget 2026!A1:D10")),
                ("/major_dimension", json!("ROWS")),
                ("/values/0/0", json!("Month")),
                ("/values/2/1", json!("900")),
            ]),
        },
        // AP: src/lib/actions/sheets-get-all-rows.ts via common.ts getGoogleSheetRows
        Case {
            tool: "GOOGLESHEETS_GET_ALL_ROWS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "first_row_headers": true}),
            method: "GET",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A2%3AZZZ"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"range": "Sheet 1!A2:ZZZ", "majorDimension": "ROWS",
                "values": [["Alice", "30", "Berlin"], ["Bob", "25"]]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/rows/0/row", json!(2)),
                ("/rows/0/values/A", json!("Alice")),
                ("/rows/0/values/B", json!("30")),
                ("/rows/0/values/C", json!("Berlin")),
                ("/rows/1/row", json!(3)),
                ("/rows/1/values/B", json!("25")),
                ("/rows/1/values/C", json!("")),
            ]),
        },
        // AP: src/lib/actions/sheets-get-row.ts via common.ts getGoogleSheetRows
        Case {
            tool: "GOOGLESHEETS_GET_ROW",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "row_number": 5}),
            method: "GET",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A5%3AZZZ5"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"range": "Sheet 1!A5:ZZZ5", "majorDimension": "ROWS",
                "values": [["Carol", "31", "Hamburg"]]}),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/row", json!(5)),
                ("/values/A", json!("Carol")),
                ("/values/C", json!("Hamburg")),
            ]),
        },
        // A row past the grid: Google answers 400 "exceeds grid limits" and the
        // piece maps that to {found: false, row: null} (isRowOutOfGridError).
        Case {
            tool: "GOOGLESHEETS_GET_ROW",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "row_number": 9999}),
            method: "GET",
            url_contains: &["/values/Sheet%201!A9999%3AZZZ9999"],
            body_eq: ptrs([]),
            response_status: 400,
            response: json!({"error": {"code": 400, "message": "'Sheet 1'!A9999:ZZZ9999 is out of grid limits",
                "status": "INVALID_ARGUMENT"}}),
            result_eq: ptrs([("/found", json!(false)), ("/row", Value::Null)]),
        },
        // AP: src/lib/actions/sheets-find-rows.ts via common.ts getGoogleSheetRows
        Case {
            tool: "GOOGLESHEETS_FIND_ROWS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Leads", "column_name": "B", "search_value": "acme",
                "starting_row": 2, "number_of_rows": 10}),
            method: "GET",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Leads!A2%3AZZZ"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"range": "Leads!A2:ZZZ", "majorDimension": "ROWS",
                "values": [["1", "Acme GmbH"], ["2", "Globex"], ["3", "ACME corp"], ["4", "Initech"]]}),
            result_eq: ptrs([
                ("/0/row", json!(2)),
                ("/0/values/B", json!("Acme GmbH")),
                ("/1/row", json!(4)),
                ("/1/values/B", json!("ACME corp")),
            ]),
        },
        // ---------------- write ----------------
        // AP: src/lib/actions/sheets-create-spreadsheet.ts (drive.files.create)
        Case {
            tool: "GOOGLESHEETS_CREATE_SPREADSHEET",
            args: json!({"spreadsheet_title": "Q1 Pipeline", "parent_folder_id": "fld9Z"}),
            method: "POST",
            url_contains: &["/drive/v3/files?supportsAllDrives=true"],
            body_eq: ptrs([
                ("/name", json!("Q1 Pipeline")),
                ("/mimeType", json!("application/vnd.google-apps.spreadsheet")),
                ("/parents/0", json!("fld9Z")),
            ]),
            response_status: 200,
            response: json!({"kind": "drive#file", "id": "1NewSheetXyz789", "name": "Q1 Pipeline",
                "mimeType": "application/vnd.google-apps.spreadsheet"}),
            result_eq: ptrs([("/id", json!("1NewSheetXyz789"))]),
        },
        // AP: src/lib/actions/sheets-add-worksheet.ts (batchUpdate addSheet)
        Case {
            tool: "GOOGLESHEETS_ADD_WORKSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "title": "Q3 Forecast"}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([("/requests/0/addSheet/properties/title", json!("Q3 Forecast"))]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "replies": [{"addSheet": {"properties": {"sheetId": 789012345, "title": "Q3 Forecast",
                    "index": 3, "sheetType": "GRID", "gridProperties": {"rowCount": 1000, "columnCount": 66}}}}]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/spreadsheet_id", json!("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")),
                ("/sheet_id", json!(789012345)),
            ]),
        },
        // AP: src/lib/actions/sheets-delete-worksheet.ts (batchUpdate deleteSheet)
        Case {
            tool: "GOOGLESHEETS_DELETE_WORKSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "sheet_id": 789012345}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([("/requests/0/deleteSheet/sheetId", json!(789012345))]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-rename-worksheet.ts (batchUpdate updateSheetProperties)
        Case {
            tool: "GOOGLESHEETS_RENAME_WORKSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 123456789, "new_name": "Expenses 2026"}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/updateSheetProperties/properties/sheetId", json!(123456789)),
                ("/requests/0/updateSheetProperties/properties/title", json!("Expenses 2026")),
                ("/requests/0/updateSheetProperties/fields", json!("title")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-copy-worksheet.ts (spreadsheets.sheets.copyTo)
        Case {
            tool: "GOOGLESHEETS_COPY_WORKSHEET",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "destination_spreadsheet_id": "1DestSheetAbc456"}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/sheets/0:copyTo"],
            body_eq: ptrs([("/destinationSpreadsheetId", json!("1DestSheetAbc456"))]),
            response_status: 200,
            response: json!({"sheetId": 1001, "title": "Copy of Sheet1", "index": 0, "sheetType": "GRID",
                "gridProperties": {"rowCount": 1000, "columnCount": 26}}),
            result_eq: ptrs([("/sheetId", json!(1001)), ("/title", json!("Copy of Sheet1"))]),
        },
        // AP: src/lib/actions/sheets-append-values.ts (values.append, INSERT_ROWS)
        Case {
            tool: "GOOGLESHEETS_APPEND_VALUES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Budget 2026", "range": "A1",
                "values": [["Mar", "1500"]], "value_input_option": "USER_ENTERED"}),
            method: "POST",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/'Budget%202026'!A1:append",
                "valueInputOption=USER_ENTERED",
                "insertDataOption=INSERT_ROWS",
            ],
            body_eq: ptrs([
                ("/majorDimension", json!("ROWS")),
                ("/values/0/0", json!("Mar")),
                ("/values/0/1", json!("1500")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "tableRange": "Budget 2026!A1:B3",
                "updates": {"updatedRange": "Budget 2026!A4:B4", "updatedRows": 1,
                    "updatedColumns": 2, "updatedCells": 2}}),
            result_eq: ptrs([
                ("/updated_range", json!("Budget 2026!A4:B4")),
                ("/updated_rows", json!(1)),
                ("/updated_cells", json!(2)),
            ]),
        },
        // AP: src/lib/actions/sheets-update-values.ts (values.update)
        Case {
            tool: "GOOGLESHEETS_UPDATE_VALUES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Budget 2026", "range": "A1:B2",
                "values": [["Month", "Amount"], ["Jan", "1300"]],
                "value_input_option": "RAW", "major_dimension": "ROWS"}),
            method: "PUT",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/'Budget%202026'!A1%3AB2",
                "valueInputOption=RAW",
            ],
            body_eq: ptrs([
                ("/majorDimension", json!("ROWS")),
                ("/values/0/0", json!("Month")),
                ("/values/1/1", json!("1300")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "updatedRange": "Budget 2026!A1:B2", "updatedRows": 2, "updatedColumns": 2, "updatedCells": 4}),
            result_eq: ptrs([
                ("/updated_range", json!("Budget 2026!A1:B2")),
                ("/updated_rows", json!(2)),
            ]),
        },
        // AP: src/lib/actions/sheets-add-row.ts (values.append, majorDimension COLUMNS)
        Case {
            tool: "GOOGLESHEETS_ADD_ROW",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "first_row_headers": true,
                "values": {"A": "Alice", "B": 30}, "as_string": false}),
            method: "POST",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A%3AA:append",
                "valueInputOption=USER_ENTERED",
            ],
            body_eq: ptrs([
                ("/majorDimension", json!("COLUMNS")),
                ("/range", json!("Sheet 1!A:A")),
                ("/values/0/values", json!("Alice")),
                ("/values/1/values", json!("30")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "tableRange": "Sheet 1!A1:B5",
                "updates": {"updatedRange": "Sheet 1!A6:B6", "updatedRows": 1, "updatedColumns": 2, "updatedCells": 2}}),
            result_eq: ptrs([
                ("/spreadsheet_id", json!("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")),
                ("/row", json!(6)),
            ]),
        },
        // AP: src/lib/actions/sheets-add-multiple-rows.ts (normalInsert: values.append)
        Case {
            tool: "GOOGLESHEETS_ADD_MULTIPLE_ROWS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1",
                "values": [{"A": "Dave", "B": 41}, {"A": "Erin", "B": 28}], "as_string": true}),
            method: "POST",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A%3AA:append",
                "valueInputOption=RAW",
            ],
            body_eq: ptrs([
                ("/majorDimension", json!("ROWS")),
                ("/values/0/0", json!("Dave")),
                ("/values/0/1", json!(41)),
                ("/values/1/0", json!("Erin")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "updates": {"updatedRange": "Sheet 1!A7:B8", "updatedRows": 2, "updatedColumns": 2, "updatedCells": 4}}),
            result_eq: ptrs([("/updates/updatedRows", json!(2)), ("/updates/updatedCells", json!(4))]),
        },
        // AP: src/lib/actions/sheets-update-row.ts (values.update of {sheet}!A{n}:ZZZ{n})
        Case {
            tool: "GOOGLESHEETS_UPDATE_ROW",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "row_id": 3, "first_row_headers": true,
                "values": {"A": "Alice", "B": "", "C": true}}),
            method: "PUT",
            url_contains: &[
                "/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A3%3AZZZ3",
                "valueInputOption=USER_ENTERED",
            ],
            // empty string skips (does not clear) the cell; bools are JSON-encoded
            body_eq: ptrs([
                ("/majorDimension", json!("ROWS")),
                ("/values/0/0", json!("Alice")),
                ("/values/0/1", Value::Null),
                ("/values/0/2", json!("true")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "updatedRange": "Sheet 1!A3:C3", "updatedRows": 1, "updatedColumns": 2, "updatedCells": 2}),
            result_eq: ptrs([("/row", json!(3)), ("/updates/updatedRange", json!("Sheet 1!A3:C3"))]),
        },
        // AP: src/lib/actions/sheets-update-multiple-rows.ts (values.batchUpdate)
        Case {
            tool: "GOOGLESHEETS_UPDATE_MULTIPLE_ROWS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1",
                "values": [{"rowId": 2, "A": "Alice2"}, {"rowId": 5, "A": "Bob2"}, {"A": "no rowId"}]}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values:batchUpdate"],
            body_eq: ptrs([
                ("/valueInputOption", json!("USER_ENTERED")),
                ("/data/0/range", json!("Sheet 1!A2:ZZZ2")),
                ("/data/0/values/0/0", json!("Alice2")),
                ("/data/1/range", json!("Sheet 1!A5:ZZZ5")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "totalUpdatedRows": 2, "totalUpdatedColumns": 1, "totalUpdatedCells": 2}),
            result_eq: ptrs([("/totalUpdatedRows", json!(2))]),
        },
        // AP: src/lib/actions/sheets-clear-values.ts (values.clear)
        Case {
            tool: "GOOGLESHEETS_CLEAR_VALUES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "worksheet_title": "Sheet 1", "starting_row": 5, "ending_row": 8}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/values/Sheet%201!A5%3AZZZ8:clear"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"clearedRange": "Sheet 1!A5:ZZZ8"}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/starting_row", json!(5)),
                ("/ending_row", json!(8)),
                ("/cleared_range", json!("Sheet 1!A5:ZZZ8")),
            ]),
        },
        // AP: src/lib/actions/sheets-delete-row.ts via common.ts deleteRow
        Case {
            tool: "GOOGLESHEETS_DELETE_ROW",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "row_number": 4}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/deleteDimension/range/sheetId", json!(0)),
                ("/requests/0/deleteDimension/range/dimension", json!("ROWS")),
                ("/requests/0/deleteDimension/range/startIndex", json!(3)),
                ("/requests/0/deleteDimension/range/endIndex", json!(4)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-delete-multiple-rows.ts (top-down, deduplicated)
        Case {
            tool: "GOOGLESHEETS_DELETE_MULTIPLE_ROWS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "mode": "list", "row_numbers": "5, 3, 5"}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/deleteDimension/range/startIndex", json!(4)),
                ("/requests/0/deleteDimension/range/endIndex", json!(5)),
                ("/requests/1/deleteDimension/range/startIndex", json!(2)),
                ("/requests/1/deleteDimension/range/endIndex", json!(3)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}, {}]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/deleted_ranges/0/start_row", json!(5)),
                ("/deleted_ranges/1/start_row", json!(3)),
            ]),
        },
        // AP: src/lib/actions/sheets-delete-dimension.ts (batchUpdate deleteDimension)
        Case {
            tool: "GOOGLESHEETS_DELETE_DIMENSION",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "dimension": "ROWS", "start_index": 2, "end_index": 5}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/deleteDimension/range/sheetId", json!(0)),
                ("/requests/0/deleteDimension/range/dimension", json!("ROWS")),
                ("/requests/0/deleteDimension/range/startIndex", json!(2)),
                ("/requests/0/deleteDimension/range/endIndex", json!(5)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-insert-dimension.ts (batchUpdate insertDimension)
        Case {
            tool: "GOOGLESHEETS_INSERT_DIMENSION",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "dimension": "ROWS", "start_index": 1, "end_index": 4,
                "inherit_from_before": true}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/insertDimension/range/sheetId", json!(0)),
                ("/requests/0/insertDimension/range/dimension", json!("ROWS")),
                ("/requests/0/insertDimension/range/startIndex", json!(1)),
                ("/requests/0/insertDimension/range/endIndex", json!(4)),
                ("/requests/0/insertDimension/inheritFromBefore", json!(true)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-append-dimension.ts (batchUpdate appendDimension)
        Case {
            tool: "GOOGLESHEETS_APPEND_DIMENSION",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "dimension": "COLUMNS", "length": 3}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/appendDimension/sheetId", json!(0)),
                ("/requests/0/appendDimension/dimension", json!("COLUMNS")),
                ("/requests/0/appendDimension/length", json!(3)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-auto-resize-dimensions.ts
        Case {
            tool: "GOOGLESHEETS_AUTO_RESIZE_DIMENSIONS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "dimension": "COLUMNS", "start_index": 0, "end_index": 3}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/autoResizeDimensions/dimensions/sheetId", json!(0)),
                ("/requests/0/autoResizeDimensions/dimensions/dimension", json!("COLUMNS")),
                ("/requests/0/autoResizeDimensions/dimensions/startIndex", json!(0)),
                ("/requests/0/autoResizeDimensions/dimensions/endIndex", json!(3)),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-update-dimension-properties.ts
        Case {
            tool: "GOOGLESHEETS_UPDATE_DIMENSION_PROPERTIES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "dimension": "COLUMNS", "start_index": 0, "end_index": 1,
                "pixel_size": 120}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/updateDimensionProperties/range/sheetId", json!(0)),
                ("/requests/0/updateDimensionProperties/range/dimension", json!("COLUMNS")),
                ("/requests/0/updateDimensionProperties/range/startIndex", json!(0)),
                ("/requests/0/updateDimensionProperties/range/endIndex", json!(1)),
                ("/requests/0/updateDimensionProperties/properties/pixelSize", json!(120)),
                ("/requests/0/updateDimensionProperties/fields", json!("pixelSize")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-update-sheet-properties.ts
        Case {
            tool: "GOOGLESHEETS_UPDATE_SHEET_PROPERTIES",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 123456789, "frozen_row_count": 1, "tab_color": "#FFD966", "hidden": false}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/updateSheetProperties/properties/sheetId", json!(123456789)),
                ("/requests/0/updateSheetProperties/properties/gridProperties/frozenRowCount", json!(1)),
                // color channels are f64/255.0; exact values are covered by the
                // hex_to_rgb unit test (the harness's JSON round-trip is not
                // bit-exact for fractions)
                ("/requests/0/updateSheetProperties/properties/tabColor/red", json!(1.0)),
                ("/requests/0/updateSheetProperties/properties/hidden", json!(false)),
                ("/requests/0/updateSheetProperties/fields",
                    json!("gridProperties.frozenRowCount,tabColor,hidden")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // AP: src/lib/actions/sheets-format-cells.ts (batchUpdate repeatCell)
        Case {
            tool: "GOOGLESHEETS_FORMAT_CELLS",
            args: json!({"spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
                "sheet_id": 0, "starting_row": 2, "ending_row": 4, "bg_color": "#FFD966",
                "text_color": "#1F1F1F", "bold": true}),
            method: "POST",
            url_contains: &["/v4/spreadsheets/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms:batchUpdate"],
            body_eq: ptrs([
                ("/requests/0/repeatCell/range/sheetId", json!(0)),
                ("/requests/0/repeatCell/range/startRowIndex", json!(1)),
                ("/requests/0/repeatCell/range/endRowIndex", json!(4)),
                // fractional color channels (f64/255.0) are covered by the
                // hex_to_rgb unit test; the harness's JSON round-trip is not
                // bit-exact for them
                ("/requests/0/repeatCell/cell/userEnteredFormat/textFormat/bold", json!(true)),
                ("/requests/0/repeatCell/fields", json!("userEnteredFormat(backgroundColor,textFormat)")),
            ]),
            response_status: 200,
            response: json!({"spreadsheetId": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms", "replies": [{}]}),
            result_eq: ptrs([("/success", json!(true))]),
        },
    ]
}

#[test]
fn recorded_actions() {
    let d = GoogleSheets::default();
    let cases = cases();
    for c in &cases {
        support::run(&d, c);
    }
    support::assert_full_coverage(&d, &cases);
}

#[test]
fn mutating_and_scopes_match_the_piece_classification() {
    let d = GoogleSheets::default();
    let read_only = [
        "GOOGLESHEETS_GET_SPREADSHEET",
        "GOOGLESHEETS_FIND_WORKSHEET",
        "GOOGLESHEETS_SEARCH_SPREADSHEETS",
        "GOOGLESHEETS_GET_VALUES",
        "GOOGLESHEETS_GET_ALL_ROWS",
        "GOOGLESHEETS_GET_ROW",
        "GOOGLESHEETS_FIND_ROWS",
    ];
    for t in &d.spec().tools {
        assert_eq!(t.mutating, !read_only.contains(&t.slug.as_str()), "{}", t.slug);
        let drive_scope = "https://www.googleapis.com/auth/drive";
        if matches!(t.slug.as_str(), "GOOGLESHEETS_SEARCH_SPREADSHEETS" | "GOOGLESHEETS_CREATE_SPREADSHEET") {
            assert!(t.scopes.iter().any(|s| s == drive_scope), "{} needs the drive scope", t.slug);
        } else {
            assert!(
                !t.scopes.iter().any(|s| s == drive_scope),
                "{} must not require the drive scope",
                t.slug
            );
        }
    }
}

#[test]
fn auth_matches_the_piece_oauth2_connection() {
    let d = GoogleSheets::default();
    let oauth = d
        .spec()
        .auth_schemes
        .iter()
        .find_map(|s| match s {
            AuthScheme::OAuth2(o) => Some(o.clone()),
            _ => None,
        })
        .expect("OAuth2 scheme");
    assert_eq!(oauth.authorize_url, "https://accounts.google.com/o/oauth2/auth");
    assert_eq!(oauth.token_url, "https://oauth2.googleapis.com/token");
    assert_eq!(
        oauth.default_scopes,
        vec![
            "https://www.googleapis.com/auth/spreadsheets",
            "https://www.googleapis.com/auth/drive.readonly",
            "https://www.googleapis.com/auth/drive",
            "email",
        ]
    );
    assert!(!oauth.pkce);
}

/// Live smoke test (read-only, #[ignore]d): searches Drive for spreadsheets,
/// reads the first one's metadata and finds its tabs by title.
/// Run: CTOX_TOOLHUB_LIVE_GOOGLESHEETS_TOKEN=<oauth token> \
///   cargo test --offline --test port_google_sheets -- --ignored
#[test]
#[ignore]
fn live_read_only_calls() {
    let d = GoogleSheets::default();
    let id = match support::live(
        &d,
        "GOOGLESHEETS_SEARCH_SPREADSHEETS",
        json!({"spreadsheet_name": "a"}),
    ) {
        None => return, // no live token configured
        Some(result) => {
            let out = result.unwrap_or_else(|e| panic!("GOOGLESHEETS_SEARCH_SPREADSHEETS: {e}"));
            support::find_str(&out, "id")
                .unwrap_or_else(|| panic!("no spreadsheet visible to this token"))
        }
    };
    let worksheets = match support::live(
        &d,
        "GOOGLESHEETS_GET_SPREADSHEET",
        json!({"spreadsheet_id": id}),
    ) {
        None => panic!("live() returned None after a successful search"),
        Some(result) => result.unwrap_or_else(|e| panic!("GOOGLESHEETS_GET_SPREADSHEET: {e}")),
    };
    let title = support::find_str(&worksheets["worksheets"], "title")
        .unwrap_or_else(|| panic!("spreadsheet {id} has no worksheets"));
    match support::live(
        &d,
        "GOOGLESHEETS_FIND_WORKSHEET",
        json!({"spreadsheet_id": id, "title": title}),
    ) {
        None => panic!("live() returned None after a successful search"),
        Some(result) => {
            let out = result.unwrap_or_else(|e| panic!("GOOGLESHEETS_FIND_WORKSHEET: {e}"));
            assert_eq!(out["found"], json!(true), "worksheet {title:?} not found");
        }
    }
}
