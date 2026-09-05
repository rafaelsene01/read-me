---
name: spec-loop
description: Varre as specs do ReadMe, reconcilia STATE.md/AGENTS.md/ROADMAP.md com o que o código realmente diz, e então executa o que falta despachando subagents em paralelo — cada execução seguida de uma validação adversarial por um agente diferente, com correção em loop. Mantém um journal que permite retomar numa sessão nova. Use quando o pedido for "veja as specs e execute o que falta", "continue de onde parou", "valide o que foi implementado" ou variações.
---

# spec-loop — execução e validação orquestradas

Esta skill é **orquestradora**. Quem a executa **não edita código, não roda gate, não dirige o app**: planeja, despacha subagents, lê o que eles devolvem, reconcilia a documentação e decide o próximo passo. Toda mudança de arquivo passa por um subagent.

A razão não é estética. O contexto do orquestrador precisa durar a run inteira — se ele gastar o contexto lendo diffs e saída de teste, ele perde a única coisa que só ele tem: a visão de quais tasks colidem, o que já foi validado e o que ficou pendente. Um agente que implementa perde essa visão em três tasks.

**A única exceção — e ela tem fronteira exata:** o orquestrador edita a documentação de **nível projeto** — `.specs/project/STATE.md`, `ROADMAP.md`, `.specs/codebase/*`, `AGENTS.md` e o journal. É bookkeeping, é o produto do próprio raciocínio dele, e delegar isso significaria delegar a memória da run.

A documentação de **nível feature** (`.specs/features/<nome>/spec.md`, `design.md`, `tasks.md`) é escrita pelo **agente que planeja aquela feature**, não pelo orquestrador. Ela é o produto do planejamento, não bookkeeping — e centralizá-la faria o orquestrador virar gargalo de digitação enquanto dois agentes esperam. *(Fronteira corrigida na run 001, quando a decisão de pagar C-03 e C-04 exigiu duas specs novas e a regra original não dizia de quem era o trabalho.)*

---

## O que este projeto já ensinou, e que esta skill existe para não repetir

Leia isto antes de despachar qualquer coisa. Cada linha custou uma sessão.

**"Compila" não é "verificado".** É a regra central do `AGENTS.md`. As três últimas sessões de UAT acharam três defeitos reais que passavam por todos os gates automatizados (AD-046: a DLL podada; AD-047: a memória morta pelo funil; AD-050: o documento irrelevante deslocando a resposta). Um agente que reporta "implementado, `cargo test` passou" não provou nada sobre o app.

**A documentação mente com autoridade.** Já houve ROADMAP dizendo "não implementado" sobre algo pronto no mesmo dia; três todos mandando verificar código removido há um milestone; e o `AGENTS.md` — o arquivo que manda ler o estado — descrevendo um estado oito ADs atrasado, com um número de migração já gasto que causaria colisão silenciosa. **Por isso a Fase 0 vem antes da Fase 1:** planejar sobre documentação não reconciliada é planejar trabalho que não existe.

**Número copiado para a prosa envelhece.** Contagem de teste, número de migração, tamanho de artefato. O baseline de testes ficou em 146 por várias sessões enquanto a suíte crescia até 177 — e ele é usado como gate. Todo número que esta skill escreve num documento tem que vir de uma medição daquela run.

**Um `- [ ]` dentro de uma task concluída é critério de aceitação, não pendência.** Um grep ingênuo por `- [ ]` em `.specs/` devolve ~337 itens; o número real de tasks abertas é próximo de zero. Ver a Fase 1.

---

## Fase 0 — Reconciliar antes de planejar

Objetivo: fazer `.specs/` e `AGENTS.md` descreverem o projeto que existe hoje. Nada é executado antes disto.

**O orquestrador despacha um subagent de auditoria** (`Explore` serve — é leitura) com esta missão:

1. Ler `.specs/project/STATE.md` (topo + Todos + Active Blockers), `ROADMAP.md`, o `**Status:**` de cada `.specs/features/*/tasks.md`, **e os 7 arquivos de `.specs/codebase/`**.

   ⚠️ **Não pule `.specs/codebase/`.** A run 001 varreu só `STATE`/`ROADMAP`/`tasks.md` e achou 5 divergências; quando um segundo agente varreu `codebase/`, achou **24** — incluindo um `CONVENTIONS.md` em que praticamente todo exemplo de UI/store/API apontava para uma feature removida havia um milestone, e um `ARCHITECTURE.md` afirmando *"não há versionamento nem migração destrutiva"* sobre um banco com 8 migrações versionadas. **É o pior lugar para haver mentira**, porque o `AGENTS.md` manda ler esses arquivos para saber como escrever código: um agente que obedecer copia exemplos que não compilam.
