use std::collections::HashMap;

use typst::text::Font;

use crate::domain::LatexConversionError;
use crate::pdf::compiler::{SandboxedWorld, compile_world_sync};

/// Pure-Typst offline compatibility prelude bridging MiTeX 0.2.4 macros/environments
/// onto Typst 0.15.1 without any external `@preview` packages.
/// Single source of truth shared with `templates/common.typ`.
pub const OFFLINE_MATH_PRELUDE: &str = include_str!("../../../templates/mitex_prelude.typ");

/// Replaces an identifier or dotted symbol path only at identifier boundaries
/// so substrings inside words (e.g. `different`, `section`, `bisect`, `arrow.l.dashed`)
/// are never corrupted.
fn replace_symbol_token(input: &str, from: &str, to: &str, allow_dot_after: bool) -> String {
    let bytes = input.as_bytes();
    let from_len = from.len();
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0usize;
    while i < bytes.len() {
        if input[i..].starts_with(from) {
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.');
            let next_ok = i + from_len == bytes.len()
                || !(bytes[i + from_len].is_ascii_alphanumeric()
                    || bytes[i + from_len] == b'_'
                    || (!allow_dot_after && bytes[i + from_len] == b'.'));
            if prev_ok && next_ok {
                out.push_str(to);
                i += from_len;
                continue;
            }
        }
        let ch = input[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Upgrades legacy Typst <=0.13 math symbols emitted by `mitex 0.2.4`
/// to Typst 0.15.1 canonical symbols without shadowing `angle` (∠).
pub fn upgrade_typst_015_math_symbols(raw: &str) -> String {
    let mut s = raw.to_string();

    // Exact dotted symbol renames between Typst <=0.13 (MiTeX 0.2.4) and Typst 0.15.1
    for (from, to, allow_dot_after) in [
        ("planck.reduce", "planck", false),
        ("angle.l", "chevron.l", false),
        ("angle.r", "chevron.r", false),
        ("integral.sect", "integral.inter", false),
        ("sect", "inter", true),
        ("dot.circle", "dot.o", true),
        ("plus.circle", "plus.o", true),
        ("minus.circle", "minus.o", true),
        ("times.circle", "times.o", true),
        ("ast.circle", "ast.o", false),
        ("dash.circle", "dash.o", false),
        ("circle.nested", "\"\u{229A}\"", false),
        ("arrow.l.dash", "arrow.l.dashed", false),
        ("arrow.r.dash", "arrow.r.dashed", false),
        ("bracket.l.double", "bracket.l.stroked", false),
        ("bracket.r.double", "bracket.r.stroked", false),
        ("ohm.inv", "\"\u{2127}\"", false),
        ("diff", "partial", false),
    ] {
        s = replace_symbol_token(&s, from, to, allow_dot_after);
    }

    // `set` is a reserved keyword in Typst code mode, so rewrite MiTeX's `set(...)`
    // call (from LaTeX `\set{...}`) to `mitexset(...)` defined in the offline prelude.
    {
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len() + 16);
        let mut i = 0usize;
        while i < bytes.len() {
            if s[i..].starts_with("set(") {
                let prev_ok = i == 0
                    || !(bytes[i - 1].is_ascii_alphanumeric()
                        || bytes[i - 1] == b'_'
                        || bytes[i - 1] == b'.');
                if prev_ok {
                    out.push_str("mitexset(");
                    i += 4;
                    continue;
                }
            }
            let ch = s[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
        s = out;
    }

    s
}

/// Strips accidental `$` / `$$` wrappers from a dedicated `latex` field before MiTeX conversion.
fn strip_math_delimiters(latex: &str) -> &str {
    let t = latex.trim();
    if let Some(inner) = t.strip_prefix("$$").and_then(|s| s.strip_suffix("$$")) {
        inner.trim()
    } else if let Some(inner) = t.strip_prefix('$').and_then(|s| s.strip_suffix('$')) {
        inner.trim()
    } else {
        t
    }
}

/// Converts a single LaTeX math string to Typst 0.15.1 math syntax and strictly verifies
/// that Typst compiles it cleanly. NEVER silently drops or alters unsupported LaTeX macros.
pub fn convert_and_verify_latex(
    latex: &str,
    location: &str,
    step_index: Option<usize>,
    fonts: &[Font],
) -> Result<String, LatexConversionError> {
    let clean = strip_math_delimiters(latex);
    if clean.is_empty() {
        return Err(LatexConversionError {
            location: location.to_string(),
            step_index,
            latex: latex.to_string(),
            reason: "空の数式文字列です".to_string(),
        });
    }

    let raw_typst = mitex::convert_math(clean, None).map_err(|err| LatexConversionError {
        location: location.to_string(),
        step_index,
        latex: latex.to_string(),
        reason: format!("未対応または不正な LaTeX 構文: {err}"),
    })?;

    let upgraded = upgrade_typst_015_math_symbols(&raw_typst);

    // Strictly verify this converted formula compiles cleanly in Typst 0.15.1 offline
    let snippet = format!("{OFFLINE_MATH_PRELUDE}\n$ {upgraded} $");
    let world = SandboxedWorld::new(snippet, HashMap::new(), fonts.to_vec()).map_err(|e| {
        LatexConversionError {
            location: location.to_string(),
            step_index,
            latex: latex.to_string(),
            reason: format!("Typst 検証環境初期化エラー: {e}"),
        }
    })?;

    compile_world_sync(&world).map_err(|compile_err| LatexConversionError {
        location: location.to_string(),
        step_index,
        latex: latex.to_string(),
        reason: format!("Typst 数式コンパイルエラー: {compile_err}"),
    })?;

    Ok(upgraded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PdfSettings;
    use crate::pdf::compiler::{CompilePdfMetadata, cached_fonts, compile_solution_to_pdf};

    #[test]
    fn test_big_delimiters_and_ams_math_convert_offline() {
        let fonts = cached_fonts();
        let samples = [
            r"x(t)=e^{-t}\bigl(A\cos(2t)+B\sin(2t)\bigr),\qquad x(0)=A=1,\qquad x'(0)=-A+2B=0",
            r"\Bigl[\frac{1}{2} m v^2 + \frac{1}{2} k x^2\Bigr] = \biggl\{ E_0 \biggr\} + \Biggl| \psi \Biggr|",
            r"\big( a + b \big) \Big( c + d \Big) \bigg[ e \bigg] \Bigg\{ f \Bigg\}",
        ];
        for sample in samples {
            convert_and_verify_latex(sample, "test", None, fonts)
                .unwrap_or_else(|e| panic!("failed for `{sample}`: {e:?}"));
        }
    }

    #[test]
    fn test_real_measure_theory_and_university_math_physics_formulas() {
        let fonts = cached_fonts();
        let samples = [
            // Measure theory (real user solution)
            r"\mathcal F\subseteq\mathcal P(X),\qquad X\in\mathcal F,\qquad E\in\mathcal F\Longrightarrow X\setminus E\in\mathcal F,\qquad (E_n)_{n=1}^{\infty}\subseteq\mathcal F\Longrightarrow\bigcup_{n=1}^{\infty}E_n\in\mathcal F.",
            r"\mu:\mathcal F\longrightarrow[0,\infty],\qquad\mu(\varnothing)=0,\qquad\mu\!\left(\bigcup_{n=1}^{\infty}E_n\right)=\sum_{n=1}^{\infty}\mu(E_n)\quad\left(E_n\in\mathcal F,\ E_i\cap E_j=\varnothing\ \text{for }i\ne j\right).",
            r"P:\mathcal F\longrightarrow[0,1],\qquad P\text{ は測度},\qquad P(X)=1.",
            r"\varnothing\ne\mathcal M\subseteq\mathcal P(X),\qquad\begin{aligned}E_n\in\mathcal M,\ E_n\subseteq E_{n+1}\ (n\ge1)&\Longrightarrow\bigcup_{n=1}^{\infty}E_n\in\mathcal M,\\ E_n\in\mathcal M,\ E_{n+1}\subseteq E_n\ (n\ge1)&\Longrightarrow\bigcap_{n=1}^{\infty}E_n\in\mathcal M.\end{aligned}",
            r"E\subseteq X\text{ が }\mu^*\text{-可測}\quad\Longleftrightarrow\quad\forall A\subseteq X,\quad\mu^*(A)=\mu^*(A\cap E)+\mu^*(A\setminus E).",
            r"\mu^*:\mathcal P(X)\longrightarrow[0,\infty],\qquad\mu^*(\varnothing)=0,\qquad A\subseteq B\Longrightarrow\mu^*(A)\le\mu^*(B),\qquad\mu^*\!\left(\bigcup_{n=1}^{\infty}A_n\right)\le\sum_{n=1}^{\infty}\mu^*(A_n).",
            r"\varnothing=X\setminus X\in\mathcal F,\qquad\bigcap_{n=1}^{\infty}E_n=X\setminus\bigcup_{n=1}^{\infty}(X\setminus E_n)\in\mathcal F.",
            r"\sum_{n=1}^{\infty}\mu(E_n)=\lim_{N\to\infty}\sum_{n=1}^{N}\mu(E_n).",
            r"1=P(X)=P(E)+P(X\setminus E),\qquad 0\le P(E)\le1\quad(E\in\mathcal F).",
            r"E_n\uparrow E\Longleftrightarrow\left(E_n\subseteq E_{n+1},\ E=\bigcup_{n=1}^{\infty}E_n\right),\qquad E_n\downarrow E\Longleftrightarrow\left(E_{n+1}\subseteq E_n,\ E=\bigcap_{n=1}^{\infty}E_n\right).",
            r"A=(A\cap E)\cup(A\setminus E),\qquad\mu^*(A)\le\mu^*(A\cap E)+\mu^*(A\setminus E).",
            // Additional university math & physics (tensor products, direct sums, quantum braket, extensible arrows, substack, boxed, operators, variants)
            r"V = \bigoplus_{i=1}^{n} V_i,\qquad W = U \otimes V,\qquad \bigotimes_{k=1}^{m} \mathcal{H}_k,\qquad \llbracket f \rrbracket",
            r"\bra{\psi} \hat{H} \ket{\phi} = \braket{\psi | \hat{H} | \phi}, \qquad \overset{\text{def}}{=} \underset{x \to 0}{\lim} \dfrac{\sin x}{x} = \boxed{1}",
            r"A \xrightarrow{f} B \xleftarrow{g} C, \qquad \sum_{\substack{1 \le i \le n \\ i \ne j}} a_{ij} \equiv 0 \pmod{p}",
            r"\argmax_{x \in \mathbb{R}} f(x), \qquad \operatorname{tr}(A), \qquad \varGamma(z) = \int_0^\infty t^{z-1} e^{-t}\,dt, \qquad \cancel{x} + \bcancel{y} + \xcancel{z}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix} \begin{bmatrix} x \\ y \end{bmatrix} = \begin{cases} 1 & x \ge 0 \\ 0 & x < 0 \end{cases}",
            r"\begin{align*} \nabla \cdot \mathbf{E} &= \frac{\rho}{\varepsilon_0} \\ \nabla \times \mathbf{B} &= \mu_0 \mathbf{J} + \mu_0 \varepsilon_0 \frac{\partial \mathbf{E}}{\partial t} \end{align*}",
            r"\mathbf{F} = m\mathbf{a}, \qquad \mathrm{d}s^2 = -c^2\,\mathrm{d}t^2 + \mathrm{d}x^2, \qquad \dashrightarrow \dashleftarrow \ominus \odot \bigodot \circledast \circledcirc \circleddash \mho",
        ];
        for (idx, sample) in samples.iter().enumerate() {
            convert_and_verify_latex(sample, "test", Some(idx), fonts)
                .unwrap_or_else(|e| panic!("failed for #{idx} `{sample}`: {}", e.reason));
        }
    }

    #[tokio::test]
    async fn test_full_measure_theory_solution_pdf_compilation() {
        let sol = serde_json::json!({
            "schema_version": 1,
            "subject": "mathematics",
            "field": "real_analysis",
            "title": "σ-集合体・測度・確率測度・単調クラス・外測度に関する可測性の定義",
            "mode": "detailed",
            "assumptions": [
                "全体集合をXとし、そのべき集合を𝒫(X)とする。",
                "μ*はX上の外測度とする。"
            ],
            "definitions": [
                {
                    "name": "(2) 測度",
                    "explanation": "可測空間(X,𝓕)上の測度",
                    "latex": "\\mu:\\mathcal F\\longrightarrow[0,\\infty],\\qquad\\mu(\\varnothing)=0,\\qquad\\mu\\!\\left(\\bigcup_{n=1}^{\\infty}E_n\\right)=\\sum_{n=1}^{\\infty}\\mu(E_n)\\quad\\left(E_n\\in\\mathcal F,\\ E_i\\cap E_j=\\varnothing\\ \\text{for }i\\ne j\\right)."
                },
                {
                    "name": "(4) 単調クラス",
                    "explanation": "単調増加列と単調減少列に閉じた集合族",
                    "latex": "\\varnothing\\ne\\mathcal M\\subseteq\\mathcal P(X),\\qquad\\begin{aligned}E_n\\in\\mathcal M,\\ E_n\\subseteq E_{n+1}\\ (n\\ge1)&\\Longrightarrow\\bigcup_{n=1}^{\\infty}E_n\\in\\mathcal M,\\\\ E_n\\in\\mathcal M,\\ E_{n+1}\\subseteq E_n\\ (n\\ge1)&\\Longrightarrow\\bigcap_{n=1}^{\\infty}E_n\\in\\mathcal M.\\end{aligned}"
                }
            ],
            "steps": [
                {
                    "type": "explanation",
                    "heading": "(5) μ*-可測性の等式",
                    "text": "外測度の劣加法性",
                    "latex": "E\\subseteq X\\text{ が }\\mu^*\\text{-可測}\\quad\\Longleftrightarrow\\quad\\forall A\\subseteq X,\\quad\\mu^*(A)=\\mu^*(A\\cap E)+\\mu^*(A\\setminus E).",
                    "display": true
                }
            ],
            "answer": {
                "text": "5つの基本定義を整理した。",
                "latex": null,
                "unit": null
            }
        });
        let compiled = compile_solution_to_pdf(
            &sol,
            &PdfSettings::default(),
            &CompilePdfMetadata {
                document_title: "測度論演習",
                problem_label: Some("問1"),
                problem_statement: None,
                version_number: 1,
                model_name: Some("codex"),
                generated_at: "2026-11-26T14:20:00Z",
            },
            Vec::new(),
        )
        .await
        .expect("full measure-theory solution PDF should compile cleanly");
        assert!(compiled.pdf_bytes.starts_with(b"%PDF-"));
        assert!(compiled.page_count >= 1);
    }

    #[test]
    #[ignore]
    fn audit_entire_mitex_spec_against_typst_015() {
        // Dynamically find any default.rkyv built by mitex-spec-gen
        let build_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/build");
        let rkyv_path = std::fs::read_dir(build_dir.join("mitex-spec-gen"))
            .expect("mitex-spec-gen build dir")
            .filter_map(Result::ok)
            .map(|e| e.path().join("out/mitex-artifacts/spec/default.rkyv"))
            .find(|p| p.is_file())
            .expect("default.rkyv");
        let bytes = std::fs::read(&rkyv_path).expect("read default.rkyv");
        let spec = mitex::CommandSpec::from_bytes(&bytes);
        let fonts = cached_fonts();

        let mut zero_arg_broken: Vec<(String, String, String)> = Vec::new();
        for (name, item) in spec.items() {
            if let mitex::CommandSpecItem::Cmd(cmd) = item {
                let alias = cmd.alias.clone().unwrap_or_else(|| name.to_string());
                let is_zero = matches!(
                    cmd.args,
                    mitex::ArgShape::Right {
                        pattern: mitex::ArgPattern::None
                    }
                );
                if is_zero {
                    let upgraded = upgrade_typst_015_math_symbols(&alias);
                    let snippet = format!("{OFFLINE_MATH_PRELUDE}\n$ {upgraded} $");
                    if let Ok(world) = SandboxedWorld::new(snippet, HashMap::new(), fonts.to_vec())
                        && let Err(err) = compile_world_sync(&world)
                    {
                        zero_arg_broken.push((name.to_string(), alias, err));
                    }
                }
            }
        }
        zero_arg_broken.sort();
        for (n, a, e) in &zero_arg_broken {
            println!("STILL BROKEN \\{n} => `{a}` : {e}");
        }
        assert!(
            zero_arg_broken.is_empty(),
            "{} zero-arg MiTeX symbols still broken",
            zero_arg_broken.len()
        );
    }
}
