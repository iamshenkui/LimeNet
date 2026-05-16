use url::Url;

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

    validate_database_url(&url)
}

fn validate_database_url(url: &str) -> Result<String, String> {
    if url.trim().is_empty() {
        return Err(
            "DATABASE_URL is set but empty. \
             Provide a valid connection string, e.g. postgres://user@localhost:5432/limenet"
                .to_string(),
        );
    }

    Ok(url.to_string())
}

/// Produce a safe, operator-readable description of the database target.
///
/// Credentials are stripped so the value can be logged safely.
/// Input is expected to be a PostgreSQL connection URI such as
/// `postgres://user:pass@host:port/dbname?params`.
///
/// Returns a redacted `host:port/dbname` string when parsing succeeds. Safe
/// query params that distinguish isolated instances (such as `search_path`)
/// are preserved. If the URL is malformed, a generic placeholder is returned
/// rather than echoing potentially sensitive input.
pub fn display_database_target(database_url: &str) -> String {
    let Ok(parsed) = Url::parse(database_url) else {
        return "<invalid database target>".to_string();
    };

    let host = parsed.host_str().unwrap_or("<unknown-host>");
    let mut target = if let Some(port) = parsed.port() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };

    let dbname = parsed.path().trim_start_matches('/');
    if !dbname.is_empty() {
        target.push('/');
        target.push_str(dbname);
    }

    if let Some(query) = safe_query_suffix(&parsed) {
        target.push('?');
        target.push_str(&query);
    }

    target
}

fn safe_query_suffix(parsed: &Url) -> Option<String> {
    let mut safe_pairs: Vec<String> = Vec::new();

    for (key, value) in parsed.query_pairs() {
        if key == "search_path" {
            safe_pairs.push(format!("search_path={value}"));
            continue;
        }

        if key == "options" && value.contains("search_path") {
            safe_pairs.push(format!("options={value}"));
        }
    }

    if safe_pairs.is_empty() {
        None
    } else {
        Some(safe_pairs.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve_database_url_missing_var() {
        let result = validate_database_url("");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("empty"),
            "error message should mention the empty value: {}",
            msg
        );
    }

    #[test]
    fn resolve_database_url_present() {
        let result = validate_database_url("postgres://u@h:5432/d");
        assert_eq!(result.unwrap(), "postgres://u@h:5432/d");
    }

    #[test]
    fn resolve_database_url_empty() {
        let result = validate_database_url("");
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
    fn display_target_preserves_search_path_query_param() {
        let url = "postgres://user:pass@host:5432/db?search_path=local_tasks";
        assert_eq!(display_database_target(url), "host:5432/db?search_path=local_tasks");
    }

    #[test]
    fn display_target_preserves_safe_options_query_param() {
        let url = "postgres://user:pass@host:5432/db?options=-csearch_path%3Dlocal_tasks&sslmode=require";
        assert_eq!(display_database_target(url), "host:5432/db?options=-csearch_path=local_tasks");
    }

    #[test]
    fn display_target_malformed_url_does_not_echo_credentials() {
        let url = "postgres://user:secret@host:5432:bad";
        assert_eq!(display_database_target(url), "<invalid database target>");
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
}
