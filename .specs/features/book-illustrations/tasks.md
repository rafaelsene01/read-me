# Gravuras no livro — Tasks

**Spec:** `.specs/features/book-illustrations/spec.md`
**Design:** `.specs/features/book-illustrations/design.md`
**Status:** T1..T6 **executadas** em 2026-09-07. **T7 (UAT com livros reais) NÃO foi executada** —
ela precisa de um EPUB e de um PDF ilustrados de verdade e do usuário na frente da tela,
e é ela que decide se a feature existe. Até lá, o que está provado é o que os testes
abaixo exercitam, contra fixtures sintéticos.

**Medido depois da execução:** `cargo test --lib` em **283 passando / 0 falhas / 18 ignorados**,
dos quais **14 testes são novos desta feature**. O baseline de 265 do `AGENTS.md` é anterior ao
trabalho de reading-history que já estava no working tree, então ele **não** foi reconferido aqui —
o número acima é o medido agora. `npm run build` exit 0. O 18º ignorado é
`rag::pdfium::tests::illustrations_of_a_real_pdf_are_extracted_and_marked_in_the_text`,
que exige a biblioteca pdfium real e um PDF ilustrado por variável de ambiente.

---

## Test Coverage Matrix

A matriz de `.specs/codebase/TESTING.md` vale aqui sem exceção: função pura em Rust é teste obrigatório; comando Tauri que só orquestra I/O **não tem runner de integração neste projeto**; componente React **não tem suíte** (`npm test` sai com *"No test files found"*, exit 1).

| Requisito | Tipo de prova | Onde |
| --- | --- | --- |
| ILLUS-01 | unit — EPUB sintético com `<img>` em dois capítulos | T2 |
| ILLUS-02 | unit do filtro (puro) + `#[ignore]` contra PDF real | T3 |
| ILLUS-03 | unit — pasta por livro, uma cópia só para dois idiomas | T1, T5 |
| ILLUS-04 | unit — formatar e reconhecer o marcador, ida e volta | T1 |
| ILLUS-05 | unit — o duble registra as requisições; nenhuma tem marcador | T4 |
| ILLUS-06 | **nenhum** — sem suíte de frontend. Lacuna, não cobertura | T6, T7 |
| ILLUS-07 | unit — `name` fora do padrão é recusado, inclusive com `..` | T5 |
| ILLUS-08 | unit — imagem quebrada some, e o marcador dela também | T2, T3 |
| ILLUS-09 | unit — reprocessar re-extrai; remover leva junto | T5 |
| ILLUS-10 | unit — `images/` não aparece em `book_languages` nem some no wipe | T5 |
| ILLUS-11 | unit — livro sem imagem produz string idêntica à de hoje | T2, T3 |
| ILLUS-12 | unit — página com marcador não empilha gravuras | T1 |

---

## Task Breakdown

### T1: `reader/illustrations.rs` — o marcador e o piso

**Depends on:** nada
**Files:** `src-tauri/src/reader/illustrations.rs` *(novo)*, `src-tauri/src/reader/mod.rs`, `src-tauri/src/reader/pagination.rs`

`Illustration { name: String, bytes: Vec<u8> }`; `marker_for(name)` e `marker_name(paragraph) -> Option<&str>`; `MIN_IMAGE_SIDE_PX = 64`; `IMAGE_BUDGET_CHARS`. A paginação passa a cobrar `IMAGE_BUDGET_CHARS` do parágrafo que é marcador.

**Tests:** ida e volta do marcador; parágrafo comum não é confundido com marcador (inclusive um que **contenha** `[[image:` no meio da prosa); página não empilha gravuras.
**Gate:** `cargo test --lib` acima do baseline.
**Success:** as funções puras existem e a paginação cobra o espaço.

---

### T2: EPUB — capturar as imagens do spine

**Depends on:** T1
**Files:** `src-tauri/src/reader/epub.rs`

`extract_epub_text` passa a devolver `(String, Vec<Illustration>)`. `<img src>` vira marcador no texto e uma entrada lida do zip; o href é resolvido contra o `.opf`, como o resto do manifest já é.

**Tests:** EPUB sintético com imagem em dois capítulos (ordem e numeração); `<img>` apontando para entrada inexistente é descartado **e o marcador não é emitido**; EPUB sem imagem devolve a string idêntica à de hoje (ILLUS-11).
**Gate:** `cargo test --lib`.
**Success:** o texto sai com marcadores e as imagens vêm junto, na ordem.

---

### T3: PDF — extrair os objetos de imagem

**Depends on:** T1
**Files:** `src-tauri/src/rag/pdfium.rs`, `src-tauri/src/rag/parsing.rs`, `src-tauri/Cargo.toml`

`extract_pdf_with_images` percorre os objetos de cada página (`PdfPageImageObject::get_raw_image`), aplica o piso e decodifica. `image = "0.25"` declarada explicitamente — já está na árvore via `pdfium-render`.

⚠️ **`rag::parsing::extract_pdf` é chamada pelo pipeline de documentos também.** A assinatura antiga continua existindo para ele: quem quer imagem chama a função nova. Mudar a assinatura da antiga arrastaria o RAG para dentro desta feature sem motivo.

