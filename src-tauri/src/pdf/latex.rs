use std::collections::HashMap;

use typst::text::Font;

use crate::domain::LatexConversionError;
use crate::pdf::compiler::{SandboxedWorld, compile_world_sync};

/// Pure-Typst offline compatibility prelude bridging MiTeX macros onto Typst 0.15.1
/// without any external `@preview` packages.
pub const OFFLINE_MATH_PRELUDE: &str = r##"
#let mitexsqrt(..args) = {
  let pos = args.pos()
  if pos.len() == 2 { math.root(pos.at(0), pos.at(1)) } else { math.sqrt(pos.at(0)) }
}
#let mitexmathbf(x) = math.upright(math.bold(x))
#let mitexmathrm(x) = math.upright(x)
#let mitexmathbb(x) = math.bb(x)
#let mitexmathcal(x) = math.cal(x)
#let pmatrix(..args) = math.mat(delim: "(", ..args)
#let bmatrix(..args) = math.mat(delim: "[", ..args)
#let Bmatrix(..args) = math.mat(delim: "{", ..args)
#let vmatrix(..args) = math.mat(delim: "|", ..args)
#let Vmatrix(..args) = math.mat(delim: "||", ..args)
#let zws = sym.zws
"##;

/// Upgrades legacy Typst <=0.13 math symbols emitted by `mitex 0.2.4`
/// to Typst 0.15.1 canonical symbols without shadowing `angle` (∠).
pub fn upgrade_typst_015_math_symbols(raw: &str) -> String {
    let s = raw
        .replace("planck.reduce", "planck")
        .replace("angle.l", "chevron.l")
        .replace("angle.r", "chevron.r");

    // Replace standalone token `diff` with `partial` without touching substrings like `different`
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 16);
    let mut i = 0usize;
    while i < bytes.len() {
        if s[i..].starts_with("diff") {
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let next_ok = i + 4 == bytes.len() || !bytes[i + 4].is_ascii_alphanumeric();
            if prev_ok && next_ok {
                out.push_str("partial");
                i += 4;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
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
