# Leitura fiel de EPUB — Specification

**Milestone:** M10.2 (extensão)
**Estende:** `.specs/features/book-reader/`
**Substitui:** `.specs/features/book-illustrations/` — planejada em 2026-09-07 e **substituída no mesmo dia, antes de qualquer linha de código** (AD-060)

## Problem Statement

O usuário mandou um print de uma página do livro e disse: *"a página do programa extraído não tem nada parecido com o arquivo, eu queria ver como se fosse o real"*.

O print mostra uma foto em preto e branco ocupando meia página, o título **O Arqueiro** centralizado, o nome do autor em versalete, títulos de obras em itálico e parágrafos justificados. **Nada disso chega à tela**: `reader/epub.rs` chama `xhtml_to_text`, que achata todas as tags e devolve texto puro, e é esse texto puro que é paginado, traduzido, gravado e exibido.

O plano do dia anterior (`book-illustrations`, AD-058) trazia **só as imagens** de volta. Confrontado com o print, ele resolveria uma fração do problema: a foto voltaria, e o versalete, o itálico, a centralização e a justificação continuariam ausentes. Melhor descobrir isso antes de escrever as sete tasks do que depois.

**A oportunidade que o formato dá:** um EPUB **já é** HTML+CSS. O app está descartando a formatação para depois tentar reconstruí-la — quando basta parar de descartá-la. É a diferença entre um leitor de EPUB e um extrator de texto.

## Goals

- A página na tela se parece com a página do livro: fonte, itálico, versalete, centralização, justificação, imagens.
- **Todas** as imagens do livro aparecem, não só as "gravuras principais".
- A tradução continua funcionando, e continua preservando a formatação de cada parágrafo.
- O que já funciona não regride: posição de leitura, dois idiomas, retradução por página, reprocessamento.

## Out of Scope

| Fora | Por quê |
| --- | --- |
| **PDF em modo fiel** | **Decidido pelo usuário em 2026-09-07:** "no momento então não precisamos dar suporte para pdf". Um PDF não tem estrutura, só posições — fidelidade ali exigiria renderizar a página como bitmap, e isso mataria a tradução |
| **Remover a leitura de PDF** | **Não foi pedido, e não será feito.** O PDF continua importável, processável e legível **como texto puro**, exatamente como hoje (READ-02, READ-08). O que ele não ganha é o modo fiel |
| Ler MOBI/AZW/AZW3 | Decidido em 2026-09-07: converter para EPUB no Calibre. READ-03 inalterado |
| Rodar JavaScript do EPUB | Um EPUB pode trazer `<script>`. Ele **não roda** — o conteúdo é de terceiro e o app é local. O isolamento é parte do design, não uma opção |
| Fontes embutidas no EPUB (`@font-face`) | Não na primeira fatia. A tipografia vem do CSS do livro com as fontes do sistema; carregar as fontes do zip é fatia própria, e o custo é medir antes |
| Paginação idêntica à do livro impresso | O número de página do original não é recuperável de um EPUB, que é refluível por natureza. As páginas continuam sendo as do app |
| Índice de capítulos navegável, busca, marcadores | Ninguém pediu. Continuam fora, como na `book-reader` |

## Assumptions & Open Questions

