//! Logging support for `hx`.
//!
//! A single [`log::Log`] implementation serves the whole process. Records go to
//! the main log, except records about one language server (those whose target
//! starts with [`helix_lsp::LOG_TARGET_PREFIX`]), which go to that server's own
//! file in the LSP log directory.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

struct Logger {
    level: log::LevelFilter,
    main: Mutex<Box<dyn Write + Send>>,
    lsp: Option<LspLogs>,
}

/// One log file per language server, opened on first use.
struct LspLogs {
    dir: PathBuf,
    /// Keyed by server name. `None` records that the file could not be opened.
    files: Mutex<HashMap<String, Option<File>>>,
}

impl Logger {
    fn write_main(&self, line: &str) {
        let mut main = self.main.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = main.write_all(line.as_bytes());
    }

    /// Writes `record` to the log file of the language server `name`. Returns
    /// `false` when the record could not be routed and belongs in the main log.
    fn write_lsp(&self, name: &str, record: &log::Record) -> bool {
        let Some(lsp) = &self.lsp else {
            return false;
        };
        let mut files = lsp.files.lock().unwrap_or_else(PoisonError::into_inner);
        let file = files
            .entry(name.to_owned())
            .or_insert_with(|| self.open_lsp_file(&lsp.dir, name));
        match file {
            Some(file) => {
                let _ = file.write_all(format_line(record, None).as_bytes());
                true
            }
            None => false,
        }
    }

    fn open_lsp_file(&self, dir: &Path, name: &str) -> Option<File> {
        let path = lsp_log_file_in(dir, name);
        match open_append(&path) {
            Ok(file) => Some(file),
            Err(err) => {
                self.write_main(&format!(
                    "{} {} [WARN] could not open {}: {err}; logging {name} here instead\n",
                    log_timestamp(),
                    module_path!(),
                    path.display(),
                ));
                None
            }
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Some(name) = record.target().strip_prefix(helix_lsp::LOG_TARGET_PREFIX) {
            if self.write_lsp(name, record) {
                return;
            }
        }
        self.write_main(&format_line(record, Some(record.target())));
    }

    fn flush(&self) {
        let mut main = self.main.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = main.flush();
        if let Some(lsp) = &self.lsp {
            let mut files = lsp.files.lock().unwrap_or_else(PoisonError::into_inner);
            for file in files.values_mut().flatten() {
                let _ = file.flush();
            }
        }
    }
}

/// Formats one log line. The target is omitted in per-server files, where it
/// would only repeat the file name.
fn format_line(record: &log::Record, target: Option<&str>) -> String {
    let timestamp = log_timestamp();
    let level = record.level();
    let args = record.args();
    match target {
        Some(target) => format!("{timestamp} {target} [{level}] {args}\n"),
        None => format!("{timestamp} [{level}] {args}\n"),
    }
}

fn open_append(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    File::options().append(true).create(true).open(path)
}

fn lsp_log_file_in(dir: &Path, name: &str) -> PathBuf {
    let file_name: String = name
        .chars()
        .map(|c| match c {
            '-' | '_' | '.' => c,
            c if c.is_alphanumeric() => c,
            _ => '_',
        })
        .collect();
    dir.join(format!("{file_name}.log"))
}

/// The log file of the language server `name`.
pub fn lsp_log_file(name: &str) -> PathBuf {
    lsp_log_file_in(&helix_loader::lsp_log_dir(), name)
}

fn install(logger: Logger) -> Result<(), log::SetLoggerError> {
    let level = logger.level;
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(level);
    Ok(())
}

/// Install the global logger. The main log goes to `path` and each language
/// server's log to its own file in `lsp_dir`; files are created if absent and
/// appended to.
pub fn init_file(level: log::LevelFilter, path: &Path, lsp_dir: PathBuf) -> std::io::Result<()> {
    let main = open_append(path)?;
    install(Logger {
        level,
        main: Mutex::new(Box::new(main)),
        lsp: Some(LspLogs {
            dir: lsp_dir,
            files: Mutex::new(HashMap::new()),
        }),
    })
    .map_err(std::io::Error::other)
}

/// Install the global logger writing everything to stdout (used by integration tests).
#[cfg(feature = "integration")]
pub fn init_stdout(level: log::LevelFilter) {
    let _ = install(Logger {
        level,
        main: Mutex::new(Box::new(std::io::stdout())),
        lsp: None,
    });
}

/// RFC3339-style UTC timestamp for a log line: `YYYY-MM-DDTHH:MM:SS.mmm`.
pub fn log_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_timestamp(now.as_secs(), now.subsec_millis())
}

