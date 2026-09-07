# Leitura fiel de EPUB — Tasks

**Spec:** `.specs/features/epub-fidelity/spec.md`
**Design:** `.specs/features/epub-fidelity/design.md`
**Status:** T1..T6 **executadas** em 2026-09-07, num lote só. **T7 quase inteira em aberto** — só a
FID-09 foi vista funcionando. Ver o roteiro no fim, item a item.

**⚠️ Leitura errada, corrigida no mesmo dia — vale como lição de UAT.** Ao abrir *A Última Carta*
e ver uma ilustração e "C A P Í T U L O  U M" na tela, isto aqui dizia que a fidelidade estava
provada. **Não estava.** Ampliando a captura: o fundo era o **tema escuro do app** e a fonte a da
interface. O iframe pinta `#fbfaf7` com serifa, sempre. Aquilo era o renderizador `.txt` com a
imagem vinda do marcador `[[image:]]` — o livro está em disco no formato antigo e o fallback da
FID-09 estava funcionando, que é outra coisa.

**O que a captura de fato provou:** a FID-09 (livro do formato antigo continua legível, com as
gravuras). **A fidelidade continua sem nenhuma prova visual**: para vê-la é preciso **reprocessar**
o livro, e reprocessar apaga as traduções — decisão do usuário, não do agente.

**O sinal que separa os dois caminhos, para a próxima vez:** fundo claro + serifa = iframe;
fundo do tema + fonte da interface = `.txt`. Uma imagem na tela **não** distingue: os dois caminhos
mostram imagem, por mecanismos diferentes.

**Defeito real que a captura achou (consertado nesta task):** o texto ocupava uma coluna estreita no
meio do painel. Eram **dois** limites, um em cada caminho — `max-w-2xl` no `ReaderPanel` e
`max-width:38rem` no `READER_CSS`. Os dois saíram: a página usa a largura da janela, e quem escolhe
a medida de leitura é o usuário redimensionando. Medida tipográfica perdida de propósito, a pedido.

---

## Test Coverage Matrix

A matriz de `.specs/codebase/TESTING.md` vale sem exceção: função pura em Rust tem teste
obrigatório; comando Tauri que só orquestra I/O **não tem runner de integração**; componente React
**não tem suíte** (`npm test` sai com *"No test files found"*, exit 1).

| Requisito | Tipo de prova | Onde |
| --- | --- | --- |
| FID-01 | unit — EPUB sintético com `<em>`, `<h1>` e imagem; os blocos saem com as tags | T2 |
| FID-02 | **nenhum automatizado** — o documento é montado em Rust e testado como string; que a página *pareça* o livro é a T7 | T5, T7 |
| FID-03 | unit — `<img>` reescrito para `NNNN.ext` e os bytes gravados | T2, T5 |
| FID-04 | **nenhum** — `sandbox=""` é uma linha de JSX. Lacuna registrada, não cobertura | T6, T7 |
| FID-05 | unit — página é lista de blocos; `split_blocks(página)` devolve os blocos | T1 |
| FID-06 | unit — a requisição leva a frase inteira com `⟦1⟧`; a volta reconstrói o `<em>` | T1, T4 |
| FID-07 | unit — quatro formas de placeholder quebrado, nenhuma gera tag | T1 |
| FID-08 | unit — bloco só com figura não vira requisição | T1, T4 |
| FID-09 | unit — `.html` ganha do `.txt`, e os dois contam como página feita | T3 |
| FID-10 | unit — `images/` e `styles/` fora de qualquer idioma | T3 |
| FID-11 | unit — nenhuma das duas aparece em `book_languages` nem some no wipe | T3, T5 |
| FID-12 | unit — o caminho do PDF continua igual (a suíte antiga não mudou) | T5 |

---

## Task Breakdown

### T1: `reader/html.rs` — bloco, texto visível, placeholders

**Files:** `src-tauri/src/reader/html.rs` *(novo)*, `src-tauri/src/reader/mod.rs`

`split_blocks`, `visible_text`, `paginate_blocks`, `placehold`/`rebuild`.

**Desvio do design, deliberado:** o design pedia **dois** arquivos (`blocks.rs` e `inline.rs`). Os
dois compartilham o mesmo scanner de tags e nada mais os usa, então virou **um**. Separá-los
duplicaria o scanner ou criaria um terceiro módulo só para ele.

**Success:** as funções puras existem, com teste.

---

### T2: `reader/epub.rs` — `extract_epub_html`

**Files:** `src-tauri/src/reader/epub.rs`