**Tests:** o filtro do piso é função pura e é testado como tal; `#[ignore]` contra PDF real por `READER_PDFIUM_TEST_PDF`, o mesmo formato que a AD-057 estabeleceu.
**Gate:** `cargo test --lib`; `cargo check --lib` sem warnings.
**Success:** um PDF com figura devolve figura; um sem figura devolve o texto de sempre.

---

### T4: A tradução copia o marcador

**Depends on:** T1
**Files:** `src-tauri/src/reader/translate.rs`

No laço de `translate_page`: parágrafo que é marcador entra no resultado sem virar requisição.

**Tests:** página com 2 parágrafos e 1 marcador produz **exatamente 2** requisições, **nenhuma** contendo `[[image:`, e o marcador volta byte a byte igual na página remontada. O duble que registra o recebido já existe.
**Gate:** `cargo test --lib`.
**Success:** o modelo nunca vê um marcador.

---

### T5: Disco, ciclo de vida e o comando

**Depends on:** T2, T3
**Files:** `src-tauri/src/reader/storage.rs`, `src-tauri/src/reader_commands.rs`, `src-tauri/src/lib.rs`

`IMAGES_DIR`, `images_dir`, `write_images`, `read_image`; `process_into_pages` grava as imagens; `wipe_languages` e `book_languages` passam a pular `images/`; comando `get_book_image` devolvendo `tauri::ipc::Response`, com `name` validado contra o padrão `NNNN.<ext>`.

**Tests:** `images/` não aparece em `book_languages`; reprocessar re-extrai sem deixar órfã; remover o livro leva a pasta; dois idiomas apontam para os mesmos arquivos; `name` com `..`, com barra e fora do padrão é recusado.
**Gate:** `cargo test --lib`.
**Success:** as imagens seguem o ciclo de vida do livro, e nada as confunde com idioma.

---

### T6: O leitor mostra a gravura

**Depends on:** T5
**Files:** `src/lib/readerApi.ts`, `src/store/readerStore.ts`, `src/components/Reader/ReaderPanel.tsx`, `src/types.ts`

O texto da página é quebrado em blocos; onde há marcador, um `<img>` com object URL vindo de `getBookImage`. Os URLs criados são revogados ao trocar de página — vazar `blob:` é o defeito clássico deste padrão.

**Tests:** nenhum — não há suíte de frontend nesta árvore. **Lacuna registrada, não cobertura.**
**Gate:** `npm run build` exit 0.
**Success:** compila. **Que a gravura apareça é T7.**

---

### T7: UAT com livros ilustrados de verdade

**Depends on:** T6
**Files:** nenhum (a menos que apareça defeito — e o conserto é **desta** task)

1. Processar um **EPUB ilustrado real** e conferir as gravuras na tela, no lugar certo.
2. Processar um **PDF ilustrado real**. Conferir o piso de 64 px: se entrou lixo (filete, logo) ou se sumiu gravura legítima, **o número está errado e ajustá-lo é parte desta task**.
3. Abrir `<livro>/images/` no explorador e conferir a numeração na ordem de leitura.
4. Traduzir o livro e confirmar que as gravuras continuam nos mesmos lugares.
5. Traduzir para um **segundo idioma** e confirmar que `images/` não foi duplicada.
6. Abrir o painel de edição e confirmar que **`images/` não aparece como idioma**.
7. Reprocessar e confirmar que as imagens voltam, sem sobra da rodada anterior.
8. Medir: quanto o livro ilustrado pesa em disco e quanto tempo a extração levou.

**Tests:** nenhum automatizado — é UAT manual, com arquivos reais. É o único tipo de prova que existe para "a gravura aparece no lugar certo", e o `AGENTS.md` conta isso como evidência quando o roteiro executado é escrito.
**Gate:** nenhum automatizado. É a task que decide se a feature existe.
**Success:** um livro ilustrado é lido, com gravuras, do começo ao fim.

---

## Execution Plan

| Fase | Tasks | Por quê juntas |
| --- | --- | --- |
| 1 — fundação | T1 | Todo o resto depende do marcador e do piso |
| 2 — extratores | T2, T3, T4 | Só dependem da T1 e **não se tocam**: EPUB, PDF e tradução são arquivos diferentes |
| 3 — disco e comando | T5 | Precisa das duas extrações existindo |
| 4 — tela | T6 | Precisa do comando |
| 5 — prova | T7 | Precisa de tudo, e do usuário |

Ordem: T1 → (T2 ‖ T3 ‖ T4) → T5 → T6 → T7. São 7 tasks, um único lote — executadas em linha, sem subagents.

## Gate Check Commands

```bash
# Backend: a suíte inteira, o gate padrão deste repositório
cd src-tauri && cargo test --lib

# Backend: warnings contam como falha nesta base
cd src-tauri && cargo check --lib

# Frontend (T6): tsc + Vite
npm run build

# T3, contra um PDF ilustrado de verdade (o formato da AD-057)
cd src-tauri && READER_PDFIUM_LIBRARY="<repo>/src-tauri/resources/pdfium/bin/pdfium.dll"   READER_PDFIUM_TEST_PDF="<livro>.pdf" cargo test --lib --ignored illustrations

# T7: o app de verdade
npm run tauri dev
```

**Baseline a manter:** `cargo test --lib` em **265 passando / 0 falhas / 17 ignorados** (2026-09-06). Teste que sumir precisa de justificativa escrita.