fn format_timestamp(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (hour, min, sec) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}")
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch (1970-01-01,
/// UTC) to `(year, month, day)`. Exact for the whole proleptic Gregorian range.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log as _;
    use std::sync::Arc;

    #[test]
    fn timestamp_matches_known_instants() {
        // epoch
        assert_eq!(format_timestamp(0, 0), "1970-01-01T00:00:00.000");
        // next day, and time-of-day + millis
        assert_eq!(
            format_timestamp(86_400 + 3661, 7),
            "1970-01-02T01:01:01.007"
        );
        // a leap day: 2024-02-29T12:30:45.123 UTC == 1709209845 s
        assert_eq!(
            format_timestamp(1_709_209_845, 123),
            "2024-02-29T12:30:45.123"
        );
        // 2000-03-01 (the algorithm's era boundary)
        assert_eq!(format_timestamp(951_868_800, 0), "2000-03-01T00:00:00.000");
    }

    #[test]
    fn lsp_log_file_names_are_sanitized() {
        let dir = Path::new("/logs");
        assert_eq!(
            lsp_log_file_in(dir, "rust-analyzer"),
            Path::new("/logs/rust-analyzer.log")
        );
        assert_eq!(
            lsp_log_file_in(dir, "my/odd server"),
            Path::new("/logs/my_odd_server.log")
        );
    }

    /// A `Write` sink that can be inspected after being boxed into the logger.
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Shared {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn record<'a>(
        target: &'a str,
        level: log::Level,
        args: std::fmt::Arguments<'a>,
    ) -> log::Record<'a> {
        log::Record::builder()
            .target(target)
            .level(level)
            .args(args)
            .build()
    }

    #[test]
    fn language_server_records_go_to_their_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let lsp_dir = dir.path().join("lsp");
        let main = Shared::default();
        let logger = Logger {
            level: log::LevelFilter::Info,
            main: Mutex::new(Box::new(main.clone())),
            lsp: Some(LspLogs {
                dir: lsp_dir.clone(),
                files: Mutex::new(HashMap::new()),
            }),
        };

        logger.log(&record(
            "helix_term::application",
            log::Level::Warn,
            format_args!("editor event"),
        ));
        logger.log(&record(
            &helix_lsp::log_target("rust-analyzer"),
            log::Level::Info,
            format_args!("<- {{}}"),
        ));
        logger.log(&record(
            &helix_lsp::log_target("rust-analyzer"),
            log::Level::Debug,
            format_args!("filtered out"),
        ));

        let main = main.contents();
        assert!(main.contains("helix_term::application [WARN] editor event\n"));
        assert!(!main.contains("<- {}"));

        let server = std::fs::read_to_string(lsp_dir.join("rust-analyzer.log")).unwrap();
        assert!(server.ends_with(" [INFO] <- {}\n"), "{server:?}");
        assert!(!server.contains("lsp/"));
        assert!(!server.contains("filtered out"));
    }

    #[test]
    fn falls_back_to_main_log_when_server_file_cannot_be_opened() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("lsp");
        std::fs::write(&blocker, "not a directory").unwrap();
        let main = Shared::default();
        let logger = Logger {
            level: log::LevelFilter::Info,
            main: Mutex::new(Box::new(main.clone())),
            lsp: Some(LspLogs {
                dir: blocker,
                files: Mutex::new(HashMap::new()),
            }),
        };

        for _ in 0..2 {
            logger.log(&record(
                &helix_lsp::log_target("gopls"),
                log::Level::Error,
                format_args!("stderr <- \"boom\""),
            ));
        }

        let main = main.contents();
        assert_eq!(main.matches("could not open").count(), 1, "{main:?}");
        assert_eq!(
            main.matches("lsp/gopls [ERROR] stderr <- \"boom\"").count(),
            2,
            "{main:?}"
        );
    }
}
