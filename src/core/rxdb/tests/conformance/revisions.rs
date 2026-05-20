pub const EXAMPLE_REVISION_1: &str = "1-12080c42d471e3d2625e49dcca3b8e1a";
pub const EXAMPLE_REVISION_2: &str = "2-22080c42d471e3d2625e49dcca3b8e2b";
pub const EXAMPLE_REVISION_3: &str = "3-32080c42d471e3d2625e49dcca3b8e3c";
pub const EXAMPLE_REVISION_4: &str = "4-42080c42d471e3d2625e49dcca3b8e3c";

#[test]
fn revision_examples_match_upstream_test_utils() {
    assert_eq!(EXAMPLE_REVISION_1, "1-12080c42d471e3d2625e49dcca3b8e1a");
    assert_eq!(EXAMPLE_REVISION_2, "2-22080c42d471e3d2625e49dcca3b8e2b");
    assert_eq!(EXAMPLE_REVISION_3, "3-32080c42d471e3d2625e49dcca3b8e3c");
    assert_eq!(EXAMPLE_REVISION_4, "4-42080c42d471e3d2625e49dcca3b8e3c");
}
