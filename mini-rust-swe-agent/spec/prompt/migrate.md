Você está trabalhando no diretório mini-rust-swe-agent/.

Existe um projeto Python de referência em mini-rust-swe-agent/mini-swe-agent/ (código do mini-swe-agent: agentes com histórico linear, ações independentes via subprocess, apenas bash, sem tool-calling nativo).

Sua tarefa:

1. Crie um novo projeto Rust em mini-rust-swe-agent/rust-mini-swe-agent/ com `cargo new --bin`.
2. Siga as melhores práticas de Rust (ownership claro, Result em vez de panic, testes, sem unsafe desnecessário). Use o skill /rust-skills como referência.
3. Use o código Python como especificação viva. Não copie cegamente; preserve a simplicidade radical do original.
4. Implemente na ordem:
   - Traits core (Model, Environment, Agent)
   - LocalEnvironment (std::process::Command)
   - DefaultAgent com histórico linear e loop simples
   - Modelo mock para testes de paridade
   - Serialização de trajectory (serde_json)
   - CLI mínima
5. Crie ou atualize spec/PLAN.md com o plano detalhado (já existe uma versão inicial; refine conforme necessário).
6. Mantenha README.md atualizado descrevendo o status e as decisões de design.

Critérios de sucesso:
- O agente Rust gera trajectories estruturalmente equivalentes ao Python em casos determinísticos (use mocks) e BDD (Behavior-Driven Development : Cucumber).
- O código permanece legível para quem conhece o original.
- Zero panic em caminhos normais.

Não adicione ferramentas além de bash, não introduza sessões de shell stateful, e não complique o control flow sem necessidade explícita.