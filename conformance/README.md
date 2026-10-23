# Pinned contracts

JSON Schemas are published by `@publisle/contracts`; they describe the `@publisle/schema` document model. Fixtures are copied byte for byte from the same source commit. `lock.json` records both package versions, the source commit and a SHA-256 for every schema and fixture. `node tools/check-conformance.mjs` checks the complete file set and checksums offline. CI also compares each locked file with its immutable upstream source commit. Updating the pin is an explicit reviewed change.

These contracts describe semantic documents. The native compiler consumes the separate projected article input; copying the contracts does not claim that the Rust engine implements every semantic contract. The gap report records the remaining work.
