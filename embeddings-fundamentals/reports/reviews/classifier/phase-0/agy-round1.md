I am reading the required skills, `.review/PACKAGE.md`, `.review/phase.diff`, and the specification files (`embeddings-fundamentals/spec/CLASSIFIER.md`, `embeddings-fundamentals/spec/PLAN.md`, and `embeddings-fundamentals/CLAUDE.md`) to prepare for the Phase 0 adversarial review.
I will now read the remainder of `.review/phase.diff`, `embeddings-fundamentals/spec/CLASSIFIER.md`, and inspect the modified and added files in the workspace.
I will now read the actual files in the worktree (`Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/classifier/mod.rs`, `src/classifier/categorias.rs`, `src/classifier/tipos.rs`, `tests/classifier_vectors.rs`, `tests/fixtures/classifier/README.md`, `tests/fixtures/classifier/vetores.json`, `reports/progress/PROGRESS.md`, and `reports/reviews/classifier/classifier-findings.md`) directly to check every detail against the spec.
I will now read the remaining changed files (`reports/progress/PROGRESS.md`, `reports/reviews/classifier/classifier-findings.md`, `Cargo.lock` around lines 1570 and 6090) and check `vetores.json` around `criterio` and `constantes`.
I will read the end of `vetores.json` as well to check all top-level keys and the rest of `criterio`.
I will check `Classificacao` in `src/classifier/tipos.rs` vs `spec/CLASSIFIER.md` §5.5 and §20 Phase 0 item 5.
Let's check `src/classifier/tipos.rs` and `spec/CLASSIFIER.md` §5.5!
Wait: in `spec/CLASSIFIER.md` §5.5:
```rust
pub struct RespostaEscolha { pub escolha: Categoria, pub probabilidades: [f64; 9] } // índice = posição em Categoria::TODAS
pub enum EstadoClassificacao { Preparado, Ok, Incerto, SemConteudo, BloqueadoPrivacidade, Erro }
pub enum ErroCategoria { Timeout, Rede, RespostaInvalida, Autenticacao, QuotaProvedor, Outro }
pub enum MotivoCodigo { SemConteudo, TextoInsuficiente, ConferenciaPrivacidade, ProvedorErro, EnvioNaoAutorizado }
// Motivos que só o CLI produz, antes de existir uma Classificacao (§10.3), com os nomes de TP
// `cli/classificar_documento.py` (sem os da conversão pelo LibreOffice, que não existe aqui):
pub enum MotivoCli { ConfiguracaoInvalida, SemChave, ModeloInvalido, ArquivoInexistente, FormatoNaoSuportado,
                     LimitePdf, ArquivoIlegivel, OcrIndisponivel, ErroInterno }
pub struct Uso { pub input_tokens: u64, pub output_tokens: u64 }

pub struct Classificacao {
    pub estado: EstadoClassificacao,
    pub paginas_modo: PaginasModo,
    pub total_paginas: u32, pub paginas_enviadas: u32, pub caracteres_enviados: u32,
    pub truncado: bool,
    pub mascaramentos: BTreeMap<String, u32>,     // camadas + "conferencia_<camada>" no bloqueio (TD §9.1)
    pub origens_texto: Vec<OrigemTexto>,
    pub entrada_digest: String,                   // hex de 64
    pub tipo: Option<Categoria>,
    pub confianca: Option<f64>,                   // probabilidade DA ESCOLHA (TD §10)
    pub margem: Option<f64>,
    pub probabilidades: Option<[f64; 9]>,          // None sem resposta válida; serializado como {} (TD §11.2)
    pub confianca_provedor: Option<f64>,           // EXTENSÃO (DV7): `confidence` do Jev, só informativa
    pub erro_categoria: Option<ErroCategoria>,
    pub motivo_codigo: Option<MotivoCodigo>,
    pub provider: &'static str,                    // "typesafe"
    pub model: String,                             // o pedido; com chamada, o ecoado pela resposta (TD §9.1)
    pub adapter_version: &'static str,             // §8.1
    pub perguntas_versao: &'static str, pub mascaramento_versao: &'static str,
    pub limiar_aplicado: f64,
    pub provider_request_id: Option<String>,
    pub latencia_ms: u64,
    pub uso: Option<Uso>,
}
```
Wait: is `Classificacao` in `src/classifier/tipos.rs`?!
Let's check `src/classifier/tipos.rs`!
In `src/classifier/tipos.rs`, lines 238-333:
- `RespostaEscolha` is there
- `EstadoClassificacao` is there
- `ErroCategoria` is there
- `MotivoCodigo` is there
- `MotivoCli` is there
- `Uso` is there
- Where is `Classificacao`?! `Classificacao` is NOT in `src/classifier/tipos.rs`!
Wait—let's check why or if `Classificacao` is part of §5.5!
Look at `spec/CLASSIFIER.md` §5:
- §5.1 Categorias
- §5.2 Entrada
- §5.3 Estado preparado
- §5.4 Estado enviado: duas serializações
- §5.5 Resposta validada e resultado -> defines `RespostaEscolha`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, `MotivoCli`, `Uso`, AND `Classificacao`!
- §5.6 Provedor (`RequestNeutro`, `RespostaProvedor`, `RlcdProvider` — which Phase 4 says: "`provedor.rs` (trait, request neutro, `avaliar` com timeout e semáforo)").
And look at `spec/CLASSIFIER.md` §20 Phase 0 item 5:
"5. criar o esqueleto de `src/classifier/` com os tipos do §5;"
And in `spec/CLASSIFIER.md` §4.1 (which was modified IN THIS DIFF!):
```diff
-├── mod.rs            # API pública: tipos (§5), montar_estado, classificar
-├── categorias.rs     # Categoria (9), pergunta, PERGUNTAS_VERSAO, listas (VOCABULARIO_TIPO etc.)
+├── mod.rs            # API pública: reexporta os tipos, montar_estado, classificar
+├── categorias.rs     # Categoria (9), pergunta, PERGUNTAS_VERSAO e as constantes de TV `constantes`
+├── tipos.rs          # tipos do §5: entrada, estado preparado, resposta, estados e motivos
```
And in `PROGRESS.md` line 75:
"`tipos.rs` holds the types of §5.2–§5.5."
Even `PROGRESS.md` claims "`tipos.rs` holds the types of §5.2–§5.5", and the module doc of `src/classifier/tipos.rs` line 1 says:
`//! Input, prepared state and result types (spec/CLASSIFIER.md §5).`
Yet `Classificacao` (from §5.5, the primary result type of the classifier!) is missing from `tipos.rs` (and not re-exported in `mod.rs`), along with its custom serialization requirement in §5.5 ("A serialização das `probabilidades` segue a ordem de `Categoria::TODAS`, e o valor `None` vira `{}`")!

