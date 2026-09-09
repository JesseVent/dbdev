use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::Payload;

/// The upgrade relationships between an extension's versions.
///
/// `bases` are versions that ship a base install script, so `create extension
/// ... version '<v>'` can name them directly. `edges` are upgrade paths, each a
/// `(from, to)` pair.
///
/// Two sources feed this. A local `Payload` gives direct edges, one per upgrade
/// file. `pgtle.extension_update_paths()` gives every reachable pair rather than
/// just adjacent ones; both are fine here, since reachability is all either
/// query answers.
pub struct VersionGraph {
    bases: HashSet<String>,
    backward: HashMap<String, Vec<String>>,
}

impl VersionGraph {
    pub fn new(
        bases: impl IntoIterator<Item = String>,
        edges: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let mut backward: HashMap<String, Vec<String>> = HashMap::new();

        for (from, to) in edges {
            backward.entry(to).or_default().push(from);
        }

        Self {
            bases: bases.into_iter().collect(),
            backward,
        }
    }

    pub fn from_payload(payload: &Payload) -> Self {
        Self::new(
            payload.install_files.iter().map(|f| f.version.clone()),
            payload
                .upgrade_files
                .iter()
                .map(|f| (f.from_version.clone(), f.to_version.clone())),
        )
    }

    /// The versions worth installing when `target` is the one being asked for:
    /// `target` itself plus everything that can upgrade into it.
    ///
    /// Versions on a branch with no path to `target` are excluded. Installing
    /// them would clutter the pg_tle catalog and imply an upgrade route that
    /// does not exist (#387).
    pub fn required_for(&self, target: &str) -> HashSet<String> {
        let mut required = HashSet::new();
        let mut queue = VecDeque::new();

        required.insert(target.to_string());
        queue.push_back(target.to_string());

        while let Some(version) = queue.pop_front() {
            for ancestor in self.backward.get(&version).into_iter().flatten() {
                if required.insert(ancestor.clone()) {
                    queue.push_back(ancestor.clone());
                }
            }
        }

        required
    }

    /// Whether `target` can be created directly, i.e. it ships a base install
    /// script of its own.
    pub fn is_base(&self, target: &str) -> bool {
        self.bases.contains(target)
    }

    /// The nearest base version that reaches `target`.
    ///
    /// `create extension ... version '<target>'` fails when `target` is only
    /// reachable by upgrade, because pg_tle looks for a `<ext>--<target>.sql`
    /// that was never published (#159). Creating at this base and then running
    /// `alter extension ... update to '<target>'` gets there instead; Postgres
    /// walks the intermediate steps itself.
    ///
    /// Returns `None` when `target` is already a base, or when no base reaches
    /// it at all.
    pub fn base_for(&self, target: &str) -> Option<String> {
        if self.is_base(target) {
            return None;
        }

        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();

        seen.insert(target.to_string());
        queue.push_back(target.to_string());

        // Breadth-first, so the first base found is the fewest upgrades away.
        while let Some(version) = queue.pop_front() {
            for ancestor in self.backward.get(&version).into_iter().flatten() {
                if !seen.insert(ancestor.clone()) {
                    continue;
                }
                if self.bases.contains(ancestor) {
                    return Some(ancestor.clone());
                }
                queue.push_back(ancestor.clone());
            }
        }

        None
    }

    /// Whether an upgrade edge is worth installing alongside `required`: both
    /// of its endpoints have to be versions we are keeping.
    pub fn edge_is_required(required: &HashSet<String>, from: &str, to: &str) -> bool {
        required.contains(from) && required.contains(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(versions: &[&str]) -> Vec<String> {
        versions.iter().map(|s| s.to_string()).collect()
    }

    fn e(edges: &[(&str, &str)]) -> Vec<(String, String)> {
        edges
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .collect()
    }

    fn sorted(set: HashSet<String>) -> Vec<String> {
        let mut out: Vec<String> = set.into_iter().collect();
        out.sort();
        out
    }

    #[test]
    fn linear_chain_keeps_the_whole_lineage() {
        let graph = VersionGraph::new(
            v(&["1.0.0"]),
            e(&[("1.0.0", "1.1.0"), ("1.1.0", "1.2.0")]),
        );

        assert_eq!(
            sorted(graph.required_for("1.2.0")),
            v(&["1.0.0", "1.1.0", "1.2.0"])
        );
    }

    // #387: 4.x and 5.x are separate lineages with no crossing upgrade. Asking
    // for 5.1.0 must not drag every 4.x version into the catalog.
    #[test]
    fn disconnected_lineage_is_excluded() {
        let graph = VersionGraph::new(
            v(&["4.0.0", "5.0.0"]),
            e(&[
                ("4.0.0", "4.1.0"),
                ("4.1.0", "4.2.0"),
                ("5.0.0", "5.1.0"),
            ]),
        );

        assert_eq!(sorted(graph.required_for("5.1.0")), v(&["5.0.0", "5.1.0"]));

        let required = graph.required_for("5.1.0");
        assert!(VersionGraph::edge_is_required(
            &required, "5.0.0", "5.1.0"
        ));
        assert!(!VersionGraph::edge_is_required(
            &required, "4.0.0", "4.1.0"
        ));
    }

    #[test]
    fn oldest_version_needs_nothing_before_it() {
        let graph = VersionGraph::new(v(&["1.0.0"]), e(&[("1.0.0", "1.1.0")]));

        assert_eq!(sorted(graph.required_for("1.0.0")), v(&["1.0.0"]));
    }

    // #159: 2.1.0 ships only an upgrade script, so it has to be reached by
    // creating 2.0.1 first.
    #[test]
    fn upgrade_only_version_resolves_to_its_base() {
        let graph = VersionGraph::new(v(&["2.0.1"]), e(&[("2.0.1", "2.1.0")]));

        assert!(!graph.is_base("2.1.0"));
        assert_eq!(graph.base_for("2.1.0"), Some("2.0.1".to_string()));
    }

    #[test]
    fn base_for_picks_the_nearest_base() {
        let graph = VersionGraph::new(
            v(&["1.0.0", "1.2.0"]),
            e(&[
                ("1.0.0", "1.1.0"),
                ("1.1.0", "1.2.0"),
                ("1.2.0", "1.3.0"),
            ]),
        );

        assert_eq!(graph.base_for("1.3.0"), Some("1.2.0".to_string()));
    }

    #[test]
    fn a_version_that_is_already_a_base_needs_no_route() {
        let graph = VersionGraph::new(v(&["1.0.0"]), e(&[]));

        assert!(graph.is_base("1.0.0"));
        assert_eq!(graph.base_for("1.0.0"), None);
    }

    #[test]
    fn unreachable_version_has_no_base() {
        let graph = VersionGraph::new(v(&["1.0.0"]), e(&[("9.0.0", "9.1.0")]));

        assert_eq!(graph.base_for("9.1.0"), None);
    }
}
