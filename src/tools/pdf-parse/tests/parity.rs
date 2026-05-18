use ctox_pdf_parse::core::config::LiteParseConfig;
use ctox_pdf_parse::core::types::{ForwardAnchors, ParsedPage, ProjectionTextBox, TextItem};
use ctox_pdf_parse::processing::bbox::build_bounding_boxes;
use ctox_pdf_parse::processing::clean_text::clean_raw_text;
use ctox_pdf_parse::processing::grid_projection::{bbox_to_lines, project_to_grid};
use ctox_pdf_parse::processing::text_utils::clean_ocr_table_artifacts;
use std::collections::BTreeMap;

#[test]
fn parity_bbox_merge_same_line() {
    let input = vec![
        ProjectionTextBox {
            str: "Hello".into(),
            x: 0.0,
            y: 10.0,
            w: 50.0,
            h: 12.0,
            str_length: 5,
            ..Default::default()
        },
        ProjectionTextBox {
            str: " World".into(),
            x: 50.0,
            y: 10.0,
            w: 55.0,
            h: 12.0,
            str_length: 6,
            ..Default::default()
        },
    ];

    let output = bbox_to_lines(&input, 10.0, 12.0, None);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0][0].str, "Hello World");
    assert_eq!(output[0][0].w, 105.0);
}

#[test]
fn parity_bbox_unsorted_sorts_and_merges() {
    let input = vec![
        ProjectionTextBox {
            str: "C".into(),
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            str_length: 1,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "A".into(),
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            str_length: 1,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "B".into(),
            x: 10.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            str_length: 1,
            ..Default::default()
        },
    ];

    let output = bbox_to_lines(&input, 10.0, 12.0, None);
    assert_eq!(output[0][0].str, "ABC");
}

#[test]
fn parity_project_single_column() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![
        ProjectionTextBox {
            str: "Hello".into(),
            x: 10.0,
            y: 100.0,
            w: 50.0,
            h: 12.0,
            r: Some(0),
            str_length: 5,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "World".into(),
            x: 10.0,
            y: 115.0,
            w: 50.0,
            h: 12.0,
            r: Some(0),
            str_length: 5,
            ..Default::default()
        },
    ];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(result.text, "Hello\nWorld");
    assert_eq!(result.prev_anchors.forward_anchor_left.get("10"), Some(&1));
}

#[test]
fn parity_project_two_column() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![
        ProjectionTextBox {
            str: "Name".into(),
            x: 10.0,
            y: 100.0,
            w: 40.0,
            h: 12.0,
            r: Some(0),
            str_length: 4,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Age".into(),
            x: 300.0,
            y: 100.0,
            w: 30.0,
            h: 12.0,
            r: Some(0),
            str_length: 3,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Alice".into(),
            x: 10.0,
            y: 115.0,
            w: 50.0,
            h: 12.0,
            r: Some(0),
            str_length: 5,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "30".into(),
            x: 300.0,
            y: 115.0,
            w: 20.0,
            h: 12.0,
            r: Some(0),
            str_length: 2,
            ..Default::default()
        },
    ];

    let mut left = BTreeMap::new();
    left.insert("10".into(), 1);
    left.insert("300".into(), 11);

    let prev = ForwardAnchors {
        forward_anchor_left: left,
        ..Default::default()
    };

    let result = project_to_grid(&config, &page, boxes, prev.clone(), 1);
    assert_eq!(result.text, "Name        Age\nAlice        30");
    assert_eq!(result.prev_anchors, prev);
}

#[test]
fn parity_project_right_rail_keeps_sidebar_off_main_line() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 640.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![
        ProjectionTextBox {
            str: "Body intro".into(),
            x: 20.0,
            y: 100.0,
            w: 140.0,
            h: 12.0,
            r: Some(0),
            str_length: 10,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "continues here".into(),
            x: 170.0,
            y: 100.0,
            w: 120.0,
            h: 12.0,
            r: Some(0),
            str_length: 14,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Promo".into(),
            x: 520.0,
            y: 100.0,
            w: 46.0,
            h: 12.0,
            r: Some(0),
            str_length: 5,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Second body".into(),
            x: 20.0,
            y: 116.0,
            w: 120.0,
            h: 12.0,
            r: Some(0),
            str_length: 11,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "line follows".into(),
            x: 150.0,
            y: 116.0,
            w: 100.0,
            h: 12.0,
            r: Some(0),
            str_length: 12,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Details".into(),
            x: 518.0,
            y: 116.0,
            w: 52.0,
            h: 12.0,
            r: Some(0),
            str_length: 7,
            ..Default::default()
        },
    ];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(
        result.text,
        "Body intro continues here\nPromo\nSecond body line follows\nDetails"
    );
}

#[test]
fn parity_project_preserves_camelcase_terms() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![ProjectionTextBox {
        str: "commercial completion engines, such as GitHub Copilot, INSEC".into(),
        x: 54.0,
        y: 100.0,
        w: 260.0,
        h: 10.0,
        r: Some(0),
        str_length: 58,
        ..Default::default()
    }];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(
        result.text,
        "commercial completion engines, such as GitHub Copilot, INSEC"
    );
}