Let's also check if there are any other findings in the phase diff!
Let's check every item of Phase 0 in `spec/CLASSIFIER.md` §20 and every section mentioned in `.review/PROMPT.md` (§0, §2, §3, §4, §5, §14.1, §20 Phase 0).

1. **Phase 0 Entry & Item 1 & Item 2 (`.review/PACKAGE.md` and `tests/fixtures/classifier/README.md`, `spec/CLASSIFIER.md` §0.1, §3.4, §20 Phase 0):**
Let's check `.review/PACKAGE.md` line 80:
"- **Input:** the phase entry needs TestoDoc's rename commit pushed, with a fixed message. That is the owner's action and is still pending. The vectors' bytes do not depend on it: the file is pinned by sha256, and the README names the branch and the parent commit."
Wait: look at `spec/CLASSIFIER.md` §20 Phase 0:
- **Entradas:** o commit da troca de nomes no TestoDoc (§3.4), com a mensagem sem as palavras originais e enviado ao remoto.
- **Trabalho:**
  2. copiar TV para `tests/fixtures/classifier/vetores.json`, com o sha256 e o commit de origem em `tests/fixtures/classifier/README.md`, e rodar a checagem de palavras dos exames (só contagens) antes do commit;
Look at `tests/fixtures/classifier/README.md` line 8:
`| Branch and commit | assessment, the commit after 06f559f that renames two synthetic names (spec/CLASSIFIER.md §3.4) |`
And `spec/CLASSIFIER.md` §0.1 line 16:
`repositório TestoDoc/testo-doc, branch assessment, commit seguinte a 06f559f com a troca de nomes sintéticos de 31/10/2026 (F8; hoje 6cd3dd0, ainda não enviado ao remoto)`
Wait: `tests/fixtures/classifier/README.md` does not record the actual source commit hash (`6cd3dd0` or the rewritten commit hash after amending the commit message and pushing to remote), only the prose description `"the commit after 06f559f that renames two synthetic names"`, and the Phase 0 entry prerequisite ("com a mensagem sem as palavras originais e enviado ao remoto") is still unfulfilled as noted in `PACKAGE.md`.

2. **Check `constants_match` and `meta_matches_the_port_versions` in `tests/classifier_vectors.rs`:**
Let's check if all keys in `meta`, `constantes`, and `criterio` in `vetores.json` are checked, or if extra/unknown keys could be ignored or if any key is unchecked:
In `vetores.json`:
- `meta`:
  - `descricao`
  - `mascaramento_versao`: `"mascaramento-5"`
  - `perguntas_versao`: `"classificador-documento-1"`
  - `unicode_versao`: `"16.0.0"`
  - `python`: `"3.14"`
  - `gerador`: `"backend/app/cli/vetores_classificador.py"`
- `constantes`:
  - `LIMITE_PAGINA`: 6000
  - `LIMITE_TOTAL`: 60000
  - `TEXTO_UTIL_MIN`: 40
  - `TOLERANCIA_LIMIAR`: 1e-09
  - `TOLERANCIA_SOMA_PROBABILIDADES`: 0.001
  - `REMOVIDO`: `"[REMOVIDO]"`
  - `DATA`: `"[DATA]"`
  - `ETAPA`: `"classificacao_documento"`
  - `CRITERIO_ID`: `"tipo_documento"`
  Wait: in `tests/classifier_vectors.rs`:
  Does `constants_match` check that `c.as_object().unwrap().len() == 9`? No, if `constantes` gained a 10th constant in TV, `constants_match` wouldn't notice unless the sha256 pin caught it (though the sha256 pin checks the exact file).

