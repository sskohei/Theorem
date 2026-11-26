// Theorem — Common Typst Template & Offline MiTeX Compatibility Prelude
// Pure offline Typst 0.15.1 (zero external @preview dependencies).

#let mitexsqrt(..args) = {
  let pos = args.pos()
  if pos.len() == 2 { math.root(pos.at(0), pos.at(1)) } else { math.sqrt(pos.at(0)) }
}
#let mitexmathbf(x) = math.upright(math.bold(x))
#let mitexmathrm(x) = math.upright(x)
#let mitexmathbb(x) = math.bb(x)
#let mitexmathcal(x) = math.cal(x)
#let mitexmathfrak(x) = math.frak(x)
#let mitexmathsf(x) = math.sans(x)
#let mitexmathtt(x) = math.mono(x)
#let mitexmathit(x) = math.italic(x)
#let big(x) = math.lr(x, size: 120%)
#let Big(x) = math.lr(x, size: 180%)
#let bigg(x) = math.lr(x, size: 240%)
#let Bigg(x) = math.lr(x, size: 300%)
#let bigl(x) = math.class("opening", math.lr(x, size: 120%))
#let bigr(x) = math.class("closing", math.lr(x, size: 120%))
#let bigm(x) = math.class("relation", math.lr(x, size: 120%))
#let Bigl(x) = math.class("opening", math.lr(x, size: 180%))
#let Bigr(x) = math.class("closing", math.lr(x, size: 180%))
#let Bigm(x) = math.class("relation", math.lr(x, size: 180%))
#let biggl(x) = math.class("opening", math.lr(x, size: 240%))
#let biggr(x) = math.class("closing", math.lr(x, size: 240%))
#let biggm(x) = math.class("relation", math.lr(x, size: 240%))
#let Biggl(x) = math.class("opening", math.lr(x, size: 300%))
#let Biggr(x) = math.class("closing", math.lr(x, size: 300%))
#let Biggm(x) = math.class("relation", math.lr(x, size: 300%))
#let pmatrix(..args) = math.mat(delim: "(", ..args)
#let bmatrix(..args) = math.mat(delim: "[", ..args)
#let Bmatrix(..args) = math.mat(delim: "{", ..args)
#let vmatrix(..args) = math.mat(delim: "|", ..args)
#let Vmatrix(..args) = math.mat(delim: "||", ..args)
#let zws = sym.zws

#let theorem_setup(
  paper: "a4",
  body_size: 10.5pt,
  doc_title: "",
  header_label: "Theorem",
  body,
) = {
  set page(
    paper: paper,
    margin: (top: 22mm, bottom: 22mm, left: 20mm, right: 20mm),
    header: context {
      if counter(page).get().first() > 1 {
        set text(size: 8.5pt, fill: luma(110))
        grid(
          columns: (1fr, auto),
          align(left)[#doc_title],
          align(right)[#header_label],
        )
        v(-4pt)
        line(length: 100%, stroke: 0.4pt + luma(210))
      }
    },
    footer: context {
      set text(size: 8.5pt, fill: luma(110))
      align(center)[#counter(page).display("1 / 1", both: true)]
    },
  )
  set text(
    font: ("Noto Serif JP", "BIZ UDMincho", "Yu Mincho", "Libertinus Serif"),
    size: body_size,
    lang: "ja",
    fill: rgb("#1e293b"),
  )
  set par(leading: 0.72em, justify: false)
  set heading(numbering: none)
  show heading.where(level: 1): it => {
    set text(size: 15pt, weight: "bold", fill: rgb("#0f172a"))
    block(above: 0pt, below: 8pt, sticky: true, it)
  }
  show heading.where(level: 2): it => {
    set text(size: 11.5pt, weight: "bold", fill: rgb("#1e3a8a"))
    block(above: 12pt, below: 6pt, sticky: true, [
      #it
      #v(-5pt)
      #line(length: 100%, stroke: 0.5pt + rgb("#cbd5e1"))
    ])
  }
  body
}

#let info_box(title, accent: rgb("#2563eb"), bg: rgb("#f8fafc"), body) = {
  block(
    width: 100%,
    inset: (x: 10pt, y: 8pt),
    radius: 4pt,
    fill: bg,
    stroke: (left: 2.5pt + accent, rest: 0.5pt + rgb("#e2e8f0")),
    breakable: true,
    [
      #text(weight: "bold", size: 9.5pt, fill: accent)[#title]
      #v(3pt)
      #body
    ],
  )
}

#let answer_box(body) = {
  block(
    width: 100%,
    inset: (x: 12pt, y: 10pt),
    radius: 5pt,
    fill: rgb("#eff6ff"),
    stroke: 1pt + rgb("#2563eb"),
    breakable: false,
    [
      #text(weight: "bold", size: 10pt, fill: rgb("#1d4ed8"))[最終解答]
      #v(4pt)
      #body
    ],
  )
}
