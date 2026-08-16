//! AC-49 — the kernel stays pure.
//!
//! `boids-core` must never depend on the web framework, the workflow engine,
//! a database driver, or an HTTP client. That rule is what makes the whole
//! simulation testable without infrastructure and reusable outside this
//! application, and a rule that is only written down is a rule that erodes.
//! These tests read `boids-core/Cargo.toml` as text and fail the build if a
//! forbidden dependency ever appears.
//!
//! The manifest is parsed by hand rather than with a `toml` crate, because
//! adding a dependency in order to police dependencies would be its own kind
//! of joke.

/// Crates the kernel must never depend on, matched as crate *families*: a
/// name matches if it equals an entry or extends it with a `-` suffix, so
/// `autumn-harvest-macros` is caught by `autumn-harvest`.
const FORBIDDEN: [&str; 8] = [
    // The web framework and its HTTP stack.
    "autumn-web",
    "axum",
    // The durable workflow engine: the workflow layer depends on the kernel,
    // so a dependency back would be an architectural cycle.
    "autumn-harvest",
    // Database drivers.
    "diesel",
    "sqlx",
    // Async runtime: scheduling nondeterminism breaks reproducibility.
    "tokio",
    // HTTP client: the kernel performs no IO.
    "reqwest",
    // Ambient entropy defeats seeded reproducibility (AC-5).
    "rand",
];

/// TOML tables whose keys name dependencies.
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Keys that appear *inside* a dependency specification.
///
/// A multi-line inline table puts these at the start of a line, where they
/// would otherwise look like dependency names.
const SPEC_KEYS: [&str; 12] = [
    "version",
    "features",
    "workspace",
    "optional",
    "default-features",
    "path",
    "git",
    "branch",
    "tag",
    "rev",
    "package",
    "registry",
];

/// Cargo treats `-` and `_` as equivalent in crate names, so the guard must
/// too or it could be side-stepped by a spelling.
fn normalize(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

/// Drop a trailing `# comment`, ignoring `#` inside quoted strings (git URLs
/// with a fragment are the realistic case).
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..i],
            None => {}
        }
    }
    line
}

/// Split a table header on `.`, keeping quoted segments intact so that
/// `target."cfg(unix)".dependencies` yields three segments, not four.
fn split_header(header: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in header.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '.' => parts.push(std::mem::take(&mut current).trim().to_string()),
            None => current.push(c),
        }
    }
    parts.push(current.trim().to_string());
    parts
}

/// Every dependency name declared anywhere in a manifest.
///
/// Handles the three dependency tables, `[dependencies.name]` sub-tables, and
/// `[target.'cfg(..)'.dependencies]`.
fn declared_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dependency_table = false;

    for raw_line in manifest.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        // A table header re-establishes where we are.
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let segments = split_header(header.trim());
            let table = segments
                .iter()
                .position(|s| DEPENDENCY_TABLES.contains(&normalize(s).as_str()));
            in_dependency_table = false;
            match table {
                // `[dependencies]` — the keys that follow are dependencies.
                Some(i) if i + 1 == segments.len() => in_dependency_table = true,
                // `[dependencies.serde]` — the name is in the header itself,
                // and the keys that follow are its specification.
                Some(i) => deps.push(segments[i + 1].clone()),
                None => {}
            }
            continue;
        }

        if in_dependency_table && let Some((key, _)) = line.split_once('=') {
            let name = key.trim().trim_matches(['"', '\'']).trim();
            if !name.is_empty() && !SPEC_KEYS.contains(&normalize(name).as_str()) {
                deps.push(name.to_string());
            }
        }
    }
    deps
}

/// Declared dependencies that belong to a forbidden crate family.
fn banned_dependencies(manifest: &str) -> Vec<String> {
    declared_dependencies(manifest)
        .into_iter()
        .filter(|dep| {
            let dep = normalize(dep);
            FORBIDDEN
                .iter()
                .any(|f| dep == *f || dep.starts_with(&format!("{f}-")))
        })
        .collect()
}

mod manifest_parsing {
    use super::{banned_dependencies, declared_dependencies};

    #[test]
    fn finds_dependencies_in_every_dependency_table() {
        let manifest = r#"
[package]
name = "boids-core"
version = "0.1.0"

[dependencies]
serde = { workspace = true }
axum = "0.7"

[dev-dependencies]
serde_json = "1"

[build-dependencies]
diesel = "2"
"#;
        let deps = declared_dependencies(manifest);
        assert!(deps.contains(&"serde".to_string()), "{deps:?}");
        assert!(deps.contains(&"axum".to_string()), "{deps:?}");
        assert!(deps.contains(&"serde_json".to_string()), "{deps:?}");
        assert!(deps.contains(&"diesel".to_string()), "{deps:?}");
    }

    #[test]
    fn finds_dependencies_declared_as_their_own_tables() {
        let manifest = r#"
[dependencies.tokio]
version = "1"
features = ["full"]

[dev-dependencies.sqlx]
version = "0.7"
"#;
        let deps = declared_dependencies(manifest);
        assert!(deps.contains(&"tokio".to_string()), "{deps:?}");
        assert!(deps.contains(&"sqlx".to_string()), "{deps:?}");
        // The keys *inside* a dependency's own table are not dependencies.
        assert!(!deps.contains(&"version".to_string()), "{deps:?}");
        assert!(!deps.contains(&"features".to_string()), "{deps:?}");
    }

    #[test]
    fn finds_target_specific_dependencies() {
        let manifest = r#"
[target."cfg(unix)".dependencies]
reqwest = "0.12"
"#;
        let deps = declared_dependencies(manifest);
        assert!(deps.contains(&"reqwest".to_string()), "{deps:?}");
    }

