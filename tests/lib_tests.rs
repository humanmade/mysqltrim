use mysqltrim::*;
use regex::Regex;
use std::io::Cursor;

#[test]
fn parse_table_name_from_drop_and_create() {
    let drop = b"DROP TABLE IF EXISTS `wp_posts`;\n";
    let create = b"CREATE TABLE wp_users (id int);\n";
    assert_eq!(table_name_from_ddl_line(drop).as_deref(), Some("wp_posts"));
    assert_eq!(table_name_from_ddl_line(create).as_deref(), Some("wp_users"));
}

#[test]
fn filter_include_exclude() {
    let inc = Regex::new("^wp_").ok();
    let exc = Regex::new("usermeta$").ok();
    assert_eq!(should_skip("wp_posts", inc.as_ref(), exc.as_ref()), false);
    assert_eq!(should_skip("users", inc.as_ref(), exc.as_ref()), true);
    assert_eq!(should_skip("wp_usermeta", inc.as_ref(), exc.as_ref()), true);
}

#[test]
fn extract_filters_and_writes() {
    let sql = b"DROP TABLE IF EXISTS `wp_a`;\nCREATE TABLE `wp_a` (...);\nINSERT INTO `wp_a` VALUES (1);\nDROP TABLE IF EXISTS `wp_b`;\nCREATE TABLE `wp_b` (...);\nINSERT INTO `wp_b` VALUES (2);\n";
    let reader = Cursor::new(sql);
    let mut out = Vec::new();
    let include = Regex::new("^wp_a$").ok();
    let tables = extract_sql(reader, &mut out, include.as_ref(), None).unwrap();
    let out_str = String::from_utf8_lossy(&out);
    assert!(out_str.contains("wp_a"));
    assert!(!out_str.contains("wp_b"));
    assert!(tables.contains("wp_a"));
    assert!(tables.contains("wp_b")); // encountered even if skipped
}

#[test]
fn sizes_accumulate_for_inserts() {
    let sql = b"CREATE TABLE t1 (...);\nINSERT INTO t1 VALUES (1);\nINSERT INTO t1 VALUES (2);\nCREATE TABLE t2 (...);\nINSERT INTO t2 VALUES (3);\n";
    let reader = Cursor::new(sql);
    let set = compute_table_sizes(reader, None, None);
    let mut t1 = set.iter().find(|t| t.name == "t1").unwrap().clone();
    let t2 = set.iter().find(|t| t.name == "t2").unwrap().clone();
    // Expect two insert lines counted for t1
    // exact byte sizes depend on line lengths; just ensure t1 > t2
    assert!(t1.size > t2.size);
    // mutate to check that Hash + replace works (no panic)
    t1.size += 1;
}

#[test]
fn human_sizes() {
    assert_eq!(human_bytes(999), "999 B");
    assert_eq!(human_bytes(1024), "1.00 KiB");
    assert_eq!(human_bytes(10 * 1024 * 1024), "10.0 MiB");
}

#[test]
fn row_counts_multi_values_and_multiline() {
    let sql = b"CREATE TABLE t1 (...);\n\
INSERT INTO t1 VALUES (1, '(paren)'), (2), ('x, y');\n\
INSERT INTO t1 VALUES\n(3),\n(4);\n\
CREATE TABLE t2 (...);\n\
INSERT INTO t2 VALUES ('(only)');\n";
    let reader = Cursor::new(sql);
    let set = compute_table_row_counts(reader, None, None);
    let t1 = set.iter().find(|t| t.name == "t1").unwrap().clone();
    let t2 = set.iter().find(|t| t.name == "t2").unwrap().clone();
    assert_eq!(t1.rows, 5, "t1 should have 3 + 2 tuples counted");
    assert_eq!(t2.rows, 1, "t2 should have a single tuple counted");
}
