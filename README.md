Trim an SQL file down to a smaller file, based off table includes / excludes.

Blazingly fast, extracing a table from a 15GB database dump takes just 7.3 seconds.

```
Trim an SQL file down to a smaller file, based off table includes / excludes

Usage: mysqltrim <COMMAND>

Commands:
  extract      Extract tables from a SQL file
  show-tables  Show the tables in a SQL file
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## `extract`

```
Extract tables from a SQL file

Usage: mysqltrim extract [OPTIONS] <FILE> [DEST]

Arguments:
  <FILE>  The SQL file to extract from
  [DEST]  The destination file to write to

Options:
      --include <INCLUDE>  Only include tables that match this regex
      --exclude <EXCLUDE>  Exclude tables that match this regex
  -h, --help               Print help
```

## `show-tables`

```
Show the tables in a SQL file

Usage: mysqltrim show-tables [OPTIONS] <FILE>

Arguments:
  <FILE>  The SQL file to extract from

Options:
      --human              Display sizes in human readable units
      --include <INCLUDE>  Only include tables that match this regex
      --exclude <EXCLUDE>  Exclude tables that match this regex
  -h, --help               Print help
```
