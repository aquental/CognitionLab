C2-19: CLOSED — `evaluate` rejects repeated chunk IDs within the scored top three; regression tests cover duplicate rejection and metric bounds.

C2-20: CLOSED — both local and OpenAI adapters perform batch tokenization and budget checks inside `spawn_blocking`; the local regression test verifies runtime responsiveness.

C2-21: CLOSED — `LocalTextAdapter<B, T>` uses a concrete generic tokenizer; dynamic dispatch remains at the prescribed `EmbeddingStrategy::token_counter` boundary.

NO FINDINGS