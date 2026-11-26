// Theorem — University Mathematics Layout Helpers
#import "common.typ": info_box

#let math_definition_box(name, body) = {
  info_box("定義: " + name, accent: rgb("#0f766e"), bg: rgb("#f0fdfa"), body)
}

#let math_theorem_box(name, body) = {
  info_box("定理・補題: " + name, accent: rgb("#4338ca"), bg: rgb("#eef2ff"), body)
}

#let math_proof_block(body) = {
  block(
    width: 100%,
    inset: (left: 10pt, right: 6pt, top: 6pt, bottom: 6pt),
    stroke: (left: 1.5pt + rgb("#64748b")),
    breakable: true,
    [
      #text(weight: "bold", fill: rgb("#334155"))[証明]
      #v(3pt)
      #body
      #h(1fr)
      #text(weight: "bold")[$qed$]
    ],
  )
}
