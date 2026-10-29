# Plano de migração: mini-swe-agent Python → Rust

Data da análise: 28/10/2026. Estado: planejamento; nenhuma implementação Rust ou paridade comprovada nesta entrega.

## 1. Objetivo e referência

Reimplementar o harness em Rust, mantendo o Python executável como oráculo de comportamento. O resultado deve preservar prompts, ações, observações, limites, custos, término e trajetória nos casos definidos pelo contrato. Traduzir classes linha a linha não é o objetivo: preservar comportamento com ownership claro, tipos explícitos e fluxo simples é.

O código analisado está em `../mini-swe-agent/src/minisweagent/` e declara versão **2.4.6**. O requisito local em [prompt/migrate.md](prompt/migrate.md) pede apenas ações textuais de shell, sem tool-calling nativo, sem sessões persistentes, e inclui BDD/Cucumber. A referência atual também contém tool-calling e Responses API; portanto, esse requisito descreve um **recorte**, não todo o upstream.

**Atenção à procedência:** `git -C mini-swe-agent rev-parse --show-toplevel` resolve para `CognitionLab`. O SHA `b3cb1431177eba1289ea00ffe7c983a25db8fdad` observado é do repositório pai e **não identifica o commit upstream**. Não declarar a referência congelada pelo SHA até verificar sua origem.

Como identificação provisória do conteúdo, a leitura dos 205 arquivos da cópia, excluindo componentes `.git`, `.venv`, `__pycache__` e `.pytest_cache`, produziu SHA-256 agregado `1c9632a76ba2171f2f5421c51d9b64391901997e1c599963bdb937b7913f294e`. Algoritmo: ordenar caminhos relativos; para cada arquivo gerar `sha256(bytes)`, dois espaços, caminho POSIX e newline; concatenar essas linhas em UTF-8 e calcular SHA-256. Isso identifica o snapshot local, não comprova origem upstream. O valor foi recalculado em 28/10/2026 com o mesmo algoritmo e coincidiu (205 arquivos). A análise detalhada do código que sustenta este plano está em [STUDY.md](STUDY.md); quando qualquer um dos dois divergir do código, prevalece o código Python e tanto PLAN.md quanto STUDY.md devem ser corrigidos.

## 2. Organização proposta

Todos os caminhos a seguir são relativos a `mini-rust-swe-agent/`. A árvore é planejada; não significa que os arquivos já existem.

```text
mini-rust-swe-agent/
├── README.md                    # estudo, arquitetura e status verificável
├── mini-swe-agent/              # referência Python preservada
├── spec/
│   ├── PLAN.md
│   ├── STUDY.md                 # análise do código Python e gates P01–P18
│   ├── PRD.md                   # placeholder vazio; preencher ou remover
│   ├── prompt/                  # enunciados da tarefa (migrate.md, estudo.md)
│   ├── reference.json           # origem, SHA ou snapshot, versões e hashes
│   ├── parity.md               # contratos e divergências permitidas
│   ├── scope.md                # matriz de cobertura
│   └── adr/                    # decisões com motivos e consequências
├── rust-mini-swe-agent/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── rust-toolchain.toml
│   ├── src/
│   │   ├── main.rs              # argumentos, composição e exit code
│   │   ├── lib.rs
│   │   ├── agent.rs
│   │   ├── model.rs
│   │   ├── environment.rs
│   │   ├── config.rs
│   │   ├── templates.rs
│   │   ├── trajectory.rs
│   │   └── error.rs
│   └── tests/
│       ├── parity.rs
│       └── bdd.rs
├── parity/
│   ├── fixtures/               # entradas comuns e resultados esperados
│   ├── python_runner.py        # importa a referência, sem alterá-la
│   ├── compare.py              # diff estrutural por caminho JSON
│   └── features/               # cenários Gherkin compartilhados
└── reports/                    # evidências de execução, sem segredos
```

Começar com **um pacote Cargo, biblioteca e binário**. Extrair submódulos quando surgirem implementações alternativas; não começar com um workspace de várias crates. O histórico Git permanece no pai. A referência Python não deve receber ajustes para fazer o Rust passar.

## 3. Escopo e marcos

| Capacidade | M1: paridade offline | M2: agente utilizável | Evolução posterior |
|---|---|---|---|
| Traits, loop, histórico, erros e limites | Obrigatória | Mantida | Mantida |
| Modelo determinístico e ambiente roteirizado | Obrigatórios | Regressão | Regressão |
| Parsing textual, templates, configuração | Obrigatórios no subconjunto documentado | Configurações textuais completas selecionadas | Ampliar com fixtures |
| LocalEnvironment POSIX e trajetória 1.1 | Obrigatórios | Mantidos | Outros SOs exigem contrato próprio |
| CLI não interativa | Obrigatória | Interface documentada | Compatibilidade de aliases quando necessária |
| Um provedor real textual | Fora do gate offline | Obrigatório, provedor/modelo identificados | Cada outro provedor tem gate próprio |
| Docker | Contrato e casos definidos | Marco separado M3 | Não presumir isolamento pelo nome do backend |
| OpenRouter/Requesty/Portkey/LiteLLM | Inventariados | Apenas integração escolhida | Adaptadores explícitos; sem promessa de substituir todo LiteLLM |
| InteractiveAgent e inspector | Adiados | Adiados | M4, se incluídos no produto |
| SWE-bench, ProgramBench e execução em lote | Adiados | Adiados | M4, avaliação e concorrência controlada |
| Singularity, Bubblewrap, Contree, SWE-ReX/Modal | Adiados | Adiados | Cada backend exige ambiente e teste próprios |
| Tool-calling, Responses API, multimodal | Excluídos do recorte inicial | Excluídos | Só ampliar mediante mudança explícita do escopo |
| Importação dinâmica de classes Python | Excluída | Registro de implementações Rust | Não prometer plugins Python no binário |
| Ferramentas além do shell e sessões persistentes | Excluídas | Excluídas | Exigem novo desenho e novo contrato |

