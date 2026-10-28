The diff was cut off before the canonical-JSON change. I am reading the omitted sections before judging C0-4 and looking for new defects.C0-4 CORRECT. `canonical_json` NFC-normalizes object keys, sorts those normalized keys, and returns `IdError::DuplicateKey` when any two collide, so either insertion order or a nested object yields one encoding error instead of duplicate keys.

NO FINDINGS
