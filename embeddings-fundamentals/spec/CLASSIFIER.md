# CLASSIFIER — módulo RLCD/Jev em Rust: classificador do tipo de exame

| | |
|---|---|
| Versão | 2 (31/10/2026), com os 12 achados da 1ª rodada do Antigravity corrigidos; o histórico está no §21 |
| Pedido do dono | "no relatório de anonimização, inclua mais um passo: o resultado do classificador, coloque na tabela o tipo de exame o classificador identificou e os pesos de todas alternativas" e "crie em spec/CLASSIFIER.md uma especificação para implementar em Rust o módulo RLCD que chama o Jev da Typesafe.ai [...] e um plano faseado" |
| Onde o código vai morar | `embeddings-fundamentals/src/classifier/` (este crate), subcomando `classify` |
| Estado | Especificação e plano. Nenhum código existe ainda. |

## 0. Fontes, precedência e como ler

### 0.1 Fontes normativas (fixadas por hash)

| Sigla | Fonte | Identificação |
|---|---|---|
| **TD** | Especificação do classificador Python do TestoDoc, `docs/system-design/classificador.md` (versão 5) | repositório `TestoDoc/testo-doc`, branch `assessment`, commit seguinte a `06f559f` com a troca de nomes sintéticos de 31/10/2026 (F8; hoje `6cd3dd0`, ainda não enviado ao remoto); sha256 `1219c95f718a6b799a3dd07e5579586eb50daa8c3c5f0fd30d6e83f554792aef` |
| **TV** | Vetores de conformidade, `backend/tests/fixtures/classificador/vetores.json` (gerados pelo Python 3.14, UCD 16.0.0, `mascaramento-5`, `classificador-documento-1`) | idem; sha256 `f14676c78e9bf590e0cb1534c83ab456c82cb8bd67e696cc94bb239d891db22e`; 347 582 bytes. Antes da troca: `beae67ca…a512` |
| **TP** | Código Python de referência | `backend/app/services/{mascaramento,classificador_documento,rlcd_client,patient_hints}.py`, `backend/app/rlcd_providers/typesafe.py` (sha256 no §0.3) |
| **TS-API** | OpenAPI oficial do Jev, `POST /v1/systemone` (versão 0.2.0) | `https://docs.typesafe.ai/api.md`, lida em 31/10/2026 |
| **TS-ERR**, **TS-MOD**, **TS-JAG** | Regras de request e erros; modelos e limites; falhas conhecidas do Jev 1.13 | `https://docs.typesafe.ai/api/errors.md`, `/models.md`, `/model-jaggedness/jev-1.13.md`, lidas em 31/10/2026 |
| **SDK** | `typesafe-sdk` 0.7.4 (o que o Python usa de fato) | cache local do uv, `typesafe_sdk/_core/{endpoints,json,transport}.py`, `_core/schemas/base.py` |

Fonte informativa, não normativa: o PRD do TestoDoc, `docs/analise/RLCD-CLASSIFIER.md` (**TD-PRD**, sha256 `ed0a7bd0…ea9ef8`). Ele explica o porquê das decisões, mas é anterior ao código (TD, cabeçalho).

### 0.2 Precedência

1. **TV** decide o comportamento do núcleo (N1): um porte está correto quando reproduz cada caso **byte a byte** (TD §17.2).
2. Onde TV não cobre, vale **TP**, e TD descreve TP. Uma divergência entre TD e TP é defeito de TD (TD, cabeçalho).
3. O fio HTTP segue **TS-API**. Onde TS-API não diz, vale o que o **SDK** 0.7.4 faz (é o que a calibração do TestoDoc mediu).
4. Este documento decide só o que é próprio do CognitionLab: escopo (§2), privacidade (§3), arquitetura Rust (§4–§5), CLI (§10), integração com o relatório (§11) e as divergências deliberadas, todas listadas no §13. **Nenhum comportamento do classificador é inventado aqui**: quando falta evidência, a lacuna está no §18.

### 0.3 Hashes do código de referência (commit `06f559f`)

```
9c8d9b0b0d1cb3170480e7ca8d8fdc242b49db383b522ab400d6d98e3133da05  backend/app/services/mascaramento.py
72f893b3384134352357e4156dde0b8cd00387ae80d5f34ec9295ec7e996c8f4  backend/app/services/classificador_documento.py
b2f9cfaf8c40c82f85449fb2e4e276bbe66120a5759269d6f24cadb393dfdcf2  backend/app/services/rlcd_client.py
36925de5bf44c76200085cd7ab7fe0883dea501ca593df8b10f876516f5d6520  backend/app/rlcd_providers/typesafe.py
43917ab30c9e22b3245c2c73cef9833f69d24efbe5d42eed40125f0cffd94a54  backend/app/services/patient_hints.py
```

Se qualquer hash do §0.1 ou do §0.3 mudar, a implementação para e este documento é revisto antes (§20, Fase 0).

### 0.4 Divergências e lacunas encontradas nas fontes (validação de fidelidade)

| # | Fonte | Observação | Efeito aqui |
|---|---|---|---|
| F1 | TD, cabeçalho | Diz que os vetores estão "ainda não commitados". Estão no commit `3baedb5` e em `06f559f`. | Nenhum: TV é lido do commit fixado. |
| F2 | TD §9.3 e Q8 | "Não há especificação OpenAPI no repositório." Hoje há a OpenAPI oficial (TS-API, versão 0.2.0). | Q8 do TestoDoc fica respondida para o fio do Jev (§8). |
| F3 | TS-API × SDK | TS-API não documenta o header `x-typesafe-request-id`. O SDK o lê e **levanta** `TypeSafeError` quando falta (`schemas/base.py`, `request_id`), que é a causa do achado A1 (TD §17.4). | Divergência deliberada DV2 (§13). |
| F4 | TS-ERR × TD §9.3 | TS-ERR documenta `413`, `415` e `529`, ausentes da tabela de TD. No Python, todos caem em `outro` (o `_categoria` de `typesafe.py` só reconhece os nomes de exceção listados). | §8.6 inclui os três, com a mesma categoria `outro`. |
| F5 | TS-MOD × TP | `typesafe.py` diz que `jev-latest` resolve para `jev-1.13.0` (14/10/2026). Hoje resolve para `jev-1.14.0` (lançado em 14/10/2026). | O modelo fixo continua `jev-1.13.0` (§12). Trocar exige recalibrar (Q-C3). |
| F6 | TP `_finito` | `float(valor)` aceita string numérica e `bool`. No caminho real, quem decodifica é o pydantic do SDK. | Divergência deliberada DV3 (§13). |
| F7 | TS-JAG #8 | "jev-1.13 leans toward the option that comes first." A ordem das opções faz parte do request. | §8.2 fixa a ordem do fio. A conferência por reordenação fica na Fase 7 (opcional). |
| F9 | TD §7.3 × TP | TD diz que a remoção é idempotente. TP não é: o diferencial da Fase 2 achou 65 textos não idempotentes em 5 000, todos com peças separadas por espaço ou pontuação. Exemplo: `"Paciente:: Médico responsável:\t123.456.789-09: "` dá `"Paciente:[REMOVIDO]: "` na primeira passada e `"Paciente:[REMOVIDO] "` na segunda. | O porte reproduz TP. A propriedade do §14.2 virou a comparação da segunda passada com TP no diferencial (§14.3), mais um teste com o exemplo. |
| F10 | TP, camadas 2 e 4 | Um texto colado a dígitos escapa: em `"CRM/SP 123456Zanoni Prado"`, o nome conhecido não sai, porque não há `\b` entre `6` e `Z`, e a `conferir` usa o mesmo padrão e também não acha. O número de registro colado ao texto seguinte só sai numa segunda passada. | **Corrigido no porte por decisão do dono (31/10/2026: "não seja compatível com o Python, corrija no Rust"), DV10.** O mesmo vazamento vale para `"Ref 12345Zanoni Prado"` (número que não é padrão), `"arquivo_Zanoni_Prado_eco"` e `"Zanoni2 Prado3"`: em TP o nome sai inteiro e a `conferir` não acha. O número de registro colado continua como em TP: não sai na primeira passada, mas a `conferir` o acha e bloqueia o envio. A correção no TestoDoc é do dono. |
| F8 | TV | Duas palavras usadas como **nome sintético** nos vetores (uma de 6 letras, 6 ocorrências depois de rótulo de nome; uma de 4 letras, numa lista de nomes conhecidos) coincidiam com palavras dos nomes de arquivo dos exames reais em `tests/sample-exams/`. A verificação foi feita só por contagem: nenhuma palavra foi lida nem impressa. | **Resolvido em 31/10/2026 (Q-P2, opção a):** trocadas por nomes inventados (§3.4) e vetores regenerados. |

## 1. Objetivo e contexto

O anonimizador (`../anonimizador`) gera, a cada execução, um índice Markdown com uma linha por exame. O dono quer nesse índice **o tipo de exame que o classificador identificou e os pesos (probabilidades) de todas as alternativas**.

O classificador decide **qual dos nove tipos** (TD §4) um documento é, com uma **confiança** em [0, 1]. Ele faz uma única pergunta `Choice` ao Jev, o modelo System One da TypeSafe, sobre um estado JSON que só contém o que identifica o tipo. Os dados pessoais que as regras reconhecem são removidos antes do envio, e uma conferência final bloqueia o envio se sobrar algo (TD §1, §6–§8).

O classificador em produção é Python, no TestoDoc. Este documento especifica um **porte em Rust** do núcleo e da chamada ao Jev, com o mesmo comportamento verificável byte a byte (TV). Especifica também um CLI de um arquivo e a ligação com o relatório do anonimizador.

## 2. Escopo

### 2.1 As 30 partes do TestoDoc (TD §2.1) neste porte

| # | Parte (TD §2.1) | Camada | Aqui | Motivo |
|---|---|---|---|---|
| 1 | Categorias, pergunta, versão das perguntas | N1 | **Entra** | Normativo (TD §3.3, §4). |
| 2 | `VOCABULARIO_TIPO`, `termos_permitidos` | N1 | **Entra** | |
| 3 | `compor`, `dobrar`, `_substituir` | N1 | **Entra** | |
| 4 | `remover_dados_pessoais` (5 camadas + colapso) | N1 | **Entra** | É o que decide o que sai da máquina. |
| 5 | `conferir` | N1 | **Entra** | |
| 6 | `extract_patient_hints` (campo `nome`), `_nomes_dos_indicios`, `nomes_dos_rotulos` | N1 | **Entra** | |
| 7 | `montar_estado` | N1 | **Entra** | |
| 8 | `json_canonico`, `digest_entrada` | N1 | **Entra** | |
| 9 | `decidir_classificacao` | N1 | **Entra** | |
| 10 | `validar_respostas` (só o critério `Choice` do classificador) | N1 | **Entra** | Bool e Score não são usados pelo classificador. |
| 11 | `montar_request`, `minimizar_estado`, `referencia_pseudonima` | N2 | **Entra em parte** | O request neutro entra. `minimizar_estado` vira um invariante testado (§9.2). `referencia_pseudonima` fica fora: o adaptador do Jev não a envia (TD §9.3, "O `referencia` e o `request_id` não são enviados"), e este porte não tem outro provedor nem banco. |
| 12 | Adaptador TypeSafe/Jev | N2 | **Entra** | HTTP direto, sem SDK (§8). |
| 13 | Adaptador OpenAI Decisions | N2 | Fora | O pedido é o módulo que chama o Jev. O trait do §5.6 permite acrescentá-lo depois. |
| 14 | Dublê do provedor | N2 | **Só em testes** | Implementação do trait em `#[cfg(test)]` e em `tests/`; não existe no binário (DV5). |
| 15 | `avaliar`: timeout, categorias de erro, semáforo | N2 | **Entra** | |
| 16–20 | Orquestração com reutilização, repositório Postgres, travas Redis/Postgres, registro de uso, teto da etapa | N3 | Fora | Dependem do banco, do worker e do `UsageContext` do TestoDoc, que não existem aqui (TD §11–§12). |
| 21–27 | `entrada_da_analise`, regras de `ativo`, `aplicar_resumo`, `resolver_modelo`, `process_analise`, API, validação de boot | N4 | Fora | Integração com o pipeline de laudo do TestoDoc. |
| 28 | CLI de um arquivo | N4 | **Entra, adaptado** | §10. |
| 29 | Lote de calibração e métricas | N4 | **Opcional** (Fase 7) | Exige um conjunto rotulado por médico (TD §17.3). |
| 30 | OCR local (Tesseract) | N4 | **Entra, pela extração do crate** | Reusa `OcrEngine` (`src/extraction/`). |

### 2.2 Fora do escopo, explicitamente

- Gravar classificações, reutilizar respostas anteriores ou fazer cache. Cada execução com envio chama o Jev uma vez por documento.
- Os modos `observe`/`ativo` e qualquer efeito sobre um laudo. Aqui o resultado só é mostrado e escrito no relatório.
- Mudar categorias, pergunta, descrições, limites ou o mascaramento. Qualquer mudança nessas partes exige nova `PERGUNTAS_VERSAO` ou `MASCARAMENTO_VERSAO` **no TestoDoc** e vetores regenerados lá (TD §17.1, invariante 8). Exceção decidida pelo dono: a DV10 (§13).

## 3. Privacidade: regras que bloqueiam

### 3.1 O conflito

O classificador **envia texto de exame, já mascarado, a um serviço remoto** (o Jev). O `CLAUDE.md` deste crate proíbe enviar os exames de `tests/sample-exams/`, ou qualquer coisa derivada deles, "a qualquer motor remoto". Rodar o classificador com envio sobre esses exames só é possível com uma exceção escrita pelo dono (Q-P1).

