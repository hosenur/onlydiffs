//! Verdicts decided from a file's name and the lines its change adds.
//!
//! A lockfile is skimmed only when it is version and checksum updates from
//! the usual registries, and needs review the moment it adds a package or a
//! source — the two things a dependency attack changes. (Apple's on-device
//! model, tried for this, called every lockfile "mechanical", including one
//! fetching a package over plain http from a bare IP address.)

use std::collections::HashSet;

use super::{Level, Verdict};

const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "cargo.lock",
    "gemfile.lock",
    "poetry.lock",
    "uv.lock",
    "pipfile.lock",
    "composer.lock",
    "go.sum",
    "package.resolved",
    "mix.lock",
    "pubspec.lock",
    "flake.lock",
];

/// Where the package managers' own registries serve from. A source in an
/// added line that starts with none of these is worth a look.
const REGISTRIES: &[&str] = &[
    "https://registry.npmjs.org/",
    "https://registry.yarnpkg.com/",
    "https://github.com/rust-lang/crates.io-index",
    "https://index.crates.io/",
    "https://static.crates.io/",
    "https://pypi.org/",
    "https://files.pythonhosted.org/",
    "https://rubygems.org/",
    "https://proxy.golang.org/",
    "https://repo.packagist.org/",
    "https://api.github.com/repos/",
];

fn verdict(level: Level, reason: impl Into<String>) -> Verdict {
    Verdict { level, reason: reason.into() }
}

/// A verdict when a rule covers the file.
pub fn judge(path: &str, before: &str, after: &str) -> Option<Verdict> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();

    if before == after {
        return Some(verdict(Level::Skim, "Moved or mode change only; the content is the same."));
    }
    if LOCKFILES.contains(&name.as_str()) {
        return Some(lockfile(&name, before, after));
    }
    if is_asset(&name) {
        return Some(verdict(Level::Skim, "Image or font asset."));
    }
    if is_generated(path, &name, after) {
        return Some(verdict(Level::Skim, "Generated output."));
    }
    if strip_whitespace(before) == strip_whitespace(after) {
        return Some(verdict(Level::Skim, "Whitespace and formatting only."));
    }
    None
}

fn strip_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Vector images and fonts: text git can diff, nothing a person reads line
/// by line. (Raster images are binary and never reach triage.)
fn is_asset(name: &str) -> bool {
    [".svg", ".ico", ".ttf", ".otf", ".woff", ".woff2", ".eot"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

fn is_generated(path: &str, name: &str, after: &str) -> bool {
    let lower = path.to_lowercase();
    if name.ends_with(".min.js") || name.ends_with(".min.css") || name.ends_with(".map") {
        return true;
    }
    if lower.contains("__snapshots__/") || name.ends_with(".snap") {
        return true;
    }
    // The markers code generators write at the top of their output.
    after.lines().take(5).any(|line| {
        let line = line.to_lowercase();
        line.contains("@generated") || line.contains("do not edit") || line.contains("code generated")
    })
}

/// Lines in `after` that are not in `before`.
fn added<'a>(before: &str, after: &'a str) -> Vec<&'a str> {
    let old: HashSet<&str> = before.lines().collect();
    after.lines().filter(|line| !old.contains(line)).collect()
}

fn lockfile(name: &str, before: &str, after: &str) -> Verdict {
    let added = added(before, after);
    for line in &added {
        if line.contains("http://") {
            return verdict(Level::Review, "A package is fetched over plain http.");
        }
        if let Some(source) = outside_source(line) {
            return verdict(Level::Review, format!("A package comes from outside the usual registry: {source}"));
        }
    }
    let new: Vec<String> = {
        let old = package_names(name, before);
        let mut new: Vec<String> = package_names(name, after).difference(&old).cloned().collect();
        new.sort();
        new
    };
    if !new.is_empty() {
        let shown = new.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        let more = if new.len() > 5 { format!(" and {} more", new.len() - 5) } else { String::new() };
        return verdict(Level::Review, format!("Adds dependencies: {shown}{more}."));
    }
    verdict(Level::Skim, "Lockfile: version and checksum updates only.")
}

/// The first URL in `line` that none of the registries serve, if any.
fn outside_source(line: &str) -> Option<String> {
    let mut rest = line;
    while let Some(at) = rest.find("https://") {
        let url: String = rest[at..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ',' | '#' | ')' | ']'))
            .collect();
        if !REGISTRIES.iter().any(|registry| url.starts_with(registry)) {
            return Some(url);
        }
        rest = &rest[at + "https://".len()..];
    }
    None
}

