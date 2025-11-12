

use clap::{Parser, Subcommand};
use regex::Regex;
use mysqltrim::*;

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
        Commands::ShowTables { file, human, include,
            exclude } => {
            let file = std::fs::File::open(file).unwrap();
            let reader = std::io::BufReader::new(file);
            let mut tables = compute_table_sizes(reader, include.as_ref(), exclude.as_ref());

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
            let mut table_vec: Vec<_> = tables.drain().collect();
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
// human_bytes moved to library