“Migração do recorte textual concluída” e “paridade com todo o upstream” são afirmações diferentes. Registrar cada exclusão com justificativa. M1 não conclui a migração integral.

## 4. Passo a passo

### Passo 0 — Fixar a referência e tornar o Python reproduzível

1. Preservar a cópia atual; registrar inventário e hashes em `spec/reference.json`/manifesto separado.
2. Se houver acesso à origem, obter um checkout separado, identificar um SHA upstream verificável e comparar seu conteúdo com a cópia. Registrar alterações locais. Não substituir silenciosamente a referência pela branch principal.
3. Preservar `LICENSE.md` e atribuição aos autores; documentar a origem dos prompts, fixtures e código adaptado.
4. Fixar versão do Python e resolver dependências num ambiente exclusivo. O `pyproject.toml` possui intervalos abertos; apenas `pip install` não oferece reprodutibilidade. Exportar lock/constraints com versões e, quando viável, hashes.
5. Usar diretório temporário para `MSWEA_GLOBAL_CONFIG_DIR`, `MSWEA_SILENT_STARTUP=1`, ambiente controlado e ausência de credenciais reais. A importação do pacote cria diretório e carrega `.env` global.
6. Rodar primeiro testes selecionados de configuração, serialização, agente e ambiente local. Registrar dependências, SO, shell, código de saída, falhas e skips. Separar testes de rede, containers e `--run-fire`, que podem custar dinheiro.

Comandos iniciais, a executar da pasta deste projeto; não executados por esta documentação:

```bash
python3 -m venv .venv-reference
.venv-reference/bin/python -m pip install -e './mini-swe-agent[dev]'
.venv-reference/bin/python -m pip freeze > spec/python-reference.freeze.txt
mkdir -p reports/python-config
MSWEA_GLOBAL_CONFIG_DIR="$PWD/reports/python-config" MSWEA_SILENT_STARTUP=1 \
  .venv-reference/bin/python -m pytest \
  mini-swe-agent/tests/agents/test_default.py \
  mini-swe-agent/tests/environments/test_local.py \
  mini-swe-agent/tests/config/test_init.py \
  mini-swe-agent/tests/utils/test_serialize.py
```

O freeze registra o ambiente inicial, mas a instalação editável referencia o código local: reprodutibilidade exige também o snapshot e um lock validado numa instalação limpa. Alternativa equivalente: `uv run pytest ...` executado dentro de `mini-swe-agent/`, que resolve o `pyproject.toml`; não há `uv.lock` na cópia, então exportar `uv lock`/`uv export` para `spec/` cumpre o mesmo papel do freeze.

**Aceite:** manifesto identifica a referência sem confundir SHA do pai; conjunto offline executável e resultado registrado; nenhuma alteração na fonte Python.

### Passo 1 — Escrever os contratos antes da implementação

Criar `spec/parity.md` com entradas, saídas, estado, efeitos colaterais e exemplos de cada fronteira:

| Fronteira | Contrato mínimo |
|---|---|
| Model | Consulta recebe histórico ordenado; resposta inclui conteúdo, ações e custo; formata mensagens e observações; exporta variáveis e configuração |
| Environment | `execute(action, cwd="")` no Protocol; `LocalEnvironment` aceita ainda `timeout` como keyword, mas `DefaultAgent.execute_actions` chama `env.execute(action)` sem cwd nem timeout, logo o cwd efetivo é `config.cwd` ou o cwd do processo e o timeout é `config.timeout` (30 s). Um campo `cwd` dentro da action não é lido. Retorna output, returncode, exception_info e extras, ou sinaliza submissão |
| Agent | Coordena chamadas, custos, limites, mensagens e término; retorna extras da mensagem final |
| Configuração | YAML e `a.b=valor`; parsing JSON do valor ou string; merge recursivo; ausência é distinta de null/false/zero |
| Templates | Jinja com StrictUndefined; ordem das variáveis, filtros, slicing, Unicode e whitespace preservados |
| Trajetória | Objeto completo com `info`, `messages` e `trajectory_format`; preservar campos extensíveis |
| CLI | Argumentos, precedência, stdout/stderr, caminho de saída, exit code e interrupção definidos |

Registrar sutilezas do código real:

