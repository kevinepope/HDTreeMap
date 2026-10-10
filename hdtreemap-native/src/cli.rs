//! Command-line parsing for the `hdtreemap` binary.
//!
//! ```text
//! hdtreemap [PATH]                    open the app (scanning PATH straight away if given)
//! hdtreemap --demo [ID]               the simulated drives of the web demo (scan ID now)
//! hdtreemap --dump-top N [PATH]       scan PATH (default: .) headless, print the top N
//! hdtreemap --dump-top N --demo ID    the same for a simulated drive
//! hdtreemap --help
//! ```

use std::path::PathBuf;

/// Text printed for `--help` and after a usage error.
pub const USAGE: &str = "\
Usage:
  hdtreemap [PATH]                   Open HDTreeMap, scanning PATH if given
  hdtreemap --demo [ID]              Use the web demo's simulated drives (scan ID now)
  hdtreemap --dump-top N [PATH]      Scan PATH (default .) without a window, print the
                                     N largest folders and files, totals and timing
  hdtreemap --dump-top N --demo ID   The same for a simulated drive
  hdtreemap --help                   Show this help

Simulated drive ids: laptop-ssd, dev-workstation, media-library, usb-stick,
messy-downloads, archive-server";

/// What to scan headless.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DumpSource {
    Path(PathBuf),
    Demo(String),
}

/// What the command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// The windowed app on the real machine, optionally scanning `path` at once.
    App {
        path: Option<PathBuf>,
    },
    /// The windowed app on the simulated demo machine, optionally scanning drive `id`.
    Demo {
        id: Option<String>,
    },
    /// Scan without a window and print the `top` largest entries.
    DumpTop {
        top: usize,
        source: DumpSource,
    },
    Help,
}

/// Parse the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut demo: Option<Option<String>> = None;
    let mut top: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" => return Ok(Command::Help),
            "--demo" => {
                let id = args
                    .get(i + 1)
                    .filter(|next| !next.starts_with("--"))
                    .cloned();
                if id.is_some() {
                    i += 1;
                }
                demo = Some(id);
            }
            "--dump-top" => {
                let n = args
                    .get(i + 1)
                    .ok_or("--dump-top needs a number")?
                    .parse::<usize>()
                    .map_err(|_| "--dump-top needs a whole number".to_string())?;
                top = Some(n);
                i += 1;
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
            _ => {
                if path.is_some() {
                    return Err("only one PATH can be given".to_string());
                }
                path = Some(PathBuf::from(arg));
            }
        }
        i += 1;
    }
    if demo.is_some() && path.is_some() {
        return Err("--demo and a PATH cannot be combined".to_string());
    }
    match (top, demo) {
        (Some(top), Some(Some(id))) => Ok(Command::DumpTop {
            top,
            source: DumpSource::Demo(id),
        }),
        (Some(_), Some(None)) => Err("--dump-top with --demo needs a drive id".to_string()),
        (Some(top), None) => Ok(Command::DumpTop {
            top,
            source: DumpSource::Path(path.unwrap_or_else(|| PathBuf::from("."))),
        }),
        (None, Some(id)) => Ok(Command::Demo { id }),
        (None, None) => Ok(Command::App { path }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(&args)
    }

    #[test]
    fn plain_runs_and_paths() {
        assert_eq!(p(&[]), Ok(Command::App { path: None }));
        assert_eq!(
            p(&["/data"]),
            Ok(Command::App {
                path: Some(PathBuf::from("/data"))
            })
        );
        assert_eq!(p(&["--help"]), Ok(Command::Help));
        assert!(p(&["a", "b"]).is_err());
        assert!(p(&["--bogus"]).is_err());
    }

    #[test]
    fn demo_with_and_without_a_drive() {
        assert_eq!(p(&["--demo"]), Ok(Command::Demo { id: None }));
        assert_eq!(
            p(&["--demo", "usb-stick"]),
            Ok(Command::Demo {
                id: Some("usb-stick".into())
            })
        );
        assert!(p(&["--demo", "x", "/path"]).is_err());
    }

    #[test]
    fn dump_top_takes_a_count_and_a_source() {
        assert_eq!(
            p(&["--dump-top", "15", "/home"]),
            Ok(Command::DumpTop {
                top: 15,
                source: DumpSource::Path(PathBuf::from("/home"))
            })
        );
        assert_eq!(
            p(&["--dump-top", "5"]),
            Ok(Command::DumpTop {
                top: 5,
                source: DumpSource::Path(PathBuf::from("."))
            })
        );
        assert_eq!(
            p(&["--dump-top", "3", "--demo", "archive-server"]),
            Ok(Command::DumpTop {
                top: 3,
                source: DumpSource::Demo("archive-server".into())
            })
        );
        assert!(p(&["--dump-top"]).is_err());
        assert!(p(&["--dump-top", "ten"]).is_err());
        assert!(p(&["--dump-top", "3", "--demo"]).is_err());
    }
}
