//! IL-aware (semantic) diffing.
//!
//! A plain textual diff of rendered IL is misleading: a *renamed* callee and a
//! *replaced* callee produce identical text diffs even though one is cosmetic
//! and the other is a real semantic change. The fix is to diff a *normalized*
//! token stream — volatile tokens (variables, immediates, stack offsets,
//! addresses) are canonicalized, and matched-callee renames are resolved
//! through a rename map — so the differ compares structure, not surface text.
//!
//! The frontend (Binary Ninja Python) extracts a typed token stream per IL
//! line via `InstructionTextToken`; this module is pure and testable without
//! Binary Ninja.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One typed token of a rendered IL line. `kind` mirrors Binary Ninja's
/// `InstructionTextToken` type name (e.g. "register", "localVariable",
/// "integer", "codeSymbol"); unknown kinds are treated as literal text.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IlToken {
    pub kind: String,
    pub text: String,
}

/// One rendered IL line as a typed token stream, plus the original rendered
/// text (kept verbatim for display).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IlLine {
    pub tokens: Vec<IlToken>,
    pub text: String,
}

/// A function's IL at one level (LLIL/MLIL/HLIL/Pseudo-C).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IlFunction {
    pub level: String,
    pub lines: Vec<IlLine>,
}

/// Request payload for the IL-diff FFI: two IL functions plus an optional
/// callee rename map (symbol-in-A -> symbol-in-B for callees that the matcher
/// paired). Renamed-but-matched calls normalize equal; everything else that
/// differs is a genuine change.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IlDiffRequest {
    pub a: IlFunction,
    pub b: IlFunction,
    /// symbol-in-A -> symbol-in-B for matched callees.
    pub rename_map: HashMap<String, String>,
    /// code-address-in-A -> code-address-in-B for matched callees (rendered as
    /// literal addresses). Lets a relocated-but-matched call target normalize
    /// equal instead of reading as a false change. Keys/values are the rendered
    /// address text (e.g. "0x1800010e8").
    pub addr_map: HashMap<String, String>,
}

/// Per-line diff op.
/// - `equal`: identical original text.
/// - `rename`: canonical form matches but original text differs (cosmetic:
///   variable renumbering, a matched-callee rename) — *not* a semantic change.
/// - `replace`: canonical forms differ; a real change. Paired A/B lines.
/// - `insert` / `delete`: line only on one side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DiffOp {
    Equal,
    Rename,
    Replace,
    Insert,
    Delete,
}

/// A token span for inline highlighting: the rendered text plus whether it
/// changed relative to the paired line (token-level LCS over canonical forms).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpan {
    pub text: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub op: DiffOp,
    pub a: Option<String>,
    pub b: Option<String>,
    /// Per-token spans for the A/B side of a `replace`/`rename` line, so the UI
    /// can highlight just the changed tokens. Empty for whole-line ops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub a_spans: Vec<TokenSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub b_spans: Vec<TokenSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IlDiff {
    pub lines: Vec<DiffLine>,
    /// Fraction of canonical lines that are unchanged (LCS / max len).
    pub similarity: f64,
}

/// Canonicalize one token: volatile kinds collapse to a placeholder so cosmetic
/// differences don't register as semantic ones. Callee symbols are resolved
/// through `rename_map` (forward, A->B) so a matched rename normalizes equal on
/// both sides.
fn canonical_token(
    tok: &IlToken,
    rename_map: &HashMap<String, String>,
    addr_map: &HashMap<String, String>,
) -> String {
    // Binary Ninja InstructionTextToken type names are matched case-insensitively
    // and loosely (contains) so LLIL/MLIL/HLIL variants all collapse correctly.
    let kind = tok.kind.to_ascii_lowercase();
    if kind.contains("variable") || kind.contains("localvar") || kind.contains("stackvar") {
        return "VAR".to_string();
    }
    if kind.contains("integer") || kind.contains("possiblevalue") || kind.contains("floatingpoint")
    {
        return "IMM".to_string();
    }
    // Code/call-target addresses are kept literal so a genuinely changed call
    // target surfaces as a real change. A matched callee that merely relocated
    // is resolved through addr_map to its B-side address so it normalizes equal.
    // Data/global addresses relocate unpredictably and are collapsed to ADDR.
    if kind.contains("coderelative") || kind.contains("codeaddress") {
        let text = tok.text.trim();
        let resolved = addr_map.get(text).map(String::as_str).unwrap_or(text);
        return strip_dup_suffix(resolved).to_string();
    }
    if kind.contains("address") {
        return "ADDR".to_string();
    }
    if kind.contains("symbol") || kind.contains("import") || kind.contains("indirect") {
        // A matched-callee rename resolves to the B-side name on both sides.
        let resolved = rename_map
            .get(&tok.text)
            .cloned()
            .unwrap_or_else(|| tok.text.clone());
        return format!("SYM:{}", strip_dup_suffix(&resolved));
    }
    // Registers, keywords, operators, punctuation, raw text: kept literally
    // (they carry structural meaning). Whitespace-only tokens are dropped.
    let t = tok.text.trim();
    if t.is_empty() {
        String::new()
    } else {
        strip_dup_suffix(t).to_string()
    }
}

