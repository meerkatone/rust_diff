//! Basic-block correspondence between two matched functions.
//!
//! The function matcher pairs whole functions; the graph diff overlay needs a
//! block↔block mapping with a per-pair status. The matching is incremental,
//! BinDiff-style:
//!   1. seed with the entry blocks and blocks whose canonical IL line-sets are
//!      identical and unique on both sides (exact block hash),
//!   2. propagate greedily over CFG edges from already-matched pairs,
//!   3. score the remaining cross-pairs with an IL line LCS and accept above a
//!      threshold via a stable, deterministic greedy assignment.
//!
//! Canonicalization reuses `il::canonical_line` so rename≠replace semantics
//! (variable renumbering, matched-callee renames, relocated addresses) carry
//! over: a block whose canonical lines match but whose rendered text differs is
//! `cosmetic`, not `changed`.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::il::{canonical_line, IlLine};

/// One basic block of the function at the chosen IL level. `index` is the
/// caller's block id (Binary Ninja basic-block index); `successors` reference
/// other blocks' `index` values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BlockInfo {
    pub index: usize,
    pub lines: Vec<IlLine>,
    pub successors: Vec<usize>,
}

/// A function as a list of IL basic blocks. `entry` is the `index` of the
/// entry block (defaults to the first block's index when absent).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BlockFunction {
    pub level: String,
    pub blocks: Vec<BlockInfo>,
    pub entry: Option<usize>,
}

fn default_threshold() -> f64 {
    0.5
}

/// Request payload for the block-diff FFI. `rename_map`/`addr_map` have the
/// same meaning as in `il::IlDiffRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockDiffRequest {
    pub a: BlockFunction,
    pub b: BlockFunction,
    pub rename_map: HashMap<String, String>,
    pub addr_map: HashMap<String, String>,
    /// Minimum line-LCS similarity for a propagated/global pair to be accepted.
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

