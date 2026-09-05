# Summary — Quick Task 006

**Date:** 2026-07-27
**Status:** Done, no code touched

## O que foi feito

Saneamento de documentação em dois arquivos. Nenhuma linha de código mudou —
o que mudou foi a descrição do código.

**`STATE.md`** — três todos passaram a **sem objeto**, riscados e com a
evidência ao lado, nunca apagados: verificar `connections-models` contra um
Ollama/LM Studio real, verificar na UI o fluxo do par ativo, e confirmar que o
`ensure_dylib` baixa o `onnxruntime.dll`. As três coisas foram removidas pelo
M9. Cada desfecho nomeia a spec (SELF-01/02/07/12) e a AD (AD-039/AD-042).

**`AGENTS.md`** — três correções, uma pedida e duas achadas:

| Linha | Dizia | Diz |
| --- | --- | --- |
| Estado atual | "o M9 está no meio; o frontend ainda chama `list_connections`" | nenhuma task de código aberta nas 11 features; o que resta é verificação, separada por quem consegue fazer |
| Baseline de testes | 146 passando | **177 / 0 falhas / 15 ignorados**, medido |
| Migração | "a próxima é a 8" | **a próxima é a 9** — a 8 é `MIGRATION_8_CHAT_MEMORY` |

## Por que isso não era cosmético

Os três eram armadilhas ativas, não desatualização inofensiva:

- O **parágrafo de estado** é o primeiro que qualquer agente lê, por instrução
  do próprio arquivo. Ele mandava consertar um desalinhamento consertado há
  oito ADs — trabalho inventado sobre código que não existe.
- O **baseline de 146** é usado como gate ("se o número cair, justifique").
  31 testes atrasado, ele daria por normal a perda de 30.
- A **migração 8** é a pior: numerar a próxima como 8 **não quebra nada
  visivelmente**. A entrada duplicada simplesmente nunca roda, porque o
  `user_version` já passou dela. O banco do desenvolvedor, criado do zero,
  ficaria correto; o do usuário, que migra, ficaria sem a coluna.

## Verificado

Cada afirmação removida foi conferida **por grep no código**, não deduzida das
specs:

- `OllamaClient`, `LmStudioClient`, `toggle_connection`, `list_connections`,
  `pull_model`, `get_active_pair` — zero ocorrências em código; só no
  `CHANGELOG.md`
- `src/components/Connections/` não existe; `src/components/Runtime/` tem 5
  componentes
- `rag/onnxruntime.rs` lido inteiro: nenhuma rede, só `bundled::onnxruntime_dylib`
- `db.rs:164-173`: a lista `MIGRATIONS` termina em `(8, MIGRATION_8_CHAT_MEMORY)`

Gates, mesmo sem código tocado: `cargo test --lib` **177 passando / 0 falhas /
15 ignorados**; `npm run build` limpo; `npm run test:scripts` **49 passando**.

## O que esta task NÃO fez

- **Não varreu os outros documentos** em busca do mesmo tipo de defeito. O
  `PROJECT.md`, o `ROADMAP.md` e os sete arquivos de `codebase/` podem ter
  números envelhecidos pela mesma razão; só o `AGENTS.md` foi lido linha a
  linha aqui.
- **Não fechou nenhuma pendência real.** Os todos que saíram da lista saíram
  por terem perdido o objeto, não por terem sido verificados. A contagem de
  pendências caiu de 18 para 15, e as 15 continuam valendo.
- **Não commitou** — working tree, como manda o `AGENTS.md`.

## Commit

pendente — `docs: retire todos the single-runtime milestone made moot`