- O parser textual usa a regex ```` ```mswea_bash_command\s*\n(.*?)\n``` ```` com DOTALL (newline obrigatório após o nome e antes do fecho), aplica `strip()` a cada captura e exige **exatamente uma** captura. `content` nulo vira `""`; um bloco vazio ainda é uma captura válida (comando vazio); várias linhas num bloco são um único comando; fence `bash` comum não conta; zero ou vários blocos produzem `FormatError`. O motor `regex` do Rust não aceita lookaround nem todas as flags do `re`: `action_regex` configurável só é suportado no subconjunto comum, com rejeição explícita do resto.
- Limites são verificados **antes** da consulta: `0 < limite <= acumulado`. Zero desabilita. `n_calls` aumenta antes de `model.query` e esse incremento sobrevive a `FormatError` e a falhas do modelo: `api_calls` já conta a tentativa enquanto `instance_cost` pode ainda não conter o custo dessa resposta. O custo é acrescentado após a resposta ou pelo tratamento de erro de formato.
- **Estatísticas globais** (`GlobalModelStats`, STUDY §8) fazem parte do contrato de Model: todo `query()` bem-sucedido chama `add(custo)` depois de calcular o custo e **antes** do parser; os limiares vêm de `MSWEA_GLOBAL_COST_LIMIT`/`MSWEA_GLOBAL_CALL_LIMIT`, usam `<` (`0 < cost_limit < custo_total`, `0 < call_limit < n_calls + 1`, logo `call_limit == 1` falha já na primeira resposta) e lançam `RuntimeError`, que o agente trata como erro inesperado (mensagem `exit` diagnóstica, save, propagação). O Rust recebe a instância de estatísticas por composição, nunca por singleton oculto; cada cenário de teste cria a sua.
- `FormatError` preserva mensagens, cobra o custo disponível na primeira mensagem carregada, incrementa erros consecutivos e pode terminar com `exit_status: RepeatedFormatError`, que é um **valor de status** na mensagem `exit`, não uma classe de exceção. Nesse caminho nenhuma mensagem `assistant` é criada: o Rust não deve sintetizar uma. Um passo limpo zera esse contador.
- `extra.actions` ausente ou `[]` não é erro no agente: `execute_actions` produz lista vazia de outputs e chama `format_observation_messages` uma vez; o passo conta como limpo e o loop segue até um limite.
- `run()` limpa mensagens, mas não reinicializa todos os contadores nem o tempo iniciado no construtor. Testar reutilização do mesmo agente ou documentar restrição a uma execução por instância; não “corrigir” silenciosamente.
- `execute_actions()` executa sequencialmente. Uma submissão interrompe a sequência e não gera a observação normal. Testar isso mesmo que o parser textual normalmente produza uma única ação.
- O `finally` salva após cada tentativa de passo; erros inesperados acrescentam mensagem `exit` e são propagados. Falhas antes de entrar no loop (por exemplo, renderização inicial) têm comportamento distinto.
- `get_template_vars()` faz merge de configuração do agente → ambiente → modelo → contadores/tempo → variáveis extras → argumentos. No ambiente local entram plataforma e variáveis de ambiente.
- O shell local usa `shell=True`: em POSIX o padrão é `/bin/sh -c`, apesar do texto mencionar bash. Registrar ADR: modo compatível reproduz `/bin/sh`; bash explícito é opção distinta, testada, com divergência declarada.
- `cd` e `export` não persistem entre ações; alterações em arquivos persistem. `stderr` é mesclado ao stdout, UTF-8 inválido é substituído e timeout mata o grupo de processos em POSIX.
- Submissão: `lines = output.lstrip().splitlines(keepends=True)`; termina se `lines[0].strip() == "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"` e `returncode == 0`; submissão = `"".join(lines[1:])`. Espaços ao redor do marcador na primeira linha são aceitos; case diferente, texto extra na linha, log antes do marcador, retorno não zero ou marcador apenas no texto do LLM não submetem. CRLF é preservado na submissão; marcador sem newline final produz submissão vazia. A matriz completa (8 + 2 casos) está no STUDY §5.
- **Seleção de classes não vem do YAML textual:** `default.yaml` e `mini_textbased.yaml` não definem `model.model_class`, e `get_model` cai em `LitellmModel` (tool-calling). O oráculo Python deve instanciar `DefaultAgent`, `LitellmTextbasedModel`/`DeterministicModel` e `LocalEnvironment` diretamente, ou passar `model_class` explícito. No Rust, a CLI mínima só oferece o caminho textual, e isso é uma decisão registrada, não paridade com `mini` (STUDY §3.3).
- As configs são Pydantic com `extra='ignore'`: `mode: confirm` do YAML é descartado pelo `DefaultAgent`, chaves desconhecidas não aparecem em `info.config` nem no contexto de template; `output_path` serializa como string ou `null`. Serde deve ignorar campos desconhecidos nesses structs e serializar `Option<PathBuf>` como `null`.
- Um filho em background que herda o stdout mantém o pipe aberto: o comando só termina no timeout, com saída parcial e grupo morto (STUDY §4). O Rust deve reproduzir isso lendo até EOF, não até a saída do shell pai.
- `DeterministicModel` tem ações especiais `raise`, `/sleep N` e `/warning msg` que avançam para o próximo output sem contar chamada; o custo global usa `cost_per_call` e o custo local usa `extra.cost` da mensagem (STUDY §3.4). Em JSON, `raise` vira uma tag com o nome da exceção.
- `run()` acumula `extra_template_vars` com `|=` e não zera `n_consecutive_format_errors`; falhas de `save()` dentro do `finally` substituem a exceção em voo.

**Aceite:** cada regra aponta para fonte/teste Python e possui cenário observável; divergências deliberadas têm ADR.

### Passo 2 — Criar o pacote Rust

Executar a partir de `mini-rust-swe-agent/` quando começar a implementação:

