use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::Path;

use regex::bytes::Regex as BytesRegex;
use regex::Regex;

/// Returns true if the line looks like the start of a table DDL statement
/// e.g. "DROP TABLE IF EXISTS ..." or "CREATE TABLE ..."
#[inline]
pub fn is_table_ddl_line(line: &[u8]) -> bool {
    line.starts_with(b"DROP TABLE IF EXISTS ") || line.starts_with(b"CREATE TABLE ")
}

/// Extract the table name from a DDL line. Accepts backticked or bare identifiers.
/// Returns None if a name can't be parsed.
pub fn table_name_from_ddl_line(line: &[u8]) -> Option<String> {
    // Narrow down to just after the DDL keyword prefix, then parse identifier at start
    let rest = if line.starts_with(b"DROP TABLE IF EXISTS ") {
        &line[b"DROP TABLE IF EXISTS ".len()..]
    } else if line.starts_with(b"DROP TABLE ") {
        &line[b"DROP TABLE ".len()..]
    } else if line.starts_with(b"CREATE TABLE ") {
        &line[b"CREATE TABLE ".len()..]
    } else if line.starts_with(b"CREATE TABLE IF NOT EXISTS ") {
        &line[b"CREATE TABLE IF NOT EXISTS ".len()..]
    } else {
        line
    };

    let re: BytesRegex = BytesRegex::new(r"^\s*`?([A-Za-z0-9_]+)`?").unwrap();
    re.captures(rest)
        .and_then(|caps| caps.get(1))
        .map(|m| String::from_utf8_lossy(m.as_bytes()).to_string())
}

/// Determine whether to skip a table based on include/exclude regexes
pub fn should_skip(table: &str, include: Option<&Regex>, exclude: Option<&Regex>) -> bool {
    if let Some(re) = include {
        if !re.is_match(table) {
            return true;
        }
    }
    if let Some(re) = exclude {
        if re.is_match(table) {
            return true;
        }
    }
    false
}

/// Core extraction loop that hands each included line to a sink.
fn extract_sql_core<R: BufRead, F>(
    mut reader: R,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
    mut write_line: F,
) -> std::io::Result<HashSet<String>>
where
    F: FnMut(Option<&str>, &[u8]) -> std::io::Result<()>,
{
    let mut tables = HashSet::new();
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut skip = false;
    let mut current_table: Option<String> = None;

    loop {
        buf.clear();
        let n = match reader.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        };
        if n == 0 {
            break; // EOF
        }

        if is_table_ddl_line(&buf) {
            if let Some(name) = table_name_from_ddl_line(&buf) {
                tables.insert(name.clone());
                skip = should_skip(&name, include, exclude);
                current_table = if skip { None } else { Some(name) };
            } else {
                skip = false; // couldn't parse; default to include
                current_table = None;
            }
        }

        if skip {
            continue;
        }

        write_line(current_table.as_deref(), &buf)?;
    }

    Ok(tables)
}

/// Extracts SQL related to selected tables from a byte reader and writes to the writer.
/// Returns the set of table names encountered.
pub fn extract_sql<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> std::io::Result<HashSet<String>> {
    extract_sql_core(reader, include, exclude, |_, line| writer.write_all(line))
}

/// Extract SQL into one file per table. Each table becomes `<table>.sql` in `out_dir`.
pub fn extract_sql_per_table<R: BufRead, P: AsRef<Path>>(
    reader: R,
    out_dir: P,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> std::io::Result<HashSet<String>> {
    std::fs::create_dir_all(&out_dir)?;
    let out_dir = out_dir.as_ref().to_path_buf();
    let mut writers: HashMap<String, std::fs::File> = HashMap::new();

    extract_sql_core(reader, include, exclude, |table, line| {
        if let Some(table) = table {
            let writer = match writers.entry(table.to_string()) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => {
                    let path = out_dir.join(format!("{}.sql", table));
                    v.insert(std::fs::File::create(path)?)
                }
            };
            writer.write_all(line)?;
        }
        Ok(())
    })
}

#[derive(Default, Clone, Debug)]
pub struct Table {
    pub name: String,
    pub size: usize,
}
impl Eq for Table {}
impl PartialEq for Table {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl std::hash::Hash for Table {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

/// Walks through an SQL dump and accumulates per-table INSERT byte sizes.
pub fn compute_table_sizes<R: BufRead>(
    mut reader: R,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> HashSet<Table> {
    let mut tables: HashSet<Table> = HashSet::new();
    let mut current_table: Option<Table> = None;
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut skip = false;

    loop {
        buf.clear();
        let n = match reader.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                break;
            }
        };
        if n == 0 {
            break;
        }

        if is_table_ddl_line(&buf) {
            if let Some(name) = table_name_from_ddl_line(&buf) {
                skip = should_skip(&name, include, exclude);
                if skip {
                    current_table = None;
                    continue;
                }
                let table = Table {
                    name,
                    ..Default::default()
                };
                tables.insert(table.clone());
                current_table = Some(table);
            } else {
                current_table = None;
                skip = false;
            }
        } else if !skip && buf.starts_with(b"INSERT ") {
            if let Some(cur) = &current_table {
                if let Some(existing) = tables.get(cur) {
                    let mut table = existing.clone();
                    table.size += buf.len();
                    tables.replace(table);
                }
            }
        }
    }