**Decisão do dono (Q-P1, 31/10/2026): o envio é permitido só para as cópias anonimizadas que passaram na re-verificação do `anonimizador` (`verify` ok), com o opt-in duplo de R-P1.** O envio do texto dos originais continua proibido. A exceção está registrada no `CLAUDE.md`.

Fatos que embasaram a decisão:
- O mascaramento não guarda o que remove. `Resultado` tem só o texto mascarado e as contagens por camada (TP `mascaramento.py`, classe `Resultado`), e `EstadoPreparado` tem só contagens (TP `classificador_documento.py`, classe `EstadoPreparado`). Nada do que foi removido é guardado nem enviado. O porte mantém isso (§5.3; invariante 8 do §16).
- O mascaramento só remove o que as regras reconhecem e não garante anonimização universal (TD-PRD §2, M4; TD §8, "a conferência não é uma segunda garantia independente"). Um dado pessoal que as regras não reconhecem, como um nome sem rótulo e sem tratamento no meio do texto, iria junto. Por isso a entrada é a cópia anonimizada: as caixas pretas da rasterização vêm antes do mascaramento.
- A TypeSafe declara que o Jev não é treinado com requests nem respostas de clientes. A retenção zero (ZDR) só existe para clientes enterprise (TS-MOD, "Data handling").

### 3.2 Regras de implementação

- **R-P1. Envio só por opt-in duplo.** O CLI só chama o Jev com a opção `--enviar` **e** a variável `CLASSIFIER_REMOTE_ALLOWED=1`. Faltando qualquer um dos dois, o resultado é `preparado` (sem envio, como `--sem-envio` no TestoDoc, TD §5.4). O equivalente de `remote_allowed` do crate (CLAUDE.md, "Data handling") é checado **antes** de ler a chave e de construir o cliente HTTP.
- **R-P2. Nenhum texto em logs nem na saída padrão.** Os eventos `tracing` e a linha JSON (§10.3) levam contagens, digest, estado, tipo e probabilidades, nunca texto, nomes nem o nome do arquivo de entrada. A exceção é `--mostrar-estado`, que imprime o estado mascarado e só pode ser usada com entradas sintéticas (R-P4).
- **R-P3. A chave nunca aparece.** `Debug` e `Display` dos tipos que a guardam a redigem. Ela não entra em mensagens de erro e só é lida depois de R-P1.
- **R-P4. Testes.** Os testes automáticos usam só entradas sintéticas. Os testes com exames reais seguem o `CLAUDE.md` (`#[ignore]`, `RUN_SAMPLE_EXAMS=1`, só propriedades e contagens) e **nunca** usam `--enviar`.
- **R-P5. Revisores.** Os pacotes de revisão nunca levam exames, texto deles, nem as palavras do F8 (§3.4).

### 3.3 O nome do arquivo

`termos_permitidos` só deixa passar os termos de `VOCABULARIO_TIPO` (TD §6, passo 5, e invariante 4), então o nome do arquivo e o título nunca levam nome de pessoa ao provedor. Mesmo assim, R-P2 vale: o nome do arquivo não aparece na saída.

### 3.4 Os vetores e o achado F8

Copiar TV para `tests/fixtures/classifier/` o publica no GitHub, e a regra "dados sintéticos não reusam palavras dos nomes de arquivo dos exames" vale para ele. O dono escolheu (Q-P2, 31/10/2026) **regenerar os vetores no TestoDoc** com outros nomes inventados. Feito no mesmo dia:
- **Troca:** as duas palavras foram trocadas por nomes inventados de mesmo tamanho (`Zanoni` e `Nilo`), preservando caixa e acento (`Niló`), em 32 ocorrências de 5 arquivos do classificador. Os arquivos foram o gerador `app/cli/vetores_classificador.py`, os testes `test_mascaramento.py`, `test_classificador_documento.py` e `test_classificar_documento_cli.py`, e a TD. A palavra de 4 letras também existe fora do classificador, em minúsculas, como nome de biblioteca em `security.py` e `requirements.txt`; ali não foi tocada.
- **Como os nomes foram escolhidos:** nenhum está nos nomes de arquivo dos exames, no vocabulário do classificador ou em outro lugar do TestoDoc.
- **Vetores regenerados:** com o Python 3.14, só as 14 linhas de entrada mudaram. Saídas, estados e digests ficaram idênticos, porque os nomes são mascarados antes de qualquer saída. Os 356 testes do classificador no TestoDoc passam.
- **Checagem final:** nenhuma palavra usada como nome nos vetores coincide com os nomes de arquivo dos exames. Sobram duas coincidências com palavras comuns de 7 letras (usadas em 15 e 16 arquivos do TestoDoc), sem uso como nome.
- **Commit:** a troca está no commit `6cd3dd0` do TestoDoc, ainda não enviado ao remoto. Os hashes do §0.1 batem com ele.

## 4. Arquitetura no crate

### 4.1 Módulos

```
src/classifier/
├── mod.rs            # API pública: reexporta os tipos, montar_estado, classificar
├── categorias.rs     # Categoria (9), pergunta, PERGUNTAS_VERSAO e as constantes de TV `constantes`
├── tipos.rs          # tipos do §5: entrada, estado preparado, resposta, estados e motivos
├── pyclasses.rs      # GERADO: classes de caracteres do `re`/`str` do Python 3.14 (UCD 16.0.0), §6.3
├── texto.rs          # compor, dobrar, isupper/lower à Python, mapa de offsets, fatiamento em code points
├── pyre.rs           # Envoltório de regex com a semântica do `re` do Python (pos/endpos, `$`, IGNORECASE), §6.4
├── mascaramento/
│   ├── mod.rs        # remover_dados_pessoais, conferir, MASCARAMENTO_VERSAO
│   ├── rotulos.rs    # camada 1: _ROTULO_RE, em linha, em colunas, _valor_mascarado, nomes_dos_rotulos, _parece_nome
│   ├── conhecidos.rs # camada 2: partes_de_nome, literais
│   ├── tratamento.rs # camada 3
│   ├── padroes.rs    # camada 4 (com os filtros _nao_e_faixa e _nao_e_contagem)
│   ├── datas.rs      # camada 5
│   └── substituir.rs # _substituir e o colapso dos marcadores
├── indicios.rs       # extract_patient_hints (só o campo nome) e _nomes_dos_indicios
├── estado.rs         # montar_estado, caracteres_uteis, termo_do_tipo, EstadoPreparado
├── canonico.rs       # json_canonico à Python e digest (NÃO reusa ids::canonical_json, §7.4)
├── decisao.rs        # validar_resposta_escolha, decidir_classificacao
├── provedor.rs       # trait RlcdProvider, request neutro, avaliar (timeout + semáforo)
└── jev.rs            # adaptador HTTP do Jev; só com a feature `classifier-jev`
```

O CLI fica em `src/cli.rs` (subcomando `classify`), e a extração reusa `src/extraction/`.

### 4.2 Features e dependências

- **N1** (todos os módulos menos `jev.rs`) compila sempre. É puro: sem I/O, sem rede e sem estado global mutável.
- **`classifier-jev = ["dep:reqwest"]`**: o adaptador HTTP. `reqwest` já é dependência opcional do crate (`engine-openai`), com `rustls`.
- Dependências novas:

  | Crate | Versão | Para quê |
  |---|---|---|
  | `fancy-regex` | `0.19.2` | Lookbehind e lookahead (`(?<=\s\s)`, `(?<=[\t\|;])`, `(?!…)`), que o crate `regex` não suporta (TD §18.1). |

  Nada mais: `sha2`, `hex`, `serde_json`, `unicode-normalization`, `tokio` e `thiserror` já estão no `Cargo.toml`.
- **Unicode 16.0.0 em tudo.** TV foi gerado com a UCD 16.0.0 (TV `meta.unicode_versao`):
  - `regex-syntax` 0.8.11, usado pelo `fancy-regex`, também é UCD 16.0.0 (`unicode_tables/perl_word.rs`), mas as classes `\w`, `\d`, `\s` e `\b` do motor **não** são usadas (§6.3);
  - `unicode-normalization` 0.1.26, a versão do `Cargo.lock`, é **Unicode 18.0.0**, e a 0.1.24 é **16.0.0** (constante `UNICODE_VERSION` de cada versão). A Fase 0 fixa `unicode-normalization = "=0.1.24"` se `pdf-extract` e `stringprep`, que também dependem dele, aceitarem.
  - Senão, `compor` e `dobrar` deixam inalterado todo code point **não atribuído na UCD 16.0.0**, consultando uma tabela gerada como as do §6.3. Nesse caminho, `compor` aplica o NFC **separadamente a cada trecho contíguo de code points atribuídos**, e os não atribuídos ficam como barreira. Na UCD 16 eles têm classe combinante 0, então uma marca nova das UCD 17/18 nunca é reordenada junto com as vizinhas.
  - Os dois caminhos só diferem para caracteres novos nas versões 17 e 18, e o teste do §14.4 cobre o caminho escolhido.

### 4.3 Convenções (PLAN §2.3 e `rust-skills`)

- Nada de `unwrap`, `expect` ou `panic!` fora de testes: o `Cargo.toml` nega `unwrap_used`, `expect_used` e `panic`.
- **Regexes fixas:** compiladas uma vez, sem `LazyLock` + `expect`. Ficam num `Mascarador`, construído por `Mascarador::new() -> Result<Self, ClassifierError>` e passado por referência (`&Mascarador`, `Send + Sync`) a toda função de N1 que usa regex. O CLI o constrói uma vez, e um erro ali sai como erro interno (código 1).
- **Regexes dinâmicas:** as de nomes conhecidos (`_padroes_conhecidos`) e a busca de `_nomes_dos_indicios` (§6.4) são compiladas por chamada, e uma falha de compilação vira `ClassifierError`, nunca pânico.
- Erros com `thiserror` na biblioteca; `anyhow` só em `main`/`cli`. `#[expect(lint, reason = "...")]` em vez de `#[allow]`.
- Enums e newtypes no lugar de strings (`Categoria`, `PaginasModo`, `OrigemTexto`, `EstadoClassificacao`, `ErroCategoria`, `MotivoCodigo`, `Camada`).
- `async fn` nativo em trait, sem `#[async_trait]`: o provedor não é usado como `dyn` (PLAN §2.2). O CLI é genérico sobre `P: RlcdProvider`.
- Floats: `f64`; comparação com `total_cmp`; nenhum `==` entre floats fora das tolerâncias do §7.

## 5. Contratos de tipos

Os nomes de campo e os valores de enum, na serialização, são **exatamente** os do Python (TD §5, §11.2), para que a saída se compare com TV e com o CLI do TestoDoc.

### 5.1 Categorias (TD §4, normativo)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Categoria { Vop, EcoConvencional, EcoAvancado, Calorimetria, Ergoespirometria, Holter, Mapa, Ecg, Outro }

impl Categoria {
    /// Na ordem da tupla CATEGORIAS: é a ordem das opções no fio (§8.2).
    pub const TODAS: [Categoria; 9] = [/* Vop … Outro, nessa ordem */];
    pub const fn valor(self) -> &'static str;      // "vop", "eco_convencional", …, "outro"
    pub const fn rotulo(self) -> &'static str;     // "VOP", "Eco convencional", …, "Outro"
    pub const fn descricao(self) -> &'static str;  // literal de TD §4 / TV `criterio.opcoes[i].descricao`
    pub const fn slug(self) -> Option<&'static str>; // "eco-strain-avancado" para EcoAvancado; None para Outro
}
```

- `PERGUNTA` é o literal de TD §3.3(a) (= TV `criterio.pergunta`).
- `CRITERIO_ID = "tipo_documento"`, `PERGUNTAS_VERSAO = "classificador-documento-1"`, `MASCARAMENTO_VERSAO = "mascaramento-5"` (a de TV) e `MASCARAMENTO_VERSAO_PORTE = "mascaramento-5+cognitionlab-1"` (a do mascaramento do porte, com a DV10; é a que sai em `mascaramento_versao` e em `politica_versao`).
- Um teste compara cada constante com TV (`criterio`, `constantes`, `meta`). Os textos **não** são redigitados: são copiados do TV e conferidos.

### 5.2 Entrada (TD §5.1)

```rust
pub enum OrigemTexto { Nativo, OcrLocal, Visao }        // "nativo", "ocr_local", "visao"
pub struct Pagina { pub n: u32, pub texto: String, pub origem: OrigemTexto }
pub struct EntradaClassificacao {
    pub nome_arquivo: String,       // pode ser vazio
    pub titulo: String,             // pode ser vazio
    pub extensao: String,
    pub total_paginas: u32,         // copiado; não é validado contra paginas.len() (TD §5.1)
    pub paginas: Vec<Pagina>,       // na ordem do documento; podem ter texto vazio
    pub nomes_conhecidos: Vec<String>,
}
pub enum PaginasModo { Uma, Duas, Todas }               // "1", "2", "todas"
```

`PaginasModo` é um enum, então o `ValueError` do Python para um modo inválido (TD §6, passo 1) vira erro de parse no CLI (código 5).

### 5.3 Estado preparado (TD §6)

```rust
pub struct EstadoPreparado {
    pub estado: EstadoEnviado,          // §5.4
    pub digest: [u8; 32],               // sha256(json_canonico(estado))
    pub paginas_enviadas: u32,
    pub caracteres_enviados: u32,       // em code points
    pub truncado: bool,
    pub mascaramentos: BTreeMap<Camada, u32>,  // só valores > 0
    pub origens_texto: Vec<OrigemTexto>,       // ordenada pelo valor string, sem repetição
    pub bloqueio: BTreeMap<Camada, u32>,       // vazio = nada achado
    pub caracteres_uteis: u32,
    pub termo_do_tipo: bool,
}
impl EstadoPreparado {
    pub fn texto_insuficiente(&self) -> bool; // paginas_enviadas > 0 && !termo_do_tipo && caracteres_uteis < 40
    pub fn sem_conteudo(&self) -> bool;       // paginas_enviadas == 0 || texto_insuficiente()
}
pub enum Camada { Rotulo, NomeConhecido, Tratamento, Padrao, Data } // "rotulo", "nome_conhecido", "tratamento", "padrao", "data"
```

### 5.4 Estado enviado: duas serializações

```rust
pub struct EstadoEnviado { pub arquivo: Arquivo, pub paginas_modo: PaginasModo, pub paginas: Vec<PaginaEnviada> }
pub struct Arquivo { pub termos_nome_arquivo: Vec<String>, pub termos_titulo: Vec<String>, pub extensao: String, pub total_paginas: u32 }
pub struct PaginaEnviada { pub n: u32, pub texto: String }
```

O mesmo estado tem **duas** serializações, e nenhuma pode depender de `#[derive(Serialize)]` nem da feature `preserve_order` do `serde_json`. O `Cargo.toml` liga `preserve_order` nas dev-dependencies, então um teste passaria com uma ordem que o binário não produz.

