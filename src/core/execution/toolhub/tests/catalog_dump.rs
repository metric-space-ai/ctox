#[test]
#[ignore]
fn dump() {
    for a in ctox_toolhub::toolkits::builtin() {
        let s = a.spec();
        let m = s.tools.iter().filter(|t| t.mutating).count();
        println!("CAT {} {} read={} write={}", s.slug, s.tools.len(), s.tools.len() - m, m);
    }
}
