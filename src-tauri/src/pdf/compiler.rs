use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use std::time::Duration as StdDuration;

use chrono::{Datelike, Timelike, Utc};
use serde_json::Value;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

use crate::domain::{LatexConversionError, PdfSettings};
use crate::pdf::latex::convert_and_verify_latex;

const TEMPLATE_COMMON: &str = include_str!("../../../templates/common.typ");
const TEMPLATE_MATH: &str = include_str!("../../../templates/mathematics.typ");
const TEMPLATE_PHYSICS: &str = include_str!("../../../templates/physics.typ");
const TEMPLATE_CONCISE: &str = include_str!("../../../templates/concise.typ");
const TEMPLATE_DETAILED: &str = include_str!("../../../templates/detailed.typ");

const COMPILE_TIMEOUT_SECS: u64 = 20;

/// Strictly sandboxed in-memory Typst World:
/// - Offline only (explicitly denies `VirtualRoot::Package(_)`)
/// - Filesystem-isolated (only embedded templates + explicitly registered virtual artifacts readable)
pub struct SandboxedWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    sources: HashMap<FileId, Source>,
    files: HashMap<FileId, Bytes>,
}

impl SandboxedWorld {
    pub fn new(
        typst_source: String,
        virtual_files: HashMap<String, Vec<u8>>,
        fonts: Vec<Font>,
    ) -> Result<Self, String> {
        let book = FontBook::from_fonts(&fonts);
        let main_vpath = VirtualPath::new("main.typ").map_err(|e| format!("{e:?}"))?;
        let main_id = FileId::new(RootedPath::new(VirtualRoot::Project, main_vpath));
        let main_source = Source::new(main_id, typst_source);

        let mut sources = HashMap::new();
        sources.insert(main_id, main_source);

        // Register verified built-in templates as virtual project sources
        for (name, content) in [
            ("common.typ", TEMPLATE_COMMON),
            ("mathematics.typ", TEMPLATE_MATH),
            ("physics.typ", TEMPLATE_PHYSICS),
            ("concise.typ", TEMPLATE_CONCISE),
            ("detailed.typ", TEMPLATE_DETAILED),
        ] {
            let vp = VirtualPath::new(name).map_err(|e| format!("{e:?}"))?;
            let fid = FileId::new(RootedPath::new(VirtualRoot::Project, vp));
            sources.insert(fid, Source::new(fid, content.to_string()));
        }

        let mut files = HashMap::new();
        for (vpath, bytes) in virtual_files {
            let vp = VirtualPath::new(&vpath).map_err(|e| format!("{e:?}"))?;
            let fid = FileId::new(RootedPath::new(VirtualRoot::Project, vp));
            files.insert(fid, Bytes::new(bytes));
        }

        Ok(Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main_id,
            sources,
            files,
        })
    }
}

impl World for SandboxedWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return Err(FileError::AccessDenied);
        }
        self.sources
            .get(&id)
            .cloned()
            .ok_or(FileError::AccessDenied)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return Err(FileError::AccessDenied);
        }
        if let Some(src) = self.sources.get(&id) {
            return Ok(Bytes::from_string(src.text().to_string()));
        }
        self.files.get(&id).cloned().ok_or(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        let now = Utc::now();
        Datetime::from_ymd_hms(
            now.year(),
            now.month().try_into().ok()?,
            now.day().try_into().ok()?,
            now.hour().try_into().ok()?,
            now.minute().try_into().ok()?,
            now.second().try_into().ok()?,
        )
    }
}

/// Escapes untrusted plain text so Typst markup/code (`#read(...)`, `#import`, `$`, etc.)
/// can NEVER execute when embedded into verified templates.
pub fn escape_typst_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for ch in input.chars() {
        match ch {
            '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '[' | ']' | '{' | '}' | '~'
            | '=' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Escapes a string literal inside Typst double quotes `"..."`.
fn escape_typst_string_literal(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', "")
}

/// Loads embedded Typst math/serif/mono fonts plus Japanese fonts once per process.
pub fn cached_fonts() -> &'static Vec<Font> {
    static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut fonts = Vec::new();
        for data in typst_assets::fonts() {
            let bytes = Bytes::new(data);
            for font in Font::iter(bytes) {
                fonts.push(font);
            }
        }
        let jp_paths = [
            r"C:\Windows\Fonts\NotoSerifJP-VF.ttf",
            r"C:\Windows\Fonts\NotoSansJP-VF.ttf",
            r"C:\Windows\Fonts\BIZ-UDMinchoM.ttc",
            r"C:\Windows\Fonts\BIZ-UDGothicR.ttc",
            r"C:\Windows\Fonts\yumin.ttf",
            r"C:\Windows\Fonts\meiryo.ttc",
            "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSerifJP-Regular.ttf",
            "/System/Library/Fonts/Hiragino Mincho ProN.ttc",
        ];
        for path in jp_paths {
            if let Ok(raw) = fs::read(path) {
                let bytes = Bytes::new(raw);
                for font in Font::iter(bytes) {
                    fonts.push(font);
                }
            }
        }
        fonts
    })
}

