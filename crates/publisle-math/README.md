# publisle-math

A bounded, non-executing TeX math parser and MathML generator. No fonts, PDF
libraries, network access, or TeX installation are required.

```rust
let expression = publisle_math::parse(r"\frac{x}{2}")?;
assert!(expression.mathml().contains("<mfrac>"));
# Ok::<(), publisle_math::Diagnostic>(())
```

Expressions are limited to 64 KiB, 64 nesting levels, and 8192 syntax nodes.
Malformed groups and execution commands are rejected. Unsupported commands are
preserved visibly in red and reported by `parse_with_diagnostics`.