| Questão | Decisão | Por quê |
| --- | --- | --- |
| **Como o HTML do livro chega à tela sem contaminar o app nem rodar script?** | Um `<iframe sandbox srcdoc>` **sem `allow-scripts`** | Resolve três problemas com um mecanismo, e é o que leitores de EPUB fazem: o CSS do livro não vaza para a interface do app, o CSS do app não deforma o livro, e o `<script>` do EPUB não roda porque o sandbox não o permite. Sanitizar HTML à mão resolveria um dos três e teria que ser mantido para sempre |
| **Como as imagens entram no HTML dentro do iframe?** | Embutidas como `data:` URI na montagem do `srcdoc` | Um iframe sandbox sem `allow-same-origin` tem **origem opaca**: um `blob:` criado pelo app não é acessível lá dentro. `data:` funciona em qualquer origem e não depende de afrouxar o sandbox. **Custo:** base64 infla ~33%, e está marcado com `ponytail:` no código com o caminho de upgrade |
| **Como uma página é cortada sem quebrar tag?** | Paginação por **blocos**: cada filho de `<body>` é atômico | Preserva a invariante da READ-10 — página é um número inteiro de parágrafos — e torna impossível cortar no meio de um `<p>` ou de uma tag. O orçamento continua sendo `PAGE_BUDGET_CHARS`, contado sobre o **texto visível** do bloco, não sobre a marcação |
| **Como traduzir sem destruir `<em>` e versalete?** | Placeholders numerados no texto mandado ao modelo, remontados na volta | Mandar HTML cru a um 7B é convidá-lo a fechar tag errado. Traduzir cada nó de texto isolado destruiria a frase — *"Fã de histórias de suspense, Geraldo descobriu"* + *"O Código Da Vinci"* + *"antes mesmo de ele ser lançado"* traduzidos separadamente perdem a concordância. O placeholder mantém a frase inteira num pedido só |
| **E se o modelo devolver os placeholders quebrados?** | O parágrafo entra **sem a formatação inline**, e isso é contado | Nunca uma tag inventada, nunca um `<em>` órfão. A degradação é explícita e o número de blocos degradados é medido na UAT — não é um silêncio |
| **O que acontece com os livros já processados em `.txt`?** | `read_page` procura `.html` e **cai para `.txt`** | O usuário já tem livros processados e traduzidos. Invalidá-los custaria o trabalho de tradução dele. O leitor renderiza `.txt` como texto pré-formatado, e reprocessar o livro o promove ao formato novo |

**Open questions:** nenhuma bloqueando. Uma medição fica para a UAT: quanto o `data:` URI infla a página de um livro com muitas fotos.

---

## User Stories

### P1: A página se parece com o livro ⭐ MVP

**Como** leitor, **quero** a página com a cara do livro, **para** ler o livro e não um despejo de texto.

**Critérios de aceitação:**

1. QUANDO um EPUB é processado, ENTÃO o XHTML de cada item do spine é preservado, não achatado.
2. QUANDO uma página é exibida, ENTÃO ela usa o CSS do próprio livro: itálico, versalete, centralização, justificação e espaçamento aparecem como no original.
3. QUANDO a página tem imagem, ENTÃO **todas** as imagens dela aparecem, no lugar e no tamanho que o livro define.
4. QUANDO o EPUB traz `<script>`, ENTÃO ele **não é executado**.
5. QUANDO o CSS do livro define cor ou fonte, ENTÃO isso não vaza para a interface do app.
6. QUANDO um EPUB é processado, ENTÃO cada item do spine (capítulo ou parte pré-textual) começa numa página nova; um capítulo maior que uma página continua dividido em várias. (FID-13, AD-068)

### P1: Traduzir sem perder a formatação

**Critérios de aceitação:**

1. QUANDO um bloco com `<em>` é traduzido, ENTÃO o `<em>` cerca a expressão correspondente no texto traduzido.
2. QUANDO um bloco é traduzido, ENTÃO a requisição ao modelo contém a frase **inteira**, não pedaços dela.
3. QUANDO o modelo devolve placeholders quebrados, ENTÃO o parágrafo entra sem formatação inline e **nenhuma tag inválida** é gravada.
4. QUANDO uma página contém só imagem, ENTÃO nenhuma requisição de tradução é feita para ela.

### P1: Nada do que já funciona regride

**Critérios de aceitação:**

1. QUANDO um livro processado antes desta feature é aberto, ENTÃO ele continua legível, no formato antigo.
2. QUANDO um livro é reprocessado, ENTÃO ele passa ao formato novo e a posição de leitura continua sendo clampada como na READ-13.
3. QUANDO um PDF é processado, ENTÃO ele continua produzindo texto puro, sem erro novo.
4. QUANDO o livro tem dois idiomas, ENTÃO as imagens não são duplicadas por idioma.

