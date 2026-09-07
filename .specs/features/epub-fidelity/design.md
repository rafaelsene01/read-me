# Leitura fiel de EPUB — Design

**Spec:** `.specs/features/epub-fidelity/spec.md`

## O fluxo

Hoje:

```
.epub → xhtml_to_text (achata tudo) → String → paginate(chars) → original/NNNN.txt → traduz parágrafo
```

Depois:

```
.epub → extract_epub_html → Vec<Block> + Vec<Asset> + css
             │                    │            │
             │                    │            └→ styles/book.css
             │                    └→ images/NNNN.<ext>
             └→ paginate_blocks → original/NNNN.html → traduz bloco (placeholders)
                                        │
                                        └→ ReaderPanel: <iframe sandbox srcdoc>
```

O PDF continua saindo pelo caminho de hoje, sem desvio: `extract_pdf` → texto → `paginate` → `.txt`.

## Componentes

| Onde | O que muda | Requisitos |
| --- | --- | --- |
| `reader/epub.rs` | `extract_epub_html`: percorre o spine e devolve os blocos do `<body>` com a marcação preservada, mais o CSS e os assets. `xhtml_to_text` **continua existindo** para o caminho antigo e para o PDF | FID-01, FID-03 |
| `reader/blocks.rs` *(novo)* | O que é um bloco, o texto visível de um bloco, a paginação por blocos | FID-05 |
| `reader/inline.rs` *(novo)* | Texto do bloco com placeholders `⟦1⟧…⟦/1⟧`, e a remontagem na volta, com a degradação da FID-07 | FID-06, FID-07 |
| `reader/storage.rs` | `.html` além de `.txt`; `images/`, `styles/`; `read_page` com fallback | FID-09, FID-10 |
| `reader/translate.rs` | traduz o texto do bloco, não a string da página; bloco sem texto não vira requisição | FID-06, FID-08 |
| `reader_commands.rs` | `images/` e `styles/` fora da varredura de idiomas; comando que serve um asset | FID-03, FID-11 |
| `ReaderPanel.tsx` | monta o `srcdoc` e o renderiza num `<iframe sandbox>` | FID-02, FID-04 |

## Decisões

**1. `<iframe sandbox srcdoc>` sem `allow-scripts`.**
Um mecanismo resolve os três problemas que o HTML de terceiro traz: o `<script>` do EPUB não roda, o CSS do livro não vaza para a interface, e o CSS do app não deforma o livro. A alternativa — sanitizar tags e atributos à mão e injetar no DOM do app — resolveria só o primeiro, e teria que ser mantida contra cada truque novo. **O sandbox é a única defesa que não precisa de manutenção.**

**2. Imagens como `data:` URI dentro do `srcdoc`.**
Um iframe sandbox sem `allow-same-origin` tem **origem opaca**, e um `blob:` criado pelo app não é legível de lá. `data:` não depende de origem. Alternativa medida e descartada por ora: `allow-same-origin` + `blob:`, que economiza memória e afrouxa o sandbox. `ponytail:` — a inflação de ~33% do base64 é o teto conhecido, e a troca está descrita no código.

**3. A unidade de paginação passa a ser o bloco.**
Cada filho de `<body>` é atômico: `<p>`, `<h1>`, `<div class="imagem">`, `<blockquote>`. Uma página é uma lista de blocos cujo **texto visível** somado cabe em `PAGE_BUDGET_CHARS`. Isso preserva a intenção da READ-10 (página é um número inteiro de parágrafos) e torna estruturalmente impossível cortar uma tag ao meio. **Um bloco maior que o orçamento inteiro vira uma página sozinho** — cortar dentro dele exigiria entender a árvore, e a READ-10 já aceita a página que estoura em vez de partir um parágrafo.

**4. A tradução manda a frase inteira, com placeholders no lugar das tags inline.**

```
<p>Fã de suspense, descobriu <em>O Código Da Vinci</em> antes do lançamento.</p>
   → "Fã de suspense, descobriu ⟦1⟧O Código Da Vinci⟦/1⟧ antes do lançamento."
   → modelo
   → "Fan of suspense, he discovered ⟦1⟧The Da Vinci Code⟦/1⟧ before the release."
   → <p>Fan of suspense, he discovered <em>The Da Vinci Code</em> before the release.</p>
```

Traduzir cada nó de texto separadamente destruiria a concordância da frase; mandar HTML cru convidaria o modelo a fechar tag errada. O placeholder é o meio-termo que a indústria de tradução usa, e mantém a READ-22 intacta: **um pedido por bloco**.

**5. Placeholder quebrado degrada, e o número é medido.**
Se a volta não traz exatamente os mesmos marcadores, o bloco entra **sem formatação inline** — texto correto, sem `<em>`. Nunca uma tag inventada, nunca uma tag órfã. A contagem de blocos degradados é o número que a UAT usa para decidir se o modelo dá conta; se for alto, o caminho é trocar o prompt ou o modelo, não relaxar a validação.

**6. `read_page` procura `.html` e cai para `.txt`.**
O usuário já tem livros processados e **traduzidos**. Invalidá-los cobraria dele o trabalho de tradução de novo. Reprocessar promove o livro ao formato novo; até lá, ele continua legível como texto pré-formatado. Duas linhas no storage e um ramo no leitor.

**7. `xhtml_to_text` não é removida.**
Ela continua servindo o PDF e o formato antigo. Removê-la seria arrastar o pipeline de documentos para dentro desta feature, que é exatamente o que a T3 da spec anterior já tinha evitado.

**8. `images/` e `styles/` precisam ser excluídas explicitamente da varredura de idiomas.**
`wipe_languages` apaga toda subpasta do livro e `book_languages` lista toda subpasta como idioma (`reader_commands.rs:169`). Agora são **duas** pastas a excluir, não uma — e isso é código com teste, não convenção.

## Riscos

| Risco | Mitigação |
| --- | --- |
| O CSS do livro assume a página inteira do leitor (margens, colunas) e fica estranho na área do app | O iframe define a caixa. A UAT (T9) é quem olha; ajustar é CSS do contêiner, não arquitetura |
| Placeholders sobrevivendo mal em modelos pequenos | É o número que a T9 mede. A degradação já está desenhada e nunca produz HTML inválido |
| EPUB com href percent-encoded | Já é uma limitação conhecida e marcada com `ponytail:` no `join_path` de hoje. Se aparecer num livro real, decodificar ali resolve os dois caminhos de uma vez |
| `data:` URI inflando páginas com muitas fotos | Medido na T9: peso da página e tempo até pintar. O upgrade (blob + `allow-same-origin`) está escrito |
| A invariante `paginate(t).concat() == t` deixar de valer como está | **Ela muda de forma, e o teste muda junto** — de concatenação de texto para concatenação de blocos. Isso é reescrita de teste, não remoção: o teste antigo cobre o caminho `.txt`, que continua existindo |