2. Para **cada** afirmação sobre o código, conferir por grep no código: o símbolo citado existe? o arquivo existe? o comando ainda é registrado?
3. Caçar especificamente o padrão **número copiado para a prosa**:
   - contagem de testes no `AGENTS.md` vs. `cargo test --lib` medido agora;
   - "a próxima migração é a N" vs. a lista `MIGRATIONS` em `src-tauri/src/db.rs`;
   - tamanhos de artefato, contagens de chave i18n, contagens de task.
4. Devolver uma **tabela de divergências**: afirmação, onde está escrita, o que o código diz, evidência (comando + saída).

O orquestrador então aplica as correções em `.specs/` e `AGENTS.md` — ele mesmo, porque é bookkeeping.

**Regras de correção**, herdadas da regra 4 de `.claude/rules/spec-driven-changes.md`:

- Requisito ou todo que perdeu o objeto é **riscado com o motivo**, nunca apagado. Nomeie a spec que o revogou (`SELF-01`) e a AD (`AD-042`). O histórico do "por quê" tem valor.
- Marque como **"sem objeto, não verificado"**, nunca como "feito". Não foi verificado; deixou de haver o que verificar. A distinção é a mesma entre "compila" e "verificado".
- Se o escopo de um milestone mudou, atualize `ROADMAP.md` e o topo do `STATE.md` juntos.

**Saída da fase:** a tabela de divergências entra no journal, com o que foi corrigido e o que ficou pendente.

---

## Fase 1 — Inventário e classificação

### Como ler o estado de uma task (não existe fonte única)

Medido neste repositório: **119 headers de task, 59 linhas de `Execution Log`, 1 marcador `⚠️ EXIGE O USUÁRIO`** — embora o `STATE.md` conheça ~6 itens que só o mantenedor faz. As convenções são inconsistentes por herança. Consulte nesta ordem e pare no primeiro que responder:

1. **Header:** `^### T\d+: <título>` em `tasks.md`. `[P]` no título = o autor julgou paralelizável.
2. **Tabela `## Execution Log`:** linha `| T<n> | ✅ | evidência |`. Glifos em uso: `✅` concluída, `⏳` parcial, `⚠️` com ressalva.
3. **Prosa `**Status:**`** no topo do `tasks.md` — costuma dizer o que a tabela não diz ("sobram dois itens da T9 que exigem clique").
4. **`STATE.md`**, seção Todos — é onde moram as pendências que atravessam features.

⚠️ **Os `- [ ]` dentro do corpo de uma task são critérios de aceitação.** Só conte como pendência os de uma task **não** marcada `✅`. Ignorar isto transforma ~337 checkboxes em pendências fantasma.

⚠️ **Nada disso é confiável sozinho.** Um status só vira verdade depois de conferido contra o código — é o que a Fase 0 produziu.

### Classificação

Cada item aberto recebe exatamente um rótulo:

| Rótulo | O que é | Quem executa |
| --- | --- | --- |
| `code` | implementável e provável por teste | subagent implementador |
| `uat-agent` | exige o app rodando, mas um agente consegue dirigir | subagent de UAT, **nunca em paralelo** |
| `human-only` | exige máquina, conta ou permissão que nenhum agente tem | **ninguém — só relate** |
| `moot` | o código que ele testava não existe mais | Fase 0 já deveria ter riscado |
| `blocked` | sem solução conhecida, com medição que descarta a óbvia | só relate, com o número |

**`human-only` neste projeto** (não tente, não delegue, não invente um jeito): instalar sem direitos de administrador; aplicar update de verdade; publicar release (`workflow_dispatch` é do mantenedor, por regra do `AGENTS.md`); qualquer coisa que exija runner Linux; assinatura de código; qualquer coisa que exija uma segunda máquina ou rede desligada.

**Se o inventário der zero itens `code` e `uat-agent`: pare e relate.** Não invente refatoração, não "melhore" o que ninguém pediu, não converta `blocked` em tentativa. Um relatório dizendo "não há o que executar, e eis por quê" é o resultado correto — este projeto já chegou nesse estado.

---

## Fase 2 — Portão de decisões (todas as perguntas antes de qualquer execução)

A regra 1 de `.claude/rules/spec-driven-changes.md` manda perguntar **antes de planejar**, não no meio: *"uma pergunta feita depois de três arquivos editados custa o retrabalho dos três."* Esta fase é esse portão, e ela é **bloqueante** — nenhum subagent de execução é despachado enquanto houver pergunta em aberto.

