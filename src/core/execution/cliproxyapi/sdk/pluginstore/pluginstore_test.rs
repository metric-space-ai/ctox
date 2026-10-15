// ref: sdk/pluginstore/pluginstore_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn sample_plugin() -> Plugin {
    Plugin {
        id: "sample-provider".to_owned(),
        name: "Sample Provider".to_owned(),
        description: "Adds sample provider support.".to_owned(),
        author: "author-name".to_owned(),
        repository: "https://github.com/author-name/sample-provider".to_owned(),
        ..Plugin::default()
    }
}

fn valid_test_manifest() -> Manifest {
    Manifest {
        id: "sample-provider".to_owned(),
        name: "Sample Provider".to_owned(),
        description: "Adds sample provider support.".to_owned(),
        author: "author-name".to_owned(),
        version: "0.2.0".to_owned(),
        release_tag: "v0.2.0".to_owned(),
        repository: "https://github.com/author-name/sample-provider".to_owned(),
        ..Manifest::default()
    }
}
