# publisle-cli

Install with `cargo install publisle-cli`. The executable is **publisle-rs** so
it can coexist with the npm `publisle` Markdown CLI.

```sh
publisle-rs pdf input.json paper.pdf --fonts /absolute/path/fonts.json
publisle-rs math '\hat{x}+1' --mathml
publisle-rs fonts check --fonts /absolute/path/fonts.json
```

`pdf` consumes the resolved native JSON contract, not Markdown. It creates a new
output file and refuses to overwrite existing data. Both PDF and font commands
require `--fonts` or `PUBLISLE_NATIVE_FONTS`. Paths inside the manifest refer to
host files. The manifest is a JSON array:

```json
[
  { "path": "/absolute/path/fonts/NotoSerif-Regular.ttf", "role": "regular" },
  { "path": "/absolute/path/fonts/NotoSerif-Italic.ttf", "role": "italic" }
]
```

Use the matching `@publisle/pdf-native-*` npm package's `fonts/` directory and
`fonts.json` manifest as a starting point; resolve its paths for your machine.
Fonts are host-supplied and are not bundled in the crates.io crate. The compiler
and math parser remain experimental; inspect output before publication.
