# Gravuras no livro — Specification

**Milestone:** M10.2 (extensão)
**Estende:** `.specs/features/book-reader/` — o leitor, a paginação, a tradução e o layout em disco já existem; esta feature acrescenta as imagens a eles.

## Problem Statement

O leitor extrai **só texto**. Um livro ilustrado — e o `ROADMAP.md` do M10.2 pedia explicitamente "preservando as gravuras" — chega ao leitor sem uma única imagem: `reader/epub.rs` não olha `<img>`, e do PDF sai apenas a camada de texto. A spec da `book-reader` registrou isso no Out of Scope como **suposição a vetar**, porque o pedido literal do usuário dizia "extrair o texto" e as duas fontes divergiam.

**O usuário vetou em 2026-09-07**, escolhendo, entre três desenhos apresentados, o do meio: **texto + gravuras**, com o texto continuando a refluir e a ser traduzido. Fidelidade de página (renderizar a página como bitmap) foi **rejeitada explicitamente** porque mataria a tradução.

## Goals

- A gravura aparece na leitura, entre os parágrafos onde ela estava no original.
- A tradução continua funcionando exatamente como hoje, e o modelo **nunca vê** uma imagem nem o marcador dela.
- As imagens ficam legíveis no explorador de arquivos, como as páginas já ficam (mesma filosofia da READ-31).
- Um livro sem imagem nenhuma se comporta hoje como se comportava ontem.

## Out of Scope

| Fora | Por quê |
| --- | --- |
| Renderizar a página como imagem (fac-símile) | **Rejeitado pelo usuário em 2026-09-07**, com o trade-off na mesa: mataria a tradução, a busca e o ajuste de tamanho. `pdfium-render` tem `page.render_with_config()` e continua disponível se a decisão mudar |
| Layout exato: colunas, posição milimétrica, fonte original | Mesma decisão. O texto reflui; a imagem entra no fluxo, não numa coordenada |
| Ler MOBI/AZW/AZW3 | **Decidido pelo usuário em 2026-09-07:** converter para EPUB no Calibre, fora do app. AZW3 é HTML+CSS+imagens em PalmDB — a mesma matéria-prima do EPUB, que o app já lê — então um parser PalmDB em Rust custaria caro e **não melhoraria o conteúdo**. READ-03 fica como está |
| Legendas ligadas à figura | A legenda é um parágrafo de texto e continua sendo tratada como texto. Amarrá-la à imagem exigiria heurística que ninguém pediu |
| Zoom, girar, abrir a imagem em tela cheia | Ninguém pediu |
| OCR de PDF escaneado | Um PDF sem camada de texto continua falhando com `NoTextFound`. Isso não muda aqui |
| Recomprimir ou redimensionar a imagem | A imagem vai para o disco como o livro a traz. Otimizar é medir primeiro |

## Assumptions & Open Questions

| Questão | Decisão | Por quê |
| --- | --- | --- |
| **Como a imagem chega à tela?** | Comando Tauri devolvendo os bytes (`tauri::ipc::Response`), como os outros 11 comandos | **Medido, não presumido:** `app.security.assetProtocol` **não está habilitado** em `tauri.conf.json`, e `grep asset` no `gen/schemas/desktop-schema.json` dá **zero ocorrências** — o caminho do `convertFileSrc` exigiria habilitar o protocolo, mexer em capabilities e liberar em runtime um diretório que fica **fora do repositório** (a pasta-base do usuário). O comando não exige config nova, permissão nova, nem escopo de arquivo novo. `tauri::ipc::Response::new` existe no tauri 2.11.5 (`src/ipc/mod.rs:200`) e passa bytes crus, sem base64 |
| **Onde as imagens moram?** | `<livro>/images/`, uma pasta por livro, **compartilhada por todos os idiomas** | A gravura não muda quando o texto é traduzido. Uma cópia por idioma multiplicaria o disco por nada |
| **Como o texto diz onde a imagem entra?** | Uma linha própria: `[[image: 0007.png]]` | Precisa ser um **parágrafo** (`split_paragraphs` divide por `\n\n`), sobreviver ao pipeline inteiro, e ser legível para quem abrir o `.txt` no explorador e editar à mão — que é uma feature declarada da READ-31, não um acidente |
| **O modelo traduz o marcador?** | **Nunca.** O parágrafo que é marcador é copiado literal | Mandar `[[image: 0007.png]]` a um modelo 7B é convidá-lo a "traduzir" o nome do arquivo. Também economiza uma requisição por imagem |
| **Como as imagens são numeradas?** | Ordem de aparição no documento, base 1: `0001.png`, `0002.png`… | **Não pode ser por página:** as imagens são extraídas antes de a paginação existir — é o texto com marcadores que é paginado depois |
| **Imagem minúscula conta como gravura?** | Não. Abaixo de um piso de dimensão, é descartada | Um PDF tem dezenas de objetos de imagem que são filete, bullet e logo de rodapé. Sem piso, o livro fica salpicado de lixo de 3×3 px |
| **A pasta `images/` vira um "idioma"?** | Não, e isso exige código | **Achado lendo o código, não suposto:** `wipe_languages` (`reader_commands.rs:169`) apaga **toda subpasta** do livro, e `book_languages` lista subpasta como idioma. Sem tratar a exceção, `images/` apareceria na tela como um idioma e sumiria no reprocessamento |

