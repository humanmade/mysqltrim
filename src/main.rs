use std::{
    collections::HashSet,
    io::{BufRead, Write},
    process::exit,
    thread::current,
};

use clap::{Parser, Subcommand};
use regex::Regex;

/// Trim an SQL file down to a smaller file, based off table includes / excludes
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract tables from a SQL file
    Extract {
        /// The SQL file to extract from
        #[arg(index = 1)]
        file: String,
        /// The destination file to write to
        #[arg(index = 2)]
        dest: Option<String>,
        /// Only include tables that match this regex
        #[arg(long)]
        include: Option<Regex>,
        /// Exclude tables that match this regex
        #[arg(long)]
        exclude: Option<Regex>,
    },
    /// Show the tables in a SQL file
    ShowTables {
        /// The SQL file to extract from
        #[arg(index = 1)]
        file: String,
        /// Display sizes in human readable units (KiB, MiB, GiB)
        #[arg(long = "human", action = clap::ArgAction::SetTrue, help = "Display sizes in human readable units")]
        human: bool,
        /// Only include tables that match this regex
        #[arg(long)]
        include: Option<Regex>,
        /// Exclude tables that match this regex
        #[arg(long)]
        exclude: Option<Regex>,
    },
}

fn main() {
    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Commands::Extract {
            file,
            dest,
            include,
            exclude,
        } => {
            // Open database.sql for reading line by line
            let file = std::fs::File::open(file).unwrap();
            let mut destination = dest
                .clone()
                .map(|dest| std::fs::File::create(dest).unwrap());

            let mut current_table_name;
            let mut skip = false;
            let table_name_regex = Regex::new("`?([a-zA-Z0-9_]+)`").unwrap();
            let mut tables = HashSet::new();
            for line in std::io::BufReader::new(file).lines().map(|l| l.unwrap()) {
                // If the line matches "DROP TABLE IF EXISTS `wp_2_commentmeta`;" set current table
                if line.starts_with("DROP TABLE IF EXISTS ") || line.starts_with("CREATE TABLE ") {
                    current_table_name = table_name_regex
                        .captures(&line)
                        .unwrap()
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_string();
                    tables.insert(current_table_name.clone());

                    if let Some(regex) = &include {
                        skip = !regex.is_match(&current_table_name)
                    }

                    if let Some(regex) = &exclude {
                        skip = regex.is_match(&current_table_name)
                    }
                }

                if skip {
                    continue;
                }

                // Write the line to the destination file, appending a newline character
                match &mut destination {
                    Some(destination) => {
                        destination.write_all(line.as_bytes()).unwrap();
                        destination.write_all(b"\n").unwrap();
                    }
                    None => println!("{}", line),
                }
            }
        }
        Commands::ShowTables { file, human, include,
            exclude } => {
            #[derive(Default, Clone, Debug)]
            struct Table {
                name: String,
                size: usize,
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
            let mut tables = HashSet::new();
            // Open database.sql for reading line by line
            let mut current_table = None;

            let mut skip = false;

            let file = std::fs::File::open(file).unwrap();
            for line in std::io::BufReader::new(file).lines().map(|l| l.unwrap()) {
                // If the line matches "DROP TABLE IF EXISTS `wp_2_commentmeta`;" set current table
                if line.starts_with("DROP TABLE IF EXISTS ") || line.starts_with("CREATE TABLE ") {
                    let table_name_regex = Regex::new("`?([a-zA-Z0-9_]+)`").unwrap();
                    let current_table_name = table_name_regex
                        .captures(&line)
                        .unwrap()
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_string();

                    if let Some(regex) = &include {
                        skip = !regex.is_match(&current_table_name)
                    }

                    if let Some(regex) = &exclude {
                        skip = regex.is_match(&current_table_name)
                    }
                    if skip {
                        continue;
                    }

                    let table = Table {
                        name: current_table_name,
                        ..Default::default()
                    };
                    tables.insert(table.clone());
                    current_table = Some(table);
                } else if ! skip && line.starts_with("INSERT ") && current_table.is_some() {
                    let mut table = tables.get(&current_table.clone().unwrap()).unwrap().clone();
                    table.size += line.len();
                    tables.replace(table);
                }
            }

            // Render a nicely formatted CLI table
            let mut table_view = comfy_table::Table::new();
            use comfy_table::{presets::UTF8_FULL, Cell, CellAlignment, Row};
            table_view.load_preset(UTF8_FULL);
            table_view.set_header(vec![
                Cell::new("Table").set_alignment(CellAlignment::Left),
                Cell::new(if *human { "Size" } else { "Bytes" })
                    .set_alignment(CellAlignment::Right),
            ]);

            // Collect & sort by size descending
            let mut table_vec: Vec<_> = tables.into_iter().collect();
            table_vec.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));

            // helper to format size

            for t in table_vec {
                let size_cell = if *human {
                    Cell::new(human_bytes(t.size)).set_alignment(CellAlignment::Right)
                } else {
                    Cell::new(t.size).set_alignment(CellAlignment::Right)
                };
                table_view.add_row(Row::from(vec![
                    Cell::new(t.name),
                    size_cell,
                ]));
            }

            println!("{}", table_view);
        }
    }
}

fn human_bytes(n: usize) -> String {
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
