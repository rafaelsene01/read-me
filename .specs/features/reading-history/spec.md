# Histórico de leitura — Specification

**Milestone:** M10 — Pivô para leitor
**Status:** requisitos escritos (2026-09-04). **Bloqueador levantado em 2026-09-05** — a âncora de posição foi decidida em `.specs/features/book-reader/`, e é de lá que vêm as tasks. **Backend em pé desde 2026-09-06** (T7 da `book-reader`): as colunas, os comandos e o SQL do histórico existem e têm teste. **O frontend existe desde 2026-09-06** (T8-T11): a lateral lista leituras, não chats — mas **nenhuma tela foi vista rodando**. **HIST-09 acrescentado em 2026-09-07** (apagar uma entrada), com o backend testado e a interface apenas compilada.

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
| O que é uma "posição" | **Índice de página (base 0)** — decidido em 2026-09-05 pela `book-reader` | Ficou em aberto enquanto não havia leitor: o significado dependia de como o livro seria remontado. A `book-reader` remonta em páginas persistidas e determinísticas, então a página é ao mesmo tempo o que o usuário vê e o que o app grava. Offset de caractere criaria uma segunda fonte de verdade para a mesma coisa. |
| Onde a posição mora | `books.last_page` e `books.last_opened_at`, na migração **10** | A tabela já existe depois da `book-library`; a posição é atributo do livro, não entidade nova. O número exato da migração se confere na lista em `db.rs` na hora, nunca aqui. |
| O histórico substitui a lista de chats ou convive | Substitui | Foi o que o usuário pediu. A `chat-messaging` fica marcada como revogada pela AD-052. |
| Livro importado e nunca aberto aparece no histórico | Não | Histórico é do que foi lido; a biblioteca é que lista tudo. |

Open questions: nenhuma. A única que existia — **qual é a âncora da posição de leitura** — foi respondida em 2026-09-05 pelo design da `book-reader`: índice de página.

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

### P1: Apagar uma leitura do histórico

**User Story**: Como leitor, quero apagar uma leitura do histórico, para tirar da lateral um livro que não estou mais lendo.

**Acceptance Criteria**:

1. WHEN o usuário aciona apagar numa entrada do histórico THEN o sistema SHALL pedir confirmação, porque a posição de leitura será perdida
2. WHEN a remoção é confirmada THEN o sistema SHALL zerar `last_opened_at` **e** `last_page` daquele livro
3. WHEN a entrada é apagada THEN o livro SHALL sumir do histórico e SHALL permanecer na Biblioteca, com o arquivo importado e **todas** as pastas de tradução intactas em disco
4. WHEN um livro apagado do histórico é reaberto THEN o sistema SHALL começar da primeira página
5. WHEN uma entrada é apagada THEN as demais entradas do histórico SHALL permanecer intactas

---

## Edge Cases

- WHEN o livro é reprocessado por uma versão nova do extrator THEN a posição salva SHALL continuar apontando para o mesmo trecho do texto, ou SHALL ser invalidada explicitamente — silenciosamente deslocar o marcador é o modo de falha a evitar
- WHEN o arquivo do livro sumiu do disco THEN a entrada do histórico SHALL indicar isso em vez de falhar ao abrir

---

## Bloqueador — **levantado em 2026-09-05**

`design.md` e `tasks.md` desta feature **só podiam ser escritos depois** que o design do leitor definisse a âncora de posição. Escrever a migração antes disso seria gastar um número de migração num esquema que ia mudar.

**A âncora foi decidida em `.specs/features/book-reader/`**: a posição de leitura é o **índice de página** (base 0) na paginação persistida do livro, gravada em `books.last_page` pela migração 10. Consequência: esta feature **não ganha `design.md` nem `tasks.md` próprios** — os HIST-01..HIST-08 são implementados pelas tasks da `book-reader` (o HIST-09, acrescentado depois, foi implementado direto, sem task da `book-reader`), e a rastreabilidade abaixo aponta para lá. Isso é deliberado: dois `tasks.md` para o mesmo código seriam duas fontes de verdade.