    #[test]
    fn ignores_non_dependency_tables() {
        let manifest = r#"
[package]
name = "boids-core"

[lints.rust]
unsafe_code = "forbid"

[profile.test]
incremental = false
"#;
        let deps = declared_dependencies(manifest);
        assert!(deps.is_empty(), "non-dependency tables leaked: {deps:?}");
    }

    #[test]
    fn ignores_commented_out_dependencies() {
        // A commented dependency is not a dependency; treating it as one
        // would make the guard cry wolf and get it deleted.
        let manifest = r#"
[dependencies]
serde = { workspace = true }
# axum = "0.7"
tokio = "1" # this one is real
"#;
        let deps = declared_dependencies(manifest);
        assert!(!deps.contains(&"axum".to_string()), "{deps:?}");
        assert!(deps.contains(&"tokio".to_string()), "{deps:?}");
    }

    #[test]
    fn a_hash_inside_a_string_does_not_start_a_comment() {
        let manifest = r#"
[dependencies]
weird = { git = "https://example.com/repo#branch" }
serde = "1"
"#;
        let deps = declared_dependencies(manifest);
        assert!(deps.contains(&"serde".to_string()), "{deps:?}");
        assert!(deps.contains(&"weird".to_string()), "{deps:?}");
    }

    #[test]
    fn detects_every_forbidden_dependency() {
        // The guard must actually fire. Without this, a parser bug would
        // make the real assertion below silently vacuous.
        for name in [
            "autumn-web",
            "autumn-harvest",
            "diesel",
            "axum",
            "tokio",
            "reqwest",
            "sqlx",
            "rand",
        ] {
            let manifest = format!("[dependencies]\n{name} = \"1\"\n");
            let found = banned_dependencies(&manifest);
            assert_eq!(found, vec![name.to_string()], "failed to detect {name}");
        }
    }

    #[test]
    fn detects_forbidden_dependencies_by_family_prefix() {
        // `autumn-harvest-macros` pulls in the workflow engine just as surely
        // as `autumn-harvest` does.
        let manifest = "[dependencies]\nautumn-harvest-macros = \"0.5\"\n";
        assert_eq!(
            banned_dependencies(manifest),
            vec!["autumn-harvest-macros".to_string()]
        );
    }

    #[test]
    fn treats_underscores_and_hyphens_as_the_same_crate() {
        // Cargo does; a guard that did not could be trivially side-stepped.
        let manifest = "[dependencies]\nautumn_web = \"0.6\"\n";
        assert_eq!(
            banned_dependencies(manifest),
            vec!["autumn_web".to_string()]
        );
    }

    #[test]
    fn a_pure_manifest_reports_nothing() {
        let manifest = r#"
[dependencies]
serde = { workspace = true }

[dev-dependencies]
serde_json = { workspace = true }
"#;
        assert!(banned_dependencies(manifest).is_empty());
    }
}

/// AC-49: the real manifest declares no forbidden dependency.
#[test]
fn boids_core_declares_no_forbidden_dependency() {
    let manifest = read_own_manifest();
    let found = banned_dependencies(&manifest);
    assert!(
        found.is_empty(),
        "boids-core declares forbidden dependencies: {found:?}\n\n\
         boids-core is the pure simulation kernel. It must never depend on the web \
         framework (autumn-web, axum), the workflow engine (autumn-harvest), a database \
         driver (diesel, sqlx), an async runtime (tokio), an HTTP client (reqwest), or an \
         ambient RNG (rand).\n\n\
         Why this rule exists:\n\
         - The kernel is tested without a database, a server, or a runtime. Every one of \
           these crates would drag infrastructure into a unit test.\n\
         - Reproducibility (AC-22) requires that a scenario replay bit-identically in a \
           separate process. An async runtime introduces scheduling nondeterminism and \
           `rand` introduces ambient entropy; either would break that contract.\n\
         - The workflow layer depends on the kernel, so a dependency back on the workflow \
           engine would be a cycle in the architecture, not merely extra weight.\n\n\
         If the kernel appears to need one of these, the need belongs in the `boidboard` \
         crate instead: keep the kernel a pure value transformation and let the caller own \
         the IO."
    );
}

/// A dependency added here is an architectural decision, so it should require
/// a deliberate edit to this list rather than passing unnoticed.
#[test]
fn boids_core_dependencies_are_on_the_approved_list() {
    // Normalized spellings (hyphenated), to match `normalize`.
    const APPROVED: [&str; 2] = [
        // Frame and scenario serialization; the kernel defines the shapes,
        // the caller decides where the bytes go.
        "serde",
        // Dev-only: round-trip assertions in the kernel's own tests.
        "serde-json",
    ];

    let manifest = read_own_manifest();
    let unexpected: Vec<String> = declared_dependencies(&manifest)
        .into_iter()
        .filter(|d| !APPROVED.contains(&normalize(d).as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "boids-core declares unapproved dependencies: {unexpected:?}\n\n\
         The kernel's dependency list is deliberately near-empty: it is a pure value \
         transformation, and every crate added here is a crate that a downstream consumer \
         of the simulation is forced to take. If this dependency genuinely belongs in the \
         kernel, add it to APPROVED in {} with a comment justifying it. If it exists to do \
         IO, talk to a database, or serve HTTP, it belongs in the `boidboard` crate.",
        file!()
    );
}

/// Read this crate's own manifest.
///
/// `CARGO_MANIFEST_DIR` is set by Cargo, so the test does not depend on the
/// working directory it happens to be invoked from.
fn read_own_manifest() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}
