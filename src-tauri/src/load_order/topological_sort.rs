use super::mod_info::{sanitize_mod_id, ModManifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::iter::FromIterator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAnalysisResult {
    pub sorted_mod_ids: Vec<String>,
    pub missing_dependencies: Vec<String>,
    pub has_circular_dependency: bool,
}

fn find_manifest_id_by_req<'a>(req: &str, manifests: &'a [ModManifest]) -> Option<&'a str> {
    let clean_req = req.trim().to_lowercase();
    let sanitized_req = sanitize_mod_id(&clean_req);

    // 1. Pass 1: Exact match on ID or sanitized ID FIRST!
    if let Some(exact) = manifests.iter().find(|m| {
        let clean_m = m.id.trim().to_lowercase();
        let sanitized_m = sanitize_mod_id(&clean_m);
        clean_m == clean_req || sanitized_m == sanitized_req
    }) {
        return Some(&exact.id);
    }

    // 2. Pass 2: Filter candidates by prefix matching
    let candidates: Vec<&'a ModManifest> = manifests.iter().filter(|m| {
        let clean_m = m.id.trim().to_lowercase();
        let sanitized_m = sanitize_mod_id(&clean_m);
        sanitized_m.starts_with(&format!("{}_", sanitized_req)) || sanitized_m.starts_with(&format!("{}-", sanitized_req))
    }).collect();

    if candidates.is_empty() {
        return None;
    }

    // Prioritize enabled candidate
    if let Some(enabled_cand) = candidates.iter().find(|c| c.enabled) {
        return Some(&enabled_cand.id);
    }

    Some(&candidates[0].id)
}

/// Performs topological sort on mod manifests based on require= directives.
pub fn sort_dependencies_topologically(manifests: &[ModManifest]) -> DependencyAnalysisResult {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut missing_deps = Vec::new();

    for m in manifests {
        in_degree.entry(m.id.clone()).or_insert(0);
        graph.entry(m.id.clone()).or_default();
    }

    for m in manifests {
        for req in &m.require {
            if let Some(target_id) = find_manifest_id_by_req(req, manifests) {
                if target_id != m.id {
                    // target_id (library/framework) must come BEFORE m.id
                    graph.entry(target_id.to_string()).or_default().push(m.id.clone());
                    *in_degree.entry(m.id.clone()).or_insert(0) += 1;
                }
            } else {
                if !missing_deps.contains(req) {
                    missing_deps.push(req.clone());
                }
            }
        }

        // Optional load_mod_after ordering hints (do NOT trigger missing_deps if target is absent!)
        for after in &m.load_mod_after {
            if let Some(target_id) = find_manifest_id_by_req(after, manifests) {
                if target_id != m.id {
                    graph.entry(target_id.to_string()).or_default().push(m.id.clone());
                    *in_degree.entry(m.id.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    // Kahn's algorithm for topological sorting.
    //
    // The zero-indegree seed MUST be sorted. `in_degree` is a HashMap, and
    // iterating it directly produced a different `sorted_mod_ids` order on every
    // call for the same input — which made the fix engine's plan_id unstable and
    // its staleness check unusable (a re-scan would "disagree" with a plan the
    // user had not even changed). Sorted seeding makes the whole sort
    // deterministic, which is a stronger guarantee than the old behaviour had.
    let mut seeds: Vec<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();
    seeds.sort();

    let mut queue: VecDeque<String> = VecDeque::from_iter(seeds);

    let mut sorted = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.clone());
        if let Some(neighbors) = graph.get(&node) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    let has_circular = sorted.len() < manifests.len();

    DependencyAnalysisResult {
        sorted_mod_ids: sorted,
        missing_dependencies: missing_deps,
        has_circular_dependency: has_circular,
    }
}
