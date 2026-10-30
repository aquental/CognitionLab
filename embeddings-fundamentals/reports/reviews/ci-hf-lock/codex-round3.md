CF-3: CLOSED — `src/embedding/hf.rs:101–102` opens the cache directory read-only; lines `117–118` lock it without depending on `.locks` permissions.

NO FINDINGS.