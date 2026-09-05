# Quick Task 006: Três todos sem objeto e o cabeçalho do AGENTS.md desatualizado

**Date:** 2026-07-27
**Status:** Done
**Features afetadas:** nenhuma — é saneamento de documentação

## Description

Três itens da lista `## Todos` do `STATE.md` mandam verificar coisas que **não
existem mais no código**, removidas pelo M9 (AD-039/AD-042). E o parágrafo
*"Estado atual"* do `AGENTS.md` — o primeiro que qualquer agente lê, por
instrução do próprio arquivo — descreve o M9 como estando no meio, com o
frontend chamando `list_connections`, `pull_model` e `get_active_pair`. Isso é
oito ADs atrás do estado real.

O `AGENTS.md` manda, na linha seguinte à que ficou errada: *"Não presuma o
estado a partir do código: [...] Quando encontrar essa divergência, corrija o
documento."* Esta task é essa correção aplicada ao próprio arquivo que a exige.

**Por que isso importa mais que a contagem sugere:** um todo que aponta para
código removido não é ruído neutro. Ele infla a lista de pendências e, pior,
manda um leitor futuro procurar um defeito num lugar onde não há código. A
quick task 002 fez exatamente este saneamento nas specs de feature e **não
encostou no `STATE.md` nem no `AGENTS.md`** — é a metade que faltou.

## Files Changed

- `AGENTS.md` — parágrafo "Estado atual" reescrito; baseline de `cargo test`
  corrigido de 146 para o número medido; **número da próxima migração corrigido
  de 8 para 9**
- `.specs/project/STATE.md` — três todos marcados como sem objeto, com a
  evidência de código ao lado; linha da quick task na tabela

## Dois defeitos a mais, achados varrendo o resto do arquivo

O pedido era o parágrafo do topo. Varrer o `AGENTS.md` inteiro custou uma
leitura e achou mais duas afirmações falsas — as duas do mesmo tipo, um número
que envelheceu sem ninguém reconferir:

**O baseline de testes dizia 146; a suíte está em 177.** Esse número não é
decorativo: o próprio arquivo manda justificar todo teste perdido em relação a
ele. Um baseline 31 testes atrasado não detecta perda nenhuma — daria por
normal alguém apagar 30 testes. Medido nesta task (`cargo test --lib`), não
copiado do `STATE.md`.

**"A próxima migração é a 8" — e a 8 já tinha sido gasta.** `db.rs:172` a usa
como `MIGRATION_8_CHAT_MEMORY`, o toggle de memória do M6. Este é o mais
perigoso dos três achados porque **falha em silêncio**: duas entradas com o
mesmo número não quebram a compilação nem disparam teste; a segunda apenas
nunca roda, porque o `user_version` já passou dela. O resultado seria uma coluna
ausente em produção e um banco novo funcionando perfeitamente na máquina de
quem escreveu a migração.

As duas linhas ganharam, junto da correção, a instrução de conferir na fonte em
vez de confiar no texto — um número copiado para a documentação envelhece, e
foi exatamente isso que aconteceu com os três.

## O mapa, conferido por grep no código atual (não deduzido das specs)

| Todo | Desfecho | Evidência |
| --- | --- | --- |
| Verificar `connections-models` com Ollama/LM Studio rodando | **Sem objeto** (SELF-01/02, AD-042) | `OllamaClient`/`LmStudioClient`/`toggle_connection` não aparecem em nenhum arquivo fora de `CHANGELOG.md` |
| Verificar na UI o fluxo do par ativo (ativar Ollama → ativar LM Studio) | **Sem objeto** (SELF-01/07, AD-042) | não há conexão a ativar; `src/components/Connections/` não existe, só `Runtime/` |
| Confirmar que `ensure_dylib` baixa e extrai o `onnxruntime.dll` | **Sem objeto** (SELF-12) | `rag/onnxruntime.rs:19` chama `bundled::onnxruntime_dylib`; o doc-comment da função registra que o download saiu |

**Nenhum todo foi apagado.** Os três seguem na lista, riscados, nomeando a spec
que os tornou sem objeto e a evidência — mesmo critério da quick task 002, que
preservou os 37 IDs das specs revogadas com o desfecho ao lado. Apagar
economizaria três linhas e destruiria o registro de que aquela verificação
chegou a ser considerada necessária.

**Marcados como sem objeto, não como feitos.** Nenhum dos três foi verificado;
eles deixaram de ter o que verificar. A distinção é a mesma que o `AGENTS.md`
faz entre "compila" e "verificado", e escrevê-los como `[x] feito` seria
exatamente o tipo de afirmação confortável que este repositório evita.

## Verification

- [x] `grep -rn "OllamaClient\|LmStudioClient\|list_connections\|pull_model\|get_active_pair\|toggle_connection"` fora de `.specs/` retorna só `CHANGELOG.md` (histórico) — nenhum arquivo de código
- [x] `src/components/Runtime/` existe com 5 componentes; `src/components/Connections/` não existe
- [x] `rag/onnxruntime.rs` lido inteiro: nenhuma chamada de rede, só `bundled::onnxruntime_dylib`
- [x] O parágrafo novo do `AGENTS.md` não afirma nada que não esteja medido em AD-048/AD-050 ou conferido aqui
- [x] Baseline de `cargo test` do `AGENTS.md` **medido nesta task**, não copiado do `STATE.md`: **177 passando / 0 falhas / 15 ignorados**
- [x] Número da próxima migração conferido na lista `MIGRATIONS` (`db.rs:164-173`), que termina na 8 — logo a próxima é 9, não 8
- [x] Gates rodados mesmo sem código tocado: `cargo test --lib` **177/0/15**, `npm run build` limpo (1859 módulos, 5,59 s), `npm run test:scripts` **49 passando**

## Commit

pendente — `docs: retire todos the single-runtime milestone made moot`