| Uso | Ordem das chaves | Formato |
|---|---|---|
| **Digest** (`json_canonico`, §7.4) | Ordenada por code point: raiz `arquivo, paginas, paginas_modo`; `arquivo` → `extensao, termos_nome_arquivo, termos_titulo, total_paginas`; página → `n, texto` (TD §11.1) | Separadores `,` e `:`, sem espaços, não-ASCII literal |
| **Fio** (`state` do Jev, §8.2) | A de construção do Python: `arquivo{termos_nome_arquivo, termos_titulo, extensao, total_paginas}`, `paginas_modo`, `paginas[{n, texto}]` (TD §6, passo 5) | Compacto, como o `pydantic_core.to_json` do SDK |

Cada serialização tem um teste de bytes exatos:
- **digest:** o estado vira um `serde_json::Value` e passa por `json_canonico` (§7.4), que ordena as chaves sozinho;
- **fio:** escrito à mão (`Serializer::serialize_map` na ordem de construção, ou escrevendo os bytes).

### 5.5 Resposta validada e resultado

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
    pub model: Option<String>,                     // o pedido; com chamada, o ecoado pela resposta (TD §9.1);
                                                   // None sem envio autorizado, quando RLCD_MODEL nem é lido (§12)
    pub adapter_version: &'static str,             // §8.1
    pub perguntas_versao: &'static str, pub mascaramento_versao: &'static str,
    pub limiar_aplicado: f64,
    pub provider_request_id: Option<String>,
    pub latencia_ms: u64,
    pub uso: Option<Uso>,
}
```

- `Preparado` e `EnvioNaoAutorizado` só existem neste porte e no CLI (o `preparado` vem do CLI do TestoDoc, TD §5.4). `motivo_codigo = envio_nao_autorizado` marca o `preparado` causado por R-P1, para distinguir de quem não pediu envio. Sem `--enviar`, `motivo_codigo` é `None`.
- A serialização das `probabilidades` segue a ordem de `Categoria::TODAS`, e o valor `None` vira `{}`.

### 5.6 Provedor

```rust
pub struct RequestNeutro<'a> {
    pub estado: &'a EstadoEnviado,
    pub criterio: &'static CriterioEscolha,  // id, pergunta, opções em ordem
    pub perguntas_versao: &'static str,
    pub politica_versao: &'static str,       // MASCARAMENTO_VERSAO_PORTE (TD §9.2; DV10)
    pub alvo_versao: String,                 // digest.hex()[..16] (TD §9.2)
}
pub struct RespostaProvedor {
    pub resultado: Result<RespostaEscolha, ErroCategoria>,
    pub model: String, pub provider_request_id: Option<String>,
    pub latencia_ms: u64, pub uso: Option<Uso>, pub confianca_provedor: Option<f64>,
}
pub trait RlcdProvider: Send + Sync {
    fn provider(&self) -> &'static str;
    fn model(&self) -> &str;
    fn adapter_version(&self) -> &'static str;
    fn evaluate(&self, req: &RequestNeutro<'_>) -> impl Future<Output = RespostaProvedor> + Send;
}
```

`evaluate` nunca devolve `Err`: toda falha vira `RespostaProvedor` com `ErroCategoria`, como no Python ("nenhuma sobe para a política", `typesafe.py`, docstring). O `request_id` uuid4 de TD §9.2 não é enviado ao Jev nem gravado, então não existe aqui.

O prazo e o semáforo do §8.3 ficam num `Avaliador { semaforo, prazo }`, separado do provedor, e `Avaliador::avaliar(&P, &RequestNeutro)` é o `avaliar` de TP. Ele mede `latencia_ms` do início da espera pela vaga até o fim, em todos os casos, como o `evaluate` do Python mede antes do `async with self._semaforo`. A autorização chega a `classificar(prep, limiar, envio)` como `enum Envio { NaoPedido, NaoAutorizado, Autorizado { provedor, avaliador } }`: só a variante autorizada tem como chamar o provedor.

## 6. Núcleo N1: texto, Unicode e regex

As seções 6 e 7 dizem **como** reproduzir em Rust o que TD §6–§8 e §10 especificam. O algoritmo em si está em TD e não é repetido aqui. A referência de cada função é a seção de TD citada, e o critério de aceite é TV.

### 6.1 `compor` (TD §6, passo 0)

`NFC(t)` e depois a troca, por `\n`, de cada ocorrência de `\r\n|[\r\x0b\x0c\x1c\x1d\x1e\x85  ]`, nessa ordem. Não inclui `\x1f` (TD §18.1, `str.splitlines()`). Vetores: TV `compor`.

**Quem compõe** é exatamente quem compõe em TP (grep `compor(` em `mascaramento.py` e `classificador_documento.py`, no commit fixado):
- `remover_dados_pessoais`, sobre o texto;
- `conferir`, sobre o texto, antes da camada 2;
- `nomes_dos_rotulos`, sobre o texto;
- `partes_de_nome`, sobre o nome;
- `_padroes_conhecidos`, sobre cada conhecido;
- `termos_permitidos`, como `dobrar(compor(texto))`;
- `montar_estado`, sobre cada `pagina.texto`, no início.

**Não compõem:** `dobrar` (senão `"á"`, com 2 code points, viraria `"a"`, quebrando o invariante do §6.2 e os casos NFD de TV `dobrar`), `extract_patient_hints`, `_nomes_dos_indicios`, `_caracteres_uteis` e o `extensao` de `montar_estado`. Esses recebem o que o chamador passar. Dentro de `montar_estado`, o texto das páginas já vem composto.

### 6.2 `dobrar` e offsets (TD §7.3, "Primitivas")

- Para cada `char ch`: `base = NFD(ch)` (decomposição **canônica**, nunca NFKD), primeiro code point; `baixa = lower_python(base)`; o resultado é `baixa` se ela tiver um único code point, senão `base`.
- `lower_python` é o `str.lower()` do Python 3.14 para um único caractere. Ele pode devolver mais de um code point (`İ` → `i̇`), e por isso existe a regra acima. A tabela de exceções multi-code-point vem do gerador (§6.3).
- **Invariante:** `dobrar(t).chars().count() == t.chars().count()`, testado por propriedade (proptest).
- **Offsets.** O Python indexa em code points. O `fancy-regex` devolve offsets em **bytes**, e `É` (2 bytes) dobra para `e` (1 byte). Regra: todo trecho `(inicio, fim)` de N1 é em **índices de code point**. `texto.rs` oferece um `MapaOffsets` (byte → code point) por string dobrada, e o `_substituir` trabalha sobre `Vec<char>` ou com o mapa. Os cortes de 6 000 e 60 000 e `caracteres_enviados` também são em code points (TD §6, passo 3).

### 6.3 Classes de caracteres à Python (TD §18.1)

As classes do motor de regex do Rust **não** servem. Verificado em 31/10/2026 com `regex` 1.13.1: `^\w+$` casa `"a\u{301}"` e `"a\u{203f}"`, e no Python 3.14 nenhum dos dois é `\w` (TD §18.1).

- `scripts/gen-pyclasses.py` gera `src/classifier/pyclasses.rs`. São listas de faixas de code points para:
  - as classes de regex `\d`, `\w`, `[^\W\d_]`, `[^\W_]` e `\s`;
  - os predicados de caractere `isupper`, `islower`, `isdigit`, `isdecimal` e `isspace`, o de titlecase e `iscased`:
    - **titlecase** é a categoria `Lt` (`unicodedata.category(ch) == "Lt"`), a flag que o `s.isupper()` de string testa no CPython. Não serve o `str.istitle()` de um caractere, que também é verdadeiro para maiúsculas (`"A".istitle()`); a Fase 1 achou isso pelo TDD;
    - **`iscased`** é a propriedade Unicode Cased, que o CPython usa em `_PyUnicode_IsCased`; para um caractere, `ch.isupper() or ch.islower() or category == "Lt"`;
  - os mapeamentos completos `lower()`, `upper()` e `title()` de cada caractere em que o resultado difere da entrada, inclusive os de mais de um code point (`ß` → `SS`, `İ` → `i̇`, `ǆ` → `ǅ`);
  - **Case_Ignorable**, para o sigma final: o `lower()` e o ramo minúsculo do `title()` do CPython trocam `Σ` por `ς` quando, pulando os code points Case_Ignorable, há um code point com caixa antes e nenhum depois (`handle_capital_sigma`; `"ΟΣ".lower() == "ος"`). O `unicodedata` não expõe a propriedade, e o gerador a lê do próprio `str.lower()`. O `dobrar` não usa o contexto: dobra cada code point sozinho, como TP (achado C1-1 do Codex);
  - o conjunto de code points atribuídos: só no caminho com guarda do §4.2. Não é gerado, porque o pin na 0.1.24 deu certo (Fase 0).
- O gerador roda com o **Python 3.14** e **aborta** se `unicodedata.unidata_version != "16.0.0"`. Nesta máquina, `python3` é o 3.15.2, com UCD 17.0.0; o 3.14.10 (`/opt/homebrew/bin/python3.14`) tem a 16.0.0.
- O cabeçalho do arquivo gerado registra a versão do Python, a UCD e o sha256 do gerador. Um teste confere que o arquivo gerado bate com o gerador (como `test_vetores_classificador` faz no TestoDoc).
- Cada regex portada usa essas faixas como classes explícitas `[...]`, e `\b` vira `(?:(?<=W)(?!W)|(?<!W)(?=W))`, com `W` a classe `\w` do Python. O `fancy-regex` aceita lookbehind de largura fixa.

### 6.4 Semântica do `re` do Python que o porte precisa reproduzir

| Ponto | Python | Em Rust | Evidência |
|---|---|---|---|
| `pattern.search(s, pos)` / `.match(s, pos)` | `pos` não fatia: o lookbehind enxerga antes de `pos`, e `^` com MULTILINE só casa em `pos` se houver início de linha ali | `fancy_regex::Regex::find_from_pos(s, pos)` (offset em bytes), sem fatiar; para `match`, conferir que o achado começa em `pos` | TD §18.1; verificado: `(?<=b)a` com `find_from_pos("ba", 1)` dá `Some(1)` |
| `search(s, pos, endpos)` com `$` sem MULTILINE | `$` casa em `endpos` **e** antes de um `\n` em `endpos − 1` | fatiar até `endpos` e trocar o `$` final por `\n?\z` em `_ANTES_DE_FAIXA` e `_ANTES_DE_CONTAGEM` | TD §7.3, camada 4; TD §18.1 |
| `IGNORECASE` com padrão Unicode | as letras ASCII casam também 4 letras não ASCII: `İ` (U+0130) e `ı` (U+0131) com `i`, `ſ` (U+017F) com `s`, `K` (U+212A) com `k`. Vale para literais e para faixas de classe (`[A-Z]`, `[A-Za-z]`) | `(?i)` do Rust casa `ſ` e `K`, mas **não** `İ` nem `ı`. Em toda região case-insensitive: o literal `i`/`I` vira `[iIİı]`, e toda classe que contém `i` ou `I`, inclusive por faixa, ganha `İı`. Vale para a regex 1 de `patient_hints` (`[A-ZÁÉ…]`, `[A-Za-z…]`), `_REGISTRO_RE` (`[A-Za-z]`) e a busca dinâmica de `_nomes_dos_indicios` | Verificado em 31/10/2026: Python 3.14 `re.match('(?i)paciente', 'PACİENTE')` é verdadeiro e `'pacıente'` também; `regex` 1.13.1 dá falso para os dois. Faixas: AG-4 (Antigravity) |
| Alternação | leftmost-first, na ordem do texto da regex | igual no `fancy-regex`; copiar a ordem de TV `regexes` | TD §18.1 (A8) |
| `str.split()`, `strip()`, `isspace` | conjunto `isspace` do Python (inclui U+001C a U+001F) | usar `pyclasses::is_space`, nunca `char::is_whitespace` (outro conjunto) | TD §18.1 |
| `ch.isupper()` de **um** caractere | `palavra[:1].isupper()`: só a inicial, em `_parece_nome` (`mascaramento.py:284`) e em `_fim_do_nome` (`:384`). `"João"[:1].isupper()` é verdadeiro | `pyclasses::is_upper(primeiro char)`. **Nunca** o predicado de string, que recusaria `"João"` e deixaria o nome passar | TP, linhas citadas |
| `s.isupper()` de **string** | verdadeiro se há ao menos um caractere `isupper` e nenhum `iscased` que não seja `isupper`; espaços e pontuação não contam. Só em `extract_patient_hints` (`nome.isupper()`, `patient_hints.py:29`) | `str_isupper_python(s)` com `is_upper` e `is_cased` | TP |
| `s.title()` | troca para `title(ch)` depois de um caractere não `iscased` e para `lower(ch)` depois de um `iscased` (CPython `do_title`); usado em `extract_patient_hints` quando `nome.isupper()` | `title_python` com `is_cased` e os mapeamentos completos `title()` e `lower()` gerados | TP `patient_hints.py:29` |
| `s.upper()` | mapeamento completo, que pode mudar o tamanho (`ß` → `SS`); usado em `extract_patient_hints` para comparar com `{NOME, PACIENTE, DT NASC, SOLICITANTE}` | `upper_python` com o mapeamento `upper()` gerado | TP `patient_hints.py:28` |

**Inventário das regexes de N1** (TP no commit fixado; a Fase 2 o confere de novo com `grep -n "re\.\(compile\|search\|match\|sub\|findall\|finditer\|split\)"` nos três arquivos). Cada uma é traduzida **à mão** para `pyre.rs`, junto de uma constante com o **texto Python original** e as **flags**.

| Grupo | Regexes | Flags (TP) | Texto conferido contra |
|---|---|---|---|
| TV `regexes` | `_ROTULO_RE`, `_TRATAMENTO_RE`, `_REGISTRO_RE`, `_IDENTIFICADOR_ROTULADO`, `_ANTES_DE_FAIXA`, `_ANTES_DE_CONTAGEM`, `_DEPOIS_DE_CONTAGEM`, `_DATAS_NUMERICAS`, `_DATAS_EXTENSO`, `_SEQUENCIA_DE_MARCADORES`: 10 regexes | `MULTILINE` em `_ROTULO_RE`; `IGNORECASE` em `_REGISTRO_RE`, `_IDENTIFICADOR_ROTULADO` e `_SEQUENCIA_DE_MARCADORES`; flags embutidas `(?i:…)` onde o texto as tiver | TV `regexes` |
| `_PADROES` (TV `regexes._PADROES`) | 12 regexes, na ordem: e-mail, URL, CNPJ, CPF, CPF com espaços, CNS, RG, CEP, telefone com DDD entre parênteses, telefone com DDD, telefone sem DDD (`_nao_e_faixa`), `\b\d{6,}\b` (`_nao_e_contagem`) | `IGNORECASE` na URL e nos dois telefones que terminam com `_UNIDADE_DEPOIS` (`mascaramento.py:446,457,458`) | TV `regexes._PADROES` |
| `patient_hints` | regex 1 (paciente/nome) e regex 2 (GE/Philips), TD §7.1; mais `\s+` na normalização do grupo | regex 1: `IGNORECASE`; regex 2: nenhuma | `patient_hints.py:15–26` |
| Auxiliares fixas | `_QUEBRAS_DE_LINHA` (`:58`), `_FIM_DE_CELULA` (`:109`), `_TRATAMENTO_NO_INICIO` (`:272`), `_PALAVRA_DE_NOME` (`:273`), `_PARTICULA_DE_NOME` (`:364`), `_MARCADORES_RE` (`classificador_documento.py:214`), `[ \t]*\([^)]*\)` (`_rotulo_de_linha`), `[^\W\d_]+` (`partes_de_nome`), `\S+` (`_fim_do_nome`), `[a-z0-9]+` (`termos_permitidos`), `[^\W_]+` (`_caracteres_uteis`) | as do texto | TP, linhas citadas |
| Dinâmicas | `_padroes_conhecidos`: `\b(?:p1\|p2\|…)\b` e um `re.escape(literal)` por literal (`:342,344`); `_nomes_dos_indicios`: `\s+`.join(`re.escape(p)`) com `IGNORECASE` (`classificador_documento.py:239`) | `IGNORECASE` só na de `_nomes_dos_indicios` | testes de TV `remover_dados_pessoais` e `nomes_dos_indicios` |

- **Escape das dinâmicas:** `re.escape` do Python vira um escape equivalente no `fancy-regex`, e `\s` vira a classe `isspace` gerada.
- **Texto das traduções:** um teste confere cada constante de texto Python com TV `regexes`. As que não estão em TV são conferidas com o arquivo TP fixado, por um teste opcional que lê o checkout do TestoDoc (`TESTODOC_DIR`). Quando os vetores mudarem, o teste aponta a regex que precisa de nova tradução.
- **Flags:** TV guarda só o texto das regexes. As flags acima são conferidas pelos casos de TV e pelos testes do §14.4.

### 6.5 Mascaramento (TD §7.3 e §8)

Implementar exatamente TD §7.3: passo 0, camadas 1 a 5, colapso, `_substituir` (ordem `(início, fim, texto)`, descarte de sobreposições, o mais curto vence no mesmo início), contagens **antes** do descarte, camadas 2 e 4 **em sequência** sobre o texto já substituído, ordem total `(−tamanho, texto)` para partes, literais e rótulos simples. A `conferir` segue TD §8.

Pontos de atenção para o Rust:
- os conjuntos de rótulos (TD §7.3, "Conjuntos exatos") e as listas (`VOCABULARIO_TIPO`, `TERMOS_CLINICOS`, `PARTICULAS`, `PALAVRAS_DE_ROTULO`, `SEXOS`, `NAO_E_NOME`) são **copiados de TV `listas`** por um teste de igualdade, não redigitados;
- `_valores_em_colunas` não é janela deslizante (TD §7.3, "Avanço do laço");
- **dois detalhes de TP que o resumo de TD §7.3 não diz** (vale TP, §0.2):
  - `_rotulo_composto` (`mascaramento.py:188`), usado nas tabelas por colunas e em `_e_pessoal`/`_e_de_nome`, só exige de 2 a 3 palavras com a primeira em `_ROTULOS_COMPOSTOS`, **sem** exigir `[a-z]+` nas demais (`"telefone 1"` numa coluna vale). A exigência de `[a-z]+` existe só em `_ROTULO_RE`, para rótulos em linha;
  - `_camada_rotulos` (`:262–269`) junta `[*em_colunas (só pessoais), *em_linha]` e entrega tudo a `_substituir`. Já `nomes_dos_rotulos` (`:304`) junta `[*em_linha, *em_colunas (todos)]`, ordena **de forma estável só pelo início** e testa `_parece_nome` em cada valor de rótulo de nome, **sem** descartar sobreposições. As duas ordens são reproduzidas como estão;
- `_valor_mascarado` devolve `valor.strip()` quando nada mudou e normaliza espaços internos (TD §7.3);
- o marcador nunca carrega parte do valor (invariante 5).

### 6.6 Nomes derivados (TD §7.1, §7.2)

`extract_patient_hints` só precisa do campo `nome`. As duas regexes de TD §7.1 têm flags diferentes: a regex 1 é `IGNORECASE` **inteira**, inclusive a classe `[A-ZÁÉÍÓÚÂÊÔÃÕÇ]`, e a regex 2 (GE/Philips) não tem `IGNORECASE`. Vetores: TV `nomes_dos_indicios`, `nomes_dos_rotulos`.

### 6.7 `montar_estado` (TD §6)

Seguir TD §6, passos 0 a 10, incluindo:
- a seleção de páginas **antes** de descartar as vazias (A2);
- `conhecidos` tirados do texto de **todas** as páginas;
- por página, a ordem **remover → conferir a página inteira já mascarada → cortar**, com `_sem_marcador_partido`;
- `truncado` só por corte ou por página não vazia sem espaço;
- a conferência string a string das demais strings do estado (`arquivo.*`, `paginas_modo`), nunca do JSON serializado (A9);
- `caracteres_uteis` e `termo_do_tipo` com `TERMOS_GENERICOS = {laudo, exame, relatorio}`.

Vetores: TV `montar_estado` (21 casos: estado, JSON canônico, digest, contagens, `truncado`, bloqueio, texto útil, `sem_conteudo`).

## 7. Núcleo N1: JSON, digest e decisão

### 7.1 Validação da resposta (TD §9.5; TP `validar_respostas`)

`decisao::validar_respostas(brutas) -> Option<RespostaEscolha>` reproduz TP `rlcd_client.validar_respostas` restrito ao critério `Choice` do classificador. A entrada é o **formato neutro** de TP, o mesmo de TV `validar_respostas`, e não o fio do Jev:

```
brutas: { <criterio_id>: <bruto> }   onde <bruto> é
  { "escolha": <qualquer JSON>, "probabilidades": <qualquer JSON, ou ausente/null> }  ou  <string escolha>
```

Na ordem:
1. as chaves de `brutas` são exatamente `{tipo_documento}`;
2. `escolha` é uma string entre as nove opções;
3. se `probabilidades` é `null`/ausente, ou o bruto é uma string, as probabilidades viram one-hot na escolha (`rlcd_client.py:224–225`). Isso só acontece com o dublê: o adaptador do Jev sempre entrega um mapa (§8.4);
4. `probabilidades` é um objeto e só tem chaves entre as opções; as ausentes valem 0,0;
5. cada valor, convertido como o `_finito` de TP (`float(valor)`: número JSON, ou string numérica, ou `true`/`false` como 1,0/0,0), é finito e está em [0, 1];
6. `|Σ − 1| ≤ 1e-3` (`TOLERANCIA_SOMA_PROBABILIDADES`), com a soma feita **na ordem de `Categoria::TODAS`**, a mesma de TP (`for op in criterio.opcoes`), para que o arredondamento seja idêntico.

Falhou alguma: `None`, e o resultado é `ErroCategoria::RespostaInvalida`. Vetores: TV `validar_respostas` (12). A conversão de `_finito` só existe aqui, para a conformidade; o fio do Jev é decodificado estritamente antes (§8.4, DV3).

### 7.2 Decisão (TD §10, normativo)

```
confianca = probabilidades[escolha]
ordenadas = probabilidades em ordem decrescente (total_cmp)
margem    = max(0.0, ordenadas[0] − ordenadas[1])
estado    = Ok se confianca + 1e-9 >= limiar, senão Incerto
```

`confianca` é a probabilidade **da escolha**, que pode não ser a maior. O `confidence` do Jev (TS-API, `ChoiceAnswer.confidence`) é outra medida: um resumo da forma da distribuição, que dá 1 quando toda a probabilidade está numa opção e 0 quando está espalhada por igual (`https://docs.typesafe.ai/confidence.md`). Ele é guardado como `confianca_provedor` (DV7) e não entra na decisão. Vetores: TV `decidir_classificacao` (7), comparados com tolerância de 1e-12 (TD §17.2).

### 7.3 Limiar

`CLASSIFIER_CONFIANCA_MIN`, padrão `0.70`, aceito só se for finito e estiver em [0, 1] (TD §16.1). `NaN` é recusado com o código 5.

### 7.4 `json_canonico` e digest (TD §11.1)

Equivale a `json.dumps(x, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`:
- chaves ordenadas por code point, sem espaços, não-ASCII literal em UTF-8;
- escapes `\"`, `\\`, `\n`, `\r`, `\t`, `\b`, `\f`, e `\u00xx` em hex minúsculo para os demais controles abaixo de 0x20;
- U+007F, U+2028 e U+2029 sem escape;
- inteiros em decimal; o estado não tem floats.

Depois, `sha256` dos bytes UTF-8. O escape de strings do `serde_json::to_string` coincide com o do Python (verificado: `"\u{1f}\u{7f}\u{2028}"` vira `"\u001f` + U+007F + U+2028 literais), então pode ser usado para **cada string**.

`canonico::json_canonico(v: &serde_json::Value) -> String` serve para **qualquer** `Value`, como em TP e TV `json_canonico`, onde os casos são objetos arbitrários. Ela percorre o valor recursivamente e, em cada `Value::Object`, coleta as chaves e as **ordena explicitamente** por code point (`str::cmp` em Rust ordena por bytes UTF-8, o que dá a mesma ordem). Assim a ordem não depende de `preserve_order`. Números inteiros saem em decimal. Um float sai como o `repr` do Python, que é o que o `json.dumps` escreve: os dígitos mais curtos que voltam ao mesmo float, com empate para o dígito par; notação científica com sinal e ao menos dois dígitos no expoente quando o expoente decimal fica abaixo de −4 ou acima de 16 (`1e-05`, `1e+16`); senão posicional, com `.0` quando não há parte fracionária (`1.0`, `-0.0`). O `serde_json` escreve outro formato (`1e-9`, `1e16`). O estado não tem floats, e TV `json_canonico` também não; um diferencial opcional compara com o Python (achado C1-2 do Codex). O digest do estado é `sha256(json_canonico(&estado_como_value))`.

**Não reusar `ids::canonical_json`.** Ele normaliza toda string e toda chave para NFC (`src/ids.rs`, `write_string`), e o Python não normaliza. `extensao` passa só por `dobrar`, sem `compor` (TD §6, passo 5), e pode ter uma marca combinante solta, que o NFC juntaria à letra anterior, mudando o digest. Vetores: TV `json_canonico` (3) e os digests de TV `montar_estado`.

## 8. Adaptador do Jev (N2, feature `classifier-jev`)

### 8.1 Identidade

- `provider = "typesafe"`.
- `adapter_version = "cognitionlab-jev-http/1"`: próprio do porte e diferente de `typesafe-sdk-0.7.4/1` (TD Q5). Sobe para `/2` em qualquer mudança no fio ou no tratamento da resposta.
- `model`: `RLCD_MODEL`, padrão `jev-1.13.0`. Precisa casar `^jev-\d+\.\d+\.\d+$` (TD §16.1, `modelo_rlcd_fixo`), testado depois de `strip()`; o valor enviado é o configurado. Um alias (`jev-latest`, `jev-preview`) é recusado com o código 5, porque um alias muda sem aviso e invalida a calibração (TS-MOD, "Aliases"; TP `typesafe.py`).

### 8.2 Request

- `POST {base}/v1/systemone`, com `base = RLCD_BASE_URL` ou o padrão `https://api.typesafe.ai`. A URL é feita por concatenação de strings, depois de tirar uma `/` final de `base`, nunca por `Url::join` (TD §9.3).
- Headers: `Authorization: Bearer <chave>`, `Content-Type: application/json`, `Accept: application/json`, e `User-Agent: cognitionlab-classifier/<versão do crate>`. O SDK também manda headers próprios de SDK e runtime (`transport.py`), que este porte não imita. **Não** manda `TypeSafe-Danger-Zone-Long-Context` nem `Content-Encoding`.
- Corpo, compacto, UTF-8 sem BOM, nesta ordem de chaves, que é a do SDK (`endpoints.py`, `prepare_system_one`):

