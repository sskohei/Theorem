use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::domain::{SolutionDiffHunk, SolutionVersionDiff};

fn summarize_value(val: &Value) -> String {
    match val {
        Value::Null => "(なし)".to_string(),
        Value::String(s) => truncate_chars(s.trim(), 220),
        Value::Array(arr) => {
            if arr.is_empty() {
                "(空)".to_string()
            } else {
                let joined = arr
                    .iter()
                    .map(summarize_value)
                    .collect::<Vec<_>>()
                    .join(" / ");
                truncate_chars(&joined, 260)
            }
        }
        Value::Object(map) => {
            let mut parts: Vec<String> = Vec::new();
            for key in [
                "heading",
                "name",
                "step",
                "text",
                "explanation",
                "latex",
                "unit",
                "note",
            ] {
                if let Some(v) = map.get(key).and_then(Value::as_str)
                    && !v.trim().is_empty()
                {
                    if key == "latex" {
                        parts.push(format!("$${}$$", v.trim()));
                    } else {
                        parts.push(v.trim().to_string());
                    }
                }
            }
            if parts.is_empty() {
                truncate_chars(&val.to_string(), 240)
            } else {
                truncate_chars(&parts.join(" — "), 280)
            }
        }
        other => truncate_chars(&other.to_string(), 220),
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

fn is_effectively_empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

fn compare_scalar_field(
    base: &Value,
    proposed: &Value,
    key: &str,
    label: &str,
    hunks: &mut Vec<SolutionDiffHunk>,
) {
    let b = base.get(key);
    let p = proposed.get(key);
    if b == p || (is_effectively_empty(b) && is_effectively_empty(p)) {
        return;
    }
    let change_kind = if is_effectively_empty(b) {
        "added"
    } else if is_effectively_empty(p) {
        "removed"
    } else {
        "modified"
    };
    hunks.push(SolutionDiffHunk {
        hunk_id: key.to_string(),
        field_path: key.to_string(),
        label: label.to_string(),
        change_kind: change_kind.to_string(),
        before_json: b.cloned(),
        after_json: p.cloned(),
        before_summary: b
            .map(summarize_value)
            .unwrap_or_else(|| "(なし)".to_string()),
        after_summary: p
            .map(summarize_value)
            .unwrap_or_else(|| "(なし)".to_string()),
    });
}

fn compare_array_items(
    base: &Value,
    proposed: &Value,
    key: &str,
    item_label: &str,
    hunks: &mut Vec<SolutionDiffHunk>,
) {
    let b_arr = base
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let p_arr = proposed
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let max_len = b_arr.len().max(p_arr.len());
    for i in 0..max_len {
        let b = b_arr.get(i);
        let p = p_arr.get(i);
        if b == p {
            continue;
        }
        let change_kind = match (b, p) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ => "modified",
        };
        hunks.push(SolutionDiffHunk {
            hunk_id: format!("{key}:{i}"),
            field_path: format!("{key}[{i}]"),
            label: format!("{item_label} {}", i + 1),
            change_kind: change_kind.to_string(),
            before_json: b.cloned(),
            after_json: p.cloned(),
            before_summary: b
                .map(summarize_value)
                .unwrap_or_else(|| "(なし)".to_string()),
            after_summary: p
                .map(summarize_value)
                .unwrap_or_else(|| "(なし)".to_string()),
        });
    }
}

/// Computes a structured hunk-by-hunk diff between two Schema v1 solution JSON objects.
pub fn compute_solution_diff(base: &Value, proposed: &Value) -> Vec<SolutionDiffHunk> {
    let mut hunks = Vec::new();

    for (key, label) in [
        ("title", "タイトル"),
        ("coordinate_system", "座標系・符号規約"),
        ("domain_and_boundary", "定義域・境界条件"),
        ("assumptions", "仮定"),
        ("principles", "適用する物理法則・原理"),
        ("given_quantities", "与えられた物理量"),
    ] {
        compare_scalar_field(base, proposed, key, label, &mut hunks);
    }

    compare_array_items(base, proposed, "definitions", "定義", &mut hunks);
    compare_array_items(base, proposed, "theorems", "定理・補題", &mut hunks);
    compare_array_items(base, proposed, "steps", "解法ステップ", &mut hunks);
    compare_array_items(base, proposed, "proof", "証明ステップ", &mut hunks);

    for (key, label) in [
        ("answer", "最終解答"),
        ("dimensional_check", "単位・次元確認"),
        ("limiting_cases", "極限考察"),
        ("alternative_solutions", "別解"),
        ("counterexamples", "反例"),
        ("warnings", "注意点・不確実性"),
    ] {
        compare_scalar_field(base, proposed, key, label, &mut hunks);
    }

    hunks
}

/// Builds a version comparison summary between any two persisted solutions.
pub fn build_version_diff(
    base_id: &str,
    base_version: i64,
    base_json: &Value,
    target_id: &str,
    target_version: i64,
    target_json: &Value,
) -> SolutionVersionDiff {
    let hunks = compute_solution_diff(base_json, target_json);
    let added_count = hunks.iter().filter(|h| h.change_kind == "added").count();
    let modified_count = hunks.iter().filter(|h| h.change_kind == "modified").count();
    let removed_count = hunks.iter().filter(|h| h.change_kind == "removed").count();
    SolutionVersionDiff {
        base_solution_id: base_id.to_string(),
        base_version_number: base_version,
        target_solution_id: target_id.to_string(),
        target_version_number: target_version,
        hunks,
        added_count,
        modified_count,
        removed_count,
    }
}

/// Applies ONLY the user-approved hunks from `proposed` onto `base`,
/// strictly preserving unapproved sections and preventing accidental full overwrites.
pub fn apply_approved_hunks(
    base: &Value,
    proposed: &Value,
    approved_hunk_ids: Option<&[String]>,
) -> Value {
    let Some(ids) = approved_hunk_ids else {
        // Explicit "Approve all hunks": keep schema_version / problem_id scope from base
        let mut merged = proposed.clone();
        if let (Some(b_obj), Some(m_obj)) = (base.as_object(), merged.as_object_mut()) {
            if let Some(sv) = b_obj.get("schema_version") {
                m_obj.insert("schema_version".to_string(), sv.clone());
            }
            if let Some(pid) = b_obj.get("problem_id") {
                m_obj.insert("problem_id".to_string(), pid.clone());
            }
        }
        return merged;
    };

    let approved: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut out_obj: Map<String, Value> = base.as_object().cloned().unwrap_or_default();
    let p_obj = proposed.as_object();

    // Scalar fields
    for key in [
        "title",
        "coordinate_system",
        "domain_and_boundary",
        "assumptions",
        "principles",
        "given_quantities",
        "answer",
        "dimensional_check",
        "limiting_cases",
        "alternative_solutions",
        "counterexamples",
        "warnings",
    ] {
        if approved.contains(key)
            && let Some(po) = p_obj
        {
            if let Some(v) = po.get(key) {
                out_obj.insert(key.to_string(), v.clone());
            } else {
                out_obj.remove(key);
            }
        }
    }

    // Element-wise array fields
    for key in ["definitions", "theorems", "steps", "proof"] {
        let b_arr = base
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let p_arr = proposed
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let max_len = b_arr.len().max(p_arr.len());
        let mut next_arr = Vec::with_capacity(max_len);
        for i in 0..max_len {
            let hunk_id = format!("{key}:{i}");
            let is_approved = approved.contains(hunk_id.as_str());
            match (b_arr.get(i), p_arr.get(i), is_approved) {
                (_, Some(pv), true) => next_arr.push(pv.clone()),
                (_, None, true) => {
                    // Approved removal -> omit item
                }
                (Some(bv), _, false) => next_arr.push(bv.clone()),
                (None, Some(_), false) => {
                    // Unapproved addition -> omit item
                }
                (None, None, false) => {}
            }
        }
        if !next_arr.is_empty() || out_obj.contains_key(key) {
            out_obj.insert(key.to_string(), Value::Array(next_arr));
        }
    }

    Value::Object(out_obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compute_diff_and_partial_hunk_apply() {
        let base = json!({
            "schema_version": 1,
            "problem_id": "prob-1",
            "title": "元の解答",
            "steps": [
                { "type": "equation", "text": "ステップ1", "latex": "x = 1" },
                { "type": "equation", "text": "ステップ2", "latex": "y = 2" }
            ],
            "answer": { "text": "旧答え", "latex": "3" }
        });
        let proposed = json!({
            "schema_version": 1,
            "problem_id": "prob-1",
            "title": "修正タイトル",
            "steps": [
                { "type": "equation", "text": "ステップ1", "latex": "x = 1" },
                { "type": "equation", "text": "ステップ2修正", "latex": "y = 4" },
                { "type": "explanation", "text": "検算追記", "latex": "1 + 4 = 5" }
            ],
            "answer": { "text": "新答え", "latex": "5" }
        });

        let hunks = compute_solution_diff(&base, &proposed);
        let ids: Vec<&str> = hunks.iter().map(|h| h.hunk_id.as_str()).collect();
        assert_eq!(ids, vec!["title", "steps:1", "steps:2", "answer"]);

        // Approve ONLY steps:1 and answer (reject title change and steps:2 addition)
        let partial = apply_approved_hunks(
            &base,
            &proposed,
            Some(&["steps:1".to_string(), "answer".to_string()]),
        );
        assert_eq!(partial["title"], "元の解答");
        assert_eq!(partial["steps"].as_array().unwrap().len(), 2);
        assert_eq!(partial["steps"][1]["latex"], "y = 4");
        assert_eq!(partial["answer"]["latex"], "5");
    }
}
