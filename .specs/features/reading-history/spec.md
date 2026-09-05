# Histórico de leitura — Specification

**Milestone:** M10 — Pivô para leitor
**Status:** requisitos escritos (2026-09-04). **Sem `design.md` e sem `tasks.md`, de propósito** — ver "Bloqueador" abaixo. Nada implementado.

## Problem Statement

O usuário pediu: *"parte que temos ali de chat vai ser o histórico de leituras, deve ficar marcado onde parou"*. A lista de chats na sidebar deixa de listar conversas e passa a listar leituras, com a posição em que cada livro foi abandonado, para o usuário retomar de onde parou.

O requisito é claro. O que não existe é **quem escreve a posição**: nenhum código do projeto abre um livro para leitura hoje. Uma coluna de posição criada agora nasceria sempre nula, e o teste que a cobrisse só provaria que zero continua zero. Pior: o significado de "posição" depende de como o leitor remonta o livro — offset de caractere no texto extraído, índice de parágrafo, âncora no HTML remontado ou timestamp do TTS são coisas diferentes, e escolher errado agora obriga a uma segunda migração para corrigir o que a primeira chutou.

Por isso os requisitos ficam registrados aqui, com IDs rastreáveis, e as tasks saem junto com o leitor.

## Goals

- [ ] A sidebar mostra o histórico de leituras no lugar da lista de chats
- [ ] Cada livro aberto guarda onde a leitura parou
- [ ] Reabrir um livro retoma daquele ponto

## Out of Scope

| Feature | Reason |
| --- | --- |
| O leitor em si (extração, remontagem, renderização) | Feature própria do M10.2; este histórico é o consumidor dela, não o produtor |
| A marcação karaokê durante a leitura em voz alta | M10.3 — é a posição **da fala**, efêmera, não a posição salva |
| Importar, guardar e listar livros | `.specs/features/book-library/spec.md` |
| Remover o código de chat | Revogação registrada na AD-052; a remoção física é trabalho próprio |

---

## Assumptions & Open Questions

| Assunção | Escolha adotada | Racional |
| --- | --- | --- |
| O que é uma "posição" | **Não decidido — é o bloqueador desta spec** | Depende do formato em que o leitor remonta o livro. Decidir antes seria chutar o esquema. |
| Onde a posição mora | Coluna(s) numa migração ≥ 10, na tabela `books` | A tabela já existe depois da `book-library`; a posição é atributo do livro, não entidade nova. O número exato da migração se confere na lista em `db.rs` na hora, nunca aqui. |
| O histórico substitui a lista de chats ou convive | Substitui | Foi o que o usuário pediu. A `chat-messaging` fica marcada como revogada pela AD-052. |
| Livro importado e nunca aberto aparece no histórico | Não | Histórico é do que foi lido; a biblioteca é que lista tudo. |

Open questions: uma, e ela bloqueia as tasks — **qual é a âncora da posição de leitura**. Só o design do leitor responde.

---

## User Stories

### P1: Ver o que estou lendo

**User Story**: Como leitor, quero que a lateral do app mostre minhas leituras em vez de conversas, para voltar rápido ao que eu estava lendo.

**Acceptance Criteria**:

1. WHEN a sidebar é renderizada THEN o sistema SHALL listar as leituras, e SHALL NOT listar conversas de chat
2. WHEN há mais de uma leitura THEN o sistema SHALL ordená-las da mais recentemente aberta para a mais antiga
3. IF nenhum livro foi aberto ainda WHEN a sidebar é renderizada THEN o sistema SHALL mostrar um estado vazio que aponta para a Biblioteca

---

### P1: Retomar de onde parei

**User Story**: Como leitor, quero reabrir um livro no ponto onde parei, para não procurar a página toda vez.

**Acceptance Criteria**:

1. WHEN um livro é aberto para leitura THEN o sistema SHALL registrar ou atualizar o instante da última abertura
2. WHILE a leitura avança THEN o sistema SHALL persistir a posição corrente
3. WHEN o usuário reabre um livro que já tem posição salva THEN o sistema SHALL abrir naquela posição
4. IF o livro nunca foi aberto WHEN o usuário o abre THEN o sistema SHALL começar do início
5. WHEN um livro é removido da Biblioteca THEN o sistema SHALL remover também a entrada dele no histórico

---

## Edge Cases

- WHEN o livro é reprocessado por uma versão nova do extrator THEN a posição salva SHALL continuar apontando para o mesmo trecho do texto, ou SHALL ser invalidada explicitamente — silenciosamente deslocar o marcador é o modo de falha a evitar
- WHEN o arquivo do livro sumiu do disco THEN a entrada do histórico SHALL indicar isso em vez de falhar ao abrir

---

## Bloqueador

`design.md` e `tasks.md` desta feature **só podem ser escritos depois** que o design do leitor definir a âncora de posição. Escrever a migração antes disso é gastar um número de migração num esquema que vai mudar.

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| HIST-01 | P1: Sidebar lista leituras, não conversas | pending | pending |
| HIST-02 | P1: Ordenação por abertura mais recente | pending | pending |
| HIST-03 | P1: Estado vazio apontando para a Biblioteca | pending | pending |
| HIST-04 | P1: Registrar a última abertura | pending | pending |
| HIST-05 | P1: Persistir a posição durante a leitura | pending | pending |
| HIST-06 | P1: Reabrir na posição salva | pending | pending |
| HIST-07 | P1: Livro nunca aberto começa do início | pending | pending |
| HIST-08 | P1: Remover o livro remove o histórico | pending | pending |

**ID format:** `HIST-[NUMBER]`