/// Strip a trailing `_<n>` (1-3 digits) that Binary Ninja appends to
/// disambiguate duplicated variables/constants across builds (e.g.
/// `__xmm@..0303_1` -> `__xmm@..0303`). Longer numeric suffixes are left
/// intact so real identifiers (e.g. `Feature_1196105017`) are not mangled.
fn strip_dup_suffix(s: &str) -> &str {
    if let Some(pos) = s.rfind('_') {
        let suffix = &s[pos + 1..];
        if !suffix.is_empty()
            && suffix.len() <= 3
            && suffix.bytes().all(|b| b.is_ascii_digit())
            && pos > 0
        // keep at least one leading char
        {
            return &s[..pos];
        }
    }
    s
}

/// Canonical key for a whole line: space-joined non-empty canonical tokens.
/// Falls back to the trimmed rendered text when no typed tokens are present.
pub(crate) fn canonical_line(
    line: &IlLine,
    rename_map: &HashMap<String, String>,
    addr_map: &HashMap<String, String>,
) -> String {
    if line.tokens.is_empty() {
        return line.text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    line.tokens
        .iter()
        .map(|t| canonical_token(t, rename_map, addr_map))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// An aligned line op referencing original A/B line indices (built during
/// backtrack, before paired lines get their token-level spans).
enum Aligned {
    Equal(usize, usize),
    Rename(usize, usize),
    Replace(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Semantic IL diff: LCS over canonical line keys, with cosmetic-only changes
/// reported as `rename` and genuine changes as `replace`. `replace`/`rename`
/// lines also carry token-level spans for inline highlighting.
pub fn diff_il(req: &IlDiffRequest) -> IlDiff {
    let canon = |l: &IlLine| canonical_line(l, &req.rename_map, &req.addr_map);
    let keys_a: Vec<String> = req.a.lines.iter().map(canon).collect();
    let keys_b: Vec<String> = req.b.lines.iter().map(canon).collect();

    let n = keys_a.len();
    let m = keys_b.len();

    // LCS DP over canonical keys.
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if keys_a[i] == keys_b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let lcs = dp[0][0];

    // Backtrack into aligned ops. Unmatched A/B lines are first emitted as
    // delete/insert, then adjacent delete+insert runs are paired into `replace`
    // so changed lines show side by side.
    let mut aligned: Vec<Aligned> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if keys_a[i] == keys_b[j] {
            // Same canonical form: equal if rendered text matches, else a
            // cosmetic rename (var renumber / matched-callee rename).
            let same = req.a.lines[i].text.trim() == req.b.lines[j].text.trim();
            aligned.push(if same {
                Aligned::Equal(i, j)
            } else {
                Aligned::Rename(i, j)
            });
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            aligned.push(Aligned::Delete(i));
            i += 1;
        } else {
            aligned.push(Aligned::Insert(j));
            j += 1;
        }
    }
    while i < n {
        aligned.push(Aligned::Delete(i));
        i += 1;
    }
    while j < m {
        aligned.push(Aligned::Insert(j));
        j += 1;
    }

    pair_replacements(&mut aligned);

    // Materialize aligned ops into DiffLines, computing token spans for the
    // changed (replace/rename) lines.
    let span_line = |idx_a: usize, idx_b: usize| {
        token_spans(
            &req.a.lines[idx_a],
            &req.b.lines[idx_b],
            &req.rename_map,
            &req.addr_map,
        )
    };
    let lines: Vec<DiffLine> = aligned
        .into_iter()
        .map(|a| match a {
            Aligned::Equal(ia, ib) => DiffLine {
                op: DiffOp::Equal,
                a: Some(req.a.lines[ia].text.clone()),
                b: Some(req.b.lines[ib].text.clone()),
                a_spans: Vec::new(),
                b_spans: Vec::new(),
            },
            Aligned::Rename(ia, ib) => {
                let (sa, sb) = span_line(ia, ib);
                DiffLine {
                    op: DiffOp::Rename,
                    a: Some(req.a.lines[ia].text.clone()),
                    b: Some(req.b.lines[ib].text.clone()),
                    a_spans: sa,
                    b_spans: sb,
                }
            }
            Aligned::Replace(ia, ib) => {
                let (sa, sb) = span_line(ia, ib);
                DiffLine {
                    op: DiffOp::Replace,
                    a: Some(req.a.lines[ia].text.clone()),
                    b: Some(req.b.lines[ib].text.clone()),
                    a_spans: sa,
                    b_spans: sb,
                }
            }
            Aligned::Delete(ia) => DiffLine {
                op: DiffOp::Delete,
                a: Some(req.a.lines[ia].text.clone()),
                b: None,
                a_spans: Vec::new(),
                b_spans: Vec::new(),
            },
            Aligned::Insert(ib) => DiffLine {
                op: DiffOp::Insert,
                a: None,
                b: Some(req.b.lines[ib].text.clone()),
                a_spans: Vec::new(),
                b_spans: Vec::new(),
            },
        })
        .collect();

    let denom = n.max(m);
    let similarity = if denom == 0 {
        1.0
    } else {
        lcs as f64 / denom as f64
    };

    IlDiff { lines, similarity }
}

/// Token-level LCS between two lines over their canonical token forms; returns
/// (a_spans, b_spans) where a token is `changed` if it is not part of the
/// common subsequence. Drives inline highlighting of just the changed tokens.
fn token_spans(
    line_a: &IlLine,
    line_b: &IlLine,
    rename_map: &HashMap<String, String>,
    addr_map: &HashMap<String, String>,
) -> (Vec<TokenSpan>, Vec<TokenSpan>) {
    // Keep every token (so concatenating span texts reproduces the line
    // verbatim, preserving spacing/punctuation); canonical form drives the
    // match. Whitespace tokens canonicalize to "" and match harmlessly.
    let ta: Vec<(&IlToken, String)> = line_a
        .tokens
        .iter()
        .map(|t| (t, canonical_token(t, rename_map, addr_map)))
        .collect();
    let tb: Vec<(&IlToken, String)> = line_b
        .tokens
        .iter()
        .map(|t| (t, canonical_token(t, rename_map, addr_map)))
        .collect();

    let (na, nb) = (ta.len(), tb.len());
    let mut dp = vec![vec![0usize; nb + 1]; na + 1];
    for i in (0..na).rev() {
        for j in (0..nb).rev() {
            dp[i][j] = if ta[i].1 == tb[j].1 {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut sa = Vec::with_capacity(na);
    let mut sb = Vec::with_capacity(nb);
    let (mut i, mut j) = (0usize, 0usize);
    while i < na && j < nb {
        if ta[i].1 == tb[j].1 {
            sa.push(TokenSpan {
                text: ta[i].0.text.clone(),
                changed: false,
            });
            sb.push(TokenSpan {
                text: tb[j].0.text.clone(),
                changed: false,
            });
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            // Don't emphasize whitespace/punctuation-only tokens (empty canonical).
            sa.push(TokenSpan {
                text: ta[i].0.text.clone(),
                changed: !ta[i].1.is_empty(),
            });
            i += 1;
        } else {
            sb.push(TokenSpan {
                text: tb[j].0.text.clone(),
                changed: !tb[j].1.is_empty(),
            });
            j += 1;
        }
    }
    while i < na {
        sa.push(TokenSpan {
            text: ta[i].0.text.clone(),
            changed: !ta[i].1.is_empty(),
        });
        i += 1;
    }
    while j < nb {
        sb.push(TokenSpan {
            text: tb[j].0.text.clone(),
            changed: !tb[j].1.is_empty(),
        });
        j += 1;
    }
    (sa, sb)
}

/// Collapse each run of consecutive deletes-then-inserts into paired `replace`
/// ops (with any leftover left as plain insert/delete), so a changed line
/// renders as old||new rather than two separate hunks.
fn pair_replacements(aligned: &mut Vec<Aligned>) {
    let mut out: Vec<Aligned> = Vec::with_capacity(aligned.len());
    let mut k = 0;
    while k < aligned.len() {
        if matches!(aligned[k], Aligned::Delete(_)) {
            let del_start = k;
            while k < aligned.len() && matches!(aligned[k], Aligned::Delete(_)) {
                k += 1;
            }
            let ins_start = k;
            while k < aligned.len() && matches!(aligned[k], Aligned::Insert(_)) {
                k += 1;
            }
            let dels: Vec<usize> = aligned[del_start..ins_start]
                .iter()
                .map(|a| {
                    if let Aligned::Delete(i) = a {
                        *i
                    } else {
                        unreachable!()
                    }
                })
                .collect();
            let inss: Vec<usize> = aligned[ins_start..k]
                .iter()
                .map(|a| {
                    if let Aligned::Insert(j) = a {
                        *j
                    } else {
                        unreachable!()
                    }
                })
                .collect();
            let paired = dels.len().min(inss.len());
            for p in 0..paired {
                out.push(Aligned::Replace(dels[p], inss[p]));
            }
            for &d in dels.iter().skip(paired) {
                out.push(Aligned::Delete(d));
            }
            for &ins in inss.iter().skip(paired) {
                out.push(Aligned::Insert(ins));
            }
        } else {
            // Move the non-delete op as-is.
            match aligned[k] {
                Aligned::Equal(a, b) => out.push(Aligned::Equal(a, b)),
                Aligned::Rename(a, b) => out.push(Aligned::Rename(a, b)),
                Aligned::Replace(a, b) => out.push(Aligned::Replace(a, b)),
                Aligned::Insert(b) => out.push(Aligned::Insert(b)),
                Aligned::Delete(a) => out.push(Aligned::Delete(a)),
            }
            k += 1;
        }
    }
    *aligned = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers to build IL lines as typed token streams.
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

    // The two functions from the spec, as HLIL token streams.
    // old:  if (true) / return 1 / j__guard_check_icall(nullptr) / return testa(arg1, zx.q(arg2), arg3)
    fn old_lines() -> Vec<IlLine> {
        vec![
            line(
                vec![
                    tok("keyword", "if"),
                    tok("text", "("),
                    tok("keyword", "true"),
                    tok("text", ")"),
                ],
                "if (true)",
            ),
            line(
                vec![tok("keyword", "return"), tok("integer", "1")],
                "    return 1",
            ),
            line(
                vec![
                    tok("codeSymbol", "j__guard_check_icall"),
                    tok("text", "("),
                    tok("keyword", "nullptr"),
                    tok("text", ")"),
                ],
                "j__guard_check_icall(nullptr)",
            ),
            line(
                vec![
                    tok("keyword", "return"),
                    tok("codeSymbol", "testa"),
                    tok("text", "("),
                    tok("localVariable", "arg1"),
                    tok("text", ", zx.q("),
                    tok("localVariable", "arg2"),
                    tok("text", "), "),
                    tok("localVariable", "arg3"),
                    tok("text", ")"),
                ],
                "return testa(arg1, zx.q(arg2), arg3)",
            ),
        ]
    }
    // new (replace): if (false) ... return testb(...)
    fn new_lines_replace() -> Vec<IlLine> {
        vec![
            line(
                vec![
                    tok("keyword", "if"),
                    tok("text", "("),
                    tok("keyword", "false"),
                    tok("text", ")"),
                ],
                "if (false)",
            ),
            line(
                vec![tok("keyword", "return"), tok("integer", "1")],
                "    return 1",
            ),
            line(
                vec![
                    tok("codeSymbol", "j__guard_check_icall"),
                    tok("text", "("),
                    tok("keyword", "nullptr"),
                    tok("text", ")"),
                ],
                "j__guard_check_icall(nullptr)",
            ),
            line(
                vec![
                    tok("keyword", "return"),
                    tok("codeSymbol", "testb"),
                    tok("text", "("),
                    tok("localVariable", "arg1"),
                    tok("text", ", zx.q("),
                    tok("localVariable", "arg2"),
                    tok("text", "), "),
                    tok("localVariable", "arg3"),
                    tok("text", ")"),
                ],
                "return testb(arg1, zx.q(arg2), arg3)",
            ),
        ]
    }

    #[test]
    fn replace_call_is_a_real_change() {
        // No rename map: testa->testb is a genuine replacement.
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: old_lines(),
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: new_lines_replace(),
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        // Line 0 (if true/false) and line 3 (testa/testb) are replaces;
        // lines 1 and 2 are equal.
        let replaces: Vec<&DiffLine> = d.lines.iter().filter(|l| l.op == DiffOp::Replace).collect();
        assert_eq!(
            replaces.len(),
            2,
            "if-condition and call should both be replaces: {:#?}",
            d.lines
        );
        assert!(d.lines.iter().any(|l| l.op == DiffOp::Replace
            && l.a.as_deref() == Some("return testa(arg1, zx.q(arg2), arg3)")
            && l.b.as_deref() == Some("return testb(arg1, zx.q(arg2), arg3)")));
    }

    #[test]
    fn renamed_call_is_cosmetic_not_a_replace() {
        // testa and testb are the SAME matched callee, just renamed across
        // builds. The condition flip is still a real change; the call is not.
        let mut rename_map = HashMap::new();
        rename_map.insert("testa".to_string(), "testb".to_string());
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: old_lines(),
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: new_lines_replace(),
            },
            rename_map,
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        // The call line now matches canonically -> reported as `rename`, not replace.
        let call = d
            .lines
            .iter()
            .find(|l| l.a.as_deref() == Some("return testa(arg1, zx.q(arg2), arg3)"))
            .unwrap();
        assert_eq!(
            call.op,
            DiffOp::Rename,
            "matched-callee rename must be cosmetic: {:#?}",
            d.lines
        );
        // Exactly one real change remains: the if-condition flip.
        let replaces: Vec<&DiffLine> = d.lines.iter().filter(|l| l.op == DiffOp::Replace).collect();
        assert_eq!(
            replaces.len(),
            1,
            "only the condition flip is a real change: {:#?}",
            d.lines
        );
    }

    #[test]
    fn identical_il_is_fully_equal() {
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: old_lines(),
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: old_lines(),
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        assert!((d.similarity - 1.0).abs() < 1e-9);
        assert!(d.lines.iter().all(|l| l.op == DiffOp::Equal));
    }

    #[test]
    fn ssa_dup_suffix_is_cosmetic() {
        // `__xmm@..0303` vs `__xmm@..0303_1`: a Binary Ninja dup suffix only.
        let a = vec![line(
            vec![tok("keyword", "return"), tok("dataSymbol", "__xmm@aa0303")],
            "return __xmm@aa0303",
        )];
        let b = vec![line(
            vec![
                tok("keyword", "return"),
                tok("dataSymbol", "__xmm@aa0303_1"),
            ],
            "return __xmm@aa0303_1",
        )];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        assert_eq!(
            d.lines[0].op,
            DiffOp::Rename,
            "dup suffix must be cosmetic: {:#?}",
            d.lines
        );
    }

    #[test]
    fn long_numeric_suffix_is_preserved() {
        // `Feature_1196105017` is a real identifier, not a dup suffix.
        let a = vec![line(
            vec![tok("codeSymbol", "Feature_1196105017")],
            "Feature_1196105017",
        )];
        let b = vec![line(vec![tok("codeSymbol", "Feature_42")], "Feature_42")];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        // `Feature_42` strips to `Feature`; `Feature_1196105017` keeps its long
        // suffix -> the two differ -> a real change, not a spurious match.
        assert_eq!(
            d.lines[0].op,
            DiffOp::Replace,
            "long id must not collapse: {:#?}",
            d.lines
        );
    }

    #[test]
    fn changed_code_address_is_a_real_change() {
        // A call target rendered as a literal code address: a different target
        // is a real change (kept literal), but a data address is collapsed.
        let a = vec![line(
            vec![tok("codeRelativeAddress", "0x1000"), tok("text", "(")],
            "0x1000(",
        )];
        let b = vec![line(
            vec![tok("codeRelativeAddress", "0x2000"), tok("text", "(")],
            "0x2000(",
        )];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        assert_eq!(
            diff_il(&req).lines[0].op,
            DiffOp::Replace,
            "changed code address must surface"
        );

        // Data/global addresses relocate; collapsed to ADDR -> cosmetic.
        let a = vec![line(
            vec![
                tok("keyword", "return"),
                tok("possibleAddress", "0x1801b8300"),
            ],
            "return 0x1801b8300",
        )];
        let b = vec![line(
            vec![
                tok("keyword", "return"),
                tok("possibleAddress", "0x1801c9400"),
            ],
            "return 0x1801c9400",
        )];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        assert_eq!(
            diff_il(&req).lines[0].op,
            DiffOp::Rename,
            "relocated data address must be cosmetic"
        );
    }

    #[test]
    fn relocated_matched_callee_is_cosmetic_via_addr_map() {
        // Same callee, different literal code address across builds. Without an
        // addr_map this is a real change; with the matcher's address pairing it
        // normalizes equal (cosmetic).
        let a = vec![line(
            vec![tok("codeRelativeAddress", "0x1000"), tok("text", "(")],
            "0x1000(",
        )];
        let b = vec![line(
            vec![tok("codeRelativeAddress", "0x2000"), tok("text", "(")],
            "0x2000(",
        )];
        let mut addr_map = HashMap::new();
        addr_map.insert("0x1000".to_string(), "0x2000".to_string());
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map,
        };
        assert_eq!(
            diff_il(&req).lines[0].op,
            DiffOp::Rename,
            "relocated-but-matched call target must be cosmetic with an addr_map"
        );
    }

    #[test]
    fn replace_line_marks_only_changed_tokens() {
        // `return testa(arg1)` vs `return testb(arg1)`: only the call symbol
        // changed; the token spans must flag exactly that token.
        let a = vec![line(
            vec![
                tok("keyword", "return"),
                tok("codeSymbol", "testa"),
                tok("text", "("),
                tok("localVariable", "arg1"),
                tok("text", ")"),
            ],
            "return testa(arg1)",
        )];
        let b = vec![line(
            vec![
                tok("keyword", "return"),
                tok("codeSymbol", "testb"),
                tok("text", "("),
                tok("localVariable", "arg1"),
                tok("text", ")"),
            ],
            "return testb(arg1)",
        )];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "HLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "HLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        let l = &d.lines[0];
        assert_eq!(l.op, DiffOp::Replace);
        let changed_a: Vec<&str> = l
            .a_spans
            .iter()
            .filter(|s| s.changed)
            .map(|s| s.text.as_str())
            .collect();
        let changed_b: Vec<&str> = l
            .b_spans
            .iter()
            .filter(|s| s.changed)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(
            changed_a,
            vec!["testa"],
            "only the call symbol changed on A: {:#?}",
            l.a_spans
        );
        assert_eq!(
            changed_b,
            vec!["testb"],
            "only the call symbol changed on B: {:#?}",
            l.b_spans
        );
    }

    #[test]
    fn variable_renumbering_is_cosmetic() {
        let a = vec![line(
            vec![tok("keyword", "return"), tok("localVariable", "var_10")],
            "return var_10",
        )];
        let b = vec![line(
            vec![tok("keyword", "return"), tok("localVariable", "var_18")],
            "return var_18",
        )];
        let req = IlDiffRequest {
            a: IlFunction {
                level: "MLIL".into(),
                lines: a,
            },
            b: IlFunction {
                level: "MLIL".into(),
                lines: b,
            },
            rename_map: HashMap::new(),
            addr_map: HashMap::new(),
        };
        let d = diff_il(&req);
        assert_eq!(d.lines[0].op, DiffOp::Rename);
    }
}
