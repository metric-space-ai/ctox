//! The builder-agent generator (`examples/spec_from_builtin.rs`) drafts specs by
//! probing compiled adapters. Guard its quality: drafts are well-formed, cover every
//! tool, and most tools already build identical requests before any hand edits.
#[allow(dead_code)]
#[path = "../examples/spec_from_builtin.rs"]
mod generator;

use ctox_toolhub::spec::SpecAdapter;
use ctox_toolhub::toolkits::{self, ToolkitAdapter};
use serde_json::Value;

fn same_request(rust: &dyn ToolkitAdapter, spec: &SpecAdapter, tool: &str, schema: &Value) -> bool {
    let props = schema["properties"].as_object().cloned().unwrap_or_default();
    let args: serde_json::Map<String, Value> = props.iter().enumerate().map(|(i, (k, p))| (k.clone(), generator::sample(k, p, i))).collect();
    let args = Value::Object(args);
    match (rust.build_request(tool, &args), spec.build_request(tool, &args)) {
        (Ok(a), Ok(b)) => a.method == b.method && a.url == b.url && a.body.as_ref().map(|x| serde_json::from_slice::<Value>(x).ok()) == b.body.as_ref().map(|x| serde_json::from_slice::<Value>(x).ok()),
        (Err(_), Err(_)) => true,
        _ => false,
    }
}

#[test]
fn drafts_cover_every_tool_and_mostly_match_before_hand_edits() {
    // Ratchet: measured floors; raise them when the generator improves.
    for (slug, floor) in [("todoist", 23), ("asana", 11), ("airtable", 17)] {
        let rust = toolkits::builtin().into_iter().find(|a| a.spec().slug == slug).unwrap();
        let mut draft = generator::draft(rust.as_ref());
        assert_eq!(draft["schema"], "ctox.adapter-spec/v1");
        assert_eq!(draft["actions"].as_array().unwrap().len(), rust.spec().tools.len(), "{slug}: every tool drafted");
        // Drafts may contain actions with unmapped args; compile tool by tool.
        let actions = draft["actions"].as_array().unwrap().clone();
        let (mut ok, mut total) = (0, 0);
        for a in actions {
            draft["actions"] = Value::Array(vec![a.clone()]);
            total += 1;
            let Ok(spec) = SpecAdapter::from_json(&draft.to_string()) else { continue };
            let t = rust.spec().tools.iter().find(|t| t.slug == a["slug"]).unwrap();
            if same_request(rust.as_ref(), &spec, &t.slug, &t.input_schema) {
                ok += 1;
            }
        }
        println!("{slug}: {ok}/{total} tools identical straight from the generator");
        assert!(ok >= floor, "{slug}: only {ok}/{total} drafted tools match (floor {floor})");
    }
}
