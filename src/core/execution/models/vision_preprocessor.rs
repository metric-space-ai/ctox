// Origin: CTOX
// License: Apache-2.0
//
// Minimal image-marker helper used by the TUI chat surface.
//
// The larger gateway-side vision preprocessing experiment from the refactor
// was never wired back into the productive request path. What remains in use
// today is the canonical marker encoding for pending local image attachments.

use std::path::Path;

const CTOX_IMAGE_MARKER_PREFIX: &str = "[[ctox:image:";
const CTOX_IMAGE_MARKER_SUFFIX: &str = "]]";

pub fn encode_image_marker(path: &Path) -> String {
    format!(
        "{}{}{}",
        CTOX_IMAGE_MARKER_PREFIX,
        path.display(),
        CTOX_IMAGE_MARKER_SUFFIX
    )
}

#[cfg(test)]
mod tests {
    use super::encode_image_marker;
    use std::path::PathBuf;

    #[test]
    fn encode_image_marker_roundtrips() {
        let path = PathBuf::from("/tmp/foo.png");
        let marker = encode_image_marker(&path);
        assert_eq!(marker, "[[ctox:image:/tmp/foo.png]]");
    }
}
