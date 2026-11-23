**The core failure is that the local code is not in the environment.** The `mini-rust-swe-agent/mini-swe-agent/` directory does not exist here. Any "in-depth study" done without the exact tree you have is an approximation based on the public repository. If your copy has patches, different configs, or diverging commits, STUDY.md is out of date relative to your reality.

[Certain] There is no Codex agent/bot connected in this context. I cannot "hand" the file to another model automatically. The adversarial audit below is done by me, in the same process. If you want an external Codex to do it, the way is to copy STUDY.md and paste it into the prompt.

Create `/spec/STUDY.md` with:
- An explicit separation of the modules
- Mermaid diagrams (main flow, action cycle, exception hierarchy, template composition)
- A distinction between what is essential and what is accidental
- Direct implications for the Rust migration

### Adversarial audit (what STUDY.md may be getting wrong or leaving out)

1. **Action extraction is the real blind spot.** The diagram and the text treat `message["extra"]["actions"]` as given. What populates it varies by model (`litellm_model`, textbased, response, etc.). Without reading the concrete parser of at least one model class, the study overestimates how simple the loop is and underestimates how fragile the parsing is.

2. **Cwd and filesystem state are not as clean as the text suggests.** `LocalEnvironment` passes `cwd` per call, but keeps no state between steps. A `cd` or `export` inside a command does **not** affect the next one. This is a classic source of divergence between what the model expects and what the harness does. STUDY.md mentions it, but does not quantify the risk.

3. **`Submitted` depends on a magic string.** Detection is literal (`COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` on the first line + returncode 0). Any change to the prompt, formatting, or model that alters this string breaks termination. This is not highlighted as a parity risk.

4. **Jinja2 templates with `StrictUndefined`.** Missing variables blow up. In the migration, if the merge of template vars is incomplete, behavior changes silently or fails in a different way.

5. **GlobalModelStats is a global side effect.** Global limits through env vars + a lock. In Rust this becomes explicit shared state or disappears. The study does not force the decision.

6. **STUDY.md does not yet check the code against the earlier PLAN.md.** There is no checklist of "this must be identical for the trajectory comparison to be valid".
