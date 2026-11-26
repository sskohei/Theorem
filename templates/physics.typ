// Theorem — University Physics Layout Helpers
#import "common.typ": info_box

#let physics_setup_box(title, body) = {
  info_box(title, accent: rgb("#0369a1"), bg: rgb("#f0f9ff"), body)
}

#let physics_check_box(title, body) = {
  info_box(title, accent: rgb("#b45309"), bg: rgb("#fffbeb"), body)
}
