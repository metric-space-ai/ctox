//! Error helpers.
//!
//! Depends on [`utils_string::ucfirst`].

use serde_json::{json, Value};

use crate::plugins::utils::utils_string::ucfirst;
use crate::rx_error::RxError;

// ref: rxdb/src/plugins/utils/utils-error.ts:13-31
/// Returns an error that indicates that a plugin is missing.
/// We do not throw an RxError because this should not be handled
/// programmatically but by using the correct import.
pub fn plugin_missing(plugin_key: &str) -> std::io::Error {
    let mut plugin_name = String::from("RxDB");
    for part in plugin_key.split('-') {
        plugin_name.push_str(&ucfirst(part));
    }
    plugin_name.push_str("Plugin");
    std::io::Error::new(
        std::io::ErrorKind::Other,
        format!(
            "You are using a function which must be overwritten by a plugin.\n\
             You should either prevent the usage of this function or add the plugin via:\n    \
                 import {{ {plugin_name} }} from 'rxdb/plugins/{plugin_key}';\n    \
                 addRxPlugin({plugin_name});\n        "
        ),
    )
}

// ref: rxdb/src/plugins/utils/utils-error.ts:35-53
/// Map an error to a plain JSON object for transport across instance boundaries.
pub fn error_to_plain_json(err: &(dyn std::error::Error + 'static)) -> Value {
    // Upstream PlainJsonError fields: name, message, rxdb, parameters, extensions, code, url, stack
    let mut obj = serde_json::Map::new();
    obj.insert("name".to_string(), json!("Error"));
    obj.insert("message".to_string(), json!(err.to_string()));
    if let Some(rx) = err.downcast_ref::<RxError>() {
        obj.insert("rxdb".to_string(), json!(true));
        obj.insert("parameters".to_string(), rx.parameters().clone());
        obj.insert("code".to_string(), json!(rx.code()));
        obj.insert("url".to_string(), json!(rx.url()));
        obj.insert("name".to_string(), json!(rx.name()));
    }
    Value::Object(obj)
}