pub fn compile_world_sync(world: &SandboxedWorld) -> Result<PagedDocument, String> {
    let warned = typst::compile::<PagedDocument>(world);
    warned.output.map_err(|errs| {
        errs.iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })
}

#[derive(Debug)]
pub struct CompiledSolutionPdf {
    pub typst_source: String,
    pub pdf_bytes: Vec<u8>,
    pub page_count: usize,
}

pub struct CompilePdfMetadata<'a> {
    pub document_title: &'a str,
    pub problem_label: Option<&'a str>,
    pub problem_statement: Option<&'a str>,
    pub version_number: i64,
    pub model_name: Option<&'a str>,
    pub generated_at: &'a str,
}

/// Converts a Schema v1 validated JSON solution into verified Typst markup and compiles it to PDF
/// inside a sandboxed offline world with a hard timeout.
pub async fn compile_solution_to_pdf(
    solution_json: &Value,
    pdf_settings: &PdfSettings,
    meta: &CompilePdfMetadata<'_>,
    virtual_artifacts: HashMap<String, Vec<u8>>,
) -> Result<CompiledSolutionPdf, PdfCompileFailure> {
    let json_owned = solution_json.clone();
    let settings_owned = pdf_settings.clone();
    let doc_title = meta.document_title.to_string();
    let prob_label = meta.problem_label.map(ToString::to_string);
    let prob_statement = meta.problem_statement.map(ToString::to_string);
    let version_number = meta.version_number;
    let model_name = meta.model_name.map(ToString::to_string);
    let generated_at = meta.generated_at.to_string();

    let task = tokio::task::spawn_blocking(move || {
        let fonts = cached_fonts();
        let typst_source = render_solution_typst(
            &json_owned,
            &settings_owned,
            &CompilePdfMetadata {
                document_title: &doc_title,
                problem_label: prob_label.as_deref(),
                problem_statement: prob_statement.as_deref(),
                version_number,
                model_name: model_name.as_deref(),
                generated_at: &generated_at,
            },
            fonts,
        )?;

        let world = SandboxedWorld::new(typst_source.clone(), virtual_artifacts, fonts.clone())
            .map_err(PdfCompileFailure::Compile)?;
        let paged = compile_world_sync(&world).map_err(PdfCompileFailure::Compile)?;
        let page_count = paged.pages().len();
        let pdf_bytes = typst_pdf::pdf(&paged, &PdfOptions::default()).map_err(|errs| {
            PdfCompileFailure::Compile(
                errs.iter()
                    .map(|e| e.message.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;

        Ok(CompiledSolutionPdf {
            typst_source,
            pdf_bytes,
            page_count,
        })
    });

    match tokio::time::timeout(StdDuration::from_secs(COMPILE_TIMEOUT_SECS), task).await {
        Ok(Ok(res)) => res,
        Ok(Err(join_err)) => Err(PdfCompileFailure::Compile(format!(
            "PDF コンパイルワーカーエラー: {join_err}"
        ))),
        Err(_) => Err(PdfCompileFailure::Compile(format!(
            "PDF コンパイルがタイムアウトしました（上限 {COMPILE_TIMEOUT_SECS} 秒）"
        ))),
    }
}

#[derive(Debug)]
pub enum PdfCompileFailure {
    Latex(Vec<LatexConversionError>),
    Compile(String),
}

fn render_solution_typst(
    sol: &Value,
    settings: &PdfSettings,
    meta: &CompilePdfMetadata<'_>,
    fonts: &[Font],
) -> Result<String, PdfCompileFailure> {
    let mut latex_errors: Vec<LatexConversionError> = Vec::new();
    let mut convert_eq = |latex: &str, loc: &str, step_idx: Option<usize>| -> Option<String> {
        match convert_and_verify_latex(latex, loc, step_idx, fonts) {
            Ok(typ) => Some(typ),
            Err(err) => {
                latex_errors.push(err);
                None
            }
        }
    };

    let subject = sol
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("mathematics");
    let field = sol.get("field").and_then(Value::as_str);
    let title = sol.get("title").and_then(Value::as_str).unwrap_or("解答");
    let mode = sol
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("standard");

    let paper = match settings.paper_size.as_str() {
        "letter" => "us-letter",
        "b5" => "iso-b5",
        _ => "a4",
    };

    let style_import = match settings.default_template.as_str() {
        "concise" => "concise.typ",
        "detailed" => "detailed.typ",
        _ => {
            if mode == "concise" {
                "concise.typ"
            } else {
                "detailed.typ"
            }
        }
    };

    let mut out = String::with_capacity(4096);
    out.push_str("#import \"common.typ\": *\n");
    out.push_str(
        "#import \"mathematics.typ\": math_definition_box, math_theorem_box, math_proof_block\n",
    );
    out.push_str("#import \"physics.typ\": physics_setup_box, physics_check_box\n");
    out.push_str(&format!(
        "#import \"{style_import}\": style_body_size, style_label\n\n"
    ));

    out.push_str(&format!(
        "#show: theorem_setup.with(paper: \"{paper}\", body_size: style_body_size, doc_title: \"{}\", header_label: style_label)\n\n",
        escape_typst_string_literal(title)
    ));

    // Title + Subject/Problem Header
    out.push_str(&format!("= {}\n", escape_typst_text(title)));

    let mut meta_parts: Vec<String> = Vec::new();
    let subj_label = match subject {
        "mathematics" => "大学数学",
        "physics" => "大学物理",
        _ => "その他",
    };
    meta_parts.push(format!("科目: {subj_label}"));
    if let Some(f) = field
        && !f.trim().is_empty()
    {
        meta_parts.push(format!("分野: {f}"));
    }
    if let Some(lbl) = meta.problem_label {
        meta_parts.push(format!("問題: {lbl}"));
    }
    meta_parts.push(format!("モード: {mode}"));
    if settings.show_version_number {
        meta_parts.push(format!("バージョン: v{}", meta.version_number));
    }
    if settings.show_model_name {
        let model_label = meta.model_name.unwrap_or("未取得");
        meta_parts.push(format!("モデル: {model_label}"));
    }
    if settings.show_generated_at {
        meta_parts.push(format!("生成日時: {}", meta.generated_at));
    }

    out.push_str(&format!(
        "#text(size: 8.5pt, fill: luma(100))[{}]\n#v(6pt)\n",
        escape_typst_text(&meta_parts.join("  |  "))
    ));

    // Original Problem Statement excerpt (if provided)
    if let Some(stmt) = meta.problem_statement
        && !stmt.trim().is_empty()
    {
        out.push_str("#info_box(\"対象問題\", accent: rgb(\"#475569\"), bg: rgb(\"#f8fafc\"))[\n");
        out.push_str(&format!("  {}\n", escape_typst_text(stmt.trim())));
        out.push_str("]\n#v(6pt)\n");
    }

    // Assumptions / Coordinate System / Given Quantities / Principles
    let assumptions = str_array(sol.get("assumptions"));
    let coord = sol
        .get("coordinate_system")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let principles = str_array(sol.get("principles"));
    let domain_bound = sol
        .get("domain_and_boundary")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let given_q = sol
        .get("given_quantities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if !assumptions.is_empty()
        || coord.is_some()
        || !principles.is_empty()
        || !given_q.is_empty()
        || domain_bound.is_some()
    {
        out.push_str("== 前提条件・設定\n");
        if let Some(c) = coord {
            out.push_str(&format!(
                "#physics_setup_box(\"座標系\")[{}]\n#v(4pt)\n",
                escape_typst_text(c)
            ));
        }
        if let Some(db) = domain_bound {
            out.push_str(&format!(
                "- *定義域・境界条件*: {}\n",
                escape_typst_text(db)
            ));
        }
        if !given_q.is_empty() {
            out.push_str("*与えられた物理量・記号*:\n");
            for (idx, q) in given_q.iter().enumerate() {
                let sym = q.get("symbol").and_then(Value::as_str).unwrap_or("");
                let desc = q.get("description").and_then(Value::as_str).unwrap_or("");
                let unit = q.get("unit").and_then(Value::as_str);
                let sym_typ = if sym.contains('\\') || sym.contains('_') || sym.contains('^') {
                    convert_eq(sym, &format!("given_quantities[{idx}].symbol"), None)
                        .map(|t| format!("${t}$"))
                        .unwrap_or_else(|| escape_typst_text(sym))
                } else {
                    format!("${}$", escape_typst_text(sym))
                };
                if let Some(u) = unit
                    && !u.trim().is_empty()
                {
                    out.push_str(&format!(
                        "- {sym_typ}: {} [{}]\n",
                        escape_typst_text(desc),
                        escape_typst_text(u)
                    ));
                } else {
                    out.push_str(&format!("- {sym_typ}: {}\n", escape_typst_text(desc)));
                }
            }
        }
        if !assumptions.is_empty() {
            out.push_str("*仮定*:\n");
            for a in assumptions {
                out.push_str(&format!("- {}\n", escape_typst_text(&a)));
            }
        }
        if !principles.is_empty() {
            out.push_str("*適用する法則・原理*:\n");
            for p in principles {
                out.push_str(&format!("- {}\n", escape_typst_text(&p)));
            }
        }
        out.push('\n');
    }

    // Mathematics Definitions & Theorems
    if let Some(defs) = sol.get("definitions").and_then(Value::as_array)
        && !defs.is_empty()
    {
        out.push_str("== 使用する定義\n");
        for (idx, d) in defs.iter().enumerate() {
            let name = d.get("name").and_then(Value::as_str).unwrap_or("定義");
            let expl = d.get("explanation").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "#math_definition_box(\"{}\")[\n  {}\n",
                escape_typst_string_literal(name),
                escape_typst_text(expl)
            ));
            if let Some(ltx) = d
                .get("latex")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                && let Some(typ) = convert_eq(ltx, &format!("definitions[{idx}].latex"), None)
            {
                out.push_str(&format!("  $ {typ} $\n"));
            }
            out.push_str("]\n#v(4pt)\n");
        }
    }

    if let Some(thms) = sol.get("theorems").and_then(Value::as_array)
        && !thms.is_empty()
    {
        out.push_str("== 引用する定理・補題\n");
        for th in thms {
            let name = th.get("name").and_then(Value::as_str).unwrap_or("定理");
            let cond = th.get("conditions").and_then(Value::as_str);
            let app = th.get("application").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "#math_theorem_box(\"{}\")[\n",
                escape_typst_string_literal(name)
            ));
            if let Some(c) = cond
                && !c.trim().is_empty()
            {
                out.push_str(&format!("  *成立条件*: {} \\\n", escape_typst_text(c)));
            }
            out.push_str(&format!("  {}\n]\n#v(4pt)\n", escape_typst_text(app)));
        }
    }

    // Main Solution Steps
    out.push_str("== 解法と途中式\n");
    if let Some(steps) = sol.get("steps").and_then(Value::as_array) {
        for (idx, step) in steps.iter().enumerate() {
            if let Some(h) = step
                .get("heading")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                out.push_str(&format!("*{}*\n\n", escape_typst_text(h)));
            }
            if let Some(txt) = step
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                out.push_str(&format!("{}\n\n", escape_typst_text(txt)));
            }
            if let Some(ltx) = step
                .get("latex")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                && let Some(typ) = convert_eq(ltx, &format!("steps[{idx}].latex"), Some(idx))
            {
                let display = step.get("display").and_then(Value::as_bool).unwrap_or(true);
                if display {
                    out.push_str(&format!("$ {typ} $\n\n"));
                } else {
                    out.push_str(&format!("${typ}$\n\n"));
                }
            }
        }
    }

    // Proof block (for university math)
    if let Some(proof_steps) = sol.get("proof").and_then(Value::as_array)
        && !proof_steps.is_empty()
    {
        out.push_str("== 証明\n#math_proof_block[\n");
        for (idx, ps) in proof_steps.iter().enumerate() {
            let txt = ps.get("text").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!("  {}\n\n", escape_typst_text(txt)));
            if let Some(ltx) = ps
                .get("latex")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                && let Some(typ) = convert_eq(ltx, &format!("proof[{idx}].latex"), None)
            {
                out.push_str(&format!("  $ {typ} $\n\n"));
            }
        }
        out.push_str("]\n#v(6pt)\n");
    }

    // Final Answer Box
    if let Some(ans) = sol.get("answer").and_then(Value::as_object) {
        out.push_str("#answer_box[\n");
        if let Some(txt) = ans
            .get("text")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            out.push_str(&format!("  {}\n\n", escape_typst_text(txt)));
        }
        if let Some(ltx) = ans
            .get("latex")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            && let Some(typ) = convert_eq(ltx, "answer.latex", None)
        {
            out.push_str(&format!("  $ {typ} $\n"));
        }
        if let Some(unit) = ans
            .get("unit")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            out.push_str(&format!(
                "  #text(size: 9pt, fill: luma(90))[単位: {}]\n",
                escape_typst_text(unit)
            ));
        }
        out.push_str("]\n#v(6pt)\n");
    }

    // Dimensional Check & Limiting Cases
    let dim_check = sol.get("dimensional_check").and_then(Value::as_object);
    let limiting = str_array(sol.get("limiting_cases"));
    if dim_check.is_some() || !limiting.is_empty() {
        out.push_str("== 物理的検証・考察\n");
        if let Some(dc) = dim_check {
            out.push_str("#physics_check_box(\"次元解析\")[\n");
            if let Some(note) = dc.get("note").and_then(Value::as_str) {
                out.push_str(&format!("  {}\n", escape_typst_text(note)));
            }
            if let Some(ltx) = dc
                .get("latex")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                && let Some(typ) = convert_eq(ltx, "dimensional_check.latex", None)
            {
                out.push_str(&format!("  $ {typ} $\n"));
            }
            out.push_str("]\n#v(4pt)\n");
        }
        if !limiting.is_empty() {
            out.push_str("*極限ケースの考察*:\n");
            for lc in limiting {
                out.push_str(&format!("- {}\n", escape_typst_text(&lc)));
            }
            out.push('\n');
        }
    }

    // Alternative Solutions & Counterexamples
    if let Some(alts) = sol.get("alternative_solutions").and_then(Value::as_array)
        && !alts.is_empty()
    {
        out.push_str("== 別解\n");
        for (idx, alt) in alts.iter().enumerate() {
            let t = alt.get("title").and_then(Value::as_str).unwrap_or("別解");
            let sum = alt.get("summary").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "- *{}*: {}\n",
                escape_typst_text(t),
                escape_typst_text(sum)
            ));
            if let Some(ltx) = alt
                .get("latex")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                && let Some(typ) =
                    convert_eq(ltx, &format!("alternative_solutions[{idx}].latex"), None)
            {
                out.push_str(&format!("  $ {typ} $\n"));
            }
        }
        out.push('\n');
    }

    let counterexamples = str_array(sol.get("counterexamples"));
    if !counterexamples.is_empty() {
        out.push_str("== 反例\n");
        for ce in counterexamples {
            out.push_str(&format!("- {}\n", escape_typst_text(&ce)));
        }
        out.push('\n');
    }

    // Verification Results (strictly distinguishing science_mcp vs ai_self_check)
    if let Some(verifs) = sol.get("verification").and_then(Value::as_array)
        && !verifs.is_empty()
    {
        out.push_str("== 検算・検証記録\n");
        for v in verifs {
            let src = match v.get("source_kind").and_then(Value::as_str).unwrap_or("") {
                "science_mcp" => "計算ツール検証 (Science MCP)",
                "user_check" => "ユーザー確認",
                _ => "AI 自己チェック（ツール未検証）",
            };
            let target = v.get("target").and_then(Value::as_str).unwrap_or("");
            let status = v.get("status").and_then(Value::as_str).unwrap_or("not_run");
            let note = v.get("note").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "- *[{} / {}]* {} {}\n",
                escape_typst_text(src),
                escape_typst_text(status),
                escape_typst_text(target),
                if note.is_empty() {
                    String::new()
                } else {
                    format!("— {}", escape_typst_text(note))
                }
            ));
        }
        out.push('\n');
    }

    let warnings = str_array(sol.get("warnings"));
    if !warnings.is_empty() {
        out.push_str(
            "#info_box(\"注意点・補足\", accent: rgb(\"#d97706\"), bg: rgb(\"#fffbeb\"))[\n",
        );
        for w in warnings {
            out.push_str(&format!("  - {}\n", escape_typst_text(&w)));
        }
        out.push_str("]\n");
    }

    if !latex_errors.is_empty() {
        return Err(PdfCompileFailure::Latex(latex_errors));
    }

    Ok(out)
}