    tables
}

/// Format a byte size in human-friendly units (KiB, MiB, ...)
pub fn human_bytes(n: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{} B", n);
    }
    let mut size = n as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if size >= 100.0 || unit == 0 {
        // no decimal for large numbers
        format!("{:.0} {}", size, UNITS[unit])
    } else if size >= 10.0 {
        format!("{:.1} {}", size, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

// Tests moved to tests/lib_tests.rs

#[derive(Default, Clone, Debug)]
pub struct TableRows {
    pub name: String,
    pub rows: usize,
}
impl Eq for TableRows {}
impl PartialEq for TableRows {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl std::hash::Hash for TableRows {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[inline]
fn matches_values_kw(bytes: &[u8], i: usize) -> bool {
    const NEEDLE: &[u8; 6] = b"values";
    if i + NEEDLE.len() > bytes.len() {
        return false;
    }
    for j in 0..NEEDLE.len() {
        if bytes[i + j].to_ascii_lowercase() != NEEDLE[j] {
            return false;
        }
    }
    true
}

/// Count how many tuple groups appear in an INSERT ... VALUES statement.
/// Attempts to ignore parentheses inside quoted strings and only starts
/// counting after the VALUES keyword. Works across single lines; for multi-line
/// INSERTs, call on each line and sum the results.
fn count_insert_values_tuples_line(line: &[u8], mut values_seen: bool) -> (usize, bool, bool) {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    let mut count = 0usize;
    let mut ended = false;
    let mut i = 0usize;

    while i < line.len() {
        let c = line[i];

        if escape {
            escape = false;
            i += 1;
            continue;
        }

        if c == b'\\' {
            // MySQL uses C-style backslash escapes in dumps
            escape = true;
            i += 1;
            continue;
        }

        if !in_double && c == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if !in_single && c == b'"' {
            in_double = !in_double;
            i += 1;
            continue;
        }

        if !in_single && !in_double && !values_seen {
            if matches_values_kw(line, i) {
                // Ensure word boundary around VALUES to reduce false positives
                let start = i;
                let end = i + 6;
                let prev_ok = start == 0 || !line[start - 1].is_ascii_alphabetic();
                let next_ok = end >= line.len() || !line[end].is_ascii_alphabetic();
                if prev_ok && next_ok {
                    values_seen = true;
                    i = end;
                    continue;
                }
            }
        } else if !in_single && !in_double && values_seen {
            if c == b'(' {
                count += 1;
            } else if c == b';' {
                ended = true;
                // keep scanning to preserve quote state correctness, though usually end of line
            }
        }

        i += 1;
    }

    (count, values_seen, ended)
}

/// Walks through an SQL dump and counts per-table INSERT row counts.
/// Supports multi-value INSERT syntax (INSERT ... VALUES (...), (...), ...)
pub fn compute_table_row_counts<R: BufRead>(
    mut reader: R,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> HashSet<TableRows> {
    let mut tables: HashSet<TableRows> = HashSet::new();
    let mut current_table: Option<TableRows> = None;
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut skip = false;
    let mut in_insert = false;
    let mut after_values = false;

    loop {
        buf.clear();
        let n = match reader.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                break;
            }
        };
        if n == 0 {
            break;
        }

        if is_table_ddl_line(&buf) {
            if let Some(name) = table_name_from_ddl_line(&buf) {
                skip = should_skip(&name, include, exclude);
                if skip {
                    current_table = None;
                    continue;
                }
                let table = TableRows {
                    name,
                    ..Default::default()
                };
                tables.insert(table.clone());
                current_table = Some(table);
                in_insert = false;
                after_values = false;
            } else {
                current_table = None;
                skip = false;
                in_insert = false;
                after_values = false;
            }
        } else if !skip && buf.starts_with(b"INSERT ") {
            if let Some(cur) = &current_table {
                if let Some(existing) = tables.get(cur) {
                    let mut table = existing.clone();
                    let (cnt, seen_vals, ended) = count_insert_values_tuples_line(&buf, false);
                    table.rows += cnt;
                    tables.replace(table);
                    in_insert = true;
                    after_values = seen_vals;
                    if ended {
                        in_insert = false;
                        after_values = false;
                    }
                }
            }
        } else if !skip && in_insert {
            // Continuation lines for a multi-line INSERT statement
            if let Some(cur) = &current_table {
                if let Some(existing) = tables.get(cur) {
                    let mut table = existing.clone();
                    let (cnt, seen_vals, ended) = count_insert_values_tuples_line(&buf, after_values);
                    table.rows += cnt;
                    tables.replace(table);
                    after_values = seen_vals;
                    if ended {
                        in_insert = false;
                        after_values = false;
                    }
                }
            }
        }
    }

    tables
}
