**A falha central é que o código local não está no ambiente.** O diretório `mini-rust-swe-agent/mini-swe-agent/` não existe aqui. Qualquer “estudo a fundo” feito sem o tree exato que você tem é uma aproximação baseada no repositório público. Se a sua cópia tem patches, configs diferentes ou commits divergentes, o STUDY.md está desatualizado em relação à sua realidade.

[Certain] Não existe um agente/bot Codex conectado neste contexto. Não consigo “passar” o arquivo para outro modelo de forma automática. A auditoria adversarial abaixo é feita por mim, no mesmo processo. Se você quiser que um Codex externo faça isso, o caminho é copiar o STUDY.md e colar no prompt.

Crie `/spec/STUDY.md` com:
- Separação explícita dos módulos
- Diagramas Mermaid (fluxo principal, ciclo de ação, hierarquia de exceptions, composição de templates)
- Distinção entre o que é essencial e o que é acidental
- Implicações diretas para a migração Rust

### Auditoria adversarial (o que o STUDY.md pode estar errando ou omitindo)

1. **Extração de actions é o ponto cego real.** O diagrama e o texto tratam `message["extra"]["actions"]` como dado. Quem popula isso varia por modelo (`litellm_model`, textbased, response, etc.). Sem ler o parser concreto de pelo menos um model class, o estudo superestima a simplicidade do loop e subestima a fragilidade do parsing.

2. **Cwd e estado de filesystem não são tão limpos quanto o texto sugere.** O `LocalEnvironment` passa `cwd` por chamada, mas não mantém estado entre steps. Um `cd` ou `export` dentro de um comando **não** afeta o próximo. Isso é uma fonte clássica de divergência entre o que o modelo espera e o que o harness faz. O STUDY.md menciona, mas não quantifica o risco.

3. **`Submitted` depende de uma string mágica.** A detecção é literal (`COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` na primeira linha + returncode 0). Qualquer mudança de prompt, formatação ou modelo que altere essa string quebra o término. Isso não está destacado como risco de paridade.

4. **Templates Jinja2 com `StrictUndefined`.** Variáveis faltando explodem. Na migração, se o merge de template vars for incompleto, o comportamento muda silenciosamente ou falha de forma diferente.

5. **GlobalModelStats é side-effect global.** Limites globais via env vars + lock. Em Rust isso vira estado compartilhado explícito ou desaparece. O estudo não força a decisão.

6. **O STUDY.md ainda não confronta o código com o PLAN.md anterior.** Não há checklist de “isto precisa ser idêntico para a comparação de trajectories ser válida”.