3. **Check `tests/classifier_vectors.rs` item 6 of Phase 0:**
"6. criar o teste de conformidade com uma lista de seções pendentes, que só pode encolher. Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio."
Let's check how `tests/classifier_vectors.rs` implements this!
Look at lines 18-39 and 60-104 of `tests/classifier_vectors.rs`:
```rust
const COVERED: &[&str] = &["meta", "constantes", "criterio"];
const PENDING: &[&str] = &[ ... 14 sections ... ];
const EXCLUDED: &[&str] = &["referencia_pseudonima", "bloco_tipo_detectado"];
```
And `section(vectors, name)` checks `assert!(COVERED.contains(&name), ...)`.
Wait: what if someone adds a section to `COVERED` (removing it from `PENDING`), without actually calling `section(&v, name)` in any test?
Wait: `COVERED` is a static slice `&["meta", "constantes", "criterio"]`.
Each test (`meta_matches_the_port_versions`, `constants_match`, `criterion_matches_question_and_options_in_order`) runs in a separate `#[test]` function and calls `section(&v, "meta")`, `section(&v, "constantes")`, `section(&v, "criterio")`.
Wait: `every_section_is_covered_pending_or_excluded_exactly_once` checks that `COVERED ∪ PENDING ∪ EXCLUDED == sections in vetores.json` and that they are pairwise disjoint.
And `section(&v, name)` checks that `name` is in `COVERED`.
Wait: what if a section is moved from `PENDING` to `COVERED` in `tests/classifier_vectors.rs`, and NO test calls `section(&v, that_section)` (or the test for it is deleted/forgotten)?
Then `every_section_is_covered_pending_or_excluded_exactly_once` STILL PASSES! And the section is silently skipped (never read or asserted by any test), violating "nenhuma seção fica pulada em silêncio" (no section is silently skipped)!
Let's verify this!
Suppose a developer in Phase 1 moves `"dobrar"` from `PENDING` to `COVERED`:
`const COVERED: &[&str] = &["meta", "constantes", "criterio", "dobrar"];`
and forgets to write `#[test] fn dobrar_matches() { ... section(&v, "dobrar") ... }` (or writes a test that doesn't touch `"dobrar"`).
What happens when `cargo test --test classifier_vectors` runs?
- `every_section_is_covered_pending_or_excluded_exactly_once` checks that `COVERED.chain(PENDING).chain(EXCLUDED)` equals the keys of `vetores.json`. Since `"dobrar"` moved from `PENDING` to `COVERED`, the union is unchanged and disjointness holds! The test PASSES!
- `section()` is only called for `"meta"`, `"constantes"`, `"criterio"`. Nothing checks that every entry in `COVERED` was actually accessed by a test!
- Conversely, what if a test reads `v["dobrar"]` directly instead of calling `section(&v, "dobrar")` while `"dobrar"` is still in `PENDING`? Because `vectors()` returns a raw `serde_json::Value`, any test can index `v["dobrar"]` directly without going through `section()`.
Wait: even when using `section()`, there is NO check that `COVERED` sections are actually exercised! If all assertions run in a single test (or if accessed sections are tracked and compared to `COVERED`), moving a section from `PENDING` to `COVERED` without testing it would immediately fail. Right now, moving all 14 sections from `PENDING` to `COVERED` with zero new tests passes `cargo test` completely!
That is a great catch and directly addresses:
- "tests that would pass on wrong code, and missing tests"
- "criar o teste de conformidade com uma lista de seções pendentes, que só pode encolher. Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio."

4. **Let's check `src/classifier/tipos.rs` and `src/classifier/categorias.rs` for any other contract / serde / type issues:**
Let's check every type in `src/classifier/categorias.rs` and `src/classifier/tipos.rs` against `spec/CLASSIFIER.md` §5:
- `Categoria` (`categorias.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]`
  - `#[serde(rename_all = "snake_case")]`
  - Variants: `Vop, EcoConvencional, EcoAvancado, Calorimetria, Ergoespirometria, Holter, Mapa, Ecg, Outro`
  - `TODAS`: `[Self; 9]` in the exact order.
  - `valor`, `rotulo`, `descricao`, `slug`, `indice`.
  - Wait: in `categorias.rs` test `valor_round_trips_through_from_str_and_serde` (line 202):
    It tests `serde_json::to_string(&c)` (Serialize) and `c.valor().parse::<Categoria>()` (`FromStr`), Display? Wait: `Display` and `Deserialize` (`serde_json::from_str::<Categoria>`) are not asserted in `valor_round_trips_through_from_str_and_serde` (though `#[serde(rename_all = "snake_case")]` derives both).
- `OrigemTexto` (`tipos.rs`):
  - `Nativo, OcrLocal, Visao` with `#[serde(rename_all = "snake_case")]`.
  - Wait: look at `spec/CLASSIFIER.md` §5.3:
    `pub origens_texto: Vec<OrigemTexto>, // ordenada pelo valor string, sem repetição`
    And `OrigemTexto` derives `PartialOrd, Ord` in enum declaration order (`Nativo`, `OcrLocal`, `Visao`), which happens to match alphabetical order `"nativo" < "ocr_local" < "visao"`.
- `Pagina` (`tipos.rs`):
  - `pub n: u32, pub texto: String, #[serde(default = "origem_padrao")] pub origem: OrigemTexto`
- `EntradaClassificacao` (`tipos.rs`):
  - `pub nome_arquivo: String, pub titulo: String, pub extensao: String, pub total_paginas: u32, pub paginas: Vec<Pagina>, #[serde(default)] pub nomes_conhecidos: Vec<String>`
  - Let's check `vetores.json` `montar_estado` inputs to see how `EntradaClassificacao` and `Pagina` appear in `vetores.json`!
Let's read lines 3100-3400 of `vetores.json` to see `montar_estado` and `decidir_classificacao`.
Let's check lines 2900-3100 of `vetores.json` as well to see the beginning of `montar_estado`.
Let's also check `vetores.json` lines 2500-2900 to see `origens_texto` in `montar_estado` when multiple `OrigemTexto` values are present!
Wait: look closely at `Camada` in `src/classifier/tipos.rs` (lines 132-160) and `EstadoPreparado` (lines 200-222):
```rust
/// A masking layer, as counted in `mascaramentos` and in a privacy block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Camada {
    /// Values of personal labels.
    Rotulo,
    /// Known names and identifiers.
    NomeConhecido,
    /// The name after Dr., Dra. and the like.
    Tratamento,
    /// Registry numbers, documents, contacts and long digit runs.
    Padrao,
    /// Dates.
    Data,
}
```
And in `EstadoPreparado`:
```rust
    /// Removals per layer, only the layers above zero.
    pub mascaramentos: BTreeMap<Camada, u32>,
    /// Text origins of the sent pages, sorted, without repeats.
    pub origens_texto: Vec<OrigemTexto>,
    /// What the final check found; empty means the state may be sent.
    pub bloqueio: BTreeMap<Camada, u32>,
```
Hold on! Look at `Camada`'s `Ord` derivation:
In `Camada`, the enum variants are declared in layer order:
1. `Rotulo` (`"rotulo"`)
2. `NomeConhecido` (`"nome_conhecido"`)
3. `Tratamento` (`"tratamento"`)
4. `Padrao` (`"padrao"`)
5. `Data` (`"data"`)
In `vetores.json` line 2947:
```json
    "mascaramentos": {
     "rotulo": 1,
     "data": 1
    },
```
In `BTreeMap<Camada, u32>`, `BTreeMap` orders by `Camada`'s `Ord`, which is `Rotulo < NomeConhecido < Tratamento < Padrao < Data` (the execution order of the 5 layers in Python, where `mascaramentos` is an insertion-ordered `dict` populated in layer order 1..5!). That's why `Camada`'s variant order matches Python's insertion order (`rotulo`, `nome_conhecido`, `tratamento`, `padrao`, `data`).

Now let's review all findings we found and check if there are any others!
Let's review the findings carefully:

1. **Finding `A0-1` (medium): `Classificacao` (§5.5) is missing from `src/classifier/tipos.rs` and `src/classifier/mod.rs`.**
   - **File and line:** `embeddings-fundamentals/src/classifier/tipos.rs:238–333`, `embeddings-fundamentals/src/classifier/mod.rs:16–20`, `embeddings-fundamentals/reports/progress/PROGRESS.md:75`
   - **Description and impact:**
     Phase 0 item 5 (`spec/CLASSIFIER.md` §20) requires creating the skeleton of `src/classifier/` with the types of §5 ("criar o esqueleto de `src/classifier/` com os tipos do §5"), and `PROGRESS.md` (line 75) explicitly states that "`tipos.rs` holds the types of §5.2–§5.5". However, `src/classifier/tipos.rs` omits `Classificacao`, the primary classification result struct specified in `spec/CLASSIFIER.md` §5.5 (lines 285–308), along with its re-export in `src/classifier/mod.rs` and its probability serialization rule (§5.5: "`probabilidades` segue a ordem de `Categoria::TODAS`, e o valor `None` vira `{}`"). Note that `spec/CLASSIFIER.md` §4.1 (updated in this diff at line 137) also describes `tipos.rs` as holding `tipos do §5: entrada, estado preparado, resposta, estados e motivos`. Either `Classificacao` must be defined in `tipos.rs` (and re-exported in `mod.rs`) as specified by §5.5 and §20 Phase 0 item 5, or if deferred to Phase 4, §20 Phase 0 item 5 / §4.1 / `PROGRESS.md` should not claim that all §5.2–§5.5 types are implemented in Phase 0.
   - **Reproduction or verifiable rationale:**
     Inspect `embeddings-fundamentals/src/classifier/tipos.rs` and `embeddings-fundamentals/src/classifier/mod.rs` against `embeddings-fundamentals/spec/CLASSIFIER.md` §5.5 (lines 285–312): `RespostaEscolha`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, `MotivoCli`, and `Uso` are present, whereas `pub struct Classificacao` is absent.
   - **Suggested validation:**
     Add `Classificacao` to `src/classifier/tipos.rs` (and re-export it in `src/classifier/mod.rs`) with a unit test verifying its `probabilidades` serialization (`Some([f64; 9])` as an ordered map matching `Categoria::TODAS` and `None` as `{}`), or explicitly scope `Classificacao` to Phase 4 in `spec/CLASSIFIER.md` §20 and `PROGRESS.md`.

2. **Finding `A0-2` (medium): `tests/classifier_vectors.rs` does not verify that sections listed in `COVERED` are actually tested, allowing sections moved from `PENDING` to `COVERED` to be silently skipped.**
   - **File and line:** `embeddings-fundamentals/tests/classifier_vectors.rs:19–38, 64–104`
   - **Description and impact:**
     Phase 0 item 6 (`spec/CLASSIFIER.md` §20) requires: *"criar o teste de conformidade com uma lista de seções pendentes, que só pode encolher. Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio."*
     In `tests/classifier_vectors.rs`, `section(&v, name)` enforces one direction (`name` must be in `COVERED` when a test calls `section()`), and `every_section_is_covered_pending_or_excluded_exactly_once` checks that `COVERED`, `PENDING`, and `EXCLUDED` partition the top-level keys of `vetores.json`. However, nothing verifies the converse: that every section name in `COVERED` is actually exercised by a test (or that `PENDING` only shrinks across phases). If a section is moved from `PENDING` to `COVERED` without adding a test calling `section(&v, name)` (or if a test is accidentally deleted or indexes `v[name]` directly without `section()`), `cargo test --test classifier_vectors` passes while that section is silently skipped.
   - **Reproduction or verifiable rationale:**
     In `embeddings-fundamentals/tests/classifier_vectors.rs`, move any section (e.g. `"dobrar"`, or all 14 sections) from `PENDING` into `COVERED` without writing any test for it. `every_section_is_covered_pending_or_excluded_exactly_once` still sees the exact same union of keys (`COVERED ∪ PENDING ∪ EXCLUDED == sections`), and no test checks that every element of `COVERED` was passed to `section()`. All 7 tests in `classifier_vectors` pass.
   - **Suggested validation:**
     Drive all covered-section checks from a single orchestrator test (or record every section accessed via `section()` in a set and assert `accessed == COVERED` at the end of the suite), and add a negative self-test verifying that an unexercised entry in `COVERED` causes a failure.

3. **Finding `A0-3` (low): `tests/classifier_vectors.rs` does not assert exact key counts on `constantes` (or `meta` / `criterio`), and unit tests in `categorias.rs` / `tipos.rs` do not test serde deserialization round-trips for the enums.**
   - **File and line:** `embeddings-fundamentals/tests/classifier_vectors.rs:107–178`, `embeddings-fundamentals/src/classifier/categorias.rs:199–215`, `embeddings-fundamentals/src/classifier/tipos.rs:343–389`
   - **Description and impact:**
     1. In `tests/classifier_vectors.rs`, `constants_match` (`:130–157`) checks the 9 known keys in `vetores.json["constantes"]`, but does not assert `c.as_object().expect("object").len() == 9` (nor do `meta_matches_the_port_versions` and `criterion_matches_question_and_options_in_order` assert the key sets of `meta`, `criterio`, and `opcoes[i]`). When `vetores.json` is regenerated in the future and its `VECTORS_SHA256` is updated, a newly added constant in `constantes` would be silently ignored instead of failing the conformance test (`spec/CLASSIFIER.md` §14.1: `meta`, `constantes`, `criterio` → exact equality).
     2. In `src/classifier/categorias.rs:200` (`valor_round_trips_through_from_str_and_serde`) and `src/classifier/tipos.rs:343,375`, the tests assert `serde_json::to_string` (`Serialize`) and `.parse()` (`FromStr`), as well as `Display` not being tested at all, and never call `serde_json::from_str` (`Deserialize`) despite the test name claiming a round-trip through serde and the types deriving `Deserialize`.
   - **Reproduction or verifiable rationale:**
     - In `tests/classifier_vectors.rs:130–157`, `c` is indexed key-by-key without checking `c.as_object().unwrap().len()`.
     - In `src/classifier/categorias.rs:200–215`, `serde_json::from_str::<Categoria>` and `c.to_string()` (`Display`) are never called; in `src/classifier/tipos.rs:343–389`, `serde_json::from_str` is never called on `OrigemTexto`, `Camada`, `PaginasModo`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, or `MotivoCli`.
   - **Suggested validation:**
     Assert exact key counts (or key sets) on `constantes`, `meta`, and `criterio` in `tests/classifier_vectors.rs`, and add `serde_json::from_str` and `.to_string()` assertions to the unit tests in `categorias.rs` and `tipos.rs`.

4. **Finding `A0-4` (low): Phase 0 entry prerequisite and exact source commit SHA in `tests/fixtures/classifier/README.md` are still pending.**
   - **File and line:** `embeddings-fundamentals/tests/fixtures/classifier/README.md:8`, `embeddings-fundamentals/spec/CLASSIFIER.md:16, 130, 854–857`
   - **Description and impact:**
     `spec/CLASSIFIER.md` §20 Phase 0 specifies:
     - **Entradas:** *"o commit da troca de nomes no TestoDoc (§3.4), com a mensagem sem as palavras originais e enviado ao remoto."*
     - **Trabalho 2:** *"copiar TV para `tests/fixtures/classifier/vetores.json`, com o sha256 e o commit de origem em `tests/fixtures/classifier/README.md`"*
     As noted in `.review/PACKAGE.md` ("Notes for this round"), the TestoDoc rename commit has not yet had its commit message amended and been pushed to the remote, so `tests/fixtures/classifier/README.md:8` currently describes the source commit indirectly (`"assessment, the commit after 06f559f that renames two synthetic names (spec/CLASSIFIER.md §3.4)"`) rather than pinning the final immutable commit hash, and `spec/CLASSIFIER.md:16,130` still says `hoje 6cd3dd0, ainda não enviado ao remoto`.
   - **Reproduction or verifiable rationale:**
     Read `embeddings-fundamentals/tests/fixtures/classifier/README.md:8` and `embeddings-fundamentals/spec/CLASSIFIER.md:16,130`.
   - **Suggested validation:**
     Once the owner amends the commit message in TestoDoc and pushes it to the remote, record the final commit SHA in `tests/fixtures/classifier/README.md:8` and `spec/CLASSIFIER.md` §0.1 / §3.4 before closing the classifier port.

Let's double-check all requirements in `.review/PROMPT.md` and the user prompt:
- "Read .review/PROMPT.md and perform exactly the review it asks for, reading the files directly (no shell commands needed). Your answer must include the Coverage section and the per-criterion verification lines, then the complete findings list in the requested format, or NO FINDINGS."
- From `.review/PROMPT.md`:
  - **Output.** A list of findings. Each one has:
    - **ID** (`A0-1`, `A0-2`, …);
    - **severity** (high, medium or low);
    - **file and line**;
    - **description and impact**;
    - **reproduction or verifiable rationale**;
    - **suggested validation**.
  - **Required in every answer**, before the findings or `NO FINDINGS`:
    1. a "Coverage" section listing each file of `.review/phase.diff` you read, and the spec sections you checked it against;
    2. for each exit criterion of Phase 0, one line on how you verified it.

Let's check the 12 files listed in `.review/PACKAGE.md` / `.review/phase.diff`:
1. `embeddings-fundamentals/Cargo.lock`
2. `embeddings-fundamentals/Cargo.toml`
3. `embeddings-fundamentals/reports/progress/PROGRESS.md`
4. `embeddings-fundamentals/reports/reviews/classifier/classifier-findings.md`
5. `embeddings-fundamentals/spec/CLASSIFIER.md`
6. `embeddings-fundamentals/src/classifier/categorias.rs`
7. `embeddings-fundamentals/src/classifier/mod.rs`
8. `embeddings-fundamentals/src/classifier/tipos.rs`
9. `embeddings-fundamentals/src/lib.rs`
10. `embeddings-fundamentals/tests/classifier_vectors.rs`
11. `embeddings-fundamentals/tests/fixtures/classifier/README.md`
12. `embeddings-fundamentals/tests/fixtures/classifier/vetores.json` (in the worktree, left out of the diff because of its size)

And let's check each exit criterion / work item of Phase 0 (`spec/CLASSIFIER.md` §20, "Fase 0: preparação e decisões"):
- **Entradas:** o commit da troca de nomes no TestoDoc (§3.4), com a mensagem sem as palavras originais e enviado ao remoto.
- **Trabalho 1:** conferir os hashes do §0;
- **Trabalho 2:** copiar TV para `tests/fixtures/classifier/vetores.json`, com o sha256 e o commit de origem em `tests/fixtures/classifier/README.md`, e rodar a checagem de palavras dos exames (só contagens) antes do commit;
- **Trabalho 3:** resolver o `unicode-normalization` (`cargo update -p unicode-normalization --precise 0.1.24`, se possível; senão, a guarda do §4.2);
- **Trabalho 4:** acrescentar `fancy-regex = "0.19.2"` e a feature `classifier-jev`;
- **Trabalho 5:** criar o esqueleto de `src/classifier/` com os tipos do §5;
- **Trabalho 6:** criar o teste de conformidade com uma lista de seções pendentes, que só pode encolher. Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio;
- **Saída:** compila; testes dos tipos e das constantes contra TV (`meta`, `constantes`, `criterio`).

Everything is clear, thorough, and well-verified. I am ready to write the complete response.## Coverage

Every file in [`.review/phase.diff`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/.review/phase.diff) (plus [`vetores.json`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/vetores.json) in the worktree) was read directly and checked against the specifications:

1. [`embeddings-fundamentals/Cargo.toml`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.toml) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §4.2, §4.3, §20 (Phase 0 items 3–4), [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3, §5.4, and [`CLAUDE.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/CLAUDE.md).
2. [`embeddings-fundamentals/Cargo.lock`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.lock) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §4.2 and §20 (Phase 0 items 3–4).
3. [`embeddings-fundamentals/src/lib.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/lib.rs) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §4.1, §20 (Phase 0 item 5), and [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3.
4. [`embeddings-fundamentals/src/classifier/mod.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/mod.rs) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §4.1, §5, §15, §20 (Phase 0 item 5), and [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3.
5. [`embeddings-fundamentals/src/classifier/categorias.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §2.1, §4.1, §4.3, §5.1, §8.2, §14.1, §20 (Phase 0 item 5 & exit), [`vetores.json`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/vetores.json) (`meta`, `constantes`, `criterio`), and [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3.
6. [`embeddings-fundamentals/src/classifier/tipos.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §3.1, §3.2, §4.1, §4.3, §5.2–§5.5, §16 (invariant 8), §20 (Phase 0 item 5 & exit), and [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3.
7. [`embeddings-fundamentals/tests/classifier_vectors.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §0.1, §2.1, §4.2, §4.3, §5.1, §14.1, §20 (Phase 0 items 1, 3, 6 & exit), and [`spec/PLAN.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/PLAN.md) §2.3.
8. [`embeddings-fundamentals/tests/fixtures/classifier/README.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §0.1, §3.4, and §20 (Phase 0 entry & item 2).
9. [`embeddings-fundamentals/tests/fixtures/classifier/vetores.json`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/vetores.json) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §0.1, §2.1, §3.4, §5.1, §14.1, and §20 (Phase 0 item 2 & exit).
10. [`embeddings-fundamentals/spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) — checked against §0, §2, §3, §4, §5, §14.1, and §20 (Phase 0).
11. [`embeddings-fundamentals/reports/progress/PROGRESS.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/reports/progress/PROGRESS.md) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §20 and [`CLAUDE.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/CLAUDE.md).
12. [`embeddings-fundamentals/reports/reviews/classifier/classifier-findings.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/reports/reviews/classifier/classifier-findings.md) — checked against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §20, §21.2, and [`CLAUDE.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/CLAUDE.md).

---

## Phase 0 Exit Criteria Verification

- **Entry (TestoDoc rename commit with clean message pushed to remote, §3.4):** Checked [`.review/PACKAGE.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/.review/PACKAGE.md#L80), [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L16), and [`tests/fixtures/classifier/README.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L8); the synthetic names (`Zanoni`, `Nilo`/`Niló`) are already in `vetores.json`, while the owner's commit-message amendment and remote push in TestoDoc remain pending (see `A0-4`).
- **Criterion 1 (verify hashes in §0):** Verified [`VECTORS_SHA256`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L16) (`f14676c78e9bf590e0cb1534c83ab456c82cb8bd67e696cc94bb239d891db22e`) in [`tests/classifier_vectors.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L82-L85) and [`tests/fixtures/classifier/README.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L9) against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L17) §0.1.
- **Criterion 2 (copy TV to `tests/fixtures/classifier/vetores.json`, document sha256 and source commit in `README.md`, and run count-only exam-word check):** Verified [`vetores.json`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/vetores.json) and [`tests/fixtures/classifier/README.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L1-L15) directly; the sha256 and count-only check are documented, though `README.md` currently records a prose description of the commit rather than the final SHA (see `A0-4`).
- **Criterion 3 (resolve `unicode-normalization` to `0.1.24` or implement the §4.2 guard):** Verified [`Cargo.toml`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.toml#L52) (`unicode-normalization = "=0.1.24"`), [`Cargo.lock`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.lock#L6099-L6102) (`version = "0.1.24"`), and [`normalization_tables_are_the_vectors_unicode_version`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L119-L127) asserting `unicode_normalization::UNICODE_VERSION == (16, 0, 0)`.
- **Criterion 4 (add `fancy-regex = "0.19.2"` and feature `classifier-jev`):** Verified [`Cargo.toml`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.toml#L20-L27) (`classifier-jev = ["dep:reqwest"]` and `fancy-regex = "0.19.2"`) and [`Cargo.lock`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/Cargo.lock#L1578).
- **Criterion 5 (create `src/classifier/` skeleton with the types of §5):** Verified [`src/classifier/mod.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/mod.rs), [`src/classifier/categorias.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs), and [`src/classifier/tipos.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs) against [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §5.1–§5.5; all §5.1–§5.4 types and most §5.5 types are implemented, except `Classificacao` from §5.5 (see `A0-1`).
- **Criterion 6 (conformance test with shrinking pending list; pending section fails if covered without leaving the list; no section silently skipped):** Verified [`tests/classifier_vectors.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L18-L104); `section()` panics if called on a section in `PENDING` (`a_pending_section_cannot_be_tested_without_moving_it`), and `every_section_is_covered_pending_or_excluded_exactly_once` checks the partition, but nothing checks that every section listed in `COVERED` is actually tested (see `A0-2`).
- **Exit (compiles; tests of types and constants against TV `meta`, `constantes`, `criterio`):** Verified the unit tests in [`categorias.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs#L191-L216) and [`tipos.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L334-L443), the conformance tests [`meta_matches_the_port_versions`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L107-L116), [`constants_match`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L130-L157), and [`criterion_matches_question_and_options_in_order`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L160-L178) against [`vetores.json`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/vetores.json#L2-L430), and the check evidence in [`.review/PACKAGE.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/.review/PACKAGE.md#L40-L76) (see `A0-3`).

---

## Findings

### `A0-1`
- **Severity:** medium
- **File and line:** [`embeddings-fundamentals/src/classifier/tipos.rs:238-333`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L238-L333), [`embeddings-fundamentals/src/classifier/mod.rs:16-20`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/mod.rs#L16-L20), [`embeddings-fundamentals/reports/progress/PROGRESS.md:364`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/reports/progress/PROGRESS.md#L364)
- **Description and impact:**
  [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L860) §20 (Phase 0, item 5) requires creating the skeleton of `src/classifier/` with the types of §5 (*"criar o esqueleto de `src/classifier/` com os tipos do §5"*), [`tipos.rs:1`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L1) states *"Input, prepared state and result types (spec/CLASSIFIER.md §5)"*, and [`PROGRESS.md:364`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/reports/progress/PROGRESS.md#L364) claims *"`tipos.rs` holds the types of §5.2–§5.5"*. However, `Classificacao` — the main classification result struct defined in [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L285-L312) §5.5 — is missing from `tipos.rs` and `mod.rs`, along with its custom serialization rules (`probabilidades` serialized as an object in [`Categoria::TODAS`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs#L68-L78) order, or `{}` when `None`).
- **Reproduction or verifiable rationale:**
  Compare [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L275-L312) §5.5 against [`src/classifier/tipos.rs:238-333`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L238-L333): `RespostaEscolha`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, `MotivoCli`, and `Uso` are defined, whereas `pub struct Classificacao` is absent.
- **Suggested validation:**
  Add `Classificacao` to [`src/classifier/tipos.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs) and re-export it from [`src/classifier/mod.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/mod.rs) with a unit test covering its `probabilidades` serialization (`None` → `{}` and `Some([..])` in `Categoria::TODAS` order) — or, if `Classificacao` is intentionally deferred to Phase 4 (`classificar`), explicitly document that split in [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md) §4.1/§20 and [`PROGRESS.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/reports/progress/PROGRESS.md).

### `A0-2`
- **Severity:** medium
- **File and line:** [`embeddings-fundamentals/tests/classifier_vectors.rs:19-38`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L19-L38), [`embeddings-fundamentals/tests/classifier_vectors.rs:64-104`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L64-L104)
- **Description and impact:**
  [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L861) §20 (Phase 0, item 6) requires that no section in `vetores.json` can be silently skipped (*"Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio"*).
  In [`tests/classifier_vectors.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs), [`section()`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L64-L72) checks `COVERED.contains(&name)` when a test calls `section()`, and [`every_section_is_covered_pending_or_excluded_exactly_once`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L88-L104) checks that `COVERED`, `PENDING`, and `EXCLUDED` partition the top-level keys of `vetores.json`. However, nothing checks that every section listed in `COVERED` is actually accessed and asserted by a test. If a section is moved from `PENDING` to `COVERED` without adding a test that calls `section(&v, name)` (or if a test is removed), the test suite passes while that section is silently skipped.
- **Reproduction or verifiable rationale:**
  Move `"dobrar"` (or all 14 entries of `PENDING`) from `PENDING` into `COVERED` in [`tests/classifier_vectors.rs:19-38`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L19-L38) without adding any test for `"dobrar"`. `COVERED.iter().chain(PENDING).chain(EXCLUDED)` still partitions `v.as_object().unwrap().keys()`, and no test verifies that `"dobrar"` was ever passed to `section()`. All tests in `tests/classifier_vectors.rs` pass.
- **Suggested validation:**
  Have a single conformance test (or a registry of accessed section names verified against `COVERED`) assert that the set of sections actually checked equals `COVERED`, and add a self-test verifying that an unexercised section in `COVERED` fails.

### `A0-3`
- **Severity:** low
- **File and line:** [`embeddings-fundamentals/tests/classifier_vectors.rs:107-178`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L107-L178), [`embeddings-fundamentals/src/classifier/categorias.rs:199-215`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs#L199-L215), [`embeddings-fundamentals/src/classifier/tipos.rs:343-389`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L343-L389)
- **Description and impact:**
  1. In [`tests/classifier_vectors.rs`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L130-L157), `constants_match` checks the 9 known keys in `vetores.json["constantes"]` individually but does not assert that `constantes` has no additional keys (`c.as_object().unwrap().len() == 9`), nor does `criterion_matches_question_and_options_in_order` assert the key count of `criterio` (3) or each `opcao` (4). If a future `vetores.json` update adds a constant to `constantes`, `constants_match` will still pass after updating `VECTORS_SHA256`.
  2. In [`src/classifier/categorias.rs:199-215`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs#L199-L215), `valor_round_trips_through_from_str_and_serde` only tests `serde_json::to_string(&c)` (`Serialize`) and `FromStr`, never `serde_json::from_str::<Categoria>` (`Deserialize`) or `Display` (`c.to_string()`), despite its name claiming a serde round-trip. Similarly, [`src/classifier/tipos.rs:343-389`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L343-L389) only tests `Serialize` on `OrigemTexto`, `Camada`, `PaginasModo`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, and `MotivoCli`, never `Deserialize` or `PaginasModo`'s `Display`.
- **Reproduction or verifiable rationale:**
  - In [`tests/classifier_vectors.rs:130-157`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/classifier_vectors.rs#L130-L157), `c` is indexed by hardcoded key names without asserting `c.as_object().unwrap().len() == 9`.
  - In [`src/classifier/categorias.rs:201-209`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/categorias.rs#L201-L209) and [`src/classifier/tipos.rs:343-389`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/src/classifier/tipos.rs#L343-L389), a broken custom `Deserialize` or `Display` implementation on those enums would still pass all current unit tests.
- **Suggested validation:**
  Assert exact object lengths in `constants_match` and `criterion_matches_question_and_options_in_order`, and add `serde_json::from_str` and `.to_string()` round-trip assertions in `categorias::tests` and `tipos::tests`.

### `A0-4`
- **Severity:** low
- **File and line:** [`embeddings-fundamentals/tests/fixtures/classifier/README.md:8`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L8), [`embeddings-fundamentals/spec/CLASSIFIER.md:16`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L16), [`embeddings-fundamentals/spec/CLASSIFIER.md:130`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L130)
- **Description and impact:**
  [`spec/CLASSIFIER.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L854-L857) §20 (Phase 0 entry & item 2) requires that the TestoDoc synthetic-name replacement commit have a commit message without the original words, be pushed to the remote, and have its source commit recorded in [`tests/fixtures/classifier/README.md`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L8). As noted in [`.review/PACKAGE.md:80`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/.review/PACKAGE.md#L80), that owner action in TestoDoc is still pending, so `README.md:8` currently says `"assessment, the commit after 06f559f that renames two synthetic names (spec/CLASSIFIER.md §3.4)"` instead of pinning the final pushed commit SHA.
- **Reproduction or verifiable rationale:**
  Inspect [`embeddings-fundamentals/tests/fixtures/classifier/README.md:8`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L8) and [`embeddings-fundamentals/spec/CLASSIFIER.md:16,130`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L16).
- **Suggested validation:**
  Once the TestoDoc commit message is amended and pushed to the remote, replace the prose commit description in [`tests/fixtures/classifier/README.md:8`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/tests/fixtures/classifier/README.md#L8) and [`spec/CLASSIFIER.md:16,130`](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/b7a8c7eb-40fe-411d-89b8-2f8aa53d9963/scratchpad/cls-wt/embeddings-fundamentals/spec/CLASSIFIER.md#L16) with the final pushed commit SHA.