**Open questions:** nenhuma bloqueando.

---

## User Stories

### P1: Ver a gravura enquanto lê ⭐ MVP

**Como** leitor de um livro ilustrado, **quero** ver as gravuras no meio do texto, **para** não perder metade do livro.

**Critérios de aceitação:**

1. QUANDO um EPUB com imagens é processado, ENTÃO cada imagem referenciada por um `<img>` do spine é gravada em `<livro>/images/`, na ordem de aparição.
2. QUANDO um PDF com imagens é processado, ENTÃO cada objeto de imagem da página que passe do piso de dimensão é gravado da mesma forma.
3. QUANDO uma página com gravura é aberta no leitor, ENTÃO a imagem aparece entre os parágrafos que a cercavam no original.
4. QUANDO o livro não tem imagem nenhuma, ENTÃO nada muda: nenhuma pasta `images/` é criada e o texto é idêntico ao que a versão anterior produzia.
5. QUANDO uma imagem não pode ser extraída ou decodificada, ENTÃO o livro é processado assim mesmo, sem aquela imagem, e o erro **não** derruba a extração.

### P1: A tradução não estraga a gravura

**Como** leitor de um livro traduzido, **quero** as gravuras nos mesmos lugares na tradução, **para** que traduzir não custe o conteúdo visual.

**Critérios de aceitação:**

1. QUANDO uma página com marcador é traduzida, ENTÃO o marcador chega à página traduzida **byte a byte igual**.
2. QUANDO uma página é traduzida, ENTÃO nenhuma requisição ao modelo contém um marcador.
3. QUANDO o livro é traduzido para dois idiomas, ENTÃO as duas traduções apontam para os **mesmos arquivos** de `images/` — nenhuma cópia é feita por idioma.

### P1: As gravuras seguem o ciclo de vida do livro

**Critérios de aceitação:**

1. QUANDO o livro é reprocessado, ENTÃO as imagens são re-extraídas e as antigas não sobram.
2. QUANDO o livro é removido, ENTÃO a pasta `images/` vai junto.
3. QUANDO os idiomas do livro são listados, ENTÃO `images/` **não** aparece como idioma.

---

## Requirement Traceability