#[test]
fn parity_project_splits_long_camel_terms() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![
        ProjectionTextBox {
            str: "AcroForm".into(),
            x: 54.0,
            y: 100.0,
            w: 50.0,
            h: 10.0,
            r: Some(0),
            str_length: 8,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "TextField:".into(),
            x: 54.0,
            y: 114.0,
            w: 60.0,
            h: 10.0,
            r: Some(0),
            str_length: 10,
            ..Default::default()
        },
    ];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(result.text, "Acro Form\nText Field:");
}

#[test]
fn parity_project_preserves_urlish_fragments() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![ProjectionTextBox {
        str: "Powered by TCPDF (www.tcpdf.org) support@cosinex.de http://www.dtvp.de".into(),
        x: 10.0,
        y: 100.0,
        w: 360.0,
        h: 12.0,
        r: Some(0),
        str_length: 71,
        ..Default::default()
    }];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(
        result.text,
        "Powered by TCPDF (www.tcpdf.org) support@cosinex.de http://www.dtvp.de"
    );
}

#[test]
fn parity_project_two_column_region_orders_left_before_right() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let boxes = vec![
        ProjectionTextBox {
            str: "Left introduction line".into(),
            x: 50.0,
            y: 100.0,
            w: 220.0,
            h: 12.0,
            r: Some(0),
            str_length: 22,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Figure caption line".into(),
            x: 320.0,
            y: 100.0,
            w: 210.0,
            h: 12.0,
            r: Some(0),
            str_length: 19,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Left body continues".into(),
            x: 50.0,
            y: 116.0,
            w: 220.0,
            h: 12.0,
            r: Some(0),
            str_length: 19,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Right column starts".into(),
            x: 320.0,
            y: 116.0,
            w: 210.0,
            h: 12.0,
            r: Some(0),
            str_length: 19,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Left closing sentence".into(),
            x: 50.0,
            y: 132.0,
            w: 220.0,
            h: 12.0,
            r: Some(0),
            str_length: 21,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Right body follows".into(),
            x: 320.0,
            y: 132.0,
            w: 210.0,
            h: 12.0,
            r: Some(0),
            str_length: 18,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Left footer line".into(),
            x: 50.0,
            y: 148.0,
            w: 220.0,
            h: 12.0,
            r: Some(0),
            str_length: 16,
            ..Default::default()
        },
        ProjectionTextBox {
            str: "Right closing line".into(),
            x: 320.0,
            y: 148.0,
            w: 210.0,
            h: 12.0,
            r: Some(0),
            str_length: 18,
            ..Default::default()
        },
    ];

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(
        result.text,
        "Left introduction line\nLeft body continues\nLeft closing sentence\nLeft footer line\n\nFigure caption line\nRight column starts\nRight body follows\nRight closing line"
    );
}

#[test]
fn parity_dot_garbage_filtering() {
    let config = LiteParseConfig::default();
    let page = ParsedPage {
        page_num: 1,
        width: 612.0,
        height: 792.0,
        text: String::new(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    };

    let mut boxes = vec![];
    for _ in 0..110 {
        boxes.push(ProjectionTextBox {
            str: "...".into(),
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            r: Some(0),
            str_length: 3,
            ..Default::default()
        });
    }
    boxes.push(ProjectionTextBox {
        str: "Revenue".into(),
        x: 10.0,
        y: 100.0,
        w: 70.0,
        h: 12.0,
        r: Some(0),
        str_length: 7,
        ..Default::default()
    });
    boxes.push(ProjectionTextBox {
        str: "500".into(),
        x: 300.0,
        y: 100.0,
        w: 30.0,
        h: 12.0,
        r: Some(0),
        str_length: 3,
        ..Default::default()
    });

    let result = project_to_grid(&config, &page, boxes, ForwardAnchors::default(), 1);
    assert_eq!(result.text, "Revenue        500");
}

#[test]
fn parity_build_bounding_boxes() {
    let items = vec![
        TextItem {
            str: "Hello".into(),
            x: 10.0,
            y: 20.0,
            w: Some(30.0),
            h: Some(12.0),
            ..Default::default()
        },
        TextItem {
            str: " ".into(),
            x: 40.0,
            y: 20.0,
            w: Some(5.0),
            h: Some(12.0),
            ..Default::default()
        },
    ];

    let boxes = build_bounding_boxes(&items);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].x1, 10.0);
    assert_eq!(boxes[0].x2, 40.0);
}

#[test]
fn parity_clean_ocr_artifacts() {
    assert_eq!(clean_ocr_table_artifacts("| 123 |"), "123");
    assert_eq!(clean_ocr_table_artifacts("N/A"), "N/A");
    assert_eq!(clean_ocr_table_artifacts("| Revenue |"), "| Revenue |");
}

#[test]
fn parity_clean_raw_text() {
    let mut pages = vec![ParsedPage {
        page_num: 1,
        width: 100.0,
        height: 100.0,
        text: "\n   Hello   \n   World\u{0000}\n".into(),
        text_items: vec![],
        images: vec![],
        bounding_boxes: None,
    }];

    clean_raw_text(&mut pages, &LiteParseConfig::default());
    assert_eq!(pages[0].text, "Hello\nWorld");
}