```bash
cargo new --bin --edition 2024 --vcs none --name mini-swe rust-mini-swe-agent
cd rust-mini-swe-agent
cargo build
cargo test
```

`--vcs none` evita repositório aninhado. O pacote `mini-swe` terá import Rust `mini_swe`; o diretório continua `rust-mini-swe-agent`. Se a pasta já existir, inspecionar seu conteúdo antes de usar `cargo init`; não sobrescrever outro pacote. Ver [Cargo: criação de pacote](https://doc.rust-lang.org/cargo/guide/creating-a-new-project.html) e [cargo init](https://doc.rust-lang.org/cargo/commands/cargo-init.html).

1. Adicionar `src/lib.rs`; manter `main.rs` apenas como composição e tratamento final de erros.
2. Fixar a toolchain em `rust-toolchain.toml` com `channel = "1.99.1"` e componentes `rustfmt` e `clippy`, como em `../embeddings-fundamentals/`; `cargo 1.99.1` e `rustc 1.99.1` foram verificados instalados em 28/10/2026. Declarar `edition = "2024"`, `rust-version = "1.99"` e `publish = false`. Copiar o `clippy.toml` do crate irmão (`allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests = true`) para que a regra "sem unwrap fora de testes" seja verificada por lint, não por revisão.
3. Versionar `Cargo.lock` do executável. Adicionar ignore de `target`, ambientes Python e relatórios temporários sem esconder fixtures.
4. Introduzir dependências conforme a etapa: `serde`/`serde_json` (feature `preserve_order`, para que a ordem das chaves acompanhe a inserção como no Python), `thiserror`, `clap`, `regex`; `minijinja` com `UndefinedBehavior::Strict` como candidato ao motor de templates, validado no passo 5; `serde_yaml` ou sucessor mantido para YAML; `nix` (feature `signal`/`process`) para `killpg` sem `unsafe` próprio; biblioteca de HTTP somente no M2. Confirmar manutenção, licenças, MSRV e recursos antes de fixar versões. Estado em 28/10/2026: toolchain 1.99.1, `std::io::pipe` com `PipeWriter::try_clone`, `process_group` e `ExitStatusExt::signal` confirmados por compilação; `minijinja` (slices negativos, `length` em code points, `is defined` sob `Strict`), `nix::sys::signal::killpg`, `serde_yaml` e `cucumber` ainda **não confirmados**: verificar antes de fixar e gravar as versões em `spec/reference.json`.
5. Preferir execução síncrona no primeiro marco, coerente com `std::process::Command` e o loop original. Tokio só quando houver necessidade comprovada de I/O concorrente. Não introduzir async em toda a API antecipadamente.
6. Habilitar warnings e Clippy seletivamente; proibir unsafe próprio no núcleo quando possível. Não copiar automaticamente `panic=abort` ou ajustes agressivos de release: encerramento e limpeza precisam ser verificados.

**Aceite:** build, fmt, Clippy e testes básicos passam na toolchain fixada; biblioteca importável; binário sem lógica de domínio.

### Passo 3 — Definir tipos e traits core

1. Criar `Model`, `Environment` e `Agent` com operações equivalentes aos protocols Python. Separar a representação interna do JSON de compatibilidade.
2. Definir tipos para mensagem, ação, resultado da execução, estado final e estatísticas. Usar enums para resultados conhecidos e `serde_json::Value`/mapas apenas nas extensões necessárias.
3. Usar `&str` e slices nas entradas, `&mut self` para estado mutável e ownership único do histórico. Generics são suficientes inicialmente; enums ou objetos de trait podem compor backends quando a CLI exigir seleção dinâmica.
4. Modelar submissão/limite como resultado de controle carregando mensagens; falhas recuperáveis com `Result`, erros tipados com `thiserror`, contexto no limite da aplicação. Não transformar toda interrupção normal em falha fatal.
5. Definir relógio injetável para testes que devolve segundos como `f64`, no modo paridade com semântica de parede igual a `time.time()`: `elapsed_seconds = int(now - start)` trunca rumo a zero, então `wall_time_limit_seconds: 1` ainda permite uma consulta aos 1,999 s. Um relógio monotônico muda términos e prompts e fica como ADR de produto fora do gate M1. O comparador exercita os limites avançando o relógio injetado, sem sleeps.
6. Iniciar uma implementação roteirizada mínima de ambiente para testar o contrato antes de conectar processos reais. Isso é infraestrutura de teste; o modelo determinístico completo continua no passo 6.

**Aceite:** regras de serialização dos tipos e resultados de controle verificadas; APIs não exigem clone do histórico a cada passo; sem panic em erros esperados.

### Passo 4 — Implementar LocalEnvironment

1. Criar processos independentes com `std::process::Command`, cwd e ambiente obedecendo à precedência Python.
2. Mesclar stdout/stderr no ponto de captura com **um único pipe**: criar com `std::io::pipe()` (estável desde Rust 1.87), passar o escritor como `Stdio` de stdout e um `try_clone()` dele como stderr, fechar as cópias do pai e ler o leitor até EOF numa thread. Não concatenar dois buffers ao final, pois isso altera a ordem. Ler até EOF, não até `wait()` retornar, para reproduzir o caso do filho em background que segura o pipe. Um programa mínimo com `std::io::pipe`, `try_clone`, `process_group(0)` e `ExitStatusExt::signal` compilou e rodou com `rustc 1.99.1` em 28/10/2026 (saída mesclada na ordem, `code() == None`, `signal() == Some(15)` para um shell morto por SIGTERM).
3. Lançar o shell com `std::os::unix::process::CommandExt::process_group(0)` e, no timeout, `nix::sys::signal::killpg(pid, SIGKILL)` sobre o grupo; matar apenas o shell pai é insuficiente. `process_group(0)` faz `setpgid` (novo grupo, mesma sessão), enquanto `start_new_session=True` faz `setsid` (nova sessão e grupo); para `killpg` no líder o efeito é o mesmo, e a diferença de sessão (HUP, terminal controlador) fica registrada como ADR porque `setsid` via `pre_exec` exigiria `unsafe`. O timeout é um **prazo absoluto desde o spawn**, como em `communicate(timeout=...)`: a thread leitora lê até EOF e o pai espera o resultado até o deadline; nunca medir ociosidade entre bytes, pois um comando que escreve sem parar por 60 s com timeout 30 s precisa expirar aos 30 s. Após o prazo: `killpg`, `wait()`, devolver a saída parcial já lida. O shell é `/bin/sh -c <comando>`, como `shell=True` no POSIX.
4. Definir retorno por sinal (Python entrega `-N` para sinal N; `ExitStatus::code()` é `None` e exige `ExitStatusExt::signal()`), falha ao criar processo, diretório inexistente, timeout e UTF-8 inválido (`String::from_utf8_lossy`, que substitui como `errors="replace"`). O resultado de erro é `returncode: -1`, `exception_info: "An error occurred while executing the command: ..."` e `extra.{exception_type, exception}`; `exception_type` usa o nome Python (`TimeoutExpired`, `FileNotFoundError`, `NotADirectoryError`) como metadado de compatibilidade, sem fingir traceback.
5. Implementar marcador de submissão com testes de espaços iniciais, CRLF, linha sem newline, marcador no meio do output e retorno não zero.
6. Executar comandos de teste apenas em diretórios temporários. Definir SOs suportados: Linux/macOS POSIX no início; Windows fica explicitamente fora desse gate.

**Aceite:** testes isolados de `LocalEnvironment` (cwd/env/precedência, retorno, sinal, stream mesclado, UTF-8, marcador, timeout) passam, sem depender do comparador do passo 7; dois casos de filho em background são obrigatórios: (a) filho que herda o stdout → `TimeoutExpired`, saída parcial preservada, grupo inteiro morto; (b) filho com stdio redirecionado → comando retorna 0 imediatamente e o descendente **sobrevive**, como na referência; nenhuma persistência indevida de cwd/env entre ações; arquivos persistem como no Python.

### Passo 5 — Implementar DefaultAgent, configuração e templates

1. Reproduzir `run()`/`step()` como no STUDY §2: renderizar system e instance **uma vez, antes do loop** (falha aqui não salva trajetória); depois, a cada iteração, `step()` = verificar limites → `n_calls += 1` → consultar → somar custo e acrescentar `assistant` → executar ações em sequência → formatar observações; três saídas do passo: limpo (zera erros consecutivos), `FormatError` (cobra, conta, acrescenta mensagens e talvez `exit RepeatedFormatError`), interrupção com mensagens (`Submitted`, `LimitsExceeded`, `TimeExceeded`: só acrescenta) ou erro inesperado (acrescenta `exit` diagnóstico e propaga); `save` roda sempre após a tentativa, e só então se testa `messages[-1].role == "exit"`. Uma submissão interrompe `execute_actions` sem a observação normal.
2. Preservar o caminho excepcional de erro de formato, custos, contadores e mensagens; separar erros inesperados de término normal.
3. Reproduzir busca de YAML e merge de specs; testar false, zero, null, listas substituídas, objetos mesclados, chave vazia e campos desconhecidos. Os defaults das structs Rust são os do Pydantic, não os dos YAMLs; sem `deny_unknown_fields`:

| Campo | Default Pydantic | `default.yaml` | `mini_textbased.yaml` |
|---|---|---|---|
| `agent.step_limit` | 0 | 0 | 0 |
| `agent.cost_limit` | 3.0 | 0.0 (desligado) | 3.0 |
| `agent.wall_time_limit_seconds` | 0 | ausente | ausente |
| `agent.max_consecutive_format_errors` | 3 | ausente | ausente |
| `agent.output_path` | None | ausente | ausente |
| `agent.mode` | não existe em `AgentConfig` | ausente | `confirm` (ignorado) |
| `environment.{cwd, env, timeout}` | `""`, `{}`, 30 | só `env` | só `env` |
| `model.action_regex` / `format_error_template` | regex padrão / template curto | — / template longo | — / template longo |
| `model.cost_per_call` (determinístico) | 1.0 | — | — |

4. Validar o motor de template com os arquivos reais `default.yaml` e `mini_textbased.yaml`. Recursos efetivamente usados: `{{ var }}`, `{% if %}`/`{% else %}`, `{% set %}`, teste `is defined`, operador `in [..]`, filtro `length`, slices de string `[:5000]`/`[-5000:]`, controle de whitespace `{%- -%}` e acesso a atributo (`output.output`). Verificar no MiniJinja slices negativos e contagem por code point; se algum recurso faltar, registrar ADR em vez de reescrever os templates. Comparar prompts byte a byte, nos três contextos descritos no STUDY §7, cada um com seu golden: system/instance (contexto completo do agente, cujo merge interno do ambiente é config → `platform.uname()` → `os.environ` → kwargs), observação (contexto do agente mais `output=`), erro de formato (apenas `actions`, `error` e `finish_reason`). Fixtures congelam `uname` e `environ` por injeção, senão o prompt depende da máquina. Testar colisões: env var `system` sobrescrevendo a plataforma e chave `output` no contexto, que no Python é erro de argumento duplicado. `get_template_vars` usa `model_dump()` sem `mode="json"`, portanto `output_path` chega ao template como objeto `Path`, não string.
5. Testar truncamento de observações em torno de 10.000 caracteres, inclusive Unicode multibyte; contar caracteres como a referência, não bytes.
6. Cobrir falha ao salvar e precedência entre erro original e erro de persistência. Melhorias como gravação atômica devem ter ADR e preservar o contrato externo definido.

**Aceite:** loop testado com o modelo e ambiente roteirizados mínimos do passo 3 (sequência de mensagens, custos, contadores e status de saída conferidos contra valores derivados à mão do código Python); templates completos do recorte passam, sem normalização de whitespace. A comparação automática com o Python fica para o passo 7.

### Passo 6 — Modelo determinístico e oráculo diferencial

1. Reutilizar conceitualmente `models/test_models.py`, mantendo o runner externo à referência. `DeterministicModel` devolve mensagens já estruturadas: **ele sozinho não testa parsing da resposta textual**. O double Rust reproduz `cost_per_call` (estatística compartilhada) separado de `extra.cost` (custo do agente), as ações especiais `/sleep N` e `/warning msg` (avançam sem contar chamada) e `raise` como tag JSON com o nome da exceção; esgotar `outputs` é erro inesperado, como o `IndexError` Python.
2. Criar dois conjuntos: respostas estruturadas para o loop; conteúdo bruto para o parser e adaptador textual. Simular também resposta faturada inválida para verificar cobrança e persistência.
3. Definir fixtures JSON com tarefa, config, sequência de respostas, custo, saídas do ambiente, relógio e resultado esperado. Eventos de erro devem ser tags JSON, não objetos de exceção Python serializados. `tests/test_data/local.traj.json` serve apenas como roteiro de respostas e observações esperadas: é uma lista de mensagens com prompts de uma versão anterior dos templates, e `update_outputs_local.py` não roda mais com a `DeterministicModelConfig` atual (STUDY §10).
4. O runner Python adapta a fixture aos componentes originais; o Rust consome a mesma fixture. Capturar pedidos enviados ao modelo e ações passadas ao ambiente, além da trajetória final.
5. Casos de processo real usam cópias independentes do mesmo diretório inicial. Nunca rodar Python e Rust sequencialmente sobre o mesmo diretório mutável.
6. Falhar se respostas ou outputs roteirizados sobrarem/faltarem. Evita testes que passam sem executar o cenário inteiro.
7. As estatísticas compartilhadas são injetadas por cenário; `cargo test` roda em paralelo e um singleton de processo faria cenários dividirem orçamento, como o `reset_global_stats` do Python precisa contornar. Se existir algum fallback global, seus testes usam `--test-threads=1` ou `serial_test`.

**Aceite:** runner repetível offline, sem shell no conjunto puramente roteirizado e sem rede; diferenças reportadas por caminho JSON, com entradas e versões preservadas.

### Passo 7 — Serialização e comparação das saídas

O envelope é `recursive_merge(agent_data, model.serialize(), env.serialize(), *extra_dicts)`, onde `agent_data` tem `info.model_stats.{instance_cost, api_calls}`, `info.config.{agent, agent_type}`, `info.mini_version`, `info.exit_status`, `info.submission` (os dois últimos lidos do `extra` da última mensagem, `""` se ausentes), `messages` e `trajectory_format: "mini-swe-agent-1.1"`; o modelo contribui `info.config.{model, model_type}` e o ambiente `info.config.{environment, environment_type}`. As configs entram via `model_dump(mode="json")` (Path → string, None → null). `save` cria os diretórios pai e grava `json.dumps(data, indent=2)`. Golden files vêm de `DefaultAgent.save()` no oráculo identificado; `local.traj.json` é uma lista de seis mensagens com templates antigos e não serve de golden.

O comparador terá três resultados separados:

1. **Protocolo:** prompts, respostas, ações, observações e ordem coincidem.
2. **Estado:** custos, número de chamadas, término, submissão e snapshots intermediários coincidem.
3. **Processo/artefatos:** stdout, stderr, exit code e diff de arquivos coincidem segundo o contrato da CLI/ambiente.

Política de normalização:

- Comparar o JSON **interpretado**, nunca os bytes: `json.dumps(indent=2)` escapa não-ASCII (`ensure_ascii=True`) e o `serde_json` não; separadores e indentação também podem diferir sem violar o contrato. Ordenar chaves apenas para exibir diff; manter ordem de arrays.
- Congelar timestamps quando possível; caso contrário, normalizar somente caminhos aprovados como `messages[*].extra.timestamp`, preservando presença e tipo (float em segundos, como `time.time()`; `elapsed_seconds` é inteiro).
- Mapear caminhos temporários e nomes de classes em `info.config.*_type` explicitamente. Comparar os valores originais contra os esperados antes do mapeamento.
- Versão Rust e origem Python devem ter metadados próprios; não declarar que o Rust é a versão Python. Registrar mapeamento de `mini_version` para compatibilidade.
- Tracebacks nativos diferem: validar presença/categoria e normalizar apenas o campo diagnosticamente não portátil. Não apagar `extra` inteiro.
- Strings de prompts, comandos, output e submissão são exatas. Não aplicar strip ou remover linhas para “corrigir” divergências.
- Custo deve ter política explícita: comparação exata em fixtures controladas e tolerância pequena justificada para floating point calculado; testar limiares sem arredondar antes de decidir término.
- Campo ausente, null, string vazia e zero continuam distintos. JSON inesperado deve falhar, não ser ignorado.

Testar o próprio comparador com mutações: alterar um comando, uma mensagem, custo, campo ausente e ordem deve falhar; alterar exclusivamente timestamp autorizado deve passar. Golden files só podem ser atualizados conscientemente a partir do Python identificado, com revisão do diff.

Interface **planejada**, a implementar antes de usar:

```bash
.venv-reference/bin/python parity/python_runner.py --fixture parity/fixtures/success.json --output reports/python.json
cargo run --manifest-path rust-mini-swe-agent/Cargo.toml --bin parity-runner -- --fixture parity/fixtures/success.json --output reports/rust.json
.venv-reference/bin/python parity/compare.py reports/python.json reports/rust.json
```

`parity-runner` será um alvo auxiliar, não a interface de produção. O comparador deverá retornar zero para equivalência, um para diferença e outro código documentado para erro de execução.

**Aceite:** o comparador rejeita cada mutação listada (comando, mensagem, custo, campo ausente, ordem) e aceita apenas o timestamp autorizado; ao menos uma fixture roteirizada produz envelope Python e Rust equivalentes segundo a política acima; os campos `*_type` e `mini_version` do Rust estão documentados com o mapeamento usado.

### Passo 8 — BDD/Cucumber e CLI mínima

Criar cenários Gherkin em `parity/features/`, executados por um alvo de teste Rust com Cucumber. Usar adapters para as mesmas fixtures Python; não duplicar expectativas manualmente em duas suítes. Confirmar a API e versão da crate Cucumber na implementação. BDD documenta comportamento público; testes unitários continuam cobrindo detalhes do parser e merge.

```gherkin
Feature: Conclusão e limites do agente textual
  Scenario: Submissão encerra a execução
    Given uma tarefa com respostas determinísticas
    And o comando retorna o marcador de submissão e "resolvido" com código zero
    When executo o cenário nos harnesses Python e Rust
    Then ambos terminam com status "Submitted"
    And a submissão e o histórico são equivalentes
    And nenhuma consulta adicional é realizada

  Scenario: Limite é verificado antes da próxima consulta
    Given um limite de 1 chamada e uma resposta que não submete
    When executo o cenário nos harnesses Python e Rust
    Then exatamente 1 consulta é realizada em cada harness
    And ambos terminam com status "LimitsExceeded"
```

Adicionar cenários para timeout com filho, shell sem estado persistente, erro de formato consecutivo, custo no limiar, template inválido, saída Unicode e gravação em falha.

A CLI mínima aceita tarefa, config repetível, modelo, ambiente, limites e arquivo de saída. Defaults devem selecionar o agente não interativo e caminho textual. O `mini` Python usa agente interativo por padrão e, sem `model_class`, o modelo de tool-calling: o runner de comparação instancia `DefaultAgent` e o modelo textual/determinístico diretamente, sem passar pelo `mini`. Documentar correspondência e divergências de flags/exit codes, sem prometer interface completa de `mini-extra`. Política de interrupção: no Python, `KeyboardInterrupt` não passa pelo `except Exception`, mas o `finally` ainda salva a trajetória; a CLI Rust instala um handler de SIGINT que salva o estado corrente e sai com código documentado, em vez do abort padrão sem save. Exit code: 0 para término normal (qualquer `exit_status`), diferente de zero para erro propagado.

**Aceite M1:** os gates P01–P18 do STUDY §10 estão fechados, cada um apontando para o teste ou fixture que o cobre; as features Gherkin obrigatórias são as nominadas acima (submissão, limite antes da consulta, timeout com filho, shell sem estado, erro de formato consecutivo, custo no limiar, template inválido, saída Unicode, gravação em falha), sem skip silencioso; CLI gera trajetória legível; relatório indica exatamente o que foi testado.

### Passo 9 — Um provedor real (M2)

1. Escolher e registrar provedor, modelo e formato de API; manter ações textuais de shell.
2. Implementar autenticação por variável de ambiente, payload, timeout, retry limitado, erros HTTP, respostas vazias/truncadas, usage e custo. Não registrar tokens ou payloads sensíveis.
3. Usar servidor HTTP de teste e respostas gravadas/sanitizadas para comparar o adaptador com o caminho Python escolhido. LiteLLM agrega comportamento que não é automaticamente reproduzido por um cliente HTTP.
4. Testar separadamente 401/403, 429, 5xx, timeout e JSON inválido. Congelar tempo/backoff nos testes; distinguir tentativas HTTP de consultas contabilizadas pelo agente.
5. Executar smoke test real opt-in com orçamento explícito e diretório descartável. Não exigir texto idêntico de duas chamadas reais, mesmo com temperatura zero.
6. Avaliar tarefas fixas com número de amostras, versão do modelo, custo, latência e taxa de sucesso. Separar variação do modelo de regressão do harness.

**Aceite M2:** contrato HTTP gravado passa, teste real identificado funciona e custos estão visíveis. Isso comprova apenas a integração selecionada.

### Passo 10 — Docker (M3) e expansão controlada (M4)

Docker tem contrato próprio em `spec/docker.md`: imagem por digest, criação, cwd, `env` versus `forward_env`, argumentos, interpretador (`bash -lc` na referência), timeout de pull/exec, output e limpeza. Testar indisponibilidade do daemon, startup parcial, comando com filho, timeout, interrupção e remoção do container. Um timeout do cliente `docker exec` não prova que o processo dentro do container foi encerrado.

Separar testes do comando Docker roteirizado dos testes reais de isolamento. Verificar mounts, acesso a host/socket, rede, usuário e limites de recursos; um container com mounts amplos não é isolamento suficiente por si só. Perfis mais restritos são decisões de segurança documentadas, não paridade presumida. Limpeza explícita e verificável deve ser a via principal; `Drop` apenas fallback, sem panic nem dependência de async no destrutor.

Só depois ampliar provedores, ambientes, interface interativa e benchmarks. Para cada item: contrato → fixture Python → adaptação Rust → testes diferenciais → teste integrado → relatório. Execução em lote exige limites de concorrência, isolamento por tarefa, escrita coordenada de resultados e retomada. Tool-calling/multimodal permanecem excluídos até mudança de escopo.

### Passo 11 — Qualidade, CI e evidências do estudo

Pipeline proposto:

```bash
cargo fmt --manifest-path rust-mini-swe-agent/Cargo.toml --check
cargo clippy --manifest-path rust-mini-swe-agent/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust-mini-swe-agent/Cargo.toml --locked
cargo doc --manifest-path rust-mini-swe-agent/Cargo.toml --no-deps
```

Adicionar jobs de Python/oráculo e comparação offline, BDD, Linux/macOS e MSRV. Docker e provedores reais ficam em jobs separados com pré-requisitos explícitos. Uma suíte pulada por falta de credenciais/daemon é **não verificada**, não aprovada. Publicar os diffs de paridade como artefatos de CI, com segredos removidos.

Medir startup, memória e throughput do loop com modelo/ambiente simulados em release; medir latência ponta a ponta e custos separadamente com API real. Fixar hardware, versões, workload e repetições. Não prometer que Rust acelera uma execução dominada pelo tempo do LLM.

## 5. Aplicação de rust-skills

A skill foi consultada para este plano. Aplicar regras por necessidade, não todas as otimizações de forma automática:

| Regras | Aplicação neste projeto |
|---|---|
| `own-borrow-over-clone`, `own-slice-over-vec` | Emprestar histórico; mover mensagens novas; sem clones grandes por conveniência |
| `err-result-over-panic`, `err-thiserror-lib`, `err-context-chain` | Falhas de processo/rede/config com Result e causa preservada |
| `type-no-stringly`, `pat-exhaustive-enum` | Estados e términos tipados, conversão explícita para JSON de compatibilidade |
| `serde-default-compat`, `serde-flatten` | Preservar extras e distinções do schema; defaults só onde há contrato |
| `proj-lib-main-split`, `proj-flat-small`, `proj-msrv-declare` | Um pacote simples, lib testável, main fino e toolchain reproduzível |
| `test-mock-traits`, `test-integration-dir` | Dependências roteirizadas e cenários de ponta a ponta |
| `obs-structured-fields`, `obs-no-sensitive-data` | Run ID, fase, duração e custo em logs estruturados sem segredos |
| `perf-profile-first`, `anti-premature-optimize` | Medir antes de Arc, pools, cache ou coleções especializadas |

Evitar `unwrap`/`expect` em caminhos normais; preferir APIs seguras para processos. Não introduzir `unsafe`, generics excessivos ou async apenas para demonstrar domínio técnico. Se async entrar depois, garantir cancelamento, limites de concorrência e ausência de locks mantidos através de await.

## 6. Critério de conclusão e checklist

- [ ] Referência e ambiente Python reproduzíveis, origem identificada.
- [ ] Escopo e contratos aprovados no repositório; exclusões transparentes.
- [ ] Pacote Rust criado, build e verificações de qualidade passando.
- [ ] LocalEnvironment correto, incluindo timeout e descendentes.
- [ ] Loop, templates, parsing, custos e limites equivalentes.
- [ ] Comparador testado contra diferenças reais; normalizações restritas.
- [ ] Trajetórias e snapshots intermediários equivalentes nas fixtures.
- [ ] Cenários Cucumber passando junto com testes Rust/Python.
- [ ] CLI mínima e instruções reproduzíveis publicadas (M1).
- [ ] Provedor real escolhido validado com contrato e smoke test (M2).
- [ ] Docker validado separadamente ou explicitamente pendente (M3).
- [ ] Expansões com cobertura por componente, sem alegar paridade integral (M4).
- [ ] README atualizado com comandos executados, resultados, limitações e links para evidências.

Cada etapa pode ser um PR pequeno. A descrição deve explicar mudança de comportamento, referência Python e validação. O portfólio demonstra engenharia pelo vínculo entre requisito, decisão, teste e evidência; não pelo número de dependências ou por resultados ainda não medidos.
