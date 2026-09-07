# Gravuras no livro — Design

**Spec:** `.specs/features/book-illustrations/spec.md`

## O fluxo, e onde ele muda

Hoje:

```
arquivo → extract_epub_text / extract_pdf → String → paginate → write_pages(original/) → translate_book
```

Depois:

```
arquivo → extract_*_with_images → (String com marcadores, Vec<Illustration>)
                                        │              │
                                        │              └→ write_images(<livro>/images/)
                                        └→ paginate → write_pages(original/) → translate_book
                                                                                    │
                                                             marcador = parágrafo copiado literal
```

A extração passa a devolver **dois** valores. Tudo depois dela continua trabalhando com `String`, e é por isso que a paginação, o armazenamento das páginas e a posição de leitura não precisam saber que imagem existe.

## Componentes

| Onde | O que muda | Requisitos |
| --- | --- | --- |
| `reader/illustrations.rs` *(novo)* | `Illustration { name, bytes }`, o formato e o parser do marcador, o piso de dimensão, `write_images` | ILLUS-03, ILLUS-04 |
| `reader/epub.rs` | passa a capturar `<img src>` no XHTML, resolve o href contra o `.opf` e lê a entrada do zip | ILLUS-01 |
| `rag/parsing.rs` + `rag/pdfium.rs` | `extract_pdf_with_images`: percorre os objetos da página, filtra pelo piso, decodifica | ILLUS-02, ILLUS-08 |
| `reader/pagination.rs` | o marcador custa mais que os seus 22 caracteres no orçamento | ILLUS-12 |
| `reader/translate.rs` | parágrafo que é marcador é copiado, não traduzido | ILLUS-05 |
| `reader/storage.rs` | `IMAGES_DIR`, `images_dir()`, `write_images()`, `read_image()` | ILLUS-03, ILLUS-07 |
| `reader_commands.rs` | `wipe_languages` e `book_languages` passam a pular `images/`; comando `get_book_image` | ILLUS-07, ILLUS-09, ILLUS-10 |
| `src/components/Reader/ReaderPanel.tsx` | quebra o texto em blocos e renderiza `<img>` no lugar do marcador | ILLUS-06 |
| `src/lib/readerApi.ts`, `src/store/readerStore.ts` | `getBookImage`, e o object URL por imagem | ILLUS-06 |

## Decisões

**1. O marcador é um parágrafo, e é isso que o faz funcionar.**
`[[image: 0007.png]]`, sozinho numa linha, cercado por `\n\n`. `split_paragraphs` já é a única definição de parágrafo do app (READ-22), então o marcador atravessa paginação e tradução usando a máquina que já existe. Alternativa descartada: uma tabela ligando página→imagem, que obrigaria a reancorar tudo a cada repaginação — exatamente o problema que a READ-13 resolve clampando.

**2. `translate_page` copia o marcador em vez de traduzi-lo.**
Uma linha no laço: se o parágrafo casa com o marcador, ele entra no resultado sem virar requisição. Isso é o que garante a ILLUS-05 nas duas metades (chega igual **e** o modelo nunca o vê), e o teste consegue afirmar as duas coisas com o duble que já registra o que recebeu.

**3. As imagens são numeradas por ordem de aparição, não por página.**
A paginação só existe depois. Numerar por página exigiria renomear arquivo a cada reprocessamento que mudasse o corte — e o usuário abre essa pasta no explorador.

**4. `images/` é irmã das pastas de idioma, e as duas funções que varrem subpastas precisam sabê-lo.**
`wipe_languages` apaga toda subpasta; `book_languages` lista toda subpasta como idioma. As duas passam a pular `IMAGES_DIR`. **Isto é código, não convenção** — sem ele a pasta aparece como um idioma chamado "images" na tela de edição, com uma contagem de páginas absurda, e some no primeiro reprocessamento.

**5. A imagem vai à tela por comando, não por `assetProtocol`.**
Medido: o protocolo está desabilitado e não há permissão de asset no schema gerado. `get_book_image(book_id, name) -> tauri::ipc::Response` devolve bytes crus. **O `name` é validado**: só o padrão `NNNN.<ext>`, nunca um caminho — um `name` com `..` sairia da pasta do livro, e essa é a única entrada desta feature que vem do outro lado da fronteira.

**6. O piso de dimensão existe por causa do PDF.**
Um PDF tem filete, bullet e logo como objetos de imagem. Piso inicial: **64×64 px**, escrito como constante com o motivo ao lado. É um número escolhido, não medido — a T7 (UAT) é quem o confronta com livros reais, e mudá-lo é mudar uma constante.

**7. A imagem que falha é descartada, e o livro segue.**
Uma gravura ilegível não pode custar o livro inteiro ao usuário — o mesmo princípio que a READ-11 aplica à tradução. O que **não** pode acontecer em silêncio é o texto sair com um marcador apontando para um arquivo que não existe: se a imagem é descartada, o marcador dela não é emitido.

**8. O custo do marcador na paginação é um número declarado.**
`IMAGE_BUDGET_CHARS`, para uma página não juntar cinco gravuras achando que gastou 110 caracteres. `ponytail:` é uma aproximação grosseira — a alternativa (medir a altura real da imagem renderizada) exigiria saber a largura da tela dentro do backend, que é onde a paginação mora.

## Riscos

| Risco | Mitigação |
| --- | --- |
| EPUB com `<img>` apontando para fora do zip, ou com `..` no href | O href é resolvido contra o `.opf` e a leitura é feita **por nome de entrada do zip**: entrada que não existe é imagem descartada (ILLUS-08), nunca leitura de disco |
| O modelo mexer no marcador mesmo assim | Ele nunca o recebe (decisão 2). O teste afirma o conteúdo de **cada** requisição, como o `each_request_carries_exactly_one_paragraph` já faz |
| Imagem enorme travando o IPC | Uma imagem por vez, sob demanda, e nenhuma recompressão. Se travar, a medida vem antes do conserto |
| `image` como dependência nova | Já está na árvore, transitiva do `pdfium-render` (feature `image_latest` → `image_025`). Declará-la explicitamente não acrescenta binário; sem ela não há como gravar um PNG a partir do `DynamicImage` que a pdfium devolve |