| Requirement ID | Story | Onde | Status |
| --- | --- | --- | --- |
| ILLUS-01 | P1: Imagens do EPUB extraídas em ordem de aparição | `reader/epub.rs` — `extract_epub` | implemented — unit |
| ILLUS-02 | P1: Imagens do PDF extraídas, com piso de dimensão | `rag/pdfium.rs` — `extract_text_and_images` | implemented — unit do piso; PDF real só no `#[ignore]` |
| ILLUS-03 | P1: Uma pasta `images/` por livro, compartilhada pelos idiomas | `reader/storage.rs` — `write_images` | implemented — unit |
| ILLUS-04 | P1: Marcador `[[image: NNNN.ext]]` como parágrafo próprio no texto | `reader/illustrations.rs` | implemented — unit |
| ILLUS-05 | P1: O marcador nunca é enviado ao modelo, e chega igual à tradução | `reader/translate.rs` — `translate_page` | implemented — unit |
| ILLUS-06 | P1: O leitor renderiza a imagem no lugar do marcador | `components/Reader/ReaderPanel.tsx` | implemented — **sem teste**: não há suíte de frontend. Compila; que apareça é a T7 |
| ILLUS-07 | P1: Comando que serve os bytes da imagem, com o caminho confinado ao livro | `reader_commands.rs` — `get_book_image` | implemented — unit do guarda de nome; o comando em si não é exercitado. ⚠️ **Defeito relatado e corrigido em 2026-09-12 (AD-077):** o guarda exigia exatamente 4 dígitos e recusava a imagem 10.000 (`not an illustration name: "10000.png"`), derrubando o processamento de um EPUB com 26.787 imagens. Agora aceita 4 a 6 dígitos; units `the_ten_thousandth_illustration_is_still_an_illustration` e `a_book_with_more_than_9999_illustrations_writes_and_reads_them` (343/0/22). **O livro real não foi reprocessado** |
| ILLUS-08 | P1: Imagem ilegível degrada para texto, sem derrubar o processamento | `reader/epub.rs`, `rag/pdfium.rs` | implemented — unit no EPUB; no PDF só por leitura |
| ILLUS-09 | P1: Reprocessar re-extrai; remover o livro leva as imagens junto | `reader/storage.rs`, `reader_commands.rs` | implemented — unit |
| ILLUS-10 | P1: `images/` não é listada como idioma nem apagada como idioma | `reader_commands.rs` — `wipe_languages`, `book_languages` | implemented — unit |
| ILLUS-11 | P1: Livro sem imagem produz exatamente o texto de antes | `reader/epub.rs`, `reader/storage.rs` | implemented — unit no EPUB |
| ILLUS-12 | P1: A paginação leva em conta o espaço que a gravura ocupa | `reader/pagination.rs` — `budget_end` | implemented — unit |

**"implemented" aqui significa: o código existe e o teste citado passa.** Que a gravura apareça
na tela de um livro de verdade é a T7 (UAT), que **não foi executada**.

---
| ILLUS-01 | P1: Imagens do EPUB extraídas em ordem de aparição | in tasks | pending |
| ILLUS-02 | P1: Imagens do PDF extraídas, com piso de dimensão | in tasks | pending |
| ILLUS-03 | P1: Uma pasta `images/` por livro, compartilhada pelos idiomas | in tasks | pending |
| ILLUS-04 | P1: Marcador `[[image: NNNN.ext]]` como parágrafo próprio no texto | in tasks | pending |
| ILLUS-05 | P1: O marcador nunca é enviado ao modelo, e chega igual à tradução | in tasks | pending |
| ILLUS-06 | P1: O leitor renderiza a imagem no lugar do marcador | in tasks | pending |
| ILLUS-07 | P1: Comando que serve os bytes da imagem, com o caminho confinado ao livro | in tasks | pending |
| ILLUS-08 | P1: Imagem ilegível degrada para texto, sem derrubar o processamento | in tasks | pending |
| ILLUS-09 | P1: Reprocessar re-extrai; remover o livro leva as imagens junto | in tasks | pending |
| ILLUS-10 | P1: `images/` não é listada como idioma nem apagada como idioma | in tasks | pending |
| ILLUS-11 | P1: Livro sem imagem produz exatamente o texto de antes | in tasks | pending |
| ILLUS-12 | P1: A paginação leva em conta o espaço que a gravura ocupa | in tasks | pending |

---

## O que esta spec muda na `book-reader`

Seguindo `.claude/rules/spec-driven-changes.md` item 4 — **as duas specs convivem**, nenhuma é revogada:

- **Out of Scope, linha "Imagens e gravuras no texto remontado":** revogada por esta feature, por decisão do usuário em 2026-09-07. A linha fica na `book-reader` marcada como revogada, com o apontador para cá — não é apagada.
- **READ-31 (texto em arquivos, um por página):** continua valendo. Ganha uma vizinha `images/` na pasta do livro, e o `.txt` passa a poder conter linhas de marcador.
- **READ-10 (paginação determinística):** continua valendo, e o marcador é um parágrafo como qualquer outro — a fronteira de parágrafo não muda. O que muda é o **custo** que ele ocupa no orçamento da página (ILLUS-12).
- **READ-22 (um parágrafo por requisição):** continua valendo. O marcador é a única exceção, e ela é **para menos**: um parágrafo que não vira requisição nenhuma.
- **READ-03 (MOBI/AZW/AZW3 sem leitura):** **inalterado**, agora por decisão explícita do usuário, e não só por falta de crate.