/// The package names a lockfile lists, read the way each format writes them.
/// A version bump keeps the name, so only a genuinely new package differs.
fn package_names(name: &str, text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let found: Option<&str> = match name {
            "cargo.lock" | "poetry.lock" | "uv.lock" => trimmed
                .strip_prefix("name = \"")
                .and_then(|rest| rest.strip_suffix('"')),
            "package-lock.json" | "npm-shrinkwrap.json" => trimmed
                .strip_prefix('"')
                .and_then(|rest| rest.split_once("\":"))
                .map(|(key, _)| key)
                .filter(|key| key.contains("node_modules/"))
                .and_then(|key| key.rsplit("node_modules/").next()),
            "yarn.lock" if !line.starts_with(' ') && trimmed.ends_with(':') => {
                // `"@babel/core@^7.0.0", "@babel/core@^7.1.0":` or `left-pad@^1.3.0:`
                let first = trimmed.trim_end_matches(':').split(", ").next().unwrap_or("");
                without_version(first.trim_matches('"'))
            }
            "pnpm-lock.yaml" if line.starts_with("  ") && !line.starts_with("   ") && trimmed.ends_with(':') => {
                // `  /@types/node@20.14.10:` or `  '@types/node@20.14.10':`
                let key = trimmed.trim_end_matches(':').trim_matches('\'').trim_start_matches('/');
                without_version(key)
            }
            "bun.lock" => trimmed
                .strip_prefix('"')
                .and_then(|rest| rest.split_once("\": [\""))
                .map(|(key, _)| key),
            "go.sum" => trimmed.split_whitespace().next(),
            "gemfile.lock" if line.starts_with("    ") && !line.starts_with("     ") => {
                trimmed.split_whitespace().next()
            }
            "composer.lock" => trimmed
                .strip_prefix("\"name\": \"")
                .and_then(|rest| rest.split_once('"'))
                .map(|(key, _)| key),
            "package.resolved" => trimmed
                .strip_prefix("\"identity\" : \"")
                .and_then(|rest| rest.split_once('"'))
                .map(|(key, _)| key),
            _ => None,
        };
        if let Some(found) = found.filter(|found| !found.is_empty()) {
            names.insert(found.to_owned());
        }
    }
    names
}

/// `@types/node@20.1.0` → `@types/node`; `left-pad@^1.3.0` → `left-pad`.
fn without_version(spec: &str) -> Option<&str> {
    let at = spec[1..].rfind('@')? + 1;
    Some(&spec[..at])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(path: &str, before: &str, after: &str) -> Option<Level> {
        judge(path, before, after).map(|v| v.level)
    }

    #[test]
    fn a_version_bump_in_a_lockfile_is_skimmed() {
        let before = "      \"version\": \"4.17.20\",\n      \"resolved\": \"https://registry.npmjs.org/lodash/-/lodash-4.17.20.tgz\",";
        let after = "      \"version\": \"4.17.21\",\n      \"resolved\": \"https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz\",";
        assert_eq!(level("package-lock.json", before, after), Some(Level::Skim));

        let before = "[[package]]\nname = \"serde\"\nversion = \"1.0.210\"";
        let after = "[[package]]\nname = \"serde\"\nversion = \"1.0.214\"";
        assert_eq!(level("Cargo.lock", before, after), Some(Level::Skim));
    }

    #[test]
    fn a_new_dependency_needs_review() {
        let before = "[[package]]\nname = \"rustls\"\nversion = \"0.23.12\"";
        let after = format!("{before}\n\n[[package]]\nname = \"rustls-helper-unofficial\"\nversion = \"0.0.3\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"");
        let v = judge("Cargo.lock", before, &after).unwrap();
        assert_eq!(v.level, Level::Review);
        assert!(v.reason.contains("rustls-helper-unofficial"), "{}", v.reason);
    }

    #[test]
    fn a_package_from_outside_the_registry_needs_review() {
        let before = "left-pad@^1.3.0:\n  version \"1.3.0\"\n  resolved \"https://registry.yarnpkg.com/left-pad/-/left-pad-1.3.0.tgz#5b8a\"";
        let http = before.replace("https://registry.yarnpkg.com/left-pad/-", "http://45.137.21.9:8080");
        assert_eq!(level("yarn.lock", before, &http), Some(Level::Review));
        let mirror = before.replace("https://registry.yarnpkg.com/", "https://evil.example.com/");
        assert_eq!(level("yarn.lock", before, &mirror), Some(Level::Review));
    }

    #[test]
    fn package_names_are_read_per_format() {
        let names = |file: &str, text: &str| {
            let mut v: Vec<_> = package_names(file, text).into_iter().collect();
            v.sort();
            v
        };
        assert_eq!(names("yarn.lock", "\"@babel/core@^7.0.0\", \"@babel/core@^7.1.0\":\n  version \"7.1.0\"\nleft-pad@^1.3.0:"), ["@babel/core", "left-pad"]);
        assert_eq!(names("pnpm-lock.yaml", "packages:\n  /@types/node@20.14.10:\n    resolution: {}"), ["@types/node"]);
        assert_eq!(names("package-lock.json", "    \"node_modules/a/node_modules/b\": {"), ["b"]);
        assert_eq!(names("bun.lock", "    \"typescript\": [\"typescript@5.6.3\", \"\", {}],"), ["typescript"]);
        assert_eq!(names("go.sum", "golang.org/x/net v0.28.0 h1:abc="), ["golang.org/x/net"]);
    }

    #[test]
    fn whitespace_moves_and_generated_files_are_skimmed() {
        assert_eq!(level("a.rs", "fn a() { 1 }", "fn a() {\n    1\n}"), Some(Level::Skim));
        assert_eq!(level("a.rs", "same", "same"), Some(Level::Skim));
        assert_eq!(level("dist/app.min.js", "a", "b"), Some(Level::Skim));
        assert_eq!(level("src/__snapshots__/x.snap", "a", "b"), Some(Level::Skim));
        assert_eq!(level("gen/api.go", "x", "// Code generated by protoc. DO NOT EDIT.\nx2"), Some(Level::Skim));
        assert_eq!(level("src/table.rs", "", "// Generated by a script.\n// Do not edit by hand.\nx"), Some(Level::Skim));
        assert_eq!(level("assets/icons/rust.svg", "", "<svg/>"), Some(Level::Skim));
        assert_eq!(level("assets/fonts/Geist.woff2", "", "x"), Some(Level::Skim));
    }

    #[test]
    fn real_code_gets_no_verdict() {
        assert_eq!(level("src/a.rs", "const N: u32 = 5;", "const N: u32 = 1;"), None);
        assert_eq!(level("README.md", "npm instal", "npm install"), None);
    }
}
