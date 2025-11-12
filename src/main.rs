

use clap::{Parser, Subcommand};
use regex::Regex;
use mysqltrim::*;
use std::collections::HashMap;

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
            // Open database.sql and process as raw bytes per line to support non-UTF8 dumps
            let file = std::fs::File::open(file).unwrap();
            let reader = std::io::BufReader::new(file);

            match dest {
                Some(path) => {
                    let out = std::fs::File::create(path).unwrap();
                    let _ = extract_sql(reader, out, include.as_ref(), exclude.as_ref());
                }
                None => {
                    let mut stdout = std::io::stdout();
                    let _ = extract_sql(reader, &mut stdout, include.as_ref(), exclude.as_ref());
                }
            }
        }
        Commands::ShowTables { file, human, include, exclude } => {
            // Compute sizes
            let file1 = std::fs::File::open(file).unwrap();
            let reader1 = std::io::BufReader::new(file1);
            let mut sizes = compute_table_sizes(reader1, include.as_ref(), exclude.as_ref());

            // Compute row counts (separate pass)
            let file2 = std::fs::File::open(file).unwrap();
            let reader2 = std::io::BufReader::new(file2);
            let mut rows = compute_table_row_counts(reader2, include.as_ref(), exclude.as_ref());

            // Merge into a name -> (size, rows) map
            let mut map: HashMap<String, (usize, usize)> = HashMap::new();
            for t in sizes.drain() {
                map.entry(t.name).or_insert((0, 0)).0 = t.size;
            }
            for r in rows.drain() {
                map.entry(r.name).or_insert((0, 0)).1 = r.rows;
            }

            // Render a nicely formatted CLI table
            let mut table_view = comfy_table::Table::new();
            use comfy_table::{presets::UTF8_FULL, Cell, CellAlignment, Row};
            table_view.load_preset(UTF8_FULL);
            table_view.set_header(vec![
                Cell::new("Table").set_alignment(CellAlignment::Left),
                Cell::new("Rows").set_alignment(CellAlignment::Right),
                Cell::new(if *human { "Size" } else { "Bytes" })
                    .set_alignment(CellAlignment::Right),
            ]);

            // Collect & sort by size descending (then name)
            let mut table_vec: Vec<_> = map.into_iter().collect();
            table_vec.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));

            // helper to format size

            for (name, (size, row_count)) in table_vec {
                let size_cell = if *human {
                    Cell::new(human_bytes(size)).set_alignment(CellAlignment::Right)
                } else {
                    Cell::new(size).set_alignment(CellAlignment::Right)
                };
                table_view.add_row(Row::from(vec![
                    Cell::new(name),
                    Cell::new(row_count).set_alignment(CellAlignment::Right),
                    size_cell,
                ]));
            }

            println!("{}", table_view);
        }
    }
}
// human_bytes moved to library