impl Default for BlockDiffRequest {
    fn default() -> Self {
        Self {
            a: BlockFunction::default(),
            b: BlockFunction::default(),
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
            threshold: default_threshold(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockStatus {
    Equal,
    Cosmetic,
    Changed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPair {
    pub a: usize,
    pub b: usize,
    pub status: BlockStatus,
    /// Canonical line-LCS similarity of the pair (1.0 for equal/cosmetic).
    pub similarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDiff {
    pub pairs: Vec<BlockPair>,
    /// Blocks only in A (removed), by `index`, ascending.
    pub only_a: Vec<usize>,
    /// Blocks only in B (added), by `index`, ascending.
    pub only_b: Vec<usize>,
    /// Whole-function block-level similarity: sum of pair similarities over
    /// max(|A|, |B|).
    pub similarity: f64,
}

/// Internal per-block precomputation: canonical line keys, a joined hash key,
/// and the trimmed rendered text (for equal-vs-cosmetic).
struct Prep {
    canon_lines: Vec<String>,
    canon_key: String,
    text_key: String,
    succs: Vec<usize>,
    preds: Vec<usize>,
}

fn prepare(
    f: &BlockFunction,
    rename_map: &HashMap<String, String>,
    addr_map: &HashMap<String, String>,
) -> HashMap<usize, Prep> {
    let mut preds: HashMap<usize, Vec<usize>> = HashMap::new();
    for b in &f.blocks {
        for &s in &b.successors {
            preds.entry(s).or_default().push(b.index);
        }
    }
    f.blocks
        .iter()
        .map(|b| {
            let canon_lines: Vec<String> = b
                .lines
                .iter()
                .map(|l| canonical_line(l, rename_map, addr_map))
                .collect();
            let canon_key = canon_lines.join("\n");
            let text_key = b
                .lines
                .iter()
                .map(|l| l.text.trim())
                .collect::<Vec<_>>()
                .join("\n");
            let mut p = preds.get(&b.index).cloned().unwrap_or_default();
            p.sort_unstable();
            (
                b.index,
                Prep {
                    canon_lines,
                    canon_key,
                    text_key,
                    succs: b.successors.clone(),
                    preds: p,
                },
            )
        })
        .collect()
}

/// LCS-based similarity over two canonical line lists (LCS / max len; empty vs
/// empty is 1.0).
fn line_similarity(a: &[String], b: &[String]) -> f64 {
    let (n, m) = (a.len(), b.len());
    if n == 0 && m == 0 {
        return 1.0;
    }
    if n == 0 || m == 0 {
        return 0.0;
    }
    let mut prev = vec![0usize; m + 1];
    let mut cur = vec![0usize; m + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            cur[j] = if a[i] == b[j] {
                prev[j + 1] + 1
            } else {
                prev[j].max(cur[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
        cur.fill(0);
    }
    prev[0] as f64 / n.max(m) as f64
}

fn status_of(pa: &Prep, pb: &Prep) -> BlockStatus {
    if pa.canon_key == pb.canon_key {
        if pa.text_key == pb.text_key {
            BlockStatus::Equal
        } else {
            BlockStatus::Cosmetic
        }
    } else {
        BlockStatus::Changed
    }
}

/// Greedily accept candidate pairs (score, a, b) above `threshold`,
/// deterministically: best score first, ties broken by (a, b). Returns the
/// accepted pairs; updates the matched sets.
fn greedy_assign(
    mut candidates: Vec<(f64, usize, usize)>,
    matched_a: &mut HashSet<usize>,
    matched_b: &mut HashSet<usize>,
    threshold: f64,
) -> Vec<(usize, usize, f64)> {
    candidates.sort_by(|x, y| {
        y.0.partial_cmp(&x.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.1.cmp(&y.1))
            .then(x.2.cmp(&y.2))
    });
    let mut out = Vec::new();
    for (score, a, b) in candidates {
        if score < threshold {
            break;
        }
        if matched_a.contains(&a) || matched_b.contains(&b) {
            continue;
        }
        matched_a.insert(a);
        matched_b.insert(b);
        out.push((a, b, score));
    }
    out
}

pub fn block_diff(req: &BlockDiffRequest) -> BlockDiff {
    let prep_a = prepare(&req.a, &req.rename_map, &req.addr_map);
    let prep_b = prepare(&req.b, &req.rename_map, &req.addr_map);
    let ids_a: Vec<usize> = {
        let mut v: Vec<usize> = req.a.blocks.iter().map(|b| b.index).collect();
        v.sort_unstable();
        v
    };
    let ids_b: Vec<usize> = {
        let mut v: Vec<usize> = req.b.blocks.iter().map(|b| b.index).collect();
        v.sort_unstable();
        v
    };

    let mut matched_a: HashSet<usize> = HashSet::new();
    let mut matched_b: HashSet<usize> = HashSet::new();
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();

    // 1a. Entry blocks always correspond (a function relates to its entry by
    // definition; the pair's *status* still reflects how much it changed).
    let entry_a = req
        .a
        .entry
        .or_else(|| req.a.blocks.first().map(|b| b.index));
    let entry_b = req
        .b
        .entry
        .or_else(|| req.b.blocks.first().map(|b| b.index));
    if let (Some(ea), Some(eb)) = (entry_a, entry_b) {
        if prep_a.contains_key(&ea) && prep_b.contains_key(&eb) {
            matched_a.insert(ea);
            matched_b.insert(eb);
            let sim = line_similarity(&prep_a[&ea].canon_lines, &prep_b[&eb].canon_lines);
            pairs.push((ea, eb, sim));
        }
    }

    // 1b. Exact-hash seeding: canonical block keys that occur exactly once on
    // each side are unambiguous matches.
    let mut by_key_a: HashMap<&str, Vec<usize>> = HashMap::new();
    for &id in &ids_a {
        by_key_a.entry(&prep_a[&id].canon_key).or_default().push(id);
    }
    let mut by_key_b: HashMap<&str, Vec<usize>> = HashMap::new();
    for &id in &ids_b {
        by_key_b.entry(&prep_b[&id].canon_key).or_default().push(id);
    }
    let mut seeds: Vec<(usize, usize)> = Vec::new();
    for (key, va) in &by_key_a {
        if va.len() != 1 {
            continue;
        }
        if let Some(vb) = by_key_b.get(key) {
            if vb.len() == 1 {
                seeds.push((va[0], vb[0]));
            }
        }
    }
    seeds.sort_unstable();
    for (a, b) in seeds {
        if matched_a.contains(&a) || matched_b.contains(&b) {
            continue;
        }
        matched_a.insert(a);
        matched_b.insert(b);
        pairs.push((a, b, 1.0));
    }

    // 2. Local propagation over CFG edges: from each matched pair, score the
    // still-unmatched successor (and predecessor) cross-pairs; accept the best
    // above threshold; iterate to fixpoint.
    let mut frontier: Vec<(usize, usize)> = pairs.iter().map(|&(a, b, _)| (a, b)).collect();
    while !frontier.is_empty() {
        let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
        for &(a, b) in &frontier {
            let (pa, pb) = (&prep_a[&a], &prep_b[&b]);
            for (na, nb) in [(&pa.succs, &pb.succs), (&pa.preds, &pb.preds)] {
                for &ca in na {
                    if matched_a.contains(&ca) || !prep_a.contains_key(&ca) {
                        continue;
                    }
                    for &cb in nb {
                        if matched_b.contains(&cb) || !prep_b.contains_key(&cb) {
                            continue;
                        }
                        let sim =
                            line_similarity(&prep_a[&ca].canon_lines, &prep_b[&cb].canon_lines);
                        candidates.push((sim, ca, cb));
                    }
                }
            }
        }
        let accepted = greedy_assign(candidates, &mut matched_a, &mut matched_b, req.threshold);
        frontier = accepted.iter().map(|&(a, b, _)| (a, b)).collect();
        pairs.extend(accepted);
    }

    // 3. Global fallback over all remaining cross-pairs.
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for &a in &ids_a {
        if matched_a.contains(&a) {
            continue;
        }
        for &b in &ids_b {
            if matched_b.contains(&b) {
                continue;
            }
            let sim = line_similarity(&prep_a[&a].canon_lines, &prep_b[&b].canon_lines);
            candidates.push((sim, a, b));
        }
    }
    pairs.extend(greedy_assign(
        candidates,
        &mut matched_a,
        &mut matched_b,
        req.threshold,
    ));

    pairs.sort_by_key(|&(a, _, _)| a);
    let out_pairs: Vec<BlockPair> = pairs
        .iter()
        .map(|&(a, b, sim)| {
            let status = status_of(&prep_a[&a], &prep_b[&b]);
            let similarity = if status == BlockStatus::Changed {
                sim
            } else {
                1.0
            };
            BlockPair {
                a,
                b,
                status,
                similarity,
            }
        })
        .collect();

    let only_a: Vec<usize> = ids_a
        .iter()
        .copied()
        .filter(|i| !matched_a.contains(i))
        .collect();
    let only_b: Vec<usize> = ids_b
        .iter()
        .copied()
        .filter(|i| !matched_b.contains(i))
        .collect();

    let denom = ids_a.len().max(ids_b.len());
    let similarity = if denom == 0 {
        1.0
    } else {
        out_pairs.iter().map(|p| p.similarity).sum::<f64>() / denom as f64
    };

    BlockDiff {
        pairs: out_pairs,
        only_a,
        only_b,
        similarity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::il::IlToken;

    fn tok(kind: &str, text: &str) -> IlToken {
        IlToken {
            kind: kind.to_string(),
            text: text.to_string(),
        }
    }
    fn line(tokens: Vec<IlToken>, text: &str) -> IlLine {
        IlLine {
            tokens,
            text: text.to_string(),
        }
    }
    fn tline(text: &str) -> IlLine {
        // Untyped line: canonicalizes to its whitespace-normalized text.
        IlLine {
            tokens: Vec::new(),
            text: text.to_string(),
        }
    }
    fn block(index: usize, lines: Vec<IlLine>, successors: Vec<usize>) -> BlockInfo {
        BlockInfo {
            index,
            lines,
            successors,
        }
    }
    fn func(blocks: Vec<BlockInfo>) -> BlockFunction {
        BlockFunction {
            level: "HLIL".into(),
            blocks,
            entry: None,
        }
    }
    fn req(a: BlockFunction, b: BlockFunction) -> BlockDiffRequest {
        BlockDiffRequest {
            a,
            b,
            ..Default::default()
        }
    }

    fn pair_of(d: &BlockDiff, a: usize) -> &BlockPair {
        d.pairs.iter().find(|p| p.a == a).unwrap()
    }

    #[test]
    fn identical_functions_match_fully_equal() {
        let mk = || {
            func(vec![
                block(
                    0,
                    vec![tline("x = arg1"), tline("if (x > 5) goto 1 else 2")],
                    vec![1, 2],
                ),
                block(1, vec![tline("return 1")], vec![]),
                block(2, vec![tline("return 0")], vec![]),
            ])
        };
        let d = block_diff(&req(mk(), mk()));
        assert_eq!(d.pairs.len(), 3);
        assert!(d.pairs.iter().all(|p| p.status == BlockStatus::Equal));
        assert!(d.only_a.is_empty() && d.only_b.is_empty());
        assert!((d.similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn entry_blocks_are_anchored_even_when_changed() {
        let a = func(vec![block(
            0,
            vec![tline("x = 1"), tline("return x")],
            vec![],
        )]);
        let b = func(vec![block(
            0,
            vec![tline("y = 2"), tline("call init()"), tline("return y")],
            vec![],
        )]);
        let d = block_diff(&req(a, b));
        assert_eq!(d.pairs.len(), 1);
        assert_eq!(pair_of(&d, 0).status, BlockStatus::Changed);
    }

    #[test]
    fn reordered_blocks_match_by_content() {
        // Same blocks, different indices/order on the B side.
        let a = func(vec![
            block(
                0,
                vec![tline("entry"), tline("if (c) goto 1 else 2")],
                vec![1, 2],
            ),
            block(1, vec![tline("a = compute_one()")], vec![3]),
            block(2, vec![tline("a = compute_two()")], vec![3]),
            block(3, vec![tline("return a")], vec![]),
        ]);
        let b = func(vec![
            block(
                0,
                vec![tline("entry"), tline("if (c) goto 1 else 2")],
                vec![3, 1],
            ),
            block(1, vec![tline("a = compute_two()")], vec![2]),
            block(2, vec![tline("return a")], vec![]),
            block(3, vec![tline("a = compute_one()")], vec![2]),
        ]);
        let d = block_diff(&req(a, b));
        assert_eq!(pair_of(&d, 1).b, 3);
        assert_eq!(pair_of(&d, 2).b, 1);
        assert_eq!(pair_of(&d, 3).b, 2);
        assert!(d.only_a.is_empty() && d.only_b.is_empty());
    }

    #[test]
    fn added_guard_block_is_only_b() {
        // B inserts a guard block between entry and body (the memcpy_diff shape).
        let a = func(vec![
            block(0, vec![tline("n = arg3")], vec![1]),
            block(
                1,
                vec![tline("memcpy(dst, src, sx.q(n))"), tline("return")],
                vec![],
            ),
        ]);
        let b = func(vec![
            block(0, vec![tline("n = arg3")], vec![1]),
            block(1, vec![tline("if (n > 0x100) goto 2 else 3")], vec![2, 3]),
            block(2, vec![tline("return -1")], vec![]),
            block(
                3,
                vec![tline("memcpy(dst, src, zx.q(n))"), tline("return")],
                vec![],
            ),
        ]);
        let d = block_diff(&req(a, b));
        assert_eq!(pair_of(&d, 0).b, 0);
        assert_eq!(pair_of(&d, 0).status, BlockStatus::Equal);
        // The memcpy body matches (sx.q -> zx.q makes it changed) ...
        assert_eq!(pair_of(&d, 1).b, 3);
        assert_eq!(pair_of(&d, 1).status, BlockStatus::Changed);
        // ... and the new guard + error blocks are additions on B.
        assert_eq!(d.only_b, vec![1, 2]);
        assert!(d.only_a.is_empty());
    }

    #[test]
    fn removed_block_is_only_a() {
        let a = func(vec![
            block(0, vec![tline("entry")], vec![1, 2]),
            block(1, vec![tline("log_debug(x)")], vec![2]),
            block(2, vec![tline("return x")], vec![]),
        ]);
        let b = func(vec![
            block(0, vec![tline("entry")], vec![1]),
            block(1, vec![tline("return x")], vec![]),
        ]);
        let d = block_diff(&req(a, b));
        assert_eq!(d.only_a, vec![1]);
        assert_eq!(pair_of(&d, 2).b, 1);
    }

    #[test]
    fn variable_renumber_is_cosmetic_at_block_level() {
        let a = func(vec![block(
            0,
            vec![line(
                vec![tok("keyword", "return"), tok("localVariable", "var_10")],
                "return var_10",
            )],
            vec![],
        )]);
        let b = func(vec![block(
            0,
            vec![line(
                vec![tok("keyword", "return"), tok("localVariable", "var_18")],
                "return var_18",
            )],
            vec![],
        )]);
        let d = block_diff(&req(a, b));
        assert_eq!(pair_of(&d, 0).status, BlockStatus::Cosmetic);
        assert!((d.similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn renamed_matched_callee_is_cosmetic_via_rename_map() {
        let mk = |callee: &str| {
            func(vec![block(
                0,
                vec![line(
                    vec![
                        tok("keyword", "return"),
                        tok("codeSymbol", callee),
                        tok("text", "()"),
                    ],
                    &format!("return {}()", callee),
                )],
                vec![],
            )])
        };
        let mut r = req(mk("testa"), mk("testb"));
        assert_eq!(block_diff(&r).pairs[0].status, BlockStatus::Changed);
        r.rename_map.insert("testa".into(), "testb".into());
        assert_eq!(block_diff(&r).pairs[0].status, BlockStatus::Cosmetic);
    }

    #[test]
    fn ambiguous_duplicates_resolved_by_cfg_propagation() {
        // Two identical "x += 1" blocks on each side; only the CFG can tell
        // which corresponds to which.
        let mk = || {
            func(vec![
                block(0, vec![tline("if (c) goto 1 else 2")], vec![1, 2]),
                block(1, vec![tline("x += 1")], vec![3]),
                block(2, vec![tline("x += 1")], vec![4]),
                block(3, vec![tline("return left(x)")], vec![]),
                block(4, vec![tline("return right(x)")], vec![]),
            ])
        };
        let d = block_diff(&req(mk(), mk()));
        assert_eq!(pair_of(&d, 1).b, 1);
        assert_eq!(pair_of(&d, 2).b, 2);
        assert!(d.pairs.iter().all(|p| p.status == BlockStatus::Equal));
    }

    #[test]
    fn dissimilar_leftovers_stay_unmatched() {
        let a = func(vec![
            block(0, vec![tline("entry")], vec![1]),
            block(
                1,
                vec![tline("a"), tline("b"), tline("c"), tline("d")],
                vec![],
            ),
        ]);
        let b = func(vec![
            block(0, vec![tline("entry")], vec![1]),
            block(
                1,
                vec![tline("w"), tline("x"), tline("y"), tline("z")],
                vec![],
            ),
        ]);
        let d = block_diff(&req(a, b));
        // Block 1 pairs are below the 0.5 threshold -> unmatched on both sides.
        assert_eq!(d.only_a, vec![1]);
        assert_eq!(d.only_b, vec![1]);
    }

    #[test]
    fn deterministic_output() {
        let mk_a = || {
            func(vec![
                block(0, vec![tline("entry"), tline("branch")], vec![1, 2]),
                block(1, vec![tline("x = f()"), tline("y = g()")], vec![3]),
                block(2, vec![tline("x = f()"), tline("y = h()")], vec![3]),
                block(3, vec![tline("return x + y")], vec![]),
            ])
        };
        let mk_b = || {
            func(vec![
                block(0, vec![tline("entry"), tline("branch")], vec![1, 2]),
                block(1, vec![tline("x = f()"), tline("y = h()")], vec![3]),
                block(2, vec![tline("x = f()"), tline("y = g()")], vec![3]),
                block(3, vec![tline("return x + y")], vec![]),
            ])
        };
        let d1 = serde_json::to_string(&block_diff(&req(mk_a(), mk_b()))).unwrap();
        for _ in 0..10 {
            let d2 = serde_json::to_string(&block_diff(&req(mk_a(), mk_b()))).unwrap();
            assert_eq!(d1, d2);
        }
    }

    #[test]
    fn empty_functions() {
        let d = block_diff(&req(func(vec![]), func(vec![])));
        assert!(d.pairs.is_empty() && d.only_a.is_empty() && d.only_b.is_empty());
        assert!((d.similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn json_roundtrip_request() {
        // The FFI contract: a minimal JSON request parses with defaults.
        let j = r#"{
            "a": {"level":"HLIL","blocks":[{"index":0,"lines":[{"text":"return 1"}],"successors":[]}]},
            "b": {"level":"HLIL","blocks":[{"index":0,"lines":[{"text":"return 1"}],"successors":[]}]}
        }"#;
        let r: BlockDiffRequest = serde_json::from_str(j).unwrap();
        assert!((r.threshold - 0.5).abs() < 1e-9);
        let d = block_diff(&r);
        assert_eq!(d.pairs.len(), 1);
        assert_eq!(d.pairs[0].status, BlockStatus::Equal);
    }
}