### 1. Levantar todas de uma vez

Varra o inventário e junte **tudo** que exige o usuário:

- **Open Questions** ainda abertas nos `design.md` das features envolvidas;
- itens `blocked` onde existe mais de um caminho e a escolha é de produto, não técnica;
- tasks cuja spec admite **duas leituras** que levariam a trabalhos diferentes;
- qualquer coisa que pareça **contradizer uma AD registrada** — cite a AD na pergunta;
- mudança que implica **remover algo que hoje funciona**, sem estar dito se a remoção é intencional;
- mais de um lugar razoável para a mudança morar (`runtime/` vs `providers/`, store vs componente).

**Não pergunte o que o repositório responde.** Leia `.specs/` e o código primeiro. Pergunta boa é a que só o usuário pode responder — e este projeto pune a preguiça aqui: a resposta costuma estar numa AD.

### 2. Escrever a lista no journal **antes** de perguntar

A lista inteira vai para o journal antes da primeira pergunta. Uma run interrompida no meio do interrogatório precisa que o disco já saiba quais perguntas faltavam — senão a sessão seguinte recomeça o levantamento e faz o usuário responder de novo o que ele já respondeu.

### 3. Perguntar uma de cada vez

Uma pergunta por vez, com as opções reais e o **trade-off de cada uma**. Quando houver número que sustente a escolha (uma distância medida, um tamanho, uma contagem), traga o número na pergunta — foi assim que a AD-050 descartou o limiar absoluto: o usuário escolheu entre quatro opções apresentadas com as medições ao lado.

Registre **cada resposta assim que ela chega**, em dois lugares:
- no journal, com a data;
- no `context.md` da feature (é o arquivo que existe para guardar decisão de usuário em área cinzenta) — e, se a escolha for não óbvia, uma AD nova no `STATE.md` com o motivo e o trade-off.

### 4. Só então executar

Com a lista esgotada, siga para a Fase 3 e vá até o fim sem voltar a perguntar.

**Se uma decisão nova aparecer no meio da execução** — acontece, e a AD-050 é exemplo: o defeito do documento irrelevante só apareceu com o app rodando —, não adivinhe e não trave a onda inteira. **Estacione aquela task**, anote a pergunta no journal, deixe as outras da onda terminarem, e pergunte na fronteira da onda. Uma task parada é barata; uma onda parada por uma pergunta de uma task, não.

---

## Fase 3 — Ondas de paralelismo

Paralelizar é o objetivo, mas colisão custa mais do que a economia. Uma task entra na onda com outra **só se todas as condições valerem**:

1. **Conjuntos de arquivos disjuntos.** Leia o campo `Where`/`Files` da task. Interseção não vazia → ondas diferentes.
2. **Sem dependência declarada** (`Depends on`) ainda aberta.
3. **Nenhuma é `uat-agent`.** UAT é sempre sozinha — o app é instância única, a porta 1420 é única (dois incidentes registrados de servidor Vite órfão travando o restart), e a UAT toca os dados reais do usuário.
4. **Nenhuma toca território compartilhado conhecido:**

| Território | Por que colide |
| --- | --- |
| `src-tauri/src/db.rs` — lista `MIGRATIONS` | duas migrações com o mesmo número **não quebram a compilação**; a segunda só nunca roda, porque o `user_version` já passou. Falha silenciosa em produção |
| `src/i18n/locales/en.json` + `pt.json` | paridade obrigatória (**148/148** hoje); dois agentes editando geram divergência de chave |
| `src/types.ts` | espelha structs Rust **à mão**, sem geração |
| `runtime/process.rs`, `runtime/detect.rs` | tocados por `sidecar-lifecycle` **e** `self-contained-runtime` — o ROADMAP registra explicitamente "não em paralelo por dois agentes" |

**Teto de 3 subagents de execução simultâneos.** Não é limite técnico: acima disso o orquestrador não consegue validar com cuidado o que volta, e validação superficial é o modo de falha que esta skill existe para evitar.

Despache as tasks de uma onda em **uma única mensagem** com múltiplas chamadas do Agent — é o que as faz rodar de fato em paralelo.

---

## Fase 4 — O loop de execução e validação

Para cada task da onda:

