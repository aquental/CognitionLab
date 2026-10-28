C0-1 CLOSED — Scaled `f64` normalization handles finite nonzero extremes, with regression coverage.
C0-2 CLOSED — Text and image calls enforce `batch` and `max_batch`, returning permanent errors above the limit.
C0-3 CLOSED — Constructors, parsing, and serde enforce the structured chunk format, including canonical decimals and nonzero pages.

ID: C0-4
Severity: medium
File:line: src/ids.rs:67
Description: Canonicalization normalizes object keys before sorting but does not reject keys that become identical after NFC normalization. The stable sort preserves insertion order between these equal keys, violating PLAN §2.1’s canonical encoding contract.
Impact: `Fingerprint::of` can produce different fingerprints for identical mappings inserted in different orders. The emitted JSON also contains duplicate keys, making subsequent interpretation ambiguous.
Reproduction scenario or verifiable rationale: With the enabled `preserve_order` feature, construct two maps containing `"é": 1` and `"e\u{301}": 2`, inserted in opposite orders. Both keys normalize to `"é"`. The outputs are respectively `{"é":1,"é":2}` and `{"é":2,"é":1}`, producing different hashes. The existing property test generates only ASCII keys.
Validation suggestion: Reject duplicate normalized keys with a typed error before encoding. Add tests for both insertion orders, including collisions inside nested objects.

🌱 graft saved ~22,426 tokens this turn, 1 call.