⚠️ **Executado até aqui (2026-09-06):** a migração 10 (T2), o layout em disco e o processamento (T5/T16/T17) e os comandos de leitura e histórico (T7) — `open_book`, `save_reading_position`, `get_book_page`, `list_reading_history`, em `src-tauri/src/reader_commands.rs`, com 8 testes contra banco em memória e pasta temporária. **O frontend passou a existir** (T8-T11, T15): `readerStore`, `ReaderPanel`, `ReadingList` na lateral e a rota `reader`, com `npm run build` **exit 0**. Continua valendo o essencial: **nenhum comando Tauri rodou** (não há runner de integração neste projeto), **não há suíte de frontend** (`npm test` sai com *"No test files found"*, exit 1) e **nenhuma tela foi vista** — todo HIST-xx que depende de comportamento espera a T13.

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| HIST-01 | P1: Sidebar lista leituras, não conversas | **`book-reader` T11**: `src/components/Sidebar/ReadingList.tsx` (novo) entrou no lugar do `ChatList` na `Sidebar`, consumindo `list_reading_history` | **não verificado — só compila.** `npm run build` **exit 0**. O `ChatList.tsx` foi **apagado** (o `tsc` o derrubou ao tirar `"chat"` do `ActiveView`), então não sobra lista de conversas nem órfã. **A lateral nunca foi vista na tela** (T13) |
| HIST-02 | P1: Ordenação por abertura mais recente | `book-reader` T7 escreveu `reading_history` (`WHERE last_opened_at IS NOT NULL ORDER BY last_opened_at DESC`) | **verificado como SQL** — `the_history_lists_the_most_recently_opened_first` em **257/0/16** (2026-09-06), com timestamps escritos à mão. pending — **a lateral que consome isso é a T11** · **T11:** a `ReadingList` mostra a lista **na ordem em que o comando a devolve**, sem reordenar no cliente, e recarrega depois de abrir um livro. **Só compila** — a ordem na tela é T13 |
| HIST-03 | P1: Estado vazio apontando para a Biblioteca | **`book-reader` T11**: com zero entradas a `ReadingList` mostra `reader.historyEmpty` — *"Nenhum livro aberto ainda. Importe um na Biblioteca."* | **não verificado — só compila.** O texto existe nos dois idiomas (paridade **206/206**), mas **ninguém o viu**, e que ele apareça só quando o histórico está vazio é T13 |
| HIST-04 | P1: Registrar a última abertura | `book-reader` T7: `open_position` grava `Utc::now().to_rfc3339()` | **verificado em unidade, com ressalva no próprio teste** — `opening_a_book_records_the_moment_it_was_opened` (257/0/16): a coluna deixa de ser nula e o valor relê como RFC 3339; **que o instante seja o certo é fé no relógio do sistema** |
| HIST-05 | P1: Persistir a posição durante a leitura | `book-reader` T7: `save_position`, com o clamp no próprio SQL | **metade verificada (backend)** — `saving_a_position_persists_it` (257/0/16): gravar 2 deixa 2, gravar 999 deixa `page_count - 1`. pending — **quem grava enquanto a leitura avança é o store da T8/T9, com debounce**, e ele não existe · **T8/T9 escreveram o debounce** (`readerStore.goToPage` agenda 800 ms, `closeBook` faz *flush*) e **a T11 deu a rota** que finalmente monta quem o chama. **O debounce continua sem nunca ter disparado**: não há suíte de frontend e o app não foi aberto (T13) |
| HIST-06 | P1: Reabrir na posição salva | `book-reader` T7: `open_position` devolve `last_page` clampado | **metade verificada (backend)** — `reopening_a_book_returns_the_saved_page` e `a_position_beyond_the_page_count_is_clamped_on_open` (257/0/16). pending — **reabrir pela tela é T11/T13** · **T11:** clicar numa entrada da lateral chama `readerStore.openBook`, que pede a posição ao backend em vez de adivinhá-la, e o idioma de leitura vem de `list_book_languages` (a `ReadingEntry` não carrega essa coluna). **Só compila**; fechar o app, reabrir e cair na página certa é T13, item 6 |
| HIST-07 | P1: Livro nunca aberto começa do início | `book-reader` T7: `last_page` NULL → 0, e a abertura não grava posição | **verificado em unidade** — `a_book_never_opened_starts_at_the_first_page` (257/0/16): devolve 0 **e a coluna continua NULL**; `an_imported_book_never_opened_is_not_in_the_history` confere o outro lado, o livro importado fora do histórico |
| HIST-08 | P1: Remover o livro remove o histórico | in tasks | pending — `book-reader` T2 |
| HIST-09 | P1: Apagar uma leitura do histórico | `forget_position` + comando `forget_reading_entry` em `src-tauri/src/reader_commands.rs`; `readerApi.forgetReadingEntry`; botão de lixeira por linha na `ReadingList`, com `window.confirm` (mesmo padrão do `BookEditPanel`); chaves `reader.historyRemove` e `reader.historyRemoveConfirm` | **backend verificado por teste; a tela só compila.** `deleting_a_history_entry_forgets_the_position_and_keeps_the_book` cobre AC 2/3/5 (as duas colunas voltam a NULL, o vizinho segue no histórico com a posição dele, o `.epub` e as pastas `original/`, `pt/` e `en/` continuam em disco com o mesmo número de páginas) e `a_book_deleted_from_the_history_reopens_at_the_first_page` cobre AC 4 (7 → 0); `forgetting_a_book_that_does_not_exist_is_an_error` cobre a linha inexistente. Suíte em **269/0/17** (2026-09-07); com o `last_page = NULL` removido do UPDATE, **2 desses testes falham** — foi medido. **AC 1 e o sumiço da linha nunca foram vistos**: `npm run build` **exit 0** e nada mais, porque não há suíte de frontend (`npm test`: *"No test files found"*, exit 1) e o app não foi aberto |

**ID format:** `HIST-[NUMBER]`