fn str_array(val: Option<&Value>) -> Vec<String> {
    val.and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_compile_japanese_math_and_physics_pdf_and_escape_injection() {
        let sol = json!({
            "schema_version": 1,
            "problem_id": "p-1",
            "subject": "mathematics",
            "field": "calculus",
            "title": "ガウス積分と固有値 #read(\"/etc/passwd\")",
            "mode": "detailed",
            "assumptions": ["x は実数"],
            "definitions": [
                { "name": "ガウス積分", "latex": "\\int_0^{\\infty} e^{-x^2}\\,dx", "explanation": "広義積分として収束する。" }
            ],
            "theorems": [
                { "name": "フビニの定理", "conditions": "非負可測関数", "application": "重積分の順序交換に用いる。" }
            ],
            "steps": [
                { "type": "explanation", "heading": "変数変換", "text": "極座標へ変換し #import \"@preview/evil:0.1.0\" は単なる文字として出力する。" },
                { "type": "equation", "latex": "\\int_0^{\\infty} e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}", "display": true },
                { "type": "equation", "latex": "A = \\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}, \\quad \\frac{\\partial^2 u}{\\partial t^2} = c^2 \\nabla^2 u", "display": true }
            ],
            "proof": [
                { "kind": "derivation", "text": "二重積分を極座標で計算する。", "latex": "I^2 = \\int_0^{\\pi/2} \\int_0^{\\infty} r e^{-r^2}\\,dr\\,d\\theta = \\frac{\\pi}{4}" }
            ],
            "answer": { "text": "正の平方根をとる。", "latex": "\\frac{\\sqrt{\\pi}}{2}" },
            "verification": [
                { "source_kind": "ai_self_check", "target": "符号・極限", "status": "passed", "note": "正の値" }
            ],
            "artifacts": [],
            "warnings": []
        });

        let compiled = compile_solution_to_pdf(
            &sol,
            &PdfSettings::default(),
            &CompilePdfMetadata {
                document_title: "テスト問題",
                problem_label: Some("第1問"),
                problem_statement: Some("ガウス積分を求めよ。"),
                version_number: 1,
                model_name: Some("mock-model"),
                generated_at: "2026-11-26T00:00:00Z",
            },
            HashMap::new(),
        )
        .await
        .expect("PDF must compile cleanly");

        assert!(compiled.pdf_bytes.starts_with(b"%PDF-"));
        assert!(compiled.page_count >= 1);
        assert!(compiled.typst_source.contains(r"\#read"));
    }

    #[tokio::test]
    async fn test_unsupported_latex_macro_fails_strictly_with_location() {
        let sol = json!({
            "schema_version": 1,
            "subject": "mathematics",
            "title": "未対応マクロ検出テスト",
            "mode": "standard",
            "steps": [
                { "type": "equation", "latex": "\\totallyUnknownCustomMacro{x} + \\frac{1}{2}", "display": true }
            ],
            "answer": { "latex": "x = 1" }
        });

        let res = compile_solution_to_pdf(
            &sol,
            &PdfSettings::default(),
            &CompilePdfMetadata {
                document_title: "テスト",
                problem_label: None,
                problem_statement: None,
                version_number: 1,
                model_name: None,
                generated_at: "2026-11-26T00:00:00Z",
            },
            HashMap::new(),
        )
        .await;

        match res {
            Err(PdfCompileFailure::Latex(errs)) => {
                assert_eq!(errs.len(), 1);
                assert_eq!(errs[0].location, "steps[0].latex");
                assert_eq!(errs[0].step_index, Some(0));
            }
            other => panic!("expected strict Latex conversion error, got: {other:?}"),
        }
    }

    #[test]
    fn test_sandboxed_world_denies_external_packages_and_read() {
        let fonts = cached_fonts();
        let pkg_src = r#"#import "@preview/mitex:0.2.4": *"#.to_string();
        let w1 = SandboxedWorld::new(pkg_src, HashMap::new(), fonts.clone()).expect("world");
        assert!(compile_world_sync(&w1).is_err());

        let read_src = r#"#read("../Cargo.toml")"#.to_string();
        let w2 = SandboxedWorld::new(read_src, HashMap::new(), fonts.clone()).expect("world");
        assert!(compile_world_sync(&w2).is_err());
    }
}