---

## Requirement Traceability

| Requirement ID | Story | Onde | Status |
| --- | --- | --- | --- |
| FID-01 | P1: XHTML do spine preservado, não achatado | `reader/epub.rs` — `extract_epub_html` | implemented — unit |
| FID-02 | P1: CSS do livro aplicado à página | `reader_commands.rs` — `document` | implemented — **sem prova visual nenhuma**: a montagem é testada como string, e a única tela conferida até agora era o caminho `.txt`. Só reprocessando um EPUB (T7) |
| FID-03 | P1: Todas as imagens do livro extraídas e exibidas | `reader/epub.rs`, `reader_commands.rs` — `inline_images` | implemented — unit |
| FID-04 | P1: Renderização isolada — script não roda, CSS não vaza | `ReaderPanel.tsx` — `<iframe sandbox="">` | implemented — **sem teste**: não há suíte de frontend. Lacuna, não cobertura |
| FID-05 | P1: Paginação por blocos, sem cortar tag | `reader/html.rs` — `paginate_blocks` | implemented — unit |
| FID-06 | P1: Tradução por bloco com placeholders | `reader/html.rs`, `reader/translate.rs` | implemented — unit |
| FID-07 | P1: Placeholder quebrado degrada sem tag inválida | `reader/html.rs` — `rebuild` | implemented — unit, 4 formas de quebra |
| FID-08 | P1: Página sem texto não gera requisição | `reader/translate.rs` — `translate_blocks` | implemented — unit |
| FID-09 | P1: Livro no formato antigo (`.txt`) continua legível | `reader/storage.rs` — `PAGE_EXTENSIONS` | **verified** — unit, e visto na tela em 2026-09-07 com um livro real processado antes da feature |
| FID-10 | P1: Imagens compartilhadas entre idiomas | `reader/storage.rs` — `write_images` | implemented — unit |
| FID-11 | P1: `images/` e `styles/` não são idioma | `reader_commands.rs`, `storage::is_reserved_dir` | implemented — unit |
| FID-12 | P1: PDF continua produzindo texto puro | `reader_commands.rs` — `extract` | implemented — a suíte do caminho de PDF não mudou |
| FID-13 | P1: Cada item do spine começa numa página nova (AD-068) | `reader/epub.rs` — `EpubHtml.chapters`; `reader/html.rs` — `paginate_chapters` | implemented — units `each_spine_document_is_its_own_chapter_in_spine_order` e `every_chapter_opens_a_page_and_a_long_one_still_spans_several`. **Não medido no livro real:** exige reprocessar *A Última Carta*, o que apaga a tradução `en` (READ-13). O fallback `.txt` (EPUB que não abre estruturalmente) **não** ganhou a quebra |

**"implemented" = o código existe e o teste citado passa.** Nenhum EPUB real passou por aqui: a
T7 (UAT) **não foi executada**, e é ela que responde ao print que abriu a feature.

---

## O que esta spec muda na `book-reader`

Seguindo `.claude/rules/spec-driven-changes.md` item 4 — as duas convivem, **nada é apagado**:

- **READ-31 (um arquivo por página):** continua valendo, e o arquivo passa a ser `.html` para EPUB. A promessa de "legível e editável à mão no explorador" **melhora**: HTML é texto, e corrigir uma tradução continua sendo abrir o arquivo.
- **READ-10 (paginação determinística em fronteira de parágrafo):** continua valendo, com o bloco no lugar do parágrafo. A invariante `paginate(t).concat() == t` **muda de forma** — a concatenação passa a ser de blocos, e o teste que a afirma precisa ser reescrito, não apagado.
- **READ-22 (um parágrafo por requisição):** continua valendo. O que muda é que o parágrafo vai com placeholders no lugar das tags inline.
- **READ-08 (extração de PDF pelo caminho existente):** **inalterado**, por decisão explícita do usuário.
- **Out of Scope "Imagens e gravuras":** revogado (AD-058), agora por esta feature em vez da `book-illustrations`.
