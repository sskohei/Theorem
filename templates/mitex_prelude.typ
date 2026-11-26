// Theorem — Offline MiTeX 0.2.4 -> Typst 0.15.1 Compatibility Prelude
// Pure offline Typst 0.15.1 (zero external @preview dependencies).
// Single source of truth shared by `templates/common.typ` and `src-tauri/src/pdf/latex.rs`.

// --- Core MiTeX math helpers ---
#let zws = sym.zws

#let mitexsqrt(..args) = {
  let pos = args.pos()
  if pos.len() >= 2 { math.root(pos.at(0), pos.at(1)) } else if pos.len() == 1 { math.sqrt(pos.at(0)) } else { math.sqrt([]) }
}

// --- Math style & font helpers ---
#let mitexdisplay(..args) = math.display(args.pos().join())
#let mitexinline(..args) = math.inline(args.pos().join())
#let mitexscript(..args) = math.script(args.pos().join())
#let mitexsscript(..args) = math.sscript(args.pos().join())

#let mitexmathbf(x) = math.upright(math.bold(x))
#let mitexmathrm(x) = math.upright(x)
#let mitexmathbb(x) = math.bb(x)
#let mitexmathcal(x) = math.cal(x)
#let mitexmathfrak(x) = math.frak(x)
#let mitexmathsf(x) = math.sans(x)
#let mitexmathtt(x) = math.mono(x)
#let mitexmathit(x) = math.italic(x)

#let mitexbold(..args) = math.bold(args.pos().join())
#let mitexcal(..args) = math.cal(args.pos().join())
#let mitexfrak(..args) = math.frak(args.pos().join())
#let mitexitalic(..args) = math.italic(args.pos().join())
#let mitexupright(..args) = math.upright(args.pos().join())
#let mitexsans(..args) = math.sans(args.pos().join())
#let mitexmono(..args) = math.mono(args.pos().join())

// --- Text-in-math helpers (#textmath[...], #textbf[...], etc.) ---
#let textmath(x) = text(style: "normal", x)
#let textbf(x) = strong(x)
#let textit(x) = emph(x)
#let textmd(x) = text(weight: "regular", x)
#let textnormal(x) = text(style: "normal", weight: "regular", x)
#let textrm(x) = text(style: "normal", x)
#let textsf(x) = text(style: "normal", x)
#let texttt(x) = math.mono(x)
#let textup(x) = text(style: "normal", x)
#let hbox(x) = text(style: "normal", x)

// --- Spacing helpers ---
#let negthinspace = h(-1em / 6)
#let negmedspace = h(-2em / 9)
#let negthickspace = h(-5em / 18)
#let negthinmedspace = h(-1em / 6)
#let enspace = h(0.5em)
#let hspace(x) = h(0.5em)
#let vspace(x) = v(0.5em)
#let textwidth = 100%

// --- Math environments (align / aligned / gather / split / matrices / array / cases) ---
#let aligned(..args) = args.pos().join()
#let alignedat(..args) = args.pos().join()
#let matrix(..args) = math.mat(delim: none, ..args.pos())
#let pmatrix(..args) = math.mat(delim: "(", ..args.pos())
#let bmatrix(..args) = math.mat(delim: "[", ..args.pos())
#let Bmatrix(..args) = math.mat(delim: "{", ..args.pos())
#let vmatrix(..args) = math.mat(delim: "|", ..args.pos())
#let Vmatrix(..args) = math.mat(delim: "||", ..args.pos())
#let smallmatrix(..args) = math.script(math.mat(delim: none, ..args.pos()))
#let mitexarray(..args) = math.mat(delim: none, ..args.pos())
#let rcases(..args) = math.cases(reverse: true, ..args.pos())

// --- Delimiter sizing ---
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
#let middle(x) = math.mid(x)

// --- Fractions, binomials, stacking, over/under, operators ---
#let dfrac(a, b) = math.display(math.frac(a, b))
#let tfrac(a, b) = math.inline(math.frac(a, b))
#let cfrac(a, b) = math.display(math.frac(a, b))
#let dbinom(a, b) = math.display(math.binom(a, b))
#let tbinom(a, b) = math.inline(math.binom(a, b))
#let atop(a, b) = math.vec(delim: none, a, b)
#let brace(a, b) = math.vec(delim: "{", a, b)
#let brack(a, b) = math.vec(delim: "[", a, b)
#let substack(x) = math.script(x)
#let overset(a, b) = math.attach(math.limits(b), t: a)
#let underset(a, b) = math.attach(math.limits(b), b: a)
#let stackrel(a, b) = math.attach(math.limits(b), t: a)
#let operatorname(x) = math.op(x)
#let operatornamewithlimits(x) = math.op(x, limits: true)
#let pmod(x) = [#h(0.45em);(#math.op("mod") #x)]
#let pod(x) = [#h(0.45em);(#x)]

#let mitexoverbrace(..args) = math.overbrace(..args)
#let mitexunderbrace(..args) = math.underbrace(..args)
#let mitexoverbracket(..args) = math.overbracket(..args)
#let mitexunderbracket(..args) = math.underbracket(..args)
#let overgroup(x) = math.overparen(x)
#let undergroup(x) = math.underparen(x)
#let overlinesegment(x) = math.overline(x)
#let overleftrightarrow(x) = math.accent(x, sym.arrow.l.r)
#let overleftharpoon(x) = math.accent(x, sym.harpoon.lt)
#let overrightharpoon(x) = math.accent(x, sym.harpoon.rt)
#let underbar(x) = math.underline(x)
#let mathring(x) = math.accent(x, sym.circle.tiny)

