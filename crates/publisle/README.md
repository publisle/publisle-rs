# publisle

One library dependency for Publisle. The default `pdf` feature exposes the native
`compile` function and resolved input `model`. Enable `math` for the independent,
bounded TeX parser and MathML generator. Fonts must be supplied by the host.

```rust
# #[cfg(feature = "math")] {
let mathml = publisle::math::parse("x+1")?.mathml();
assert!(mathml.contains("<math") || mathml.contains("<mrow>"));
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```