Percorre o spine, pega o `<body>` de cada documento, devolve os blocos com a marcação, o CSS
(`<link rel=stylesheet>` e `<style>`, cada folha uma vez só) e as imagens, com o `src` reescrito
para o nome gravado. `<script>` sai. A rota `container.xml → .opf → manifest+spine` foi **extraída**
para `open_package`, usada agora pelos dois extratores — era copiar e colar, ou uma cópia que
divergiria.

**Success:** o texto sai com tags, e as imagens vêm junto.

---

### T3: `reader/storage.rs` — `.html`, `styles/`, e o fallback

**Files:** `src-tauri/src/reader/storage.rs`

`PAGE_EXTENSIONS = ["html", "txt"]` (mais fiel primeiro), `page_file_ext`, `existing_page`,
`write_pages_ext`, `write_css`/`read_css`, `is_reserved_dir`. `translated_pages` passa a contar as
duas extensões — sem isso um livro traduzido no formato antigo apareceria como inteiro por traduzir.

**Success:** um livro do formato antigo continua legível e completo (FID-09).

---

### T4: `reader/translate.rs` — traduzir bloco

**Files:** `src-tauri/src/reader/translate.rs`

`translate_blocks` ao lado de `translate_page`; qual roda depende da extensão da página em
`original/`, e a tradução é gravada **na mesma extensão**.

**Success:** o `<em>` volta no lugar certo e o modelo nunca vê uma tag.

---

### T5: `reader_commands.rs` — extração, ciclo de vida e a montagem do documento

**Files:** `src-tauri/src/reader_commands.rs`

`extract` devolve páginas **já cortadas** (só ele sabe se a página é texto ou blocos), com o CSS e o
formato. `wipe_languages`/`book_languages` pulam `images/` e `styles/`. `BookPage` ganha `format`, e
`page_text` monta o documento inteiro para o iframe: CSS base + CSS do livro + a página com cada
`<img>` virando `data:` URI.

**Desvio do design, deliberado:** o design falava num "comando que serve um asset". Não existe: o
documento sai pronto do backend. Um iframe sandbox tem origem opaca, então o frontend não teria como
buscar o asset de dentro dele de qualquer forma, e assim são **zero** comandos novos.

**Fallback que o design não pedia:** se `extract_epub_html` não acha `<body>` em documento nenhum, o
EPUB cai no extrator de texto de antes. Um livro que não abre estruturalmente continua legível.

**Success:** o livro vai para o disco no formato novo e volta montado.

---

### T6: `ReaderPanel.tsx` — o iframe

**Files:** `src/types.ts`, `src/store/readerStore.ts`, `src/components/Reader/ReaderPanel.tsx`, i18n

`<iframe sandbox="" srcDoc={text}>` quando `format === "html"`; o caminho de texto continua igual.
`src/types.ts` é escrito à mão e **não tem gate** (AD-054): o campo `format` foi conferido campo a
campo contra a struct.

**Tests:** nenhum — não há suíte de frontend. **Lacuna registrada, não cobertura.**
**Gate:** `npm run build` exit 0.
**Success:** compila. **Que a página pareça o livro é a T7.**

---

### T7: UAT com um EPUB de verdade — **PARCIAL (só a FID-09)**

**Files:** nenhum, a menos que apareça defeito

1. ⬜ — **reprocessar** um EPUB real e comparar a tela com a página do arquivo: foto, título
   centralizado, versalete, itálico, justificação. Nada da fidelidade foi visto ainda; reprocessar
   apaga as traduções do livro, então quem decide é o usuário.
2. ⬜ — conferir que o texto não vaza da caixa do iframe e que o CSS do livro não mexe na interface.
3. ⬜ — traduzir uma página com itálico e **contar os blocos degradados** (FID-07). Esse número é o
   que decide se o modelo dá conta.
4. Medir o peso de uma página com foto (o `data:` URI infla ~33%) e o tempo até pintar.
5. ✅ **FID-09 provado na tela** — *A Última Carta*, processada antes desta feature, abriu inteira
   no formato antigo, com as gravuras, sem erro.
6. Processar um **PDF** e confirmar que nada mudou nele (FID-12).
7. Conferir que `images/` e `styles/` não aparecem no seletor de idioma.

**Gate:** nenhum automatizado. É a task que decide se a feature existe.

---

## Gate Check Commands

```bash
cd src-tauri && cargo test --lib
cd src-tauri && cargo check --lib
npm run build
npm run tauri dev
```