// --- Boxes, cancellation, phantoms, math classes ---
#let boxed(x) = box(stroke: 0.5pt, inset: 3pt, math.equation(block: false, x))
#let fbox(x) = box(stroke: 0.5pt, inset: 3pt, x)
#let bcancel(x) = math.cancel(inverted: true, x)
#let xcancel(x) = math.cancel(cross: true, x)
#let sout(x) = strike(x)
#let phantom(x) = hide(x)
#let hphantom(x) = box(height: 0pt, hide(x))
#let vphantom(x) = box(width: 0pt, hide(x))
#let mathclap(x) = box(width: 0pt, align(center, x))
#let raisebox(d, x) = x
#let mitexcolor(..args) = args.pos().last(default: [])
#let colortext(c, x) = x
#let colorbox(c, x) = box(inset: 2pt, x)

#let mathbin(x) = math.class("binary", x)
#let mathrel(x) = math.class("relation", x)
#let mathop(x) = math.class("large", x)
#let mathopen(x) = math.class("opening", x)
#let mathclose(x) = math.class("closing", x)
#let mathpunct(x) = math.class("punctuation", x)
#let mathord(x) = math.class("normal", x)
#let mathinner(x) = math.class("normal", x)

// --- Braket & set notation (quantum physics / set theory) ---
#let bra(x) = math.lr(sym.chevron.l + x + "|")
#let ket(x) = math.lr("|" + x + sym.chevron.r)
#let braket(x) = math.lr(sym.chevron.l + x + sym.chevron.r)
#let Bra(x) = math.lr(sym.chevron.l + x + "|")
#let Ket(x) = math.lr("|" + x + sym.chevron.r)
#let Braket(x) = math.lr(sym.chevron.l + x + sym.chevron.r)
#let mitexset(x) = math.lr("{" + x + "}")
#let Set(x) = math.lr("{" + x + "}")

// --- Extensible arrows ---
#let xrightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.r)), t: x)
#let xleftarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l)), t: x)
#let xleftrightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l.r)), t: x)
#let xRightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.r.double)), t: x)
#let xLeftarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l.double)), t: x)
#let xLeftrightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l.r.double)), t: x)
#let xmapsto(x) = math.attach(math.limits(math.stretch(sym.arrow.r.bar)), t: x)
#let xhookrightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.r.hook)), t: x)
#let xhookleftarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l.hook)), t: x)
#let xleftharpoonup(x) = math.attach(math.limits(math.stretch(sym.harpoon.lt)), t: x)
#let xleftharpoondown(x) = math.attach(math.limits(math.stretch(sym.harpoon.lb)), t: x)
#let xrightharpoonup(x) = math.attach(math.limits(math.stretch(sym.harpoon.rt)), t: x)
#let xrightharpoondown(x) = math.attach(math.limits(math.stretch(sym.harpoon.rb)), t: x)
#let xrightleftharpoons(x) = math.attach(math.limits(math.stretch(sym.harpoons.rtlb)), t: x)
#let xleftrightharpoons(x) = math.attach(math.limits(math.stretch(sym.harpoons.ltrb)), t: x)
#let xlongequal(x) = math.attach(math.limits(math.stretch("=")), t: x)
#let xtofrom(x) = math.attach(math.limits(math.stretch(sym.arrows.rl)), t: x)
#let xtwoheadleftarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.l.twohead)), t: x)
#let xtwoheadrightarrow(x) = math.attach(math.limits(math.stretch(sym.arrow.r.twohead)), t: x)

// --- Labels / references / booktabs safe fallbacks ---
#let mitexlabel(x) = []
#let mitexref(x) = [(#x)]
#let mitexcite(x) = [[#x]]
#let mitexcaption(x) = x
#let toprule(..args) = []
#let midrule(..args) = []
#let bottomrule(..args) = []

// --- Zero-arg standard math operators & symbols missing from Typst core ---
#let argmax = math.op("arg max", limits: true)
#let argmin = math.op("arg min", limits: true)
#let injlim = math.op("inj lim", limits: true)
#let projlim = math.op("proj lim", limits: true)
#let plim = math.op("plim", limits: true)
#let sh = math.op("sh")
#let ch = math.op("ch")
#let th = math.op("th")
#let cth = math.op("cth")
#let cosec = math.op("cosec")
#let arctg = math.op("arctg")
#let arcctg = math.op("arcctg")
#let cotg = math.op("cotg")
#let smallint = math.inline(sym.integral)
#let thickapprox = sym.approx
#let thicksim = sym.tilde.op
#let TeX = "TeX"
#let LaTeX = "LaTeX"
#let KaTeX = "KaTeX"

// --- Slanted uppercase Greek variants (\varGamma .. \varOmega) ---
#let varGamma = math.italic(sym.Gamma)
#let varDelta = math.italic(sym.Delta)
#let varTheta = math.italic(sym.Theta)
#let varLambda = math.italic(sym.Lambda)
#let varXi = math.italic(sym.Xi)
#let varPi = math.italic(sym.Pi)
#let varSigma = math.italic(sym.Sigma)
#let varUpsilon = math.italic(sym.Upsilon)
#let varPhi = math.italic(sym.Phi)
#let varPsi = math.italic(sym.Psi)
#let varOmega = math.italic(sym.Omega)
