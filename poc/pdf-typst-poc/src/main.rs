use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

/// Pure-Typst offline compatibility prelude bridging MiTeX helper macros
/// onto Typst 0.15.1 without any external `@preview` packages.
const OFFLINE_MATH_PRELUDE: &str = r##"
#let mitexsqrt(..args) = {
  let pos = args.pos()
  if pos.len() == 2 { math.root(pos.at(0), pos.at(1)) } else { math.sqrt(pos.at(0)) }
}
#let mitexmathbf(x) = math.upright(math.bold(x))
#let mitexmathrm(x) = math.upright(x)
#let pmatrix(..args) = math.mat(delim: "(", ..args)
#let bmatrix(..args) = math.mat(delim: "[", ..args)
#let Bmatrix(..args) = math.mat(delim: "{", ..args)
#let vmatrix(..args) = math.mat(delim: "|", ..args)
#let Vmatrix(..args) = math.mat(delim: "||", ..args)
#let zws = sym.zws
"##;

/// Upgrades legacy Typst <=0.13 math symbols emitted by converters (`mitex 0.2.4` /
/// `tex2typst-rs 0.4.1`) to Typst 0.15.1 canonical symbols without shadowing `angle` (∠).
fn upgrade_typst_015_math_symbols(raw: &str) -> String {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FormulaBenchmarkCase {
    id: &'static str,
    category: &'static str,
    latex: &'static str,
    expect_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ConversionComparison {
    id: String,
    category: String,
    latex: String,
    mitex_ok: bool,
    mitex_output: String,
    mitex_raw_compiles: bool,
    mitex_with_prelude_compiles: bool,
    tex2typst_ok: bool,
    tex2typst_output: String,
    tex2typst_raw_compiles: bool,
    tex2typst_with_prelude_compiles: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PdfPocReport {
    typst_version: String,
    embedded_font_count: usize,
    japanese_font_loaded: Option<String>,
    compile_ms: u128,
    pdf_export_ms: u128,
    pdf_bytes: usize,
    page_count: usize,
    svg_embedded: bool,
    png_embedded: bool,
    injection_escaped_safely: bool,
    unsupported_latex_detected_by_mitex: bool,
    unsupported_latex_rejected_by_typst: bool,
    formula_corpus_total: usize,
    mitex_raw_pass: usize,
    mitex_prelude_pass: usize,
    tex2typst_raw_pass: usize,
    tex2typst_prelude_pass: usize,
    comparisons: Vec<ConversionComparison>,
}

/// Strictly sandboxed in-memory Typst World:
/// - Offline only (rejects all `@preview/...` or external package imports)
/// - Filesystem-isolated (only explicitly registered virtual assets are readable)
struct SandboxedWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    source: Source,
    files: HashMap<FileId, Bytes>,
}

impl SandboxedWorld {
    fn new(
        typst_source: String,
        virtual_files: HashMap<String, Vec<u8>>,
        fonts: Vec<Font>,
    ) -> Result<Self, String> {
        let book = FontBook::from_fonts(&fonts);
        let main_vpath = VirtualPath::new("main.typ").map_err(|e| format!("{e:?}"))?;
        let main_id = FileId::new(RootedPath::new(VirtualRoot::Project, main_vpath));
        let source = Source::new(main_id, typst_source);
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
            source,
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
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        // Explicitly deny any package requests (offline-only enforcement)
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return Err(FileError::AccessDenied);
        }
        if id == self.main_id {
            return Ok(Bytes::from_string(self.source.text().to_string()));
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

/// Escapes untrusted plain text so Typst markup/code (e.g. `#read(...)`) can never execute.
fn escape_typst_text(input: &str) -> String {
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

fn load_fonts() -> (Vec<Font>, Option<String>) {
    let mut fonts: Vec<Font> = Vec::new();
    // 1. Built-in Typst assets (New Computer Modern Math, Libertinus Serif, DejaVu Sans Mono)
    for data in typst_assets::fonts() {
        let bytes = Bytes::new(data);
        for font in Font::iter(bytes) {
            fonts.push(font);
        }
    }

    // 2. Redistributable OFL Japanese fonts (probe installed Noto/BIZ UD fonts on Windows for PoC)
    let jp_candidates = [
        ("Noto Serif JP", r"C:\Windows\Fonts\NotoSerifJP-VF.ttf"),
        ("Noto Sans JP", r"C:\Windows\Fonts\NotoSansJP-VF.ttf"),
        ("BIZ UDMincho", r"C:\Windows\Fonts\BIZ-UDMinchoM.ttc"),
        ("BIZ UDGothic", r"C:\Windows\Fonts\BIZ-UDGothicR.ttc"),
    ];
    let mut loaded_jp: Option<String> = None;
    for (family_label, path) in jp_candidates {
        if let Ok(raw) = fs::read(path) {
            let bytes = Bytes::new(raw);
            let mut added = 0usize;
            for font in Font::iter(bytes) {
                fonts.push(font);
                added += 1;
            }
            if added > 0 && loaded_jp.is_none() {
                loaded_jp = Some(format!("{family_label} ({path})"));
            }
        }
    }
    (fonts, loaded_jp)
}

/// Generates a crisp 480x240 PNG plot purely in Rust to verify raster image embedding.
fn generate_sample_png() -> Result<Vec<u8>, String> {
    let width: u32 = 480;
    let height: u32 = 240;
    let mut buf = vec![255u8; (width * height * 3) as usize];

    let mut set_px = |x: i32, y: i32, r: u8, g: u8, b: u8| {
        if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
            let idx = ((y as u32 * width + x as u32) * 3) as usize;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
        }
    };

    for x in 30..(width as i32 - 20) {
        set_px(x, 120, 30, 41, 59);
    }
    for y in 20..(height as i32 - 20) {
        set_px(40, y, 30, 41, 59);
    }
    for dx in 0..400 {
        let t = dx as f64;
        let val = (-0.006 * t).exp() * (0.055 * t).sin();
        let x = 40 + dx;
        let y = 120 - (val * 85.0) as i32;
        for ox in -1..=1 {
            for oy in -1..=1 {
                set_px(x + ox, y + oy, 37, 99, 235);
            }
        }
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header error: {e}"))?;
        writer
            .write_image_data(&buf)
            .map_err(|e| format!("PNG data error: {e}"))?;
    }
    Ok(out)
}

fn compile_typst_snippet(typst_markup: &str, fonts: &[Font]) -> Result<PagedDocument, String> {
    let world = SandboxedWorld::new(typst_markup.to_string(), HashMap::new(), fonts.to_vec())?;
    let warned = typst::compile::<PagedDocument>(&world);
    warned.output.map_err(|errs| {
        errs.iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })
}

fn formula_corpus() -> Vec<FormulaBenchmarkCase> {
    vec![
        FormulaBenchmarkCase {
            id: "calc_integral",
            category: "微分積分",
            latex: r"\int_{0}^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "calc_partial",
            category: "偏微分",
            latex: r"\frac{\partial^2 u}{\partial t^2} = c^2 \left(\frac{\partial^2 u}{\partial x^2} + \frac{\partial^2 u}{\partial y^2}\right)",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "calc_limit_sum",
            category: "極限・級数",
            latex: r"\lim_{n \to \infty} \sum_{k=1}^{n} \frac{1}{k^2} = \frac{\pi^2}{6}",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "linalg_matrix",
            category: "線形代数（行列）",
            latex: r"A = \begin{pmatrix} a & b \\ c & d \end{pmatrix}, \quad \det(A) = ad - bc",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "linalg_eigen",
            category: "線形代数（固有値）",
            latex: r"\det(A - \lambda I) = 0 \implies \lambda_{1,2} = \frac{\mathrm{tr}(A) \pm \sqrt{(\mathrm{tr}(A))^2 - 4\det(A)}}{2}",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "piecewise_cases",
            category: "場合分け",
            latex: r"f(x) = \begin{cases} x^2 & (x \ge 0) \\ -x^2 & (x < 0) \end{cases}",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "complex_cauchy",
            category: "複素解析",
            latex: r"f(z_0) = \frac{1}{2\pi i} \oint_{\gamma} \frac{f(z)}{z - z_0}\,dz",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "set_quantifiers",
            category: "集合・論理",
            latex: r"\forall \varepsilon > 0,\ \exists \delta > 0,\ 0 < |x - a| < \delta \implies |f(x) - L| < \varepsilon",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "phys_mechanics",
            category: "古典力学",
            latex: r"m \ddot{x} + 2 m \gamma \dot{x} + m \omega_0^2 x = 0",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "phys_maxwell",
            category: "電磁気学",
            latex: r"\nabla \times \mathbf{E} = -\frac{\partial \mathbf{B}}{\partial t}, \quad \nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "phys_quantum",
            category: "量子力学",
            latex: r"i \hbar \frac{\partial}{\partial t} |\psi(t)\rangle = \hat{H} |\psi(t)\rangle",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "phys_units",
            category: "単位・次元",
            latex: r"a = g \sin\theta \quad [\mathrm{m/s^2}]",
            expect_valid: true,
        },
        FormulaBenchmarkCase {
            id: "unsupported_macro",
            category: "未対応マクロ検出テスト",
            latex: r"\totallyUnknownCustomMacro{x} + \frac{1}{2}",
            expect_valid: false,
        },
    ]
}

fn run_poc(output_dir: &Path) -> Result<PdfPocReport, String> {
    fs::create_dir_all(output_dir).map_err(|e| format!("Failed creating output dir: {e}"))?;
    let (fonts, japanese_font_loaded) = load_fonts();

    // 1. Compare mitex vs tex2typst-rs (raw vs offline Typst 0.15 compatibility prelude)
    let corpus = formula_corpus();
    let mut comparisons = Vec::new();
    let mut mitex_raw_pass = 0usize;
    let mut mitex_prelude_pass = 0usize;
    let mut tex2typst_raw_pass = 0usize;
    let mut tex2typst_prelude_pass = 0usize;
    let mut unsupported_latex_detected_by_mitex = false;
    let mut unsupported_latex_rejected_by_typst = false;

    for case in &corpus {
        let mitex_res = mitex::convert_math(case.latex, None);
        let tex2typst_res = tex2typst_rs::tex2typst(case.latex);

        let mitex_raw_compiles = match &mitex_res {
            Ok(typ) => compile_typst_snippet(&format!("$ {typ} $"), &fonts).is_ok(),
            Err(_) => false,
        };
        let mitex_upgraded = mitex_res
            .as_ref()
            .map(|s| upgrade_typst_015_math_symbols(s));
        let mitex_with_prelude_compiles = match &mitex_upgraded {
            Ok(typ) => {
                compile_typst_snippet(&format!("{OFFLINE_MATH_PRELUDE}\n$ {typ} $"), &fonts).is_ok()
            }
            Err(_) => false,
        };
        let tex2typst_raw_compiles = match &tex2typst_res {
            Ok(typ) => compile_typst_snippet(&format!("$ {typ} $"), &fonts).is_ok(),
            Err(_) => false,
        };
        let tex2typst_upgraded = tex2typst_res
            .as_ref()
            .map(|s| upgrade_typst_015_math_symbols(s));
        let tex2typst_with_prelude_compiles = match &tex2typst_upgraded {
            Ok(typ) => {
                compile_typst_snippet(&format!("{OFFLINE_MATH_PRELUDE}\n$ {typ} $"), &fonts).is_ok()
            }
            Err(_) => false,
        };

        if case.expect_valid {
            if mitex_raw_compiles {
                mitex_raw_pass += 1;
            }
            if mitex_with_prelude_compiles {
                mitex_prelude_pass += 1;
            }
            if tex2typst_raw_compiles {
                tex2typst_raw_pass += 1;
            }
            if tex2typst_with_prelude_compiles {
                tex2typst_prelude_pass += 1;
            }
        } else {
            unsupported_latex_detected_by_mitex = mitex_res.is_err();
            unsupported_latex_rejected_by_typst =
                !mitex_with_prelude_compiles && !tex2typst_with_prelude_compiles;
        }

        comparisons.push(ConversionComparison {
            id: case.id.to_string(),
            category: case.category.to_string(),
            latex: case.latex.to_string(),
            mitex_ok: mitex_res.is_ok(),
            mitex_output: mitex_upgraded.unwrap_or_else(|e| format!("ERR: {e}")),
            mitex_raw_compiles,
            mitex_with_prelude_compiles,
            tex2typst_ok: tex2typst_res.is_ok(),
            tex2typst_output: tex2typst_upgraded.unwrap_or_else(|e| format!("ERR: {e}")),
            tex2typst_raw_compiles,
            tex2typst_with_prelude_compiles,
        });
    }

    // 2. Build full A4 Japanese + University Math/Physics PDF with SVG & PNG & injection safety test
    let svg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sine_plot.svg");
    let svg_bytes =
        fs::read(&svg_path).map_err(|e| format!("Failed reading {}: {e}", svg_path.display()))?;
    let png_bytes = generate_sample_png()?;

    let mut vfiles = HashMap::new();
    vfiles.insert("assets/sine_plot.svg".to_string(), svg_bytes);
    vfiles.insert("assets/damped_plot.png".to_string(), png_bytes);

    let untrusted_injection = r#"問題文内インジェクション試験: #read("Cargo.toml") / $1+1$"#;
    let escaped_injection = escape_typst_text(untrusted_injection);

    let converted_sections: String = comparisons
        .iter()
        .filter(|c| c.id != "unsupported_macro")
        .map(|c| {
            // Prefer MiTeX + offline prelude (preserves pmatrix delimiters & cases braces)
            let eq = if c.mitex_with_prelude_compiles {
                &c.mitex_output
            } else if c.tex2typst_with_prelude_compiles {
                &c.tex2typst_output
            } else {
                "\"変換失敗\""
            };
            format!("=== {}\n$ {} $\n", escape_typst_text(&c.category), eq)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let jp_font_family = if japanese_font_loaded
        .as_deref()
        .is_some_and(|s| s.contains("Noto Serif JP"))
    {
        r#"("Noto Serif JP", "BIZ UDMincho", "Libertinus Serif")"#
    } else {
        r#"("BIZ UDMincho", "Noto Sans JP", "Libertinus Serif")"#
    };

    let doc_source = format!(
        r##"{OFFLINE_MATH_PRELUDE}
#set page(paper: "a4", margin: (x: 20mm, y: 20mm), numbering: "1 / 1")
#set text(font: {jp_font_family}, lang: "ja", size: 10.5pt)
#set par(justify: true, leading: 0.75em)

= FormulaDesk Phase 0 — Typst PDF 生成技術検証サンプル

*科目*: 大学数学・大学物理（統合組版 PoC） / *用紙*: A4 / *組版エンジン*: Embedded Typst 0.15.1

#line(length: 100%, stroke: 0.5pt + gray)

== 1. 日本語組版およびインジェクション防御検証

本ドキュメントは、構造化解答 JSON から LaTeX→Typst 変換層および厳格なテキストエスケープを経て生成されたサンプル解答 PDF である。
長い日本語文の自動改行・禁則処理と、数式・証明・ベクター図・ラスター画像の混在レイアウトを検証する。

*エスケープ検証文字列*: {escaped_injection}

== 2. 大学数学・大学物理 複雑数式コーパス（MiTeX + オフライン互換プレリュード変換）

{converted_sections}

== 3. 論理的証明レイアウト検証（Cauchy-Schwarz の不等式）

#block(
  fill: luma(248),
  stroke: (left: 2.5pt + rgb("#2563eb")),
  inset: 10pt,
  radius: 3pt,
  breakable: true,
)[
  *定理（Cauchy-Schwarz の不等式）*: 内積空間 $V$ の任意の元 $u, v in V$ に対して、
  $ |chevron.l u, v chevron.r| <= ||u|| dot ||v|| $
  が成り立つ。等号成立は $u$ と $v$ が線形従属であるときに限る。

  *証明*: $v = 0$ のとき両辺とも $0$ となり自明に成立する。$v != 0$ とし、任意のスカラー $t in CC$ に対して $||u - t v||^2 >= 0$ を考える。
  $t = chevron.l u, v chevron.r / ||v||^2$ と選べば、
  $ 0 <= ||u||^2 - (|chevron.l u, v chevron.r|^2) / (||v||^2) $
  となり主張を得る。 #h(1fr) $square$
]

== 4. 長い数式の折り返し・複数行整列検証

$
  cal(L)(q, dot(q), t) &= T - V \
  &= 1/2 m (dot(r)^2 + r^2 dot(theta)^2 + r^2 sin^2 theta dot(phi)^2) - (- (G M m) / r) \
  &= 1/2 m dot(r)^2 + (L^2) / (2 m r^2) + (G M m) / r
$

== 5. 図・グラフの埋め込み検証（SVG ベクター / PNG ラスター）

#figure(
  image("assets/sine_plot.svg", width: 82%),
  caption: [減衰振動のベクターグラフ（SVG 埋め込み）],
)

#figure(
  image("assets/damped_plot.png", width: 82%),
  caption: [数値計算プロット（PNG 埋め込み）],
)
"##
    );

    let compile_start = Instant::now();
    let world = SandboxedWorld::new(doc_source, vfiles, fonts)?;
    let compiled = typst::compile::<PagedDocument>(&world);
    let document = compiled.output.map_err(|errs| {
        errs.iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let compile_ms = compile_start.elapsed().as_millis();

    let export_start = Instant::now();
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errs| {
        errs.iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let pdf_export_ms = export_start.elapsed().as_millis();

    // Verify sandbox actually blocks raw `#read("Cargo.toml")`
    let raw_injection_world = SandboxedWorld::new(
        r#"#read("Cargo.toml")"#.to_string(),
        HashMap::new(),
        world.fonts.clone(),
    )?;
    let raw_blocked = typst::compile::<PagedDocument>(&raw_injection_world)
        .output
        .is_err();

    let pdf_path = output_dir.join("sample_solution.pdf");
    fs::write(&pdf_path, &pdf_bytes)
        .map_err(|e| format!("Failed writing {}: {e}", pdf_path.display()))?;

    let valid_total = corpus.iter().filter(|c| c.expect_valid).count();
    let report = PdfPocReport {
        typst_version: "0.15.1 (embedded rust crate)".to_string(),
        embedded_font_count: world.fonts.len(),
        japanese_font_loaded,
        compile_ms,
        pdf_export_ms,
        pdf_bytes: pdf_bytes.len(),
        page_count: document.pages().len(),
        svg_embedded: true,
        png_embedded: true,
        injection_escaped_safely: raw_blocked,
        unsupported_latex_detected_by_mitex,
        unsupported_latex_rejected_by_typst,
        formula_corpus_total: valid_total,
        mitex_raw_pass,
        mitex_prelude_pass,
        tex2typst_raw_pass,
        tex2typst_prelude_pass,
        comparisons,
    };

    let report_path = output_dir.join("pdf_poc_report.json");
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(&report_path, json).map_err(|e| format!("Failed writing report: {e}"))?;

    Ok(report)
}

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output");
    match run_poc(&out_dir) {
        Ok(report) => {
            println!("=== FormulaDesk Phase 0: PDF / Typst PoC ===");
            println!("Typst: {}", report.typst_version);
            println!(
                "Fonts loaded: {} (Japanese: {:?})",
                report.embedded_font_count, report.japanese_font_loaded
            );
            println!(
                "Compile: {} ms | PDF export: {} ms | Pages: {} | Size: {} bytes",
                report.compile_ms, report.pdf_export_ms, report.page_count, report.pdf_bytes
            );
            println!(
                "Corpus ({} valid formulas): mitex raw={}/{} prelude={}/{} | tex2typst-rs raw={}/{} prelude={}/{}",
                report.formula_corpus_total,
                report.mitex_raw_pass,
                report.formula_corpus_total,
                report.mitex_prelude_pass,
                report.formula_corpus_total,
                report.tex2typst_raw_pass,
                report.formula_corpus_total,
                report.tex2typst_prelude_pass,
                report.formula_corpus_total
            );
            println!(
                "Unsupported macro detected (mitex={}, typst={}) | Sandbox blocks #read: {}",
                report.unsupported_latex_detected_by_mitex,
                report.unsupported_latex_rejected_by_typst,
                report.injection_escaped_safely
            );
            for c in &report.comparisons {
                println!(
                    "  [{}] mitex(raw={}, prelude={}) | tex2typst(raw={}, prelude={})",
                    c.id,
                    c.mitex_raw_compiles,
                    c.mitex_with_prelude_compiles,
                    c.tex2typst_raw_compiles,
                    c.tex2typst_with_prelude_compiles
                );
            }
            println!(
                "Output PDF: {}",
                out_dir.join("sample_solution.pdf").display()
            );
        }
        Err(err) => {
            eprintln!("PDF PoC failed: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_prevents_typst_code_execution() {
        let escaped = escape_typst_text(r#"#read("Cargo.toml")"#);
        assert!(escaped.starts_with(r"\#read"));
    }

    #[test]
    fn test_pdf_generation_and_sandbox() {
        let out_dir = std::env::temp_dir().join("formuladesk-pdf-typst-poc-test");
        let report = run_poc(&out_dir).expect("PoC should compile and export PDF");
        assert!(report.pdf_bytes > 5_000);
        assert!(report.page_count >= 1);
        assert!(report.injection_escaped_safely);
        assert!(report.unsupported_latex_detected_by_mitex);
        assert!(report.unsupported_latex_rejected_by_typst);
        assert_eq!(
            report.mitex_prelude_pass, report.formula_corpus_total,
            "MiTeX + offline prelude must pass 100% of math/physics corpus"
        );
        assert_eq!(
            report.tex2typst_prelude_pass, report.formula_corpus_total,
            "tex2typst-rs + offline prelude must pass 100% of math/physics corpus"
        );
        let pdf_bytes = fs::read(out_dir.join("sample_solution.pdf")).expect("PDF should exist");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }
}