  ```json
  {"state":<estado no formato de fio, §5.4>,
   "model":"<RLCD_MODEL>",
   "questions":{"tipo_documento":{"type":"choice","instructions":"<PERGUNTA>",
                                  "criteria":{"vop":"<descrição>", …, "outro":"<descrição>"}}}}
  ```

- **A ordem de `criteria` é a de `Categoria::TODAS`** e é obrigatória: o Jev 1.13 tende para a primeira opção (TS-JAG #8), e a calibração do TestoDoc usou essa ordem. Um teste confere os **bytes** do corpo com a feature `preserve_order` ligada e desligada.
- O `state` é um objeto JSON (TS-API aceita string, objeto ou array). Não é a string do JSON canônico, que só a OpenAI recebia (TD §9.3).
- Limites: o corpo fica muito abaixo de 32 MiB (TS-ERR). O estado tem no máximo 60 000 code points de texto, cerca de 17 000 tokens (TD-PRD §6.6), abaixo de 32k tokens por pergunta e de 64k por request (TS-MOD). O porte não conta tokens. Um `400` com `max_tokens_exceeded` cai em `resposta_invalida`.

### 8.3 Timeout, concorrência e cancelamento (TD §9.4)

- `avaliar = tokio::time::timeout(RLCD_TIMEOUT_S, async { adquirir o semáforo; fazer o POST; ler o corpo })`. A espera pelo semáforo conta dentro do timeout, como no Python (TD §9.3). Padrão: 20 s.
- Semáforo de `RLCD_MAX_CONCURRENCY` vagas (padrão 4). O CLI usa 1 (TD §9.3).
- O timeout total é o do `tokio`. O cliente `reqwest` não tem timeout próprio (nem `timeout` nem `connect_timeout`), para que só um relógio decida. Mesmo assim, um `reqwest::Error` com `is_timeout()` é mapeado para `timeout` (§8.5).
- Cancelamento: um future do Rust cancelado é descartado. Nenhum código engole o descarte, e não há `catch_unwind`. Isso equivale a "`CancelledError` propaga" (TD §15).
- **Sem retry.** TS-ERR recomenda backoff exponencial em `429`/`529`, mas o classificador Python desliga o retry (`RetryPolicy(max_retries=0)`), e um erro só se repete na próxima execução (TD §15). O porte mantém isso.

### 8.4 Resposta 200

A decodificação tem duas etapas: o **envelope**, pelo esquema de TS-API e do SDK, e depois a **validação** do §7.1. No SDK 0.7.4, `SystemOneResponse` exige `model`, `answers` (pelo menos 1) e `usage`, e `ChoiceAnswer` exige `type`, `choice`, `confidence` e `probabilities` (`_schemas/models.py`).

1. **Envelope**, com `serde`, sem `deny_unknown_fields` (o serviço é externo e pode acrescentar campos):
   ```rust
   struct RespostaFio { model: String, answers: BTreeMap<String, serde_json::Value>, usage: UsoFio }
   struct UsoFio { input_tokens: u64, output_tokens: u64 }
   ```
   `answers` é um **mapa**, não uma struct, para que uma resposta extra chegue à regra 1 do §7.1 em vez de ser descartada pelo `serde`. `usage` e os dois campos dele são obrigatórios, como no SDK: ausente, a resposta é inválida. (TD §9.3 diz "ausente, vira `None`", mas isso descreve o `getattr` do adaptador, que o pydantic do SDK nunca deixa chegar sem `usage`.)
2. **Conversão para o formato neutro** (`jev.rs`): para cada `(id, valor)` de `answers`:
   - se `valor` é um objeto com `type == "choice"`, `choice` string, `confidence` número e `probabilities` objeto de números, o bruto é `{"escolha": choice, "probabilidades": probabilities}` e `confianca_provedor = confidence`;
   - qualquer outra forma (`type` de outra primitiva, `choice` não string, ou `probabilities` ausente, `null` ou com valor não numérico) vira o bruto `{"escolha": null, "probabilidades": {}}`, que a regra 2 do §7.1 recusa. **Nunca** vira one-hot.
3. **Validação** (§7.1) sobre o formato neutro.

| Situação | Resultado | Metadados guardados |
|---|---|---|
| Corpo que não é JSON, ou envelope fora do esquema (`model` ausente, `answers` vazio ou ausente, `usage` ausente, tipo errado) | `resposta_invalida` (TD Q8, recomendação; DV3) | `latencia_ms`, `provider_request_id` do header (DV8); `model` = o pedido; `uso` = `None` |
| Envelope ok, conversão ou validação falha (resposta extra, `tipo_documento` ausente, `type` diferente de `choice`, escolha fora das opções, soma errada) | `resposta_invalida` | `model` ecoado, `uso`, `latencia_ms`, `provider_request_id` (TP `montar_resposta`) |
| Envelope ok e validação ok | `Ok(RespostaEscolha)` | todos, mais `confianca_provedor` |
| Header `x-typesafe-request-id` | `provider_request_id`. Ausente: `None`, **sem** erro (DV2) | — |

### 8.5 Erros de transporte

Testado nesta ordem; a primeira verdadeira decide. É a mesma ordem do SDK, que testa `TimeoutException` antes de `RequestError` (`transport.py`).

| # | Situação | Categoria |
|---|---|---|
| 1 | O `tokio::time::timeout` estourou, em qualquer fase (fila do semáforo, conexão, envio, leitura), ou um `reqwest::Error` tem `is_timeout()` | `timeout` |
| 2 | Falha de DNS, conexão, TLS ou leitura do corpo (`reqwest::Error` com `is_connect()`, `is_request()` ou `is_body()`; e qualquer erro de `bytes()` na leitura de um 200, que o `reqwest` marca como `is_decode()` quando o corpo vem cortado, embora ali nada seja decodificado) | `rede` |
| 3 | Qualquer outro `reqwest::Error` | `outro` |

### 8.6 Status HTTP (TD §9.3, TS-ERR)

| Status | Categoria |
|---|---|
| 401, 403 | `autenticacao` |
| 429 | `quota_provedor` |
| 400 (inclui `max_tokens_exceeded`), 422 | `resposta_invalida` |
| 404, 413, 415, 5xx (inclui 529 `Overloaded`), qualquer outro não 2xx | `outro` |
| 2xx diferente de 200 | `outro` (TS-API só define 200) |

O corpo do erro **não** é registrado, porque pode ecoar o `state` (TS-API, `ValidationError.input`, "Text preview of the rejected input"). Só o status e a categoria vão para o log.

## 9. `classificar` (TD §9.1)

### 9.1 Ordem de decisão

```
prep = montar_estado(entrada, modo)
1. prep.sem_conteudo()      → SemConteudo, motivo TextoInsuficiente se prep.texto_insuficiente() senão SemConteudo; sem chamada
2. !prep.bloqueio.is_empty() → BloqueadoPrivacidade, motivo ConferenciaPrivacidade,
                               mascaramentos += {"conferencia_<camada>": n}; sem chamada
3. envio não autorizado (R-P1) → Preparado (motivo EnvioNaoAutorizado se --enviar foi pedido); sem chamada
4. senão                     → request (§9.2), avaliar (§8.3);
                               erro → Erro, erro_categoria, motivo ProvedorErro;
                               ok   → decidir (§7.2), tipo = escolha, probabilidades copiadas
```

O passo 3 ocupa o lugar do passo "dublê não permitido" de TD (DV5). Os passos 1 e 2 vêm **antes** da autorização, então um `preparado` sempre significa "seria enviado".

### 9.2 Invariante de minimização

O estado enviado só tem `arquivo.{termos_nome_arquivo, termos_titulo, extensao, total_paginas}`, `paginas_modo` e `paginas[].{n, texto}` (TD §17.1, invariante 3). Os tipos do §5.4 garantem isso por construção, e um teste confere que nenhuma chave do estado está em `CHAVES_PROIBIDAS_NO_ESTADO` (copiada de TP `rlcd_client.py`).

## 10. CLI `classify`

### 10.1 Uso

```
embeddings-fundamentals classify <arquivo> [--paginas 1|2|todas] [--json] [--enviar] [--limiar X]
                                 [--titulo T] [--nome-paciente N]... [--mostrar-estado]
```

O padrão de `--paginas` é `CLASSIFIER_PAGINAS`, ou `"2"` (TD §16.1). Há uma linha JSON com `--json`; sem ele, um texto legível com os mesmos campos e nenhum texto do exame.

`--mostrar-estado` acrescenta `estado_preparado` (o estado no formato de fio) **só** quando `estado ∈ {preparado, ok, incerto, erro}`, `prep.bloqueio` está vazio e `!prep.sem_conteudo()` (TP `classificar_documento.py:369`). Em `bloqueado_privacidade` o estado contém justamente o dado pessoal que a conferência achou, então a chave fica **ausente** do JSON e o modo legível não o imprime. A opção existe só para entradas sintéticas (R-P4).

**Ordem de execução** (a de TP `_executar`, adaptada):
1. validar `--paginas`/`CLASSIFIER_PAGINAS` e o limiar (código 5, `configuracao_invalida`);
2. o arquivo existe (código 2, `arquivo_inexistente`) e a extensão é suportada (código 2, `formato_nao_suportado`);
3. se o envio está autorizado (R-P1: `--enviar` **e** `CLASSIFIER_REMOTE_ALLOWED=1`): validar `RLCD_MODEL` (código 5, `modelo_invalido`), a chave (código 5, `sem_chave`), `RLCD_BASE_URL`, `RLCD_TIMEOUT_S` e `RLCD_MAX_CONCURRENCY` (código 5, `configuracao_invalida`), e construir o cliente. Sem autorização, **nenhuma** dessas variáveis é lida nem validada;
4. conferir as ferramentas: `pdfinfo`, `pdftotext` e `pdfimages` para PDF; `tesseract` com o idioma `por` para imagem. Num PDF, o `tesseract` é conferido na primeira página que precisar de OCR, antes de chamá-lo. Faltando qualquer um, código 5, `ocr_indisponivel`;
5. extrair (§10.2). Uma falha de leitura do arquivo dá código 2 (`arquivo_ilegivel` ou `limite_pdf`);
6. `montar_estado` e `classificar` (§9.1).

Qualquer outro erro não previsto: código 1, `erro_interno`.

### 10.2 Entrada

| Formato | Páginas |
|---|---|
| `.pdf` | Até 200 páginas (`MAX_PDF_PAGES` de `pdf_extract.py`); acima disso, código 2 (`limite_pdf`). O texto de cada página é `PageText::text()` do `TextExtractor` do crate (poppler `pdftotext`), sem nenhum `strip()` adicional, com origem `nativo`. Nas páginas selecionadas pelo modo, com `nativo = strip_python(texto)`, roda o OCR (`OcrEngine::ocr_page`) quando `nativo` é vazio, ou quando a página é visual (§10.4) e `nativo` tem menos de 220 code points (`TEXT_SPARSE_CHARS`). Se o OCR devolver texto com `strip_python()` não vazio, a página vira `"\n".join(t for t in (nativo, strip_python(ocr)) if t)`, com origem `ocr_local`. Um erro de OCR numa página deixa a página como estava (TP: `ValueError` → `continue`). |
| `.png`, `.jpg`, `.jpeg` | Primeiro a imagem é decodificada com o crate `image`, já dependência; se falhar, código 2 (`arquivo_ilegivel`). Depois, uma página `n = 1` com o texto do OCR da imagem, origem `ocr_local` (TD §5.2, ramo "outra extensão"). Sem texto, nenhuma página, e o resultado é `sem_conteudo` (código 3). É o formato de uma cópia anonimizada de imagem. |
| outro | Código 2 (`formato_nao_suportado`). Diferente do TestoDoc, não há conversão de DOC/DOCX/ODT pelo LibreOffice. |

**OCR.**
- **Configuração:** a extração do classificador usa um `ExtractionConfig` próprio, com `ocr_lang = "por"`, o idioma do TestoDoc (`ocr_local.IDIOMA`). Os demais campos (`ocr_dpi`, `ocr_psm`) ficam no padrão do crate.
- **Texto:** é o `PageText::text()` do crate, cujas linhas já saem sem espaços à direita (`poppler.rs`). A diferença em relação ao `pytesseract.image_to_string` do TestoDoc entra em DV4.
- **Imagem avulsa:** o trait `OcrEngine` só lê páginas de PDF (`ocr_page(pdf, page)`). A Fase 5 acrescenta ao motor tesseract do crate um método `ocr_image(&self, image: &Path) -> Result<PageText, ExtractionError>`, que chama o tesseract direto sobre a imagem já validada. Ele fica fora do trait, para não obrigar o `FixtureExtractor` a implementá-lo; o dublê de teste do CLI tem o seu.

- `nome_arquivo` é o nome do arquivo com a extensão; `titulo` é `--titulo` ou o nome sem a extensão; `extensao` é o sufixo em minúsculas, sem o ponto (TD §5.3). `total_paginas` é `len(paginas)` (TD §5.2). `nomes_conhecidos` são os `--nome-paciente`.
- A ferramenta ausente é conferida **antes** da extração (§10.1, passo 4), e por isso um `ExtractionError` durante a extração é sempre tratado como arquivo ilegível (código 2). Os dois casos não se confundem.

### 10.3 Saída e códigos

Os códigos de saída são os de TD §5.4: 0 (`ok`, `incerto`, `preparado`), 1 (erro interno), 2 (entrada), 3 (`sem_conteudo`), 4 (`erro` do provedor), 5 (configuração: chave, modelo, OCR, modo ou limiar inválidos), 6 (`bloqueado_privacidade`). Uma falta de autorização **não** é erro: dá `preparado` e o código 0.

Com `--json` sai exatamente uma linha em stdout, também nos erros, com as chaves:

```
estado, codigo_saida, envio_realizado, provider, model, adapter_version, perguntas_versao, mascaramento_versao,
origens_texto, paginas_modo, total_paginas, paginas_enviadas, caracteres_enviados, truncado, mascaramentos,
termos_nome_arquivo, entrada_digest, tipo, rotulo, confianca, margem, limiar_aplicado, probabilidades,
confianca_provedor, erro_categoria, motivo_codigo, latencia_ms, provider_request_id, uso
```

- Diferenças em relação ao CLI do TestoDoc (TD §5.4): **sem** `arquivo` (R-P2: o nome do arquivo de um exame real pode ter o nome do paciente) e sem `simulado`, porque não há dublê no binário. **Com** as chaves de identidade, `rotulo`, `limiar_aplicado`, `confianca_provedor` e `uso`.
- **Envelope de erro antes da classificação** (como `_envelope` de TP):
  - quando o CLI para nos passos 1 a 5 do §10.1, a linha sai com `estado = "erro"`, `codigo_saida` do passo e `motivo_codigo` de `MotivoCli` em `snake_case`;
  - `envio_realizado = false`, `origens_texto = []`, `mascaramentos = {}`, `termos_nome_arquivo = []` e `probabilidades = {}`;
  - `null` em `paginas_modo` (salvo depois do passo 1), `total_paginas`, `paginas_enviadas`, `caracteres_enviados`, `truncado`, `entrada_digest`, `tipo`, `rotulo`, `confianca`, `margem`, `confianca_provedor`, `erro_categoria`, `latencia_ms`, `provider_request_id` e `uso`;
  - `provider = "typesafe"` e as chaves de versão (`adapter_version`, `perguntas_versao`, `mascaramento_versao`) sempre preenchidas; `model` preenchido só se o passo 3 rodou; `limiar_aplicado` preenchido depois do passo 1.

  A struct da linha (`LinhaCli`) é **separada** de `Classificacao`, com esses campos em `Option`. `Classificacao` continua sem `Option` nos campos que sempre existem depois de `montar_estado`.
- **Sem caminho e sem mensagem bruta.** O modo legível e o stderr dizem só o motivo (`arquivo_ilegivel`, `ocr_indisponivel`…) e uma frase fixa por motivo. Nunca o caminho do arquivo nem o texto de um `io::Error`, de um erro de ferramenta ou de um erro de parser, que podem conter o caminho ou trechos do documento. TP inclui `{caminho}` e `{exc}` nessas mensagens (`classificar_documento.py:147,313,374`); aqui isso é proibido (invariante 5). O mesmo vale para os eventos `tracing`.
- `envio_realizado` é verdadeiro se o estado for `ok` ou `incerto`, ou se `motivo_codigo == provedor_erro` (TD §5.4).
- O stdout é só da linha JSON. Diferente do PyMuPDF do Python, poppler e tesseract rodam como processos filhos com o stdout capturado, então não é preciso redirecionar o fd 1.

### 10.4 Página visual (divergência DV4)

O TestoDoc decide se uma página é visual pelo PyMuPDF (`pdf_extract._is_visual`): imagens que somam ≥ 2 % da área da página, ou desenho vetorial numa página com menos de 220 caracteres. Com poppler, o porte usa só a regra das imagens:
- `pdfimages -list -f N -l N <pdf>` lista as imagens da página N. Contam as linhas de `type` igual a `image` ou `stencil`. Segundo `man pdfimages`, `stencil` é "a monochrome mask image used for painting a color or pattern", ou seja, conteúdo pintado na página, como um scan de 1 bit. `mask` e `smask` "immediately follow the image" de que são a transparência, e somá-las contaria a mesma área duas vezes. A área de cada imagem é `(width / x-ppi × 72) × (height / y-ppi × 72)` em pontos²;
- a área da página vem de `pdfinfo -f N -l N <pdf>`, linha `Page N size: W x H pts`, como `W × H`;
- a página é visual se a soma das áreas for ≥ 2 % da área da página (`MIN_IMAGE_AREA_RATIO` de `pdf_extract.py`);
- falha de inspeção (ferramenta com erro, saída que não se interpreta, `x-ppi` ou `y-ppi` igual a 0, área da página 0) trata a página como **visual**, como TP: "é mais barato renderizar à toa do que perder conteúdo" (`pdf_extract._is_visual`).

**Não** detecta desenho vetorial. Por isso a entrada e o digest do porte podem diferir dos do CLI do TestoDoc para o mesmo PDF. A conformidade é medida em N1 (TV), não na extração.

## 11. Integração com o relatório do anonimizador (Fase 6)

### 11.1 Onde e quando

- `anonimizador batch` ganha a opção `--classifier <binário>` (o `embeddings-fundamentals` já compilado, com `classifier-jev`). Sem ela, o relatório sai **exatamente** como hoje: sem as colunas novas, sem a linha de cabeçalho e sem o diretório `classifier/`.
- Com ela, `start()` também cria `<run>/classifier/` e o inclui nas sondagens de `uncommittable`, como já faz com `run.copies` e `run.logs` (`batch.py`, `start`).
- Classifica **a cópia anonimizada**, não o original, e só quando a re-verificação da cópia deu `ok` (`anonymized_status`). Uma cópia que falhou na verificação nunca é classificada. Justificativa: defesa em profundidade, com o mascaramento do Jev por cima de uma cópia rasterizada, já sem texto, metadados nem os achados do `verify`.
- Chama `<binário> classify --json [--enviar] <cópia>` como processo filho, sem shell, com um timeout de `2 × (RLCD_TIMEOUT_S + 5) + 5` segundos, a mesma fórmula do teto da etapa de TD §12.2. `--enviar` só é passado se o próprio `batch` recebeu `--classify-remote`, e o processo filho ainda exige `CLASSIFIER_REMOTE_ALLOWED=1` (R-P1).
- **Uma falha do classificador nunca falha a anonimização** (análogo de TD §17.1, invariante 1). Timeout do processo filho, falha ao iniciá-lo, stdout que não é exatamente uma linha JSON válida ou código de saída fora de {0, 3, 4, 6} viram a célula `error: classifier failed` (§11.2). O código de saída do `batch` não muda.

### 11.2 O que entra no índice

O índice (`batch.index`) ganha as colunas **Exam type (classifier)** e **Weights**, depois de "Verify anonymized":

| Situação | Exam type (classifier) | Weights |
|---|---|---|
| `ok` | `<rotulo> (<confianca>)` | as nove alternativas, da maior para a menor: `Eco avançado 0.930 · Eco convencional 0.040 · … · Outro 0.004` |
| `incerto` | `uncertain: <rotulo> (<confianca>)` | igual |
| `preparado` | `not sent` | — |
| `sem_conteudo` | `no content` | — |
| `bloqueado_privacidade` | `blocked (privacy check)` | — |
| `erro` do provedor (código 4) | `error: <erro_categoria>` | — |
| `erro` do CLI (códigos 1, 2, 5) | `error: <motivo_codigo>` | — |
| falha do processo filho (§11.1) | `error: classifier failed` | — |
| cópia não classificada, porque a verificação falhou ou não há cópia | `—` | `—` |

- `<confianca>` e os pesos usam 3 casas decimais e ponto, porque o índice é em inglês, como o resto. Os empates nos pesos se desfazem pela ordem de `Categoria::TODAS`.
- **Cabeçalho:** ganha uma linha com `provider`, `model`, `adapter_version`, `perguntas_versao`, `mascaramento_versao`, limiar e modo de páginas, tirados da **primeira** linha JSON válida da execução, e diz se o envio foi pedido. Se nenhuma cópia produziu uma linha JSON válida, a linha diz `Classifier: no result` e se o envio foi pedido. O caminho do binário não aparece.
- **Linha JSON gravada:** a de cada cópia vai para `<run>/classifier/<stem da cópia>.json` (por exemplo `exam-<digest>.json`, o mesmo nome neutro da cópia, com `.json`), pelo `write_new` com a checagem de git do anonimizador. A célula liga para ela. Ela não tem texto (R-P2). Uma linha inválida não é gravada.

Os nomes das colunas e o formato dos pesos são decisões de apresentação deste documento. O dono pode mudá-los (Q-C2).

## 12. Configuração

Lida de variáveis de ambiente, com `.env` da raiz do crate via `dotenvy`, já dependência. Nenhum valor de chave é impresso.

| Variável | Padrão | Quando é lida | Validação (código 5, com o motivo indicado, se inválida) |
|---|---|---|---|
| `CLASSIFIER_PAGINAS` | `2` | sempre | ∈ {`1`, `2`, `todas`} (`configuracao_invalida`) |
| `CLASSIFIER_CONFIANCA_MIN` | `0.70` | sempre | finito, em [0, 1] (`configuracao_invalida`) |
| `CLASSIFIER_REMOTE_ALLOWED` | ausente | sempre | **nunca dá erro**: só o valor exato `1` autoriza (R-P1); qualquer outro valor, ou a ausência, vale como "não autorizado" e dá `preparado` |
| `RLCD_MODEL` | `jev-1.13.0` | só com envio autorizado | `^jev-\d+\.\d+\.\d+$` depois de `strip()` (`modelo_invalido`) |
| `RLCD_API_KEY`, `TYPESAFE_API_KEY` | — | só com envio autorizado | `RLCD_API_KEY` tem precedência, e `TYPESAFE_API_KEY` vale quando ela está vazia (TD §16.1). Ambas vazias: `sem_chave` |
| `RLCD_BASE_URL` | `https://api.typesafe.ai` | só com envio autorizado | vazia ou só espaços vale o padrão (TD §16.1 tem `""` como padrão). Senão, URL absoluta `https`, ou `http` com host `127.0.0.1` ou `localhost`, aceita **no binário**, porque a Fase 5 testa o CLI contra o servidor local (`configuracao_invalida`) |
| `RLCD_TIMEOUT_S` | `20.0` | só com envio autorizado | finito, > 0 (`configuracao_invalida`) |
| `RLCD_MAX_CONCURRENCY` | `4` (CLI: 1, fixo) | só com envio autorizado, fora do CLI | inteiro ≥ 1 (`configuracao_invalida`) |

## 13. Divergências deliberadas em relação ao Python

| # | Divergência | Por quê | Efeito |
|---|---|---|---|
| DV1 | `adapter_version` próprio (`cognitionlab-jev-http/1`) | TD Q5: não reaproveitar identidades do Python | Só identificação; não há banco aqui |
| DV2 | Resposta 200 sem `x-typesafe-request-id` dá `provider_request_id = None`, não erro | O A1 do Python vem de um acesso fora do `try` (TD §17.4) e "parece não intencional"; TS-API não documenta o header | Uma resposta válida continua válida. Confirmar (Q-C1) |
| DV3 | Números do fio decodificados estritamente (só número JSON) | TS-API declara `number`. O `_finito` do Python aceita `"0.5"` e `True`, mas no caminho real quem decodifica é o pydantic do SDK | Um corpo fora do esquema dá `resposta_invalida` |
| DV4 | Extração por poppler, OCR pelo tesseract do crate, página visual sem desenho vetorial (§10.4) | O crate não usa PyMuPDF | Entradas e digests do CLI podem diferir dos do Python para o mesmo PDF; N1 é idêntico |
| DV5 | Sem dublê no binário; o passo 3 do §9.1 é a autorização de envio | O dublê só serve a testes | — |
| DV6 | Só o Jev, sem persistência, reutilização, trava nem `ativo` | §2 | Cada execução com envio custa uma chamada |
| DV7 | `confianca_provedor` (o `confidence` do Jev) guardado | É informação que o Jev já devolve e o Python descarta | Só aparece na saída; não muda a decisão |
| DV8 | Numa resposta 200 cujo envelope não decodifica, `provider_request_id` vem do header | No Python, a exceção do SDK descarta o id (`typesafe.py`, `resposta_erro` sem `provider_request_id`) | Só metadado; ajuda a rastrear o erro com a TypeSafe |
| DV9 | Mensagens do CLI sem caminho do arquivo nem texto de exceção (§10.3) | TP põe `{caminho}` e `{exc}` no stderr; aqui viola R-P2 | Só a mensagem humana muda |
| DV10 | Na camada 2 (e na `conferir`, que a usa), cada parte de nome conhecido é delimitada por "não letra", `(?<![^\W\d_])(?:…)(?![^\W\d_])`, e não por `\b` | F10 (§0.4): para o `\b`, dígitos e `_` são letras de palavra, e um nome colado a um número ou entre sublinhados saía inteiro e passava pela conferência. Decisão do dono, 31/10/2026 | O texto mascarado difere de TP só quando uma parte de nome conhecido encosta num dígito ou `_`. TV não tem esse caso e continua idêntico. A saída e o request usam `MASCARAMENTO_VERSAO_PORTE = "mascaramento-5+cognitionlab-1"`; `MASCARAMENTO_VERSAO` continua a de TV, conferida pelos vetores. O diferencial (§14.3) aplica a mesma troca ao lado Python e compara todo o resto |

## 14. Testes e conformidade

### 14.1 Conformidade com TV

`tests/classifier_vectors.rs` lê TV **diretamente**, sem copiar casos para o código, e cobre cada seção:

| Seção de TV | Função Rust | Comparação |
|---|---|---|
| `meta`, `constantes`, `listas`, `criterio` | constantes de `categorias.rs` e `mascaramento` | igualdade exata |
| `regexes` | constantes do texto Python em `pyre.rs` | igualdade exata do texto (§6.4) |
| `dobrar`, `compor`, `partes_de_nome`, `termos_permitidos` | primitivas | byte a byte |
| `nomes_dos_indicios`, `nomes_dos_rotulos` | §6.6 | byte a byte |
| `remover_dados_pessoais` (126) | §6.5 | texto byte a byte e contagens |
| `conferir` (12) | §6.5 | contagens |
| `montar_estado` (21) | §6.7 | estado, JSON canônico, digest, contagens, flags |
| `decidir_classificacao` (7), `validar_respostas` (12) | §7 | floats com tolerância de 1e-12 |
| `json_canonico` (3) | §7.4 | byte a byte e sha256 |
| `referencia_pseudonima`, `bloco_tipo_detectado` | — | **fora do escopo** (§2.1): o teste lista as duas seções numa constante de exclusões e falha se TV ganhar uma seção que não esteja coberta nem excluída |

Um teste confere o sha256 de TV com o §0.1.

### 14.2 Propriedades (proptest, já dev-dependency)

- `dobrar` preserva a contagem de code points.
- A remoção **não** é idempotente em TP (F9, §0.4): o diferencial (§14.3) compara a segunda passada com a de TP, e um teste fixa o exemplo de F9.
- O marcador nunca carrega parte do valor: depois de remover um nome conhecido sintético, nenhuma parte dele com 3 ou mais letras sobra no texto, também com as peças coladas umas às outras, a dígitos ou a `_` (DV10; em TP o nome colado escapa, F10).
- Determinismo: a mesma entrada dá o mesmo estado e o mesmo digest.
- Limites: `caracteres_enviados ≤ 60 000`, cada página ≤ 6 000, e `truncado` coerente.
- `json_canonico` independe da ordem de inserção, com `preserve_order` ligada.

### 14.3 Diferencial com o Python (opcional, local)

`RUN_DIFFERENTIAL=1 TESTODOC_DIR=… cargo test --test classifier_differential -- --ignored` gera textos sintéticos aleatórios: rótulos, nomes inventados, datas, números e Unicode das classes difíceis (NFD, `İ`, `ı`, U+2028, `²`, `‿`). Roda `remover_dados_pessoais` e `montar_estado` no Python 3.14 do TestoDoc e no Rust e compara. Nunca usa exames reais.

### 14.4 Unicode

Testes com os caracteres das armadilhas: `İ`/`ı` sob IGNORECASE, em literal e em faixa de classe (`[A-Z]`, `[A-Za-z]`) (§6.4); `ch.isupper()` só da inicial × `s.isupper()` da string (`"Dr. João Silva"` removido; `"JOÃO DA SILVA"` vira `"João Da Silva"` em `extract_patient_hints`); `title()` com `ª`/`º` (letras sem caixa) e com `ǆ`; `upper()` com `ß`; marcas combinantes e `‿` fora de `\w`, `²`/`½`/`Ⅻ` em `[^\W\d_]`, U+001C a U+001F em `\s`, e `\x1f` fora de `compor`. Também um code point atribuído só na UCD 17 ou 18, para fixar o caminho escolhido no §4.2.

### 14.5 Adaptador HTTP (sem rede)

Um servidor HTTP local de teste (um `tokio::net::TcpListener` em `127.0.0.1`, sem dependência nova) cobre:
- **200 ok:** confere os headers, o corpo **byte a byte** (ordem de `criteria` e do `state`) e a decisão;
- **200 sem o header de request id:** `None` (DV2);
- **200 inválido:** corpo não JSON; `usage` ausente; `answers` vazio; `answers` com uma resposta extra além de `tipo_documento` (precisa dar `resposta_invalida` com `model` e `uso` preservados); `type` errado; `probabilities` ausente, `null`, vazio ou com chave estranha; soma 0,98; `NaN` como string; número como string;
- **timeout do `reqwest`:** um erro com `is_timeout()` vira `timeout`, não `rede`;
- **status:** 400, 401, 403, 404, 413, 415, 422, 429, 500, 529 e 204;
- **timeout:** o servidor dorme além do prazo, e também a espera na fila do semáforo;
- **conexão recusada:** porta fechada;
- **segurança:** a chave não aparece em `Debug`, nos erros nem nos logs (com `tracing-subscriber` de teste); o corpo de um erro não é registrado.

### 14.6 Jev real (opcional)

`RUN_JEV_LIVE=1 TYPESAFE_API_KEY=… cargo test --features classifier-jev --test classifier_live -- --ignored`: três documentos **sintéticos** (eco com strain, Holter, exame de sangue), que conferem a forma da resposta e a validação. Não conferem o tipo exato: o modelo é externo.

### 14.7 Exames reais (local)

`RUN_SAMPLE_EXAMS=1` (`tests/sample_exams.rs`), **sempre sem envio**. Confere só propriedades: nenhum pânico, `estado ∈ {preparado, sem_conteudo, bloqueado_privacidade}`, limites respeitados e nenhuma saída com texto. As mensagens nomeiam o documento por índice.

### 14.8 Comandos de aceite de cada fase

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features classifier-jev -- -D warnings
cargo test
cargo test --features classifier-jev
```

## 15. Requisitos não funcionais

- **Pureza e concorrência:** N1 é `Send + Sync`, sem I/O e sem estado global mutável. As regexes fixas são compiladas uma vez, no `Mascarador` (§4.3), e N1 pode rodar em várias threads sobre o mesmo `&Mascarador`.
- **Memória:** cada texto enviado tem no máximo 60 000 code points. A busca de nomes varre o texto de todas as páginas, até 200.
- **Latência:** limitada por `RLCD_TIMEOUT_S` por chamada. Não há meta para N1 (TD Q6). A Fase 3 mede `montar_estado` sobre os 21 casos de TV e um documento sintético de 200 páginas, e registra o resultado sem gate.
- **Determinismo:** a mesma entrada e o mesmo modo dão o mesmo estado, o mesmo digest e os mesmos bytes de request.

## 16. Invariantes do porte

1. O Jev nunca é chamado com `sem_conteudo`, com bloqueio ou sem a autorização R-P1.
2. O estado enviado só tem as chaves do §9.2. Os termos de nome de arquivo e título vêm só de `VOCABULARIO_TIPO`.
3. N1 reproduz TV byte a byte. Qualquer mudança no mascaramento exige outra `MASCARAMENTO_VERSAO`, feita no TestoDoc. A única exceção é a DV10 (§13), decidida pelo dono, que tem versão própria (`MASCARAMENTO_VERSAO_PORTE`) e não muda nenhum caso de TV.
4. `criteria` vai na ordem de `Categoria::TODAS`, e o `state` na ordem de construção, sejam quais forem as features do `serde_json`.
5. Logs, saída e relatório nunca têm texto do exame, nome de pessoa, nome do arquivo de entrada, chave nem corpo de erro do provedor.
6. Uma falha do classificador nunca falha a anonimização.
7. A chave só é lida depois da autorização de envio.
8. Nenhuma estrutura guarda os valores removidos pelo mascaramento: só o texto mascarado e as contagens (TP `Resultado`, `EstadoPreparado`).

## 17. O que um implementador Rust precisa saber (checklist)

- [ ] Ler TD §6–§10 inteiras antes de escrever N1. Este documento só diz como portar.
- [ ] Offsets em code points, com o mapa byte → code point (§6.2).
- [ ] Classes de caracteres geradas do Python 3.14 com UCD 16.0.0, nunca `\w`, `\d`, `\s`, `\b` nem `char::is_*` (§6.3).
- [ ] `unicode-normalization` na UCD 16.0.0 ou com a guarda de code points não atribuídos (§4.2).
- [ ] `pos` sem fatiar; `$` com `\n?\z` nos filtros da camada 4; `[iIİı]` em regiões IGNORECASE (§6.4).
- [ ] Texto Python de cada regex conferido com TV (§6.4).
- [ ] Listas e conjuntos copiados de TV por teste, não redigitados (§6.5).
- [ ] `_substituir` com a ordem `(início, fim, texto)`; contagens antes do descarte; camadas 2 e 4 em sequência (§6.5).
- [ ] Ordem remover → conferir a página inteira → cortar, com `_sem_marcador_partido` (§6.7).
- [ ] `json_canonico` próprio, sem NFC; duas serializações escritas à mão (§5.4, §7.4).
- [ ] Soma das probabilidades na ordem das categorias; tolerâncias 1e-3 e 1e-9 (§7).
- [ ] Ordem de `criteria` no fio testada em bytes com e sem `preserve_order` (§8.2).
- [ ] Timeout em volta do semáforo e do POST; sem retry; mapa de status e transporte, com `is_timeout()` primeiro (§8.3–§8.6).
- [ ] `answers` decodificado como mapa; envelope × validação, com os metadados preservados de cada caso (§8.4).
- [ ] `validar_respostas` no formato neutro de TV, com o one-hot só para `null`/string (§7.1).
- [ ] `ch.isupper()` só da inicial em `_parece_nome`/`_fim_do_nome`; `s.isupper()`, `title()` e `upper()` de string só em `extract_patient_hints` (§6.4).
- [ ] Só as funções listadas no §6.1 chamam `compor`; `dobrar` não.
- [ ] `estado_preparado` nunca em bloqueio nem em `sem_conteudo`; envelope de erro do CLI com `MotivoCli`; mensagens sem caminho (§10).
- [ ] Opt-in duplo antes de ler a chave; chave redigida (§3.2).
- [ ] Nenhum texto em log ou saída (§3.2, §16).

## 18. Perguntas abertas e decisões pendentes do dono

| # | Pergunta | Bloqueia | Recomendação |
|---|---|---|---|
| ~~Q-P1~~ | ~~Autoriza enviar ao Jev o texto mascarado dos exames de `tests/sample-exams/`?~~ **Decidido em 31/10/2026: só as cópias anonimizadas com `verify` ok, com o opt-in duplo** (§3.1; exceção no `CLAUDE.md`). | — | — |
| ~~Q-P2~~ | ~~Os vetores têm nomes sintéticos que coincidem com palavras dos nomes de arquivo dos exames (F8)?~~ **Decidido em 31/10/2026: (a), regenerar no TestoDoc. Feito** (§3.4, commit `6cd3dd0` lá). | — | — |
| Q-C1 | Confirma DV2: uma resposta sem `x-typesafe-request-id` continua válida? | Não; o padrão está definido | Sim |
| Q-C2 | Colunas e formato dos pesos no índice (§11.2) | Não | Como no §11.2 |
| Q-C3 | Manter `jev-1.13.0` ou passar para `jev-1.14.0`? Trocar invalida qualquer calibração do limiar de 0,70 (TS-MOD, "Aliases") | Não | Manter 1.13.0 até a Fase 7 |
| Q-C4 | Fazer a Fase 7 (calibração com conjunto rotulado por médico e teste de reordenação das opções, TS-JAG #8)? | Não | Só depois da Fase 6 e de um conjunto rotulado |

Lacunas da fonte que **não** bloqueiam este porte: TD Q1b (transporte entre o Python e o Rust no TestoDoc) e Q7 (Redis em produção), que não se aplicam aqui; TD Q4 (CLI × worker), registrada como DV4; TD Q6 (meta de latência), registrada no §15.

## 19. Declaração

Com as fontes do §0 fixadas e os vetores (TV), este documento é suficiente para implementar em Rust o núcleo N1, o adaptador do Jev (N2) e o CLI. A semântica fica fixada até o motor de regex, o Unicode e o fio HTTP, e a equivalência do núcleo é verificável byte a byte. As duas decisões que bloqueavam estão tomadas:
- **Q-P1:** o envio vale só para as cópias anonimizadas verificadas (§3.1).
- **Q-P2:** os vetores foram regenerados com nomes inventados (§3.4).

A origem de TV é o commit da troca no TestoDoc (§0.1).

## 20. Plano faseado

Cada fase termina com os comandos do §14.8 verdes, um registro em `reports/progress/PROGRESS.md` e a revisão adversarial do `CLAUDE.md` (Grok → Codex → Antigravity, um de cada vez, com o pacote sem dados de exame, R-P5), salva em `reports/reviews/classifier/phase-N/`. O dono pode dispensar um revisor por escrito, como já fez com o Grok. A fase só fecha com todos os achados válidos corrigidos ou refutados com evidência.

### Fase 0: preparação e decisões

- **Entradas:** o commit da troca de nomes no TestoDoc (§3.4), com a mensagem sem as palavras originais e enviado ao remoto.
- **Trabalho:**
  1. conferir os hashes do §0;
  2. copiar TV para `tests/fixtures/classifier/vetores.json`, com o sha256 e o commit de origem em `tests/fixtures/classifier/README.md`, e rodar a checagem de palavras dos exames (só contagens) antes do commit;
  3. resolver o `unicode-normalization` (`cargo update -p unicode-normalization --precise 0.1.24`, se possível; senão, a guarda do §4.2);
  4. acrescentar `fancy-regex = "0.19.2"` e a feature `classifier-jev`;
  5. criar o esqueleto de `src/classifier/` com os tipos do §5;
  6. criar o teste de conformidade com uma lista de seções pendentes, que só pode encolher. Uma seção pendente falha se passar a ser coberta sem sair da lista, e nenhuma seção fica pulada em silêncio. Uma seção só é coberta se estiver registrada com a sua verificação, e um único teste roda todas as verificações registradas.
- **Saída:** compila; testes dos tipos e das constantes contra TV (`meta`, `constantes`, `criterio`).

### Fase 1: primitivas e Unicode

- **Trabalho:**
  1. `scripts/gen-pyclasses.py` e `pyclasses.rs`, com o teste de sincronia;
  2. `compor`, `dobrar`, `lower_python`, `isupper_python`, `title_python`, `MapaOffsets`;
  3. `json_canonico` e digest;
  4. `partes_de_nome`, `termos_permitidos`;
  5. testes do §14.4.
- **Saída:** TV `dobrar`, `compor`, `partes_de_nome`, `termos_permitidos`, `json_canonico` passam; proptests de `dobrar` e de `json_canonico`.

### Fase 2: mascaramento

- **Trabalho:**
  1. `pyre.rs`, com a tradução das regexes do inventário do §6.4 (22 de TV, 3 de `patient_hints`, as auxiliares e as dinâmicas) e o teste de texto e flags;
  2. camadas 1 a 5, `_substituir`, colapso, `conferir`, `nomes_dos_rotulos`, `_parece_nome`, `extract_patient_hints` e `_nomes_dos_indicios`;
  3. diferencial opcional (§14.3).
- **Saída:** TV `remover_dados_pessoais` (126), `conferir` (12), `nomes_dos_indicios`, `nomes_dos_rotulos` passam; proptests de idempotência e do marcador.
- **Risco principal:** semântica de regex (§6.4). Mitigação: diferencial e casos de Unicode.

### Fase 3: estado e decisão

- **Trabalho:** `montar_estado`, `caracteres_uteis`, `termo_do_tipo`, as duas serializações do §5.4, `validar_resposta_escolha`, `decidir_classificacao`, a medição do §15.
- **Saída:** TV `montar_estado` (21), `validar_respostas` (12), `decidir_classificacao` (7) passam. A lista de seções pendentes fica vazia (só restam as duas exclusões).

### Fase 4: adaptador do Jev e `classificar`

- **Trabalho:** `provedor.rs` (trait, request neutro, `avaliar` com timeout e semáforo), `jev.rs` (§8), `classificar` (§9.1), o dublê de teste e o servidor local do §14.5.
- **Saída:** todos os casos do §14.5; teste ao vivo opcional (§14.6) rodado pelo dono, se quiser, com dados sintéticos.

### Fase 5: CLI `classify`

- **Trabalho:**
  1. subcomando, configuração (§12), entrada (§10.2), página visual (§10.4), saída e códigos (§10.3), opt-in duplo;
  2. fixtures sintéticas: um PDF de texto nativo, um PDF digitalizado e um PNG, gerados como as de `examples/gen_fixtures.rs`, com nomes e números inventados do zero;
  3. teste local com exames reais **sem envio** (§14.7);
  4. testes de regressão do CLI:
     - `estado_preparado` ausente em `bloqueado_privacidade` (o caso A9: `--nome-paciente "Joana Mapa"` com um arquivo `eco_mapa.pdf` sintético), em `sem_conteudo` e nos erros antes da classificação (AG-2);
     - nenhuma mensagem do stderr ou do modo legível contém o caminho do arquivo (DV9);
     - `CLASSIFIER_REMOTE_ALLOWED=0` dá `preparado` e código 0, sem ler a chave (§12).
- **Saída:**
  - todos os códigos de saída cobertos por testes;
  - `cargo run -- classify --json <fixture>` dá `preparado` sem autorização;
  - com o servidor local, dá `ok`.

### Fase 6: integração com o relatório do anonimizador (Python)

- **Entradas:** Q-C2. A Q-P1 está decidida: o envio vale só para as cópias com `verify` ok.
- **Trabalho:** no `anonimizador` (Python, `uv`, `ruff`, `pytest`), as opções `--classifier` e `--classify-remote`, o passo de classificação depois da re-verificação, as colunas e o cabeçalho do §11.2, a gravação da linha JSON com `write_new`, e o isolamento de falhas (§11.1). Testes com um binário falso, um script que imprime linhas JSON fixas: `ok`, `incerto`, cada estado, JSON inválido, timeout e código inesperado.
- **Revisão:** como no estágio 1–2 do anonimizador (Codex → Antigravity, com o aviso de que o código é Python e deve seguir Python idiomático; Grok conforme a dispensa do dono).
- **Saída:**
  - o relatório com as colunas novas, sem envio (`not sent`);
  - com o envio autorizado (Q-P1), o dono roda `scripts/anonymize-sample-exams.sh --classifier … --classify-remote` com `CLASSIFIER_REMOTE_ALLOWED=1` e revisa o índice localmente. Nada disso é commitado.

### Fase 7 (opcional): calibração

- **Entradas:** conjunto de documentos rotulado por médico, separado em ajuste e validação (TD §17.3); Q-C3 e Q-C4.
- **Trabalho:** métricas de TD §17.3 (precisão ≥ 95 % entre os aceitos dos oito tipos, cobertura ≥ 80 %, nenhum `outro` aceito como tipo, nenhuma troca confiante nos pares críticos), uma tabela por modelo e a conferência de estabilidade com as opções reordenadas (TS-JAG #8), só como medição. O request normativo não muda sem nova `PERGUNTAS_VERSAO` no TestoDoc.

### Dependências entre fases

```
Fase 0 ─► Fase 1 ─► Fase 2 ─► Fase 3 ─► Fase 4 ─► Fase 5 ─► Fase 6 ─► (Fase 7)
  ▲
  commit da troca de nomes no TestoDoc enviado ao remoto (§3.4)
```

## 21. Histórico e auditoria

### 21.1 Versão 1 (31/10/2026)

Primeira versão, feita a partir de TD v5, TV, TP, TS-API/TS-ERR/TS-MOD/TS-JAG e do SDK 0.7.4. As verificações executadas para escrevê-la, todas reproduzíveis, foram:
- `UNICODE_VERSION` de `unicode-normalization` 0.1.24 (16.0.0), 0.1.25 (17.0.0) e 0.1.26 (18.0.0);
- `regex-syntax` 0.8.11 na UCD 16.0.0;
- `unidata_version` do `python3` (3.15.2: 17.0.0) e do `python3.14` (3.14.10: 16.0.0);
- `IGNORECASE` com `İ`/`ı` no Python 3.14 × `regex` 1.13.1;
- `\w` com marca combinante e `‿` no `regex` 1.13.1;
- `find_from_pos` com lookbehind no `fancy-regex` 0.19.2;
- escape de `\x1f`, U+007F e U+2028 no `serde_json` 1.0.154;
- a contagem de palavras dos nomes de arquivo dos exames em TV (F8).

### 21.2 Auditoria adversarial (Antigravity)

**Rodada 1 (31/10/2026).**
- **Como rodou:** `agy` 1.4.12, `--mode plan --sandbox`, somente leitura, a partir de um diretório isolado fora do repositório, com o pacote filtrado do §3.2 (R-P5).
- **Pacote:** a versão 1 desta especificação, TD, TP, trechos do SDK, documentação da TypeSafe, arquivos do crate e o resumo estrutural de TV, sem os casos.
- **Tentativa abortada:** a primeira execução parou antes de revisar, por um caminho errado no pacote (`fontes/crate/extraction/mod.rs`). Foi refeita com a lista exata dos arquivos.
- **Resultado:** o relatório bruto está em `reports/reviews/classifier/spec/agy-round1.md`. Os 12 achados foram conferidos no código (TP, SDK e crate) e **todos procedem**; nenhum foi refutado.

| ID | Sev. | Crítica | Veredito e evidência | Correção |
|---|---|---|---|---|
| AG-1 | alta | `isupper` de palavra × de inicial; faltam `islower`/`istitle`/`iscased` e os mapeamentos; `title_python` errado | **Procede.** `mascaramento.py:284,384` usam `[:1].isupper()`; `patient_hints.py:28–29` usa `nome.upper()`, `nome.isupper()` e `nome.title()`. Conferido no Python: `"ǆ".title() == "ǅ"`, `"ßa".upper() == "SSA"` | §6.3 (tabelas e mapeamentos); §6.4 (quatro linhas: caractere, string, `title`, `upper`); §14.4; §17 |
| AG-2 | alta | `--mostrar-estado` emitiria o estado em `bloqueado_privacidade` | **Procede.** `classificar_documento.py:369` só emite com `estado ∈ {preparado, ok, incerto, erro}`, sem bloqueio e sem `sem_conteudo` | §10.1; §17 |
| AG-3 | alta | `answers` como struct engole chaves extras; metadados perdidos; `usage` contraditório; TV `validar_respostas` usa o formato neutro | **Procede.** SDK `_schemas/models.py`: `usage`, `probabilities` e `confidence` são obrigatórios; `rlcd_client.py:195–243` usa `brutas` neutro com one-hot em `None`/string | §7.1 reescrito (formato neutro); §8.4 reescrito (envelope × conversão × validação, metadados por caso); DV8; §14.5; §17 |
| AG-4 | alta | `İ`/`ı` também em faixas `[A-Z]`; regexes dinâmicas × `LazyLock`; contagem "13" errada; flags fora de TV | **Procede.** `patient_hints.py:15–18` e `_REGISTRO_RE` (`:405–410`) com `IGNORECASE`; dinâmicas em `:342,344` e `classificador_documento.py:239`; TV tem 10 + 12 (`_PADROES`) regexes; o crate nega `expect_used` | §4.3 (`Mascarador` sem `expect`; dinâmicas com `Result`); §6.4 (faixas e inventário completo com flags); §15; Fase 2 |
| AG-5 | média | "toda função pública compõe" contradiz TP (`dobrar` não compõe) | **Procede.** `grep compor(` dá exatamente as funções listadas no §6.1 | §6.1 (lista de quem compõe e de quem não) |
| AG-6 | média | `_rotulo_composto` sem `[a-z]+`; ordens diferentes em `_camada_rotulos` × `nomes_dos_rotulos` | **Procede.** `mascaramento.py:188–190,262–269,304` | §6.5 |
| AG-7 | média | `OcrEngine` não lê imagem avulsa; `por+eng` × `por`; ferramenta ausente × arquivo ilegível; detalhes de `pdfimages` | **Procede.** `extraction/mod.rs:166,214`; `ocr_local.IDIOMA`; `pdf_extract.py:36,92–117` | §10.1 (passos 4 e 5); §10.2 (OCR, imagem, `ocr_image`); §10.4 (`image`+`stencil`, `-f/-l`, falha = visual) |
| AG-8 | média | `connect_timeout` do `reqwest` cairia em `rede` | **Procede** (o SDK testa timeout antes de `RequestError`, `transport.py`) | §8.3 (só o relógio do `tokio`); §8.5 (`is_timeout()` primeiro); §14.5 |
| AG-9 | média | envelope de erro e motivos do CLI; ordem das validações; `REMOTE_ALLOWED` não é erro; `BASE_URL` vazia; caminho em mensagens | **Procede.** `classificar_documento.py:191–200` (`_envelope`), `:292–316` (ordem), `:147,313,374` (caminho e exceção) | §5.5 (`MotivoCli`); §10.1 (ordem); §10.3 (envelope e mensagens); §12 (quando cada variável é lida); DV9 |
| AG-10 | média | célula de erro sem `erro_categoria`; diretório `classifier/` em `start()`; nome do arquivo; casas de `<confianca>`; cabeçalho sem resultado; "opção ausente" contraditório | **Procede.** `batch.py` (`start`, `index`) | §11.1; §11.2 |
| AG-11 | média | TV `json_canonico` usa objetos arbitrários | **Procede.** TV `json_canonico[0].valor` é `{"b":…, "a":…, "c":{…}}` | §5.4; §7.4 (`json_canonico(&Value)` com ordenação explícita) |
| AG-12 | baixa | NFC da 0.1.26 em texto inteiro reordenaria marcas novas no caminho com guarda | **Procede** (em UCD 16 os não atribuídos têm classe combinante 0) | §4.2 (NFC por trecho de atribuídos) |

**Rodada 2 (31/10/2026).**
- **Pacote:** o mesmo processo, sobre a versão 2. Os vetores e a TD já estavam com os nomes trocados, e o pacote ganhou `schemas_models.py` do SDK, `ocr_local.py` e `poppler.rs`.
- **Resultado:** AG-1 a AG-12 **fechados**, e na revisão completa, **NO FINDINGS**. O veredito do Antigravity é que a versão 2 "é suficiente para implementar o porte em Rust sem adivinhação". O relatório bruto está em `reports/reviews/classifier/spec/agy-round2.md`.
- **Imprecisões do relatório:** ao justificar o fechamento, ele atribui à especificação três detalhes que ela não tem:
  - testes com "Đorđe/Łukasz" no §14.1;
  - um motivo `ferramenta_ausente` (o motivo especificado é `ocr_indisponivel`);
  - um teste explícito de `--mostrar-estado` no §14.

  As correções que fecham cada achado existem; as imprecisões estão só nas descrições dele. O teste de regressão de `--mostrar-estado` (AG-2) foi então acrescentado à Fase 5, item 4, **depois** da rodada 2. É só um teste novo, sem mudança de comportamento, e não foi revisado.

**Changelog da versão 2:** cabeçalho; §4.2; §4.3; §5.4; §5.5; §6.1; §6.3; §6.4; §6.5; §7.1; §7.4; §8.3; §8.4; §8.5; §10.1–§10.4; §11.1; §11.2; §12; §13 (DV8, DV9); §14.4; §14.5; §15; §17; §20 (Fase 2). Também entraram, fora da auditoria:
- a decisão Q-P1 (§3.1, invariante 8, exceção no `CLAUDE.md`);
- a decisão Q-P2 e a troca de nomes nos vetores do TestoDoc (§0.1, §0.4 F8, §3.4).
