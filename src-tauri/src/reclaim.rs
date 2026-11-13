//! The "Reclaimable" lens: a static rule table that recognises well-known
//! caches and build output in a finished scan. It only reports what it finds
//! and the command a person could run; nothing here deletes or executes anything.

use crate::model::{NodeKind, ScanNode, ScanTree};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Matcher {
    /// Exactly this absolute path.
    Absolute(&'static str),
    /// A path below the user's home folder.
    Home(&'static str),
    /// A directory with this name anywhere in the tree.
    Name(&'static str),
}

#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub matchers: &'static [Matcher],
    /// i18n keys; the interface owns the wording.
    pub title_key: &'static str,
    pub explanation_key: &'static str,
    pub command: Option<&'static str>,
    /// The command is destructive enough to be shown with a warning.
    pub warning: bool,
    /// The real saving cannot be known in advance, so the size is not in the total.
    pub uncertain: bool,
    /// Matches inside this folder are reported separately and only the rest is counted here.
    pub container: bool,
}

use Matcher::{Absolute, Home, Name};

pub static RULES: &[Rule] = &[
    Rule {
        id: "user_cache",
        matchers: &[Home(".cache")],
        title_key: "reclaimUserCacheTitle",
        explanation_key: "reclaimCacheExplanation",
        command: None,
        warning: false,
        uncertain: false,
        container: true,
    },
    Rule {
        id: "thumbnails",
        matchers: &[Home(".cache/thumbnails")],
        title_key: "reclaimThumbnailsTitle",
        explanation_key: "reclaimCacheExplanation",
        command: None,
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "trash",
        matchers: &[Home(".local/share/Trash")],
        title_key: "reclaimTrashTitle",
        explanation_key: "reclaimTrashExplanation",
        command: None,
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "pacman_cache",
        matchers: &[Absolute("/var/cache/pacman/pkg")],
        title_key: "reclaimPacmanTitle",
        explanation_key: "reclaimPackageCacheExplanation",
        command: Some("sudo paccache -rk2"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "apt_cache",
        matchers: &[Absolute("/var/cache/apt/archives")],
        title_key: "reclaimAptTitle",
        explanation_key: "reclaimPackageCacheExplanation",
        command: Some("sudo apt clean"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "dnf_cache",
        matchers: &[Absolute("/var/cache/dnf"), Absolute("/var/cache/libdnf5")],
        title_key: "reclaimDnfTitle",
        explanation_key: "reclaimPackageCacheExplanation",
        command: Some("sudo dnf clean all"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "journal",
        matchers: &[Absolute("/var/log/journal")],
        title_key: "reclaimJournalTitle",
        explanation_key: "reclaimJournalExplanation",
        command: Some("sudo journalctl --vacuum-size=200M"),
        warning: false,
        uncertain: true,
        container: false,
    },
    Rule {
        id: "coredumps",
        matchers: &[Absolute("/var/lib/systemd/coredump")],
        title_key: "reclaimCoredumpTitle",
        explanation_key: "reclaimCoredumpExplanation",
        command: Some("sudo rm /var/lib/systemd/coredump/*"),
        warning: true,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "npm_cache",
        matchers: &[Home(".npm/_cacache")],
        title_key: "reclaimNpmTitle",
        explanation_key: "reclaimDownloadsExplanation",
        command: Some("npm cache clean --force"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "pnpm_store",
        matchers: &[Home(".local/share/pnpm/store")],
        title_key: "reclaimPnpmTitle",
        explanation_key: "reclaimDownloadsExplanation",
        command: Some("pnpm store prune"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "yarn_cache",
        matchers: &[Home(".cache/yarn")],
        title_key: "reclaimYarnTitle",
        explanation_key: "reclaimDownloadsExplanation",
        command: Some("yarn cache clean"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "pip_cache",
        matchers: &[Home(".cache/pip")],
        title_key: "reclaimPipTitle",
        explanation_key: "reclaimDownloadsExplanation",
        command: Some("pip cache purge"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "cargo_cache",
        matchers: &[Home(".cargo/registry/cache"), Home(".cargo/git/checkouts")],
        title_key: "reclaimCargoTitle",
        explanation_key: "reclaimCargoExplanation",
        command: None,
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "rust_target",
        matchers: &[Name("target")],
        title_key: "reclaimRustTargetTitle",
        explanation_key: "reclaimRustTargetExplanation",
        command: Some("cargo clean --manifest-path {manifest}"),
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "node_modules",
        matchers: &[Name("node_modules")],
        title_key: "reclaimNodeModulesTitle",
        explanation_key: "reclaimNodeModulesExplanation",
        command: None,
        warning: false,
        uncertain: false,
        container: false,
    },
    Rule {
        id: "docker",
        matchers: &[Absolute("/var/lib/docker")],
        title_key: "reclaimDockerTitle",
        explanation_key: "reclaimDockerExplanation",
        command: Some("docker system prune"),
        warning: false,
        uncertain: true,
        container: false,
    },
    Rule {
        id: "flatpak",
        matchers: &[Absolute("/var/lib/flatpak"), Home(".local/share/flatpak")],
        title_key: "reclaimFlatpakTitle",
        explanation_key: "reclaimFlatpakExplanation",
        command: Some("flatpak uninstall --unused"),
        warning: false,
        uncertain: true,
        container: false,
    },
];

/// What the matcher needs to know about one directory beyond its path.
#[derive(Debug, Clone, Copy)]
pub struct DirFacts<'a> {
    pub path: &'a Path,
    pub name: &'a str,
    pub has_cachedir_tag: bool,
    pub has_cargo_toml_sibling: bool,
    pub has_package_json_sibling: bool,
}

fn matcher_hits(matcher: &Matcher, facts: &DirFacts, home: Option<&Path>) -> bool {
    match matcher {
        Absolute(path) => facts.path == Path::new(path),
        Home(relative) => home.is_some_and(|home| facts.path == home.join(relative)),
        // A `target` directory is only build output with a cache tag or a sibling manifest.
        Name("target") => {
            facts.name == "target" && (facts.has_cachedir_tag || facts.has_cargo_toml_sibling)
        }
        // Without a package.json beside it, node_modules belongs to an installed tool.
        Name("node_modules") => facts.name == "node_modules" && facts.has_package_json_sibling,
        Name(name) => facts.name == *name,
    }
}

pub fn rule_for(facts: &DirFacts, home: Option<&Path>) -> Option<&'static Rule> {
    RULES.iter().find(|rule| {
        rule.matchers
            .iter()
            .any(|matcher| matcher_hits(matcher, facts, home))
    })
}

/// Matches kept in the list; the total still covers all of them.
pub const MAX_MATCHES: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimMatch {
    pub rule_id: &'static str,
    pub title_key: &'static str,
    pub explanation_key: &'static str,
    pub command: Option<String>,
    pub warning: bool,
    /// False when the real saving is unknowable; the size is then not in the total.
    pub counted: bool,
    pub path: String,
    pub size_bytes: u64,
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reclaimable {
    pub total_bytes: u64,
    pub matches: Vec<ReclaimMatch>,
    /// Matches left out of the list by the cap.
    pub more: usize,
}

/// Single-quotes a value for a POSIX shell.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn child_named<'a>(tree: &'a ScanTree, node: &ScanNode, name: &str) -> Option<&'a ScanNode> {
    node.children
        .iter()
        .filter_map(|id| tree.nodes.get(*id as usize))
        .find(|child| child.name == name)
}

/// The rule's command with its location filled in; `None` when the command
/// needs a location this match does not have.
fn command_for(rule: &Rule, node: &ScanNode, tree: &ScanTree) -> Option<String> {
    let command = rule.command?;
    if !command.contains("{manifest}") {
        return Some(command.to_string());
    }
    let parent = tree.nodes.get(node.parent_id? as usize)?;
    child_named(tree, parent, "Cargo.toml")?;
    let manifest = Path::new(parent.local_path.as_deref()?).join("Cargo.toml");
    Some(command.replace("{manifest}", &shell_quote(manifest.to_str()?)))
}

/// Matches below `start` (inclusive). A match is not entered, so nothing is
/// counted twice, except a container rule (the user cache): matches inside it are
/// reported as their own items and the container only keeps what they do not cover.
fn collect(tree: &ScanTree, start: u64, home: Option<&Path>) -> Vec<ReclaimMatch> {
    let mut matches = Vec::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        let Some(node) = tree.nodes.get(id as usize) else {
            continue;
        };
        if node.kind != NodeKind::Directory {
            continue;
        }
        let path = node
            .local_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&node.display_path));
        let facts = DirFacts {
            path: &path,
            name: &node.name,
            has_cachedir_tag: child_named(tree, node, "CACHEDIR.TAG").is_some(),
            has_cargo_toml_sibling: node
                .parent_id
                .and_then(|parent| tree.nodes.get(parent as usize))
                .is_some_and(|parent| child_named(tree, parent, "Cargo.toml").is_some()),
            has_package_json_sibling: node
                .parent_id
                .and_then(|parent| tree.nodes.get(parent as usize))
                .is_some_and(|parent| child_named(tree, parent, "package.json").is_some()),
        };
        let Some(rule) = rule_for(&facts, home) else {
            stack.extend(node.children.iter().copied());
            continue;
        };
        let mut size = node.allocated_bytes;
        if rule.container {
            let inner: Vec<ReclaimMatch> = node
                .children
                .iter()
                .flat_map(|child| collect(tree, *child, home))
                .collect();
            let covered: u64 = inner.iter().map(|found| found.size_bytes).sum();
            size = size.saturating_sub(covered);
            matches.extend(inner);
        }
        matches.push(ReclaimMatch {
            rule_id: rule.id,
            title_key: rule.title_key,
            explanation_key: rule.explanation_key,
            command: command_for(rule, node, tree),
            warning: rule.warning,
            counted: !rule.uncertain,
            path: node.display_path.clone(),
            size_bytes: size,
            node_id: node.id,
        });
    }
    matches
}

/// Matches in the tree, largest first, capped at `MAX_MATCHES`.
pub fn find(tree: &ScanTree, home: Option<&Path>) -> Reclaimable {
    let mut matches = collect(tree, tree.root_id, home);
    matches.retain(|found| found.size_bytes > 0);
    matches.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    let total_bytes = matches
        .iter()
        .filter(|found| found.counted)
        .map(|found| found.size_bytes)
        .sum();
    let more = matches.len().saturating_sub(MAX_MATCHES);
    matches.truncate(MAX_MATCHES);
    Reclaimable {
        total_bytes,
        matches,
        more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScanSummary;

    fn facts<'a>(path: &'a str, name: &'a str, tag: bool, cargo: bool) -> DirFacts<'a> {
        DirFacts {
            path: Path::new(path),
            name,
            has_cachedir_tag: tag,
            has_cargo_toml_sibling: cargo,
            has_package_json_sibling: false,
        }
    }

    fn id_of(facts: &DirFacts, home: Option<&Path>) -> Option<&'static str> {
        rule_for(facts, home).map(|rule| rule.id)
    }

    #[test]
    fn home_relative_rules_follow_the_home_folder() {
        let home = Some(Path::new("/home/ada"));
        assert_eq!(id_of(&facts("/home/ada/.cache", ".cache", false, false), home), Some("user_cache"));
        assert_eq!(
            id_of(&facts("/home/ada/.npm/_cacache", "_cacache", false, false), home),
            Some("npm_cache")
        );
        assert_eq!(id_of(&facts("/home/bob/.cache", ".cache", false, false), home), None);
        assert_eq!(id_of(&facts("/home/ada/.cache", ".cache", false, false), None), None);
    }

    #[test]
    fn absolute_rules_carry_their_commands() {
        let rule = rule_for(&facts("/var/cache/pacman/pkg", "pkg", false, false), None).unwrap();
        assert_eq!(rule.command, Some("sudo paccache -rk2"));
        let rule = rule_for(&facts("/var/cache/libdnf5", "libdnf5", false, false), None).unwrap();
        assert_eq!(rule.id, "dnf_cache");
        let rule = rule_for(&facts("/var/lib/systemd/coredump", "coredump", false, false), None).unwrap();
        assert!(rule.warning);
        assert_eq!(id_of(&facts("/var/cache/pacman", "pacman", false, false), None), None);
    }

    #[test]
    fn node_modules_needs_a_package_json_beside_it() {
        let mut node_modules = facts("/srv/app/node_modules", "node_modules", false, false);
        assert_eq!(id_of(&node_modules, None), None);
        node_modules.has_package_json_sibling = true;
        assert_eq!(id_of(&node_modules, None), Some("node_modules"));
    }

    #[test]
    fn shell_quoting_survives_single_quotes_and_spaces() {
        assert_eq!(shell_quote("/a b/it's"), "'/a b/it'\\''s'");
    }

    #[test]
    fn rust_target_needs_a_cache_tag_or_a_manifest_next_to_it() {
        assert_eq!(id_of(&facts("/p/target", "target", true, false), None), Some("rust_target"));
        assert_eq!(id_of(&facts("/p/target", "target", false, true), None), Some("rust_target"));
        assert_eq!(id_of(&facts("/p/target", "target", false, false), None), None);
        assert_eq!(id_of(&facts("/p/other", "other", true, true), None), None);
    }

    fn dir(id: u64, parent: Option<u64>, parent_path: &str, name: &str, bytes: u64) -> ScanNode {
        ScanNode {
            id,
            parent_id: parent,
            name: name.into(),
            display_path: format!("{}/{name}", parent_path.trim_end_matches('/')),
            uri: String::new(),
            local_path: Some(PathBuf::from(format!("{}/{name}", parent_path.trim_end_matches('/')))),
            kind: if name.contains('.') && name != ".cache" {
                NodeKind::File
            } else {
                NodeKind::Directory
            },
            allocated_bytes: bytes,
            apparent_bytes: bytes,
            children: Vec::new(),
            file_count: 0,
            directory_count: 0,
            modified_ms: None,
            permissions: None,
            hard_links: 1,
            flags: Vec::new(),
        }
    }

    fn build(entries: &[(&str, &str, u64)]) -> ScanTree {
        // (parent path, name, bytes); the first entry is the root, parents come first.
        let mut nodes: Vec<ScanNode> = Vec::new();
        for (parent_path, name, bytes) in entries {
            let id = nodes.len() as u64;
            let parent = nodes
                .iter()
                .position(|n| n.display_path == *parent_path)
                .map(|p| p as u64);
            nodes.push(dir(id, parent, parent_path, name, *bytes));
            if let Some(parent) = parent {
                nodes[parent as usize].children.push(id);
            }
        }
        ScanTree {
            root_id: 0,
            nodes,
            issues: Vec::new(),
            summary: ScanSummary::default(),
        }
    }

    fn seen(found: &Reclaimable) -> Vec<(&'static str, &str, u64)> {
        found
            .matches
            .iter()
            .map(|m| (m.rule_id, m.path.as_str(), m.size_bytes))
            .collect()
    }

    #[test]
    fn specific_rules_under_the_user_cache_are_their_own_items() {
        // .cache holds 400: thumbnails 100, pip 60, a proj with a node_modules 50 (no package.json) and 190 loose.
        let tree = build(&[
            ("/h", "ada", 400),
            ("/h/ada", ".cache", 400),
            ("/h/ada/.cache", "thumbnails", 100),
            ("/h/ada/.cache", "pip", 60),
            ("/h/ada/.cache", "proj", 50),
            ("/h/ada/.cache/proj", "node_modules", 50),
        ]);
        let found = find(&tree, Some(Path::new("/h/ada")));
        assert_eq!(
            seen(&found),
            [
                ("user_cache", "/h/ada/.cache", 240),
                ("thumbnails", "/h/ada/.cache/thumbnails", 100),
                ("pip_cache", "/h/ada/.cache/pip", 60),
            ]
        );
        // Nothing is counted twice: the parts add up to the folder's own size.
        assert_eq!(found.total_bytes, 400);
        assert_eq!(found.matches[2].command.as_deref(), Some("pip cache purge"));
    }

    #[test]
    fn other_nested_matches_are_reported_under_the_outermost_only() {
        let tree = build(&[
            ("/h", "ada", 1000),
            ("/h/ada", "proj", 500),
            ("/h/ada/proj", "Cargo.toml", 1),
            ("/h/ada/proj", "package.json", 1),
            ("/h/ada/proj", "target", 300),
            ("/h/ada/proj/target", "debug", 290),
            ("/h/ada/proj", "web", 150),
            ("/h/ada/proj/web", "package.json", 1),
            ("/h/ada/proj/web", "node_modules", 120),
            ("/h/ada/proj/web/node_modules", "a", 100),
            ("/h/ada/proj/web/node_modules/a", "node_modules", 50),
        ]);
        let found = find(&tree, Some(Path::new("/h/ada")));
        let rows = seen(&found);
        // `target` has no package.json beside it and its sibling Cargo.toml makes it Rust output.
        assert_eq!(rows[0], ("rust_target", "/h/ada/proj/target", 300));
        assert_eq!(rows[1], ("node_modules", "/h/ada/proj/web/node_modules", 120));
        assert_eq!(rows.len(), 2);
        assert_eq!(found.total_bytes, 420);
    }

    #[test]
    fn cargo_clean_names_its_project_and_is_quoted() {
        let tree = build(&[
            ("/h", "ada", 10),
            ("/h/ada", "my proj", 10),
            ("/h/ada/my proj", "Cargo.toml", 1),
            ("/h/ada/my proj", "target", 9),
        ]);
        let found = find(&tree, None);
        assert_eq!(
            found.matches[0].command.as_deref(),
            Some("cargo clean --manifest-path '/h/ada/my proj/Cargo.toml'")
        );
        // CACHEDIR.TAG alone does not say which project to clean.
        let tagged = build(&[
            ("/h", "ada", 10),
            ("/h/ada", "build", 10),
            ("/h/ada/build", "target", 9),
            ("/h/ada/build/target", "CACHEDIR.TAG", 1),
        ]);
        assert_eq!(find(&tagged, None).matches[0].command, None);
    }

    #[test]
    fn unknowable_savings_are_listed_but_not_totalled_and_the_list_is_capped() {
        let mut entries = vec![("", "", 0), ("/", "var", 0)];
        entries.push(("/var", "lib", 0));
        entries.push(("/var/lib", "docker", 900));
        entries.push(("/var/lib", "flatpak", 50));
        entries.push(("/var", "cache", 0));
        entries.push(("/var/cache", "pacman", 0));
        entries.push(("/var/cache/pacman", "pkg", 10));
        let found = find(&build(&entries), None);
        assert_eq!(found.matches.len(), 3);
        assert_eq!(found.total_bytes, 10);
        assert!(found.matches.iter().filter(|m| !m.counted).count() == 2);

        let names: Vec<String> = (0..250).map(|n| format!("p{n}")).collect();
        let mut many = vec![("", "", 0), ("/", "h", 0)];
        for name in &names {
            many.push(("/h", name.as_str(), 1));
        }
        let mut tree = build(&many);
        // Give each project a counted match: a node_modules next to a package.json.
        for n in 0..250_u64 {
            let parent_path = format!("/h/p{n}");
            let a = tree.nodes.len() as u64;
            tree.nodes.push(dir(a, Some(n + 2), &parent_path, "package.json", 1));
            let b = a + 1;
            tree.nodes.push(dir(b, Some(n + 2), &parent_path, "node_modules", 7));
            tree.nodes[n as usize + 2].children.extend([a, b]);
        }
        let found = find(&tree, None);
        assert_eq!(found.matches.len(), MAX_MATCHES);
        assert_eq!(found.more, 50);
        assert_eq!(found.total_bytes, 250 * 7);
    }
}