```
        ┌──────────────┐
        │ IMPLEMENTADOR│  subagent novo, contexto mínimo
        └──────┬───────┘
               ↓ devolve: arquivos, gates, desvios
        ┌──────────────┐
        │  VALIDADOR   │  subagent DIFERENTE, missão adversarial
        └──────┬───────┘
         aprovado? ──sim──→ journal + próxima
               │ não
               ↓
        ┌──────────────┐
        │  CORRETOR    │  recebe só a lista de defeitos
        └──────┬───────┘
               ↓
          revalidar (validador novo)  ── até 3 ciclos ──→ escalar ao usuário
```

**O validador nunca é o implementador, e nunca recebe o contexto dele.** Um agente que acabou de escrever o código conhece a intenção e lê o próprio trabalho com ela na cabeça; ele valida o que quis fazer, não o que fez. O validador recebe o requisito e o diff, e a missão de **falsificar**.

**Teto de 3 ciclos por task.** Se um defeito sobrevive a três correções, o problema é de spec ou de desenho, não de código — pare, escreva no journal o que cada ciclo tentou, e traga ao usuário.

Os briefs exatos de cada papel estão em [references/agent-briefs.md](references/agent-briefs.md). Não improvise: o que o subagent recebe é o que decide a qualidade do que volta.

---

## Fase 5 — Journal, e retomar numa sessão nova

O journal é o que torna a run interrompível. **Escreva depois de cada task, não no fim** — uma run que morre no meio precisa que o disco já saiba o que aconteceu.

Local: `.specs/runs/<NNN>-<YYYY-MM-DD>/JOURNAL.md`.

```markdown
# Run NNN — <data>

**Status:** em andamento | pausada | concluída
**Orquestrador:** sessão iniciada em <hora>

## Fase 0 — Divergências encontradas
| Afirmação | Onde | O que o código diz | Corrigido? |

## Inventário
| Item | Feature | Classificação | Onda |

## Decisões pendentes do usuário (Fase 2)
Escrito ANTES da primeira pergunta. Uma run interrompida no interrogatório
retoma daqui sem refazer o levantamento nem repetir pergunta já respondida.

| # | Pergunta | Por que só o usuário responde | Resposta | Data |

## Execução
| Task | Onda | Implementador | Gates | Validação | Ciclos | Status |

## Gates medidos nesta run
- `cargo test --lib`: <n> passando / <n> falhas / <n> ignorados
- `npm run build`: <resultado>
- `npm run test:scripts`: <n>

## Working tree no último checkpoint
<saída de `git status --short`>

## Não verificado
<o que ficou sem prova de execução — obrigatório, mesmo que vazio explique>
```

**Retomar:** ler o journal mais recente com `Status: em andamento` ou `pausada`, rodar `git status --short`, comparar com o checkpoint registrado. Divergência entre os dois significa que algo mudou fora da skill — reconcilie antes de continuar. Depois siga da primeira task sem `Status: concluída`.

**Sem commits.** O `AGENTS.md` é explícito: o padrão é deixar as mudanças no working tree, e commitar só quando o usuário pedir. A retomada se apoia no journal + `git status`, não no histórico. Ao fim da run, **sugira** a mensagem de commit; não execute.

---

## Nunca faça (herdado do AGENTS.md — vale para todo subagent despachado)

- **Não toque nos dados reais do usuário.** A pasta-base fica fora do repositório e tem as conversas dele. Para validar migração ou banco vetorial: **copie** para o scratchpad, trabalhe na cópia, apague. O original nunca é aberto para escrita por um teste.
- **Não commite** sem o usuário pedir. **Não faça force-push nem reescreva `master`** — desfazer é `git revert`.
- **Não dispare release.** `workflow_dispatch` é manual de propósito.
- **Não deixe arquivo de diagnóstico temporário no repositório.** Já houve um órfão com caminhos absolutos da máquina do usuário meses depois da investigação.
- **Não invente ID de requisito.** Se o requisito não existe, ele precisa existir na spec primeiro.

---

## Relatório final

O `AGENTS.md` manda relatar o que foi **executado**, não o que deveria funcionar. Esta skill acrescenta a origem de cada evidência:

- **o que rodou**, com número medido (contagem de teste, tempo, bytes) — nunca adjetivo;
- **quem provou o quê**: gate automatizado, validador adversarial, ou clique no app. As três têm forças diferentes e o leitor precisa saber qual foi;
- **quais specs foram atualizadas** e o que ficou pendente nelas;
- **quais marcadores `SPEC:`** foram adicionados ou mexidos;
- **o que NÃO foi verificado** — obrigatório, na mesma frase que descreve o que foi feito;
- **o que depende do usuário**, nomeando por que nenhum agente resolve.
