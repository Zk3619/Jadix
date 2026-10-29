use super::*;

#[test]
fn smaps_rollup_sums_private() {
    let text = "\
Rss:                   10000 kB
Private_Clean:          1024 kB
Private_Dirty:          2048 kB
Pss:                    2500 kB
";
    assert_eq!(parse_smaps_rollup_private_kb(text), Some(3072));
}

#[test]
fn status_rss_anon() {
    let text = "Name:\tjadix\nVmRSS:\t5000 kB\nRssAnon:\t4096 kB\nRssFile:\t900 kB\n";
    assert_eq!(parse_status_rss_anon_kb(text), Some(4096));
}
