/// Resolve the `DATABASE_URL` environment variable.
///
/// Returns an error if the variable is not set so operators are forced to
/// make an explicit choice and cannot silently fall back to the wrong
/// database.
pub fn resolve_database_url() -> Result<String, String> {
    let url = std::env::var("DATABASE_URL").map_err(|_| {
        "DATABASE_URL is not set. \
         Set it explicitly, e.g. DATABASE_URL=postgres://user@localhost:5432/limenet"
            .to_string()
    })?;

    if url.trim().is_empty() {
        return Err(
            "DATABASE_URL is set but empty. \
             Provide a valid connection string, e.g. postgres://user@localhost:5432/limenet"
                .to_string(),
        );
    }

    Ok(url)
}

/// Produce a safe, operator-readable description of the database target.
///
/// Credentials are stripped so the value can be logged safely.
/// Input is expected to be a PostgreSQL connection URI such as
/// `postgres://user:pass@host:port/dbname?params`.
///
/// Returns `host:port/dbname` when parsing succeeds, or the original
/// string when it does not.
pub fn display_database_target(database_url: &str) -> String {
    let without_prefix = database_url
        .strip_prefix("postgres://")
        .or_else(|| database_url.strip_prefix("postgresql://"))
        .unwrap_or(database_url);

    // Everything after '@' is the host portion; if there is no '@' the
    // whole remainder is the host portion.
    let host_and_rest = without_prefix
        .split_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(without_prefix);

    // Split host:port from path/query. We only care about the first '/'.
    let (host_port, dbname) = match host_and_rest.split_once('/') {
        Some((hp, rest)) => {
            // dbname may contain query parameters; strip them.
            let db = rest.split_once('?').map(|(d, _)| d).unwrap_or(rest);
            (hp, db)
        }
        None => (host_and_rest, ""),
    };

    if dbname.is_empty() {
        host_port.to_string()
    } else {
        format!("{}/{}", host_port, dbname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-var mutation tests to avoid data races.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_database_url_missing_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = TestEnvGuard::new("DATABASE_URL");
        let result = resolve_database_url();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("DATABASE_URL is not set"),
            "error message should mention the missing variable: {}",
            msg
        );
    }

    #[test]
    fn resolve_database_url_present() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = TestEnvGuard::with_value("DATABASE_URL", "postgres://u@h:5432/d");
        let result = resolve_database_url();
        assert_eq!(result.unwrap(), "postgres://u@h:5432/d");
    }

    #[test]
    fn resolve_database_url_empty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = TestEnvGuard::with_value("DATABASE_URL", "");
        let result = resolve_database_url();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("empty"),
            "error message should mention empty value: {}",
            msg
        );
    }

    #[test]
    fn display_target_with_credentials() {
        let url = "postgres://user:secret@db.example.com:5432/limenet";
        assert_eq!(display_database_target(url), "db.example.com:5432/limenet");
    }

    #[test]
    fn display_target_without_credentials() {
        let url = "postgres://user@localhost:5432/postgres";
        assert_eq!(display_database_target(url), "localhost:5432/postgres");
    }

    #[test]
    fn display_target_with_query_params() {
        let url = "postgres://user:pass@host:5432/db?sslmode=require";
        assert_eq!(display_database_target(url), "host:5432/db");
    }

    #[test]
    fn display_target_no_dbname() {
        let url = "postgres://user:pass@host:5432";
        assert_eq!(display_database_target(url), "host:5432");
    }

    #[test]
    fn display_target_no_port() {
        let url = "postgres://user@localhost/mydb";
        assert_eq!(display_database_target(url), "localhost/mydb");
    }

    #[test]
    fn display_target_ipv6() {
        let url = "postgres://user@[::1]:5432/mydb";
        assert_eq!(display_database_target(url), "[::1]:5432/mydb");
    }

    /// Temporarily removes (or overrides) an environment variable for the
    /// duration of a test, restoring the original value on drop.
    struct TestEnvGuard {
        key: String,
        original: Option<String>,
    }

    impl TestEnvGuard {
        fn new(key: &str) -> Self {
            let original = std::env::var(key).ok();
            // SAFETY: only used in single-threaded tests.
            unsafe { std::env::remove_var(key) };
            Self {
                key: key.to_string(),
                original,
            }
        }

        fn with_value(key: &str, value: &str) -> Self {
            let original = std::env::var(key).ok();
            // SAFETY: only used in single-threaded tests.
            unsafe { std::env::set_var(key, value) };
            Self {
                key: key.to_string(),
                original,
            }
        }
    }

    impl Drop for TestEnvGuard {
        fn drop(&mut self) {
            // SAFETY: only used in single-threaded tests.
            unsafe {
                match &self.original {
                    Some(v) => std::env::set_var(&self.key, v),
                    None => std::env::remove_var(&self.key),
                }
            }
        }
    }
}
