use std::collections::HashSet;
use std::io::{BufRead, Write};

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

/// Extracts SQL related to selected tables from a byte reader and writes to the writer.
/// Returns the set of table names encountered.
pub fn extract_sql<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> std::io::Result<HashSet<String>> {
    let mut tables = HashSet::new();
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut skip = false;

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
            } else {
                skip = false; // couldn't parse; default to include
            }
        }

        if !skip {
            writer.write_all(&buf)?;
        }
    }

    Ok(tables)
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
