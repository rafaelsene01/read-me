# Quick Task 007: Skill orquestradora de execução e validação de specs

**Date:** 2026-07-27
**Status:** Done (escrita e conferida contra o repositório; **nunca executada**)

## Description

Criar a skill `spec-loop`: varre as specs, reconcilia a documentação contra o
código, levanta as decisões que dependem do usuário e pergunta uma a uma, e só
então executa o que falta **despachando subagents** — cada implementação
seguida de validação adversarial por um agente diferente, com correção em loop,
e um journal que permite retomar numa sessão nova.

Pedido do usuário, refinado em três mensagens durante a execução: (1) a skill
deve ser **orquestradora pura**, sempre mandando subagent para executar ou
validar; (2) decisões que dependem do usuário são levantadas **todas**, e
perguntadas **uma a uma** antes de qualquer implementação.

## Files Changed

- `.claude/skills/spec-loop/SKILL.md` — as 6 fases
- `.claude/skills/spec-loop/references/agent-briefs.md` — os briefs dos 4 papéis
- `AGENTS.md` — ponteiro para a skill na seção "Como este projeto trabalha"
- `.specs/project/STATE.md` — linha na tabela de quick tasks

## As decisões de desenho, e o que as sustenta

**Orquestradora pura, com uma exceção.** O orquestrador não edita código, não
roda gate, não dirige o app. A razão é de contexto, não de estética: ele precisa
durar a run inteira, e a única coisa que só ele tem é a visão de quais tasks
colidem e o que já foi validado. Um agente que implementa perde isso em três
tasks. **A exceção é `.specs/` e o `AGENTS.md`** — bookkeeping é o produto do
raciocínio dele, e delegar seria delegar a memória da run.

**Reconciliar antes de planejar (Fase 0).** Planejar sobre documentação não
reconciliada é planejar trabalho que não existe. Não é hipótese: nesta mesma
sessão, três todos mandavam verificar código removido há um milestone, e o
`AGENTS.md` descrevia um estado oito ADs atrasado — incluindo um número de
migração já gasto, que causaria colisão silenciosa.

**O portão de decisões é bloqueante (Fase 2).** A regra 1 de
`spec-driven-changes.md` já mandava perguntar antes de planejar; a skill torna
isso um passo com estado. A lista inteira vai para o journal **antes** da
primeira pergunta, para que uma run interrompida no meio do interrogatório não
faça o usuário responder de novo o que já respondeu.

**O validador nunca é o implementador.** Quem acabou de escrever o código lê o
próprio trabalho com a intenção na cabeça: valida o que quis fazer, não o que
fez. O brief do validador manda **falsificar**, não conferir.

**Teto de 3 subagents simultâneos e 3 ciclos de correção.** Nenhum dos dois é
limite técnico. Acima de 3 em paralelo o orquestrador não consegue validar com
cuidado o que volta — e validação superficial é o modo de falha que a skill
existe para evitar. Um defeito que sobrevive a 3 correções é problema de spec,
não de código.

## O que foi conferido no repositório, e não presumido

O desenho depende de fatos sobre este repo. Medi cada um:

| Fato | Medição | Consequência no desenho |
| --- | --- | --- |
| Formato de task | **119** headers `### T<n>:`, dos quais **50** com `[P]` | a skill lê o header, não inventa unidade |
| Status de task | só **59** linhas de `Execution Log` (55 ✅, 2 ⏳, 1 ⚠️) | **não há fonte única** — a skill consulta 4 lugares em ordem e confere no código |
| Marcador de "exige o usuário" | **1** ocorrência, contra ~6 itens conhecidos no `STATE.md` | a convenção é inconsistente; a classificação não pode depender dela |
| `- [ ]` em `.specs/` | **~337** | são critérios de aceitação dentro de tasks concluídas. Um grep ingênuo inventaria 337 pendências — está escrito na skill com esse número |
| Território compartilhado | `runtime/process.rs` e `detect.rs` tocados por 2 features; `MIGRATIONS`; `en/pt.json`; `types.ts` | tabela de exclusão de paralelismo |

**A tabela de colisão não foi deduzida do código** — o ROADMAP já registrava
"só não em paralelo por dois agentes" sobre `runtime/`, e a colisão de migração
foi confirmada em `db.rs:164-173` nesta sessão.

## Verification

- [x] Frontmatter YAML válido, `name` casando com o diretório
- [x] Numeração das 6 fases consistente depois da renumeração — todas as
      referências cruzadas conferidas por grep
- [x] O link `references/agent-briefs.md` aponta para arquivo que existe
- [x] Cada número citado na skill medido nesta sessão (119/50/59/337), não
      copiado de outro documento
- [x] As proibições do `AGENTS.md` reproduzidas no contexto comum dos briefs —
      um subagent começa frio e não as conhece
- [x] Gates do repositório, mesmo sem código de produto tocado

## O que NÃO foi verificado

**A skill nunca foi executada.** Nada aqui foi provado rodando: o formato do
journal, a classificação, o gating de paralelismo e os briefs são desenho
conferido contra o repositório, não comportamento observado. Pelo próprio
critério do `AGENTS.md`, isto está no nível de *"YAML validado"* — que é
exatamente a evidência que a L-005 registra como insuficiente, quando o `ci.yml`
foi dado como pronto e falhou na primeira execução real.

**E há uma razão para a primeira execução ser fraca como teste:** hoje o
inventário tende a zero itens `code`. A skill vai exercitar as Fases 0 a 2 e
parar, relatando que não há o que executar. O loop de execução/validação — o
coração dela — só será exercitado quando houver spec nova.

## Commit

pendente — `feat(skills): add spec-loop orchestrator for spec execution`
