use std::{collections::HashSet, io::{BufRead, Write}};

use clap::Parser;
use regex::Regex;

/// Trim an SQL file down to a smaller file, based off table includes / excludes
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    file: String,
    #[arg(index = 2)]
    dest: Option<String>,
    /// Only include tables that match this regex
    #[arg(long)]
    include: Option<Regex>,
    /// Exclude tables that match this regex
    #[arg(long)]
    exclude: Option<Regex>,
    /// Only print names of tables that would be included
    #[arg(long, default_value="false")]
    show_tables:bool
}

fn main() {
    let args = Args::parse();

    // Open database.sql for reading line by line
    let file = std::fs::File::open(args.file).unwrap();
    let mut destination = args.dest.map(|dest| std::fs::File::create(dest).unwrap());

    let mut current_table_name;
    let mut skip = false;
    let table_name_regex = Regex::new("`?([a-zA-Z0-9_]+)`").unwrap();
    let mut tables = HashSet::new();
    for line in std::io::BufReader::new(file).lines().map(|l| l.unwrap()) {
        // If the line matches "DROP TABLE IF EXISTS `wp_2_commentmeta`;" set current table
        if line.starts_with("DROP TABLE IF EXISTS ") || line.starts_with("CREATE TABLE ") {

            current_table_name = table_name_regex.captures(&line).unwrap().get(1).unwrap().as_str().to_string();
            tables.insert(current_table_name.clone());

            if let Some(regex) = &args.include {
                skip = ! regex.is_match(&current_table_name)
            }

            if let Some(regex) = &args.exclude {
                skip = regex.is_match(&current_table_name)
            }
        }

        if skip || args.show_tables {
            continue;
        }

        // Write the line to the destination file, appending a newline character
        match  &mut destination {
            Some(destination) => {
                destination.write_all(line.as_bytes()).unwrap();
                destination.write_all(b"\n").unwrap();
            }
            None => println!("{}", line),
        }
    }

    if args.show_tables {
        for table in tables {
            println!("{}", table);
        }
    }
}
