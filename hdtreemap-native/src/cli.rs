//! Command-line parsing for the `hdtreemap` binary.
//!
//! ```text
//! hdtreemap [PATH]                    open the app (scanning PATH straight away if given)
//! hdtreemap --demo [ID]               the simulated drives of the web demo (scan ID now)
//! hdtreemap --dump-top N [PATH]       scan PATH (default: .) headless, print the top N
//! hdtreemap --dump-top N --demo ID    the same for a simulated drive
//! hdtreemap --help
//! ```
//!
//! `--highlight EXT` (with the windowed app) highlights extension EXT on the map once the
//! scan finishes, as a legend click would; handy for screenshots.

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

  --highlight EXT                    With a scan: highlight EXT's files on the map

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
        highlight: Option<String>,
    },
    /// The windowed app on the simulated demo machine, optionally scanning drive `id`.
    Demo {
        id: Option<String>,
        highlight: Option<String>,
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
    let mut highlight: Option<String> = None;
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
            "--highlight" => {
                let ext = args.get(i + 1).ok_or("--highlight needs an extension")?;
                let ext = ext.trim_start_matches('.').to_lowercase();
                if ext.is_empty() {
                    return Err("--highlight needs an extension".to_string());
                }
                highlight = Some(ext);
                i += 1;
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
            _ => {
                if path.is_some() {
                    return Err("only one PATH can be given".to_string());
                }
                path = Some(absolute(arg));
            }
        }
        i += 1;
    }
    if demo.is_some() && path.is_some() {
        return Err("--demo and a PATH cannot be combined".to_string());
    }
    if top.is_some() && highlight.is_some() {
        return Err("--highlight is for the windowed app, not --dump-top".to_string());
    }
    match (top, demo) {
        (Some(top), Some(Some(id))) => Ok(Command::DumpTop {
            top,
            source: DumpSource::Demo(id),
        }),
        (Some(_), Some(None)) => Err("--dump-top with --demo needs a drive id".to_string()),
        (Some(top), None) => Ok(Command::DumpTop {
            top,
            source: DumpSource::Path(path.unwrap_or_else(|| absolute("."))),
        }),
        (None, Some(id)) => Ok(Command::Demo { id, highlight }),
        (None, None) => Ok(Command::App { path, highlight }),
    }
}

/// `arg` as an absolute path (relative to the working directory), so a name such as
/// `-rf` can never reach a command line as an option. The scanner canonicalises it too.
fn absolute(arg: &str) -> PathBuf {
    std::path::absolute(arg).unwrap_or_else(|_| PathBuf::from(arg))
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
        assert_eq!(
            p(&[]),
            Ok(Command::App {
                path: None,
                highlight: None
            })
        );
        assert_eq!(
            p(&["/data"]),
            Ok(Command::App {
                path: Some(absolute("/data")),
                highlight: None
            })
        );
        assert_eq!(p(&["--help"]), Ok(Command::Help));
        assert!(p(&["a", "b"]).is_err());
        assert!(p(&["--bogus"]).is_err());
    }

    #[test]
    fn relative_and_dash_paths_become_absolute() {
        let cwd = std::env::current_dir().expect("cwd");
        let Ok(Command::App {
            path: Some(path), ..
        }) = p(&["-rf"])
        else {
            panic!("a dash name is a path");
        };
        assert_eq!(path, cwd.join("-rf"));
        let Ok(Command::App {
            path: Some(path), ..
        }) = p(&["photos"])
        else {
            panic!("a relative path");
        };
        assert_eq!(path, cwd.join("photos"));
    }

    #[test]
    fn demo_with_and_without_a_drive() {
        assert_eq!(
            p(&["--demo"]),
            Ok(Command::Demo {
                id: None,
                highlight: None
            })
        );
        assert_eq!(
            p(&["--demo", "usb-stick"]),
            Ok(Command::Demo {
                id: Some("usb-stick".into()),
                highlight: None
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
                source: DumpSource::Path(absolute("/home"))
            })
        );
        assert_eq!(
            p(&["--dump-top", "5"]),
            Ok(Command::DumpTop {
                top: 5,
                source: DumpSource::Path(absolute("."))
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

    #[test]
    fn highlight_names_an_extension_for_the_app() {
        assert_eq!(
            p(&["--demo", "dev-workstation", "--highlight", ".JS"]),
            Ok(Command::Demo {
                id: Some("dev-workstation".into()),
                highlight: Some("js".into())
            })
        );
        assert_eq!(
            p(&["/data", "--highlight", "mov"]),
            Ok(Command::App {
                path: Some(absolute("/data")),
                highlight: Some("mov".into())
            })
        );
        assert!(p(&["--highlight"]).is_err());
        assert!(p(&["--highlight", "."]).is_err());
        assert!(p(&["--dump-top", "3", "--highlight", "js"]).is_err());
    }
}
